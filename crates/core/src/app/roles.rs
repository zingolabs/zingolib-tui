use super::state::{OutputRole, OutputRow};
use zingolib::wallet::summary::data::{SendType, TransactionKind};

pub fn assign_roles(
    kind: TransactionKind,
    mut received: Vec<OutputRow>,
    outgoing: Vec<OutputRow>,
) -> Vec<OutputRow> {
    let own = |o: &OutputRow| {
        received
            .iter()
            .any(|r| r.pool == o.pool && r.index == o.index)
    };
    let mut sent: Vec<OutputRow> = outgoing.into_iter().filter(|o| !own(o)).collect();
    sent.iter_mut().for_each(|o| o.role = OutputRole::Sent);

    let external = |o: &OutputRow| {
        o.scope
            .as_deref()
            .is_none_or(|s| s.eq_ignore_ascii_case("external"))
    };
    let any_external = received.iter().any(external);
    for o in &mut received {
        o.role = match kind {
            TransactionKind::Received => OutputRole::Received,
            TransactionKind::Sent(SendType::Shield) => OutputRole::ToSelf,
            TransactionKind::Sent(SendType::SendToSelf)
                if external(o) || !any_external =>
            {
                OutputRole::ToSelf
            }
            TransactionKind::Sent(SendType::Send) if external(o) => OutputRole::ToSelf,
            TransactionKind::Sent(_) => OutputRole::Change,
        };
    }
    sent.extend(received);
    sent
}

#[cfg(test)]
mod tests {
    use super::*;
    use zcash_protocol::PoolType;
    use zingolib::wallet::output::SpendStatus;

    fn output(pool: PoolType, value: u64, index: u32, scope: &str) -> OutputRow {
        OutputRow {
            pool,
            role: OutputRole::Received,
            value,
            index,
            address: None,
            memo: None,
            spend: SpendStatus::Unspent,
            scope: Some(scope.to_string()),
        }
    }

    fn roles(outputs: &[OutputRow]) -> Vec<(PoolType, u64, OutputRole)> {
        outputs.iter().map(|o| (o.pool, o.value, o.role)).collect()
    }

    #[test]
    fn a_payment_keeps_its_recipient_and_calls_the_rest_change() {
        let received = vec![output(PoolType::ORCHARD, 150_000, 1, "internal")];
        let outgoing = vec![output(PoolType::SAPLING, 10_000, 0, "external")];
        let out = assign_roles(TransactionKind::Sent(SendType::Send), received, outgoing);
        assert_eq!(
            roles(&out),
            vec![
                (PoolType::SAPLING, 10_000, OutputRole::Sent),
                (PoolType::ORCHARD, 150_000, OutputRole::Change),
            ]
        );
    }

    #[test]
    fn a_payment_to_one_of_the_wallets_own_addresses_is_to_self() {
        let received = vec![
            output(PoolType::TRANSPARENT, 17_000, 2, "external"),
            output(PoolType::ORCHARD, 150_000, 1, "internal"),
        ];
        let outgoing = vec![
            output(PoolType::TRANSPARENT, 17_000, 2, "external"),
            output(PoolType::SAPLING, 23_000, 0, "external"),
        ];
        let out = assign_roles(TransactionKind::Sent(SendType::Send), received, outgoing);
        assert_eq!(
            roles(&out),
            vec![
                (PoolType::SAPLING, 23_000, OutputRole::Sent),
                (PoolType::TRANSPARENT, 17_000, OutputRole::ToSelf),
                (PoolType::ORCHARD, 150_000, OutputRole::Change),
            ]
        );
    }

    #[test]
    fn a_same_pool_send_to_self_splits_purpose_from_change() {
        let received = vec![
            output(PoolType::ORCHARD, 40_000, 0, "internal"),
            output(PoolType::ORCHARD, 0, 1, "external"),
        ];
        let out = assign_roles(
            TransactionKind::Sent(SendType::SendToSelf),
            received,
            vec![],
        );
        assert_eq!(
            roles(&out),
            vec![
                (PoolType::ORCHARD, 40_000, OutputRole::Change),
                (PoolType::ORCHARD, 0, OutputRole::ToSelf),
            ]
        );
    }

    #[test]
    fn consolidating_into_change_is_all_to_self() {
        let received = vec![output(PoolType::SAPLING, 90_000, 0, "internal")];
        let out = assign_roles(
            TransactionKind::Sent(SendType::SendToSelf),
            received,
            vec![],
        );
        assert_eq!(
            roles(&out),
            vec![(PoolType::SAPLING, 90_000, OutputRole::ToSelf)]
        );
    }

    #[test]
    fn everything_in_a_received_transaction_is_received() {
        let received = vec![output(PoolType::ORCHARD, 5, 0, "external")];
        let out = assign_roles(TransactionKind::Received, received, vec![]);
        assert_eq!(
            roles(&out),
            vec![(PoolType::ORCHARD, 5, OutputRole::Received)]
        );
    }

    #[test]
    fn a_shield_comes_back_on_purpose() {
        let received = vec![output(PoolType::ORCHARD, 1_000_000, 0, "internal")];
        let out = assign_roles(
            TransactionKind::Sent(SendType::Shield),
            received,
            vec![],
        );
        assert_eq!(
            roles(&out),
            vec![(PoolType::ORCHARD, 1_000_000, OutputRole::ToSelf)]
        );
    }
}

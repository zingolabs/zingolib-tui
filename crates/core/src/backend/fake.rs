use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::mpsc;

use pepper_sync::sync::{ScanPriority, ScanRange};
use pepper_sync::wallet::SyncMode;
use zcash_protocol::PoolType;
use zcash_protocol::TxId;
use zcash_protocol::consensus::BlockHeight;
use zingolib::wallet::output::SpendStatus;
use zingolib::wallet::summary::data::{SendType, TransactionKind};
use zingolib::perspective::value_transfer::{SelfSendValueTransfer, SentValueTransfer, ValueTransferKind};
use zingo_status::confirmation_status::ConfirmationStatus;

use crate::app::{
    Action, AddressKind, AddressRow, Balance, Command, InputRow, KeyMaterial, OpenSpec,
    OutputRole, OutputRow, PoolBalance, SyncNote, Tip,
    TxRow, WalletData, WalletEntry, WalletEventRow, WalletInfo, WalletKind,
};

const TICK: Duration = Duration::from_millis(500);
const OUTPUTS: u64 = 12_000;
const PACE: u64 = 180;
const RESYNC: Duration = Duration::from_secs(15);
const SEED_PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";

pub fn spawn(
    commands: mpsc::UnboundedReceiver<Command>,
    actions: mpsc::UnboundedSender<Action>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(Fake::new(actions).run(commands))
}

type Story = (u64, u64, TransactionKind, u64, Option<u64>, &'static str);

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }

    fn hex(&mut self, len: usize) -> String {
        let mut s = String::with_capacity(len);
        while s.len() < len {
            s.push_str(&format!("{:016x}", self.next()));
        }
        s.truncate(len);
        s
    }

    fn txid(&mut self) -> TxId {
        TxId::from_hex(&self.hex(64)).expect("64 hex digits")
    }

    fn bech(&mut self, len: usize) -> String {
        const CHARS: &[u8] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
        (0..len)
            .map(|_| CHARS[self.below(32) as usize] as char)
            .collect()
    }
}

struct Fake {
    actions: mpsc::UnboundedSender<Action>,
    rng: Rng,
    tip: u32,
    birthday: u32,
    name: String,
    kind: WalletKind,
    data: WalletData,
    opened: bool,
    proposal: Option<(String, u64, Option<String>, u64)>,
    mode: SyncMode,
    auto: bool,
    scanned: u64,
    sessions: u64,
    next_launch: Option<Instant>,
    ticks: u64,
}

impl Fake {
    fn new(actions: mpsc::UnboundedSender<Action>) -> Self {
        Self {
            actions,
            rng: Rng(0x5eed_1234_abcd_0001),
            tip: 3_112_480,
            birthday: 2_618_000,
            name: String::new(),
            kind: WalletKind::Seed,
            data: WalletData::default(),
            opened: false,
            proposal: None,
            mode: SyncMode::NotRunning,
            auto: true,
            scanned: 0,
            sessions: 0,
            next_launch: None,
            ticks: 0,
        }
    }

    fn send(&self, action: Action) {
        let _ = self.actions.send(action);
    }

    fn note(&self, note: SyncNote) {
        self.send(Action::Sync(note, Instant::now()));
    }

    async fn run(mut self, mut commands: mpsc::UnboundedReceiver<Command>) {
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                cmd = commands.recv() => match cmd {
                    None => break,
                    Some(cmd) => if self.handle(cmd).await { break; },
                },
                _ = tick.tick() => self.on_tick(),
            }
        }
    }

    fn now_unix() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(1_800_000_000)
    }

    async fn handle(&mut self, cmd: Command) -> bool {
        match cmd {
            Command::ListWallets => {
                tokio::time::sleep(Duration::from_millis(150)).await;
                self.send(Action::WalletsListed(vec![
                    WalletEntry {
                        name: "main".into(),
                        kind: WalletKind::Seed,
                        birthday: Some(2_618_000),
                        age_secs: Some(130),
                    },
                    WalletEntry {
                        name: "phone-watch".into(),
                        kind: WalletKind::Watch,
                        birthday: Some(2_618_000),
                        age_secs: Some(3_700),
                    },
                ]));
            }
            Command::FetchTip => {
                tokio::time::sleep(Duration::from_millis(400)).await;
                self.send(Action::TipFetched(Ok(Tip {
                    height: self.tip,
                    at_unix: Self::now_unix(),
                    spacing_secs: 75,
                })));
            }
            Command::GenerateSeed => self.send(Action::SeedGenerated(SEED_PHRASE.into())),
            Command::Open(spec) => {
                if self.opened {
                    return false;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
                let (name, kind, birthday) = match spec {
                    OpenSpec::Read { name } => {
                        let kind = if name.contains("watch") {
                            WalletKind::Watch
                        } else {
                            WalletKind::Seed
                        };
                        (name, kind, self.birthday)
                    }
                    OpenSpec::Create {
                        name,
                        material,
                        birthday,
                        ..
                    } => {
                        let kind = match material {
                            KeyMaterial::Seed(_) => WalletKind::Seed,
                            KeyMaterial::Ufvk(_) => WalletKind::Watch,
                        };
                        (name, kind, birthday)
                    }
                };
                self.name = name.clone();
                self.kind = kind;
                self.birthday = birthday;
                self.opened = true;
                self.data = self.make_data();
                self.send(Action::WalletOpened(WalletInfo {
                    wallet_name: name,
                    wallet_path: "(fake, nothing on disk)".into(),
                    birthday,
                    server: crate::backend::default_server(),
                    has_orchard: true,
                    spacing_secs: 75,
                    kind,
                }));
                self.send(Action::DataLoaded(self.data.clone()));
                self.launch();
            }
            Command::Refresh => {
                if self.opened {
                    self.send(Action::DataLoaded(self.data.clone()));
                }
            }
            Command::NewUnifiedAddress { orchard, sapling } => {
                let index = self
                    .data
                    .addresses
                    .iter()
                    .filter(|a| a.kind.is_unified())
                    .count() as u32;
                let address = format!("u1{}", self.rng.bech(140));
                self.data.addresses.push(AddressRow {
                    kind: AddressKind::Unified {
                        orchard,
                        sapling,
                        transparent: true,
                    },
                    index,
                    address,
                    received: 0,
                });
                self.send(Action::Notice(format!("new unified address #{index}")));
                self.send(Action::DataLoaded(self.data.clone()));
            }
            Command::NewTransparentAddress => {
                let index = self
                    .data
                    .addresses
                    .iter()
                    .filter(|a| !a.kind.is_unified())
                    .count() as u32;
                let address = format!("t1{}", self.rng.bech(33));
                self.data.addresses.push(AddressRow {
                    kind: AddressKind::Transparent {
                        scope: "External".into(),
                    },
                    index,
                    address,
                    received: 0,
                });
                self.send(Action::Notice(format!("new transparent address #{index}")));
                self.send(Action::DataLoaded(self.data.clone()));
            }
            Command::PauseSync => match self.mode {
                SyncMode::Running => {
                    self.mode = SyncMode::Paused;
                    self.note(SyncNote::Mode(SyncMode::Paused));
                }
                SyncMode::NotRunning => {
                    self.auto = false;
                    self.next_launch = None;
                    self.note(SyncNote::Auto(false));
                }
                _ => {}
            },
            Command::ResumeSync => match self.mode {
                SyncMode::Paused => {
                    self.mode = SyncMode::Running;
                    self.note(SyncNote::Mode(SyncMode::Running));
                }
                SyncMode::NotRunning => {
                    self.auto = true;
                    self.note(SyncNote::Auto(true));
                    self.launch();
                }
                _ => {}
            },
            Command::Rescan => {
                if !self.opened {
                    return false;
                }
                self.note(SyncNote::Reset);
                self.auto = true;
                self.next_launch = None;
                self.note(SyncNote::Auto(true));
                self.launch();
            }
            Command::Quit => {
                self.send(Action::Shutdown);
                return true;
            }
            Command::ProposeSend { to, amount, memo } => {
                tokio::time::sleep(Duration::from_millis(500)).await;
                const FEE: u64 = 15_000;
                let spendable = self.data.balance.total() - self.data.balance.pending();
                let result = if !(to.starts_with("u") || to.starts_with("t") || to.starts_with("z"))
                    || to.len() < 20
                {
                    Err("that is not a zcash address".to_string())
                } else if amount + FEE > spendable {
                    Err(format!(
                        "not enough spendable funds: {} ZEC after the fee",
                        crate::format::zec(spendable.saturating_sub(FEE))
                    ))
                } else {
                    self.proposal = Some((to, amount, memo, FEE));
                    Ok((FEE, amount + FEE))
                };
                self.send(Action::SendProposed(result));
            }
            Command::ConfirmSend => {
                tokio::time::sleep(Duration::from_millis(900)).await;
                let result = match self.proposal.take() {
                    Some((to, amount, memo, fee)) => {
                        let height = self.tip + 1;
                        let tx = self.payment(
                            TransactionKind::Sent(SendType::Send),
                            amount,
                            Some(fee),
                            memo.as_deref(),
                            Some(to),
                            height,
                        );
                        let txid = tx.txid.clone();
                        self.data.txs.insert(0, tx);
                        self.data.balance.orchard.confirmed =
                            self.data.balance.orchard.confirmed.saturating_sub(amount + fee);
                        Ok(vec![txid])
                    }
                    None => Err("nothing is proposed".to_string()),
                };
                self.send(Action::SendDone(result));
                self.send(Action::DataLoaded(self.data.clone()));
            }
            Command::CancelSend => {
                self.proposal = None;
            }
            Command::Copy(_) | Command::SaveTheme(_) | Command::ForceQuit => {}
        }
        false
    }

    fn payment(
        &mut self,
        kind: TransactionKind,
        value: u64,
        fee: Option<u64>,
        memo: Option<&str>,
        address: Option<String>,
        height: u32,
    ) -> TxRow {
        let role = if kind == TransactionKind::Received {
            OutputRole::Received
        } else {
            OutputRole::Sent
        };
        TxRow {
            txid: self.rng.hex(64),
            datetime: Self::now_unix() as u32,
            height,
            status: ConfirmationStatus::Mempool(BlockHeight::from_u32(height)),
            kind,
            value,
            fee,
            inputs: Vec::new(),
            outputs: vec![OutputRow {
                pool: PoolType::ORCHARD,
                role,
                value,
                index: 0,
                address,
                memo: memo.map(str::to_string),
                spend: SpendStatus::Unspent,
                scope: Some("External".into()),
            }],
            wallet_events: vec![WalletEventRow {
                kind: if kind == TransactionKind::Received {
                    ValueTransferKind::Received
                } else {
                    ValueTransferKind::Sent(SentValueTransfer::Send)
                },
                value,
                recipient: None,
                pools: vec![PoolType::ORCHARD],
                memos: memo.map(str::to_string).into_iter().collect(),
            }],
        }
    }

    fn launch(&mut self) {
        self.next_launch = None;
        self.scanned = 0;
        self.ticks = 0;
        self.mode = SyncMode::Running;
        self.note(SyncNote::Mode(SyncMode::Running));
        self.note(SyncNote::SessionStarted {
            tip: self.tip,
            birthday: self.birthday,
            total_blocks: u64::from(self.tip - self.birthday),
            scanned_blocks: 0,
            total_outputs: OUTPUTS,
            scanned_outputs: 0,
        });
        self.note(SyncNote::ScanPlanUpdated {
            ranges: self.plan(0.0),
        });
    }

    fn range(&self, start: u32, end: u32, priority: ScanPriority) -> ScanRange {
        ScanRange::from_parts(
            BlockHeight::from_u32(start)..BlockHeight::from_u32(end),
            priority,
        )
    }

    fn plan(&self, fraction: f64) -> Vec<ScanRange> {
        let span = u64::from(self.tip - self.birthday);
        let at = self.birthday + (span as f64 * fraction) as u32;
        let live_end = (at + 2_000).min(self.tip);
        let mut ranges = vec![self.range(
            self.tip.saturating_sub(100),
            self.tip,
            ScanPriority::ChainTip,
        )];
        if at > self.birthday {
            ranges.push(self.range(self.birthday, at, ScanPriority::Scanned));
        }
        if live_end > at {
            ranges.push(self.range(at, live_end, ScanPriority::Scanning));
        }
        if self.tip.saturating_sub(100) > live_end {
            ranges.push(self.range(
                live_end,
                self.tip.saturating_sub(100),
                ScanPriority::Historic,
            ));
        }
        ranges
    }

    fn on_tick(&mut self) {
        if !self.opened {
            return;
        }
        match self.mode {
            SyncMode::Running => {
                self.ticks += 1;
                self.scanned = (self.scanned + PACE).min(OUTPUTS);
                let fraction = self.scanned as f64 / OUTPUTS as f64;
                let blocks = (u64::from(self.tip - self.birthday) as f64 * fraction) as u64;
                self.note(SyncNote::Progress {
                    total_blocks: u64::from(self.tip - self.birthday),
                    scanned_blocks: blocks,
                    total_outputs: OUTPUTS,
                    scanned_outputs: self.scanned,
                });
                if self.ticks.is_multiple_of(4) {
                    self.note(SyncNote::ScanPlanUpdated {
                        ranges: self.plan(fraction),
                    });
                }
                if self.sessions == 0 && self.ticks.is_multiple_of(9) && self.data.txs.len() < 40 {
                    let tx = self.fresh_tx(fraction);
                    self.note(SyncNote::TxDiscovered {
                        txid: tx.txid.clone(),
                        confirmed: true,
                    });
                    self.data.txs.insert(0, tx);
                    self.data.txs.sort_by(|a, b| b.datetime.cmp(&a.datetime));
                    self.send(Action::DataLoaded(self.data.clone()));
                }
                if self.scanned >= OUTPUTS {
                    self.mode = SyncMode::NotRunning;
                    self.sessions += 1;
                    self.note(SyncNote::Finished {
                        blocks_scanned: u64::from(self.tip - self.birthday),
                    });
                    self.note(SyncNote::Mode(SyncMode::NotRunning));
                    if self.auto {
                        let at = Instant::now() + RESYNC;
                        self.next_launch = Some(at);
                        self.note(SyncNote::Scheduled { at });
                    }
                }
            }
            SyncMode::NotRunning => {
                if self.auto && self.next_launch.is_some_and(|at| at <= Instant::now()) {
                    self.tip += 48;
                    self.note(SyncNote::TipMoved { to: self.tip });
                    self.launch();
                }
            }
            SyncMode::Paused | SyncMode::Shutdown => {}
        }
    }

    fn fresh_tx(&mut self, fraction: f64) -> TxRow {
        let height = self.birthday + (f64::from(self.tip - self.birthday) * fraction) as u32;
        let value = 50_000_000 + self.rng.below(400_000_000);
        let memos = [
            "lunch on me",
            "thanks for the help",
            "coffee money",
            "for the trip",
            "",
        ];
        let memo = memos[self.rng.below(memos.len() as u64) as usize];
        let txid = self.rng.hex(64);
        TxRow {
            txid: txid.clone(),
            datetime: (Self::now_unix() - u64::from(self.tip - height) * 75) as u32,
            height,
            status: ConfirmationStatus::Confirmed(BlockHeight::from_u32(height)),
            kind: TransactionKind::Received,
            value,
            fee: None,
            inputs: Vec::new(),
            outputs: vec![OutputRow {
                pool: PoolType::ORCHARD,
                role: OutputRole::Received,
                value,
                index: 0,
                address: None,
                memo: (!memo.is_empty()).then(|| memo.to_string()),
                spend: SpendStatus::Unspent,
                scope: Some("External".into()),
            }],
            wallet_events: vec![WalletEventRow {
                kind: ValueTransferKind::Received,
                value,
                recipient: None,
                pools: vec![PoolType::ORCHARD],
                memos: if memo.is_empty() {
                    Vec::new()
                } else {
                    vec![memo.to_string()]
                },
            }],
        }
    }

    fn make_data(&mut self) -> WalletData {
        let unified = format!("u1{}", self.rng.bech(140));
        let unified2 = format!("u1{}", self.rng.bech(140));
        let transparent = format!("t1{}", self.rng.bech(33));
        let addresses = vec![
            AddressRow {
                kind: AddressKind::Unified {
                    orchard: true,
                    sapling: true,
                    transparent: true,
                },
                index: 0,
                address: unified.clone(),
                received: 2_312_500_000,
            },
            AddressRow {
                kind: AddressKind::Unified {
                    orchard: true,
                    sapling: false,
                    transparent: false,
                },
                index: 1,
                address: unified2,
                received: 150_000_000,
            },
            AddressRow {
                kind: AddressKind::Transparent {
                    scope: "External".into(),
                },
                index: 0,
                address: transparent.clone(),
                received: 30_000_000,
            },
        ];
        let now = Self::now_unix();
        let day = 86_400u64;
        let mut txs = Vec::new();
        let at = |days_ago: u64, hours: u64| -> (u32, u32) {
            let t = now - days_ago * day - hours * 3600;
            let height = self.tip.saturating_sub(((now - t) / 75) as u32);
            (t as u32, height)
        };
        let steps: Vec<Story> = vec![
            (0, 1, TransactionKind::Received, 42_000_000, None, "lunch on me"),
            (
                1,
                3,
                TransactionKind::Sent(SendType::Send),
                125_000_000,
                Some(15_000),
                "rent, october",
            ),
            (2, 9, TransactionKind::Received, 500_000_000, None, "from the exchange"),
            (5, 7, TransactionKind::Sent(SendType::SendToSelf), 80_000_000, Some(10_000), ""),
            (6, 1, TransactionKind::Sent(SendType::Shield), 30_000_000, Some(10_000), ""),
            (
                12,
                2,
                TransactionKind::Received,
                1_000_000_000,
                None,
                "welcome to the main chain",
            ),
            (15, 8, TransactionKind::Sent(SendType::Send), 7_500_000, Some(10_000), "coffee"),
            (19, 3, TransactionKind::Received, 95_000_000, None, ""),
            (
                31,
                7,
                TransactionKind::Sent(SendType::Send),
                60_000_000,
                Some(10_000),
                "thanks for the review",
            ),
            (36, 4, TransactionKind::Received, 310_000_000, None, "refund"),
            (41, 9, TransactionKind::Received, 12_500_000, None, ""),
            (48, 2, TransactionKind::Sent(SendType::Send), 200_000_000, Some(15_000), ""),
            (
                55,
                6,
                TransactionKind::Received,
                2_500_000_000,
                None,
                "initial funding",
            ),
        ];
        for (i, (days, hours, kind, value, fee, memo)) in steps.into_iter().enumerate() {
            let (datetime, height) = at(days, hours);
            let txid = self.rng.hex(64);
            let status = if i == 0 {
                ConfirmationStatus::Mempool(BlockHeight::from_u32(height))
            } else {
                ConfirmationStatus::Confirmed(BlockHeight::from_u32(height))
            };
            let pool = match i % 3 {
                0 => PoolType::IRONWOOD,
                1 => PoolType::SAPLING,
                _ => PoolType::ORCHARD,
            };
            let memo_opt = (!memo.is_empty()).then(|| memo.to_string());
            let (inputs, outputs, transfers) = match kind {
                TransactionKind::Received => (
                    Vec::new(),
                    vec![OutputRow {
                        pool,
                        role: OutputRole::Received,
                        value,
                        index: 0,
                        address: Some(unified.clone()),
                        memo: memo_opt.clone(),
                        spend: if i % 4 == 1 {
                            SpendStatus::Spent(self.rng.txid())
                        } else {
                            SpendStatus::Unspent
                        },
                        scope: Some("External".into()),
                    }],
                    vec![WalletEventRow {
                        kind: ValueTransferKind::Received,
                        value,
                        recipient: None,
                        pools: vec![pool],
                        memos: memo_opt.clone().into_iter().collect(),
                    }],
                ),
                TransactionKind::Sent(SendType::Send) => (
                    vec![InputRow {
                        pool,
                        value: value + fee.unwrap_or(0) + 1_250_000,
                        source_txid: self.rng.hex(64),
                        output_index: 0,
                        memo: None,
                        pending: false,
                    }],
                    vec![
                        OutputRow {
                            pool,
                            role: OutputRole::Sent,
                            value,
                            index: 0,
                            address: Some(format!("u1{}", self.rng.bech(140))),
                            memo: memo_opt.clone(),
                            spend: SpendStatus::Unspent,
                            scope: None,
                        },
                        OutputRow {
                            pool,
                            role: OutputRole::Change,
                            value: 1_250_000,
                            index: 1,
                            address: None,
                            memo: None,
                            spend: SpendStatus::Unspent,
                            scope: Some("Internal".into()),
                        },
                    ],
                    vec![WalletEventRow {
                        kind: ValueTransferKind::Sent(SentValueTransfer::Send),
                        value,
                        recipient: Some(format!("u1{}", self.rng.bech(140))),
                        pools: Vec::new(),
                        memos: memo_opt.clone().into_iter().collect(),
                    }],
                ),
                _ => (
                    vec![InputRow {
                        pool: PoolType::TRANSPARENT,
                        value: value + fee.unwrap_or(0),
                        source_txid: self.rng.hex(64),
                        output_index: 0,
                        memo: None,
                        pending: false,
                    }],
                    vec![OutputRow {
                        pool: PoolType::ORCHARD,
                        role: OutputRole::ToSelf,
                        value,
                        index: 0,
                        address: None,
                        memo: None,
                        spend: SpendStatus::Unspent,
                        scope: Some("Internal".into()),
                    }],
                    vec![WalletEventRow {
                        kind: match kind {
                            TransactionKind::Sent(SendType::Shield) => {
                                ValueTransferKind::Sent(SentValueTransfer::SendToSelf(
                                    SelfSendValueTransfer::Shield,
                                ))
                            }
                            TransactionKind::Sent(SendType::SendToSelf) => {
                                ValueTransferKind::Sent(SentValueTransfer::SendToSelf(
                                    SelfSendValueTransfer::Basic,
                                ))
                            }
                            _ => ValueTransferKind::Sent(SentValueTransfer::Send),
                        },
                        value,
                        recipient: None,
                        pools: vec![PoolType::ORCHARD],
                        memos: Vec::new(),
                    }],
                ),
            };
            txs.push(TxRow {
                txid,
                datetime,
                height,
                status,
                kind,
                value,
                fee,
                inputs,
                outputs,
                wallet_events: transfers,
            });
        }
        let balance = Balance {
            ironwood: PoolBalance {
                confirmed: 3_000_000_000,
                pending: 0,
            },
            orchard: PoolBalance {
                confirmed: 1_250_000_000,
                pending: 42_000_000,
            },
            sapling: PoolBalance {
                confirmed: 98_000_000,
                pending: 0,
            },
            transparent: PoolBalance {
                confirmed: 30_000_000,
                pending: 0,
            },
        };
        WalletData {
            balance,
            txs,
            addresses,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::app::{Onboarding, Phase, State, reduce};

    async fn drive(
        state: &mut State,
        actions: &mut mpsc::UnboundedReceiver<Action>,
        commands: &mpsc::UnboundedSender<Command>,
        done: impl Fn(&State) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !done(state) {
            let left = deadline.saturating_duration_since(Instant::now());
            let action = tokio::time::timeout(left, actions.recv())
                .await
                .expect("the fake backend went quiet")
                .expect("the fake backend closed its channel");
            for command in reduce(state, action) {
                if !matches!(command, Command::Copy(_) | Command::ForceQuit) {
                    commands.send(command).unwrap();
                }
            }
        }
    }

    #[tokio::test]
    async fn the_fake_backend_walks_the_whole_flow_without_a_network() {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (act_tx, mut act_rx) = mpsc::unbounded_channel();
        let task = spawn(cmd_rx, act_tx);
        let mut s = State::new(
            Phase::Onboarding(Box::new(Onboarding::new(None))),
            Instant::now(),
        );
        cmd_tx.send(Command::ListWallets).unwrap();
        drive(
            &mut s,
            &mut act_rx,
            &cmd_tx,
            |s| matches!(&s.phase, Phase::Onboarding(o) if o.listed),
        )
        .await;
        let Phase::Onboarding(o) = &s.phase else {
            panic!()
        };
        assert_eq!(o.wallets.len(), 2);
        cmd_tx.send(Command::FetchTip).unwrap();
        cmd_tx.send(Command::GenerateSeed).unwrap();
        drive(
            &mut s,
            &mut act_rx,
            &cmd_tx,
            |s| matches!(&s.phase, Phase::Onboarding(o) if o.tip.is_some() && o.seed.is_some()),
        )
        .await;
        cmd_tx
            .send(Command::Open(OpenSpec::Read {
                name: "main".into(),
            }))
            .unwrap();
        drive(&mut s, &mut act_rx, &cmd_tx, |s| {
            s.phase == Phase::Main && !s.txs.is_empty() && s.sync.mode == SyncMode::Running
        })
        .await;
        let info = s.wallet.as_ref().unwrap();
        assert_eq!(info.wallet_name, "main");
        assert_eq!(info.kind, WalletKind::Seed);
        assert!(s.txs.iter().any(|t| t.kind == TransactionKind::Sent(SendType::Send)));
        assert!(s.txs.iter().any(|t| t.first_memo().is_some()));
        assert!(s.addresses.iter().any(|a| a.kind.is_unified()));
        assert!(s.addresses.iter().any(|a| !a.kind.is_unified()));
        let before = s.sync.blocks_scanned;
        drive(&mut s, &mut act_rx, &cmd_tx, |s| {
            s.sync.blocks_scanned > before
        })
        .await;
        let addresses = s.addresses.len();
        cmd_tx
            .send(Command::ProposeSend {
                to: "u1".to_string() + &"x".repeat(40),
                amount: 100_000_000,
                memo: Some("rent".into()),
            })
            .unwrap();
        cmd_tx.send(Command::ConfirmSend).unwrap();
        drive(&mut s, &mut act_rx, &cmd_tx, |s| {
            s.txs[0].kind == TransactionKind::Sent(SendType::Send) && s.txs[0].first_memo() == Some("rent")
        })
        .await;
        cmd_tx.send(Command::NewTransparentAddress).unwrap();
        drive(&mut s, &mut act_rx, &cmd_tx, |s| {
            s.addresses.len() > addresses
        })
        .await;
        cmd_tx.send(Command::Quit).unwrap();
        drive(&mut s, &mut act_rx, &cmd_tx, |s| s.exit).await;
        task.await.unwrap();
    }
}

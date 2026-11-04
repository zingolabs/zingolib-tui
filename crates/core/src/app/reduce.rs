use std::time::Instant;

use super::action::{Action, Command, Key, SyncNote, VotingNote, VotingStageUpdate};
use super::onboarding;
use super::state::{
    DetailMode, FoundTx, Overlay, Phase, ReceiveKind, Screen, SendField, SendForm, SendStage, State,
};
use super::voting::VotingStage;
use crate::format;

pub fn reduce(state: &mut State, action: Action) -> Vec<Command> {
    match action {
        Action::Tick(now) => {
            state.now = now;
            state.ticks = state.ticks.wrapping_add(1);
            Vec::new()
        }
        Action::Resize(w, h) => {
            state.size = (w, h);
            Vec::new()
        }
        Action::Key(key) => on_key(state, key),
        Action::Paste(text) => on_paste(state, &text),
        Action::WalletsListed(wallets) => onboarding::wallets_listed(state, wallets),
        Action::TipFetched(result) => {
            onboarding::tip_fetched(state, result);
            Vec::new()
        }
        Action::SeedGenerated(phrase) => {
            onboarding::seed_generated(state, phrase);
            Vec::new()
        }
        Action::WalletOpened(info) => {
            state.sync.error = None;
            state.wallet = Some(info);
            state.onboarding_backup = None;
            state.phase = Phase::Main;
            state.screen = Screen::Home;
            state.overlay = Overlay::None;
            vec![Command::Refresh]
        }
        Action::SpacingMeasured { spacing_secs } => {
            if let Some(wallet) = state.wallet.as_mut() {
                wallet.spacing_secs = spacing_secs;
            }
            Vec::new()
        }
        Action::OpenFailed(message) => {
            onboarding::open_failed(state, message);
            Vec::new()
        }
        Action::DataLoaded(data) => {
            state.data_loaded = true;
            state.balance = data.balance;
            state.txs = data.txs;
            state.addresses = data.addresses;
            clamp_selections(state);
            Vec::new()
        }
        Action::Sync(note, at) => {
            on_sync(state, note, at);
            Vec::new()
        }
        Action::Notice(message) => {
            state.set_notice(message);
            Vec::new()
        }
        Action::Copied(ok) => {
            state.set_notice(if ok {
                "copied to clipboard"
            } else {
                "clipboard copy failed"
            });
            Vec::new()
        }
        Action::Shutdown => {
            state.exit = true;
            Vec::new()
        }
        Action::SendProposed(result) => {
            match result {
                Ok((fee, total)) => {
                    state.send.error = None;
                    state.send.stage = SendStage::Confirm { fee, total };
                }
                Err(e) => {
                    state.send.error = Some(e);
                    state.send.stage = SendStage::Edit;
                }
            }
            Vec::new()
        }
        Action::SendDone(result) => {
            match result {
                Ok(txids) => {
                    state.send.error = None;
                    state.send.stage = SendStage::Sent { txids };
                    return vec![Command::Refresh];
                }
                Err(e) => {
                    state.send.error = Some(e);
                    state.send.stage = SendStage::Edit;
                }
            }
            Vec::new()
        }
        Action::Voting(note) => on_voting(state, note),
    }
}

/// Folds voting notes into the screen's view state.
fn on_voting(state: &mut State, note: VotingNote) -> Vec<Command> {
    let voting = &mut state.voting;
    match note {
        VotingNote::Round(Ok(summary)) => {
            voting.push_log(format!(
                "round {} resolved at snapshot {}",
                summary.round_id, summary.snapshot_height
            ));
            voting.load_round(
                summary.round_id,
                summary.snapshot_height,
                summary.vote_end_time_seconds,
                summary.eligible_weight_zatoshi,
                summary.bundle_count,
                summary.proposals,
            );
            vec![Command::FetchVotingResults]
        }
        VotingNote::Round(Err(error)) => {
            voting.push_log(format!("round fetch failed: {error}"));
            if error.contains("NoActiveRound") || error.to_lowercase().contains("no active") {
                voting.stage = VotingStage::Idle;
                state.set_notice("no round is open right now - history has past ballots");
            } else {
                voting.stage = VotingStage::Failed(error);
            }
            Vec::new()
        }
        VotingNote::Event { line, stage } => {
            let failure = line.clone();
            voting.push_log(line);
            match stage {
                Some(VotingStageUpdate::Arming) => voting.stage = VotingStage::Arming,
                Some(VotingStageUpdate::Armed) => {
                    voting.stage = VotingStage::Armed;
                    let rows = voting
                        .ballot
                        .iter()
                        .map(|row| {
                            (
                                row.proposal_id,
                                match row.choice {
                                    Some(0) | None => None,
                                    Some(n) => Some(n - 1),
                                },
                            )
                        })
                        .collect();
                    return vec![Command::CastVote(rows)];
                }
                Some(VotingStageUpdate::Casting) => voting.stage = VotingStage::Casting,
                Some(VotingStageUpdate::Finished) => voting.stage = VotingStage::Finished,
                Some(VotingStageUpdate::Failed) => {
                    voting.stage = VotingStage::Failed(failure);
                }
                None => {}
            }
            Vec::new()
        }
        VotingNote::Cast(Ok(outcome)) => {
            voting.push_log(format!(
                "cast finished: all_decided={}, recovery={}",
                outcome.all_decided, outcome.blocking_recovery
            ));
            voting.stage = VotingStage::Finished;
            vec![Command::FetchVotingResults]
        }
        VotingNote::Cast(Err(error)) => {
            voting.push_log(format!("cast failed: {error}"));
            voting.stage = VotingStage::Failed(error);
            Vec::new()
        }
        VotingNote::JournalLoaded(journal) => {
            let restored = journal.len();
            for (proposal_id, decision) in journal {
                if let Some(row) = voting
                    .ballot
                    .iter_mut()
                    .find(|row| row.proposal_id == proposal_id)
                {
                    row.choice = Some(decision.map_or(0, |choice| choice + 1));
                    row.journaled = true;
                }
            }
            if restored > 0 {
                voting.push_log(format!("{restored} journaled decision(s) restored"));
            }
            Vec::new()
        }
        VotingNote::Cleared(proposal_id) => {
            if let Some(row) = voting
                .ballot
                .iter_mut()
                .find(|row| row.proposal_id == proposal_id)
            {
                row.choice = None;
                row.journaled = false;
            }
            state.set_notice(format!("cleared decision on proposal {proposal_id}"));
            Vec::new()
        }
        VotingNote::Results(Ok(rows)) => {
            for (proposal_id, choice) in &rows {
                voting.push_log(format!(
                    "tally: proposal {proposal_id} choice {}",
                    choice + 1
                ));
            }
            voting.tallies = rows;
            Vec::new()
        }
        VotingNote::Past(Ok(rows)) => {
            let count = rows.len();
            voting.past = rows;
            merge_history(voting);
            if count > 0 {
                voting.push_log(format!("{count} previous ballot(s) on file"));
            }
            Vec::new()
        }
        VotingNote::Past(Err(_)) => Vec::new(),
        VotingNote::Source(source) => {
            voting.source = source;
            voting.push_log(format!("voting network: {}", voting.source_label()));
            Vec::new()
        }
        VotingNote::Network(Ok(rows)) => {
            voting.push_log(format!(
                "network lists {} round(s), {} endorsed",
                rows.len(),
                rows.iter().filter(|row| !row.endorsers.is_empty()).count()
            ));
            voting.network = rows;
            merge_history(voting);
            Vec::new()
        }
        VotingNote::Network(Err(_)) => Vec::new(),
        VotingNote::RoundTallies { round_id, tallies } => {
            if let (Ok(raw), Some(row)) = (
                tallies,
                voting
                    .history
                    .iter_mut()
                    .find(|row| row.round_id == round_id),
            ) {
                // Map option labels to their index in the round's options.
                row.tallies = raw
                    .into_iter()
                    .filter_map(|(proposal_id, label, zats)| {
                        let index = row
                            .proposals
                            .iter()
                            .find(|proposal| proposal.id == proposal_id)
                            .and_then(|proposal| {
                                proposal
                                    .options
                                    .iter()
                                    .position(|option| option.eq_ignore_ascii_case(&label))
                            })? as u32;
                        Some((proposal_id, index, zats))
                    })
                    .collect();
            }
            Vec::new()
        }
        VotingNote::Results(Err(error)) => {
            if voting.stage == VotingStage::Finished {
                voting.push_log(format!("results unavailable: {error}"));
            }
            Vec::new()
        }
    }
}

fn clamp_selections(state: &mut State) {
    state.tx_selected = state.tx_selected.min(state.txs.len().saturating_sub(1));
    state.addr_selected = state
        .addr_selected
        .min(state.addresses.len().saturating_sub(1));
}

fn on_sync(state: &mut State, note: SyncNote, at: Instant) {
    let sync = &mut state.sync;
    match note {
        SyncNote::SessionStarted {
            tip,
            birthday,
            total_blocks,
            scanned_blocks,
            total_outputs,
            scanned_outputs,
        } => {
            sync.birthday = Some(birthday);
            sync.tip = Some(tip);
            sync.blocks_total = total_blocks;
            sync.blocks_scanned = scanned_blocks;
            sync.outputs_total = total_outputs;
            sync.outputs_scanned = scanned_outputs;
            sync.error = None;
            sync.next_launch = None;
            sync.progress_log.clear();
            sync.begin_session();
            sync.record_progress(at);
            sync.log(format!(
                "session started at tip {}, {} of {} blocks already scanned",
                format::group(u64::from(tip)),
                format::group(scanned_blocks),
                format::group(total_blocks),
            ));
        }
        SyncNote::ScanPlanUpdated { ranges } => sync.set_scan_plan(ranges),
        SyncNote::TxDiscovered { txid, confirmed } => {
            sync.txs_found += 1;
            let status = if confirmed { "confirmed" } else { "pending" };
            sync.log(format!(
                "transaction {status}: {}",
                format::truncate_middle(&txid, 24)
            ));
            sync.last_found = Some(FoundTx {
                txid,
                confirmed,
                at,
            });
        }
        SyncNote::TipMoved { to } => {
            sync.tip = Some(to);
            sync.log(format!("new block {}", format::group(u64::from(to))));
        }
        SyncNote::Progress {
            total_blocks,
            scanned_blocks,
            total_outputs,
            scanned_outputs,
        } => {
            if total_blocks > 0 {
                sync.blocks_total = total_blocks;
            }
            if total_outputs > 0 {
                sync.outputs_total = total_outputs;
            }
            sync.blocks_scanned = scanned_blocks;
            sync.outputs_scanned = scanned_outputs;
            sync.record_progress(at);
        }
        SyncNote::Finished { blocks_scanned } => {
            sync.sessions_done += 1;
            sync.blocks_scanned = sync.blocks_total;
            sync.outputs_scanned = sync.outputs_total;
            sync.error = None;
            sync.log(format!(
                "session finished, {} blocks scanned",
                format::group(blocks_scanned)
            ));
        }
        SyncNote::Failed(message) => {
            sync.log(format!("sync failed: {message}"));
            sync.error = Some(message);
        }
        SyncNote::Mode(mode) => {
            sync.mode = mode;
        }
        SyncNote::Auto(auto) => {
            sync.auto = auto;
            if !auto {
                sync.next_launch = None;
            }
        }
        SyncNote::Scheduled { at } => {
            sync.next_launch = Some(at);
        }
        SyncNote::Reset => {
            sync.reset_progress();
            sync.txs_found = 0;
            sync.last_found = None;
            sync.sessions_done = 0;
            sync.error = None;
            sync.log("rescan started from birthday".into());
        }
    }
}

fn on_paste(state: &mut State, text: &str) -> Vec<Command> {
    if matches!(state.phase, Phase::Onboarding(_)) {
        return onboarding::paste(state, text);
    }
    if state.overlay != Overlay::None {
        return Vec::new();
    }
    if state.screen == Screen::Pay && state.send.stage == SendStage::Edit {
        let text: String = text.chars().filter(|c| state.send.accepts(*c)).collect();
        state.send.value_mut().push_str(&text);
    }
    Vec::new()
}

fn on_key(state: &mut State, key: Key) -> Vec<Command> {
    match &state.phase {
        Phase::Fatal(_) => {
            if matches!(key, Key::Char('q') | Key::Esc | Key::Enter | Key::CtrlC) {
                state.exit = true;
                return vec![Command::ForceQuit];
            }
            Vec::new()
        }
        Phase::Loading(_) => {
            if matches!(key, Key::CtrlC) {
                state.exit = true;
                return vec![Command::ForceQuit];
            }
            Vec::new()
        }
        Phase::ShuttingDown => {
            if matches!(key, Key::Char('q') | Key::CtrlC) {
                state.exit = true;
                return vec![Command::ForceQuit];
            }
            Vec::new()
        }
        Phase::Onboarding(_) => onboarding::key(state, key),
        Phase::Main => main_key(state, key),
    }
}

fn request_quit(state: &mut State) -> Vec<Command> {
    if state.quit_requested {
        state.exit = true;
        return vec![Command::ForceQuit];
    }
    state.quit_requested = true;
    state.phase = Phase::ShuttingDown;
    vec![Command::Quit]
}

fn main_key(state: &mut State, key: Key) -> Vec<Command> {
    if matches!(key, Key::CtrlC | Key::Char('q')) {
        return request_quit(state);
    }
    match state.overlay.clone() {
        Overlay::Help => {
            if matches!(key, Key::Esc | Key::Char('?')) {
                state.overlay = Overlay::None;
            }
            Vec::new()
        }
        Overlay::TxDetail { mode } => {
            match key {
                Key::Esc => {
                    state.overlay = Overlay::None;
                    state.detail_scroll = 0;
                }
                Key::Char('v') => {
                    state.overlay = Overlay::TxDetail {
                        mode: match mode {
                            DetailMode::InputsOutputs => DetailMode::WalletEvents,
                            DetailMode::WalletEvents => DetailMode::InputsOutputs,
                        },
                    };
                    state.detail_scroll = 0;
                }
                Key::Down | Key::Char('j') => {
                    state.detail_scroll = state.detail_scroll.saturating_add(1)
                }
                Key::Up | Key::Char('k') => {
                    state.detail_scroll = state.detail_scroll.saturating_sub(1)
                }
                Key::PageDown => state.detail_scroll = state.detail_scroll.saturating_add(10),
                Key::PageUp => state.detail_scroll = state.detail_scroll.saturating_sub(10),
                Key::Char('y') => {
                    if let Some(tx) = state.selected_tx() {
                        return vec![Command::Copy(tx.txid.clone())];
                    }
                }
                _ => {}
            }
            Vec::new()
        }
        Overlay::RescanConfirm => {
            match key {
                Key::Char('y') => {
                    state.overlay = Overlay::None;
                    return vec![Command::Rescan];
                }
                Key::Char('n') | Key::Esc => state.overlay = Overlay::None,
                _ => {}
            }
            Vec::new()
        }
        Overlay::NewUnifiedAddress { orchard_only } => {
            match key {
                Key::Up | Key::Down | Key::Char('j') | Key::Char('k') => {
                    state.overlay = Overlay::NewUnifiedAddress {
                        orchard_only: !orchard_only,
                    };
                }
                Key::Enter => {
                    state.overlay = Overlay::None;
                    return vec![Command::NewUnifiedAddress {
                        orchard: true,
                        sapling: !orchard_only,
                    }];
                }
                Key::Esc => state.overlay = Overlay::None,
                _ => {}
            }
            Vec::new()
        }
        Overlay::AddressQr => {
            match key {
                Key::Esc => state.overlay = Overlay::None,
                Key::Char('y') => {
                    if let Some(a) = state.selected_address() {
                        return vec![Command::Copy(a.address.clone())];
                    }
                }
                _ => {}
            }
            Vec::new()
        }
        Overlay::ThemePicker { original } => {
            let count = crate::palette::THEMES.len();
            match key {
                Key::Down | Key::Char('j') => state.theme = (state.theme + 1) % count,
                Key::Up | Key::Char('k') => state.theme = (state.theme + count - 1) % count,
                Key::Home | Key::Char('g') => state.theme = 0,
                Key::End | Key::Char('G') => state.theme = count - 1,
                Key::Enter => {
                    state.overlay = Overlay::None;
                    let def = &crate::palette::THEMES[state.theme];
                    state.set_notice(format!("theme set to {}", def.name));
                    return vec![Command::SaveTheme(def.id)];
                }
                Key::Esc => {
                    state.theme = original;
                    state.overlay = Overlay::None;
                }
                _ => {}
            }
            Vec::new()
        }
        Overlay::None => screen_key(state, key),
    }
}

fn pay_key(state: &mut State, key: Key) -> Vec<Command> {
    if !state.can_spend() {
        if matches!(key, Key::Esc) {
            state.screen = Screen::Home;
        }
        return Vec::new();
    }
    let form = &mut state.send;
    match form.stage.clone() {
        SendStage::Edit => match key {
            Key::Esc => {
                state.screen = Screen::Home;
                Vec::new()
            }
            Key::Tab | Key::Down => {
                form.field = form.field.next();
                Vec::new()
            }
            Key::BackTab | Key::Up => {
                form.field = form.field.previous();
                Vec::new()
            }
            Key::Backspace => {
                form.value_mut().pop();
                Vec::new()
            }
            Key::Enter => match review(form) {
                Ok(command) => {
                    form.error = None;
                    form.stage = SendStage::Proposing;
                    vec![command]
                }
                Err((field, message)) => {
                    form.field = field;
                    form.error = Some(message);
                    Vec::new()
                }
            },
            Key::Char(c) if form.accepts(c) => {
                form.value_mut().push(c);
                Vec::new()
            }
            _ => Vec::new(),
        },
        SendStage::Proposing => match key {
            Key::Esc => {
                form.stage = SendStage::Edit;
                vec![Command::CancelSend]
            }
            _ => Vec::new(),
        },
        SendStage::Confirm { .. } => match key {
            Key::Char('y') | Key::Enter => {
                form.stage = SendStage::Sending;
                vec![Command::ConfirmSend]
            }
            Key::Char('n') | Key::Esc => {
                form.stage = SendStage::Edit;
                vec![Command::CancelSend]
            }
            _ => Vec::new(),
        },
        SendStage::Sending => Vec::new(),
        SendStage::Sent { txids } => match key {
            Key::Char('y') => vec![Command::Copy(txids.join("\n"))],
            Key::Esc | Key::Enter => {
                state.screen = Screen::Home;
                state.send = SendForm::new();
                Vec::new()
            }
            _ => Vec::new(),
        },
    }
}

fn review(form: &SendForm) -> Result<Command, (SendField, String)> {
    let to = form.to.trim();
    if to.is_empty() {
        return Err((SendField::To, "type an address to send to".into()));
    }
    let amount = form
        .amount_zats()
        .map_err(|message| (SendField::Amount, message))?;
    Ok(Command::ProposeSend {
        to: to.to_string(),
        amount,
        memo: form.memo(),
    })
}

fn screen_key(state: &mut State, key: Key) -> Vec<Command> {
    if state.screen == Screen::Pay {
        return pay_key(state, key);
    }
    match key {
        Key::Char('?') => {
            state.overlay = Overlay::Help;
            return Vec::new();
        }
        Key::Char('c') => {
            state.overlay = Overlay::ThemePicker {
                original: state.theme,
            };
            return Vec::new();
        }
        Key::Char('p') => {
            state.send = SendForm::new();
            state.screen = Screen::Pay;
            return Vec::new();
        }
        Key::Char('h') => {
            state.screen = Screen::Home;
            return Vec::new();
        }
        Key::Char('t') => {
            state.screen = Screen::Transactions;
            return Vec::new();
        }
        Key::Char('r') => {
            state.screen = Screen::Receive;
            return Vec::new();
        }
        Key::Char('a') => {
            state.screen = Screen::Addresses;
            return Vec::new();
        }
        Key::Char('s') => {
            state.screen = Screen::Sync;
            return Vec::new();
        }
        Key::Char('v') => {
            state.screen = Screen::Voting;
            return vec![Command::FetchVotingRound, Command::FetchPastBallots];
        }
        Key::Char('b') => {
            state.screen = Screen::Ballots;
            return vec![Command::FetchPastBallots];
        }
        Key::Esc => {
            state.screen = Screen::Home;
            return Vec::new();
        }
        _ => {}
    }
    match state.screen {
        Screen::Pay => Vec::new(),
        Screen::Home | Screen::Transactions => {
            let len = state.txs.len();
            move_selection(&mut state.tx_selected, len, key, state.size.1);
            if matches!(key, Key::Enter) && len > 0 {
                state.overlay = Overlay::TxDetail {
                    mode: DetailMode::InputsOutputs,
                };
                state.detail_scroll = 0;
            }
            Vec::new()
        }
        Screen::Receive => match key {
            Key::Char('u') => {
                state.receive_kind = match state.receive_kind {
                    ReceiveKind::Unified => ReceiveKind::Transparent,
                    ReceiveKind::Transparent => ReceiveKind::Unified,
                };
                Vec::new()
            }
            Key::Char('n') => new_address(state, state.receive_kind == ReceiveKind::Unified),
            Key::Char('y') => match state.receive_address() {
                Some(a) => vec![Command::Copy(a.address.clone())],
                None => Vec::new(),
            },
            _ => Vec::new(),
        },
        Screen::Addresses => {
            let len = state.addresses.len();
            move_selection(&mut state.addr_selected, len, key, state.size.1);
            match key {
                Key::Enter if len > 0 => {
                    state.overlay = Overlay::AddressQr;
                    Vec::new()
                }
                Key::Char('n') => {
                    let unified = state.selected_address().is_none_or(|a| a.kind.is_unified());
                    new_address(state, unified)
                }
                Key::Char('y') => match state.selected_address() {
                    Some(a) => vec![Command::Copy(a.address.clone())],
                    None => Vec::new(),
                },
                _ => Vec::new(),
            }
        }
        Screen::Voting => voting_key(state, key),
        Screen::Ballots => ballots_key(state, key),
        Screen::Sync => match key {
            Key::Char(' ') => match state.sync.p_pauses() {
                Some(true) => vec![Command::PauseSync],
                Some(false) => vec![Command::ResumeSync],
                None => Vec::new(),
            },
            Key::Char('m') => {
                state.sync_map = !state.sync_map;
                Vec::new()
            }
            Key::Char('R') => {
                state.overlay = Overlay::RescanConfirm;
                Vec::new()
            }
            _ => Vec::new(),
        },
    }
}

fn new_address(state: &mut State, unified: bool) -> Vec<Command> {
    if !unified {
        return vec![Command::NewTransparentAddress];
    }
    if state.has_orchard() {
        state.overlay = Overlay::NewUnifiedAddress {
            orchard_only: false,
        };
        Vec::new()
    } else {
        vec![Command::NewUnifiedAddress {
            orchard: false,
            sapling: true,
        }]
    }
}

fn move_selection(selected: &mut usize, len: usize, key: Key, rows: u16) {
    if len == 0 {
        *selected = 0;
        return;
    }
    let last = len - 1;
    let page = usize::from(rows.saturating_sub(6).max(1));
    *selected = match key {
        Key::Down | Key::Char('j') => (*selected + 1).min(last),
        Key::Up | Key::Char('k') => selected.saturating_sub(1),
        Key::PageDown => (*selected + page).min(last),
        Key::PageUp => selected.saturating_sub(page),
        Key::Home | Key::Char('g') => 0,
        Key::End | Key::Char('G') => last,
        _ => *selected,
    };
}

#[cfg(test)]
#[cfg(test)]
fn now() -> Instant {
    Instant::now()
}

#[cfg(test)]
fn main_state() -> State {
    let mut s = State::new(Phase::Main, now());
    s.wallet = Some(crate::app::state::WalletInfo {
        wallet_name: "w.dat".into(),
        wallet_path: "/tmp/w.dat".into(),
        birthday: 100,
        server: "https://mainnet.zainod.zingolabs.dev:443".into(),
        has_orchard: true,
        spacing_secs: 75,
        kind: crate::app::state::WalletKind::Watch,
    });
    s
}

#[cfg(test)]
fn keys(state: &mut State, keys: &[Key]) -> Vec<Command> {
    keys.iter()
        .flat_map(|k| reduce(state, Action::Key(*k)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pepper_sync::sync::{ScanPriority, ScanRange};
    use pepper_sync::wallet::SyncMode;
    use zcash_protocol::consensus::BlockHeight;
    use zingo_status::confirmation_status::ConfirmationStatus;
    use zingolib::wallet::summary::data::TransactionKind;

    use crate::app::state::{AddressKind, AddressRow, Stage, TxRow, WalletData, WalletKind};

    fn range(start: u32, end: u32, priority: ScanPriority) -> ScanRange {
        ScanRange::from_parts(
            BlockHeight::from_u32(start)..BlockHeight::from_u32(end),
            priority,
        )
    }

    fn confirmed(height: u32) -> ConfirmationStatus {
        ConfirmationStatus::Confirmed(BlockHeight::from_u32(height))
    }
    use std::time::Duration;

    fn tx(i: u32) -> TxRow {
        TxRow {
            txid: format!("txid{i}"),
            datetime: 1_700_000_000 + i,
            height: 1000 + i,
            status: confirmed(1000 + i),
            kind: TransactionKind::Received,
            value: 1000,
            fee: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
            wallet_events: Vec::new(),
        }
    }

    #[test]
    fn screen_keys_navigate_and_esc_returns_home() {
        let mut s = main_state();
        keys(&mut s, &[Key::Char('t')]);
        assert_eq!(s.screen, Screen::Transactions);
        keys(&mut s, &[Key::Char('s')]);
        assert_eq!(s.screen, Screen::Sync);
        keys(&mut s, &[Key::Esc]);
        assert_eq!(s.screen, Screen::Home);
        assert!(!s.exit);
        keys(&mut s, &[Key::Char('r'), Key::Char('t')]);
        assert_eq!(s.screen, Screen::Transactions);
        keys(&mut s, &[Key::Char('s'), Key::Char('r')]);
        assert_eq!(s.screen, Screen::Receive);
        keys(&mut s, &[Key::Char('h')]);
        assert_eq!(s.screen, Screen::Home);
        keys(&mut s, &[Key::Char('a')]);
        assert_eq!(s.screen, Screen::Addresses);
    }

    #[test]
    fn q_quits_once_and_forces_on_second_press() {
        let mut s = main_state();
        let cmds = keys(&mut s, &[Key::Char('q')]);
        assert_eq!(cmds, vec![Command::Quit]);
        assert_eq!(s.phase, Phase::ShuttingDown);
        assert!(!s.exit);
        let cmds = keys(&mut s, &[Key::Char('q')]);
        assert_eq!(cmds, vec![Command::ForceQuit]);
        assert!(s.exit);
    }

    #[test]
    fn shutdown_from_backend_exits() {
        let mut s = main_state();
        keys(&mut s, &[Key::Char('q')]);
        reduce(&mut s, Action::Shutdown);
        assert!(s.exit);
    }

    #[test]
    fn help_opens_and_closes_with_esc() {
        let mut s = main_state();
        keys(&mut s, &[Key::Char('?')]);
        assert_eq!(s.overlay, Overlay::Help);
        keys(&mut s, &[Key::Char('t')]);
        assert_eq!(s.screen, Screen::Home, "keys are swallowed under help");
        keys(&mut s, &[Key::Esc]);
        assert_eq!(s.overlay, Overlay::None);
    }

    #[test]
    fn transaction_list_selection_and_detail() {
        let mut s = main_state();
        reduce(
            &mut s,
            Action::DataLoaded(WalletData {
                txs: (0..5).map(tx).collect(),
                ..Default::default()
            }),
        );
        keys(
            &mut s,
            &[
                Key::Char('t'),
                Key::Char('j'),
                Key::Char('j'),
                Key::Char('k'),
            ],
        );
        assert_eq!(s.tx_selected, 1);
        keys(&mut s, &[Key::Char('G')]);
        assert_eq!(s.tx_selected, 4);
        keys(&mut s, &[Key::Char('j')]);
        assert_eq!(s.tx_selected, 4, "clamped at the end");
        keys(&mut s, &[Key::Enter]);
        assert_eq!(
            s.overlay,
            Overlay::TxDetail {
                mode: DetailMode::InputsOutputs
            }
        );
        keys(&mut s, &[Key::Char('v')]);
        assert_eq!(
            s.overlay,
            Overlay::TxDetail {
                mode: DetailMode::WalletEvents
            }
        );
        keys(&mut s, &[Key::Esc]);
        assert_eq!(s.overlay, Overlay::None);
        assert_eq!(s.screen, Screen::Transactions, "esc closes the detail only");
    }

    #[test]
    fn data_reload_clamps_selection() {
        let mut s = main_state();
        reduce(
            &mut s,
            Action::DataLoaded(WalletData {
                txs: (0..5).map(tx).collect(),
                ..Default::default()
            }),
        );
        s.tx_selected = 4;
        reduce(
            &mut s,
            Action::DataLoaded(WalletData {
                txs: (0..2).map(tx).collect(),
                ..Default::default()
            }),
        );
        assert_eq!(s.tx_selected, 1);
    }

    #[test]
    fn receive_screen_toggles_kind_and_derives_addresses() {
        let mut s = main_state();
        keys(&mut s, &[Key::Char('r')]);
        assert_eq!(s.screen, Screen::Receive);
        let cmds = keys(&mut s, &[Key::Char('n')]);
        assert!(cmds.is_empty());
        assert_eq!(
            s.overlay,
            Overlay::NewUnifiedAddress {
                orchard_only: false
            }
        );
        keys(&mut s, &[Key::Char('j')]);
        let cmds = keys(&mut s, &[Key::Enter]);
        assert_eq!(
            cmds,
            vec![Command::NewUnifiedAddress {
                orchard: true,
                sapling: false
            }]
        );
        keys(&mut s, &[Key::Char('u')]);
        assert_eq!(s.receive_kind, ReceiveKind::Transparent);
        let cmds = keys(&mut s, &[Key::Char('n')]);
        assert_eq!(cmds, vec![Command::NewTransparentAddress]);
    }

    #[test]
    fn sapling_only_wallet_skips_receiver_picker() {
        let mut s = main_state();
        s.wallet.as_mut().unwrap().has_orchard = false;
        keys(&mut s, &[Key::Char('r')]);
        let cmds = keys(&mut s, &[Key::Char('n')]);
        assert_eq!(
            cmds,
            vec![Command::NewUnifiedAddress {
                orchard: false,
                sapling: true
            }]
        );
    }

    #[test]
    fn receive_address_is_newest_of_kind() {
        let mut s = main_state();
        let row = |kind, index| AddressRow {
            kind,
            index,
            address: format!("addr{index}"),
            received: 0,
        };
        let unified = |i| {
            row(
                AddressKind::Unified {
                    orchard: true,
                    sapling: true,
                    transparent: false,
                },
                i,
            )
        };
        s.addresses = vec![
            unified(0),
            unified(2),
            unified(1),
            row(
                AddressKind::Transparent {
                    scope: "External".into(),
                },
                0,
            ),
            row(
                AddressKind::Transparent {
                    scope: "Internal".into(),
                },
                7,
            ),
        ];
        assert_eq!(s.receive_address().unwrap().index, 2);
        s.receive_kind = ReceiveKind::Transparent;
        assert_eq!(s.receive_address().unwrap().index, 0);
        s.screen = Screen::Receive;
        let cmds = keys(&mut s, &[Key::Char('y')]);
        assert_eq!(cmds, vec![Command::Copy("addr0".into())]);
    }

    #[test]
    fn sync_screen_pause_resume_and_rescan() {
        let mut s = main_state();
        keys(&mut s, &[Key::Char('s')]);
        s.sync.mode = SyncMode::Running;
        assert_eq!(keys(&mut s, &[Key::Char(' ')]), vec![Command::PauseSync]);
        s.sync.mode = SyncMode::Paused;
        assert_eq!(keys(&mut s, &[Key::Char(' ')]), vec![Command::ResumeSync]);
        s.sync.mode = SyncMode::NotRunning;
        s.sync.auto = false;
        assert_eq!(keys(&mut s, &[Key::Char(' ')]), vec![Command::ResumeSync]);
        keys(&mut s, &[Key::Char('R')]);
        assert_eq!(s.overlay, Overlay::RescanConfirm);
        assert!(keys(&mut s, &[Key::Char('n')]).is_empty());
        keys(&mut s, &[Key::Char('R')]);
        assert_eq!(keys(&mut s, &[Key::Char('y')]), vec![Command::Rescan]);
        assert_eq!(s.overlay, Overlay::None);
        assert!(keys(&mut s, &[Key::Char('m')]).is_empty());
        assert!(s.sync_map);
        keys(&mut s, &[Key::Char('m')]);
        assert!(!s.sync_map);
    }

    #[test]
    fn sync_progress_and_eta() {
        let mut s = main_state();
        let t0 = now();
        reduce(
            &mut s,
            Action::Sync(
                SyncNote::SessionStarted {
                    tip: 2_000_000,
                    birthday: 1,
                    total_blocks: 2_000_000,
                    scanned_blocks: 200_000,
                    total_outputs: 1000,
                    scanned_outputs: 100,
                },
                t0,
            ),
        );
        assert!((s.sync.fraction() - 0.1).abs() < 1e-9);
        assert_eq!(s.sync.tip, Some(2_000_000));
        reduce(
            &mut s,
            Action::Sync(
                SyncNote::ScanPlanUpdated {
                    ranges: vec![range(1, 1_000_000, ScanPriority::Historic)],
                },
                t0,
            ),
        );
        assert_eq!(s.sync.stage(100, 200), Some(Stage::NotYet));
        assert_eq!(s.sync.stage(1_000_000, 1_000_100), None, "outside the plan");
        reduce(
            &mut s,
            Action::Sync(
                SyncNote::Progress {
                    total_blocks: 2_000_000,
                    scanned_blocks: 1_000_000,
                    total_outputs: 1000,
                    scanned_outputs: 500,
                },
                t0 + Duration::from_secs(2),
            ),
        );
        assert!((s.sync.fraction() - 0.5).abs() < 1e-9);
        assert_eq!(s.sync.blocks_scanned, 1_000_000);
        let eta = s.sync.eta_secs().expect("rate known after two samples");
        assert!(
            (2..=3).contains(&eta),
            "a million blocks left at 400,000 a second, got {eta}"
        );
        reduce(
            &mut s,
            Action::Sync(
                SyncNote::ScanPlanUpdated {
                    ranges: vec![
                        range(1, 500_000, ScanPriority::Scanned),
                        range(500_000, 1_000_000, ScanPriority::Scanning),
                    ],
                },
                t0,
            ),
        );
        assert_eq!(s.sync.stage(0, 1_000), Some(Stage::Scanned));
        assert_eq!(
            s.sync.stage(600_000, 600_100),
            Some(Stage::Live),
            "live work shows in any stretch that holds it"
        );
        reduce(
            &mut s,
            Action::Sync(
                SyncNote::Progress {
                    total_blocks: 0,
                    scanned_blocks: 1_400_000,
                    total_outputs: 0,
                    scanned_outputs: 500,
                },
                t0 + Duration::from_secs(3),
            ),
        );
        assert!((s.sync.fraction() - 0.7).abs() < 1e-9);
        assert!(s.sync.eta_secs().is_some());
        reduce(
            &mut s,
            Action::Sync(SyncNote::Finished { blocks_scanned: 20 }, t0),
        );
        assert!((s.sync.fraction() - 1.0).abs() < 1e-9);
        assert_eq!(s.sync.sessions_done, 1);
        reduce(&mut s, Action::Sync(SyncNote::Reset, t0));
        assert_eq!(s.sync.blocks_scanned, 0);
        assert_eq!(s.sync.sessions_done, 0);
        assert!(s.sync.chain_ranges.is_empty());
    }

    #[test]
    fn sync_label_reflects_state() {
        let mut s = main_state();
        assert_eq!(s.sync.label(), "connecting");
        assert!(!s.data_loaded);
        s.sync.mode = SyncMode::Running;
        s.sync.blocks_total = 10;
        s.sync.blocks_scanned = 5;
        assert_eq!(s.sync.label(), "syncing 50%");
        s.sync.mode = SyncMode::NotRunning;
        s.sync.sessions_done = 1;
        s.sync.next_launch = Some(now() + Duration::from_secs(20));
        assert_eq!(s.sync.label(), "up to date");
        s.sync.auto = false;
        assert_eq!(s.sync.label(), "sync paused");
        s.sync.auto = true;
        s.sync.error = Some("boom".into());
        assert_eq!(s.sync.label(), "sync failed");
    }

    #[test]
    fn pay_screen_moves_through_proposal_confirmation_and_send() {
        let mut s = main_state();
        s.wallet.as_mut().unwrap().kind = WalletKind::Seed;
        keys(&mut s, &[Key::Char('p')]);
        assert_eq!(s.screen, Screen::Pay);
        for c in "u1abc".chars() {
            keys(&mut s, &[Key::Char(c)]);
        }
        keys(&mut s, &[Key::Tab]);
        for c in "1.5".chars() {
            keys(&mut s, &[Key::Char(c)]);
        }
        let cmds = keys(&mut s, &[Key::Enter]);
        assert_eq!(
            cmds,
            vec![Command::ProposeSend {
                to: "u1abc".into(),
                amount: 150_000_000,
                memo: None,
            }]
        );
        assert_eq!(s.send.stage, SendStage::Proposing);
        let cmds = reduce(&mut s, Action::SendProposed(Ok((10_000, 150_010_000))));
        assert!(cmds.is_empty());
        assert_eq!(
            s.send.stage,
            SendStage::Confirm {
                fee: 10_000,
                total: 150_010_000
            }
        );
        assert_eq!(keys(&mut s, &[Key::Char('y')]), vec![Command::ConfirmSend]);
        assert_eq!(s.send.stage, SendStage::Sending);
        let cmds = reduce(&mut s, Action::SendDone(Ok(vec!["txid1".into()])));
        assert_eq!(cmds, vec![Command::Refresh]);
        assert_eq!(
            s.send.stage,
            SendStage::Sent {
                txids: vec!["txid1".into()]
            }
        );
        keys(&mut s, &[Key::Esc]);
        assert_eq!(s.screen, Screen::Home);
    }

    #[test]
    fn a_declined_send_backs_out_with_a_cancel() {
        let mut s = main_state();
        s.wallet.as_mut().unwrap().kind = WalletKind::Seed;
        keys(&mut s, &[Key::Char('p'), Key::Enter]);
        reduce(&mut s, Action::SendProposed(Ok((1, 2))));
        let cmds = keys(&mut s, &[Key::Char('n')]);
        assert_eq!(cmds, vec![Command::CancelSend]);
        assert_eq!(s.send.stage, SendStage::Edit);
    }

    #[test]
    fn open_failure_without_prompt_is_fatal() {
        let mut s = State::new(Phase::Loading("Opening wallet".into()), now());
        reduce(&mut s, Action::OpenFailed("boom".into()));
        assert_eq!(s.phase, Phase::Fatal("boom".into()));
        let cmds = keys(&mut s, &[Key::Char('q')]);
        assert_eq!(cmds, vec![Command::ForceQuit]);
        assert!(s.exit);
    }

    #[test]
    fn wallet_opened_enters_main_and_refreshes() {
        let mut s = State::new(Phase::Loading("Opening wallet".into()), now());
        let cmds = reduce(&mut s, Action::WalletOpened(main_state().wallet.unwrap()));
        assert_eq!(cmds, vec![Command::Refresh]);
        assert_eq!(s.phase, Phase::Main);
    }
}

/// Keys on the voting screen: pick a proposal, pick an option, cast.
fn voting_key(state: &mut State, key: Key) -> Vec<Command> {
    let voting = &mut state.voting;
    match key {
        Key::Up
        | Key::Char('k')
        | Key::Down
        | Key::Char('j')
        | Key::PageUp
        | Key::PageDown
        | Key::Home
        | Key::End => {
            let len = voting.selectable_len();
            move_selection(&mut voting.selected, len, key, state.size.1);
            Vec::new()
        }
        Key::Enter => {
            if voting.stage != VotingStage::Ready {
                return Vec::new();
            }
            let item = voting.selected_item();
            match item {
                // An option line: picking it sets that proposal's vote.
                Some((proposal, Some(option))) => {
                    if let Some(row) = voting.ballot.get_mut(proposal)
                        && option <= row.options
                    {
                        row.choice = Some(option);
                    }
                    Vec::new()
                }
                // A Skip line.
                Some((proposal, None)) => {
                    if let Some(row) = voting.ballot.get_mut(proposal) {
                        row.choice = Some(0);
                    }
                    Vec::new()
                }
                // The Cast vote row.
                None => match voting.stage {
                    VotingStage::Ready if voting.decided_all() => {
                        voting.stage = VotingStage::Arming;
                        vec![Command::ArmVoting]
                    }
                    VotingStage::Ready => {
                        state.set_notice(
                            "pick an option for every proposal first - enter sets the highlighted one",
                        );
                        Vec::new()
                    }
                    _ => Vec::new(),
                },
            }
        }
        Key::Char('u') => {
            if let Some((proposal, _)) = voting.selected_item()
                && let Some(row) = voting.ballot.get_mut(proposal)
            {
                row.choice = None;
                let proposal_id = row.proposal_id;
                if row.journaled {
                    return vec![Command::ClearVotingDecision(proposal_id)];
                }
            }
            Vec::new()
        }
        Key::Char('R') => {
            voting.stage = VotingStage::Loading;
            vec![Command::FetchVotingRound]
        }
        Key::Tab | Key::Char('b') => {
            state.screen = Screen::Ballots;
            vec![Command::FetchPastBallots]
        }
        _ => Vec::new(),
    }
}

/// The Ballots screen: a compact list; Enter opens one round's details.
fn ballots_key(state: &mut State, key: Key) -> Vec<Command> {
    let voting = &mut state.voting;
    if voting.history_detail.is_some() {
        match key {
            Key::Left => {
                voting.history_detail = None;
                voting.scroll = 0;
                Vec::new()
            }
            // The detail is a document: the movement keys scroll it.
            Key::Up | Key::Char('k') => {
                voting.scroll = voting.scroll.saturating_sub(1);
                Vec::new()
            }
            Key::Down | Key::Char('j') => {
                voting.scroll = voting.scroll.saturating_add(1);
                Vec::new()
            }
            Key::PageUp => {
                voting.scroll = voting
                    .scroll
                    .saturating_sub(state.size.1.saturating_sub(6).max(1));
                Vec::new()
            }
            Key::PageDown => {
                voting.scroll = voting
                    .scroll
                    .saturating_add(state.size.1.saturating_sub(6).max(1));
                Vec::new()
            }
            Key::Home => {
                voting.scroll = 0;
                Vec::new()
            }
            _ => Vec::new(),
        }
    } else {
        match key {
            Key::Up | Key::Char('k') | Key::Down | Key::Char('j') => {
                move_selection(
                    &mut voting.history_selected,
                    voting.history.len(),
                    key,
                    state.size.1,
                );
                Vec::new()
            }
            Key::Enter | Key::Right => {
                if let Some(row) = voting.history.get(voting.history_selected) {
                    voting.history_detail = Some(voting.history_selected);
                    voting.scroll = 0;
                    let round_id = row.round_id.clone();
                    let needs_tallies = row.status == 3 && row.tallies.is_empty();
                    if needs_tallies {
                        vec![Command::FetchRoundTallies(round_id)]
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            }
            Key::Left => {
                state.screen = Screen::Voting;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}

/// Rebuilds the merged history list: the network's rounds joined with the
/// sidecar's record of this wallet's participation. Preserves tallies
/// already fetched and keeps the selection clamped.
fn merge_history(voting: &mut crate::app::voting::VotingView) {
    let previous = std::mem::take(&mut voting.history);
    let tally_of = |round_id: &str| {
        previous
            .iter()
            .find(|row| row.round_id == round_id)
            .map(|row| row.tallies.clone())
            .unwrap_or_default()
    };
    for round in &voting.network {
        voting.history.push(crate::app::voting::HistoryRow {
            round_id: round.round_id.clone(),
            end_time: round.end_time,
            snapshot_height: round.snapshot_height,
            status: round.status,
            proposals: round.proposals.clone(),
            my_votes: voting
                .past
                .iter()
                .find(|ballot| ballot.round_id == round.round_id)
                .map(|ballot| ballot.votes.clone())
                .unwrap_or_default(),
            tallies: tally_of(&round.round_id),
            endorsers: round.endorsers.clone(),
        });
    }
    // Sidecar rounds the network no longer lists still belong to history.
    for ballot in &voting.past {
        if !voting
            .history
            .iter()
            .any(|row| row.round_id == ballot.round_id)
        {
            voting.history.push(crate::app::voting::HistoryRow {
                round_id: ballot.round_id.clone(),
                end_time: 0,
                snapshot_height: ballot.snapshot_height,
                status: 0,
                proposals: Vec::new(),
                my_votes: ballot.votes.clone(),
                tallies: tally_of(&ballot.round_id),
                endorsers: Vec::new(),
            });
        }
    }
    // Open rounds first (soonest closing at the top), then the rest,
    // newest close first. Every round stays its own row.
    voting.history.sort_by(|a, b| {
        let open_a = a.status == 1;
        let open_b = b.status == 1;
        open_b.cmp(&open_a).then_with(|| {
            if open_a && open_b {
                a.end_time.cmp(&b.end_time)
            } else {
                b.end_time.cmp(&a.end_time)
            }
        })
    });
    voting.history_selected = voting
        .history_selected
        .min(voting.history.len().saturating_sub(1));
}

#[cfg(test)]
mod voting_tests {
    use super::super::action::{VotingNote, VotingStageUpdate, VotingSummary};
    use super::*;
    use crate::app::voting::VotingStage;

    fn summary() -> VotingSummary {
        VotingSummary {
            round_id: "abc123".into(),
            snapshot_height: 3_000_000,
            vote_end_time_seconds: 1_900_000_000,
            eligible_weight_zatoshi: 12_460_000_000,
            bundle_count: 2,
            proposals: vec![
                crate::app::ProposalInfo {
                    id: 1,
                    title: "Fund core dev".into(),
                    options: vec!["Approve".into(), "Reject".into(), "Abstain".into()],
                },
                crate::app::ProposalInfo {
                    id: 2,
                    title: "Charter".into(),
                    options: vec!["Ratify".into(), "Reject".into()],
                },
            ],
        }
    }

    pub(super) fn voting_state() -> State {
        let mut s = main_state();
        s.screen = Screen::Voting;
        let _ = reduce(&mut s, Action::Voting(VotingNote::Round(Ok(summary()))));
        s
    }

    #[test]
    fn v_key_switches_screen_and_fetches() {
        let mut s = main_state();
        let commands = keys(&mut s, &[Key::Char('v')]);
        assert_eq!(s.screen, Screen::Voting);
        assert_eq!(
            commands,
            vec![Command::FetchVotingRound, Command::FetchPastBallots]
        );
    }

    #[test]
    fn round_note_opens_the_ballot_and_pulls_results() {
        let mut s = main_state();
        s.screen = Screen::Voting;
        let commands = reduce(&mut s, Action::Voting(VotingNote::Round(Ok(summary()))));
        assert_eq!(s.voting.stage, VotingStage::Ready);
        assert_eq!(s.voting.ballot.len(), 2);
        assert_eq!(commands, vec![Command::FetchVotingResults]);
    }

    #[test]
    fn round_failure_lands_on_failed() {
        let mut s = main_state();
        s.screen = Screen::Voting;
        reduce(
            &mut s,
            Action::Voting(VotingNote::Round(Err("offline".into()))),
        );
        assert_eq!(s.voting.stage, VotingStage::Failed("offline".into()));
    }

    #[test]
    fn enter_only_casts_from_the_cast_row() {
        let mut s = voting_state();
        keys(&mut s, &[Key::Enter]);
        keys(&mut s, &[Key::Down, Key::Enter]);
        // Undecided ballot on the Cast row explains itself.
        keys(&mut s, &[Key::End]);
        assert!(s.voting.on_cast_row());
        assert!(keys(&mut s, &[Key::Enter]).is_empty());
        assert_eq!(
            s.notice_text(),
            Some("pick an option for every proposal first - enter sets the highlighted one")
        );
        // Complete ballot casts: Up lands on proposal 2's Skip line, enter
        // sets it, then the Cast row arms.
        let commands = keys(&mut s, &[Key::Up, Key::Enter, Key::Down, Key::Enter]);
        assert_eq!(commands, vec![Command::ArmVoting]);
        assert_eq!(s.voting.stage, VotingStage::Arming);
    }

    #[test]
    fn no_active_round_reads_as_empty_not_failed() {
        let mut s = voting_state();
        reduce(
            &mut s,
            Action::Voting(VotingNote::Round(Err(
                "voting failure: NoActiveRound".into()
            ))),
        );
        assert_eq!(s.voting.stage, VotingStage::Idle);
        assert!(s.notice_text().unwrap().contains("no round is open"));
    }

    #[test]
    fn finished_ballot_is_read_only() {
        let mut s = voting_state();
        s.voting.stage = VotingStage::Finished;
        keys(&mut s, &[Key::Char('1'), Key::Right]);
        assert_eq!(s.voting.ballot[0].choice, None);
        assert_eq!(keys(&mut s, &[Key::Enter]), Vec::new());
    }

    #[test]
    fn incomplete_ballot_explains_itself_on_cast() {
        let mut s = voting_state();
        keys(&mut s, &[Key::Enter, Key::End, Key::Enter]);
        assert!(keys(&mut s, &[Key::Enter]).is_empty());
        assert_eq!(s.voting.stage, VotingStage::Ready);
        assert_eq!(
            s.notice_text(),
            Some("pick an option for every proposal first - enter sets the highlighted one")
        );
    }

    #[test]
    fn armed_event_casts_the_journaled_ballot() {
        let mut s = voting_state();
        keys(
            &mut s,
            &[
                Key::Down,
                Key::Enter,
                Key::Down,
                Key::Down,
                Key::Down,
                Key::Down,
                Key::Down,
                Key::Down,
                Key::Down,
                Key::Enter,
            ],
        );
        let commands = reduce(
            &mut s,
            Action::Voting(VotingNote::Event {
                line: "session armed".into(),
                stage: Some(VotingStageUpdate::Armed),
            }),
        );
        assert_eq!(s.voting.stage, VotingStage::Armed);
        assert_eq!(
            commands,
            vec![Command::CastVote(vec![(1, Some(1)), (2, None)])]
        );
    }

    #[test]
    fn journal_restores_one_based_choices() {
        let mut s = voting_state();
        reduce(
            &mut s,
            Action::Voting(VotingNote::JournalLoaded(vec![(1, Some(0)), (2, None)])),
        );
        assert_eq!(s.voting.ballot[0].choice, Some(1));
        assert!(s.voting.ballot[0].journaled);
        assert_eq!(s.voting.ballot[1].choice, Some(0));
        assert!(s.voting.ballot[1].journaled);
    }

    #[test]
    fn clear_drops_a_journaled_choice() {
        let mut s = voting_state();
        reduce(
            &mut s,
            Action::Voting(VotingNote::JournalLoaded(vec![(1, Some(2))])),
        );
        let commands = keys(&mut s, &[Key::Char('u')]);
        assert_eq!(commands, vec![Command::ClearVotingDecision(1)]);
        reduce(&mut s, Action::Voting(VotingNote::Cleared(1)));
        assert_eq!(s.voting.ballot[0].choice, None);
        assert!(!s.voting.ballot[0].journaled);
    }

    #[test]
    fn cast_outcome_finishes_and_pulls_results() {
        let mut s = voting_state();
        s.voting.stage = VotingStage::Casting;
        let commands = reduce(
            &mut s,
            Action::Voting(VotingNote::Cast(Ok(crate::app::VotingOutcome {
                all_decided: true,
                blocking_recovery: false,
            }))),
        );
        assert_eq!(s.voting.stage, VotingStage::Finished);
        assert_eq!(commands, vec![Command::FetchVotingResults]);
    }
}

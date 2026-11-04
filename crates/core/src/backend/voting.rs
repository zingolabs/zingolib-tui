//! Voting commands for the real backend: thin adapters from the plain
//! action/command vocabulary to the LightClient voting surface. Staging is
//! the default voting network; the config lives behind
//! `VotingSessionConfig::staging` inside zingolib.

use tokio::sync::mpsc;
use zingolib::lightclient::LightClient;
use zingolib_voting::{BallotDecision, Decision, VotingEvent};
use zip32::AccountId;

use crate::app::action::{VotingNote, VotingOutcome, VotingStageUpdate, VotingSummary};

/// Fetches the active round summary the screen renders.
pub(super) async fn fetch_round(lc: &mut LightClient) -> Result<VotingSummary, String> {
    let summary = lc
        .active_voting_round(AccountId::ZERO)
        .await
        .map_err(|e| e.to_string())?;
    let round = &summary.round;
    Ok(VotingSummary {
        round_id: round.round_id.clone(),
        snapshot_height: round.params.snapshot_height,
        vote_end_time_seconds: round.vote_end_time_seconds,
        eligible_weight_zatoshi: summary.eligible_weight_zatoshi,
        bundle_count: summary.bundle_count,
        proposals: round
            .proposals
            .iter()
            .map(|p| crate::app::ProposalInfo {
                id: p.id,
                title: p.title.clone(),
                options: p.option_labels.clone(),
            })
            .collect(),
    })
}

/// Reads the journaled decisions for `round_id` back from the sidecar.
pub(super) async fn journal(lc: &LightClient, round_id: &str) -> Vec<(u32, Option<u32>)> {
    match lc.journaled_decisions(round_id).await {
        Ok(decisions) => decisions
            .iter()
            .map(|d| (d.proposal_id, decision_index(d)))
            .collect(),
        Err(e) => {
            tracing::warn!("journal read failed: {e}");
            Vec::new()
        }
    }
}

/// Lists the wallet's past voting rounds from its sidecar, newest first.
pub(super) async fn past(lc: &LightClient) -> Result<Vec<crate::app::PastBallotRow>, String> {
    let ballots = lc.past_ballots().await.map_err(|e| e.to_string())?;
    Ok(ballots
        .into_iter()
        .map(|ballot| crate::app::PastBallotRow {
            complete: matches!(ballot.phase, zingolib_voting::RoundPhase::VoteReady),
            round_id: ballot.round_id,
            snapshot_height: ballot.snapshot_height,
            created_at: ballot.created_at,
            votes: ballot
                .votes
                .iter()
                .map(|vote| (vote.proposal_id, vote.choice))
                .collect(),
        })
        .collect())
}

/// Lists every round the vote network publishes, most recently closing
/// first.
pub(super) async fn network(lc: &LightClient) -> Result<Vec<crate::app::NetworkRoundRow>, String> {
    let rounds = lc.network_rounds().await.map_err(|e| e.to_string())?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut rows: Vec<crate::app::NetworkRoundRow> = rounds
        .into_iter()
        .map(|round| crate::app::NetworkRoundRow {
            open: round.vote_end_time_seconds > now,
            endorsers: round.endorsers.clone(),
            round_id: round.round_id,
            end_time: round.vote_end_time_seconds,
            snapshot_height: round.snapshot_height,
            status: round.status,
            proposals: round
                .proposals
                .iter()
                .map(|proposal| crate::app::ProposalInfo {
                    id: proposal.id,
                    title: proposal.title.clone(),
                    options: proposal.option_labels.clone(),
                })
                .collect(),
            tallies: Vec::new(),
        })
        .collect();
    rows.sort_by(|a, b| b.end_time.cmp(&a.end_time));
    Ok(rows)
}

/// The resolved voting source, for the screen's network label.
pub(super) fn source(lc: &LightClient) -> String {
    lc.voting_source()
}

/// Fetches a finalized round's network tally for the detail view.
pub(super) async fn round_tallies(
    lc: &LightClient,
    round_id: &str,
) -> Result<Vec<(u32, String, u64)>, String> {
    let tallies = lc
        .tally_results(round_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(tallies
        .into_iter()
        .flat_map(|proposal| {
            proposal
                .options
                .into_iter()
                .map(move |(label, zats)| (proposal.proposal_id, label, zats))
        })
        .collect())
}

/// Reads a round's durable results from its sidecar.
pub(super) async fn results(lc: &LightClient, round_id: &str) -> Result<Vec<(u32, u32)>, String> {
    let results = lc
        .voting_results(round_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(results
        .votes
        .iter()
        .map(|vote| (vote.proposal_id, vote.choice))
        .collect())
}

/// Maps the screen's plain decision rows to SDK decisions:
/// `None` or `Some(0)` = skipped, `Some(n)` = choice n.
pub(super) fn ballot(rows: &[(u32, Option<u32>)]) -> Vec<BallotDecision> {
    rows.iter()
        .map(|(proposal_id, choice)| BallotDecision {
            proposal_id: *proposal_id,
            decision: match choice {
                None | Some(0) => Decision::Skipped,
                Some(n) => Decision::Choice(*n),
            },
        })
        .collect()
}

/// Folds a stored decision into the screen's 1-based vocabulary
/// (`None` = skipped, `Some(n)` = 0-based choice n).
fn decision_index(decision: &BallotDecision) -> Option<u32> {
    match decision.decision {
        Decision::Choice(n) => Some(n),
        Decision::Skipped => None,
    }
}

/// Arms the active round and hands the client back (armed) plus its event
/// handle. Arm does network work but no proving, so it runs detached while
/// the backend keeps ticking.
pub(super) async fn run_arm(
    mut lc: LightClient,
    actions: mpsc::UnboundedSender<crate::app::Action>,
    returned: tokio::sync::oneshot::Sender<(
        LightClient,
        Option<zingolib_voting::VotingProgressHandle>,
    )>,
) {
    match arm(&mut lc, &actions).await {
        Ok(handle) => {
            let _ = returned.send((lc, Some(handle)));
        }
        Err(error) => {
            let _ = returned.send((lc, None));
            let _ = actions.send(crate::app::Action::Voting(VotingNote::Event {
                line: format!("arming failed: {error}"),
                stage: Some(VotingStageUpdate::Failed),
            }));
        }
    }
}

async fn arm(
    lc: &mut LightClient,
    actions: &mpsc::UnboundedSender<crate::app::Action>,
) -> Result<zingolib_voting::VotingProgressHandle, String> {
    let round_id = lc
        .active_voting_round(AccountId::ZERO)
        .await
        .map_err(|e| e.to_string())?
        .round
        .round_id;
    let handle = lc
        .begin_voting(AccountId::ZERO, &round_id)
        .await
        .map_err(|e| e.to_string())?;
    let _ = actions.send(crate::app::Action::Voting(VotingNote::Event {
        line: format!("session armed for {round_id}"),
        stage: Some(VotingStageUpdate::Armed),
    }));
    Ok(handle)
}

/// Drives the armed round to quiescence with the journaled ballot, then
/// hands the client back. Events stream out on `actions` as they happen.
pub(super) async fn run_cast(
    mut lc: LightClient,
    handle: zingolib_voting::VotingProgressHandle,
    actions: mpsc::UnboundedSender<crate::app::Action>,
    round_id: String,
    decisions: Vec<BallotDecision>,
    returned: tokio::sync::oneshot::Sender<LightClient>,
) {
    pump(handle.subscribe(), actions.clone());
    let result = lc
        .cast_vote(&round_id, decisions)
        .await
        .map_err(|e| e.to_string());
    let _ = returned.send(lc);
    actions
        .send(crate::app::Action::Voting(VotingNote::Cast(result.map(
            |outcome| VotingOutcome {
                all_decided: outcome.all_decided,
                blocking_recovery: outcome.blocking_recovery,
            },
        ))))
        .ok();
}

/// Forwards lifecycle events to the UI until the terminal one arrives.
fn pump(
    mut rx: tokio::sync::broadcast::Receiver<VotingEvent>,
    actions: mpsc::UnboundedSender<crate::app::Action>,
) {
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let (line, stage) = describe(&event);
                    let terminal = matches!(
                        stage,
                        Some(VotingStageUpdate::Finished | VotingStageUpdate::Failed)
                    );
                    let _ = actions.send(crate::app::Action::Voting(VotingNote::Event {
                        line,
                        stage,
                    }));
                    if terminal {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    let _ = actions.send(crate::app::Action::Voting(VotingNote::Event {
                        line: format!("event stream lagged, {n} skipped"),
                        stage: None,
                    }));
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// One log line per lifecycle event, plus the stage it moves the screen to.
fn describe(event: &VotingEvent) -> (String, Option<VotingStageUpdate>) {
    match event {
        VotingEvent::ConfigResolved { source } => (format!("config resolved from {source}"), None),
        VotingEvent::RoundResolved {
            round_id,
            snapshot_height,
            ..
        } => (
            format!("round {round_id} resolved at snapshot {snapshot_height}"),
            Some(VotingStageUpdate::Arming),
        ),
        VotingEvent::RoundInitialized { round_id } => {
            (format!("round {round_id} initialized"), None)
        }
        VotingEvent::SnapshotNotesSelected {
            note_count,
            eligible_weight_zatoshi,
        } => (
            format!("{note_count} note(s) selected, weight {eligible_weight_zatoshi} zatoshi"),
            None,
        ),
        VotingEvent::HotkeyReady { address, .. } => (format!("hotkey ready {address}"), None),
        VotingEvent::PirWarmup { cached, fetched } => (
            format!("PIR warmup: {cached} cached, {fetched} fetched"),
            Some(VotingStageUpdate::Armed),
        ),
        VotingEvent::BundlePrepared { bundle_index } => (
            format!("bundle {bundle_index} prepared"),
            Some(VotingStageUpdate::Casting),
        ),
        VotingEvent::DelegationSigned {
            bundle_index,
            delegated_weight_zatoshi,
        } => (
            format!("bundle {bundle_index} signed, {delegated_weight_zatoshi} zatoshi delegated"),
            Some(VotingStageUpdate::Casting),
        ),
        VotingEvent::DelegationSubmitted { bundle_index } => (
            format!("bundle {bundle_index} submitted"),
            Some(VotingStageUpdate::Casting),
        ),
        VotingEvent::DelegationConfirmed { bundle_index } => (
            format!("bundle {bundle_index} confirmed"),
            Some(VotingStageUpdate::Casting),
        ),
        VotingEvent::VoteCommitted { proposal_id, .. } => (
            format!("vote committed for proposal {proposal_id}"),
            Some(VotingStageUpdate::Casting),
        ),
        VotingEvent::VoteConfirmed { proposal_id, .. } => (
            format!("vote confirmed for proposal {proposal_id}"),
            Some(VotingStageUpdate::Casting),
        ),
        VotingEvent::ShareScheduled {
            proposal_id,
            share_index,
            ..
        } => (
            format!("share {share_index} scheduled for proposal {proposal_id}"),
            Some(VotingStageUpdate::Casting),
        ),
        VotingEvent::ShareDelivered {
            proposal_id,
            share_index,
            ..
        } => (
            format!("share {share_index} delivered for proposal {proposal_id}"),
            Some(VotingStageUpdate::Casting),
        ),
        VotingEvent::Finished { round_id } => (
            format!("round {round_id} finished"),
            Some(VotingStageUpdate::Finished),
        ),
        VotingEvent::Failed { error, .. } => (
            format!("failed: {} ({:?})", error.message, error.kind),
            Some(VotingStageUpdate::Failed),
        ),
    }
}

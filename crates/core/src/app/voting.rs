//! Voting screen state: what the UI needs to show a round and drive a
//! ballot, kept free of SDK types so the reducer and tests stay pure.

/// One proposal on the ballot with the user's current decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BallotRow {
    pub proposal_id: u32,
    pub title: String,
    pub options: u32,
    /// What each choice means, indexed by decision (position 0 = first).
    pub option_labels: Vec<String>,
    /// `None` = undecided, `Some(0)` = skipped, `Some(n)` = choice n.
    pub choice: Option<u32>,
    /// A previously journaled decision restored from the sidecar.
    pub journaled: bool,
}

impl BallotRow {
    pub fn decision_label(&self) -> String {
        match self.choice {
            None => "undecided".into(),
            Some(0) => "skipped".into(),
            Some(n) => format!("option {n}"),
        }
    }
}

/// One past round from the sidecar, plain values only: choice stays 0-based
/// (`None` never occurs; skipped votes store choice 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PastBallotRow {
    pub round_id: String,
    pub snapshot_height: u64,
    pub created_at: u64,
    pub complete: bool,
    pub votes: Vec<(u32, u32)>,
}

/// One merged history entry: the network's round joined with this wallet's
/// sidecar record, so a row carries both what happened and what we voted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryRow {
    pub round_id: String,
    pub end_time: u64,
    pub snapshot_height: u64,
    /// Raw server status: 1 active, 2 tallying, 3 finalized, 4 pending.
    pub status: u32,
    pub proposals: Vec<ProposalInfo>,
    /// The orgs that endorsed this round, in registry order.
    pub endorsers: Vec<String>,
    /// This wallet's recorded votes: (proposal_id, 0-based choice).
    pub my_votes: Vec<(u32, u32)>,
    /// Finalized aggregate per (proposal, decision), zatoshi; filled on
    /// demand when the detail view opens.
    pub tallies: Vec<(u32, u32, u64)>,
}

impl HistoryRow {
    pub fn status_label(&self) -> &'static str {
        match self.status {
            1 => "active",
            2 => "tallying",
            3 => "finalized",
            4 => "pending",
            _ => "unknown",
        }
    }

    /// This wallet's choice on one proposal, 1-based for display.
    pub fn my_choice(&self, proposal_id: u32) -> Option<u32> {
        self.my_votes
            .iter()
            .find(|(id, _)| *id == proposal_id)
            .map(|(_, choice)| choice + 1)
    }
}

/// One proposal as the network describes it: what it is and what each
/// choice means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalInfo {
    pub id: u32,
    pub title: String,
    pub options: Vec<String>,
}

/// One round from the vote network's public list, plain values only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkRoundRow {
    pub round_id: String,
    pub end_time: u64,
    pub snapshot_height: u64,
    pub open: bool,
    /// The orgs that endorsed this round, in registry order.
    pub endorsers: Vec<String>,
    /// Raw server status: 1 active, 2 tallying, 3 finalized, 4 pending.
    pub status: u32,
    pub proposals: Vec<ProposalInfo>,
    /// Finalized aggregate per (proposal, decision), zatoshi.
    pub tallies: Vec<(u32, u32, u64)>,
}

/// Lifecycle of the voting screen, mirroring the SDK's handle flow:
/// fetch round -> arm session -> cast -> quiescence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VotingStage {
    Idle,
    Loading,
    Ready,
    Arming,
    Armed,
    Casting,
    Finished,
    Failed(String),
}

/// Everything the voting screen renders.
#[derive(Clone, Debug)]
pub struct VotingView {
    pub stage: VotingStage,
    pub round_id: String,
    pub snapshot_height: u64,
    pub vote_end: u64,
    pub eligible_weight_zatoshi: u64,
    pub bundle_count: u32,
    pub ballot: Vec<BallotRow>,
    pub selected: usize,
    pub tallies: Vec<(u32, u32)>,
    pub past: Vec<PastBallotRow>,
    pub network: Vec<NetworkRoundRow>,
    /// The resolved voting config source (URL), set by the backend.
    pub source: String,
    pub history: Vec<HistoryRow>,
    pub history_selected: usize,
    pub history_detail: Option<usize>,
    /// Detail-view scroll offset in lines; the list view derives its own.
    pub scroll: u16,
    pub log: Vec<String>,
    pub fake: bool,
}

impl Default for VotingView {
    fn default() -> Self {
        Self::new()
    }
}

impl VotingView {
    pub fn new() -> Self {
        Self {
            stage: VotingStage::Idle,
            round_id: String::new(),
            snapshot_height: 0,
            vote_end: 0,
            eligible_weight_zatoshi: 0,
            bundle_count: 0,
            ballot: Vec::new(),
            selected: 0,
            tallies: Vec::new(),
            past: Vec::new(),
            network: Vec::new(),
            source: String::new(),
            history: Vec::new(),
            history_selected: 0,
            history_detail: None,
            scroll: 0,
            log: Vec::new(),
            fake: false,
        }
    }

    pub fn decided_all(&self) -> bool {
        !self.ballot.is_empty() && self.ballot.iter().all(|row| row.choice.is_some())
    }

    /// How many proposals carry a decision, for the cast-row counter.
    pub fn decided_count(&self) -> usize {
        self.ballot
            .iter()
            .filter(|row| row.choice.is_some())
            .count()
    }

    /// True while the selection sits on the trailing Cast vote row.
    pub fn on_cast_row(&self) -> bool {
        self.selected + 1 == self.selectable_len()
    }

    /// Selectable lines in order: each proposal's options, then its Skip
    /// line, then the trailing Cast vote row.
    pub fn selectable_len(&self) -> usize {
        self.ballot
            .iter()
            .map(|row| row.options as usize + 1)
            .sum::<usize>()
            + 1
    }

    /// What the flat selection points at: `(proposal, Some(option 1-based))`
    /// for an option line, `(proposal, None)` for its Skip line, or `None`
    /// for the Cast vote row.
    pub fn selected_item(&self) -> Option<(usize, Option<u32>)> {
        let mut rest = self.selected;
        for (index, row) in self.ballot.iter().enumerate() {
            let options = row.options as usize;
            if rest < options {
                return Some((index, Some(rest as u32 + 1)));
            }
            rest -= options;
            if rest == 0 {
                return Some((index, None));
            }
            rest -= 1;
        }
        None
    }

    /// One option's label, falling back to "Option N" when the network did
    /// not publish labels.
    /// Friendly name for the configured voting network.
    pub fn source_label(&self) -> String {
        if self.source.contains("/prod") || self.source.contains("production") {
            "mainnet (production)".to_string()
        } else if self.source.contains("/stage") || self.source.contains("staging") {
            "staging".to_string()
        } else if self.source.contains("demo") {
            "demo".to_string()
        } else {
            crate::format::truncate_middle(&self.source, 40)
        }
    }

    pub fn option_label(&self, proposal: usize, option: u32) -> String {
        self.ballot
            .get(proposal)
            .and_then(|row| row.option_labels.get(option as usize))
            .cloned()
            .unwrap_or_else(|| format!("Option {}", option + 1))
    }
}

impl VotingView {
    /// Round metadata + the wallet's eligibility, from the SDK summary.
    pub fn load_round(
        &mut self,
        round_id: String,
        snapshot_height: u64,
        vote_end: u64,
        eligible_weight_zatoshi: u64,
        bundle_count: u32,
        proposals: Vec<crate::app::ProposalInfo>,
    ) {
        self.round_id = round_id;
        self.snapshot_height = snapshot_height;
        self.vote_end = vote_end;
        self.eligible_weight_zatoshi = eligible_weight_zatoshi;
        self.bundle_count = bundle_count;
        self.ballot = proposals
            .into_iter()
            .map(|proposal| BallotRow {
                proposal_id: proposal.id,
                title: proposal.title.clone(),
                options: proposal.options.len() as u32,
                option_labels: proposal.options,
                choice: None,
                journaled: false,
            })
            .collect();
        self.selected = 0;
        self.history_detail = None;
        self.tallies.clear();
        self.stage = VotingStage::Ready;
    }

    pub fn push_log(&mut self, line: impl Into<String>) {
        const MAX_LOG: usize = 200;
        self.log.push(line.into());
        if self.log.len() > MAX_LOG {
            let drop = self.log.len() - MAX_LOG;
            self.log.drain(..drop);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> VotingView {
        let mut view = VotingView::new();
        view.load_round(
            "round".into(),
            100,
            200,
            5_0000_0000,
            2,
            vec![crate::app::ProposalInfo {
                id: 1,
                title: "Proposal".into(),
                options: vec!["Approve".into(), "Reject".into(), "Abstain".into()],
            }],
        );
        view
    }

    #[test]
    fn undecided_ballot_blocks_cast() {
        assert!(!view().decided_all());
    }

    #[test]
    fn full_ballot_allows_cast() {
        let mut view = view();
        view.ballot[0].choice = Some(1);
        assert!(view.decided_all());
    }
}

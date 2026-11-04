use std::time::Instant;

use pepper_sync::sync::ScanRange;
use pepper_sync::wallet::SyncMode;

use super::state::{KeyMaterial, Tip, WalletData, WalletEntry, WalletInfo};
use super::voting::{NetworkRoundRow, PastBallotRow, ProposalInfo};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Esc,
    Backspace,
    Tab,
    BackTab,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    CtrlC,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncNote {
    SessionStarted {
        tip: u32,
        birthday: u32,
        total_blocks: u64,
        scanned_blocks: u64,
        total_outputs: u64,
        scanned_outputs: u64,
    },
    ScanPlanUpdated {
        ranges: Vec<ScanRange>,
    },
    TxDiscovered {
        txid: String,
        confirmed: bool,
    },
    TipMoved {
        to: u32,
    },
    Progress {
        total_blocks: u64,
        scanned_blocks: u64,
        total_outputs: u64,
        scanned_outputs: u64,
    },
    Finished {
        blocks_scanned: u64,
    },
    Failed(String),
    Mode(SyncMode),
    Auto(bool),
    Scheduled {
        at: Instant,
    },
    Reset,
}

#[derive(Clone, Debug)]
pub enum Action {
    Key(Key),
    Paste(String),
    Resize(u16, u16),
    Tick(Instant),
    WalletsListed(Vec<WalletEntry>),
    TipFetched(Result<Tip, String>),
    SeedGenerated(String),
    WalletOpened(WalletInfo),
    OpenFailed(String),
    SpacingMeasured { spacing_secs: u64 },
    DataLoaded(WalletData),
    Sync(SyncNote, Instant),
    Notice(String),
    Copied(bool),
    SendProposed(Result<(u64, u64), String>),
    SendDone(Result<Vec<String>, String>),
    Voting(VotingNote),
    Shutdown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenSpec {
    Read {
        name: String,
    },
    Create {
        name: String,
        material: KeyMaterial,
        birthday: u32,
        server: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    ListWallets,
    FetchTip,
    GenerateSeed,
    Open(OpenSpec),
    Refresh,
    NewUnifiedAddress {
        orchard: bool,
        sapling: bool,
    },
    NewTransparentAddress,
    PauseSync,
    ResumeSync,
    Rescan,
    Copy(String),
    ProposeSend {
        to: String,
        amount: u64,
        memo: Option<String>,
    },
    ConfirmSend,
    CancelSend,
    FetchVotingRound,
    FetchPastBallots,
    FetchRoundTallies(String),
    ArmVoting,
    CastVote(Vec<(u32, Option<u32>)>),
    FetchVotingResults,
    ClearVotingDecision(u32),
    SaveTheme(&'static str),
    Quit,
    ForceQuit,
}

/// One voting event folded into what the reducer needs: either the round
/// summary arriving, a lifecycle log line, or a stage transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VotingNote {
    Round(Result<VotingSummary, String>),
    Event {
        line: String,
        stage: Option<VotingStageUpdate>,
    },
    Cast(Result<VotingOutcome, String>),
    JournalLoaded(Vec<(u32, Option<u32>)>),
    Cleared(u32),
    Results(Result<Vec<(u32, u32)>, String>),
    Past(Result<Vec<PastBallotRow>, String>),
    Network(Result<Vec<NetworkRoundRow>, String>),
    RoundTallies {
        round_id: String,
        tallies: Result<Vec<(u32, String, u64)>, String>,
    },
    /// The resolved voting config source, so the UI can always say which
    /// network the ballots come from.
    Source(String),
}

/// The pieces of `ActiveRoundSummary` the screen shows, plain values only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VotingSummary {
    pub round_id: String,
    pub snapshot_height: u64,
    pub vote_end_time_seconds: u64,
    pub eligible_weight_zatoshi: u64,
    pub bundle_count: u32,
    pub proposals: Vec<ProposalInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VotingStageUpdate {
    Arming,
    Armed,
    Casting,
    Finished,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VotingOutcome {
    pub all_decided: bool,
    pub blocking_recovery: bool,
}

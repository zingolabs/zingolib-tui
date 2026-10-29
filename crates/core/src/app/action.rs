use std::time::Instant;

use pepper_sync::sync::ScanRange;
use pepper_sync::wallet::SyncMode;

use super::state::{KeyMaterial, Tip, WalletData, WalletEntry, WalletInfo};

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
    SpacingMeasured {
        spacing_secs: u64,
    },
    DataLoaded(WalletData),
    Sync(SyncNote, Instant),
    Notice(String),
    Copied(bool),
    SendProposed(Result<(u64, u64), String>),
    SendDone(Result<Vec<String>, String>),
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
    SaveTheme(&'static str),
    Quit,
    ForceQuit,
}

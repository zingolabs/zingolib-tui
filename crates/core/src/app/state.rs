use std::collections::VecDeque;
use std::time::{Duration, Instant};

use pepper_sync::sync::{ScanPriority, ScanRange};
use pepper_sync::wallet::SyncMode;
use zcash_protocol::PoolType;
use zingolib::perspective::value_transfer::{
    SelfSendValueTransfer, SentValueTransfer, ValueTransferKind,
};
use zingolib::wallet::output::SpendStatus;
use zingolib::wallet::summary::data::{SendType, TransactionKind};
use zingo_status::confirmation_status::ConfirmationStatus;

use crate::chain;

pub const NOTICE_TTL: Duration = Duration::from_secs(4);
pub const EVENT_LOG_CAP: usize = 100;
const PROGRESS_WINDOW: usize = 12;

pub fn pool_label(pool: PoolType) -> &'static str {
    match pool {
        PoolType::TRANSPARENT => "transparent",
        PoolType::SAPLING => "sapling",
        PoolType::ORCHARD => "orchard",
        PoolType::IRONWOOD => "ironwood",
    }
}

pub fn tx_kind_label(kind: TransactionKind) -> &'static str {
    match kind {
        TransactionKind::Received => "received",
        TransactionKind::Sent(SendType::Send) => "sent",
        TransactionKind::Sent(SendType::Shield) => "shielded",
        TransactionKind::Sent(SendType::SendToSelf) => "sent to self",
    }
}

pub fn tx_kind_sign(kind: TransactionKind) -> &'static str {
    match kind {
        TransactionKind::Received => "+",
        TransactionKind::Sent(_) => "-",
    }
}

pub fn status_flag(status: ConfirmationStatus) -> &'static str {
    if status.is_confirmed() {
        ""
    } else if status.is_failed() {
        "failed"
    } else {
        "pending"
    }
}

pub fn spend_describe(spend: &SpendStatus) -> String {
    match spend {
        SpendStatus::Unspent => "unspent".into(),
        SpendStatus::Spent(txid) => {
            format!("spent by {}", crate::format::truncate_middle(&txid.to_string(), 18))
        }
        SpendStatus::CalculatedSpent(txid)
        | SpendStatus::TransmittedSpent(txid)
        | SpendStatus::MempoolSpent(txid) => {
            format!(
                "spend pending in {}",
                crate::format::truncate_middle(&txid.to_string(), 18)
            )
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Home,
    Pay,
    Transactions,
    Receive,
    Addresses,
    Sync,
}

impl Screen {
    pub const ALL: [Screen; 6] = [
        Screen::Home,
        Screen::Pay,
        Screen::Transactions,
        Screen::Receive,
        Screen::Addresses,
        Screen::Sync,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Screen::Home => "Home",
            Screen::Pay => "Pay",
            Screen::Transactions => "Transactions",
            Screen::Receive => "Receive",
            Screen::Addresses => "Addresses",
            Screen::Sync => "Sync",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailMode {
    InputsOutputs,
    WalletEvents,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Overlay {
    None,
    Help,
    TxDetail {
        mode: DetailMode,
    },
    RescanConfirm,
    NewUnifiedAddress {
        orchard_only: bool,
    },
    AddressQr,
    ThemePicker {
        original: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendField {
    To,
    Amount,
    Memo,
}

impl SendField {
    pub fn next(self) -> Self {
        match self {
            SendField::To => SendField::Amount,
            SendField::Amount => SendField::Memo,
            SendField::Memo => SendField::To,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            SendField::To => SendField::Memo,
            SendField::Amount => SendField::To,
            SendField::Memo => SendField::Amount,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SendField::To => "to",
            SendField::Amount => "amount",
            SendField::Memo => "memo",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SendStage {
    Edit,
    Proposing,
    Confirm {
        fee: u64,
        total: u64,
    },
    Sending,
    Sent {
        txids: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendForm {
    pub to: String,
    pub amount: String,
    pub memo: String,
    pub field: SendField,
    pub stage: SendStage,
    pub error: Option<String>,
}

impl Default for SendForm {
    fn default() -> Self {
        Self::new()
    }
}

impl SendForm {
    pub fn new() -> Self {
        Self {
            to: String::new(),
            amount: String::new(),
            memo: String::new(),
            field: SendField::To,
            stage: SendStage::Edit,
            error: None,
        }
    }

    pub fn value(&self, field: SendField) -> &str {
        match field {
            SendField::To => &self.to,
            SendField::Amount => &self.amount,
            SendField::Memo => &self.memo,
        }
    }

    pub fn value_mut(&mut self) -> &mut String {
        match self.field {
            SendField::To => &mut self.to,
            SendField::Amount => &mut self.amount,
            SendField::Memo => &mut self.memo,
        }
    }

    pub fn accepts(&self, c: char) -> bool {
        match self.field {
            SendField::To => !c.is_whitespace(),
            SendField::Amount => c.is_ascii_digit() || (c == '.' && !self.amount.contains('.')),
            SendField::Memo => !c.is_control(),
        }
    }

    pub fn amount_zats(&self) -> Result<u64, String> {
        crate::format::parse_zec(self.amount.trim())
    }

    pub fn memo(&self) -> Option<String> {
        let memo = self.memo.trim();
        (!memo.is_empty()).then(|| memo.to_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiveKind {
    Unified,
    Transparent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletKind {
    Seed,
    Watch,
    Unknown,
}

impl WalletKind {
    pub fn name(self) -> &'static str {
        match self {
            WalletKind::Seed => "seed",
            WalletKind::Watch => "watch",
            WalletKind::Unknown => "?",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "seed" => Some(WalletKind::Seed),
            "watch" => Some(WalletKind::Watch),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletEntry {
    pub name: String,
    pub kind: WalletKind,
    pub birthday: Option<u32>,
    pub age_secs: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItem {
    Create,
    Restore,
    Watch,
    Quit,
}

impl MenuItem {
    pub const ALL: [MenuItem; 4] = [
        MenuItem::Create,
        MenuItem::Restore,
        MenuItem::Watch,
        MenuItem::Quit,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MenuItem::Create => "create a new wallet",
            MenuItem::Restore => "restore from a seed phrase",
            MenuItem::Watch => "watch with a viewing key",
            MenuItem::Quit => "quit",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            MenuItem::Create => "24-word seed · spend",
            MenuItem::Restore => "spend",
            MenuItem::Watch => "balances · cannot spend",
            MenuItem::Quit => "",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BirthdayMode {
    FromApp,
    Height,
    Date,
    Unknown,
}

impl BirthdayMode {
    pub const ALL: [BirthdayMode; 4] = [
        BirthdayMode::FromApp,
        BirthdayMode::Height,
        BirthdayMode::Date,
        BirthdayMode::Unknown,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Wallets,
    Menu,
    Create,
    Restore,
    Watch,
    Birthday,
    Name,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyMaterial {
    Seed(String),
    Ufvk(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tip {
    pub height: u32,
    pub at_unix: u64,
    pub spacing_secs: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Onboarding {
    pub step: Step,
    pub wallets: Vec<WalletEntry>,
    pub listed: bool,
    pub wallet_sel: usize,
    pub menu_sel: usize,
    pub tip: Option<Tip>,
    pub tip_error: Option<String>,
    pub tip_pending: bool,
    pub seed: Option<String>,
    pub seed_line: bool,
    pub words: Vec<String>,
    pub draft: String,
    pub key: String,
    pub birthday_mode: Option<BirthdayMode>,
    pub birthday_sel: usize,
    pub birthday_input: String,
    pub birthday_hint: Option<u32>,
    pub birthday: Option<u32>,
    pub material: Option<KeyMaterial>,
    pub name: String,
    pub error: Option<String>,
    pub server: Option<String>,
}

impl Onboarding {
    pub fn new(server: Option<String>) -> Self {
        Self {
            step: Step::Wallets,
            wallets: Vec::new(),
            listed: false,
            wallet_sel: 0,
            menu_sel: 0,
            tip: None,
            tip_error: None,
            tip_pending: false,
            seed: None,
            seed_line: false,
            words: Vec::new(),
            draft: String::new(),
            key: String::new(),
            birthday_mode: None,
            birthday_sel: 1,
            birthday_input: String::new(),
            birthday_hint: None,
            birthday: None,
            material: None,
            name: String::new(),
            error: None,
            server,
        }
    }

    pub fn menu_item(&self) -> MenuItem {
        MenuItem::ALL[self.menu_sel.min(MenuItem::ALL.len() - 1)]
    }

    pub fn is_watch(&self) -> bool {
        matches!(self.material, Some(KeyMaterial::Ufvk(_)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Onboarding(Box<Onboarding>),
    Loading(String),
    Fatal(String),
    Main,
    ShuttingDown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletInfo {
    pub wallet_name: String,
    pub wallet_path: String,
    pub birthday: u32,
    pub server: String,
    pub has_orchard: bool,
    pub kind: WalletKind,
    pub spacing_secs: u64,
}

impl WalletInfo {
    pub fn placeholder(name: impl Into<String>, kind: WalletKind, birthday: u32) -> Self {
        Self {
            wallet_name: name.into(),
            wallet_path: String::new(),
            birthday,
            server: String::new(),
            has_orchard: true,
            kind,
            spacing_secs: chain::BLOCK_SPACING_SECS,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PoolBalance {
    pub confirmed: u64,
    pub pending: u64,
}

impl PoolBalance {
    pub fn total(self) -> u64 {
        self.confirmed + self.pending
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Balance {
    pub ironwood: PoolBalance,
    pub orchard: PoolBalance,
    pub sapling: PoolBalance,
    pub transparent: PoolBalance,
}

impl Balance {
    pub fn total(&self) -> u64 {
        self.pools().iter().map(|(_, b)| b.total()).sum()
    }

    pub fn pending(&self) -> u64 {
        self.pools().iter().map(|(_, b)| b.pending).sum()
    }

    pub fn pools(&self) -> [(PoolType, PoolBalance); 4] {
        [
            (PoolType::IRONWOOD, self.ironwood),
            (PoolType::ORCHARD, self.orchard),
            (PoolType::SAPLING, self.sapling),
            (PoolType::TRANSPARENT, self.transparent),
        ]
    }

    pub fn of(&self, pool: PoolType) -> PoolBalance {
        match pool {
            PoolType::IRONWOOD => self.ironwood,
            PoolType::ORCHARD => self.orchard,
            PoolType::SAPLING => self.sapling,
            PoolType::TRANSPARENT => self.transparent,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputRole {
    Received,
    Sent,
    ToSelf,
    Change,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutputRow {
    pub pool: PoolType,
    pub role: OutputRole,
    pub value: u64,
    pub index: u32,
    pub address: Option<String>,
    pub memo: Option<String>,
    pub spend: SpendStatus,
    pub scope: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputRow {
    pub pool: PoolType,
    pub value: u64,
    pub source_txid: String,
    pub output_index: u32,
    pub memo: Option<String>,
    pub pending: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletEventRow {
    pub kind: ValueTransferKind,
    pub value: u64,
    pub recipient: Option<String>,
    pub pools: Vec<PoolType>,
    pub memos: Vec<String>,
}

impl WalletEventRow {
    pub fn kind_label(&self) -> &'static str {
        match self.kind {
            ValueTransferKind::Received => "received",
            ValueTransferKind::Sent(SentValueTransfer::Send) => "sent",
            ValueTransferKind::Sent(SentValueTransfer::SendToSelf(
                SelfSendValueTransfer::Basic,
            )) => "sent to self",
            ValueTransferKind::Sent(SentValueTransfer::SendToSelf(
                SelfSendValueTransfer::Shield,
            )) => "shield",
            ValueTransferKind::Sent(SentValueTransfer::SendToSelf(
                SelfSendValueTransfer::MemoToSelf,
            )) => "memo to self",
            ValueTransferKind::Sent(SentValueTransfer::SendToSelf(
                SelfSendValueTransfer::Refund,
            )) => "refund",
            ValueTransferKind::Sent(SentValueTransfer::SendToSelf(
                SelfSendValueTransfer::Migration,
            )) => "migration",
        }
    }

    pub fn is_received(&self) -> bool {
        matches!(self.kind, ValueTransferKind::Received)
    }

    pub fn is_sent(&self) -> bool {
        matches!(self.kind, ValueTransferKind::Sent(SentValueTransfer::Send))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TxRow {
    pub txid: String,
    pub datetime: u32,
    pub height: u32,
    pub status: ConfirmationStatus,
    pub kind: TransactionKind,
    pub value: u64,
    pub fee: Option<u64>,
    pub inputs: Vec<InputRow>,
    pub outputs: Vec<OutputRow>,
    pub wallet_events: Vec<WalletEventRow>,
}

impl TxRow {
    pub fn first_memo(&self) -> Option<&str> {
        self.outputs
            .iter()
            .filter_map(|o| o.memo.as_deref())
            .find(|m| !m.is_empty())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddressKind {
    Unified {
        orchard: bool,
        sapling: bool,
        transparent: bool,
    },
    Transparent {
        scope: String,
    },
}

impl AddressKind {
    pub fn is_unified(&self) -> bool {
        matches!(self, AddressKind::Unified { .. })
    }

    pub fn describe(&self) -> String {
        match self {
            AddressKind::Unified {
                orchard,
                sapling,
                transparent,
            } => {
                let mut parts = Vec::new();
                if *orchard {
                    parts.push("orchard");
                }
                if *sapling {
                    parts.push("sapling");
                }
                if *transparent {
                    parts.push("transparent");
                }
                parts.join("+")
            }
            AddressKind::Transparent { scope } => scope.to_lowercase(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddressRow {
    pub kind: AddressKind,
    pub index: u32,
    pub address: String,
    pub received: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WalletData {
    pub balance: Balance,
    pub txs: Vec<TxRow>,
    pub addresses: Vec<AddressRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Scanned,
    Live,
    NotYet,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundTx {
    pub txid: String,
    pub confirmed: bool,
    pub at: Instant,
}

#[derive(Clone, Debug)]
pub struct SyncView {
    pub mode: SyncMode,
    pub auto: bool,
    pub birthday: Option<u32>,
    pub tip: Option<u32>,
    pub chain_ranges: Vec<ScanRange>,
    pub blocks_total: u64,
    pub blocks_scanned: u64,
    pub outputs_total: u64,
    pub outputs_scanned: u64,
    pub txs_found: u64,
    pub sessions_done: u64,
    pub progress_log: VecDeque<(Instant, u64)>,
    pub events: VecDeque<String>,
    pub last_found: Option<FoundTx>,
    pub next_launch: Option<Instant>,
    pub error: Option<String>,
}

impl SyncView {
    pub fn new() -> Self {
        Self {
            mode: SyncMode::NotRunning,
            auto: true,
            birthday: None,
            tip: None,
            chain_ranges: Vec::new(),
            blocks_total: 0,
            blocks_scanned: 0,
            outputs_total: 0,
            outputs_scanned: 0,
            txs_found: 0,
            sessions_done: 0,
            progress_log: VecDeque::with_capacity(PROGRESS_WINDOW + 1),
            events: VecDeque::with_capacity(EVENT_LOG_CAP + 1),
            last_found: None,
            next_launch: None,
            error: None,
        }
    }

    pub fn fraction(&self) -> f64 {
        if self.blocks_total == 0 {
            if self.sessions_done > 0 { 1.0 } else { 0.0 }
        } else {
            (self.blocks_scanned as f64 / self.blocks_total as f64).clamp(0.0, 1.0)
        }
    }

    pub fn set_scan_plan(&mut self, ranges: Vec<ScanRange>) {
        self.chain_ranges = ranges;
    }

    pub fn clear_scan_plan(&mut self) {
        self.chain_ranges.clear();
    }

    pub fn stage(&self, start: u32, end: u32) -> Option<Stage> {
        let overlaps = |a: u32, b: u32| a < end && start < b;
        let mut covered = false;
        let mut scanned = true;
        for range in self
            .chain_ranges
            .iter()
            .filter(|r| {
                let block_range = r.block_range();
                overlaps(u32::from(block_range.start), u32::from(block_range.end))
            })
        {
            covered = true;
            match range.priority() {
                ScanPriority::Scanning | ScanPriority::RefetchingNullifiers => {
                    return Some(Stage::Live);
                }
                ScanPriority::Scanned | ScanPriority::ScannedWithoutMapping => {}
                _ => scanned = false,
            }
        }
        covered.then_some(if scanned {
            Stage::Scanned
        } else {
            Stage::NotYet
        })
    }

    pub fn p_pauses(&self) -> Option<bool> {
        match self.mode {
            SyncMode::Running => Some(true),
            SyncMode::Paused => Some(false),
            SyncMode::NotRunning => Some(self.auto),
            SyncMode::Shutdown => None,
        }
    }

    pub fn begin_session(&mut self) {
        self.clear_scan_plan();
    }

    fn rate(&self) -> Option<f64> {
        let (first_at, first) = self.progress_log.front()?;
        let (last_at, last) = self.progress_log.back()?;
        let span = last_at.duration_since(*first_at).as_secs_f64();
        (span > 0.5 && last > first).then(|| (last - first) as f64 / span)
    }

    pub fn eta_secs(&self) -> Option<u64> {
        let rate = self.rate()?;
        (rate > 0.1 && self.blocks_total > self.blocks_scanned)
            .then(|| ((self.blocks_total - self.blocks_scanned) as f64 / rate) as u64)
    }

    pub fn record_progress(&mut self, at: Instant) {
        self.progress_log.push_back((at, self.blocks_scanned));
        if self.progress_log.len() > PROGRESS_WINDOW {
            self.progress_log.pop_front();
        }
    }

    pub fn reset_progress(&mut self) {
        self.blocks_total = 0;
        self.blocks_scanned = 0;
        self.outputs_total = 0;
        self.outputs_scanned = 0;
        self.birthday = None;
        self.tip = None;
        self.clear_scan_plan();
        self.progress_log.clear();
    }

    pub fn log(&mut self, line: String) {
        self.events.push_back(line);
        if self.events.len() > EVENT_LOG_CAP {
            self.events.pop_front();
        }
    }

    pub fn label(&self) -> String {
        match self.mode {
            SyncMode::Running => format!("syncing {:.0}%", self.fraction() * 100.0),
            SyncMode::Paused => "sync paused".into(),
            SyncMode::Shutdown => "sync stopping".into(),
            SyncMode::NotRunning => {
                if self.error.is_some() {
                    "sync failed".into()
                } else if !self.auto {
                    "sync paused".into()
                } else if self.sessions_done > 0 {
                    "up to date".into()
                } else {
                    "connecting".into()
                }
            }
        }
    }
}

impl Default for SyncView {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct State {
    pub phase: Phase,
    pub screen: Screen,
    pub overlay: Overlay,
    pub send: SendForm,
    pub size: (u16, u16),
    pub ascii_qr: bool,
    pub wallet: Option<WalletInfo>,
    pub data_loaded: bool,
    pub balance: Balance,
    pub txs: Vec<TxRow>,
    pub addresses: Vec<AddressRow>,
    pub tx_selected: usize,
    pub addr_selected: usize,
    pub receive_kind: ReceiveKind,
    pub detail_scroll: u16,
    pub sync: SyncView,
    pub sync_map: bool,
    pub notice: Option<(String, Instant)>,
    pub now: Instant,
    pub ticks: u64,
    pub exit: bool,
    pub quit_requested: bool,
    pub onboarding_backup: Option<Onboarding>,
    pub dither: crate::palette::Dither,
    pub theme: usize,
    pub color_depth: crate::palette::ColorDepth,
}

impl State {
    pub fn new(phase: Phase, now: Instant) -> Self {
        Self {
            phase,
            screen: Screen::Home,
            overlay: Overlay::None,
            send: SendForm::new(),
            size: (80, 24),
            ascii_qr: false,
            wallet: None,
            data_loaded: false,
            balance: Balance::default(),
            txs: Vec::new(),
            addresses: Vec::new(),
            tx_selected: 0,
            addr_selected: 0,
            receive_kind: ReceiveKind::Unified,
            detail_scroll: 0,
            sync: SyncView::new(),
            sync_map: false,
            notice: None,
            now,
            ticks: 0,
            exit: false,
            quit_requested: false,
            onboarding_backup: None,
            dither: crate::palette::Dither::Off,
            theme: crate::palette::choose(None, None, false),
            color_depth: crate::palette::ColorDepth::TrueColor,
        }
    }

    pub fn elapsed_secs(&self) -> f32 {
        self.ticks as f32 * crate::TICK_SECS
    }

    pub fn has_orchard(&self) -> bool {
        self.wallet.as_ref().is_some_and(|w| w.has_orchard)
    }

    pub fn selected_tx(&self) -> Option<&TxRow> {
        self.txs.get(self.tx_selected)
    }

    pub fn selected_address(&self) -> Option<&AddressRow> {
        self.addresses.get(self.addr_selected)
    }

    pub fn can_spend(&self) -> bool {
        self.wallet
            .as_ref()
            .is_some_and(|w| w.kind == WalletKind::Seed)
    }

    pub fn spacing_secs(&self) -> u64 {
        self.wallet
            .as_ref()
            .map(|w| w.spacing_secs)
            .unwrap_or(chain::BLOCK_SPACING_SECS)
    }

    pub fn receive_address(&self) -> Option<&AddressRow> {
        match self.receive_kind {
            ReceiveKind::Unified => self
                .addresses
                .iter()
                .filter(|a| a.kind.is_unified())
                .max_by_key(|a| a.index),
            ReceiveKind::Transparent => self
                .addresses
                .iter()
                .filter(|a| matches!(&a.kind, AddressKind::Transparent { scope } if scope.eq_ignore_ascii_case("external")))
                .max_by_key(|a| a.index),
        }
    }

    pub fn notice_text(&self) -> Option<&str> {
        self.notice
            .as_ref()
            .filter(|(_, at)| self.now.duration_since(*at) < NOTICE_TTL)
            .map(|(m, _)| m.as_str())
    }

    pub fn set_notice(&mut self, msg: impl Into<String>) {
        self.notice = Some((msg.into(), self.now));
    }
}

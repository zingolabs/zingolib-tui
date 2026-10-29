pub mod fake;

use std::collections::HashMap;
use std::collections::HashSet;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pepper_sync::config::{PerformanceLevel, SyncConfig, TransparentAddressDiscovery};
use pepper_sync::sync::sync_status;
use pepper_sync::wallet::{IronwoodNote, OrchardNote, OutputInterface, SaplingNote, TransparentCoin};
use tokio::sync::mpsc;
use zcash_protocol::value::Zatoshis;
use zingolib::wallet::output::SpendStatus;
use zingolib::config::{ChainType, ClientConfig, WalletConfig, default_indexer_for};
use zingolib::data::PollReport;
use zingolib::lightclient::LightClient;
use zingolib::wallet::WalletSettings;
use zingolib::wallet::keys::unified::ReceiverSelection;
use zingolib::wallet::summary::data::TransactionSummary;
use zingo_netutils::Indexer as _;

use crate::chain;
use crate::app::{
    Action, AddressKind, AddressRow, Balance, Command, InputRow, KeyMaterial, OpenSpec,
    OutputRole, OutputRow, PoolBalance, SyncNote, Tip,
    TxRow, WalletData, WalletEntry, WalletEventRow, WalletInfo, WalletKind,
};

const RESYNC_INTERVAL: Duration = Duration::from_secs(20);
const RETRY_INTERVAL: Duration = Duration::from_secs(30);
const TICK: Duration = Duration::from_millis(500);
const STOP_TIMEOUT: Duration = Duration::from_secs(120);
const SAVE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct BackendConfig {
    pub data_dir: Option<PathBuf>,
    pub server: Option<String>,
}

pub fn wallet_file(name: &str) -> String {
    format!("{name}.dat")
}

impl BackendConfig {
    pub fn wallet_dir(&self) -> PathBuf {
        match &self.data_dir {
            Some(dir) => dir.clone(),
            None => default_wallet_dir(),
        }
    }

    pub fn wallet_path(&self, name: &str) -> PathBuf {
        self.wallet_dir().join(wallet_file(name))
    }

    fn server_path(&self, name: &str) -> PathBuf {
        self.wallet_dir().join(format!("{name}.server"))
    }

    fn meta_path(&self, name: &str) -> PathBuf {
        self.wallet_dir().join(format!("{name}.meta"))
    }

    fn saved_server(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.server_path(name))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    fn save_server(&self, name: &str, server: &str) -> std::io::Result<()> {
        std::fs::write(self.server_path(name), format!("{server}\n"))
    }

    fn save_meta(&self, info: &WalletInfo) -> std::io::Result<()> {
        std::fs::write(
            self.meta_path(&info.wallet_name),
            format!(
                "kind = {}\nbirthday = {}\n",
                info.kind.name(),
                info.birthday
            ),
        )
    }

    pub fn meta_kind(&self, name: &str) -> Option<WalletKind> {
        let text = std::fs::read_to_string(self.meta_path(name)).ok()?;
        for line in text.lines() {
            if let Some((key, value)) = line.split_once('=')
                && key.trim() == "kind"
            {
                return WalletKind::parse(value.trim());
            }
        }
        None
    }

    fn server_uri(
        &self,
        name: &str,
        override_server: Option<&str>,
    ) -> Result<http::Uri, String> {
        let raw = override_server
            .map(str::to_string)
            .or_else(|| self.server.clone())
            .or_else(|| self.saved_server(name))
            .unwrap_or_else(default_server);
        parse_server(&raw)
    }

    fn client_config(
        &self,
        name: &str,
        server: http::Uri,
        wallet_config: WalletConfig,
    ) -> Result<ClientConfig, String> {
        ClientConfig::builder()
            .set_chain_type(chain_type())
            .set_indexer_uri(server)
            .set_wallet_dir(self.wallet_dir())
            .set_wallet_name(wallet_file(name))
            .set_wallet_config(wallet_config)
            .build()
            .map_err(|e| e.to_string())
    }

    pub fn list_wallets(&self) -> Vec<WalletEntry> {
        let dir = self.wallet_dir();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path
                .file_name()
                .and_then(|f| f.to_str())
                .and_then(|f| f.strip_suffix(".dat"))
            else {
                continue;
            };
            if !path.is_file() {
                continue;
            }
            let age_secs = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .map(|d| d.as_secs());
            let meta = read_meta(&self.meta_path(name));
            out.push(WalletEntry {
                name: name.to_string(),
                kind: meta
                    .0
                    .unwrap_or_else(|| self.meta_kind(name).unwrap_or(WalletKind::Unknown)),
                birthday: meta.1,
                age_secs,
            });
        }
        out.sort_by_key(|w| (w.age_secs.unwrap_or(u64::MAX), w.name.clone()));
        out
    }
}

fn read_meta(path: &Path) -> (Option<WalletKind>, Option<u32>) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return (None, None);
    };
    let mut kind = None;
    let mut birthday = None;
    for line in text.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match (key.trim(), value.trim()) {
                ("kind", v) => kind = WalletKind::parse(v),
                ("birthday", v) => birthday = v.parse().ok(),
                _ => {}
            }
        }
    }
    (kind, birthday)
}

pub fn default_wallet_dir() -> PathBuf {
    ClientConfig::builder()
        .set_chain_type(chain_type())
        .build()
        .map(|config| config.wallet_dir())
        .unwrap_or_else(|_| {
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Zcash")
        })
}

pub fn default_server() -> String {
    default_indexer_for(&chain_type())
        .map(|uri| uri.to_string().trim_end_matches('/').to_string())
        .unwrap_or_else(|| "http://127.0.0.1:9067".to_string())
}

fn parse_server(raw: &str) -> Result<http::Uri, String> {
    let with_scheme = if raw.starts_with("http") {
        raw.to_string()
    } else {
        format!("http://{raw}")
    };
    let uri: http::Uri = with_scheme
        .parse()
        .map_err(|e| format!("invalid server URL {raw}: {e}"))?;
    if uri.port().is_none() && uri.scheme_str() == Some("http") {
        format!("{with_scheme}:9067")
            .parse()
            .map_err(|e| format!("invalid server URL {raw}: {e}"))
    } else {
        Ok(uri)
    }
}

pub fn chain_type() -> ChainType {
    ChainType::Mainnet
}

fn wallet_settings() -> WalletSettings {
    WalletSettings {
        sync_config: SyncConfig {
            transparent_address_discovery: TransparentAddressDiscovery::minimal(),
            performance_level: PerformanceLevel::High,
            shutdown_on_completion: false,
        },
        min_confirmations: NonZeroU32::new(3).expect("non-zero"),
    }
}

pub fn spawn(
    config: BackendConfig,
    commands: mpsc::UnboundedReceiver<Command>,
    actions: mpsc::UnboundedSender<Action>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(Backend::new(config, actions).run(commands))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SyncSnapshot {
    started: bool,
    birthday: Option<u32>,
    tip: Option<u32>,
    ranges: Vec<pepper_sync::sync::ScanRange>,
    total_blocks: u64,
    scanned_blocks: u64,
    total_outputs: u64,
    scanned_outputs: u64,
    txs: Vec<(String, bool)>,
}

struct Backend {
    config: BackendConfig,
    actions: mpsc::UnboundedSender<Action>,
    lc: Option<LightClient>,
    wallet_name: String,
    auto: bool,
    next_launch: Option<Instant>,
    dirty: bool,
    last_mode: pepper_sync::wallet::SyncMode,
    session_pending: bool,
    known_txids: HashSet<String>,
    last: SyncSnapshot,
}

impl Backend {
    fn new(config: BackendConfig, actions: mpsc::UnboundedSender<Action>) -> Self {
        Self {
            config,
            actions,
            lc: None,
            wallet_name: String::new(),
            auto: true,
            next_launch: None,
            dirty: false,
            last_mode: pepper_sync::wallet::SyncMode::NotRunning,
            session_pending: false,
            known_txids: HashSet::new(),
            last: SyncSnapshot::default(),
        }
    }

    fn send(&self, action: Action) {
        let _ = self.actions.send(action);
    }

    fn sync_note(&self, note: SyncNote) {
        self.send(Action::Sync(note, Instant::now()));
    }

    async fn run(mut self, mut commands: mpsc::UnboundedReceiver<Command>) {
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                cmd = commands.recv() => match cmd {
                    None => break,
                    Some(cmd) => {
                        if self.handle(cmd).await {
                            break;
                        }
                    }
                },
                _ = tick.tick() => self.on_tick().await,
            }
        }
    }

    async fn handle(&mut self, cmd: Command) -> bool {
        match cmd {
            Command::ListWallets => {
                let config = self.config.clone();
                let wallets = tokio::task::spawn_blocking(move || config.list_wallets())
                    .await
                    .unwrap_or_default();
                self.send(Action::WalletsListed(wallets));
            }
            Command::FetchTip => {
                let actions = self.actions.clone();
                let server = self.config.server_uri("", None);
                tokio::spawn(async move {
                    let result = match server {
                        Ok(uri) => fetch_tip(uri).await,
                        Err(e) => Err(e),
                    };
                    let _ = actions.send(Action::TipFetched(result));
                });
            }
            Command::GenerateSeed => {
                let phrase =
                    bip0039::Mnemonic::<bip0039::English>::generate(bip0039::Count::Words24)
                        .into_phrase();
                self.send(Action::SeedGenerated(phrase));
            }
            Command::Open(spec) => {
                if self.lc.is_some() {
                    return false;
                }
                match self.open(spec).await {
                    Ok((lc, info)) => {
                        self.lc = Some(lc);
                        self.send(Action::WalletOpened(info));
                        if let Some(lc) = self.lc.as_mut() {
                            lc.save_task().await;
                        }
                        self.refresh().await;
                        self.launch().await;
                        let actions = self.actions.clone();
                        let server = self.lc.as_ref().and_then(|lc| lc.indexer_uri());
                        tokio::spawn(async move {
                            if let Some(uri) = server
                                && let Ok(tip) = fetch_tip(uri).await
                            {
                                let _ = actions.send(Action::SpacingMeasured {
                                    spacing_secs: tip.spacing_secs,
                                });
                            }
                        });
                    }
                    Err(message) => self.send(Action::OpenFailed(message)),
                }
            }
            Command::Refresh => self.refresh().await,
            Command::NewUnifiedAddress { orchard, sapling } => {
                if let Some(lc) = self.lc.as_mut() {
                    match lc
                        .generate_unified_address(
                            ReceiverSelection { orchard, sapling },
                            zip32::AccountId::ZERO,
                        )
                        .await
                    {
                        Ok((id, _)) => {
                            self.send(Action::Notice(format!(
                                "new unified address #{}",
                                id.address_index
                            )));
                            self.refresh().await;
                        }
                        Err(e) => self.send(Action::Notice(format!("cannot derive address: {e}"))),
                    }
                }
            }
            Command::NewTransparentAddress => {
                if let Some(lc) = self.lc.as_mut() {
                    match lc
                        .generate_transparent_address(zip32::AccountId::ZERO, false)
                        .await
                    {
                        Ok((id, _)) => {
                            self.send(Action::Notice(format!(
                                "new transparent address #{}",
                                id.address_index().index()
                            )));
                            self.refresh().await;
                        }
                        Err(e) => self.send(Action::Notice(format!("cannot derive address: {e}"))),
                    }
                }
            }
            Command::PauseSync => {
                if let Some(lc) = self.lc.as_ref() {
                    match lc.sync_mode() {
                        pepper_sync::wallet::SyncMode::Running => {
                            if let Err(e) = lc.pause_sync() {
                                self.send(Action::Notice(format!("cannot pause: {e}")));
                            }
                        }
                        pepper_sync::wallet::SyncMode::NotRunning => {
                            self.auto = false;
                            self.next_launch = None;
                            self.sync_note(SyncNote::Auto(false));
                        }
                        _ => {}
                    }
                }
            }
            Command::ResumeSync => {
                if let Some(lc) = self.lc.as_ref() {
                    match lc.sync_mode() {
                        pepper_sync::wallet::SyncMode::Paused => {
                            if let Err(e) = lc.resume_sync() {
                                self.send(Action::Notice(format!("cannot resume: {e}")));
                            }
                        }
                        pepper_sync::wallet::SyncMode::NotRunning => {
                            self.auto = true;
                            self.sync_note(SyncNote::Auto(true));
                            self.launch().await;
                        }
                        _ => {}
                    }
                }
            }
            Command::Rescan => {
                if self.lc.is_none() {
                    return false;
                }
                self.sync_note(SyncNote::Reset);
                self.auto = true;
                self.next_launch = None;
                self.known_txids.clear();
                self.last = SyncSnapshot::default();
                self.sync_note(SyncNote::Auto(true));
                let rescanned = self.lc.as_mut().expect("checked above").rescan().await;
                match rescanned {
                    Ok(()) => {
                        self.session_pending = true;
                        self.dirty = true;
                    }
                    Err(e) => {
                        self.sync_note(SyncNote::Failed(e.to_string()));
                        let at = Instant::now() + RETRY_INTERVAL;
                        self.next_launch = Some(at);
                        self.sync_note(SyncNote::Scheduled { at });
                    }
                }
                self.refresh().await;
            }
            Command::Quit => {
                self.shutdown().await;
                self.send(Action::Shutdown);
                return true;
            }
            Command::ProposeSend { to, amount, memo } => {
                let result = match self.lc.as_mut() {
                    Some(lc) => propose_send(lc, &to, amount, memo.as_deref()).await,
                    None => Err("no wallet is open".into()),
                };
                self.send(Action::SendProposed(result));
            }
            Command::ConfirmSend => {
                let result = match self.lc.as_mut() {
                    Some(lc) => lc
                        .send_stored_proposal(true)
                        .await
                        .map(|txids| txids.iter().map(|id| id.to_string()).collect())
                        .map_err(|e| e.to_string()),
                    None => Err("no wallet is open".into()),
                };
                self.send(Action::SendDone(result));
                self.refresh().await
            }
            Command::CancelSend => {
                if let Some(lc) = self.lc.as_mut() {
                    lc.clear_proposal().await;
                }
            }
            Command::Copy(_) | Command::SaveTheme(_) | Command::ForceQuit => {}
        }
        false
    }

    async fn open(&mut self, spec: OpenSpec) -> Result<(LightClient, WalletInfo), String> {
        let (lc, name, kind, birthday) = match spec {
            OpenSpec::Read { name } => {
                let server = self.config.server_uri(&name, None)?;
                let wallet_config = WalletConfig::Read;
                let config = self.config.client_config(&name, server, wallet_config)?;
                let lc = LightClient::new(config, false)
                    .await
                    .map_err(|e| format!("cannot open wallet: {e}"))?;
                let birthday = lc.birthday();
                let kind = {
                    let wallet = lc.wallet().read().await;
                    wallet_kind(&wallet)
                };
                (lc, name, kind, birthday)
            }
            OpenSpec::Create {
                name,
                material,
                birthday,
                server,
            } => {
                let path = self.config.wallet_path(&name);
                if path.exists() {
                    return Err(format!(
                        "a wallet file already exists at {}",
                        path.display()
                    ));
                }
                let server_uri = self.config.server_uri(&name, server.as_deref())?;
                let birthday = birthday.max(chain::sapling_activation());
                let wallet_config = match &material {
                    KeyMaterial::Ufvk(ufvk) => WalletConfig::Ufvk {
                        ufvk: ufvk.clone(),
                        birthday,
                        wallet_settings: wallet_settings(),
                    },
                    KeyMaterial::Seed(phrase) => WalletConfig::MnemonicPhrase {
                        mnemonic_phrase: phrase.clone(),
                        no_of_accounts: NonZeroU32::new(1).expect("non-zero"),
                        birthday,
                        wallet_settings: wallet_settings(),
                    },
                };
                let config =
                    self.config
                        .client_config(&name, server_uri.clone(), wallet_config)?;
                if let Some(server) = &server
                    && let Err(e) = self.config.save_server(&name, server)
                {
                    tracing::warn!("cannot save the server beside the wallet: {e}");
                }
                let lc = LightClient::new(config, false)
                    .await
                    .map_err(|e| format!("cannot create wallet: {e}"))?;
                let kind = match material {
                    KeyMaterial::Seed(_) => WalletKind::Seed,
                    KeyMaterial::Ufvk(_) => WalletKind::Watch,
                };
                (lc, name, kind, birthday)
            }
        };
        let has_orchard = {
            let wallet = lc.wallet().read().await;
            wallet
                .unified_key_store
                .get(&zip32::AccountId::ZERO)
                .and_then(|keys| keys.default_receivers())
                .is_some_and(|r| r.orchard)
        };
        self.wallet_name = name.clone();
        self.known_txids = {
            let wallet = lc.wallet().read().await;
            wallet
                .wallet_transactions
                .keys()
                .map(ToString::to_string)
                .collect::<HashSet<_>>()
        };
        self.last = SyncSnapshot::default();
        let info = WalletInfo {
            wallet_name: name,
            wallet_path: lc.wallet_path().display().to_string(),
            birthday,
            server: lc
                .indexer_uri()
                .map(|uri| uri.to_string())
                .unwrap_or_default(),
            has_orchard,
            kind,
            spacing_secs: chain::BLOCK_SPACING_SECS,
        };
        if let Err(e) = self.config.save_meta(&info) {
            tracing::warn!("cannot write the wallet's meta file: {e}");
        }
        Ok((lc, info))
    }

    async fn launch(&mut self) {
        if self.lc.is_none() {
            return;
        }
        self.next_launch = None;
        let launched = self.lc.as_mut().expect("checked above").sync().await;
        match launched {
            Ok(()) => {
                tracing::info!("sync session launched");
                self.session_pending = true;
            }
            Err(e) => {
                tracing::warn!("sync could not start: {e}");
                self.sync_note(SyncNote::Failed(e.to_string()));
                if self.auto {
                    let at = Instant::now() + RETRY_INTERVAL;
                    self.next_launch = Some(at);
                    self.sync_note(SyncNote::Scheduled { at });
                }
            }
        }
    }

    async fn on_tick(&mut self) {
        let Some(poll) = self.lc.as_mut().map(LightClient::poll_sync) else {
            return;
        };
        match poll {
            PollReport::Ready(Ok(result)) => {
                tracing::info!("sync session finished: {result:?}");
                self.observe().await;
                self.session_pending = false;
                self.sync_note(SyncNote::Finished {
                    blocks_scanned: u64::from(result.blocks_scanned),
                });
                self.dirty = true;
                if self.auto {
                    let at = Instant::now() + RESYNC_INTERVAL;
                    self.next_launch = Some(at);
                    self.sync_note(SyncNote::Scheduled { at });
                }
            }
            PollReport::Ready(Err(e)) => {
                tracing::warn!("sync session failed: {e}");
                self.session_pending = false;
                self.sync_note(SyncNote::Failed(e.to_string()));
                self.dirty = true;
                if self.auto {
                    let at = Instant::now() + RETRY_INTERVAL;
                    self.next_launch = Some(at);
                    self.sync_note(SyncNote::Scheduled { at });
                }
            }
            PollReport::NotReady => self.observe().await,
            PollReport::NoHandle | PollReport::Panicked(_) => {
                if self.auto && self.next_launch.is_some_and(|at| at <= Instant::now()) {
                    self.launch().await;
                }
            }
        }
        if let Some(mode) = self.lc.as_ref().map(|lc| lc.sync_mode())
            && mode != self.last_mode
        {
            self.last_mode = mode;
            self.sync_note(SyncNote::Mode(mode));
        }
        let save = match self.lc.as_mut() {
            Some(lc) => lc.check_save_error().await,
            None => Ok(()),
        };
        if let Err(e) = save {
            self.send(Action::Notice(format!("wallet save failed: {e}")));
        }
        if self.dirty {
            self.dirty = false;
            self.refresh().await;
        }
    }

    async fn observe(&mut self) {
        let Some(lc) = self.lc.as_ref() else { return };
        let now = snapshot(lc).await;
        if self.session_pending && now.started && now.tip.is_some() {
            self.session_pending = false;
            self.sync_note(SyncNote::SessionStarted {
                tip: now.tip.unwrap_or(0),
                birthday: now.birthday.unwrap_or(0),
                total_blocks: now.total_blocks,
                scanned_blocks: now.scanned_blocks,
                total_outputs: now.total_outputs,
                scanned_outputs: now.scanned_outputs,
            });
            self.last.ranges.clear();
            self.last.tip = now.tip;
        }
        if now.ranges != self.last.ranges {
            self.sync_note(SyncNote::ScanPlanUpdated {
                ranges: now.ranges.clone(),
            });
        }
        if let Some(to) = now.tip
            && self.last.tip.is_some_and(|last| last != to)
        {
            self.dirty = true;
            self.sync_note(SyncNote::TipMoved { to });
        }
        let progress = |s: &SyncSnapshot| {
            (
                s.total_blocks,
                s.scanned_blocks,
                s.total_outputs,
                s.scanned_outputs,
            )
        };
        if now.started && progress(&now) != progress(&self.last) {
            self.sync_note(SyncNote::Progress {
                total_blocks: now.total_blocks,
                scanned_blocks: now.scanned_blocks,
                total_outputs: now.total_outputs,
                scanned_outputs: now.scanned_outputs,
            });
        }
        for (txid, confirmed) in &now.txs {
            if self.known_txids.insert(txid.clone()) {
                self.dirty = true;
                self.sync_note(SyncNote::TxDiscovered {
                    txid: txid.clone(),
                    confirmed: *confirmed,
                });
            }
        }
        self.last = now;
    }

    async fn refresh(&mut self) {
        let Some(lc) = self.lc.as_ref() else { return };
        match load(lc).await {
            Ok(data) => self.send(Action::DataLoaded(data)),
            Err(e) => self.send(Action::Notice(format!("cannot read wallet: {e}"))),
        }
    }

    async fn shutdown(&mut self) {
        self.auto = false;
        let Some(lc) = self.lc.as_mut() else { return };
        if lc.sync_mode() != pepper_sync::wallet::SyncMode::NotRunning {
            let _ = lc.stop_sync();
            let deadline = Instant::now() + STOP_TIMEOUT;
            while matches!(lc.poll_sync(), PollReport::NotReady) && Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
        let _ = tokio::time::timeout(SAVE_TIMEOUT, lc.wait_for_save()).await;
        if let Err(e) = lc.shutdown_save_task().await {
            tracing::error!("final wallet save failed: {e}");
        }
    }
}

fn wallet_kind(wallet: &zingolib::wallet::LightWallet) -> WalletKind {
    match wallet
        .unified_key_store
        .get(&zip32::AccountId::ZERO)
        .is_some_and(|keys| keys.is_spending_key())
    {
        true => WalletKind::Seed,
        false => WalletKind::Watch,
    }
}

async fn fetch_tip(server: http::Uri) -> Result<Tip, String> {
    let mut indexer = zingo_netutils::GrpcIndexer::new(server.clone())
        .await
        .map_err(|e| format!("cannot connect to {server}: {e}"))?;
    let info = indexer
        .get_lightd_info(zingolib::lightclient::DEFAULT_REQUEST_TIMEOUT)
        .await
        .map_err(|e| format!("{server} did not answer: {e}"))?;
    let height =
        u32::try_from(info.block_height).map_err(|_| "absurd tip height".to_string())?;
    let at_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut spacing_secs = chain::BLOCK_SPACING_SECS;
    let back = height.saturating_sub(1000).max(1);
    if back < height {
        let block = |h: u32| zingo_netutils::lightwallet_protocol::BlockId {
            height: u64::from(h),
            hash: Vec::new(),
        };
        let mut client = indexer.get_clear_net_client().await;
        if let (Ok(newest), Ok(oldest)) = (
            client.get_block(block(height)).await,
            client.get_block(block(back)).await,
        ) {
            let (newest, oldest) = (newest.into_inner(), oldest.into_inner());
            let seconds = u64::from(newest.time.saturating_sub(oldest.time));
            let blocks = u64::from(height - back);
            if seconds > 0 && blocks > 0 {
                spacing_secs = (seconds / blocks).clamp(1, 3600);
            }
        }
    }
    Ok(Tip {
        height,
        at_unix,
        spacing_secs,
    })
}

async fn snapshot(lc: &LightClient) -> SyncSnapshot {
    let wallet = lc.wallet().read().await;
    let status: Option<pepper_sync::sync::SyncStatus> = sync_status(&*wallet).await.ok();
    let txs = wallet
        .wallet_transactions
        .iter()
        .map(|(txid, tx)| (txid.to_string(), tx.status().is_confirmed()))
        .collect();
    let birthday = wallet.sync_state.wallet_birthday().map(u32::from);
    let tip = wallet.sync_state.last_known_chain_height().map(u32::from);
    let mut out = SyncSnapshot {
        birthday,
        tip,
        txs,
        ..SyncSnapshot::default()
    };
    if let Some(status) = status {
        let scanned_outputs = status.total_outputs_scanned;
        let scanned_blocks = u64::from(status.total_blocks_scanned);
        let total_blocks = u64::from(status.total_blocks);
        out.started = u32::from(status.sync_start_height) != 0;
        out.ranges = status.scan_ranges.clone();
        out.total_blocks = if total_blocks > 0 {
            total_blocks
        } else {
            match (birthday, tip) {
                (Some(b), Some(t)) if t >= b => u64::from(t - b) + 1,
                _ => 0,
            }
        };
        out.total_blocks = out.total_blocks.max(scanned_blocks);
        out.scanned_blocks = scanned_blocks;
        out.total_outputs = status.total_outputs;
        out.scanned_outputs = scanned_outputs;
    }
    out
}

fn pool_balance<Op: OutputInterface>(
    wallet: &zingolib::wallet::LightWallet,
    account: zip32::AccountId,
) -> PoolBalance {
    fn zats(balance: Result<Zatoshis, zingolib::wallet::error::BalanceError>) -> u64 {
        balance.map(Zatoshis::into_u64).unwrap_or(0)
    }
    PoolBalance {
        confirmed: zats(wallet.confirmed_balance_excluding_dust::<Op>(account)),
        pending: zats(wallet.unconfirmed_balance_excluding_dust::<Op>(account)),
    }
}

async fn load(lc: &LightClient) -> Result<WalletData, String> {
    let account = zip32::AccountId::ZERO;
    let balance = {
        let wallet = lc.wallet().read().await;
        Balance {
            ironwood: pool_balance::<IronwoodNote>(&wallet, account),
            orchard: pool_balance::<OrchardNote>(&wallet, account),
            sapling: pool_balance::<SaplingNote>(&wallet, account),
            transparent: pool_balance::<TransparentCoin>(&wallet, account),
        }
    };

    let summaries = lc
        .transaction_summaries(false)
        .await
        .map_err(|e| e.to_string())?;
    let transfers = lc
        .value_transfers(false)
        .await
        .map_err(|e| e.to_string())?;
    let mut by_txid: HashMap<String, Vec<WalletEventRow>> = HashMap::new();
    for transfer in transfers.iter() {
        by_txid
            .entry(transfer.txid.to_string())
            .or_default()
            .push(wallet_event_row(transfer));
    }
    let mut index = output_index(lc).await;
    let mut txs: Vec<TxRow> = summaries
        .0
        .iter()
        .map(|s| tx_row(s, &mut by_txid, &mut index))
        .collect();
    txs.sort_by(|a, b| b.height.cmp(&a.height).then(b.datetime.cmp(&a.datetime)));

    let received = lc
        .do_total_value_to_address()
        .await
        .map(|t| t.0)
        .unwrap_or_default();
    let mut addresses = Vec::new();
    let wallet = lc.wallet().read().await;
    let chain_type = wallet.chain_type();
    for (id, unified_address) in wallet.unified_addresses() {
        let address = unified_address.encode(&chain_type);
        addresses.push(AddressRow {
            kind: AddressKind::Unified {
                orchard: unified_address.has_orchard(),
                sapling: unified_address.has_sapling(),
                transparent: unified_address.has_transparent(),
            },
            index: id.address_index,
            received: received.get(&address).copied().unwrap_or(0),
            address,
        });
    }
    for (id, address) in wallet
        .transparent_addresses()
        .iter()
        .filter(|(id, _)| id.scope() == pepper_sync::keys::transparent::TransparentScope::External)
    {
        addresses.push(AddressRow {
            kind: AddressKind::Transparent {
                scope: id.scope().to_string(),
            },
            index: id.address_index().index(),
            received: received.get(address).copied().unwrap_or(0),
            address: address.clone(),
        });
    }
    drop(wallet);

    Ok(WalletData {
        balance,
        txs,
        addresses,
    })
}

#[derive(Default)]
struct OutputIndex {
    spent_by: HashMap<String, Vec<InputRow>>,
    scopes: HashMap<(String, PoolTypeKey, u32), String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum PoolTypeKey {
    Ironwood,
    Orchard,
    Sapling,
    Transparent,
}

impl From<PoolTypeKey> for zcash_protocol::PoolType {
    fn from(key: PoolTypeKey) -> Self {
        match key {
            PoolTypeKey::Ironwood => zcash_protocol::PoolType::IRONWOOD,
            PoolTypeKey::Orchard => zcash_protocol::PoolType::ORCHARD,
            PoolTypeKey::Sapling => zcash_protocol::PoolType::SAPLING,
            PoolTypeKey::Transparent => zcash_protocol::PoolType::TRANSPARENT,
        }
    }
}

async fn output_index(lc: &LightClient) -> OutputIndex {
    let wallet = lc.wallet().read().await;
    let mut index = OutputIndex::default();

    let pools = [
        (
            PoolTypeKey::Ironwood,
            wallet.note_summaries::<IronwoodNote>(true),
        ),
        (
            PoolTypeKey::Orchard,
            wallet.note_summaries::<OrchardNote>(true),
        ),
        (
            PoolTypeKey::Sapling,
            wallet.note_summaries::<SaplingNote>(true),
        ),
    ];
    for (pool, summaries) in pools {
        for note in summaries.iter() {
            let source_txid = note.txid.to_string();
            let output_index = note.output_index;
            index.scopes.insert(
                (source_txid.clone(), pool, output_index),
                note.scope.to_string(),
            );
            if let Some((spender, pending)) = spend_target(&note.spend_status) {
                index
                    .spent_by
                    .entry(spender)
                    .or_default()
                    .push(InputRow {
                        pool: pool.into(),
                        value: note.value,
                        source_txid,
                        output_index,
                        memo: note.memo.clone(),
                        pending,
                    });
            }
        }
    }
    for coin in wallet.coin_summaries(true) {
        let source_txid = coin.txid.to_string();
        let output_index = coin.output_index;
        index.scopes.insert(
            (source_txid.clone(), PoolTypeKey::Transparent, output_index),
            coin.scope.to_string(),
        );
        if let Some((spender, pending)) = spend_target(&coin.spend_status) {
            index
                .spent_by
                .entry(spender)
                .or_default()
                .push(InputRow {
                    pool: PoolTypeKey::Transparent.into(),
                    value: coin.value,
                    source_txid,
                    output_index,
                    memo: None,
                    pending,
                });
        }
    }
    index
}

fn spend_target(status: &SpendStatus) -> Option<(String, bool)> {
    match status {
        SpendStatus::Unspent => None,
        SpendStatus::Spent(txid) => Some((txid.to_string(), false)),
        SpendStatus::CalculatedSpent(txid)
        | SpendStatus::TransmittedSpent(txid)
        | SpendStatus::MempoolSpent(txid) => Some((txid.to_string(), true)),
    }
}

fn tx_row(
    s: &TransactionSummary,
    transfers: &mut HashMap<String, Vec<WalletEventRow>>,
    index: &mut OutputIndex,
) -> TxRow {
    let txid = s.txid.to_string();
    let status = s.status;

    let scope_of = |pool: PoolTypeKey, output_index: u32| {
        index
            .scopes
            .get(&(txid.clone(), pool, output_index))
            .cloned()
    };
    let mut received = Vec::new();
    let notes = [
        (PoolTypeKey::Ironwood, &s.ironwood_notes),
        (PoolTypeKey::Orchard, &s.orchard_notes),
        (PoolTypeKey::Sapling, &s.sapling_notes),
    ];
    for (pool, notes) in notes {
        for n in notes {
            received.push(OutputRow {
                pool: pool.into(),
                role: OutputRole::Received,
                value: n.value,
                index: n.output_index,
                address: None,
                memo: n.memo.clone(),
                spend: n.spend_status,
                scope: scope_of(pool, n.output_index),
            });
        }
    }
    for c in &s.transparent_coins {
        received.push(OutputRow {
            pool: PoolTypeKey::Transparent.into(),
            role: OutputRole::Received,
            value: c.value,
            index: c.output_index,
            address: None,
            memo: None,
            spend: c.spend_summary,
            scope: scope_of(PoolTypeKey::Transparent, c.output_index),
        });
    }

    let mut outgoing = Vec::new();
    let notes = [
        (PoolTypeKey::Ironwood, &s.outgoing_ironwood_notes),
        (PoolTypeKey::Orchard, &s.outgoing_orchard_notes),
        (PoolTypeKey::Sapling, &s.outgoing_sapling_notes),
    ];
    for (pool, notes) in notes {
        for n in notes {
            outgoing.push(OutputRow {
                pool: pool.into(),
                role: OutputRole::Sent,
                value: n.value,
                index: n.output_index,
                address: Some(
                    n.recipient_unified_address
                        .clone()
                        .unwrap_or_else(|| n.recipient.clone()),
                ),
                memo: n.memo.clone(),
                spend: SpendStatus::Unspent,
                scope: Some(n.scope.to_string()),
            });
        }
    }
    for c in &s.outgoing_transparent_coins {
        outgoing.push(OutputRow {
            pool: PoolTypeKey::Transparent.into(),
            role: OutputRole::Sent,
            value: c.value,
            index: c.output_index,
            address: Some(c.recipient.clone()),
            memo: None,
            spend: SpendStatus::Unspent,
            scope: None,
        });
    }

    let mut inputs = index.spent_by.remove(&txid).unwrap_or_default();
    inputs.sort_by(|a, b| {
        a.source_txid
            .cmp(&b.source_txid)
            .then(a.output_index.cmp(&b.output_index))
    });
    let outputs = crate::app::roles::assign_roles(s.kind, received, outgoing);

    TxRow {
        wallet_events: transfers.remove(&txid).unwrap_or_default(),
        txid,
        datetime: s.datetime,
        height: u32::from(s.blockheight),
        status,
        kind: s.kind,
        value: s.value,
        fee: s.fee,
        inputs,
        outputs,
    }
}

fn wallet_event_row(
    transfer: &zingolib::perspective::value_transfer::ValueTransfer,
) -> WalletEventRow {
    WalletEventRow {
        kind: transfer.kind,
        value: transfer.value,
        recipient: transfer.recipient_address.clone(),
        pools: transfer.pools_received.clone(),
        memos: transfer.memos.clone(),
    }
}

async fn propose_send(
    lc: &mut LightClient,
    to: &str,
    amount: u64,
    memo: Option<&str>,
) -> Result<(u64, u64), String> {
    use std::str::FromStr as _;

    use zcash_protocol::memo::Memo;
    use zingolib::utils::conversion::address_from_str;
    use zip321::{Payment, TransactionRequest};

    let recipient_address =
        address_from_str(to).map_err(|_| "that is not a zcash address".to_string())?;
    let amount_zats = Zatoshis::from_u64(amount).map_err(|_| "the amount is out of range")?;
    let memo_bytes = match memo {
        Some(text) => Some(
            Memo::from_str(text)
                .map_err(|e| format!("the memo cannot be used: {e}"))?
                .encode(),
        ),
        None => None,
    };
    let payment = Payment::new(
        recipient_address,
        Some(amount_zats),
        memo_bytes,
        None,
        None,
        Vec::new(),
    )
    .map_err(|e| e.to_string())?;
    let request =
        TransactionRequest::new(vec![payment]).map_err(|e| format!("that payment fails: {e}"))?;
    let proposal = lc
        .propose_send(request, zip32::AccountId::ZERO)
        .await
        .map_err(|e| e.to_string())?;
    let fee = zingolib::data::proposal::total_fee(&proposal)
        .map(Zatoshis::into_u64)
        .map_err(|e| e.to_string())?;
    Ok((fee, amount.saturating_add(fee)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_urls_are_completed_but_never_panic() {
        assert_eq!(
            parse_server("127.0.0.1").unwrap().to_string(),
            "http://127.0.0.1:9067/"
        );
        assert_eq!(
            parse_server("https://mainnet.zainod.zingolabs.dev:443")
                .unwrap()
                .to_string(),
            "https://mainnet.zainod.zingolabs.dev:443/"
        );
        assert!(
            parse_server("https://example.com")
                .unwrap()
                .to_string()
                .ends_with("example.com/"),
            "https keeps its default port"
        );
        assert!(parse_server("http://[bad").is_err());
    }

    #[test]
    fn the_default_server_is_the_builtin_mainnet_indexer() {
        let uri = parse_server(&default_server()).unwrap();
        assert_eq!(uri.scheme_str(), Some("https"));
    }
}

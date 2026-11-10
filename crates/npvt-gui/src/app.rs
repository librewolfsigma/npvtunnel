//! Application state and logic for the NPVTunnel desktop client.
//!
//! All privileged work goes through `npvt-core`: the vault (atomic writes,
//! zeroized credentials), the .npvt v2 container crypto and the sing-box
//! sidecar management. The GUI never displays credentials unless the user
//! explicitly edits a field, and dialog passphrases are scrubbed on close.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use npvt_core::engine::{self, PidFile, DEFAULT_SOCKS_PORT};
use npvt_core::npvt::v1;
use npvt_core::npvt::v2::{self, Lock, Policy, Unlock};
use npvt_core::profile::{parse_expires, unix_now, Profile};
use npvt_core::uri::{parse_import, ImportSource};
use npvt_core::vault::{
    clean_name_input, find_profile, generate_identity, remove_profile, short_suffix,
    upsert_profile, Identity, Vault, VaultData,
};
use npvt_core::{Error, Result};
use zeroize::Zeroize;

/// Maximum accepted container file size on import.
const MAX_IMPORT_FILE: usize = v2::MAX_CONTAINER_LEN;

/// How long a freshly spawned engine gets before the first liveness probe.
const STARTUP_GRACE: Duration = Duration::from_millis(700);

/// How often a running engine is re-probed.
const LIVENESS_INTERVAL: Duration = Duration::from_secs(2);

/// Cap for the in-app activity log.
const LOG_CAP: usize = 400;

/// Which main view is active.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Dashboard,
    Profiles,
    Settings,
}

/// Liveness of the sing-box sidecar, as seen by this app.
#[derive(Clone)]
pub(crate) enum EngineState {
    Stopped,
    Starting {
        pid: u32,
        since: Instant,
        name: String,
        port: u16,
    },
    Running {
        pid: u32,
        name: String,
        port: u16,
        started_unix: u64,
    },
    Failed {
        message: String,
    },
}

/// One decoded incoming container/link, ready for the security card view.
#[derive(Clone)]
pub(crate) struct Card {
    pub profile: Profile,
    pub expires_at: Option<u64>,
    pub metered_only: bool,
    pub signed: bool,
    pub creator: Option<String>,
    pub lock_kind: String,
}

/// The staged import flow (file or link).
pub(crate) struct ImportState {
    pub source_label: String,
    pub bytes: Option<Vec<u8>>,
    pub lock_kind: String,
    pub passphrase: String,
    pub card: Option<Card>,
    pub name_override: String,
    pub group: String,
    pub error: Option<String>,
}

/// The staged export flow for one profile.
pub(crate) struct ExportState {
    pub target: String,
    pub passphrase: String,
    pub repeat: String,
    pub expires: String,
    pub out: Option<PathBuf>,
    pub error: Option<String>,
}

/// Root application state.
pub(crate) struct NpvtApp {
    pub vault: Option<Vault>,
    pub data: VaultData,
    pub vault_error: Option<String>,

    pub tab: Tab,
    pub selected_id: Option<String>,
    pub engine_state: EngineState,
    pub last_alive_check: Instant,
    had_focus: bool,

    pub socks_port: u16,
    pub dns: String,
    pub engine_path: Option<PathBuf>,

    pub import: Option<ImportState>,
    pub export: Option<ExportState>,
    pub confirm_remove: Option<String>,
    pub link_input: String,

    pub logs: VecDeque<String>,
}

impl NpvtApp {
    pub fn new() -> Self {
        let (vault, data, vault_error) = match Vault::open() {
            Ok(vault) => match vault.load() {
                Ok(data) => (Some(vault), data, None),
                Err(e) => (Some(vault), VaultData::default(), Some(e.to_string())),
            },
            Err(e) => (None, VaultData::default(), Some(e.to_string())),
        };

        let mut app = Self {
            vault,
            data,
            vault_error,
            tab: Tab::Dashboard,
            selected_id: None,
            engine_state: EngineState::Stopped,
            last_alive_check: Instant::now(),
            had_focus: false,
            socks_port: DEFAULT_SOCKS_PORT,
            dns: "1.1.1.1".to_string(),
            engine_path: None,
            import: None,
            export: None,
            confirm_remove: None,
            link_input: String::new(),
            logs: VecDeque::new(),
        };
        app.adopt_running_engine();
        if app.selected_id.is_none() {
            app.selected_id = app.data.profiles.first().map(|p| p.id.clone());
        }
        app.log("NPVTunnel started.");
        if let Some(err) = &app.vault_error {
            app.log(format!("vault problem: {err}"));
        }
        app
    }

    // ── lifecycle ───────────────────────────────────────────────────────

    /// Pick up an engine that a previous session (or the CLI) left running.
    fn adopt_running_engine(&mut self) {
        if let Ok(Some(pf)) = engine::read_pid_file() {
            if engine::alive(pf.pid) {
                self.log(format!(
                    "adopted running engine (pid {}, profile '{}')",
                    pf.pid, pf.name
                ));
                self.engine_state = EngineState::Running {
                    pid: pf.pid,
                    name: pf.name,
                    port: pf.socks_port,
                    started_unix: pf.started,
                };
                self.selected_id = Some(pf.profile_id);
            }
        }
    }

    /// Reload the vault from disk (another surface may have changed it).
    pub fn reload_vault(&mut self) {
        let Some(vault) = &self.vault else {
            return;
        };
        match vault.load() {
            Ok(data) => self.data = data,
            Err(e) => self.log(format!("vault reload failed: {e}")),
        }
    }

    /// Called every frame: engine liveness ticks and focus handling.
    pub fn tick(&mut self, focused: bool) {
        if focused && !self.had_focus {
            self.reload_vault();
        }
        self.had_focus = focused;

        match self.engine_state.clone() {
            EngineState::Starting {
                pid,
                since,
                name,
                port,
            } => {
                if since.elapsed() >= STARTUP_GRACE && engine::alive(pid) {
                    self.log(format!("tunnel up: '{name}' (sing-box pid {pid})"));
                    self.engine_state = EngineState::Running {
                        pid,
                        name,
                        port,
                        started_unix: unix_now(),
                    };
                    self.last_alive_check = Instant::now();
                } else if since.elapsed() >= STARTUP_GRACE {
                    let tail = self.engine_log_tail(12);
                    engine::clear_pid_file();
                    let message = format!("sing-box exited immediately after start.\n\n{tail}");
                    self.log("engine failed to start");
                    self.engine_state = EngineState::Failed { message };
                }
            }
            EngineState::Running { pid, .. }
                if self.last_alive_check.elapsed() >= LIVENESS_INTERVAL =>
            {
                self.last_alive_check = Instant::now();
                if !engine::alive(pid) {
                    let tail = self.engine_log_tail(12);
                    engine::clear_pid_file();
                    let message = format!(
                        "the engine stopped unexpectedly (it may have been stopped \
                         outside the app, or the connection dropped hard).\n\n{tail}"
                    );
                    self.log("engine stopped unexpectedly");
                    self.engine_state = EngineState::Failed { message };
                }
            }
            _ => {}
        }
    }

    fn engine_log_tail(&self, lines: usize) -> String {
        engine::work_dir()
            .map(|dir| engine::log_tail(&dir.join("engine.log"), lines))
            .unwrap_or_else(|_| "(engine log not readable)".to_string())
    }

    pub fn log(&mut self, line: impl Into<String>) {
        let secs = unix_now() % 86_400;
        let stamp = format!(
            "{:02}:{:02}:{:02}",
            secs / 3600,
            (secs % 3600) / 60,
            secs % 60
        );
        self.logs.push_back(format!("[{stamp}] {}", line.into()));
        while self.logs.len() > LOG_CAP {
            self.logs.pop_front();
        }
    }

    // ── engine control ──────────────────────────────────────────────────

    pub fn connect(&mut self) {
        let Some(profile) = self.selected_profile().cloned() else {
            return;
        };
        match self.engine_start(&profile) {
            Ok(()) => {}
            Err(e) => {
                self.log(format!("connect failed: {e}"));
                self.engine_state = EngineState::Failed {
                    message: e.to_string(),
                };
            }
        }
    }

    fn engine_start(&mut self, profile: &Profile) -> Result<()> {
        if let Some(pf) = engine::read_pid_file()? {
            if engine::alive(pf.pid) {
                return Err(Error::Engine(format!(
                    "an engine is already running (pid {}, profile '{}'); disconnect first",
                    pf.pid, pf.name
                )));
            }
        }
        let workdir = engine::work_dir()?;
        let cfg = engine::config(
            std::slice::from_ref(profile),
            &profile.id,
            self.socks_port,
            &self.dns,
        )?;
        let cfg_path = workdir.join("engine.json");
        engine::write_config(&cfg, &cfg_path)?;
        let bin = engine::find_engine_binary(self.engine_path.as_deref()).ok_or_else(|| {
            Error::Engine(
                "sing-box was not found. Put sing-box.exe next to npvtunnel-gui.exe, \
                 in the npvtunnel data folder under bin\\, or on PATH (Settings shows \
                 what was checked). Download: https://github.com/SagerNet/sing-box/releases"
                    .into(),
            )
        })?;
        let running = engine::start(&bin, &cfg_path, &workdir.join("engine.log"))?;
        engine::write_pid_file(&PidFile {
            pid: running.pid,
            profile_id: profile.id.clone(),
            name: profile.name.clone(),
            socks_port: self.socks_port,
            started: unix_now(),
        })?;
        self.log(format!(
            "connecting '{}' ({} {}:{}) via sing-box pid {}...",
            profile.name,
            profile.protocol.as_str(),
            profile.server,
            profile.port,
            running.pid
        ));
        self.engine_state = EngineState::Starting {
            pid: running.pid,
            since: Instant::now(),
            name: profile.name.clone(),
            port: self.socks_port,
        };
        Ok(())
    }

    pub fn disconnect(&mut self) {
        match self.engine_state.clone() {
            EngineState::Running { pid, .. } | EngineState::Starting { pid, .. } => {
                match engine::stop(pid) {
                    Ok(()) => self.log(format!("engine stopped (pid {pid})")),
                    Err(e) => self.log(format!("stop failed: {e}")),
                }
                engine::clear_pid_file();
                self.engine_state = EngineState::Stopped;
            }
            _ => {
                engine::clear_pid_file();
                self.engine_state = EngineState::Stopped;
            }
        }
    }

    pub fn dismiss_engine_failure(&mut self) {
        if matches!(self.engine_state, EngineState::Failed { .. }) {
            self.engine_state = EngineState::Stopped;
        }
    }

    pub fn selected_profile(&self) -> Option<&Profile> {
        let id = self.selected_id.as_ref()?;
        self.data.profiles.iter().find(|p| p.id == *id)
    }

    /// Path of the sing-box binary the engine would use, for the UI.
    pub fn engine_binary_path(&self) -> Option<PathBuf> {
        engine::find_engine_binary(self.engine_path.as_deref())
    }

    // ── import ──────────────────────────────────────────────────────────

    /// Begin an import from a picked file.
    pub fn import_file(&mut self, path: PathBuf) {
        let label = path.display().to_string();
        let read = std::fs::metadata(&path)
            .map_err(Error::Io)
            .and_then(|meta| {
                if meta.len() as usize > MAX_IMPORT_FILE {
                    Err(Error::InvalidConfig("file is too large".into()))
                } else {
                    std::fs::read(&path).map_err(Error::Io)
                }
            });
        match read {
            Ok(bytes) => self.begin_import(bytes, label),
            Err(e) => self.log(format!("import failed: {e}")),
        }
    }

    /// Begin an import from a typed/pasted link.
    pub fn import_link(&mut self) {
        let source = self.link_input.trim().to_string();
        if source.is_empty() {
            return;
        }
        self.link_input.clear();
        match parse_import(&source) {
            Ok(ImportSource::Container(bytes)) => self.begin_import(bytes, "share link".into()),
            Ok(ImportSource::Profile(profile)) => {
                let mut state = ImportState {
                    source_label: "share link".into(),
                    bytes: None,
                    lock_kind: "link".into(),
                    passphrase: String::new(),
                    card: None,
                    name_override: String::new(),
                    group: String::new(),
                    error: None,
                };
                match profile.validate() {
                    Ok(()) => {
                        state.card = Some(Card {
                            profile,
                            expires_at: None,
                            metered_only: false,
                            signed: false,
                            creator: None,
                            lock_kind: "link".into(),
                        });
                    }
                    Err(e) => state.error = Some(e.to_string()),
                }
                self.import = Some(state);
            }
            Err(e) => self.log(format!("import failed: {e}")),
        }
    }

    fn begin_import(&mut self, bytes: Vec<u8>, label: String) {
        let mut state = ImportState {
            source_label: label,
            bytes: Some(bytes.clone()),
            lock_kind: String::new(),
            passphrase: String::new(),
            card: None,
            name_override: String::new(),
            group: String::new(),
            error: None,
        };
        if bytes.starts_with(v2::MAGIC) {
            match v2::peek_lock(&bytes) {
                Ok(kind) => {
                    state.lock_kind = kind.to_string();
                    if kind != "passphrase" {
                        match self.decode_container(&bytes, None) {
                            Ok(card) => state.card = Some(card),
                            Err(e) => state.error = Some(e.to_string()),
                        }
                    }
                }
                Err(e) => state.error = Some(e.to_string()),
            }
        } else if v1::probe(&bytes).is_v1 {
            state.lock_kind = "legacy-v1".into();
            match v1::import_best_effort(&bytes) {
                Ok(profile) => {
                    state.card = Some(Card {
                        profile,
                        expires_at: None,
                        metered_only: false,
                        signed: false,
                        creator: None,
                        lock_kind: "legacy-v1".into(),
                    });
                }
                Err(e) => state.error = Some(e.to_string()),
            }
        } else {
            state.error = Some(
                "unrecognized file format (expected an npvt2 container or a legacy v1 file)".into(),
            );
        }
        self.import = Some(state);
    }

    /// Unlock a passphrase-locked container staged in the import dialog.
    pub fn import_unlock(&mut self) {
        let staged = self
            .import
            .as_ref()
            .map(|s| (s.bytes.clone(), s.passphrase.clone()));
        let Some((Some(bytes), pass)) = staged else {
            return;
        };
        if pass.len() < 8 {
            if let Some(state) = self.import.as_mut() {
                state.error = Some("passphrase is too short (minimum 8 characters)".into());
            }
            return;
        }
        match self.decode_container(&bytes, Some(&pass)) {
            Ok(card) => {
                if let Some(state) = self.import.as_mut() {
                    state.error = None;
                    state.card = Some(card);
                    state.passphrase.zeroize();
                    state.passphrase.clear();
                }
            }
            Err(e) => {
                if let Some(state) = self.import.as_mut() {
                    state.error = Some(e.to_string());
                    state.passphrase.zeroize();
                    state.passphrase.clear();
                }
            }
        }
    }

    fn decode_container(&self, bytes: &[u8], pass: Option<&str>) -> Result<Card> {
        let lock_kind = v2::peek_lock(bytes)?;
        let decoded = match lock_kind {
            "open" => v2::decode(bytes, Unlock::Any)?,
            "passphrase" => v2::decode(
                bytes,
                Unlock::Passphrase(
                    pass.ok_or_else(|| Error::Crypto("container is passphrase-locked".into()))?,
                ),
            )?,
            "keys" => {
                let seed = self
                    .data
                    .identity
                    .as_ref()
                    .map(|i: &Identity| i.x25519_seed)
                    .ok_or_else(|| {
                        Error::Vault(
                            "container is key-locked but this device has no identity yet".into(),
                        )
                    })?;
                v2::decode(bytes, Unlock::Keys(&[seed]))?
            }
            other => {
                return Err(Error::InvalidConfig(format!(
                    "container uses an unsupported lock kind '{other}'"
                )))
            }
        };
        if decoded.policy.is_expired(unix_now()) {
            return Err(Error::InvalidConfig(
                "container policy is expired; expired configs are refused by default".into(),
            ));
        }
        Ok(Card {
            profile: decoded.profile,
            expires_at: decoded.policy.expires_at,
            metered_only: decoded.policy.metered_only,
            signed: decoded.verification.signed,
            creator: decoded.verification.creator.clone(),
            lock_kind: lock_kind.to_string(),
        })
    }

    /// Commit the staged import card into the vault.
    pub fn import_commit(&mut self) {
        let staged = self.import.as_ref().and_then(|s| {
            s.card.clone().map(|card| {
                (
                    card,
                    s.name_override.trim().to_string(),
                    s.group.trim().to_string(),
                )
            })
        });
        let Some((card, name_override, group)) = staged else {
            return;
        };
        let mut profile = card.profile.clone();
        let name_res = match name_override.as_str() {
            "" => Ok(unique_name(&self.data, &profile.name)),
            n => clean_name_input(n),
        };
        let group_res = match group.as_str() {
            "" => Ok(None),
            g => clean_name_input(g).map(Some),
        };
        let (name, grp) = match (name_res, group_res) {
            (Ok(n), Ok(g)) => (n, g),
            (Err(e), _) | (_, Err(e)) => {
                if let Some(state) = self.import.as_mut() {
                    state.error = Some(e.to_string());
                }
                return;
            }
        };
        profile.name = name;
        profile.group = grp;
        if let Some(creator) = &card.creator {
            let label = format!("creator {}", &creator[..8.min(creator.len())]);
            self.data
                .known_creators
                .entry(creator.clone())
                .or_insert(label);
        }
        match self.save_profile(profile) {
            Ok(replaced) => {
                self.log(if replaced {
                    "profile replaced"
                } else {
                    "profile added"
                });
                self.import = None;
            }
            Err(e) => {
                if let Some(state) = self.import.as_mut() {
                    state.error = Some(e.to_string());
                }
            }
        }
    }

    fn save_profile(&mut self, profile: Profile) -> Result<bool> {
        let name = profile.name.clone();
        let replaced = upsert_profile(&mut self.data, profile)?;
        if let Some(vault) = &self.vault {
            vault.save(&self.data)?;
        }
        self.selected_id = self
            .data
            .profiles
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.id.clone());
        self.log(format!("saved profile '{name}'"));
        Ok(replaced)
    }

    pub fn cancel_import(&mut self) {
        if let Some(state) = self.import.as_mut() {
            state.passphrase.zeroize();
            state.passphrase.clear();
        }
        self.import = None;
    }

    // ── export ──────────────────────────────────────────────────────────

    pub fn begin_export(&mut self, target: String) {
        self.export = Some(ExportState {
            target,
            passphrase: String::new(),
            repeat: String::new(),
            expires: String::new(),
            out: None,
            error: None,
        });
    }

    pub fn export_pick_file(&mut self) {
        let target = self
            .export
            .as_ref()
            .map(|e| e.target.clone())
            .unwrap_or_default();
        let suggested = format!("{}.npvt2", target.replace(['/', '\\'], "_"));
        if let Some(path) = rfd::FileDialog::new().set_file_name(&suggested).save_file() {
            if let Some(state) = self.export.as_mut() {
                state.out = Some(path);
            }
        }
    }

    pub fn export_commit(&mut self) {
        let mut state = match self.export.take() {
            Some(s) => s,
            None => return,
        };
        match export_impl(&self.data, &mut state) {
            Ok((name, out, len)) => {
                self.log(format!(
                    "exported '{name}' to {} ({len} bytes)",
                    out.display()
                ));
            }
            Err(message) => {
                state.error = Some(message);
                self.export = Some(state);
            }
        }
    }

    pub fn cancel_export(&mut self) {
        if let Some(mut state) = self.export.take() {
            state.passphrase.zeroize();
            state.passphrase.clear();
            state.repeat.zeroize();
            state.repeat.clear();
        }
    }

    // ── profiles ────────────────────────────────────────────────────────

    pub fn use_profile(&mut self, id: String) {
        self.selected_id = Some(id);
        self.tab = Tab::Dashboard;
    }

    pub fn request_remove(&mut self, target: String) {
        self.confirm_remove = Some(target);
    }

    pub fn confirm_remove(&mut self) {
        let Some(target) = self.confirm_remove.take() else {
            return;
        };
        match remove_profile(&mut self.data, &target) {
            Ok(removed) => {
                if let Some(vault) = &self.vault {
                    if let Err(e) = vault.save(&self.data) {
                        self.log(format!("vault save failed: {e}"));
                    }
                }
                self.log(format!("removed profile '{}'", removed.name));
                if self.selected_id.as_deref() == Some(removed.id.as_str()) {
                    self.selected_id = self.data.profiles.first().map(|p| p.id.clone());
                }
            }
            Err(e) => self.log(format!("remove failed: {e}")),
        }
    }

    pub fn generate_identity(&mut self) {
        if self.data.identity.is_some() {
            return;
        }
        let identity = generate_identity();
        self.data.identity = Some(identity);
        if let Some(vault) = &self.vault {
            if let Err(e) = vault.save(&self.data) {
                self.log(format!("vault save failed: {e}"));
            }
        }
        self.log("creator identity generated");
    }
}

/// Validate + render + write an export; split out of `NpvtApp` so the dialog
/// state can be owned while it runs.
fn export_impl(
    data: &VaultData,
    state: &mut ExportState,
) -> std::result::Result<(String, PathBuf, usize), String> {
    let out = match &state.out {
        Some(p) => p.clone(),
        None => return Err("choose an output file first".into()),
    };
    if state.passphrase.len() < 8 {
        return Err("passphrase is too short (minimum 8 characters)".into());
    }
    if state.passphrase != state.repeat {
        return Err("passphrases do not match".into());
    }
    let policy = Policy {
        expires_at: if state.expires.trim().is_empty() {
            None
        } else {
            match parse_expires(state.expires.trim()) {
                Ok(t) => Some(t),
                Err(e) => return Err(e.to_string()),
            }
        },
        metered_only: false,
        group_hint: None,
    };
    let profile = match find_profile(data, &state.target) {
        Ok(p) => p.clone(),
        Err(e) => return Err(e.to_string()),
    };
    let signer = data
        .identity
        .as_ref()
        .map(|i| SigningKey::from_bytes(&i.ed25519_seed));
    let pass = state.passphrase.clone();
    match v2::encode(&profile, &policy, &Lock::Passphrase(pass), signer.as_ref()) {
        Ok(bytes) => match std::fs::write(&out, &bytes) {
            Ok(()) => {
                state.passphrase.zeroize();
                state.passphrase.clear();
                state.repeat.zeroize();
                state.repeat.clear();
                Ok((profile.name, out, bytes.len()))
            }
            Err(e) => Err(Error::Io(e).to_string()),
        },
        Err(e) => Err(e.to_string()),
    }
}

fn unique_name(data: &VaultData, wanted: &str) -> String {
    let clash = |n: &str| data.profiles.iter().any(|p| p.name.eq_ignore_ascii_case(n));
    if !clash(wanted) {
        return wanted.to_string();
    }
    loop {
        let candidate = format!("{wanted} - {}", short_suffix());
        if !clash(&candidate) {
            return candidate;
        }
    }
}

impl eframe::App for NpvtApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let focused = ui.ctx().input(|i| i.viewport().focused == Some(true));
        self.tick(focused);
        crate::ui::draw(self, ui);
    }
}

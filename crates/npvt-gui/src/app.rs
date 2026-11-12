//! The GUI application: state, actions, event handling and the eframe
//! update loop.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;

use eframe::egui;
use npvt_core::profile::Profile;
use npvt_core::vault;
use npvt_core::vault::VaultData;

use crate::engine_bridge::{self, Bridge, Cmd, Ev, StartSpec};
use crate::net;
use crate::settings::{PerAppMode, Settings};
use crate::theme::{self, Palette};
use crate::vault_actions;
use crate::vault_actions::ExportLock;
use crate::widgets;

/// Which modal is open, if any.
#[derive(Debug, Clone)]
pub enum Modal {
    AddConfig,
    Subscription,
    ManualAdd,
    NewGroup,
    Sort,
    Passphrase,
    Export { profile_id: String },
    RenameGroup { label: String },
    EditProfile { id: String },
    ConfirmDeleteProfile { id: String },
    ConfirmDeleteGroup { label: String },
    Settings,
    PerApp,
    PublicKey,
    Premium,
    Points,
    Assets,
    About,
}

/// Scratch state for export dialogs.
#[derive(Default)]
pub struct ExportState {
    pub profile_id: String,
    pub filename: String,
    /// 0 = lock it, 1 = keep it open.
    pub lock: usize,
    /// 0 = specific people, 1 = passphrase.
    pub deliver: usize,
    pub keys: String,
    pub pass: String,
    pub pass2: String,
    /// 0.0 MINIMAL .. 1.0 MAXIMUM (snaps to thirds).
    pub secrecy: f32,
    pub err: Option<String>,
}

/// Scratch buffers for open dialogs.
pub struct ModalState {
    pub sub_url: String,
    pub sub_group: String,
    pub sub_busy: bool,
    pub manual_link: String,
    pub manual_name: String,
    pub manual_group: String,
    pub group_name: String,
    pub pass_entry: String,
    pub pass_err: Option<String>,
    pub pending_locked: Option<Vec<u8>>,
    pub export: ExportState,
    pub edit_id: String,
    pub edit_name: String,
    pub edit_group: String,
    pub edit_udp: bool,
    pub edit_insecure: bool,
    pub set_socks: String,
    pub set_clash: String,
    pub set_dns: String,
    pub set_engine: String,
    pub set_quota: String,
    pub set_autostart: bool,
    pub set_launch: bool,
    pub set_loglevel: usize,
    pub per_app_text: String,
    pub per_app_mode: PerAppMode,
}

pub struct App {
    pub palette: Palette,
    pub settings: Settings,
    pub vault: VaultData,
    pub bridge: Bridge,
    /// Sender for worker threads that push UI events.
    pub ev_tx: Sender<Ev>,
    pub page: usize,
    pub modal: Option<Modal>,
    pub ms: ModalState,
    pub toast: Option<widgets::Toast>,
    pub time: f64,
    pub pings: BTreeMap<String, Option<u128>>,
    pub last_ping_at: Option<f64>,
    pub expanded: BTreeSet<String>,
    pub search_open: bool,
    pub search: String,
    pub log_filter: String,
    pub event_log: Vec<String>,
    pub engine_tail: Vec<String>,
    pub last_tail_refresh: f64,
    pub connecting_since: Option<f64>,
    pub quit_requested: bool,
    pub asset_busy: bool,
    pub asset_which: usize,
    pub asset_progress: net::AssetProgress,
    pub asset_status: String,
    /// Watermark of the lifetime downlink counter for monthly accounting.
    pub persisted_lifetime_down: u64,
    pub last_persist: f64,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut settings = Settings::load();
        settings.roll_month_if_needed();
        theme::apply(&cc.egui_ctx, settings.theme);
        let vault_path = vault::Vault::default_path()
            .unwrap_or_else(|_| std::env::temp_dir().join("npvtunnel-vault.json"));
        let vault_file = vault::Vault::at(vault_path);
        let vault_data = vault_file.load().unwrap_or_default();

        let bridge = Bridge::new();
        let ev_tx = bridge.event_sender();
        let mut app = App {
            palette: theme::resolve(settings.theme),
            settings,
            vault: vault_data,
            bridge,
            ev_tx,
            page: 0,
            modal: None,
            ms: ModalState::new(),
            toast: None,
            time: 0.0,
            pings: BTreeMap::new(),
            last_ping_at: None,
            expanded: BTreeSet::new(),
            search_open: false,
            search: String::new(),
            log_filter: String::new(),
            event_log: Vec::new(),
            engine_tail: Vec::new(),
            last_tail_refresh: 0.0,
            connecting_since: None,
            quit_requested: false,
            asset_busy: false,
            asset_which: 0,
            asset_progress: net::AssetProgress::new(),
            asset_status: String::new(),
            persisted_lifetime_down: 0,
            last_persist: 0.0,
        };
        app.push_log("INFO", "NPVTunnel started");
        // Dev/test harness: pick a start page or open a modal without
        // clicking (used by the visual smoke checks; ignored otherwise).
        if let Ok(page) = std::env::var("NPVT_START_PAGE") {
            if let Ok(v) = page.parse::<usize>() {
                app.page = v.min(3);
            }
        }
        if std::env::var("NPVT_EXPAND_ALL").as_deref() == Ok("1") {
            app.expanded = app.all_group_labels();
        }
        match std::env::var("NPVT_OPEN_MODAL").as_deref() {
            Ok("add") => app.open_modal(Modal::AddConfig),
            Ok("export") => {
                if let Some(profile) = app.vault.profiles.first() {
                    let id = profile.id.clone();
                    app.open_modal(Modal::Export { profile_id: id });
                }
            }
            Ok("settings") => app.open_modal(Modal::Settings),
            _ => {}
        }
        if app.settings.connect_on_launch && app.settings.active_id.is_some() {
            app.connect();
        }
        app
    }

    // ---------- helpers ----------

    pub fn open_modal(&mut self, modal: Modal) {
        self.ms.sub_busy = false;
        self.ms.export.err = None;
        match &modal {
            Modal::Export { profile_id } => {
                if let Some(profile) = self.vault.profiles.iter().find(|p| &p.id == profile_id) {
                    self.ms.export.profile_id = profile_id.clone();
                    self.ms.export.filename = format!("{}.npvt", profile.name);
                    self.ms.export.lock = 0;
                    self.ms.export.deliver = 0;
                    self.ms.export.keys.clear();
                    self.ms.export.pass.clear();
                    self.ms.export.pass2.clear();
                    self.ms.export.secrecy = 0.34;
                }
            }
            Modal::EditProfile { id } => {
                if let Some(profile) = self.vault.profiles.iter().find(|p| &p.id == id) {
                    self.ms.edit_id = id.clone();
                    self.ms.edit_name = profile.name.clone();
                    self.ms.edit_group = profile.group.clone().unwrap_or_default();
                    self.ms.edit_udp = profile.udp;
                    self.ms.edit_insecure = profile.allow_insecure;
                }
            }
            Modal::RenameGroup { label } => {
                self.ms.group_name = label.clone();
            }
            Modal::Settings => {
                self.ms.set_socks = self.settings.socks_port.to_string();
                self.ms.set_clash = self.settings.clash_port.to_string();
                self.ms.set_dns = self.settings.dns_server.clone();
                self.ms.set_engine = self.settings.engine_path.clone().unwrap_or_default();
                self.ms.set_quota = self.settings.quota_gb.to_string();
                self.ms.set_autostart = self.settings.autostart;
                self.ms.set_launch = self.settings.connect_on_launch;
                self.ms.set_loglevel = ["warn", "info", "error", "debug"]
                    .iter()
                    .position(|l| *l == self.settings.log_level)
                    .unwrap_or(0);
            }
            Modal::PerApp => {
                self.ms.per_app_mode = self.settings.per_app_mode;
                self.ms.per_app_text = self.settings.per_app.join("\n");
            }
            Modal::PublicKey => {
                vault_actions::ensure_identity(&mut self.vault);
                self.persist_vault();
            }
            _ => {}
        }
        self.modal = Some(modal);
    }

    pub fn close_modal(&mut self) {
        self.modal = None;
    }

    pub fn toast_now(&mut self, message: impl Into<String>) {
        self.toast = Some(widgets::Toast::new(message, self.time));
    }

    pub fn push_log(&mut self, level: &str, message: &str) {
        let secs = npvt_core::profile::unix_now();
        let stamp = format!(
            "[{:02}:{:02}:{:02}]",
            (secs / 3600) % 24,
            (secs / 60) % 60,
            secs % 60
        );
        self.event_log.push(format!("{stamp} {level} {message}"));
        if self.event_log.len() > 600 {
            self.event_log.remove(0);
        }
    }

    pub fn copy_to_clipboard(&mut self, text: String) {
        match arboard::Clipboard::new().and_then(|mut c| c.set_text(text.clone())) {
            Ok(()) => self.toast_now("Copied to clipboard"),
            Err(_) => self.toast_now("Clipboard not available"),
        }
    }

    pub fn persist_vault(&mut self) {
        let path = vault::Vault::default_path();
        if let Ok(path) = path {
            let file = vault::Vault::at(path);
            if let Err(e) = file.save(&self.vault) {
                self.push_log("ERROR", &format!("vault save failed: {e}"));
            }
        }
    }

    pub fn active_profile(&self) -> Option<&Profile> {
        let id = self.settings.active_id.as_deref()?;
        self.vault.profiles.iter().find(|p| p.id == id)
    }

    pub fn set_active_profile(&mut self, id: &str) {
        self.settings.active_id = Some(id.to_owned());
        self.settings.save();
    }

    pub fn members_of_group(&self, label: &str) -> Vec<Profile> {
        self.vault
            .profiles
            .iter()
            .filter(|p| vault_actions::group_label(p) == label)
            .cloned()
            .collect()
    }

    pub fn all_group_labels(&self) -> BTreeSet<String> {
        vault_actions::group_profiles(&self.vault)
            .into_iter()
            .map(|(l, _)| l)
            .chain(self.settings.extra_groups.iter().cloned())
            .collect()
    }

    pub fn last_ping_label(&self) -> String {
        match self.last_ping_at {
            None => "Never".to_owned(),
            Some(t) => {
                let elapsed = self.time - t;
                if elapsed < 60.0 {
                    format!("{}s ago", elapsed as u64)
                } else if elapsed < 3600.0 {
                    format!("{}m ago", (elapsed / 60.0) as u64)
                } else {
                    format!("{}h ago", (elapsed / 3600.0) as u64)
                }
            }
        }
    }

    // ---------- actions ----------

    pub fn toggle_connection(&mut self) {
        let state = self.bridge.shared().state();
        if state == engine_bridge::STATE_CONNECTED || state == engine_bridge::STATE_CONNECTING {
            self.disconnect();
        } else {
            self.connect();
        }
    }

    pub fn connect(&mut self) {
        let Some(active_id) = self.settings.active_id.clone() else {
            self.toast_now("Select a config in CONFIGS first");
            return;
        };
        let Some(active) = self.vault.profiles.iter().find(|p| p.id == active_id) else {
            self.toast_now("The selected config no longer exists");
            return;
        };
        let label = vault_actions::group_label(active);
        let members: Vec<Profile> = self
            .members_of_group(&label)
            .into_iter()
            .filter(|p| p.protocol.engine_supported())
            .collect();
        if members.is_empty() {
            self.toast_now("No engine-compatible config in this group");
            return;
        }
        let spec = StartSpec {
            members,
            active_id,
            socks_port: self.settings.socks_port,
            clash_port: self.settings.clash_port,
            dns_server: self.settings.dns_server.clone(),
            log_level: self.settings.log_level.clone(),
            per_app_mode: self.settings.per_app_mode,
            per_app: self.settings.per_app.clone(),
            engine_path: self.settings.engine_path.clone(),
        };
        self.bridge.send(Cmd::Start(spec));
        self.push_log("INFO", &format!("connecting to '{label}'..."));
    }

    pub fn disconnect(&mut self) {
        self.bridge.send(Cmd::Stop);
        self.push_log("INFO", "disconnecting...");
    }

    pub fn ping_all(&mut self) {
        let ids: Vec<(String, String, u16)> = self
            .vault
            .profiles
            .iter()
            .map(|p| (p.id.clone(), p.server.clone(), p.port))
            .collect();
        let count = ids.len();
        for (id, _, _) in &ids {
            self.pings.insert(id.clone(), None);
        }
        self.last_ping_at = Some(self.time);
        engine_bridge::spawn_ping_worker(self.ev_tx.clone(), ids);
        self.push_log("INFO", &format!("pinging {count} configs..."));
    }

    pub fn ping_group(&mut self, label: &str) {
        let ids: Vec<(String, String, u16)> = self
            .members_of_group(label)
            .iter()
            .map(|p| (p.id.clone(), p.server.clone(), p.port))
            .collect();
        for (id, _, _) in &ids {
            self.pings.insert(id.clone(), None);
        }
        self.last_ping_at = Some(self.time);
        engine_bridge::spawn_ping_worker(self.ev_tx.clone(), ids);
    }

    pub fn activate_group(&mut self, label: &str) {
        let members = self.members_of_group(label);
        if members.is_empty() {
            self.toast_now("Group has no configs");
            return;
        }
        // Prefer the fastest member we know about.
        let best = members
            .iter()
            .filter_map(|p| {
                self.pings
                    .get(&p.id)
                    .and_then(|ping| ping.map(|ms| (ms, p.id.clone())))
            })
            .min_by_key(|(ms, _)| *ms)
            .map(|(_, id)| id)
            .unwrap_or_else(|| members[0].id.clone());
        self.set_active_profile(&best);
        self.toast_now(format!("Active group: {label}"));
        self.push_log("INFO", &format!("active group set to '{label}'"));
    }

    pub fn delete_profile(&mut self, id: &str) {
        match vault::remove_profile(&mut self.vault, id) {
            Ok(profile) => {
                if self.settings.active_id.as_deref() == Some(id) {
                    self.settings.active_id = None;
                }
                self.pings.remove(id);
                self.persist_vault();
                self.toast_now(format!("Removed {}", profile.name));
                self.push_log("INFO", &format!("removed '{}'", profile.name));
            }
            Err(e) => self.toast_now(format!("Remove failed: {e}")),
        }
    }

    pub fn delete_group(&mut self, label: &str) {
        let members = self.members_of_group(label);
        for member in &members {
            let _ = vault::remove_profile(&mut self.vault, &member.id);
            if self.settings.active_id.as_deref() == Some(&member.id) {
                self.settings.active_id = None;
            }
            self.pings.remove(&member.id);
        }
        self.settings.extra_groups.retain(|l| l != label);
        self.settings.subscriptions.remove(label);
        self.expanded.remove(label);
        self.persist_vault();
        self.settings.save();
        self.toast_now(format!("Deleted group '{label}'"));
        self.push_log(
            "INFO",
            &format!("deleted group '{label}' ({} configs)", members.len()),
        );
    }

    pub fn rename_group(&mut self, label: &str, new_name: &str) {
        for profile in &mut self.vault.profiles {
            if vault_actions::group_label(profile) == label {
                profile.group = if new_name == "Saved" {
                    None
                } else {
                    Some(new_name.to_owned())
                };
            }
        }
        if let Some(pos) = self.settings.extra_groups.iter().position(|l| l == label) {
            self.settings.extra_groups[pos] = new_name.to_owned();
        }
        if let Some(info) = self.settings.subscriptions.remove(label) {
            self.settings
                .subscriptions
                .insert(new_name.to_owned(), info);
        }
        if self.expanded.remove(label) {
            self.expanded.insert(new_name.to_owned());
        }
        self.persist_vault();
        self.settings.save();
        self.toast_now("Group renamed");
    }

    pub fn confirm_delete_profile(&mut self, id: &str) {
        self.open_modal(Modal::ConfirmDeleteProfile { id: id.to_owned() });
    }

    pub fn confirm_delete_group(&mut self, label: &str) {
        self.open_modal(Modal::ConfirmDeleteGroup {
            label: label.to_owned(),
        });
    }

    pub fn apply_profile_edit(&mut self, id: &str) {
        let name = self.ms.edit_name.trim().to_owned();
        let group_raw = self.ms.edit_group.trim().to_owned();
        let group = match group_raw.as_str() {
            "" | "Saved" => None,
            g => Some(g.to_owned()),
        };
        if let Err(e) = vault::clean_name_input(&name) {
            self.toast_now(format!("Invalid name: {e}"));
            return;
        }
        if let Some(profile) = self.vault.profiles.iter_mut().find(|p| p.id == id) {
            profile.name = name;
            profile.group = group;
            profile.udp = self.ms.edit_udp;
            profile.allow_insecure = self.ms.edit_insecure;
            if let Err(e) = profile.validate() {
                self.toast_now(format!("Invalid profile: {e}"));
                return;
            }
        }
        self.persist_vault();
        self.toast_now("Saved");
    }

    pub fn apply_settings_dialog(&mut self) {
        let socks: u16 = self.ms.set_socks.trim().parse().unwrap_or(0);
        let clash: u16 = self.ms.set_clash.trim().parse().unwrap_or(0);
        let quota: u32 = self.ms.set_quota.trim().parse().unwrap_or(0);
        if !(1024..=65535).contains(&socks) || !(1024..=65535).contains(&clash) {
            self.toast_now("Ports must be 1024-65535");
            return;
        }
        let dns = self.ms.set_dns.trim().to_owned();
        if dns.is_empty() {
            self.toast_now("DNS server must not be empty");
            return;
        }
        let engine = self.ms.set_engine.trim().to_owned();
        if !engine.is_empty() && !std::path::Path::new(&engine).is_file() {
            self.toast_now("Engine path does not exist");
            return;
        }
        if self.settings.autostart != self.ms.set_autostart {
            if let Err(e) = crate::settings::set_autostart(self.ms.set_autostart) {
                self.toast_now(format!("Autostart failed: {e}"));
            }
        }
        self.settings.socks_port = socks;
        self.settings.clash_port = clash;
        self.settings.dns_server = dns;
        self.settings.quota_gb = quota;
        self.settings.engine_path = if engine.is_empty() {
            None
        } else {
            Some(engine)
        };
        self.settings.autostart = self.ms.set_autostart;
        self.settings.connect_on_launch = self.ms.set_launch;
        self.settings.log_level =
            ["warn", "info", "error", "debug"][self.ms.set_loglevel].to_owned();
        self.settings.save();
        self.push_log("INFO", "settings saved");
        self.toast_now("Settings saved");
        self.close_modal();
    }

    // ---------- import actions ----------

    pub fn import_clipboard(&mut self) {
        let text = match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(t) => t,
            Err(_) => {
                self.toast_now("Clipboard not available");
                return;
            }
        };
        if text.trim().is_empty() {
            self.toast_now("Clipboard is empty");
            return;
        }
        let report = vault_actions::import_links(&mut self.vault, &text, None);
        self.persist_vault();
        self.push_log(
            "INFO",
            &format!(
                "clipboard import: {} ok, {} failed",
                report.imported.len(),
                report.failed.len()
            ),
        );
        self.toast_now(report.summary());
    }

    pub fn import_file_dialog(&mut self) {
        match pick_config_file() {
            Some(path) => match std::fs::read(&path) {
                Ok(bytes) => {
                    let fallback = vault::label_from_filename(&path);
                    match vault_actions::import_file_bytes(&mut self.vault, &bytes, &fallback) {
                        Ok(name) => {
                            self.persist_vault();
                            self.push_log(
                                "INFO",
                                &format!("imported file '{fallback}' as '{name}'"),
                            );
                            self.toast_now(format!("Imported {name}"));
                        }
                        Err(vault_actions::FileImportError::NeedsUnlock(msg)) => {
                            self.push_log("INFO", &msg);
                            self.ms.pending_locked = Some(bytes);
                            self.ms.pass_entry.clear();
                            self.ms.pass_err = None;
                            self.open_modal(Modal::Passphrase);
                        }
                        Err(vault_actions::FileImportError::Other(e)) => {
                            self.toast_now(format!("Import failed: {e}"));
                        }
                    }
                }
                Err(e) => self.toast_now(format!("Cannot read file: {e}")),
            },
            None => {
                self.toast_now(
                    "No file selected (dialogs need the Windows build; paste a link instead)",
                );
            }
        }
    }

    pub fn import_qr(&mut self) {
        match pick_image_file() {
            Some(path) => match net::decode_qr_file(&path) {
                Ok(payloads) if payloads.is_empty() => {
                    self.toast_now("No QR code found in the image");
                }
                Ok(payloads) => {
                    let mut report = vault_actions::ImportReport::default();
                    for payload in payloads {
                        let sub = vault_actions::import_links(&mut self.vault, &payload, None);
                        report.imported.extend(sub.imported);
                        report.failed.extend(sub.failed);
                    }
                    self.persist_vault();
                    self.toast_now(report.summary());
                }
                Err(e) => self.toast_now(format!("QR decode failed: {e}")),
            },
            None => {
                self.toast_now(
                    "No image selected (dialogs need the Windows build; paste a link instead)",
                );
            }
        }
    }

    pub fn fetch_subscription_async(&mut self, url: String, group: String) {
        let tx = self.ev_tx.clone();
        std::thread::Builder::new()
            .name("npvt-subscription".into())
            .spawn(move || {
                let result = net::fetch_subscription(&url)
                    .map(|(info, body)| (info, net::extract_links(&body)));
                let _ = tx.send(Ev::SubscriptionDone { group, result });
            })
            .ok();
    }

    pub fn handle_subscription_done(
        &mut self,
        group: String,
        result: Result<(net::SubInfoAlias, Vec<String>), String>,
    ) {
        self.ms.sub_busy = false;
        match result {
            Ok((info, links)) => {
                if links.is_empty() {
                    self.toast_now("Subscription returned no links");
                    return;
                }
                let mut imported = 0;
                let mut failed = 0;
                for link in &links {
                    match vault_actions::import_one(&mut self.vault, link, Some(&group)) {
                        Ok(_) => imported += 1,
                        Err(_) => failed += 1,
                    }
                }
                self.persist_vault();
                if imported > 0 {
                    let sub = crate::settings::SubInfo {
                        upload: info.upload,
                        download: info.download,
                        total: info.total,
                        expire: info.expire,
                    };
                    self.settings.subscriptions.insert(group.clone(), sub);
                    if !self.settings.extra_groups.contains(&group) {
                        self.settings.extra_groups.push(group.clone());
                    }
                    self.settings.save();
                }
                self.push_log(
                    "INFO",
                    &format!("subscription '{group}': {imported} imported, {failed} failed"),
                );
                self.toast_now(format!("Subscription '{group}': {imported} configs"));
                self.close_modal();
            }
            Err(e) => {
                self.toast_now(format!("Subscription failed: {e}"));
            }
        }
    }

    // ---------- export actions ----------

    fn build_export_spec(&self) -> Result<vault_actions::ExportSpec, String> {
        let profile_id = self.ms.export.profile_id.clone();
        let Some(profile) = self.vault.profiles.iter().find(|p| p.id == profile_id) else {
            return Err("config not found".to_owned());
        };
        let lock = if self.ms.export.lock == 1 {
            ExportLock::Open
        } else if self.ms.export.deliver == 1 {
            ExportLock::Passphrase
        } else {
            ExportLock::Keys
        };
        let recipients = if lock == ExportLock::Keys {
            vault_actions::parse_recipient_keys(&self.ms.export.keys)?
        } else {
            Vec::new()
        };
        let passphrase = if lock == ExportLock::Passphrase {
            if self.ms.export.pass != self.ms.export.pass2 {
                return Err("passphrases do not match".to_owned());
            }
            Some(self.ms.export.pass.clone())
        } else {
            None
        };
        // Secrecy -> expiry: MINIMAL never expires, MODERATE 30 days,
        // MAXIMUM 24 hours.
        let expires_in = if self.ms.export.secrecy < 0.34 {
            None
        } else if self.ms.export.secrecy < 0.67 {
            Some(30u64 * 86_400)
        } else {
            Some(86_400)
        };
        Ok(vault_actions::ExportSpec {
            profile: profile.clone(),
            lock,
            expires_in,
            filename: self.ms.export.filename.clone(),
            recipients,
            passphrase,
        })
    }

    pub fn export_profile(&mut self) {
        let spec = match self.build_export_spec() {
            Ok(spec) => spec,
            Err(e) => {
                self.ms.export.err = Some(e);
                return;
            }
        };
        match vault_actions::export_container(&self.vault, &spec) {
            Ok(bytes) => {
                let filename = if spec.filename.trim().is_empty() {
                    "config.npvt".to_owned()
                } else {
                    spec.filename.trim().to_owned()
                };
                match save_export_file(&filename) {
                    Some(path) => match std::fs::write(&path, &bytes) {
                        Ok(()) => {
                            self.push_log(
                                "INFO",
                                &format!("exported '{}' to {}", filename, path.display()),
                            );
                            self.toast_now(format!("Exported {filename}"));
                            self.close_modal();
                        }
                        Err(e) => self.ms.export.err = Some(format!("write failed: {e}")),
                    },
                    None => {
                        self.ms.export.err = Some(
                            "no save location chosen (dialogs need the Windows build)".to_owned(),
                        );
                    }
                }
            }
            Err(e) => self.ms.export.err = Some(e),
        }
    }

    pub fn export_link_to_clipboard(&mut self) {
        let spec = match self.build_export_spec() {
            Ok(spec) => spec,
            Err(e) => {
                self.ms.export.err = Some(e);
                return;
            }
        };
        match vault_actions::export_container(&self.vault, &spec) {
            Ok(bytes) => {
                let link = vault_actions::container_to_link(&bytes);
                if link.len() > 60_000 {
                    self.ms.export.err = Some(
                        "container too large for a share link; save a file instead".to_owned(),
                    );
                    return;
                }
                self.copy_to_clipboard(link);
            }
            Err(e) => self.ms.export.err = Some(e),
        }
    }

    // ---------- misc actions ----------

    pub fn start_asset_download(&mut self, which: usize, name: &str) {
        let Ok(dir) = npvt_core::engine::work_dir() else {
            self.toast_now("Data directory unavailable");
            return;
        };
        let name = name.to_owned();
        self.asset_busy = true;
        self.asset_which = which;
        self.asset_status = format!("downloading {name}...");
        let progress = self.asset_progress.clone();
        let tx = self.ev_tx.clone();
        let url = if which == 0 {
            net::GEOIP_URL
        } else {
            net::GEOSITE_URL
        }
        .to_owned();
        let dest = dir.join(&name);
        std::thread::Builder::new()
            .name("npvt-asset".into())
            .spawn(move || {
                let _ = tx.send(Ev::AssetStep {
                    which,
                    name: name.to_owned(),
                });
                let err = net::download_geo_asset(&url, &dest, &progress).err();
                let _ = tx.send(Ev::AssetDone { which, err });
            })
            .ok();
    }

    pub fn open_log_folder(&mut self) {
        if let Ok(dir) = npvt_core::engine::work_dir() {
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("explorer")
                    .arg(dir.to_string_lossy().to_string())
                    .spawn();
            }
            #[cfg(not(windows))]
            {
                self.toast_now(dir.to_string_lossy().to_string());
            }
        }
    }

    pub fn refresh_engine_tail(&mut self) {
        if let Ok(dir) = npvt_core::engine::work_dir() {
            let log = dir.join("npvtunnel-gui-engine.log");
            let tail = npvt_core::engine::log_tail(&log, 120);
            self.engine_tail = tail
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| format!("[engine] {l}"))
                .collect();
        }
    }

    pub fn visible_log_lines(&self) -> Vec<String> {
        let filter = self.log_filter.to_lowercase();
        let mut lines: Vec<String> = self.event_log.clone();
        lines.extend(self.engine_tail.iter().cloned());
        if filter.is_empty() {
            lines
        } else {
            lines
                .into_iter()
                .filter(|l| l.to_lowercase().contains(&filter))
                .collect()
        }
    }

    pub fn handle_event(&mut self, ev: Ev) {
        match ev {
            Ev::Started(pid) => {
                self.connecting_since = Some(self.time);
                self.push_log("INFO", &format!("engine started (pid {pid})"));
            }
            Ev::TrafficLive => {
                self.connecting_since = None;
                self.bridge
                    .shared()
                    .set_state(engine_bridge::STATE_CONNECTED);
                self.push_log("INFO", "tunnel established (SOCKS5 127.0.0.1)");
            }
            Ev::Died { tail } => {
                self.connecting_since = None;
                self.push_log("ERROR", "engine exited unexpectedly");
                for line in tail.lines().take(12) {
                    self.push_log("ENGINE", line);
                }
            }
            Ev::StartFailed(err) => {
                self.connecting_since = None;
                self.push_log("ERROR", &format!("start failed: {err}"));
                self.toast_now("Connect failed - see LOGS");
            }
            Ev::EngineMissing => {
                self.connecting_since = None;
                self.push_log("ERROR", "sing-box engine not found");
                self.toast_now("sing-box engine not found - see LOGS");
            }
            Ev::Stopped => {
                self.connecting_since = None;
                self.push_log("INFO", "disconnected");
            }
            Ev::Ping { id, ms } => {
                if let Some(profile) = self.vault.profiles.iter().find(|p| p.id == id) {
                    let label = match ms {
                        Some(v) => format!("{} ms", v),
                        None => "timeout".to_owned(),
                    };
                    self.push_log("INFO", &format!("ping '{}': {label}", profile.name));
                }
                self.pings.insert(id, ms);
            }
            Ev::SubscriptionDone { group, result } => {
                self.handle_subscription_done(group, result);
            }
            Ev::AssetStep { which, name } => {
                self.asset_busy = true;
                self.asset_which = which;
                self.asset_status = format!("downloading {name}...");
            }
            Ev::AssetDone { which, err } => {
                self.asset_busy = false;
                match err {
                    None => {
                        self.asset_status = format!(
                            "{} saved to the npvtunnel data folder",
                            if which == 0 { "geoip.db" } else { "geosite.db" }
                        );
                        self.push_log("INFO", &self.asset_status.clone());
                        self.toast_now("Asset downloaded");
                    }
                    Some(e) => {
                        self.asset_status = e.clone();
                        self.push_log("ERROR", &format!("asset download failed: {e}"));
                    }
                }
            }
        }
    }

    fn poll_events(&mut self) {
        for ev in self.bridge.drain() {
            self.handle_event(ev);
        }
        // Fallback: connecting for >6s with a live engine counts as connected.
        if let Some(since) = self.connecting_since {
            if self.time - since > 6.0
                && self.bridge.shared().state() == engine_bridge::STATE_CONNECTING
            {
                self.connecting_since = None;
                self.bridge
                    .shared()
                    .set_state(engine_bridge::STATE_CONNECTED);
                self.push_log("INFO", "tunnel assumed established");
            }
        }
    }

    fn persist_traffic_tick(&mut self) {
        if self.time - self.last_persist < 12.0 {
            return;
        }
        self.last_persist = self.time;
        let lifetime = self.bridge.shared().lifetime_down.load(Ordering::Relaxed);
        let delta = lifetime.saturating_sub(self.persisted_lifetime_down);
        if delta > 0 {
            self.persisted_lifetime_down = lifetime;
            self.settings.roll_month_if_needed();
            self.settings.monthly_down += delta;
            self.settings.save();
        }
    }

    fn theme_tick(&mut self, ctx: &egui::Context) {
        let resolved = theme::resolve(self.settings.theme);
        // Re-apply only when something changed (cheap equality check).
        if resolved.bg != self.palette.bg || resolved.accent != self.palette.accent {
            self.palette = resolved;
            theme::apply(ctx, self.settings.theme);
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.time = ctx.input(|i| i.time);
        if self.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.theme_tick(ctx);
        self.poll_events();
        self.persist_traffic_tick();

        // Bottom navigation.
        let mut nav_clicked: Option<usize> = None;
        egui::TopBottomPanel::bottom("nav")
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                widgets::nav_bar(ui, self.palette, self.page, &mut nav_clicked);
            });
        if let Some(i) = nav_clicked {
            self.page = i;
        }

        // Main content.
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(self.palette.bg)
                    .inner_margin(egui::Margin {
                        left: 14.0,
                        right: 14.0,
                        top: 0.0,
                        bottom: 0.0,
                    }),
            )
            .show(ctx, |ui| match self.page {
                0 => self.draw_home(ui),
                1 => self.draw_configs(ui),
                2 => self.draw_logs(ui),
                _ => self.draw_more(ui),
            });

        // Modal on top of everything.
        let modal_open = self.draw_modal(ctx);

        // Toast.
        if let Some(toast) = &self.toast {
            toast.draw(ctx, self.palette);
        }
        if self.toast.as_ref().is_some_and(|t| t.expired(self.time)) {
            self.toast = None;
        }

        // Keep animating while connected/connecting or while a toast shows.
        let state = self.bridge.shared().state();
        if state == engine_bridge::STATE_CONNECTED || state == engine_bridge::STATE_CONNECTING {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        } else if self.toast.is_some() || modal_open {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
    }
}

#[cfg(windows)]
fn pick_config_file() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Config files", &["npvt", "npvs", "txt", "json"])
        .pick_file()
}

#[cfg(not(windows))]
fn pick_config_file() -> Option<std::path::PathBuf> {
    None
}

#[cfg(windows)]
fn pick_image_file() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "bmp", "webp"])
        .pick_file()
}

#[cfg(not(windows))]
fn pick_image_file() -> Option<std::path::PathBuf> {
    None
}

#[cfg(windows)]
fn save_export_file(default_name: &str) -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter("NPVT container", &["npvt"])
        .set_file_name(default_name)
        .save_file()
}

#[cfg(not(windows))]
fn save_export_file(_default_name: &str) -> Option<std::path::PathBuf> {
    None
}

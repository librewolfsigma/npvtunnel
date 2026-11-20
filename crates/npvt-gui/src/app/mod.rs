//! The GUI application: state, actions, event handling and the eframe
//! update loop.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::Sender;

use npvt_core::vault;
use npvt_core::vault::VaultData;

use crate::engine_bridge::{Bridge, Ev};
use crate::net;
use crate::settings::{PerAppMode, Settings};
use crate::theme::{self, Palette};
use crate::widgets;

pub mod actions;
pub mod export_actions;
pub mod frames;
pub mod helpers;
pub mod import_actions;
pub mod misc_actions;

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
    /// TUN mode checkbox in the settings dialog (applied on Save).
    pub set_tun: bool,
    /// TUN interface address (CIDR) text field in the settings dialog.
    pub set_tun_addr: String,
    /// Kill switch checkbox in the settings dialog (applied on Save).
    pub set_kill: bool,
    /// File-association checkbox in the settings dialog (applied on Save:
    /// writes the HKCU association when newly enabled).
    pub set_assoc: bool,
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
    /// `.npvt` / `.npvs` file passed on the command line (double-click
    /// launch), waiting to be imported once the UI is up.
    pub pending_import: Option<std::path::PathBuf>,
    /// Whether the window is currently hidden to the system tray (Windows).
    #[cfg(windows)]
    pub hidden_to_tray: bool,
    /// System tray icon handle; keeping it alive keeps the icon visible.
    #[cfg(windows)]
    pub tray: Option<tray_icon::TrayIcon>,
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
        // System tray (Windows only): closed windows keep running in the
        // tray until the menu's Quit entry is used.
        #[cfg(windows)]
        let tray = crate::tray::create();
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
            pending_import: crate::assoc::pending_file_from_args(),
            #[cfg(windows)]
            hidden_to_tray: false,
            #[cfg(windows)]
            tray,
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

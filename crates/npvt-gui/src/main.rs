//! NPV TUNNEL — the NPVTunnel native Windows GUI client.
//!
//! A `windows`-subsystem app: no console window is ever opened in release
//! builds. The UI is an egui window styled after the NPV TUNNEL brand
//! (dark surfaces, neon green power button, config cards), wired to the
//! same npvt-core vault and sing-box sidecar the CLI uses.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

mod bridge;
mod pages;
mod settings;
mod store;
mod theme;
mod widgets;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Color32, FontFamily, Layout, RichText, Ui};
use npvt_core::profile::{Credential, Profile, Protocol, Transport};
use npvt_core::vault::VaultData;

use bridge::{Bridge, EngineState, Pending};
use settings::{Settings, ThemeMode};
use store::Incoming;
use theme::Palette;
use widgets::Icon;

/// UI protocol families offered in the manual-add form (engine-supported).
const MANUAL_PROTOCOLS: [Protocol; 7] = [
    Protocol::Vless,
    Protocol::Vmess,
    Protocol::Trojan,
    Protocol::Shadowsocks,
    Protocol::Hysteria2,
    Protocol::Socks,
    Protocol::Http,
];

const MANUAL_TRANSPORTS: [Transport; 5] = [
    Transport::Plain,
    Transport::Tls,
    Transport::Ws,
    Transport::Grpc,
    Transport::H2,
];

/// Draft state for the manual add / edit dialog.
#[derive(Debug, Clone, Default)]
struct Draft {
    name: String,
    protocol: usize,
    server: String,
    port: String,
    credential: String,
    ss_method: String,
    transport: usize,
    sni: String,
    path: String,
    udp: bool,
    group: String,
}

impl Draft {
    fn from_profile(profile: &Profile) -> Self {
        Draft {
            name: profile.name.clone(),
            protocol: MANUAL_PROTOCOLS
                .iter()
                .position(|x| *x == profile.protocol)
                .unwrap_or(0),
            server: profile.server.clone(),
            port: profile.port.to_string(),
            credential: match &profile.credential {
                Credential::Uuid(v) | Credential::Password(v) | Credential::Token(v) => v.clone(),
                Credential::SsMethod { password, .. } => password.clone(),
                Credential::None => String::new(),
            },
            ss_method: match &profile.credential {
                Credential::SsMethod { method, .. } => method.clone(),
                _ => String::new(),
            },
            transport: MANUAL_TRANSPORTS
                .iter()
                .position(|x| *x == profile.transport)
                .unwrap_or(0),
            sni: profile.sni.clone().unwrap_or_default(),
            path: profile.path.clone().unwrap_or_default(),
            udp: profile.udp,
            group: profile.group.clone().unwrap_or_default(),
        }
    }

    fn credential_label(protocol: Protocol) -> &'static str {
        match protocol {
            Protocol::Vless | Protocol::Vmess => "UUID",
            Protocol::Trojan | Protocol::Hysteria2 => "Password",
            Protocol::Shadowsocks => "Password",
            Protocol::Socks | Protocol::Http => "User:password (optional)",
            _ => "Credential",
        }
    }
}

/// All modal dialogs.
#[derive(Clone)]
pub(crate) enum Modal {
    None,
    /// Unlock a passphrase-locked container.
    Passphrase {
        bytes: Option<Vec<u8>>,
        text: Option<String>,
        path: Option<PathBuf>,
        pass: String,
        error: Option<String>,
        label: String,
    },
    /// Security card before adding a decoded profile.
    SecurityCard {
        incoming: Incoming,
        group: String,
    },
    /// Manual add / edit form.
    Manual {
        draft: Draft,
        editing_id: Option<String>,
        error: Option<String>,
    },
    /// Export dialog.
    Export {
        profile_id: String,
        name: String,
        open_lock: bool,
        pass: String,
        pass2: String,
        expires_days: String,
        group_hint: String,
        sign: bool,
        error: Option<String>,
    },
    ConfirmDelete {
        profile_id: String,
        name: String,
    },
    /// Shown when the window is closed while connected.
    ConfirmExit,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ToastKind {
    Info,
    Ok,
    Error,
}

#[derive(Clone)]
pub struct Toast {
    kind: ToastKind,
    message: String,
    at: Instant,
}

/// The whole application state.
pub(crate) struct NpvApp {
    pub(crate) settings: Settings,
    pub(crate) bridge: Bridge,
    pub(crate) vault: VaultData,
    pub(crate) page: pages::Page,
    pub(crate) search: String,
    pub(crate) logs: String,
    pub(crate) logs_auto: bool,
    pub(crate) logs_next_refresh: Option<Instant>,
    pub(crate) toasts: Vec<Toast>,
    pub(crate) modal: Modal,
    pub(crate) ping_badges: HashMap<String, Option<u16>>,
    engine_path: Option<(Instant, Option<PathBuf>)>,
    theme_applied: ThemeMode,
    last_theme_refresh: Instant,
    requested_exit: bool,
    ctx_handle: egui::Context,
}

impl NpvApp {
    // ── helpers used by pages ────────────────────────────────────────────

    /// The profile that will be connected next (last selection or the only
    /// profile in the vault).
    pub(crate) fn selected_profile(&self) -> Option<&Profile> {
        if let Some(id) = &self.settings.last_profile_id {
            if let Some(profile) = self.vault.profiles.iter().find(|p| &p.id == id) {
                return Some(profile);
            }
        }
        if self.vault.profiles.len() == 1 {
            return self.vault.profiles.first();
        }
        None
    }

    pub(crate) fn select_profile(&mut self, id: &str) {
        self.settings.last_profile_id = Some(id.to_string());
        self.persist_settings();
    }

    pub(crate) fn request_connect_selected(&mut self) {
        if let Some(profile) = self.selected_profile().cloned() {
            let (socks, clash, dns) = (
                self.settings.socks_port,
                self.settings.clash_port,
                self.settings.dns_server.clone(),
            );
            let profile_id = profile.id.clone();
            self.bridge
                .connect(&self.ctx_handle, profile_id, socks, clash, &dns);
            if self.settings.auto_ping_on_connect {
                self.ping_profile(&profile.id);
            }
        } else {
            self.toast(
                ToastKind::Info,
                "Select a config in Configs first, or add one.",
            );
        }
    }

    pub(crate) fn ping_profile(&mut self, id: &str) {
        if let Some(profile) = self.vault.profiles.iter().find(|p| p.id == id).cloned() {
            self.bridge.ping(profile.id, profile.server, profile.port);
        }
    }

    pub(crate) fn ping_selected(&mut self) {
        if let Some(profile) = self.selected_profile().cloned() {
            self.ping_profile(&profile.id);
        }
    }

    pub(crate) fn confirm_disconnect(&mut self) {
        self.bridge.disconnect();
    }

    pub(crate) fn open_import_file(&mut self) {
        if let Some(path) = pick_open_file() {
            match std::fs::read(&path) {
                Ok(bytes) => self.ingest_import(Some(bytes), None, None, Some(path)),
                Err(err) => self.toast(ToastKind::Error, format!("Could not read file: {err}")),
            }
        }
    }

    pub(crate) fn import_from_clipboard(&mut self) {
        let text = arboard::Clipboard::new()
            .and_then(|mut cb| cb.get_text())
            .ok();
        match text {
            Some(text) if !text.trim().is_empty() => {
                self.ingest_import(None, Some(text), None, None)
            }
            _ => self.toast(ToastKind::Info, "The clipboard does not hold a config."),
        }
    }

    /// Route an import payload: decode directly, or ask for a passphrase.
    pub(crate) fn ingest_import(
        &mut self,
        bytes: Option<Vec<u8>>,
        text: Option<String>,
        passphrase: Option<String>,
        path: Option<PathBuf>,
    ) {
        // Fast header peek decides whether a passphrase dialog is needed.
        if let Some(bytes) = &bytes {
            if bytes.starts_with(npvt_core::npvt::v2::MAGIC) {
                if let Ok("passphrase") = store::peek_kind(bytes).as_deref() {
                    if passphrase.is_none() {
                        self.modal = Modal::Passphrase {
                            bytes: Some(bytes.clone()),
                            text: None,
                            path: path.clone(),
                            pass: String::new(),
                            error: None,
                            label: store::label_from_bytes_or_text(path.as_deref(), None),
                        };
                        return;
                    }
                }
            }
        }
        self.bridge.decode_import(bytes, text, passphrase, path);
    }

    pub(crate) fn open_manual_add(&mut self) {
        self.modal = Modal::Manual {
            draft: Draft::default(),
            editing_id: None,
            error: None,
        };
    }

    pub(crate) fn open_edit(&mut self, id: &str) {
        if let Some(profile) = self.vault.profiles.iter().find(|p| p.id == id) {
            self.modal = Modal::Manual {
                draft: Draft::from_profile(profile),
                editing_id: Some(profile.id.clone()),
                error: None,
            };
        }
    }

    pub(crate) fn open_export(&mut self, id: &str) {
        if let Some(profile) = self.vault.profiles.iter().find(|p| p.id == id) {
            self.modal = Modal::Export {
                profile_id: profile.id.clone(),
                name: format!("{}.npvt", profile.name.replace(' ', "_")),
                open_lock: false,
                pass: String::new(),
                pass2: String::new(),
                expires_days: String::new(),
                group_hint: profile.group.clone().unwrap_or_default(),
                sign: self.bridge.has_identity,
                error: None,
            };
        }
    }

    pub(crate) fn set_theme(&mut self, mode: ThemeMode) {
        self.settings.theme = mode;
        self.persist_settings();
        self.theme_applied = ThemeMode::Auto; // force restyle next logic pass
        self.last_theme_refresh = Instant::now() - Duration::from_secs(3600);
    }

    pub(crate) fn persist_settings(&mut self) {
        if let Err(err) = self.settings.save() {
            self.toast(ToastKind::Error, format!("Could not save settings: {err}"));
        }
    }

    pub(crate) fn refresh_logs(&mut self) {
        self.logs = npvt_core::engine::work_dir()
            .map(|dir| npvt_core::engine::log_tail(&dir.join("engine.log"), 400))
            .unwrap_or_else(|e| format!("(no data directory: {e})"));
        if self.logs == "(engine log not readable)" {
            self.logs = String::new();
        }
    }

    pub(crate) fn open_data_folder(&mut self) {
        if let Ok(dir) = npvt_core::engine::work_dir() {
            #[cfg(windows)]
            let _ = std::process::Command::new("explorer").arg(&dir).spawn();
            #[cfg(not(windows))]
            {
                let _ = &dir;
                self.toast(ToastKind::Info, dir.display().to_string());
            }
        }
    }

    /// Cached sing-box detection (refreshed at most once per second).
    pub(crate) fn engine_path_cached(&mut self) -> Option<PathBuf> {
        let now = Instant::now();
        match &self.engine_path {
            Some((at, path)) if now.duration_since(*at) < Duration::from_secs(1) => path.clone(),
            _ => {
                let path = npvt_core::engine::find_engine_binary(None);
                self.engine_path = Some((now, path.clone()));
                path
            }
        }
    }

    pub(crate) fn toast(&mut self, kind: ToastKind, message: impl Into<String>) {
        self.toasts.push(Toast {
            kind,
            message: message.into(),
            at: Instant::now(),
        });
        if self.toasts.len() > 5 {
            self.toasts.remove(0);
        }
    }

    // ── event handling ───────────────────────────────────────────────────

    fn handle_pending(&mut self, pending: Pending) {
        if let Some(res) = pending.vault {
            match res {
                Ok(data) => {
                    self.bridge.has_identity = data.identity.is_some();
                    self.vault = data;
                }
                Err(err) => self.toast(ToastKind::Error, format!("Vault: {err}")),
            }
        }
        if let Some(res) = pending.decode {
            match res {
                Ok(incoming) => {
                    self.modal = Modal::SecurityCard {
                        incoming,
                        group: String::new(),
                    };
                }
                Err(err) if err == store::PASSPHRASE_REQUIRED => {
                    self.toast(ToastKind::Info, "This config is passphrase-locked.");
                }
                Err(err) => self.toast(ToastKind::Error, format!("Import failed: {err}")),
            }
        }
        if let Some(res) = pending.import {
            match res {
                Ok((name, replaced)) => {
                    self.toast(
                        ToastKind::Ok,
                        if replaced {
                            format!("Updated '{name}'.")
                        } else {
                            format!("Added '{name}' to your library.")
                        },
                    );
                    self.modal = Modal::None;
                    self.bridge.load_vault();
                }
                Err(err) => self.toast(ToastKind::Error, format!("Import failed: {err}")),
            }
        }
        if let Some(res) = pending.export {
            match res {
                Ok(path) => {
                    self.toast(ToastKind::Ok, format!("Exported to {}", path.display()));
                    self.modal = Modal::None;
                }
                Err(err) => {
                    if let Modal::Export { error, .. } = &mut self.modal {
                        *error = Some(err);
                    } else {
                        self.toast(ToastKind::Error, format!("Export failed: {err}"));
                    }
                }
            }
        }
        if let Some(res) = pending.identity {
            match res {
                Ok(_pubkey) => {
                    self.toast(ToastKind::Ok, "Creator identity generated.");
                    self.bridge.load_vault();
                }
                Err(err) => self.toast(ToastKind::Error, format!("Identity: {err}")),
            }
        }
        // Ping results settle into badges when the ping thread is done.
        if !self.bridge.ping_in_flight {
            if let Some((profile_id, res)) = self.bridge.last_ping.clone() {
                let badge = res.as_ref().ok().copied();
                self.ping_badges.insert(profile_id, badge);
                self.bridge.last_ping = None;
            }
        }
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 680.0])
            .with_min_inner_size([900.0, 580.0])
            .with_title("NPV TUNNEL")
            .with_icon(load_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "NPV TUNNEL",
        options,
        Box::new(|cc| {
            theme::install_fonts(cc);
            let (palette, dark) = Settings::default().palette();
            theme::apply(&cc.egui_ctx, palette, dark);

            let mut app = NpvApp {
                settings: Settings::load(),
                bridge: Bridge::new(),
                vault: VaultData::default(),
                page: pages::Page::Home,
                search: String::new(),
                logs: String::new(),
                logs_auto: false,
                logs_next_refresh: None,
                toasts: Vec::new(),
                modal: Modal::None,
                ping_badges: HashMap::new(),
                engine_path: None,
                theme_applied: ThemeMode::Auto,
                last_theme_refresh: Instant::now() - Duration::from_secs(3600),
                requested_exit: false,
                ctx_handle: cc.egui_ctx.clone(),
            };
            app.bridge.load_vault();

            // Optional: auto-connect the last profile on launch.
            if app.settings.auto_connect_on_launch && app.selected_profile().is_some() {
                app.request_connect_selected();
            }
            Ok(Box::new(app))
        }),
    )
}

fn load_icon() -> egui::IconData {
    let png = include_bytes!("../../../assets/icon-256.png");
    match image::load_from_memory(png) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            egui::IconData {
                width: rgba.width(),
                height: rgba.height(),
                rgba: rgba.into_raw(),
            }
        }
        Err(_) => egui::IconData::default(),
    }
}

// ── native file dialogs (Windows) ───────────────────────────────────────

/// Native open-file dialog. The GUI ships for Windows; other platforms get
/// a stub so the workspace still builds and tests without GTK.
#[cfg(windows)]
fn pick_open_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("NPVT config", &["npvt", "npvs"])
        .add_filter("All files", &["*"])
        .set_title("Import a config file")
        .pick_file()
}

#[cfg(not(windows))]
fn pick_open_file() -> Option<PathBuf> {
    None
}

/// Native save-file dialog.
#[cfg(windows)]
fn pick_save_file(file_name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_file_name(file_name)
        .add_filter("NPVT config", &["npvt"])
        .set_title("Save the .npvt container")
        .save_file()
}

#[cfg(not(windows))]
fn pick_save_file(_file_name: &str) -> Option<PathBuf> {
    None
}

impl eframe::App for NpvApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Non-UI work: drain bridge events and refresh logs if needed.
        self.bridge.poll(ctx);
        let pending = self.bridge.take_pending();
        self.handle_pending(pending);

        // Window close while connected: ask before tearing the tunnel down.
        if ctx.input(|i| i.viewport().close_requested()) {
            let connected = matches!(self.bridge.state, EngineState::Connected(_));
            if connected && self.settings.confirm_disconnect_on_exit && !self.requested_exit {
                self.modal = Modal::ConfirmExit;
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            } else if self.requested_exit && matches!(self.bridge.state, EngineState::Connected(_))
            {
                self.bridge.disconnect();
            }
        }

        if self.logs_auto
            && self
                .logs_next_refresh
                .map(|t| Instant::now() >= t)
                .unwrap_or(true)
        {
            self.refresh_logs();
            self.logs_next_refresh = Some(Instant::now() + Duration::from_secs(2));
        }

        // Re-apply theme when the mode changes or the system setting may
        // have changed (Auto mode polls the registry every 2 s).
        if self.theme_applied != ThemeMode::Auto
            || self.settings.theme == ThemeMode::Auto
            || Instant::now().duration_since(self.last_theme_refresh) > Duration::from_secs(2)
        {
            let (palette, dark) = self.settings.palette();
            theme::apply(ctx, palette, dark);
            self.theme_applied = self.settings.theme;
            self.last_theme_refresh = Instant::now();
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let (palette_ref, _dark) = self.settings.palette();
        let p = Palette { ..*palette_ref };

        egui::Panel::left("sidebar")
            .resizable(false)
            .exact_size(196.0)
            .frame(egui::Frame::new().fill(p.panel))
            .show(ui, |ui| {
                sidebar(self, ui, &p);
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(p.bg)
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| match self.page {
                        pages::Page::Home => pages::home::show(self, ui, &p),
                        pages::Page::Configs => pages::configs::show(self, ui, &p),
                        pages::Page::Logs => pages::logs::show(self, ui, &p),
                        pages::Page::More => pages::more::show(self, ui, &p),
                    });
            });

        // Modal layer on top of everything; toasts bottom-right.
        show_modal_layer(self, ui.ctx(), &p);
        show_toasts(self, ui.ctx(), &p);
    }
}

// ── sidebar ─────────────────────────────────────────────────────────────────

fn sidebar(app: &mut NpvApp, ui: &mut Ui, p: &Palette) {
    ui.add_space(18.0);
    ui.horizontal(|ui| {
        ui.add_space(14.0);
        // Brand mini-glyph: tiny power symbol.
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(22.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let c = rect.center();
        painter.circle_stroke(c, 8.0, egui::Stroke::new(2.0, p.accent));
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::Pos2::new(c.x - 1.0, c.y - 10.0),
                egui::Pos2::new(c.x + 1.0, c.y - 1.0),
            ),
            1.0,
            p.accent,
        );
        ui.label(theme::bold("NPV TUNNEL").size(16.0).color(p.text));
    });
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.add_space(14.0);
        widgets::badge(ui, p, p.muted, &format!("v{}", env!("CARGO_PKG_VERSION")));
    });
    ui.add_space(14.0);
    ui.separator();

    // Status docked to the bottom of the sidebar.
    egui::Panel::bottom("sidebar-status")
        .exact_size(46.0)
        .frame(egui::Frame::new().fill(p.panel))
        .show(ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                let (color, text) = match &app.bridge.state {
                    EngineState::Connected(_) => (p.accent, "Tunnel up"),
                    EngineState::Connecting => (p.warn, "Connecting..."),
                    EngineState::Stopping => (p.warn, "Stopping..."),
                    EngineState::Disconnected => (p.muted, "Tunnel down"),
                };
                let (dot_rect, _) =
                    ui.allocate_exact_size(egui::Vec2::splat(8.0), egui::Sense::hover());
                ui.painter().circle_filled(dot_rect.center(), 3.5, color);
                ui.label(RichText::new(text).size(11.0).color(p.muted));
            });
        });

    let nav = [
        (pages::Page::Home, "Home", Icon::Home, "Ctrl+1"),
        (pages::Page::Configs, "Configs", Icon::Configs, "Ctrl+2"),
        (pages::Page::Logs, "Logs", Icon::Logs, "Ctrl+3"),
        (pages::Page::More, "More", Icon::More, "Ctrl+4"),
    ];
    for (page, label, icon, shortcut) in nav {
        let selected = app.page == page;
        let response = nav_item(ui, p, selected, label, icon);
        let response = response.on_hover_text(format!("Open {label} ({shortcut})"));
        if response.clicked() {
            app.page = page;
        }
    }

    ui.with_layout(Layout::top_down(Align::Min), |ui| {
        ui.add_space(8.0);
    });
}

fn nav_item(ui: &mut Ui, p: &Palette, selected: bool, label: &str, icon: Icon) -> egui::Response {
    let size = egui::Vec2::new(ui.available_width() - 20.0, 34.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.hovered() {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
    }
    let (fill, text_color, icon_color) = if selected {
        (p.accent_soft, p.accent, p.accent)
    } else if response.hovered() {
        (p.card, p.text, p.muted)
    } else {
        (Color32::TRANSPARENT, p.muted, p.muted)
    };
    ui.painter_at(rect).rect_filled(rect, 8.0, fill);
    let icon_rect = egui::Rect::from_center_size(
        egui::Pos2::new(rect.left() + 22.0, rect.center().y),
        egui::Vec2::splat(14.0),
    );
    widgets::draw_icon(&ui.painter_at(rect), icon, icon_rect, icon_color);
    ui.painter().text(
        egui::Pos2::new(rect.left() + 38.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        eframe::egui::FontId::new(13.0, FontFamily::Name("SemiBold".into())),
        text_color,
    );
    response
}

// ── modal layer ─────────────────────────────────────────────────────────────

fn show_modal_layer(app: &mut NpvApp, ctx: &egui::Context, p: &Palette) {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && !matches!(app.modal, Modal::None) {
        app.modal = Modal::None;
    }
    match app.modal.clone() {
        Modal::None => {}
        Modal::Passphrase {
            bytes,
            text,
            path,
            mut pass,
            error,
            label,
        } => {
            widgets::modal(ctx, "passphrase", 420.0, p, |ui| {
                ui.label(theme::bold("Locked config").size(17.0).color(p.text));
                ui.label(
                    RichText::new(format!(
                        "'{label}' is passphrase-locked. Enter the passphrase to continue."
                    ))
                    .size(12.0)
                    .color(p.muted),
                );
                ui.add_space(6.0);
                let field = ui.add(
                    egui::TextEdit::singleline(&mut pass)
                        .password(true)
                        .hint_text("Passphrase")
                        .desired_width(ui.available_width()),
                );
                if let Some(err) = &error {
                    ui.label(RichText::new(err).size(11.5).color(p.danger));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let enter =
                            field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if widgets::primary_button(ui, p, "Unlock").clicked() || enter {
                            app.modal = Modal::None;
                            app.ingest_import(bytes, text, Some(pass.clone()), path);
                        }
                        if widgets::ghost_button(ui, p, "Cancel").clicked() {
                            app.modal = Modal::None;
                        }
                    });
                });
            });
        }
        Modal::SecurityCard { incoming, group } => {
            widgets::modal(ctx, "security", 460.0, p, |ui| {
                ui.label(theme::bold("Security card").size(17.0).color(p.text));
                ui.label(
                    RichText::new("Review this config before it enters your library.")
                        .size(11.5)
                        .color(p.muted),
                );
                ui.add_space(6.0);
                let prof = &incoming.profile;
                egui::Grid::new("secgrid")
                    .num_columns(2)
                    .spacing([14.0, 5.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("name").size(12.0).color(p.muted));
                        ui.label(theme::semi(&prof.name).size(13.0).color(p.text));
                        ui.end_row();
                        ui.label(RichText::new("protocol").size(12.0).color(p.muted));
                        ui.label(
                            RichText::new(prof.protocol.as_str())
                                .size(13.0)
                                .color(p.text),
                        );
                        ui.end_row();
                        ui.label(RichText::new("endpoint").size(12.0).color(p.muted));
                        ui.label(
                            RichText::new(format!("{}:{}", prof.server, prof.port))
                                .family(FontFamily::Monospace)
                                .size(12.0)
                                .color(p.text),
                        );
                        ui.end_row();
                        if let Some(sni) = &prof.sni {
                            ui.label(RichText::new("sni").size(12.0).color(p.muted));
                            ui.label(RichText::new(sni).size(12.5).color(p.text));
                            ui.end_row();
                        }
                        ui.label(RichText::new("udp").size(12.0).color(p.muted));
                        ui.label(
                            RichText::new(if prof.udp { "enabled" } else { "off" })
                                .size(12.5)
                                .color(p.text),
                        );
                        ui.end_row();
                        ui.label(RichText::new("lock").size(12.0).color(p.muted));
                        ui.label(RichText::new(incoming.lock_kind).size(12.5).color(p.teal));
                        ui.end_row();
                        ui.label(RichText::new("signature").size(12.0).color(p.muted));
                        ui.label(
                            RichText::new(if incoming.verification.signed {
                                format!(
                                    "verified, creator {}",
                                    incoming.verification.creator.as_deref().unwrap_or("?")
                                )
                            } else {
                                "UNSIGNED (origin unproven)".to_string()
                            })
                            .size(12.5)
                            .color(if incoming.verification.signed {
                                p.accent
                            } else {
                                p.warn
                            }),
                        );
                        ui.end_row();
                    });
                if prof.allow_insecure {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "WARNING: this config disables TLS verification (allow_insecure).",
                        )
                        .size(11.5)
                        .color(p.danger),
                    );
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Group").size(12.5).color(p.text));
                    let mut group = group.clone();
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut group)
                            .hint_text("optional, e.g. work")
                            .desired_width(200.0),
                    );
                    if field.changed() {
                        if let Modal::SecurityCard { group: g, .. } = &mut app.modal {
                            *g = group;
                        }
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::primary_button(ui, p, "Add to library").clicked() {
                            let group = match &app.modal {
                                Modal::SecurityCard { group, .. } => {
                                    if group.trim().is_empty() {
                                        None
                                    } else {
                                        Some(group.trim().to_string())
                                    }
                                }
                                _ => None,
                            };
                            app.bridge.apply_import(incoming.clone(), None, group);
                            app.modal = Modal::None;
                        }
                        if widgets::ghost_button(ui, p, "Cancel").clicked() {
                            app.modal = Modal::None;
                        }
                    });
                });
            });
        }
        Modal::Manual {
            draft,
            editing_id,
            error,
        } => manual_modal(app, ctx, p, draft, editing_id, error),
        Modal::Export {
            profile_id,
            mut name,
            mut open_lock,
            mut pass,
            mut pass2,
            mut expires_days,
            mut group_hint,
            mut sign,
            error,
        } => {
            widgets::modal(ctx, "export", 470.0, p, |ui| {
                ui.label(theme::bold("Export configuration").size(17.0).color(p.text));
                ui.label(
                    RichText::new("Share configs on your terms: lock access and sign them.")
                        .size(11.5)
                        .color(p.muted),
                );
                ui.add_space(6.0);
                egui::Grid::new("expgrid")
                    .num_columns(2)
                    .spacing([14.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("Filename").size(12.5).color(p.text));
                        ui.add(
                            egui::TextEdit::singleline(&mut name)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(280.0),
                        );
                        ui.end_row();
                    });
                ui.add_space(4.0);
                ui.label(theme::semi("Lock this config?").size(12.5).color(p.text));
                ui.radio_value(&mut open_lock, false, "Lock it (passphrase)");
                ui.radio_value(&mut open_lock, true, "Keep it open");
                if !open_lock {
                    ui.add(
                        egui::TextEdit::singleline(&mut pass)
                            .password(true)
                            .hint_text("Passphrase (min 8 chars)")
                            .desired_width(ui.available_width()),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut pass2)
                            .password(true)
                            .hint_text("Repeat passphrase")
                            .desired_width(ui.available_width()),
                    );
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Expires in days").size(12.5).color(p.text));
                    ui.add(
                        egui::TextEdit::singleline(&mut expires_days)
                            .hint_text("optional")
                            .desired_width(90.0),
                    );
                    ui.label(RichText::new("(blank = never)").size(11.0).color(p.muted));
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Group hint").size(12.5).color(p.text));
                    ui.add(
                        egui::TextEdit::singleline(&mut group_hint)
                            .hint_text("optional")
                            .desired_width(160.0),
                    );
                });
                let sign_enabled = app.bridge.has_identity;
                ui.add_enabled(
                    sign_enabled,
                    egui::Checkbox::new(&mut sign, "Sign with my creator key"),
                );
                if !sign_enabled {
                    ui.label(
                        RichText::new("Generate an identity (More page) to sign exports.")
                            .size(11.0)
                            .color(p.muted),
                    );
                }
                if let Some(err) = &error {
                    ui.label(RichText::new(err).size(11.5).color(p.danger));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::primary_button(ui, p, "Export").clicked() {
                            submit_export(
                                app,
                                &profile_id,
                                &name,
                                open_lock,
                                &pass,
                                &pass2,
                                &expires_days,
                                &group_hint,
                                sign,
                                sign_enabled,
                            );
                        }
                        if widgets::ghost_button(ui, p, "Cancel").clicked() {
                            app.modal = Modal::None;
                        }
                    });
                });
            });
        }
        Modal::ConfirmDelete { profile_id, name } => {
            widgets::modal(ctx, "del", 380.0, p, |ui| {
                ui.label(theme::bold("Delete config").size(17.0).color(p.text));
                ui.label(
                    RichText::new(format!(
                        "Remove '{name}' from your library? This cannot be undone."
                    ))
                    .size(12.5)
                    .color(p.text),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::danger_button(ui, p, "Delete").clicked() {
                            app.modal = Modal::None;
                            delete_profile_now(app, &profile_id);
                        }
                        if widgets::ghost_button(ui, p, "Keep").clicked() {
                            app.modal = Modal::None;
                        }
                    });
                });
            });
        }
        Modal::ConfirmExit => {
            widgets::modal(ctx, "exit", 420.0, p, |ui| {
                ui.label(theme::bold("Tunnel is connected").size(17.0).color(p.text));
                ui.label(
                    RichText::new(
                        "Close NPV TUNNEL while the tunnel stays up, or stop the tunnel before quitting?",
                    )
                    .size(12.5)
                    .color(p.text),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::ghost_button(ui, p, "Cancel").clicked() {
                            app.modal = Modal::None;
                        }
                        if widgets::primary_button(ui, p, "Keep tunnel, quit").clicked() {
                            app.modal = Modal::None;
                            app.requested_exit = true;
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if widgets::danger_button(ui, p, "Stop tunnel and quit").clicked() {
                            app.modal = Modal::None;
                            app.requested_exit = true;
                            app.confirm_disconnect();
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn submit_export(
    app: &mut NpvApp,
    profile_id: &str,
    name: &str,
    open_lock: bool,
    pass: &str,
    pass2: &str,
    expires_days: &str,
    group_hint: &str,
    sign: bool,
    sign_enabled: bool,
) {
    let clean_name = name.trim().trim_end_matches(".npvt");
    let file = format!("{}.npvt", clean_name);
    let mut error: Option<String> = None;

    if clean_name.is_empty() {
        error = Some("Filename must not be empty.".to_string());
    }
    if error.is_none() && !open_lock {
        if pass.chars().count() < 8 {
            error = Some("Passphrase is too short (minimum 8 characters).".to_string());
        } else if pass != pass2 {
            error = Some("Passphrases do not match.".to_string());
        }
    }
    let expires_at = if expires_days.trim().is_empty() {
        None
    } else {
        match expires_days.trim().parse::<u64>() {
            Ok(days) if days <= 3650 => {
                Some(npvt_core::profile::unix_now() + days.saturating_mul(86_400))
            }
            _ => {
                error = Some("Expiry must be a number of days (max 3650).".to_string());
                None
            }
        }
    };

    if error.is_none() {
        if let Some(out_path) = pick_save_file(&file) {
            app.bridge.export(store::ExportRequest {
                profile_id: profile_id.to_string(),
                out_path,
                open_lock,
                passphrase: if open_lock {
                    None
                } else {
                    Some(pass.to_string())
                },
                expires_at,
                metered_only: false,
                group_hint: if group_hint.trim().is_empty() {
                    None
                } else {
                    Some(group_hint.trim().to_string())
                },
                sign: sign && sign_enabled,
            });
            return;
        }
        return;
    }
    if let Some(err) = error {
        if let Modal::Export { error, .. } = &mut app.modal {
            *error = Some(err);
        }
    }
}

fn delete_profile_now(app: &mut NpvApp, profile_id: &str) {
    if let Ok(path) = npvt_core::vault::Vault::default_path() {
        let vault = npvt_core::vault::Vault::at(path);
        let mut data = app.vault.clone();
        match crate::store::delete_profile(&vault, &mut data, profile_id) {
            Ok(name) => {
                app.vault = data;
                app.toast(ToastKind::Ok, format!("Removed '{name}'."));
                if app.settings.last_profile_id.as_deref() == Some(profile_id) {
                    app.settings.last_profile_id = None;
                    app.persist_settings();
                }
            }
            Err(err) => app.toast(ToastKind::Error, format!("Delete failed: {err}")),
        }
    }
}

fn manual_modal(
    app: &mut NpvApp,
    ctx: &egui::Context,
    p: &Palette,
    mut draft: Draft,
    editing_id: Option<String>,
    error: Option<String>,
) {
    let protocol = MANUAL_PROTOCOLS[draft.protocol.min(MANUAL_PROTOCOLS.len() - 1)];
    let transport = MANUAL_TRANSPORTS[draft.transport.min(MANUAL_TRANSPORTS.len() - 1)];
    widgets::modal(ctx, "manual", 500.0, p, |ui| {
        let title = if editing_id.is_some() {
            "Edit config"
        } else {
            "Add config manually"
        };
        ui.label(theme::bold(title).size(17.0).color(p.text));
        ui.add_space(6.0);
        egui::Grid::new("manualgrid")
            .num_columns(2)
            .spacing([14.0, 6.0])
            .show(ui, |ui| {
                ui.label(RichText::new("Name").size(12.5).color(p.text));
                ui.add(
                    egui::TextEdit::singleline(&mut draft.name)
                        .hint_text("e.g. My Shop")
                        .desired_width(300.0),
                );
                ui.end_row();

                ui.label(RichText::new("Protocol").size(12.5).color(p.text));
                egui::ComboBox::from_id_salt("proto")
                    .selected_text(protocol.as_str())
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for (i, proto) in MANUAL_PROTOCOLS.iter().enumerate() {
                            ui.selectable_value(&mut draft.protocol, i, proto.as_str());
                        }
                    });
                ui.end_row();

                ui.label(RichText::new("Server").size(12.5).color(p.text));
                ui.add(
                    egui::TextEdit::singleline(&mut draft.server)
                        .hint_text("host or IP")
                        .font(egui::TextStyle::Monospace)
                        .desired_width(300.0),
                );
                ui.end_row();

                ui.label(RichText::new("Port").size(12.5).color(p.text));
                ui.add(
                    egui::TextEdit::singleline(&mut draft.port)
                        .hint_text("443")
                        .font(egui::TextStyle::Monospace)
                        .desired_width(120.0),
                );
                ui.end_row();

                if protocol == Protocol::Shadowsocks {
                    ui.label(RichText::new("Method").size(12.5).color(p.text));
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.ss_method)
                            .hint_text("aes-256-gcm / 2022-blake3-aes-256-gcm")
                            .font(egui::TextStyle::Monospace)
                            .desired_width(300.0),
                    );
                    ui.end_row();
                }

                ui.label(
                    RichText::new(Draft::credential_label(protocol))
                        .size(12.5)
                        .color(p.text),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut draft.credential)
                        .password(true)
                        .desired_width(300.0),
                );
                ui.end_row();

                ui.label(RichText::new("Transport").size(12.5).color(p.text));
                egui::ComboBox::from_id_salt("transport")
                    .selected_text(transport_name(transport))
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for (i, tr) in MANUAL_TRANSPORTS.iter().enumerate() {
                            ui.selectable_value(&mut draft.transport, i, transport_name(*tr));
                        }
                    });
                ui.end_row();

                if matches!(transport, Transport::Tls) || protocol == Protocol::Hysteria2 {
                    ui.label(RichText::new("SNI").size(12.5).color(p.text));
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.sni)
                            .hint_text("server name (optional)")
                            .font(egui::TextStyle::Monospace)
                            .desired_width(300.0),
                    );
                    ui.end_row();
                }
                if matches!(transport, Transport::Ws | Transport::Grpc | Transport::H2) {
                    ui.label(RichText::new("Path").size(12.5).color(p.text));
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.path)
                            .hint_text("/path or service name")
                            .font(egui::TextStyle::Monospace)
                            .desired_width(300.0),
                    );
                    ui.end_row();
                }

                ui.label(RichText::new("Group").size(12.5).color(p.text));
                ui.add(
                    egui::TextEdit::singleline(&mut draft.group)
                        .hint_text("optional, e.g. work")
                        .desired_width(200.0),
                );
                ui.end_row();
            });

        ui.checkbox(&mut draft.udp, "UDP relay");
        if let Some(err) = &error {
            ui.label(RichText::new(err).size(11.5).color(p.danger));
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = if editing_id.is_some() {
                    "Save changes"
                } else {
                    "Add config"
                };
                if widgets::primary_button(ui, p, label).clicked() {
                    submit_manual(app, draft.clone(), editing_id.clone());
                }
                if widgets::ghost_button(ui, p, "Cancel").clicked() {
                    app.modal = Modal::None;
                }
            });
        });
    });
}

fn transport_name(transport: Transport) -> &'static str {
    match transport {
        Transport::Plain => "plain",
        Transport::Tls => "tls",
        Transport::Ws => "websocket",
        Transport::Grpc => "grpc",
        Transport::H2 => "h2",
    }
}

fn submit_manual(app: &mut NpvApp, draft: Draft, editing_id: Option<String>) {
    let protocol = MANUAL_PROTOCOLS[draft.protocol.min(MANUAL_PROTOCOLS.len() - 1)];
    let transport = MANUAL_TRANSPORTS[draft.transport.min(MANUAL_TRANSPORTS.len() - 1)];

    let build = || -> Result<Profile, String> {
        let port: u16 = draft
            .port
            .trim()
            .parse()
            .map_err(|_| "Port must be a number between 1 and 65535.".to_string())?;
        let credential = match protocol {
            Protocol::Shadowsocks => Credential::SsMethod {
                method: draft.ss_method.trim().to_string(),
                password: draft.credential.clone(),
            },
            Protocol::Vless | Protocol::Vmess => Credential::Uuid(draft.credential.clone()),
            Protocol::Trojan | Protocol::Hysteria2 => {
                Credential::Password(draft.credential.clone())
            }
            Protocol::Socks | Protocol::Http => {
                if draft.credential.trim().is_empty() {
                    Credential::None
                } else {
                    Credential::Token(draft.credential.clone())
                }
            }
            _ => return Err("protocol not supported by the engine yet".to_string()),
        };
        crate::store::manual_profile(
            editing_id.as_deref(),
            &draft.name,
            protocol,
            &draft.server,
            port,
            credential,
            transport,
            if draft.sni.trim().is_empty() {
                None
            } else {
                Some(draft.sni.trim().to_string())
            },
            if draft.path.trim().is_empty() {
                None
            } else {
                Some(draft.path.trim().to_string())
            },
            draft.udp,
            if draft.group.trim().is_empty() {
                None
            } else {
                Some(draft.group.trim().to_string())
            },
        )
    };

    match build() {
        Ok(profile) => {
            let mut data = app.vault.clone();
            if let Ok(path) = npvt_core::vault::Vault::default_path() {
                let vault = npvt_core::vault::Vault::at(path);
                match npvt_core::vault::upsert_profile(&mut data, profile.clone()) {
                    Ok(_) => match vault.save(&data) {
                        Ok(()) => {
                            let name = profile.name.clone();
                            app.vault = data;
                            app.modal = Modal::None;
                            app.toast(
                                ToastKind::Ok,
                                if editing_id.is_some() {
                                    format!("Saved '{name}'.")
                                } else {
                                    format!("Added '{name}' to your library.")
                                },
                            );
                        }
                        Err(err) => {
                            if let Modal::Manual { error, .. } = &mut app.modal {
                                *error = Some(err.to_string());
                            }
                        }
                    },
                    Err(err) => {
                        if let Modal::Manual { error, .. } = &mut app.modal {
                            *error = Some(err.to_string());
                        }
                    }
                }
            }
        }
        Err(err) => {
            if let Modal::Manual { error, .. } = &mut app.modal {
                *error = Some(err);
            }
        }
    }
}

// ── toasts ──────────────────────────────────────────────────────────────────

fn show_toasts(app: &mut NpvApp, ctx: &egui::Context, p: &Palette) {
    let now = Instant::now();
    app.toasts
        .retain(|t| now.duration_since(t.at) < Duration::from_secs(4));
    let toasts = app.toasts.clone();
    for (i, toast) in toasts.iter().enumerate() {
        let (color, label) = match toast.kind {
            ToastKind::Ok => (p.accent, "OK"),
            ToastKind::Info => (p.teal, "i"),
            ToastKind::Error => (p.danger, "!"),
        };
        egui::Area::new(egui::Id::unique(format!(
            "toast-{i}-{}",
            toast.at.elapsed().as_nanos()
        )))
        .order(egui::Order::Foreground)
        .anchor(
            egui::Align2::RIGHT_BOTTOM,
            [-16.0, -16.0 - (toasts.len() - 1 - i) as f32 * 52.0],
        )
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(p.panel)
                .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.6)))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(12, 9))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(label)
                                .family(FontFamily::Name("Bold".into()))
                                .size(12.0)
                                .color(color),
                        );
                        ui.label(RichText::new(&toast.message).size(12.0).color(p.text));
                    });
                });
        });
    }
}

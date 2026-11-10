//! Rendering for the NPVTunnel desktop client: a dark, professional VPN
//! layout with a dashboard, profile library, settings and dialog flows.
//!
//! egui 0.37 layout model: root `Ui` from eframe, `egui::Panel` for the
//! top bar / navigation / activity log, `egui::CentralPanel` for content,
//! and `egui::Window` dialogs anchored to the screen center.

use egui::{Align2, Color32, FontId, RichText, TextStyle};
use npvt_core::profile::unix_now;

use crate::app::{EngineState, ExportState, NpvtApp, Tab};

// ── palette ────────────────────────────────────────────────────────────────

const BG: Color32 = Color32::from_rgb(11, 17, 32);
const PANEL: Color32 = Color32::from_rgb(17, 26, 46);
const CARD: Color32 = Color32::from_rgb(23, 34, 59);
const CARD_2: Color32 = Color32::from_rgb(30, 43, 72);
const TEXT: Color32 = Color32::from_rgb(226, 232, 240);
const MUTED: Color32 = Color32::from_rgb(148, 163, 184);
const ACCENT: Color32 = Color32::from_rgb(59, 130, 246);
const ACCENT_HOVER: Color32 = Color32::from_rgb(96, 165, 250);
const OK: Color32 = Color32::from_rgb(34, 197, 94);
const WARN: Color32 = Color32::from_rgb(245, 158, 11);
const DANGER: Color32 = Color32::from_rgb(239, 68, 68);

// ── theme ──────────────────────────────────────────────────────────────────

pub(crate) fn apply_theme(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = PANEL;
        style.visuals.window_fill = CARD;
        style.visuals.extreme_bg_color = BG;
        style.visuals.faint_bg_color = CARD_2;
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.4);
        style.visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT_HOVER);
        style.visuals.hyperlink_color = ACCENT_HOVER;
        for state in [
            &mut style.visuals.widgets.noninteractive,
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.open,
        ] {
            state.corner_radius = egui::CornerRadius::same(10);
        }
        style.visuals.widgets.inactive.bg_fill = CARD_2;
        style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.2, TEXT);
        style.visuals.widgets.hovered.bg_fill = ACCENT.gamma_multiply(0.25);
        style.visuals.widgets.active.bg_fill = ACCENT.gamma_multiply(0.5);
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(14.0, 7.0);
        style.text_styles = [
            (TextStyle::Heading, FontId::proportional(25.0)),
            (TextStyle::Body, FontId::proportional(15.0)),
            (TextStyle::Button, FontId::proportional(16.0)),
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into_iter()
        .collect();
    });
}

// ── frame ──────────────────────────────────────────────────────────────────

pub(crate) fn draw(app: &mut NpvtApp, root: &mut egui::Ui) {
    top_bar(app, root);
    bottom_log(app, root);
    side_nav(app, root);
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(BG)
                .inner_margin(egui::Margin::same(18)),
        )
        .show(root, |ui| match app.tab {
            Tab::Dashboard => dashboard(app, ui),
            Tab::Profiles => profiles_tab(app, ui),
            Tab::Settings => settings_tab(app, ui),
        });
    let ctx = root.ctx().clone();
    import_window(app, &ctx);
    export_window(app, &ctx);
    remove_window(app, &ctx);
    // drive liveness ticks and uptime display
    root.ctx()
        .request_repaint_after(std::time::Duration::from_millis(500));
}

fn top_bar(app: &mut NpvtApp, root: &mut egui::Ui) {
    egui::Panel::top("npvt-top")
        .exact_size(56.0)
        .frame(
            egui::Frame::new()
                .fill(PANEL)
                .inner_margin(egui::Margin::symmetric(18, 0)),
        )
        .show(root, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(RichText::new("NPVTunnel").size(20.0).strong());
                ui.label(
                    RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                        .small()
                        .color(MUTED),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (color, label) = engine_status(app);
                status_dot(ui, color, 5.0);
                ui.label(RichText::new(label).strong());
            });
        });
}

fn side_nav(app: &mut NpvtApp, root: &mut egui::Ui) {
    egui::Panel::left("npvt-nav")
        .exact_size(190.0)
        .frame(
            egui::Frame::new()
                .fill(PANEL)
                .inner_margin(egui::Margin::same(12)),
        )
        .show(root, |ui| {
            ui.add_space(6.0);
            for (tab, label) in [
                (Tab::Dashboard, "Dashboard"),
                (Tab::Profiles, "Profiles"),
                (Tab::Settings, "Settings"),
            ] {
                let selected = app.tab == tab;
                if ui
                    .selectable_label(selected, RichText::new(label).size(16.0))
                    .clicked()
                {
                    app.tab = tab;
                }
                ui.add_space(2.0);
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("CLI companion:").small().color(MUTED));
                ui.label(RichText::new("npvtunnel").monospace().small().color(MUTED));
            });
        });
}

fn bottom_log(app: &mut NpvtApp, root: &mut egui::Ui) {
    egui::Panel::bottom("npvt-log")
        .default_size(150.0)
        .resizable(true)
        .frame(
            egui::Frame::new()
                .fill(PANEL)
                .inner_margin(egui::Margin::same(12)),
        )
        .show(root, |ui| {
            ui.label(RichText::new("Activity").strong().small().color(MUTED));
            egui::ScrollArea::vertical()
                .id_salt("npvt-log-scroll")
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for line in &app.logs {
                        ui.label(RichText::new(line).monospace().small().color(MUTED));
                    }
                });
        });
}

// ── dashboard ──────────────────────────────────────────────────────────────

fn dashboard(app: &mut NpvtApp, ui: &mut egui::Ui) {
    ui.add_space(4.0);

    egui::Frame::new()
        .fill(CARD)
        .corner_radius(16.0)
        .inner_margin(egui::Margin::same(20))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Profile").color(MUTED));
                profile_combo(app, ui);
            });

            let (color, big, detail) = dashboard_status(app);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let slack = (ui.available_width() - 240.0) / 2.0 - 10.0;
                ui.add_space(slack.max(0.0));
                status_dot(ui, color, 9.0);
                ui.label(RichText::new(big).size(30.0).strong());
            });
            if !detail.is_empty() {
                ui.label(RichText::new(detail).color(MUTED));
            }
            ui.add_space(14.0);

            let connected = matches!(
                app.engine_state,
                EngineState::Running { .. } | EngineState::Starting { .. }
            );
            let (label, fill) = if connected {
                ("DISCONNECT", DANGER)
            } else {
                ("CONNECT", OK)
            };
            let enabled = connected || app.selected_profile().is_some();
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(RichText::new(label).size(21.0).strong())
                        .min_size(egui::vec2(240.0, 54.0))
                        .corner_radius(14.0)
                        .fill(fill),
                )
                .clicked()
            {
                if connected {
                    app.disconnect();
                } else {
                    app.connect();
                }
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("SOCKS5 127.0.0.1:{}", active_port(app)))
                        .monospace()
                        .small()
                        .color(MUTED),
                );
                ui.separator();
                ui.label(
                    RichText::new(format!("DNS {}", app.dns))
                        .monospace()
                        .small()
                        .color(MUTED),
                );
                if let EngineState::Running { started_unix, .. } = app.engine_state {
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "Uptime {}",
                            fmt_uptime(unix_now().saturating_sub(started_unix))
                        ))
                        .monospace()
                        .small()
                        .color(MUTED),
                    );
                }
            });
        });

    ui.add_space(10.0);

    if let EngineState::Failed { message } = &app.engine_state {
        let message = message.clone();
        let dismiss = notice_frame(ui, DANGER, "Engine error", &message);
        if dismiss {
            app.dismiss_engine_failure();
        }
    }

    if app.engine_binary_path().is_none() {
        notice_frame(
            ui,
            WARN,
            "sing-box engine missing",
            "The connect engine was not found. Place sing-box.exe next to npvtunnel-gui.exe, in the npvtunnel data folder under bin\\, or on PATH. Downloads: https://github.com/SagerNet/sing-box/releases",
        );
    }

    if app.data.profiles.is_empty() && app.vault_error.is_none() {
        egui::Frame::new()
            .fill(CARD)
            .corner_radius(16.0)
            .inner_margin(egui::Margin::same(20))
            .show(ui, |ui| {
                ui.label(RichText::new("No profiles yet").strong());
                ui.label(
                    RichText::new("Import a .npvt/.npvs file or a share link to get started. Import never connects automatically - you always see a security card first.")
                        .color(MUTED),
                );
                ui.add_space(4.0);
                if ui.button("Open the Profiles tab").clicked() {
                    app.tab = Tab::Profiles;
                }
            });
    }

    if let Some(err) = &app.vault_error {
        let err = err.clone();
        notice_frame(ui, DANGER, "Vault problem", &err);
    }
}

fn profile_combo(app: &mut NpvtApp, ui: &mut egui::Ui) {
    let mut names: Vec<(String, String)> = app
        .data
        .profiles
        .iter()
        .map(|p| {
            (
                p.id.clone(),
                format!(
                    "{}  ({} {}:{})",
                    p.name,
                    p.protocol.as_str(),
                    p.server,
                    p.port
                ),
            )
        })
        .collect();
    names.sort_by(|a, b| a.1.cmp(&b.1));
    let selected_label = app
        .selected_profile()
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "No profile selected".into());
    egui::ComboBox::from_id_salt("npvt-profile-select")
        .width(420.0)
        .selected_text(RichText::new(selected_label).strong())
        .show_ui(ui, |ui| {
            for (id, label) in &names {
                ui.selectable_value(
                    &mut app.selected_id,
                    Some(id.clone()),
                    RichText::new(label).monospace(),
                );
            }
        });
}

fn dashboard_status(app: &NpvtApp) -> (Color32, String, String) {
    match &app.engine_state {
        EngineState::Stopped => (
            MUTED,
            "Offline".into(),
            app.selected_profile()
                .map(|p| {
                    format!(
                        "Ready to connect '{}' ({} {}:{})",
                        p.name,
                        p.protocol.as_str(),
                        p.server,
                        p.port
                    )
                })
                .unwrap_or_else(|| "Select or import a profile to begin".into()),
        ),
        EngineState::Starting { name, .. } => (WARN, "Connecting...".into(), name.clone()),
        EngineState::Running { name, port, .. } => (
            OK,
            "Protected".into(),
            format!("'{name}' via SOCKS5 127.0.0.1:{port} - localhost only"),
        ),
        EngineState::Failed { message } => {
            let first = message.lines().next().unwrap_or("error").to_string();
            (DANGER, "Error".into(), first)
        }
    }
}

fn active_port(app: &NpvtApp) -> u16 {
    match &app.engine_state {
        EngineState::Running { port, .. } | EngineState::Starting { port, .. } => *port,
        _ => app.socks_port,
    }
}

// ── profiles tab ───────────────────────────────────────────────────────────

fn profiles_tab(app: &mut NpvtApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.heading("Library");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Import file...").clicked() {
                let picked = rfd::FileDialog::new()
                    .add_filter("NPVTunnel configs", &["npvt", "npvt2", "npvs", "json"])
                    .add_filter("All files", &["*"])
                    .pick_file();
                if let Some(path) = picked {
                    app.import_file(path);
                }
            }
        });
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Link or file path:").color(MUTED));
        let response = ui.add(
            egui::TextEdit::singleline(&mut app.link_input)
                .hint_text("vless://...  |  vmess://...  |  ss://...  |  npvt://...")
                .desired_width(ui.available_width() - 110.0),
        );
        let enter = response.lost_focus() && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter));
        if ui.button("Import link").clicked() || enter {
            app.import_link();
        }
    });
    ui.add_space(6.0);

    if app.data.profiles.is_empty() {
        ui.label(
            RichText::new("The library is empty. Import a file or paste a share link above.")
                .color(MUTED),
        );
        return;
    }

    let mut rows: Vec<npvt_core::profile::Profile> = app.data.profiles.clone();
    rows.sort_by_key(|p| p.name.to_lowercase());

    egui::ScrollArea::vertical()
        .id_salt("npvt-profiles")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::new()
                .fill(CARD)
                .corner_radius(14.0)
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    egui::Grid::new("npvt-profile-grid")
                        .num_columns(5)
                        .spacing([16.0, 10.0])
                        .min_col_width(90.0)
                        .striped(true)
                        .show(ui, |ui| {
                            grid_header(ui);
                            for profile in &rows {
                                let selected =
                                    app.selected_id.as_deref() == Some(profile.id.as_str());
                                ui.horizontal(|ui| {
                                    if selected {
                                        status_dot(ui, ACCENT, 4.0);
                                    }
                                    ui.label(RichText::new(&profile.name).strong());
                                    if profile.allow_insecure {
                                        ui.label(RichText::new("TLS off!").small().color(WARN));
                                    }
                                });
                                ui.label(profile.protocol.as_str());
                                ui.label(
                                    RichText::new(format!("{}:{}", profile.server, profile.port))
                                        .monospace(),
                                );
                                ui.label(
                                    RichText::new(profile.group.as_deref().unwrap_or("-"))
                                        .color(MUTED),
                                );
                                ui.horizontal(|ui| {
                                    if ui.button("Use").clicked() {
                                        app.use_profile(profile.id.clone());
                                    }
                                    if ui.button("Export").clicked() {
                                        app.begin_export(profile.name.clone());
                                    }
                                    if ui.button(RichText::new("Remove").color(DANGER)).clicked() {
                                        app.request_remove(profile.name.clone());
                                    }
                                });
                                ui.end_row();
                            }
                        });
                });
        });
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!(
            "{} profile(s), {} known creator(s). Credentials are never displayed.",
            app.data.profiles.len(),
            app.data.known_creators.len()
        ))
        .small()
        .color(MUTED),
    );
}

fn grid_header(ui: &mut egui::Ui) {
    for title in ["Name", "Protocol", "Endpoint", "Group", "Actions"] {
        ui.label(RichText::new(title).strong().color(MUTED));
    }
    ui.end_row();
}

// ── settings tab ───────────────────────────────────────────────────────────

fn settings_tab(app: &mut NpvtApp, ui: &mut egui::Ui) {
    ui.heading("Settings");
    ui.add_space(4.0);

    egui::Frame::new()
        .fill(CARD)
        .corner_radius(14.0)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.label(RichText::new("Engine").strong());
            ui.horizontal(|ui| {
                ui.label("Local SOCKS5 port:");
                ui.add(
                    egui::DragValue::new(&mut app.socks_port)
                        .range(1..=65535)
                        .speed(1.0),
                );
                ui.label(RichText::new("(binds 127.0.0.1 only)").small().color(MUTED));
            });
            ui.horizontal(|ui| {
                ui.label("DNS server:");
                ui.add(egui::TextEdit::singleline(&mut app.dns).desired_width(160.0));
            });
            ui.horizontal(|ui| {
                ui.label("sing-box path:");
                let mut shown = app
                    .engine_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                let response = ui.add(
                    egui::TextEdit::singleline(&mut shown)
                        .hint_text("auto: next to the app, data bin\\ folder, or PATH")
                        .desired_width(ui.available_width() - 170.0),
                );
                if response.changed() {
                    app.engine_path = if shown.trim().is_empty() {
                        None
                    } else {
                        Some(std::path::PathBuf::from(shown))
                    };
                }
                if ui.button("Browse...").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        app.engine_path = Some(path);
                    }
                }
            });
            match app.engine_binary_path() {
                Some(path) => {
                    ui.label(
                        RichText::new(format!("Resolved engine: {}", path.display()))
                            .small()
                            .color(OK),
                    );
                }
                None => {
                    ui.label(
                        RichText::new(
                            "No engine found - connect is unavailable until sing-box is installed.",
                        )
                        .small()
                        .color(WARN),
                    );
                }
            }
        });

    ui.add_space(6.0);
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(14.0)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.label(RichText::new("Creator identity").strong());
            match &app.data.identity {
                Some(_) => {
                    ui.label(
                        RichText::new(
                            "Present - exports are signed and key-locked files can be opened.",
                        )
                        .color(OK),
                    );
                }
                None => {
                    ui.label(
                        RichText::new("No identity yet. It signs your exports and unlocks files locked to you.")
                            .color(MUTED),
                    );
                    if ui.button("Generate identity").clicked() {
                        app.generate_identity();
                    }
                }
            }
        });

    ui.add_space(6.0);
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(14.0)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.label(RichText::new("Diagnostics").strong());
            match &app.vault {
                Some(vault) => diag_row(
                    ui,
                    OK,
                    format!("vault readable at {}", vault.path().display()),
                ),
                None => diag_row(ui, DANGER, "vault unavailable".to_string()),
            }
            match &app.data.identity {
                Some(_) => diag_row(ui, OK, "identity present".to_string()),
                None => diag_row(
                    ui,
                    WARN,
                    "no identity (export signing disabled)".to_string(),
                ),
            }
            match app.engine_binary_path() {
                Some(path) => diag_row(ui, OK, format!("engine binary: {}", path.display())),
                None => diag_row(ui, WARN, "engine binary: not found".to_string()),
            }
            match &app.engine_state {
                EngineState::Running { pid, name, .. } => diag_row(
                    ui,
                    OK,
                    format!("engine running (pid {pid}, profile '{name}')"),
                ),
                EngineState::Starting { .. } => diag_row(ui, WARN, "engine starting".to_string()),
                EngineState::Failed { message } => diag_row(
                    ui,
                    DANGER,
                    message.lines().next().unwrap_or("engine error").to_string(),
                ),
                EngineState::Stopped => diag_row(ui, MUTED, "engine not running".to_string()),
            }
        });

    ui.add_space(6.0);
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(14.0)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.label(RichText::new("About").strong());
            ui.label(
                RichText::new(format!(
                    "NPVTunnel v{} - security-hardened .npvt client",
                    env!("CARGO_PKG_VERSION")
                ))
                .color(MUTED),
            );
            ui.hyperlink_to(
                RichText::new("github.com/librewolfsigma/npvtunnel").color(ACCENT_HOVER),
                "https://github.com/librewolfsigma/npvtunnel",
            );
            ui.label(
                RichText::new("Closing the window keeps a running tunnel alive; disconnect first to stop it. The engine binds 127.0.0.1 only - nothing is exposed to the network.")
                    .small()
                    .color(MUTED),
            );
        });
}

fn diag_row(ui: &mut egui::Ui, color: Color32, text: String) {
    ui.horizontal(|ui| {
        status_dot(ui, color, 3.5);
        ui.label(RichText::new(text).monospace().small());
    });
}

// ── dialogs ────────────────────────────────────────────────────────────────

fn import_window(app: &mut NpvtApp, ctx: &egui::Context) {
    if app.import.is_none() {
        return;
    }
    let mut open = true;
    egui::Window::new(RichText::new("Import profile").strong())
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .min_width(520.0)
        .show(ctx, |ui| {
            let mut unlock = false;
            let mut commit = false;
            let mut enter_pass = false;
            if let Some(state) = app.import.as_mut() {
                ui.label(
                    RichText::new(format!("Source: {}", state.source_label))
                        .small()
                        .color(MUTED),
                );
                if let Some(err) = &state.error {
                    ui.label(RichText::new(err).color(DANGER));
                }
                match &state.card {
                    None => {
                        if state.lock_kind == "passphrase" {
                            ui.label(
                                "This container is passphrase-locked. Enter the passphrase to see its security card.",
                            );
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut state.passphrase)
                                    .password(true)
                                    .desired_width(360.0),
                            );
                            enter_pass = response.lost_focus()
                                && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter));
                            ui.horizontal(|ui| {
                                unlock |= ui.button("Unlock").clicked();
                                if ui.button("Cancel").clicked() {
                                    app.cancel_import();
                                }
                            });
                        }
                        // otherwise: only an error is shown and the user can close
                    }
                    Some(card) => {
                        ui.add_space(4.0);
                        ui.label(RichText::new("Security card").strong());
                        egui::Grid::new("npvt-card-grid")
                            .num_columns(2)
                            .spacing([14.0, 6.0])
                            .show(ui, |ui| {
                                card_row(ui, "name", &card.profile.name);
                                card_row(
                                    ui,
                                    "protocol",
                                    &format!(
                                        "{} ({})",
                                        card.profile.protocol.as_str(),
                                        match card.profile.transport {
                                            npvt_core::profile::Transport::Plain => "plain",
                                            npvt_core::profile::Transport::Tls => "tls",
                                            npvt_core::profile::Transport::Ws => "websocket",
                                            npvt_core::profile::Transport::Grpc => "grpc",
                                            npvt_core::profile::Transport::H2 => "h2",
                                        }
                                    ),
                                );
                                card_row(
                                    ui,
                                    "endpoint",
                                    &format!("{}:{}", card.profile.server, card.profile.port),
                                );
                                if let Some(sni) = &card.profile.sni {
                                    card_row(ui, "sni", sni);
                                }
                                card_row(ui, "lock", &card.lock_kind);
                                if card.signed {
                                    card_row(
                                        ui,
                                        "signature",
                                        &format!(
                                            "verified, creator {}",
                                            card.creator.as_deref().unwrap_or("?")
                                        ),
                                    );
                                } else {
                                    card_row(ui, "signature", "UNSIGNED (origin unproven)");
                                }
                                if let Some(exp) = card.expires_at {
                                    card_row(ui, "expires", &fmt_date(exp));
                                }
                                if card.metered_only {
                                    card_row(ui, "policy", "metered connections only");
                                }
                            });
                        if card.profile.allow_insecure {
                            ui.label(
                                RichText::new("WARNING: this config disables TLS verification (allow_insecure).")
                                    .color(WARN),
                            );
                        }
                        ui.add_space(6.0);
                        egui::Grid::new("npvt-import-override")
                            .num_columns(2)
                            .spacing([14.0, 6.0])
                            .show(ui, |ui| {
                                ui.label("Name:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut state.name_override)
                                        .hint_text(&card.profile.name)
                                        .desired_width(280.0),
                                );
                                ui.end_row();
                                ui.label("Group:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut state.group)
                                        .hint_text("optional")
                                        .desired_width(280.0),
                                );
                                ui.end_row();
                            });
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            commit |= ui
                                .button(RichText::new("Add to library").strong())
                                .clicked();
                            if ui.button("Cancel").clicked() {
                                app.cancel_import();
                            }
                        });
                    }
                }
            }
            if unlock || enter_pass {
                app.import_unlock();
            }
            if commit {
                app.import_commit();
            }
        });
    if !open {
        app.cancel_import();
    }
}

fn export_window(app: &mut NpvtApp, ctx: &egui::Context) {
    if app.export.is_none() {
        return;
    }
    let mut open = true;
    egui::Window::new(RichText::new("Export locked container").strong())
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .min_width(520.0)
        .show(ctx, |ui| {
            let mut pick = false;
            let mut commit = false;
            if let Some(ExportState {
                target,
                passphrase,
                repeat,
                expires,
                out,
                error,
            }) = app.export.as_mut()
            {
                ui.label(
                    RichText::new(format!("Profile: {target}"))
                        .small()
                        .color(MUTED),
                );
                if let Some(err) = error {
                    ui.label(RichText::new(err.clone()).color(DANGER));
                }
                egui::Grid::new("npvt-export-grid")
                    .num_columns(2)
                    .spacing([14.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Passphrase:");
                        ui.add(
                            egui::TextEdit::singleline(passphrase)
                                .password(true)
                                .desired_width(300.0),
                        );
                        ui.end_row();
                        ui.label("Repeat:");
                        ui.add(
                            egui::TextEdit::singleline(repeat)
                                .password(true)
                                .desired_width(300.0),
                        );
                        ui.end_row();
                        ui.label("Expires:");
                        ui.add(
                            egui::TextEdit::singleline(expires)
                                .hint_text("e.g. +30d or 2027-01-31 (optional)")
                                .desired_width(300.0),
                        );
                        ui.end_row();
                        ui.label("Output:");
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(
                                    out.as_ref()
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|| "<not chosen>".into()),
                                )
                                .small(),
                            );
                            pick = ui.button("Choose...").clicked();
                        });
                        ui.end_row();
                    });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    commit |= ui.button(RichText::new("Export").strong()).clicked();
                    if ui.button("Cancel").clicked() {
                        app.cancel_export();
                    }
                });
                ui.label(
                    RichText::new("The file is locked with Argon2id + XChaCha20-Poly1305 and signed with your creator identity.")
                        .small()
                        .color(MUTED),
                );
            }
            if pick {
                app.export_pick_file();
            }
            if commit {
                app.export_commit();
            }
        });
    if !open {
        app.cancel_export();
    }
}

fn remove_window(app: &mut NpvtApp, ctx: &egui::Context) {
    let Some(target) = app.confirm_remove.clone() else {
        return;
    };
    let mut open = true;
    egui::Window::new(RichText::new("Remove profile").strong())
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(format!("Remove '{target}' from the library?"));
            ui.label(
                RichText::new("This does not touch any exported or shared files.")
                    .small()
                    .color(MUTED),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .button(RichText::new("Remove").color(DANGER).strong())
                    .clicked()
                {
                    app.confirm_remove();
                }
                if ui.button("Cancel").clicked() {
                    app.confirm_remove = None;
                }
            });
        });
    if !open {
        app.confirm_remove = None;
    }
}

// ── helpers ────────────────────────────────────────────────────────────────

fn engine_status(app: &NpvtApp) -> (Color32, String) {
    match &app.engine_state {
        EngineState::Stopped => (MUTED, "Offline".into()),
        EngineState::Starting { .. } => (WARN, "Connecting...".into()),
        EngineState::Running { .. } => (OK, "Protected".into()),
        EngineState::Failed { .. } => (DANGER, "Error".into()),
    }
}

fn status_dot(ui: &mut egui::Ui, color: Color32, radius: f32) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(radius * 2.0 + 2.0, radius * 2.0 + 2.0),
        egui::Sense::hover(),
    );
    ui.painter().circle_filled(rect.center(), radius, color);
}

fn notice_frame(ui: &mut egui::Ui, color: Color32, title: &str, body: &str) -> bool {
    let mut dismissed = false;
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .corner_radius(12.0)
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).strong().color(color));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    if ui.small_button("Dismiss").clicked() {
                        dismissed = true;
                    }
                });
            });
            ui.label(RichText::new(body).small());
        });
    dismissed
}

fn card_row(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.label(RichText::new(key).color(MUTED));
    ui.label(RichText::new(value).monospace());
    ui.end_row();
}

fn fmt_uptime(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}h {m:02}m {s:02}s")
    } else {
        format!("{m}m {s:02}s")
    }
}

/// Unix timestamp to `YYYY-MM-DD` (UTC), via Howard Hinnant's civil_from_days.
fn fmt_date(unix: u64) -> String {
    let z = (unix / 86_400) as i64 + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

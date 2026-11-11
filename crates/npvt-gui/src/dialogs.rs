//! Every modal dialog, including the "Add config" menu of reference image 4:
//! subscription, clipboard, QR code, .npvs/.npvt file import, manual config
//! and new connection group. Plus Premium, Points, Settings, Per-app Proxy,
//! My public key, User assets and the confirmations.
//!
//! Pattern: each dialog keeps its draft fields in the `Dialog` variant; the
//! closure mutates the local draft and the end of the function writes the
//! updated draft back unless the dialog decided to close.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use egui::Color32;

use npvt_core::profile::{new_profile_id, unix_now, Credential, Profile, Protocol, Transport};
use npvt_core::uri;
use npvt_core::vault::upsert_profile;

use crate::app::{App, Dialog, KebabKind, ManualDraft, SortKey};
use crate::data::{self, import_profiles, Group, GroupMode};
use crate::icons::{self, Icon};
use crate::net::{self, DownloadState, GEOIP_URL, GEOSITE_URL};
use crate::theme::{self, Palette};
use crate::widgets::{fill_button, menu_item, modal, modal_header, radio_row, tracked_label};

pub fn show(app: &mut App, ctx: &egui::Context) {
    let p = app.palette;
    if matches!(app.dialog, Dialog::Closed) {
        return;
    }
    let dialog = std::mem::take(&mut app.dialog);
    app.dialog = draw_dialog(app, ctx, &p, dialog);
}

fn draw_dialog(app: &mut App, ctx: &egui::Context, p: &Palette, dialog: Dialog) -> Dialog {
    match dialog {
        Dialog::Closed => Dialog::Closed,
        Dialog::AddMenu => add_menu(app, ctx, p),
        Dialog::AddSubscription { url, busy } => add_subscription(app, ctx, p, url, busy),
        Dialog::Manual(draft) => manual_dialog(app, ctx, p, draft, None),
        Dialog::EditProfile { profile_id, draft } => {
            manual_dialog(app, ctx, p, draft, Some(profile_id))
        }
        Dialog::NewGroup { name, mode } => new_group(app, ctx, p, name, mode),
        Dialog::EditGroup {
            group_id,
            name,
            mode,
            members,
        } => edit_group(app, ctx, p, &group_id, name, mode, members),
        Dialog::Rename { group_id, name } => rename_dialog(app, ctx, p, &group_id, name),
        Dialog::ConfirmDeleteGroup { group_id } => confirm_delete_group(app, ctx, p, &group_id),
        Dialog::ConfirmDeleteProfile { profile_id } => {
            confirm_delete_profile(app, ctx, p, &profile_id)
        }
        Dialog::ImportFile {
            path,
            lock_kind,
            passphrase,
            error,
        } => import_file(app, ctx, p, &path, &lock_kind, passphrase, error),
        Dialog::Premium => premium(app, ctx, p),
        Dialog::Points { watching } => points(app, ctx, p, watching),
        Dialog::SettingsDlg => settings_dialog(app, ctx, p),
        Dialog::PerApp { mode, processes } => per_app(app, ctx, p, mode, processes),
        Dialog::PublicKey => public_key(app, ctx, p),
        Dialog::UserAssets {
            geoip,
            geosite,
            geoip_cancel,
            geosite_cancel,
        } => user_assets(app, ctx, p, geoip, geosite, geoip_cancel, geosite_cancel),
        Dialog::Kebab { group_id, kind } => kebab_menu(app, ctx, p, &group_id, kind),
        Dialog::SortMenu => sort_menu(app, ctx, p),
        Dialog::EngineMissing { message } => engine_missing(app, ctx, p, &message),
    }
}

// ---- Add config menu (image 4) -----------------------------------------------

fn add_menu(app: &mut App, ctx: &egui::Context, p: &Palette) -> Dialog {
    let mut next = Dialog::Closed;
    modal(ctx, p, "add-menu", 350.0, |ui| {
        ui.label(theme::sans_bold("Add config", 19.0, p.text));
        ui.add_space(8.0);

        menu_item(ui, p, Icon::Rss, "Add subscription", || {
            next = Dialog::AddSubscription {
                url: String::new(),
                busy: false,
            };
        });
        menu_item(
            ui,
            p,
            Icon::Clipboard,
            "Import config from Clipboard",
            || {
                import_clipboard(app);
            },
        );
        menu_item(ui, p, Icon::Qr, "Import config from QR Code", || {
            import_qr(app);
        });
        menu_item(
            ui,
            p,
            Icon::Download,
            "Import config file (.npvs / .npvt)",
            || {
                pick_config_file(app);
            },
        );
        menu_item(ui, p, Icon::Plus, "Add config manually", || {
            next = Dialog::Manual(ManualDraft::new());
        });
        menu_item(ui, p, Icon::Cluster, "New connection group", || {
            next = Dialog::NewGroup {
                name: String::new(),
                mode: 0,
            };
        });

        ui.add_space(6.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(theme::mono_med("Cancel", 13.0, p.green))
                        .fill(Color32::TRANSPARENT)
                        .stroke(egui::Stroke::NONE),
                )
                .clicked()
            {
                next = Dialog::Closed;
            }
        });
    });
    next
}

// ---- Import implementations ----------------------------------------------------

fn import_clipboard(app: &mut App) {
    let text = match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
        Ok(t) => t,
        Err(e) => {
            app.toasts.error(format!("Clipboard unavailable: {e}"));
            return;
        }
    };
    let mut profiles = Vec::new();
    let mut errors = 0usize;
    for line in data::decode_subscription(&text) {
        match uri::parse_import(&line) {
            Ok(uri::ImportSource::Profile(prof)) => profiles.push(prof),
            _ => errors += 1,
        }
    }
    let report = import_profiles(&mut app.data, profiles);
    app.save_all();
    if report.added + report.replaced > 0 {
        app.toasts.show(report.summary());
    } else {
        app.toasts
            .error(format!("Nothing importable found ({errors} invalid)"));
    }
}

fn import_qr(app: &mut App) {
    let Some(path) = crate::pick::pick_image() else {
        return;
    };
    let img = match image::open(&path) {
        Ok(img) => img,
        Err(e) => {
            app.toasts.error(format!("Cannot read image: {e}"));
            return;
        }
    };
    let luma = img.to_luma8();
    let mut profiles = Vec::new();
    let mut prepared = rqrr::PreparedImage::prepare(luma);
    for grid in prepared.detect_grids() {
        if let Ok((_, content)) = grid.decode() {
            if let Ok(uri::ImportSource::Profile(prof)) = uri::parse_import(&content) {
                profiles.push(prof);
            }
        }
    }
    let report = import_profiles(&mut app.data, profiles);
    app.save_all();
    if report.added + report.replaced > 0 {
        app.toasts.show(report.summary());
    } else {
        app.toasts.error("No usable QR code found in that image");
    }
}

fn pick_config_file(app: &mut App) {
    let Some(path) = crate::pick::pick_config() else {
        return;
    };
    open_config_file(app, &path);
}

/// Read a config file and import it, asking for a passphrase when needed.
pub fn open_config_file(app: &mut App, path: &std::path::Path) {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            app.toasts.error(format!("Cannot read file: {e}"));
            return;
        }
    };
    if bytes.starts_with(npvt_core::npvt::v2::MAGIC) {
        let lock_kind = npvt_core::npvt::v2::peek_lock(&bytes)
            .map(|s| s.to_string())
            .unwrap_or_else(|e| format!("error: {e}"));
        if lock_kind == "open" {
            decode_container(app, &bytes, None);
        } else {
            app.dialog = Dialog::ImportFile {
                path: path.display().to_string(),
                lock_kind,
                passphrase: String::new(),
                error: None,
            };
        }
        return;
    }
    // Legacy v1 or a plain text link / JSON profile.
    match npvt_core::npvt::v1::import_best_effort(&bytes) {
        Ok(prof) => {
            let report = import_profiles(&mut app.data, vec![prof]);
            app.save_all();
            app.toasts.show(report.summary());
        }
        Err(_) => {
            let text = String::from_utf8_lossy(&bytes);
            let mut profiles = Vec::new();
            for line in data::decode_subscription(&text) {
                if let Ok(uri::ImportSource::Profile(prof)) = uri::parse_import(&line) {
                    profiles.push(prof);
                }
            }
            if profiles.is_empty() {
                if let Ok(uri::ImportSource::Profile(prof)) = uri::parse_import(text.trim()) {
                    profiles.push(prof);
                }
            }
            let report = import_profiles(&mut app.data, profiles);
            app.save_all();
            if report.added + report.replaced > 0 {
                app.toasts.show(report.summary());
            } else {
                app.toasts.error("Unsupported file format");
            }
        }
    }
}

fn decode_container(app: &mut App, bytes: &[u8], passphrase: Option<&str>) -> bool {
    let unlock = match passphrase {
        Some(p) => npvt_core::npvt::v2::Unlock::Passphrase(p),
        None => npvt_core::npvt::v2::Unlock::Any,
    };
    match npvt_core::npvt::v2::decode(bytes, unlock) {
        Ok(decoded) => {
            let mut prof = decoded.profile;
            if let Some(hint) = &decoded.policy.group_hint {
                if prof.group.is_none() {
                    prof.group = Some(hint.clone());
                }
            }
            let creator = decoded
                .verification
                .creator
                .clone()
                .unwrap_or_else(|| "unsigned".into());
            let report = import_profiles(&mut app.data, vec![prof]);
            app.save_all();
            app.toasts.show(format!(
                "{} · creator {}",
                report.summary(),
                &creator[..creator.len().min(12)]
            ));
            true
        }
        Err(npvt_core::Error::CryptoAuth) => {
            app.toasts
                .error("Unlock failed: wrong passphrase or tampered file");
            false
        }
        Err(e) => {
            app.toasts.error(format!("Import failed: {e}"));
            false
        }
    }
}

// ---- Subscription dialog -----------------------------------------------------

fn add_subscription(
    app: &mut App,
    ctx: &egui::Context,
    p: &Palette,
    url: String,
    _busy: bool,
) -> Dialog {
    let mut url = url;
    let mut next = Dialog::AddSubscription {
        url: url.clone(),
        busy: false,
    };
    modal(ctx, p, "subscription", 380.0, |ui| {
        if modal_header(ui, p, "Add subscription", Some("Cancel")) {
            next = Dialog::Closed;
            return;
        }
        ui.label(theme::sans(
            "Fetch a list of config links from a URL.",
            11.5,
            p.text_dim,
        ));
        ui.add_space(6.0);
        ui.add(
            egui::TextEdit::singleline(&mut url)
                .hint_text("https://example.com/sub")
                .desired_width(ui.available_width())
                .font(egui::TextStyle::Monospace),
        );
        ui.add_space(8.0);
        if fill_button(ui, p, "Fetch and import", !url.trim().is_empty()).clicked() {
            let target = url.trim().to_string();
            match net::fetch_text(&target) {
                Ok(body) => {
                    let lines = data::decode_subscription(&body);
                    let mut profiles = Vec::new();
                    let mut locked = 0usize;
                    for line in lines {
                        match uri::parse_import(&line) {
                            Ok(uri::ImportSource::Profile(prof)) => profiles.push(prof),
                            Ok(uri::ImportSource::Container(_)) => locked += 1,
                            Err(_) => {}
                        }
                    }
                    let report = import_profiles(&mut app.data, profiles);
                    app.save_all();
                    if report.added + report.replaced > 0 {
                        app.toasts
                            .show(format!("{} ({locked} locked skipped)", report.summary()));
                        next = Dialog::Closed;
                    } else {
                        app.toasts
                            .error("The subscription contained no importable links");
                    }
                }
                Err(e) => app.toasts.error(format!("Fetch failed: {e}")),
            }
        }
    });
    match next {
        Dialog::Closed => Dialog::Closed,
        _ => Dialog::AddSubscription {
            url: url.clone(),
            busy: false,
        },
    }
}

// ---- Manual config dialog ------------------------------------------------------

const TRANSPORTS: [&str; 5] = ["plain", "tls", "ws", "grpc", "h2"];

fn manual_dialog(
    app: &mut App,
    ctx: &egui::Context,
    p: &Palette,
    draft: ManualDraft,
    editing: Option<String>,
) -> Dialog {
    let mut draft = draft;
    let mut closed = false;
    // created_at is preserved when editing an existing profile.
    let created_at = editing
        .as_deref()
        .and_then(|id| app.data.profiles.iter().find(|prof| prof.id == id))
        .map(|prof| prof.created_at)
        .unwrap_or_else(unix_now);
    let title = if editing.is_some() {
        "Edit config"
    } else {
        "Add config manually"
    };
    modal(ctx, p, "manual", 420.0, |ui| {
        if modal_header(ui, p, title, Some("Cancel")) {
            closed = true;
            return;
        }
        let w = ui.available_width();
        field(ui, p, "Name", &mut draft.name, w);
        ui.horizontal(|ui| {
            ui.label(theme::sans_med("Protocol", 12.0, p.text_dim));
            egui::ComboBox::from_id_salt("proto")
                .selected_text(Protocol::ALL[draft.protocol_idx].as_str())
                .show_ui(ui, |ui| {
                    for (i, proto) in Protocol::ALL.iter().enumerate() {
                        ui.selectable_value(&mut draft.protocol_idx, i, proto.as_str());
                    }
                });
            ui.label(theme::sans_med("Transport", 12.0, p.text_dim));
            egui::ComboBox::from_id_salt("transport")
                .selected_text(TRANSPORTS[draft.transport_idx])
                .show_ui(ui, |ui| {
                    for (i, t) in TRANSPORTS.iter().enumerate() {
                        ui.selectable_value(&mut draft.transport_idx, i, (*t).to_string());
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label(theme::sans_med("Server", 12.0, p.text_dim));
            ui.add(
                egui::TextEdit::singleline(&mut draft.server)
                    .desired_width(w * 0.5)
                    .font(egui::TextStyle::Monospace),
            );
            ui.label(theme::sans_med("Port", 12.0, p.text_dim));
            ui.add(
                egui::TextEdit::singleline(&mut draft.port)
                    .desired_width(64.0)
                    .font(egui::TextStyle::Monospace),
            );
        });
        field(
            ui,
            p,
            "Credential (UUID / password / user:pass)",
            &mut draft.cred,
            w,
        );
        let proto = Protocol::ALL[draft.protocol_idx];
        if proto == Protocol::Shadowsocks {
            field(ui, p, "Cipher (e.g. aes-256-gcm)", &mut draft.ss_method, w);
        }
        if draft.transport_idx >= 1 {
            field(ui, p, "TLS SNI / host", &mut draft.sni, w);
            if draft.transport_idx >= 2 {
                field(ui, p, "Path / service name", &mut draft.path, w);
            }
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut draft.udp, "UDP relay");
            ui.checkbox(&mut draft.insecure, "Skip TLS verify");
        });
        ui.add_space(6.0);
        if fill_button(
            ui,
            p,
            if editing.is_some() {
                "Save changes"
            } else {
                "Add config"
            },
            true,
        )
        .clicked()
        {
            match build_profile(&draft, editing.as_deref(), created_at) {
                Ok(prof) => {
                    let _ = upsert_profile(&mut app.data, prof);
                    app.save_all();
                    app.toasts.show("Config saved");
                    closed = true;
                }
                Err(e) => app.toasts.error(format!("Invalid: {e}")),
            }
        }
    });
    if closed {
        Dialog::Closed
    } else {
        Dialog::Manual(draft)
    }
}

fn field(ui: &mut egui::Ui, p: &Palette, label: &str, value: &mut String, width: f32) {
    ui.label(theme::sans_med(label, 12.0, p.text_dim));
    ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(width)
            .font(egui::TextStyle::Monospace),
    );
    ui.add_space(2.0);
}

fn build_profile(
    draft: &ManualDraft,
    editing: Option<&str>,
    created_at: u64,
) -> Result<Profile, String> {
    let port: u16 = draft
        .port
        .trim()
        .parse()
        .map_err(|_| "port must be a number".to_string())?;
    let transport = match draft.transport_idx {
        1 => Transport::Tls,
        2 => Transport::Ws,
        3 => Transport::Grpc,
        4 => Transport::H2,
        _ => Transport::Plain,
    };
    let protocol = Protocol::ALL[draft.protocol_idx];
    let credential = match protocol {
        Protocol::Shadowsocks => Credential::SsMethod {
            method: if draft.ss_method.trim().is_empty() {
                "aes-256-gcm".into()
            } else {
                draft.ss_method.trim().to_string()
            },
            password: draft.cred.trim().to_string(),
        },
        Protocol::Vless | Protocol::Vmess => Credential::Uuid(draft.cred.trim().to_string()),
        Protocol::Socks | Protocol::Http => {
            if draft.cred.trim().is_empty() {
                Credential::None
            } else {
                Credential::Token(draft.cred.trim().to_string())
            }
        }
        _ => {
            if draft.cred.trim().is_empty() {
                return Err("this protocol needs a password".into());
            }
            Credential::Password(draft.cred.trim().to_string())
        }
    };
    let prof = Profile {
        id: editing.map(String::from).unwrap_or_else(new_profile_id),
        name: draft.name.trim().to_string(),
        protocol,
        server: draft.server.trim().to_string(),
        port,
        credential,
        transport,
        sni: if draft.sni.trim().is_empty() {
            None
        } else {
            Some(draft.sni.trim().to_string())
        },
        path: if draft.path.trim().is_empty() {
            None
        } else {
            Some(draft.path.trim().to_string())
        },
        allow_insecure: draft.insecure,
        udp: draft.udp,
        group: None,
        created_at,
    };
    prof.validate().map_err(|e| e.to_string())?;
    Ok(prof)
}

// ---- Group dialogs -------------------------------------------------------------

fn new_group(app: &mut App, ctx: &egui::Context, p: &Palette, name: String, mode: usize) -> Dialog {
    let mut name = name;
    let mut mode = mode;
    let mut closed = false;
    modal(ctx, p, "newgroup", 380.0, |ui| {
        if modal_header(ui, p, "New connection group", Some("Cancel")) {
            closed = true;
            return;
        }
        ui.add(
            egui::TextEdit::singleline(&mut name)
                .hint_text("Group name")
                .desired_width(ui.available_width()),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for (i, label) in ["Fastest", "Round robin", "Manual"].iter().enumerate() {
                if ui
                    .selectable_label(
                        mode == i,
                        theme::sans_med(
                            (*label).to_string(),
                            12.5,
                            if mode == i { p.green } else { p.text_dim },
                        ),
                    )
                    .clicked()
                {
                    mode = i;
                }
            }
        });
        ui.add_space(8.0);
        if fill_button(ui, p, "Create group", !name.trim().is_empty()).clicked() {
            let group_mode = match mode {
                1 => GroupMode::RoundRobin,
                2 => GroupMode::Manual,
                _ => GroupMode::Fastest,
            };
            app.groups.groups.push(Group {
                id: new_profile_id(),
                name: name.trim().to_string(),
                mode: group_mode,
                members: Vec::new(),
                pinned: false,
            });
            app.groups.active_group = None;
            app.groups.ensure_defaults();
            app.groups.save();
            app.toasts.show("Group created");
            closed = true;
        }
    });
    if closed {
        Dialog::Closed
    } else {
        Dialog::NewGroup { name, mode }
    }
}

fn edit_group(
    app: &mut App,
    ctx: &egui::Context,
    p: &Palette,
    group_id: &str,
    name: String,
    mode: usize,
    members: Vec<String>,
) -> Dialog {
    let mut name = name;
    let mut mode = mode;
    let mut members = members;
    let mut closed = false;
    let profiles: Vec<Profile> = app.data.profiles.clone();
    modal(ctx, p, "editgroup", 420.0, |ui| {
        if modal_header(ui, p, "Edit group", Some("Cancel")) {
            closed = true;
            return;
        }
        ui.add(
            egui::TextEdit::singleline(&mut name)
                .hint_text("Group name")
                .desired_width(ui.available_width()),
        );
        ui.horizontal(|ui| {
            for (i, label) in ["Fastest", "Round robin", "Manual"].iter().enumerate() {
                if ui
                    .selectable_label(
                        mode == i,
                        theme::sans_med(
                            (*label).to_string(),
                            12.5,
                            if mode == i { p.green } else { p.text_dim },
                        ),
                    )
                    .clicked()
                {
                    mode = i;
                }
            }
        });
        ui.add_space(4.0);
        tracked_label(ui, p, "MEMBERS");
        egui::ScrollArea::vertical()
            .max_height(240.0)
            .show(ui, |ui| {
                for prof in &profiles {
                    let included = members.contains(&prof.id);
                    let (rect, resp) = ui.allocate_exact_size(
                        egui::Vec2::new(ui.available_width(), 26.0),
                        egui::Sense::click(),
                    );
                    let painter = ui.painter_at(rect);
                    let check = if included { p.green } else { p.text_faint };
                    painter.circle_stroke(
                        egui::Pos2::new(rect.left() + 10.0, rect.center().y),
                        7.0,
                        egui::Stroke::new(1.4_f32, check),
                    );
                    if included {
                        painter.circle_filled(
                            egui::Pos2::new(rect.left() + 10.0, rect.center().y),
                            3.6,
                            p.green,
                        );
                    }
                    painter.galley(
                        egui::Pos2::new(rect.left() + 26.0, rect.center().y - 9.0),
                        painter.layout_no_wrap(
                            format!("{} ({})", prof.name, prof.protocol.as_str()),
                            egui::FontId::new(12.5, egui::FontFamily::Proportional),
                            p.text,
                        ),
                        p.text,
                    );
                    if resp.clicked() {
                        if included {
                            members.retain(|id| id != &prof.id);
                        } else {
                            members.push(prof.id.clone());
                        }
                    }
                }
            });
        ui.add_space(6.0);
        if fill_button(ui, p, "Save group", !name.trim().is_empty()).clicked() {
            if let Some(g) = app.groups.group_mut(group_id) {
                g.name = name.trim().to_string();
                g.mode = match mode {
                    1 => GroupMode::RoundRobin,
                    2 => GroupMode::Manual,
                    _ => GroupMode::Fastest,
                };
                g.members = members.clone();
            }
            app.groups.save();
            app.toasts.show("Group updated");
            closed = true;
        }
    });
    if closed {
        Dialog::Closed
    } else {
        Dialog::EditGroup {
            group_id: group_id.to_string(),
            name,
            mode,
            members,
        }
    }
}

fn rename_dialog(
    app: &mut App,
    ctx: &egui::Context,
    p: &Palette,
    group_id: &str,
    name: String,
) -> Dialog {
    let mut name = name;
    let mut closed = false;
    modal(ctx, p, "rename", 360.0, |ui| {
        if modal_header(ui, p, "Rename group", Some("Cancel")) {
            closed = true;
            return;
        }
        ui.add(
            egui::TextEdit::singleline(&mut name)
                .hint_text("New name")
                .desired_width(ui.available_width()),
        );
        ui.add_space(6.0);
        if fill_button(ui, p, "Rename", !name.trim().is_empty()).clicked() {
            if let Some(g) = app.groups.group_mut(group_id) {
                g.name = name.trim().to_string();
            }
            app.groups.save();
            closed = true;
        }
    });
    if closed {
        Dialog::Closed
    } else {
        Dialog::Rename {
            group_id: group_id.to_string(),
            name,
        }
    }
}

fn confirm_delete_group(app: &mut App, ctx: &egui::Context, p: &Palette, group_id: &str) -> Dialog {
    let mut close = false;
    modal(ctx, p, "delgroup", 380.0, |ui| {
        ui.label(theme::sans_bold("Delete group?", 17.0, p.text));
        ui.label(theme::sans(
            "The group is removed. Profiles inside it are kept in Saved.",
            12.0,
            p.text_dim,
        ));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new(theme::sans_med("Delete", 13.0, p.red)).fill(p.green_deep))
                .clicked()
            {
                app.groups.groups.retain(|g| g.id != group_id);
                if app.groups.active_group.as_deref() == Some(group_id) {
                    app.groups.active_group = None;
                }
                app.groups.ensure_defaults();
                app.groups.save();
                app.toasts.show("Group deleted");
                close = true;
            }
            if ui.button("Keep").clicked() {
                close = true;
            }
        });
    });
    if close {
        Dialog::Closed
    } else {
        Dialog::ConfirmDeleteGroup {
            group_id: group_id.to_string(),
        }
    }
}

fn confirm_delete_profile(
    app: &mut App,
    ctx: &egui::Context,
    p: &Palette,
    profile_id: &str,
) -> Dialog {
    let mut close = false;
    let name = app
        .data
        .profiles
        .iter()
        .find(|prof| prof.id == profile_id)
        .map(|prof| prof.name.clone())
        .unwrap_or_default();
    modal(ctx, p, "delprofile", 380.0, |ui| {
        ui.label(theme::sans_bold("Delete config?", 17.0, p.text));
        ui.label(theme::sans(
            format!("'{name}' will be removed from the vault and all groups."),
            12.0,
            p.text_dim,
        ));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new(theme::sans_med("Delete", 13.0, p.red)).fill(p.green_deep))
                .clicked()
            {
                let _ = npvt_core::vault::remove_profile(&mut app.data, profile_id);
                for g in &mut app.groups.groups {
                    g.members.retain(|id| id != profile_id);
                }
                app.groups.usage.remove(profile_id);
                app.save_all();
                app.toasts.show("Config deleted");
                close = true;
            }
            if ui.button("Keep").clicked() {
                close = true;
            }
        });
    });
    if close {
        Dialog::Closed
    } else {
        Dialog::ConfirmDeleteProfile {
            profile_id: profile_id.to_string(),
        }
    }
}

// ---- Import file unlock ---------------------------------------------------------

fn import_file(
    app: &mut App,
    ctx: &egui::Context,
    p: &Palette,
    path: &str,
    lock_kind: &str,
    passphrase: String,
    _error: Option<String>,
) -> Dialog {
    let mut passphrase = passphrase;
    let mut closed = false;
    modal(ctx, p, "unlock", 380.0, |ui| {
        ui.label(theme::sans_bold("Unlock config", 17.0, p.text));
        ui.label(theme::sans(
            format!("{path}\nThis container is {lock_kind}-locked."),
            11.5,
            p.text_dim,
        ));
        ui.add_space(6.0);
        if lock_kind == "keys" {
            ui.label(theme::sans(
                "It will be unlocked with your own key (MORE > My public key).",
                11.5,
                p.text_dim,
            ));
        } else {
            ui.add(
                egui::TextEdit::singleline(&mut passphrase)
                    .password(true)
                    .hint_text("Passphrase")
                    .desired_width(ui.available_width()),
            );
        }
        ui.add_space(6.0);
        if fill_button(ui, p, "Unlock and import", true).clicked() {
            match std::fs::read(path) {
                Ok(bytes) => {
                    if lock_kind == "keys" {
                        let Some(secret) = app
                            .data
                            .identity
                            .as_ref()
                            .map(|identity| identity.x25519_seed)
                        else {
                            app.toasts
                                .error("No identity yet: open MORE > My public key first");
                            return;
                        };
                        match npvt_core::npvt::v2::decode(
                            &bytes,
                            npvt_core::npvt::v2::Unlock::Keys(&[secret]),
                        ) {
                            Ok(decoded) => {
                                let report = import_profiles(&mut app.data, vec![decoded.profile]);
                                app.save_all();
                                app.toasts.show(report.summary());
                                closed = true;
                            }
                            Err(npvt_core::Error::CryptoAuth) => {
                                app.toasts
                                    .error("This container was not locked for your key");
                            }
                            Err(e) => app.toasts.error(format!("Import failed: {e}")),
                        }
                    } else if decode_container(app, &bytes, Some(passphrase.trim())) {
                        closed = true;
                    }
                }
                Err(e) => app.toasts.error(format!("Cannot read file: {e}")),
            }
        }
    });
    if closed {
        Dialog::Closed
    } else {
        Dialog::ImportFile {
            path: path.to_string(),
            lock_kind: lock_kind.to_string(),
            passphrase,
            error: None,
        }
    }
}

// ---- MORE dialogs -----------------------------------------------------------------

fn premium(app: &mut App, ctx: &egui::Context, p: &Palette) -> Dialog {
    let mut close = false;
    modal(ctx, p, "premium", 380.0, |ui| {
        ui.label(theme::sans_bold("NPV Tunnel Premium", 18.0, p.text));
        ui.add_space(4.0);
        ui.label(theme::sans(
            "Premium hides the sponsor panel from the Points screen and keeps every tunnel feature free forever.",
            12.5,
            p.text_dim,
        ));
        ui.add_space(8.0);
        if app.settings.premium {
            ui.label(theme::mono_med("PREMIUM ACTIVE", 12.0, p.green));
            if fill_button(ui, p, "Close", true).clicked() {
                close = true;
            }
        } else {
            ui.label(theme::mono(
                format!("Your points: {}", app.settings.points),
                11.5,
                p.text_dim,
            ));
            ui.add_space(4.0);
            let affordable = app.settings.points >= 5000;
            if fill_button(ui, p, "Activate with 5000 points", affordable).clicked() {
                app.settings.points -= 5000;
                app.settings.premium = true;
                app.settings.save();
                app.toasts.show("Premium activated. Enjoy!");
                close = true;
            }
            if !affordable {
                ui.label(theme::sans(
                    "Earn points on the Points screen (MORE > Points).",
                    11.5,
                    p.text_faint,
                ));
            }
        }
    });
    if close {
        Dialog::Closed
    } else {
        Dialog::Premium
    }
}

fn points(
    app: &mut App,
    ctx: &egui::Context,
    p: &Palette,
    watching: Option<std::time::Instant>,
) -> Dialog {
    let mut watching = watching;
    let mut closed = false;
    // Advance the sponsor countdown.
    if let Some(started) = watching {
        if started.elapsed() >= std::time::Duration::from_secs(8) {
            app.settings.points += 25;
            app.settings.last_sponsor = unix_now();
            app.settings.save();
            app.toasts.show("+25 points");
            watching = None;
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
    }
    modal(ctx, p, "points", 380.0, |ui| {
        if modal_header(ui, p, "Points", Some("Close")) {
            closed = true;
            return;
        }
        ui.label(theme::mono_bold(
            format!("{}", app.settings.points),
            30.0,
            p.green,
        ));
        ui.label(theme::sans("points", 11.5, p.text_faint));
        ui.add_space(8.0);
        let premium_note = if app.settings.premium {
            "Premium is active: no sponsor panels."
        } else {
            "Watch a short sponsor clip to earn 25 points."
        };
        ui.label(theme::sans(premium_note, 12.0, p.text_dim));
        ui.add_space(6.0);
        if !app.settings.premium {
            let cooldown_over = unix_now().saturating_sub(app.settings.last_sponsor) > 60;
            match watching {
                Some(started) => {
                    let left = 8 - started.elapsed().as_secs().min(8);
                    ui.label(theme::mono(
                        format!("Sponsor playing… {left}s"),
                        12.5,
                        p.cyan,
                    ));
                }
                None => {
                    if fill_button(ui, p, "Watch sponsor clip", cooldown_over).clicked() {
                        watching = Some(std::time::Instant::now());
                    }
                    if !cooldown_over {
                        ui.label(theme::sans(
                            "Available again in a minute.",
                            11.0,
                            p.text_faint,
                        ));
                    }
                }
            }
        }
    });
    if closed {
        Dialog::Closed
    } else {
        Dialog::Points { watching }
    }
}

fn settings_dialog(app: &mut App, ctx: &egui::Context, p: &Palette) -> Dialog {
    let mut close = false;
    let mut socks = app.settings.socks_port.to_string();
    let mut api = app.settings.api_port.to_string();
    let mut dns = app.settings.dns.clone();
    let mut autoconnect = app.settings.autoconnect;
    let mut autostart = if cfg!(windows) {
        data::autostart_enabled()
    } else {
        app.settings.autostart
    };
    modal(ctx, p, "settings", 400.0, |ui| {
        if modal_header(ui, p, "Settings", Some("Cancel")) {
            close = true;
            return;
        }
        ui.horizontal(|ui| {
            ui.label(theme::sans_med("SOCKS port", 12.0, p.text_dim));
            ui.add(egui::TextEdit::singleline(&mut socks).desired_width(70.0));
            ui.label(theme::sans_med("Clash API port", 12.0, p.text_dim));
            ui.add(egui::TextEdit::singleline(&mut api).desired_width(70.0));
        });
        ui.horizontal(|ui| {
            ui.label(theme::sans_med("DNS", 12.0, p.text_dim));
            ui.add(
                egui::TextEdit::singleline(&mut dns)
                    .desired_width(140.0)
                    .font(egui::TextStyle::Monospace),
            );
        });
        ui.checkbox(&mut autoconnect, "Connect automatically at launch");
        ui.checkbox(&mut autostart, "Start NPVTunnel with Windows");
        ui.add_space(6.0);
        if fill_button(ui, p, "Save settings", true).clicked() {
            if let Ok(v) = socks.trim().parse() {
                app.settings.socks_port = v;
            }
            if let Ok(v) = api.trim().parse() {
                app.settings.api_port = v;
            }
            if !dns.trim().is_empty() {
                app.settings.dns = dns.trim().to_string();
            }
            app.settings.autoconnect = autoconnect;
            app.settings.autostart = autostart;
            let _ = data::set_autostart(autostart);
            app.settings.save();
            app.toasts.show("Settings saved");
            close = true;
        }
    });
    if close {
        Dialog::Closed
    } else {
        Dialog::SettingsDlg
    }
}

fn per_app(
    app: &mut App,
    ctx: &egui::Context,
    p: &Palette,
    mode: usize,
    processes: String,
) -> Dialog {
    let mut mode = mode;
    let mut processes = processes;
    let mut closed = false;
    modal(ctx, p, "perapp", 400.0, |ui| {
        if modal_header(ui, p, "Per-app Proxy", Some("Cancel")) {
            closed = true;
            return;
        }
        ui.label(theme::sans(
            "Choose to allow/disallow certain apps from being tunneled through the VPN.",
            12.0,
            p.text_dim,
        ));
        ui.add_space(4.0);
        for (i, (label, desc)) in [
            ("Off", "Every app uses the tunnel."),
            ("Allow-list", "Only the apps below are tunneled."),
            ("Deny-list", "The apps below bypass the tunnel."),
        ]
        .iter()
        .enumerate()
        {
            if radio_row(ui, p, mode == i, label, desc).clicked() {
                mode = i;
            }
        }
        ui.add_space(4.0);
        ui.label(theme::sans_med(
            "Process names (one per line)",
            12.0,
            p.text_dim,
        ));
        ui.add(
            egui::TextEdit::multiline(&mut processes)
                .desired_rows(4)
                .desired_width(ui.available_width())
                .font(egui::TextStyle::Monospace),
        );
        ui.add_space(6.0);
        if fill_button(ui, p, "Save", true).clicked() {
            app.settings.per_app.mode = match mode {
                1 => "allow".into(),
                2 => "deny".into(),
                _ => "off".into(),
            };
            app.settings.per_app.processes = processes
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(ToOwned::to_owned)
                .collect();
            app.settings.save();
            app.toasts.show("Per-app rules saved");
            closed = true;
        }
    });
    if closed {
        Dialog::Closed
    } else {
        Dialog::PerApp { mode, processes }
    }
}

fn public_key(app: &mut App, ctx: &egui::Context, p: &Palette) -> Dialog {
    use npvt_core::npvt::v2;
    let mut close = false;
    // Generate an identity on first use.
    if app.data.identity.is_none() {
        app.data.identity = Some(npvt_core::vault::generate_identity());
        app.save_all();
    }
    let (x_pub, e_pub) = {
        let identity = app.data.identity.as_ref().expect("just generated");
        let x = v2::x25519_public_from_secret(&identity.x25519_seed);
        let signing = v2::signing_key_from_seed(&identity.ed25519_seed);
        (
            v2::key_to_b64(&x),
            v2::key_to_b64(signing.verifying_key().as_bytes()),
        )
    };
    modal(ctx, p, "pubkey", 430.0, |ui| {
        if modal_header(ui, p, "My public key", Some("Close")) {
            close = true;
            return;
        }
        ui.label(theme::sans(
            "Share these keys so other people can send you key-locked configs.",
            12.0,
            p.text_dim,
        ));
        ui.add_space(6.0);
        tracked_label(ui, p, "X25519 (FOR LOCKED SHARES)");
        key_row(ui, p, &x_pub);
        ui.add_space(6.0);
        tracked_label(ui, p, "ED25519 (CREATOR SIGNATURE)");
        key_row(ui, p, &e_pub);
    });
    if close {
        Dialog::Closed
    } else {
        Dialog::PublicKey
    }
}

fn key_row(ui: &mut egui::Ui, p: &Palette, key: &str) {
    let mut display = key.to_string();
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut display)
                .font(egui::TextStyle::Monospace)
                .desired_width(ui.available_width() - 40.0),
        );
        if icons::icon_button(ui, Icon::Copy, 26.0, p.cyan, None)
            .on_hover_text("Copy key")
            .clicked()
        {
            if let Ok(mut clip) = arboard::Clipboard::new() {
                let _ = clip.set_text(key.to_string());
            }
        }
    });
}

fn user_assets(
    _app: &mut App,
    ctx: &egui::Context,
    p: &Palette,
    geoip: Option<Arc<Mutex<DownloadState>>>,
    geosite: Option<Arc<Mutex<DownloadState>>>,
    geoip_cancel: Arc<std::sync::atomic::AtomicBool>,
    geosite_cancel: Arc<std::sync::atomic::AtomicBool>,
) -> Dialog {
    let mut geoip = geoip;
    let mut geosite = geosite;
    let mut geoip_cancel = geoip_cancel;
    let mut geosite_cancel = geosite_cancel;
    let mut closed = false;
    modal(ctx, p, "assets", 420.0, |ui| {
        if modal_header(ui, p, "User assets", Some("Close")) {
            closed = true;
            return;
        }
        ui.label(theme::sans(
            "Download geo data files (geoip, geosite) used by custom config routing.",
            12.0,
            p.text_dim,
        ));
        ui.add_space(6.0);
        let Ok(assets_dir) = npvt_core::engine::work_dir().map(|d| d.join("assets")) else {
            return;
        };
        let _ = std::fs::create_dir_all(&assets_dir);
        asset_row(
            ui,
            p,
            "geoip.db",
            GEOIP_URL,
            &assets_dir,
            &mut geoip,
            &mut geoip_cancel,
        );
        ui.add_space(8.0);
        asset_row(
            ui,
            p,
            "geosite.db",
            GEOSITE_URL,
            &assets_dir,
            &mut geosite,
            &mut geosite_cancel,
        );
    });
    if closed {
        Dialog::Closed
    } else {
        Dialog::UserAssets {
            geoip,
            geosite,
            geoip_cancel,
            geosite_cancel,
        }
    }
}

fn asset_row(
    ui: &mut egui::Ui,
    p: &Palette,
    filename: &str,
    url: &str,
    dir: &std::path::Path,
    state: &mut Option<Arc<Mutex<DownloadState>>>,
    cancel: &mut Arc<std::sync::atomic::AtomicBool>,
) {
    let dest = dir.join(filename);
    let existing = std::fs::metadata(&dest).ok();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(theme::sans_med(filename, 13.0, p.text));
            match &existing {
                Some(meta) => {
                    ui.label(theme::mono(
                        format!("{} · downloaded", data::format_bytes(meta.len())),
                        10.5,
                        p.text_dim,
                    ));
                }
                None => {
                    ui.label(theme::mono("not downloaded", 10.5, p.text_faint));
                }
            }
            if let Some(st) = state {
                if let Ok(guard) = st.lock() {
                    match guard.phase.as_str() {
                        "running" => {
                            ui.label(theme::mono(
                                format!("downloading… {}", data::format_bytes(guard.got)),
                                10.5,
                                p.cyan,
                            ));
                        }
                        "done" => {
                            ui.label(theme::mono("done", 10.5, p.green));
                        }
                        "error" => {
                            ui.label(theme::mono(
                                guard.error.clone().unwrap_or_default(),
                                10.5,
                                p.red,
                            ));
                        }
                        _ => {}
                    }
                }
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let running = state
                .as_ref()
                .map(|s| s.lock().map(|g| g.phase == "running").unwrap_or(false))
                .unwrap_or(false);
            if running {
                if ui.button("Cancel").clicked() {
                    cancel.store(true, Ordering::SeqCst);
                }
            } else if ui
                .add(
                    egui::Button::new(theme::mono_med("Download", 12.0, p.cyan))
                        .fill(Color32::TRANSPARENT)
                        .stroke(egui::Stroke::NONE),
                )
                .clicked()
            {
                let shared_state = Arc::new(Mutex::new(DownloadState::default()));
                let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
                *state = Some(Arc::clone(&shared_state));
                *cancel = Arc::clone(&cancel_flag);
                let dest = dest.clone();
                let url = url.to_string();
                std::thread::spawn(move || {
                    net::download_to(&url, &dest, &shared_state, &cancel_flag);
                });
            }
        });
    });
}

// ---- Kebab + sort menus -------------------------------------------------------

fn kebab_menu(
    app: &mut App,
    ctx: &egui::Context,
    p: &Palette,
    group_id: &str,
    kind: KebabKind,
) -> Dialog {
    let mut next = Dialog::Closed;
    modal(ctx, p, "kebab", 320.0, |ui| {
        let title = match kind {
            KebabKind::User => "Group actions",
            KebabKind::Saved => "Saved actions",
            KebabKind::Global => "Global Network actions",
        };
        ui.label(theme::sans_bold(title, 16.0, p.text));
        ui.add_space(6.0);
        let ctx3 = ui.ctx().clone();
        menu_item(ui, p, Icon::Speed, "Ping now", || {
            app.ping_group(group_id, &ctx3);
        });
        if kind == KebabKind::User {
            menu_item(ui, p, Icon::Upload, "Export / share…", || {
                if crate::export::ExportState::for_group(app, group_id).is_some() {
                    app.export_page = true;
                    next = Dialog::Closed;
                }
            });
            menu_item(ui, p, Icon::Pencil, "Rename…", || {
                if let Some(g) = app.groups.group(group_id) {
                    next = Dialog::Rename {
                        group_id: g.id.clone(),
                        name: g.name.clone(),
                    };
                }
            });
            menu_item(ui, p, Icon::Pin, "Pin / unpin", || {
                if let Some(g) = app.groups.group_mut(group_id) {
                    g.pinned = !g.pinned;
                }
                app.groups.save();
            });
            menu_item(ui, p, Icon::Trash, "Delete group…", || {
                next = Dialog::ConfirmDeleteGroup {
                    group_id: group_id.to_string(),
                };
            });
        } else {
            menu_item(ui, p, Icon::Refresh, "Clear pings", || {
                if let Ok(mut g) = app.shared.pings.lock() {
                    g.clear();
                }
            });
            if kind == KebabKind::Global {
                menu_item(ui, p, Icon::Folder, "Open data folder", || {
                    if let Ok(dir) = npvt_core::engine::work_dir() {
                        data::reveal(&dir);
                    }
                });
            }
        }
    });
    next
}

fn sort_menu(app: &mut App, ctx: &egui::Context, p: &Palette) -> Dialog {
    let mut next = Dialog::SortMenu;
    modal(ctx, p, "sort", 300.0, |ui| {
        ui.label(theme::sans_bold("Sort members by", 16.0, p.text));
        ui.add_space(6.0);
        menu_item(ui, p, Icon::Check, "Name", || {
            app.sort = SortKey::Name;
            next = Dialog::Closed;
        });
        menu_item(ui, p, Icon::Speed, "Ping result", || {
            app.sort = SortKey::Ping;
            next = Dialog::Closed;
        });
        menu_item(ui, p, Icon::Server, "Newest first", || {
            app.sort = SortKey::Created;
            next = Dialog::Closed;
        });
    });
    next
}

fn engine_missing(_app: &mut App, ctx: &egui::Context, p: &Palette, message: &str) -> Dialog {
    let mut close = false;
    modal(ctx, p, "engine", 430.0, |ui| {
        ui.label(theme::sans_bold("Engine not found", 17.0, p.red));
        ui.label(theme::sans(message, 12.0, p.text_dim));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Open download page").clicked() {
                data::open_url("https://github.com/SagerNet/sing-box/releases");
            }
            if ui.button("Close").clicked() {
                close = true;
            }
        });
    });
    if close {
        Dialog::Closed
    } else {
        Dialog::EngineMissing {
            message: message.to_string(),
        }
    }
}

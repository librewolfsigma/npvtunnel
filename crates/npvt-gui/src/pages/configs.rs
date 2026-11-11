//! Configs: the profile library — grouped cards with connect, ping, export,
//! edit and delete actions, plus import (file / clipboard / manual).

use eframe::egui::{self, Align, Layout, RichText, Ui};

use super::{header, ping_color};
use crate::theme::{self, Palette};
use crate::widgets;
use crate::NpvApp;

pub fn show(app: &mut NpvApp, ui: &mut Ui, p: &Palette) {
    let count = app.vault.profiles.len();
    header(
        ui,
        p,
        "Configurations",
        &format!("{count} configs in library"),
        |ui, p| {
            let busy = app.bridge.busy;
            ui.add_enabled_ui(!busy, |ui| {
                ui.menu_button("＋ Add config", |ui| {
                    if ui.button("Import config file (.npvt)...").clicked() {
                        ui.close();
                        app.open_import_file();
                    }
                    if ui.button("Import config from Clipboard").clicked() {
                        ui.close();
                        app.import_from_clipboard();
                    }
                    if ui.button("Add config manually").clicked() {
                        ui.close();
                        app.open_manual_add();
                    }
                    if ui.button("New connection group").clicked() {
                        ui.close();
                        app.open_manual_add();
                    }
                });
                if widgets::ghost_button(ui, p, "Refresh").clicked() {
                    app.bridge.load_vault();
                }
            });
            if busy {
                widgets::badge(ui, p, p.warn, "WORKING...");
            }
            let search = ui.add(
                egui::TextEdit::singleline(&mut app.search)
                    .hint_text("Search...")
                    .desired_width(170.0),
            );
            if search.changed() {
                app.search = app.search.trim().to_string();
            }
        },
    );

    if app.vault.profiles.is_empty() {
        super::empty_state(
            ui,
            p,
            "No configs yet",
            "Import an .npvt file, paste a share link, or add a config manually.",
        );
        return;
    }

    let filter = app.search.to_lowercase();
    let mut profiles: Vec<_> = app
        .vault
        .profiles
        .iter()
        .filter(|prof| {
            filter.is_empty()
                || prof.name.to_lowercase().contains(&filter)
                || prof.server.to_lowercase().contains(&filter)
        })
        .cloned()
        .collect();
    profiles.sort_by(|a, b| {
        let ga = a.group.clone().unwrap_or_default();
        let gb = b.group.clone().unwrap_or_default();
        ga.cmp(&gb).then_with(|| a.name.cmp(&b.name))
    });

    let mut last_group: Option<String> = None;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            for profile in &profiles {
                if profile.group != last_group {
                    last_group = profile.group.clone();
                    ui.add_space(4.0);
                    let members = profiles.iter().filter(|x| x.group == last_group).count();
                    match &last_group {
                        Some(group) => {
                            widgets::badge(
                                ui,
                                p,
                                p.teal,
                                &format!("GROUP · {group} · {members} members"),
                            );
                        }
                        None => {
                            widgets::badge(ui, p, p.muted, "UNGROUPED");
                        }
                    }
                    ui.add_space(2.0);
                }
                profile_card(app, ui, p, profile);
                ui.add_space(6.0);
            }
        });
}

fn profile_card(app: &mut NpvApp, ui: &mut Ui, p: &Palette, profile: &npvt_core::profile::Profile) {
    let is_active = matches!(
        &app.bridge.state,
        crate::bridge::EngineState::Connected(info) if info.profile_id == profile.id
    );
    let ping_badge = app.ping_badges.get(&profile.id).copied().flatten();

    let card_response = widgets::card_active_or(ui, p, is_active, |ui| {
        ui.horizontal(|ui| {
            ui.label(theme::semi(&profile.name).size(15.0).color(p.text));
            if is_active {
                widgets::badge(ui, p, p.accent, "ACTIVE");
            }
            widgets::badge(ui, p, p.teal, profile.protocol.as_str());
            match profile.transport {
                npvt_core::profile::Transport::Tls => {
                    widgets::badge(ui, p, p.muted, "tls");
                }
                npvt_core::profile::Transport::Ws => {
                    widgets::badge(ui, p, p.muted, "ws");
                }
                npvt_core::profile::Transport::Grpc => {
                    widgets::badge(ui, p, p.muted, "grpc");
                }
                npvt_core::profile::Transport::H2 => {
                    widgets::badge(ui, p, p.muted, "h2");
                }
                npvt_core::profile::Transport::Plain => {}
            }
            if profile.udp {
                widgets::badge(ui, p, p.muted, "udp");
            }
            if profile.allow_insecure {
                widgets::badge(ui, p, p.warn, "insecure tls");
            }
            if let Some(ms) = ping_badge {
                widgets::badge(ui, p, ping_color(ms), &format!("{ms} ms"));
            }
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{}:{}", profile.server, profile.port))
                    .family(eframe::egui::FontFamily::Monospace)
                    .size(11.5)
                    .color(p.muted),
            );
            if let Some(sni) = &profile.sni {
                ui.label(
                    RichText::new(format!("sni {sni}"))
                        .size(11.0)
                        .color(p.muted),
                );
            }
        });
    });

    if card_response.double_clicked() && !is_active {
        app.select_profile(&profile.id);
    }

    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.menu_button("⋯", |ui| {
                if ui.button("Ping").clicked() {
                    ui.close();
                    app.ping_profile(&profile.id);
                }
                if ui.button("Export...").clicked() {
                    ui.close();
                    app.open_export(&profile.id);
                }
                if ui.button("Edit...").clicked() {
                    ui.close();
                    app.open_edit(&profile.id);
                }
                if ui.button("Delete").clicked() {
                    ui.close();
                    app.modal = crate::Modal::ConfirmDelete {
                        profile_id: profile.id.clone(),
                        name: profile.name.clone(),
                    };
                }
            });
            if is_active {
                if widgets::danger_button(ui, p, "Disconnect").clicked() {
                    app.confirm_disconnect();
                }
            } else {
                let connecting = matches!(
                    app.bridge.state,
                    crate::bridge::EngineState::Connecting | crate::bridge::EngineState::Stopping
                );
                ui.add_enabled_ui(!connecting, |ui| {
                    if widgets::primary_button(ui, p, "Connect").clicked() {
                        app.select_profile(&profile.id);
                        app.request_connect_selected();
                    }
                });
            }
        });
    });
    ui.add_space(2.0);
}

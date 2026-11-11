//! Home: the big power button, live status, connection card and traffic
//! stats — the NPV TUNNEL signature screen, desktop edition.

use eframe::egui::{self, Align, Color32, Layout, RichText, Ui, Vec2};

use super::{header, ping_color};
use crate::theme::{self, Palette};
use crate::widgets::{self, PowerVisual};
use crate::NpvApp;

pub fn show(app: &mut NpvApp, ui: &mut Ui, p: &Palette) {
    header(ui, p, "Home", &version_line(app), |ui, p| {
        if app.engine_path_cached().is_some() {
            widgets::badge(ui, p, p.accent, "ENGINE READY");
        } else {
            widgets::badge(ui, p, p.danger, "ENGINE MISSING");
        }
    });

    // Error banner (engine errors, import failures, etc.).
    if let Some(error) = app.bridge.error.clone() {
        widgets::card(ui, p, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::semi("Engine error").size(12.5).color(p.danger));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::ghost_button(ui, p, "Copy").clicked() {
                        ui.ctx().copy_text(error.clone());
                    }
                    if widgets::ghost_button(ui, p, "Dismiss").clicked() {
                        app.bridge.error = None;
                    }
                });
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(&error)
                    .size(11.5)
                    .color(p.danger.gamma_multiply(0.8)),
            );
        });
        ui.add_space(8.0);
    }

    match app.bridge.state.clone() {
        crate::bridge::EngineState::Connected(info) => connected_block(app, ui, p, &info),
        crate::bridge::EngineState::Connecting => connecting_block(ui, p),
        crate::bridge::EngineState::Stopping => {
            center_status(ui, p, PowerVisual::Busy, "[ STOPPING ]", p.warn);
            ui.add_space(10.0);
        }
        crate::bridge::EngineState::Disconnected => disconnected_block(app, ui, p),
    }
}

fn version_line(app: &NpvApp) -> String {
    let mut line = format!("v{}", env!("CARGO_PKG_VERSION"));
    if let Some(traffic) = app.bridge.traffic {
        line.push_str(&format!("  ·  {} active connections", traffic.conns));
    }
    line
}

fn power_and_status(
    ui: &mut Ui,
    _p: &Palette,
    visual: PowerVisual,
    label: &str,
    color: Color32,
) -> egui::Response {
    ui.vertical_centered(|ui| {
        let avail = ui.available_height() - 60.0;
        let size = 190.0f32.min(avail).max(150.0);
        let resp = widgets::power_button(ui, size, visual, true);
        ui.add_space(6.0);
        ui.label(theme::mono(label).size(15.0).color(color));
        resp
    })
    .inner
}

fn center_status(ui: &mut Ui, p: &Palette, visual: PowerVisual, label: &str, color: Color32) {
    power_and_status(ui, p, visual, label, color);
}

fn connecting_block(ui: &mut Ui, p: &Palette) {
    center_status(ui, p, PowerVisual::Busy, "[ CONNECTING ]", p.warn);
    ui.add_space(6.0);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new("Starting the sing-box sidecar and verifying liveness...")
                .size(11.5)
                .color(p.muted),
        );
    });
}

fn disconnected_block(app: &mut NpvApp, ui: &mut Ui, p: &Palette) {
    let click = power_and_status(ui, p, PowerVisual::Off, "[ DISCONNECTED ]", p.muted);
    if click.clicked() {
        app.request_connect_selected();
    }
    ui.add_space(6.0);
    ui.vertical_centered(|ui| match app.selected_profile() {
        Some(profile) => {
            ui.label(
                RichText::new(format!(
                    "Next up: {}  ({} {}:{})",
                    profile.name,
                    profile.protocol.as_str(),
                    profile.server,
                    profile.port
                ))
                .size(12.5)
                .color(p.text),
            );
            ui.label(
                RichText::new(
                    "Press the power button to connect, or pick another config under Configs.",
                )
                .size(11.0)
                .color(p.muted),
            );
        }
        None => {
            ui.label(
                RichText::new("No configs yet - add one under Configs to get started.")
                    .size(12.0)
                    .color(p.muted),
            );
            if widgets::primary_button(ui, p, "Go to Configs").clicked() {
                app.page = super::Page::Configs;
            }
        }
    });
    stats_row(app, ui, p);
}

fn connected_block(app: &mut NpvApp, ui: &mut Ui, p: &Palette, info: &crate::bridge::Connected) {
    let click = power_and_status(ui, p, PowerVisual::On, "[ CONNECTED ]", p.accent);
    if click.clicked() {
        app.confirm_disconnect();
    }
    ui.add_space(4.0);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(format!(
                "SOCKS5 proxy listening on 127.0.0.1:{}  (localhost only)",
                info.socks_port
            ))
            .size(11.0)
            .color(p.muted),
        );
    });

    // Active connection card.
    ui.add_space(8.0);
    widgets::card_active(ui, p, |ui| {
        ui.horizontal(|ui| {
            ui.label(theme::semi("Active connection").size(11.5).color(p.muted));
            widgets::badge(ui, p, p.accent, "ACTIVE");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(widgets::fmt_uptime(info.started))
                        .size(11.5)
                        .color(p.muted),
                );
            });
        });
        ui.add_space(2.0);
        ui.label(theme::semi(&info.profile_name).size(16.0).color(p.text));
        let meta = app
            .vault
            .profiles
            .iter()
            .find(|prof| prof.id == info.profile_id)
            .map(|prof| {
                let group = prof
                    .group
                    .as_deref()
                    .map(|g| format!(" · group {g}"))
                    .unwrap_or_default();
                format!(
                    "{} {}:{}{}",
                    prof.protocol.as_str(),
                    prof.server,
                    prof.port,
                    group
                )
            })
            .unwrap_or_else(|| "profile details unavailable".to_string());
        ui.label(RichText::new(meta).size(11.5).color(p.muted));
    });

    stats_row(app, ui, p);
}

fn stats_row(app: &mut NpvApp, ui: &mut Ui, p: &Palette) {
    ui.add_space(8.0);
    let traffic = app.bridge.traffic.unwrap_or_default();

    // Down / Up / Ping / Connections stat cards.
    let cards: [(String, String, Color32); 4] = [
        (
            "DOWN".to_string(),
            widgets::fmt_bytes(traffic.down_total),
            p.accent,
        ),
        (
            "UP".to_string(),
            widgets::fmt_bytes(traffic.up_total),
            p.teal,
        ),
        (
            "RATE".to_string(),
            format!(
                "{} / {}",
                widgets::fmt_rate(traffic.down_bps),
                widgets::fmt_rate(traffic.up_bps)
            ),
            p.text,
        ),
        ("CONNS".to_string(), traffic.conns.to_string(), p.text),
    ];
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 3.0 * 10.0) / 4.0;
        for (label, value, color) in &cards {
            widgets::card(ui, p, |ui| {
                ui.set_min_width(width);
                ui.label(
                    RichText::new(label.as_str())
                        .size(10.0)
                        .color(p.muted)
                        .strong(),
                );
                ui.label(theme::semi(value.as_str()).size(14.5).color(*color));
                if label == "DOWN" && traffic.down_bps > 0.0 {
                    ui.label(
                        RichText::new(widgets::fmt_rate(traffic.down_bps))
                            .size(10.5)
                            .color(p.muted),
                    );
                } else if label == "UP" && traffic.up_bps > 0.0 {
                    ui.label(
                        RichText::new(widgets::fmt_rate(traffic.up_bps))
                            .size(10.5)
                            .color(p.muted),
                    );
                } else {
                    ui.label(RichText::new(" ").size(10.5));
                }
            });
        }
    });

    ui.add_space(4.0);
    widgets::card(ui, p, |ui| {
        ui.horizontal(|ui| {
            let ping_line = match &app.bridge.last_ping {
                Some((_, Ok(ms))) => RichText::new(format!("LAST PING: {ms} ms"))
                    .size(12.0)
                    .color(ping_color(*ms)),
                Some((_, Err(err))) => RichText::new(format!("LAST PING: failed ({err})"))
                    .size(12.0)
                    .color(p.danger),
                None => RichText::new("LAST PING: Never").size(12.0).color(p.muted),
            };
            ui.label(ping_line);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let busy = app.bridge.ping_in_flight;
                let btn = if busy {
                    ui.add_enabled(
                        false,
                        egui::Button::new(theme::semi("PING").size(12.0).color(p.muted))
                            .fill(p.card)
                            .min_size(Vec2::new(74.0, 28.0)),
                    )
                } else {
                    ui.add(
                        egui::Button::new(theme::semi("PING").size(12.0).color(p.on_accent))
                            .fill(p.accent)
                            .min_size(Vec2::new(74.0, 28.0)),
                    )
                };
                if btn.clicked() {
                    app.ping_selected();
                }
            });
        });
    });
}

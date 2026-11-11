//! MORE: Premium, the Light/Dark/Auto appearance segmented control, sharing
//! and app tools - a faithful port of reference image 5.

use crate::app::{App, Dialog};
use crate::data;
use crate::icons::Icon;
use crate::theme::{self, ThemeMode};
use crate::widgets::{self, divider, more_row, segmented, tracked_label};

pub const CONTENT_W: f32 = 430.0;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette;
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.with_layout(
            egui::Layout::top_down_justified(egui::Align::Center),
            |ui| {
                ui.set_max_width(CONTENT_W);
                ui.add_space(4.0);
                widgets::header(ui, &p, false, false);
                ui.add_space(8.0);

                // Premium row.
                if more_row(ui, &p, Icon::Star, "Premium", "Subscribe to remove ads.").clicked() {
                    app.dialog = Dialog::Premium;
                }
                divider(ui, &p);

                // Appearance.
                ui.add_space(10.0);
                tracked_label(ui, &p, "APPEARANCE");
                ui.add_space(4.0);
                let current = match app.settings.theme {
                    ThemeMode::Light => 0,
                    ThemeMode::Dark => 1,
                    ThemeMode::Auto => 2,
                };
                let hits = segmented(
                    ui,
                    &p,
                    &[
                        ("Light", Icon::Sun),
                        ("Dark", Icon::Moon),
                        ("Auto", Icon::Gear),
                    ],
                    current,
                );
                if let Some(idx) = hits.iter().position(|clicked| *clicked) {
                    app.settings.theme = match idx {
                        0 => ThemeMode::Light,
                        1 => ThemeMode::Dark,
                        _ => ThemeMode::Auto,
                    };
                    app.settings.save();
                }
                ui.label(theme::sans(
                    "Auto follows your device's light or dark setting.",
                    11.5,
                    p.text_dim,
                ));

                // Sharing.
                ui.add_space(14.0);
                tracked_label(ui, &p, "SHARING");
                if more_row(
                    ui,
                    &p,
                    Icon::Qr,
                    "My public key",
                    "View or copy your public key.",
                )
                .clicked()
                {
                    app.dialog = Dialog::PublicKey;
                }
                divider(ui, &p);

                // App section.
                ui.add_space(10.0);
                tracked_label(ui, &p, "APP");
                if more_row(
                    ui,
                    &p,
                    Icon::Grid,
                    "Per-app Proxy",
                    "Choose to allow/disallow certain apps from being tunneled through the VPN.",
                )
                .clicked()
                {
                    let mode = match app.settings.per_app.mode.as_str() {
                        "allow" => 1,
                        "deny" => 2,
                        _ => 0,
                    };
                    app.dialog = Dialog::PerApp {
                        mode,
                        processes: app.settings.per_app.processes.join("\n"),
                    };
                }
                divider(ui, &p);
                if more_row(
                    ui,
                    &p,
                    Icon::Star,
                    "Points",
                    "Earn points by watching videos, spend them on rewards.",
                )
                .clicked()
                {
                    app.dialog = Dialog::Points { watching: None };
                }
                divider(ui, &p);
                if more_row(
                    ui,
                    &p,
                    Icon::Gear,
                    "Settings",
                    "Customize the app's settings",
                )
                .clicked()
                {
                    app.dialog = Dialog::SettingsDlg;
                }
                divider(ui, &p);
                if more_row(
                    ui,
                    &p,
                    Icon::Folder,
                    "User assets",
                    "Download geo data files (geoip, geosite) used by custom config routing.",
                )
                .clicked()
                {
                    app.dialog = Dialog::UserAssets {
                        geoip: None,
                        geosite: None,
                        geoip_cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                            false,
                        )),
                        geosite_cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                            false,
                        )),
                    };
                }
                divider(ui, &p);

                // Footer info.
                ui.add_space(14.0);
                ui.vertical_centered(|ui| {
                    ui.label(theme::mono(
                        format!(
                            "NPVTunnel v{} · console-free desktop build",
                            env!("CARGO_PKG_VERSION")
                        ),
                        9.5,
                        p.text_faint,
                    ));
                    if ui
                        .add(
                            egui::Button::new(theme::mono_med("Open data folder", 10.5, p.cyan))
                                .fill(egui::Color32::TRANSPARENT)
                                .stroke(egui::Stroke::NONE),
                        )
                        .clicked()
                    {
                        if let Ok(dir) = npvt_core::engine::work_dir() {
                            data::reveal(&dir);
                        }
                    }
                });
                ui.add_space(20.0);
            },
        );
    });
}

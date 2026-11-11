//! More: appearance, sharing (identity/public key), engine settings, about.

use eframe::egui::{self, Align, Layout, RichText, Ui};

use super::header;
use crate::assoc;
use crate::settings::ThemeMode;
use crate::theme::{self, Palette};
use crate::widgets;
use crate::NpvApp;
use crate::ToastKind;

pub fn show(app: &mut NpvApp, ui: &mut Ui, p: &Palette) {
    header(
        ui,
        p,
        "More",
        "Manage appearance, sharing and diagnostics",
        |_, _| {},
    );

    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            appearance_card(app, ui, p);
            ui.add_space(10.0);
            sharing_card(app, ui, p);
            ui.add_space(10.0);
            file_assoc_card(app, ui, p);
            ui.add_space(10.0);
            engine_card(app, ui, p);
            ui.add_space(10.0);
            about_card(app, ui, p);
        });
}

fn section_title(ui: &mut Ui, p: &Palette, title: &str) {
    ui.label(theme::semi(title).size(11.0).color(p.muted));
}

fn file_assoc_card(app: &mut NpvApp, ui: &mut Ui, p: &Palette) {
    widgets::card(ui, p, |ui| {
        section_title(ui, p, "FILE ASSOCIATION");
        ui.add_space(4.0);
        let already = app.settings.file_assoc || assoc::is_associated();
        ui.label(
            RichText::new("Double-click a `.npvt` or `.npvs` file to import it into your library.")
                .size(12.0)
                .color(p.text),
        );
        ui.label(
            RichText::new(
                "The association is per-user only (no admin required) and the default action is \
                 add-to-library, never silent connect.",
            )
            .size(10.5)
            .color(p.muted),
        );
        ui.add_space(6.0);
        let btn_label = if already {
            "Already associated"
        } else {
            "Associate .npvt / .npvs"
        };
        let btn_resp = widgets::primary_button(ui, p, btn_label);
        if btn_resp.clicked() && !already {
            match assoc::associate() {
                Ok(()) => {
                    app.settings.file_assoc = true;
                    app.persist_settings();
                    app.toast(ToastKind::Ok, ".npvt / .npvs files are now associated.");
                }
                Err(err) => {
                    app.toast(ToastKind::Error, format!("Association failed: {err}"));
                }
            }
        }
        if already {
            ui.label(
                RichText::new(
                    "Files are associated. You can remove the association by uninstalling \
                     or clearing HKCU\\Software\\Classes\\.npvt.",
                )
                .size(10.0)
                .color(p.muted),
            );
        }
    });
}

fn appearance_card(app: &mut NpvApp, ui: &mut Ui, p: &Palette) {
    widgets::card(ui, p, |ui| {
        section_title(ui, p, "APPEARANCE");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for mode in [ThemeMode::Dark, ThemeMode::Light, ThemeMode::Auto] {
                let selected = app.settings.theme == mode;
                let label = mode.label().to_string();
                let text = if selected {
                    RichText::new(&label).size(12.5).color(p.on_accent)
                } else {
                    RichText::new(&label).size(12.5).color(p.text)
                };
                let fill = if selected { p.accent } else { p.card };
                if ui
                    .add(
                        egui::Button::new(text.clone())
                            .fill(fill)
                            .stroke(egui::Stroke::new(
                                1.0,
                                if selected { p.accent } else { p.card_stroke },
                            ))
                            .corner_radius(egui::CornerRadius::same(u8::MAX))
                            .min_size(egui::Vec2::new(86.0, 30.0)),
                    )
                    .clicked()
                {
                    app.set_theme(mode);
                }
            }
        });
        if app.settings.theme == ThemeMode::Auto {
            ui.label(
                RichText::new("Auto follows your Windows light or dark setting.")
                    .size(11.0)
                    .color(p.muted),
            );
        }
    });
}

fn sharing_card(app: &mut NpvApp, ui: &mut Ui, p: &Palette) {
    widgets::card(ui, p, |ui| {
        section_title(ui, p, "SHARING");
        ui.add_space(4.0);
        match app.vault.identity.clone() {
            Some(identity) => {
                let pubkey = crate::store::recipient_pubkey(&identity);
                ui.horizontal(|ui| {
                    ui.label(theme::semi("My public key").size(13.0).color(p.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::ghost_button(ui, p, "Copy").clicked() {
                            ui.ctx().copy_text(pubkey.clone());
                        }
                    });
                });
                ui.add(
                    egui::TextEdit::multiline(&mut pubkey.as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(2)
                        .desired_width(ui.available_width())
                        .interactive(false),
                );
                ui.label(
                    RichText::new(
                        "Recipients of this key can receive configs locked to you; exports are signed with your creator key.",
                    )
                    .size(11.0)
                    .color(p.muted),
                );
            }
            None => {
                ui.label(
                    theme::semi("No creator identity yet")
                        .size(13.0)
                        .color(p.text),
                );
                ui.label(
                    RichText::new(
                        "An identity lets you sign exports and open key-locked configs shared with you.",
                    )
                    .size(11.5)
                    .color(p.muted),
                );
                ui.add_space(2.0);
                if widgets::primary_button(ui, p, "Generate creator identity").clicked() {
                    app.bridge.generate_identity();
                }
            }
        }
    });
}

fn engine_card(app: &mut NpvApp, ui: &mut Ui, p: &Palette) {
    widgets::card(ui, p, |ui| {
        section_title(ui, p, "ENGINE");
        ui.add_space(4.0);

        let engine_status = app.engine_path_cached();
        match &engine_status {
            Some(path) => {
                ui.horizontal(|ui| {
                    widgets::badge(ui, p, p.accent, "FOUND");
                    ui.label(
                        RichText::new(format!("sing-box: {}", path.display()))
                            .family(eframe::egui::FontFamily::Monospace)
                            .size(11.0)
                            .color(p.text),
                    );
                });
            }
            None => {
                ui.horizontal(|ui| {
                    widgets::badge(ui, p, p.danger, "MISSING");
                    ui.label(
                        RichText::new(
                            "sing-box.exe not found next to NPVTunnel.exe, in the data directory under bin\\, or on PATH.",
                        )
                        .size(11.5)
                        .color(p.danger),
                    );
                });
            }
        }
        ui.add_space(6.0);

        egui::Grid::new("engine-settings")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label(RichText::new("SOCKS port").size(12.5).color(p.text));
                if port_field(ui, &mut app.settings.socks_port) {
                    app.persist_settings();
                }
                ui.end_row();

                ui.label(RichText::new("Stats port").size(12.5).color(p.text));
                if port_field(ui, &mut app.settings.clash_port) {
                    app.persist_settings();
                }
                ui.end_row();

                ui.label(RichText::new("DNS server").size(12.5).color(p.text));
                let dns = ui.add(
                    egui::TextEdit::singleline(&mut app.settings.dns_server)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(120.0),
                );
                if dns.changed() {
                    app.persist_settings();
                }
                ui.end_row();
            });

        ui.add_space(4.0);
        if ui
            .checkbox(&mut app.settings.auto_ping_on_connect, "Ping after connect")
            .changed()
        {
            app.persist_settings();
        }
        if ui
            .checkbox(
                &mut app.settings.auto_connect_on_launch,
                "Connect the last config on launch",
            )
            .changed()
        {
            app.persist_settings();
        }
        if ui
            .checkbox(
                &mut app.settings.confirm_disconnect_on_exit,
                "Ask before leaving the tunnel up when closing",
            )
            .changed()
        {
            app.persist_settings();
        }
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);
        if ui
            .checkbox(&mut app.settings.tun_mode, "TUN mode (capture all traffic)")
            .changed()
        {
            app.persist_settings();
        }
        if app.settings.tun_mode {
            ui.label(
                RichText::new(
                    "Routes all system traffic through the tunnel via a virtual network adapter. \
                     Requires administrator privileges.",
                )
                .size(10.0)
                .color(p.warn),
            );
            ui.add_space(2.0);
            ui.label(RichText::new("TUN address (CIDR)").size(11.0).color(p.text));
            ui.add(
                egui::TextEdit::singleline(&mut app.settings.tun_address)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(160.0),
            );
        }
        if ui
            .checkbox(
                &mut app.settings.kill_switch,
                "Kill switch (block traffic when disconnected)",
            )
            .changed()
        {
            app.persist_settings();
        }
        if app.settings.kill_switch {
            ui.label(
                RichText::new(
                    "When the tunnel drops, all non-local network traffic is blocked until \
                     you reconnect or disable the kill switch.",
                )
                .size(10.0)
                .color(p.warn),
            );
        }
        ui.label(
            RichText::new("The SOCKS inbound and stats port always bind 127.0.0.1 only.")
                .size(10.5)
                .color(p.muted),
        );
    });
}

fn port_field(ui: &mut Ui, value: &mut u16) -> bool {
    let mut buf = value.to_string();
    let resp = ui.add(
        egui::TextEdit::singleline(&mut buf)
            .font(egui::TextStyle::Monospace)
            .desired_width(120.0),
    );
    let mut changed = false;
    if resp.changed() {
        if let Ok(parsed) = buf.trim().parse::<u32>() {
            if (1..=65535).contains(&parsed) {
                *value = parsed as u16;
                changed = true;
            }
        }
    }
    changed
}

fn about_card(app: &mut NpvApp, ui: &mut Ui, p: &Palette) {
    widgets::card(ui, p, |ui| {
        section_title(ui, p, "ABOUT");
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!(
                "NPV TUNNEL v{} - native Windows client for the .npvt ecosystem",
                env!("CARGO_PKG_VERSION")
            ))
            .size(12.5)
            .color(p.text),
        );
        ui.label(
            RichText::new("Tunnels run through a bundled sing-box sidecar; configs live in an encrypted local vault.")
                .size(11.0)
                .color(p.muted),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if widgets::ghost_button(ui, p, "Project page").clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(
                    "https://github.com/librewolfsigma/npvtunnel",
                ));
            }
            if widgets::ghost_button(ui, p, "Open data folder").clicked() {
                app.open_data_folder();
            }
        });
    });
}

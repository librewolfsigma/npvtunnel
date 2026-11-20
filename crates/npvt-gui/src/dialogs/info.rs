//! Informational dialogs: public keys, premium, points, assets, about.

use eframe::egui::{self, Color32, FontId, Vec2};

use crate::app::App;
use crate::icons;
use crate::theme;
use crate::vault_actions;
use crate::widgets;

use super::key_row;

impl App {
    pub(crate) fn public_key_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.label(
            egui::RichText::new("My public key")
                .font(theme::bold(19.0))
                .color(p.text),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "Give this key to a creator so they can send you key-locked configs.",
            )
            .font(theme::body())
            .color(p.text_faint),
        );
        ui.add_space(10.0);
        if let Some(pk) = vault_actions::my_public_key(&self.vault) {
            let clicked = key_row(ui, p, "X25519 (config sharing)", &pk, "copy-x25519");
            if clicked {
                self.copy_to_clipboard(pk);
            }
        }
        if let Some(ck) = vault_actions::my_creator_key(&self.vault) {
            let clicked = key_row(ui, p, "Ed25519 (creator key)", &ck, "copy-ed25519");
            if clicked {
                self.copy_to_clipboard(ck);
            }
        }
        ui.add_space(10.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("Close").color(p.accent),
                ))
                .clicked()
            {
                self.close_modal();
            }
        });
    }

    pub(crate) fn premium_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            ui.add_space(6.0);
            let rect = egui::Rect::from_center_size(ui.cursor().center(), Vec2::splat(56.0));
            icons::paint_with(
                ui.painter(),
                rect,
                Color32::from_rgb(0xFB, 0xBF, 0x24),
                |c| icons::medal(c, c.color()),
                2.6,
            );
            ui.add_space(12.0);
            ui.label(egui::RichText::new("NPVTunnel Premium").font(theme::bold(19.0)).color(p.text));
            ui.label(
                egui::RichText::new("This build is free and fully unlocked. There are no ads to remove - everything you see is included.")
                    .font(FontId::new(12.5, egui::FontFamily::Proportional))
                    .color(p.text_secondary),
            );
        });
        ui.add_space(14.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("Nice").color(p.accent),
                ))
                .clicked()
            {
                self.close_modal();
            }
        });
    }

    pub(crate) fn points_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.label(
            egui::RichText::new("Points")
                .font(theme::bold(19.0))
                .color(p.text),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("Points are not needed here: NPVTunnel shows no ads and pays no rewards. Your configs are your points.")
                .font(FontId::new(12.5, egui::FontFamily::Proportional))
                .color(p.text_secondary),
        );
        ui.add_space(14.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("Got it").color(p.accent),
                ))
                .clicked()
            {
                self.close_modal();
            }
        });
    }

    pub(crate) fn assets_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.label(
            egui::RichText::new("User assets")
                .font(theme::bold(19.0))
                .color(p.text),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Geo data files used by custom config routing.")
                .font(theme::body())
                .color(p.text_faint),
        );
        ui.add_space(10.0);
        let names = ["geoip.db", "geosite.db"];
        for (i, name) in names.iter().enumerate() {
            let (rect, resp) = widgets::card(ui, p, 46.0, Some(p.surface), None);
            let painter = ui.painter();
            widgets::label_at(
                painter,
                egui::Align2::LEFT_CENTER,
                egui::Pos2::new(rect.left() + 14.0, rect.center().y),
                name,
                FontId::new(13.0, egui::FontFamily::Proportional),
                p.text,
            );
            widgets::label_at(
                painter,
                egui::Align2::RIGHT_CENTER,
                egui::Pos2::new(rect.right() - 14.0, rect.center().y),
                if self.asset_busy && self.asset_which == i {
                    "downloading..."
                } else {
                    "download"
                },
                FontId::new(12.5, egui::FontFamily::Proportional),
                if self.asset_busy && self.asset_which == i {
                    p.amber
                } else {
                    p.cyan
                },
            );
            if resp.clicked() && !self.asset_busy {
                self.start_asset_download(i, name);
            }
            ui.add_space(4.0);
        }
        ui.add_space(6.0);
        let (received, total) = self.asset_progress.snapshot();
        if total > 0 {
            let bar =
                egui::Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), 6.0));
            widgets::progress_bar(
                ui.painter(),
                bar,
                received as f32 / total as f32,
                p.accent,
                p.cyan,
                p.border,
            );
            ui.add_space(6.0);
        }
        if !self.asset_status.is_empty() {
            ui.label(
                egui::RichText::new(self.asset_status.as_str())
                    .font(FontId::new(11.5, egui::FontFamily::Proportional))
                    .color(p.text_secondary),
            );
        }
        ui.add_space(10.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("Close").color(p.accent),
                ))
                .clicked()
            {
                self.close_modal();
            }
        });
    }

    pub(crate) fn about_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(theme::spaced("NPV TUNNEL")).font(theme::mono(14.0)).color(p.accent));
            ui.label(egui::RichText::new(format!("v{} for Windows", env!("CARGO_PKG_VERSION"))).font(theme::mono(11.0)).color(p.text_faint));
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Security-hardened .npvt tunnel client.\nMIT licensed, source at github.com/librewolfsigma/npvtunnel")
                    .font(theme::body())
                    .color(p.text_secondary),
            );
        });
        ui.add_space(10.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("Copy repo link").color(p.cyan),
                ))
                .clicked()
            {
                self.copy_to_clipboard("https://github.com/librewolfsigma/npvtunnel".to_owned());
            }
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("Close").color(p.accent),
                ))
                .clicked()
            {
                self.close_modal();
            }
        });
    }
}

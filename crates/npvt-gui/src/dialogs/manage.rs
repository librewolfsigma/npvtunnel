//! Management dialogs: deletions, profile edit, settings, per-app proxy.

use eframe::egui::{self, FontId, Widget as _};

use crate::app::{App, Modal};
use crate::theme;
use crate::widgets;

use super::{field_label, footer, footer_custom, pick_engine_binary, Action};

impl App {
    pub(crate) fn delete_profile_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.label(
            egui::RichText::new("Remove config")
                .font(theme::bold(19.0))
                .color(p.text),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "This removes the config from your vault. Anyone you shared it with is unaffected.",
            )
            .font(FontId::new(12.5, egui::FontFamily::Proportional))
            .color(p.text_secondary),
        );
        ui.add_space(14.0);
        match footer_custom(ui, p, "Remove", p.red) {
            Action::Confirm => {
                if let Some(Modal::ConfirmDeleteProfile { id }) = self.modal.clone() {
                    self.delete_profile(&id);
                    self.close_modal();
                }
            }
            Action::Cancel => self.close_modal(),
            Action::None => {}
        }
    }

    pub(crate) fn delete_group_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.label(
            egui::RichText::new("Delete group")
                .font(theme::bold(19.0))
                .color(p.text),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("This removes the group and every config inside it.")
                .font(FontId::new(12.5, egui::FontFamily::Proportional))
                .color(p.text_secondary),
        );
        ui.add_space(14.0);
        match footer_custom(ui, p, "Delete", p.red) {
            Action::Confirm => {
                if let Some(Modal::ConfirmDeleteGroup { label }) = self.modal.clone() {
                    self.delete_group(&label);
                    self.close_modal();
                }
            }
            Action::Cancel => self.close_modal(),
            Action::None => {}
        }
    }

    pub(crate) fn edit_profile_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.label(
            egui::RichText::new("Edit config")
                .font(theme::bold(19.0))
                .color(p.text),
        );
        ui.add_space(8.0);
        field_label(ui, p, "Name");
        egui::TextEdit::singleline(&mut self.ms.edit_name)
            .desired_width(ui.available_width())
            .ui(ui);
        ui.add_space(6.0);
        field_label(ui, p, "Group");
        egui::TextEdit::singleline(&mut self.ms.edit_group)
            .desired_width(ui.available_width())
            .ui(ui);
        ui.add_space(8.0);
        ui.checkbox(&mut self.ms.edit_udp, "UDP relay");
        ui.checkbox(
            &mut self.ms.edit_insecure,
            "Allow insecure TLS (not recommended)",
        );
        ui.add_space(12.0);
        match footer(ui, p, false, "Save") {
            Action::Confirm => {
                if let Some(Modal::EditProfile { id }) = self.modal.clone() {
                    self.apply_profile_edit(&id);
                    self.close_modal();
                }
            }
            Action::Cancel => self.close_modal(),
            Action::None => {}
        }
    }

    pub(crate) fn settings_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.label(
            egui::RichText::new("Settings")
                .font(theme::bold(19.0))
                .color(p.text),
        );
        ui.add_space(10.0);
        let ms = &mut self.ms;
        field_label(ui, p, "SOCKS listen port");
        ui.add(egui::TextEdit::singleline(&mut ms.set_socks).desired_width(140.0));
        field_label(ui, p, "Traffic API port");
        ui.add(egui::TextEdit::singleline(&mut ms.set_clash).desired_width(140.0));
        field_label(ui, p, "DNS server");
        ui.add(egui::TextEdit::singleline(&mut ms.set_dns).desired_width(ui.available_width()));
        field_label(ui, p, "Monthly quota (GB, 0 = unlimited)");
        ui.add(egui::TextEdit::singleline(&mut ms.set_quota).desired_width(140.0));
        field_label(ui, p, "Engine path (optional)");
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut ms.set_engine)
                    .desired_width(ui.available_width() - 90.0),
            );
            if ui.button("Browse").clicked() {
                if let Some(path) = pick_engine_binary() {
                    ms.set_engine = path.to_string_lossy().to_string();
                }
            }
        });
        ui.add_space(6.0);
        field_label(ui, p, "Engine log level");
        egui::ComboBox::from_id_salt("loglevel")
            .selected_text(["warn", "info", "error", "debug"][ms.set_loglevel])
            .show_ui(ui, |ui| {
                for (i, level) in ["warn", "info", "error", "debug"].iter().enumerate() {
                    ui.selectable_value(&mut ms.set_loglevel, i, *level);
                }
            });
        ui.add_space(6.0);
        ui.checkbox(&mut ms.set_autostart, "Start with Windows");
        ui.checkbox(&mut ms.set_launch, "Connect on launch");
        ui.add_space(12.0);
        match footer(ui, p, false, "Save settings") {
            Action::Confirm => {
                self.apply_settings_dialog();
            }
            Action::Cancel => self.close_modal(),
            Action::None => {}
        }
    }

    pub(crate) fn per_app_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.label(
            egui::RichText::new("Per-app Proxy")
                .font(theme::bold(19.0))
                .color(p.text),
        );
        ui.add_space(8.0);
        let modes = ["Off", "Block apps", "Allow only"];
        if let Some(i) = widgets::segmented(ui, p, &modes, self.ms.per_app_mode as usize, &[]) {
            self.ms.per_app_mode = match i {
                1 => crate::settings::PerAppMode::Block,
                2 => crate::settings::PerAppMode::Allow,
                _ => crate::settings::PerAppMode::Off,
            };
        }
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("Process names, one per line (e.g. steam.exe). Block routes those apps outside the tunnel; Allow only tunnels those apps.")
                .font(theme::body())
                .color(p.text_faint),
        );
        egui::TextEdit::multiline(&mut self.ms.per_app_text)
            .desired_width(ui.available_width())
            .desired_rows(5)
            .font(egui::TextStyle::Monospace)
            .ui(ui);
        ui.add_space(12.0);
        match footer(ui, p, false, "Apply") {
            Action::Confirm => {
                self.settings.per_app_mode = self.ms.per_app_mode;
                self.settings.per_app = self
                    .ms
                    .per_app_text
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_owned)
                    .collect();
                self.settings.save();
                self.push_log(
                    "INFO",
                    "per-app proxy rules updated (applies on next connect)",
                );
                self.toast_now("Saved - reconnect to apply");
                self.close_modal();
            }
            Action::Cancel => self.close_modal(),
            Action::None => {}
        }
    }
}

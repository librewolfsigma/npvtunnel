//! The export sheet: filename, lock policy, delivery, secrecy, errors.

use eframe::egui::{self, Sense, Vec2, Widget as _};

use crate::app::App;
use crate::icons;
use crate::theme::{self, Palette};
use crate::widgets;

impl App {
    /// The export sheet, assembled from section helpers so every piece
    /// stays reviewable on its own.
    pub(crate) fn export_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        self.export_title_row(ui, p);
        ui.add_space(8.0);
        self.export_action_buttons(ui, p);
        self.export_filename_row(ui, p);
        self.export_lock_section(ui, p);
        self.export_delivery_section(ui, p);
        self.export_secrecy_section(ui, p);
        if let Some(err) = &self.ms.export.err {
            ui.label(egui::RichText::new(err).font(theme::body()).color(p.red));
        }
        ui.add_space(4.0);
    }

    /// Sheet title with the back chevron.
    fn export_title_row(&mut self, ui: &mut egui::Ui, p: Palette) {
        // Title row with back chevron.
        let (back_rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::hover());
        let painter = ui.painter();
        let chev = egui::Rect::from_center_size(
            egui::Pos2::new(back_rect.left() + 10.0, back_rect.center().y),
            Vec2::splat(20.0),
        );
        let title_galley = ui
            .ctx()
            .fonts(|f| f.layout_no_wrap("Share config".to_owned(), theme::bold(18.0), p.text));
        painter.galley(
            egui::Pos2::new(
                back_rect.left() + 30.0,
                back_rect.center().y - title_galley.size().y / 2.0,
            ),
            title_galley,
            p.text,
        );
        if widgets::icon_button(
            ui,
            chev,
            p.text_secondary,
            icons::chevron_left,
            "export-back",
            "Back",
        ) {
            self.close_modal();
        }
        ui.add_space(8.0);
    }

    /// The primary export button and the copy-as-link fallback.
    fn export_action_buttons(&mut self, ui: &mut egui::Ui, p: Palette) {
        // Export button.
        let enabled = !self.ms.export.filename.trim().is_empty();
        let export_btn = egui::Button::new(
            egui::RichText::new("Export Configuration")
                .font(theme::semibold(13.5))
                .color(if enabled { p.accent } else { p.text_faint }),
        )
        .fill(p.surface)
        .stroke(egui::Stroke::new(
            1.0,
            if enabled { p.border_sel } else { p.border },
        ))
        .rounding(egui::Rounding::same(10.0))
        .min_size(Vec2::new(ui.available_width(), 42.0));
        if ui.add_enabled(enabled, export_btn).clicked() {
            self.export_profile();
        }
        ui.add_space(6.0);
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("Copy as npvt:// link")
                        .font(theme::body())
                        .color(p.cyan),
                ))
                .clicked()
            {
                self.export_link_to_clipboard();
            }
        });
        ui.add_space(10.0);
    }

    fn export_filename_row(&mut self, ui: &mut egui::Ui, p: Palette) {
        super::field_label(ui, p, "Config Filename");
        egui::TextEdit::singleline(&mut self.ms.export.filename)
            .hint_text("my-shop.npvt")
            .desired_width(ui.available_width())
            .ui(ui);
        ui.add_space(12.0);
    }

    /// Lock radio group: locked (keys/passphrase) vs open container.
    fn export_lock_section(&mut self, ui: &mut egui::Ui, p: Palette) {
        // Lock radio group.
        ui.label(
            egui::RichText::new("Lock this config?")
                .font(theme::semibold(14.5))
                .color(p.text),
        );
        ui.add_space(4.0);
        if widgets::radio_row(
            ui,
            p,
            "Lock it",
            "Recipients can connect, but cannot view or edit the config.",
            self.ms.export.lock == 0,
        ) {
            self.ms.export.lock = 0;
        }
        ui.add_space(2.0);
        if widgets::radio_row(
            ui,
            p,
            "Keep it open",
            "Recipients can view, edit and share the config.",
            self.ms.export.lock == 1,
        ) {
            self.ms.export.lock = 1;
        }
        ui.add_space(10.0);
    }

    /// Delivery choice plus the conditional keys/passphrase entry fields.
    fn export_delivery_section(&mut self, ui: &mut egui::Ui, p: Palette) {
        // Deliver it with (only when locked).
        ui.label(
            egui::RichText::new("Deliver it with")
                .font(theme::semibold(14.5))
                .color(p.text),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Serverless: saves a file")
                .font(theme::mono(10.5))
                .color(p.text_faint),
        );
        ui.add_space(2.0);
        if widgets::radio_row(
            ui,
            p,
            "Specific people",
            "Enter one or more recipient public keys.",
            self.ms.export.deliver == 0,
        ) {
            self.ms.export.deliver = 0;
        }
        ui.add_space(2.0);
        if widgets::radio_row(
            ui,
            p,
            "Passphrase",
            "Post the file anywhere; share the passphrase to open it.",
            self.ms.export.deliver == 1,
        ) {
            self.ms.export.deliver = 1;
        }
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Your server: gives a link")
                .font(theme::mono(10.5))
                .color(p.text_faint),
        );
        widgets::radio_row_disabled(
            ui,
            p,
            "Share link",
            "Share link handles one config at a time.",
        );
        ui.add_space(6.0);
        if self.ms.export.lock == 0 && self.ms.export.deliver == 0 {
            super::field_label(ui, p, "Recipient public keys");
            egui::TextEdit::multiline(&mut self.ms.export.keys)
                .hint_text("one base64url X25519 key per line")
                .desired_width(ui.available_width())
                .desired_rows(3)
                .font(egui::TextStyle::Monospace)
                .ui(ui);
        } else if self.ms.export.lock == 0 && self.ms.export.deliver == 1 {
            super::field_label(ui, p, "Passphrase");
            egui::TextEdit::singleline(&mut self.ms.export.pass)
                .hint_text("at least 8 characters")
                .password(true)
                .desired_width(ui.available_width())
                .ui(ui);
            egui::TextEdit::singleline(&mut self.ms.export.pass2)
                .hint_text("repeat passphrase")
                .password(true)
                .desired_width(ui.available_width())
                .ui(ui);
        }
    }

    /// Secrecy slider with its MINIMAL/MODERATE/MAXIMUM label.
    fn export_secrecy_section(&mut self, ui: &mut egui::Ui, p: Palette) {
        // Secrecy slider.
        let secrecy_label = super::secrecy_word(self.ms.export.secrecy);
        let (row_rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 20.0), Sense::hover());
        let painter = ui.painter();
        widgets::label_at(
            painter,
            egui::Align2::LEFT_CENTER,
            egui::Pos2::new(row_rect.left(), row_rect.center().y),
            "Secrecy",
            theme::semibold(14.5),
            p.text,
        );
        widgets::label_at(
            painter,
            egui::Align2::RIGHT_CENTER,
            egui::Pos2::new(row_rect.right(), row_rect.center().y),
            secrecy_label,
            theme::mono_bold(12.0),
            p.amber,
        );
        ui.add_space(6.0);
        let (value, changed) = widgets::gradient_slider(
            ui,
            p,
            12.0,
            &[p.red, p.amber, p.accent],
            self.ms.export.secrecy,
        );
        if changed {
            self.ms.export.secrecy = (value * 3.0).round() / 3.0;
        }
        ui.label(
            egui::RichText::new("Available to the recipients you select.")
                .font(theme::body())
                .color(p.text_faint),
        );
        ui.add_space(10.0);
        if let Some(err) = &self.ms.export.err {
            ui.label(egui::RichText::new(err).font(theme::body()).color(p.red));
        }
        ui.add_space(4.0);
    }
}

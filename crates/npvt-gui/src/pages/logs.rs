//! Logs: the engine sidecar log viewer with auto-refresh and copy.

use eframe::egui::{self, RichText, ScrollArea, Ui};

use super::header;
use crate::theme::Palette;
use crate::widgets;
use crate::NpvApp;

pub fn show(app: &mut NpvApp, ui: &mut Ui, p: &Palette) {
    header(ui, p, "Engine log", "sing-box sidecar output", |ui, p| {
        ui.checkbox(&mut app.logs_auto, "Auto-refresh");
        if widgets::ghost_button(ui, p, "Refresh").clicked() {
            app.refresh_logs();
        }
        if widgets::ghost_button(ui, p, "Copy").clicked() {
            ui.ctx().copy_text(app.logs.clone());
        }
    });

    if app.logs.is_empty() {
        super::empty_state(
            ui,
            p,
            "Log is empty",
            "The engine writes here once it starts. Connect to a config first.",
        );
        return;
    }

    ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        widgets::card(ui, p, |ui| {
            ui.set_min_width(ui.available_width());
            ui.add(
                egui::TextEdit::multiline(&mut app.logs.as_str())
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(24)
                    .desired_width(ui.available_width())
                    .interactive(true),
            );
            ui.label(
                RichText::new(format!("{} lines", app.logs.lines().count()))
                    .size(10.5)
                    .color(p.muted),
            );
        });
    });
}

//! LOGS: live sing-box engine log with filter, copy and clear.

use egui::Color32;

use crate::app::App;
use crate::data;
use crate::icons::{self, Icon};
use crate::theme::{self, Palette};
use crate::widgets;

pub const CONTENT_W: f32 = 470.0;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette;
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.with_layout(
            egui::Layout::top_down_justified(egui::Align::Center),
            |ui| {
                ui.set_max_width(CONTENT_W);
                ui.add_space(4.0);
                widgets::header(ui, &p, false, false);

                // Toolbar: filter + actions.
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    widgets::tracked_label(ui, &p, "ENGINE LOG");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::icon_button(ui, Icon::Trash, 26.0, p.text_dim, None)
                            .on_hover_text("Clear the view")
                            .clicked()
                        {
                            if let Ok(mut guard) = app.shared.log_tail.lock() {
                                guard.clear();
                            }
                        }
                        if icons::icon_button(ui, Icon::Copy, 26.0, p.text_dim, None)
                            .on_hover_text("Copy log to clipboard")
                            .clicked()
                        {
                            let text = app
                                .shared
                                .log_tail
                                .lock()
                                .map(|g| g.clone())
                                .unwrap_or_default();
                            copy_to_clipboard(app, &text);
                        }
                        if icons::icon_button(ui, Icon::Folder, 26.0, p.text_dim, None)
                            .on_hover_text("Open log file")
                            .clicked()
                        {
                            if let Ok(dir) = npvt_core::engine::work_dir() {
                                data::reveal(&dir);
                            }
                        }
                    });
                });

                let filter = app.search.clone();
                ui.horizontal(|ui| {
                    let field = egui::TextEdit::singleline(&mut app.search)
                        .hint_text("Filter lines…")
                        .desired_width(ui.available_width())
                        .font(egui::TextStyle::Monospace);
                    ui.add(field);
                });

                // Log body.
                let text = app
                    .shared
                    .log_tail
                    .lock()
                    .map(|g| g.clone())
                    .unwrap_or_default();
                let lines: Vec<&str> = text
                    .lines()
                    .filter(|l| {
                        filter.is_empty() || l.to_lowercase().contains(&filter.to_lowercase())
                    })
                    .collect();
                let _height = ui.available_height() - 60.0;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        if lines.is_empty() {
                            ui.add_space(24.0);
                            ui.vertical_centered(|ui| {
                                ui.label(theme::mono(
                                    "No engine output yet.\nStart a connection to see logs here.",
                                    11.5,
                                    p.text_faint,
                                ));
                            });
                        }
                        egui::Frame::default()
                            .fill(p.input_bg)
                            .rounding(10.0)
                            .inner_margin(10.0)
                            .show(ui, |ui| {
                                for line in lines.iter().take(500) {
                                    let color = line_color(line, &p);
                                    ui.label(theme::mono((*line).to_string(), 10.5, color));
                                }
                            });
                    });
            },
        );
    });
}

fn line_color(line: &str, p: &Palette) -> Color32 {
    let lower = line.to_lowercase();
    if lower.contains("\"level\":\"error\"") || lower.contains("error") || lower.contains("fatal") {
        p.red
    } else if lower.contains("warn") {
        p.orange
    } else if lower.contains("info") {
        p.teal_text
    } else {
        p.text_dim
    }
}

fn copy_to_clipboard(app: &mut App, text: &str) {
    match arboard::Clipboard::new() {
        Ok(mut clip) => match clip.set_text(text.to_string()) {
            Ok(()) => app.toasts.show("Log copied to clipboard"),
            Err(e) => app.toasts.error(format!("Clipboard: {e}")),
        },
        Err(e) => app.toasts.error(format!("Clipboard: {e}")),
    }
}

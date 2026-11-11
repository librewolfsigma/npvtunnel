//! The four app pages. Each page is a plain function over the app state so
//! state mutations and bridge calls stay explicit.

pub mod configs;
pub mod home;
pub mod logs;
pub mod more;

use eframe::egui::{Align, Color32, Layout, Ui};

use crate::theme::{self, Palette};
use crate::widgets;

/// Which page is active in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Configs,
    Logs,
    More,
}

/// Standard page header: big title + optional right-aligned controls.
pub fn header(
    ui: &mut Ui,
    p: &Palette,
    title: &str,
    subtitle: &str,
    right: impl FnOnce(&mut Ui, &Palette),
) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(theme::bold(title).size(21.0).color(p.text));
            if !subtitle.is_empty() {
                ui.label(
                    eframe::egui::RichText::new(subtitle)
                        .size(11.5)
                        .color(p.muted),
                );
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            right(ui, p);
        });
    });
    ui.add_space(6.0);
}

/// A hint shown when a page has nothing to show yet.
pub fn empty_state(ui: &mut Ui, p: &Palette, title: &str, body: &str) {
    widgets::card(ui, p, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(18.0);
            ui.label(theme::semi(title).size(15.0).color(p.text));
            ui.add_space(4.0);
            ui.label(eframe::egui::RichText::new(body).size(12.0).color(p.muted));
            ui.add_space(18.0);
        });
    });
}

/// Ping latency coloring helper.
pub fn ping_color(ms: u16) -> Color32 {
    if ms < 100 {
        Color32::from_rgb(46, 204, 113)
    } else if ms < 250 {
        Color32::from_rgb(255, 180, 84)
    } else {
        Color32::from_rgb(255, 107, 107)
    }
}

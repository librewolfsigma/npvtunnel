//! Modal dialogs: the "Add config" sheet from the reference design, the
//! export sheet (lock / secrecy / delivery), settings, per-app proxy, keys,
//! premium/points/about and confirmation prompts. Implemented as methods on
//! `App`.

use eframe::egui::{self, Color32, Vec2};

use crate::app::{App, Modal, ModalState};
use crate::icons;
use crate::theme::{self, Palette};
use crate::widgets;

pub mod config;
pub mod export;
pub mod info;
pub mod manage;

/// What a dialog's footer buttons decided this frame.
#[derive(PartialEq, Eq)]
pub(crate) enum Action {
    None,
    Confirm,
    Cancel,
}

pub(crate) fn footer(ui: &mut egui::Ui, p: Palette, busy: bool, confirm_label: &str) -> Action {
    let mut action = Action::None;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let confirm = egui::Button::new(
            egui::RichText::new(if busy { "Working..." } else { confirm_label })
                .color(p.accent)
                .font(theme::semibold(13.5)),
        )
        .fill(p.surface_hi)
        .stroke(egui::Stroke::new(1.0, p.border_sel))
        .rounding(egui::Rounding::same(10.0));
        if ui.add_enabled(!busy, confirm).clicked() {
            action = Action::Confirm;
        }
        if ui
            .add(egui::Button::new(
                egui::RichText::new("Cancel").color(p.text_secondary),
            ))
            .clicked()
        {
            action = Action::Cancel;
        }
    });
    action
}

pub(crate) fn footer_custom(
    ui: &mut egui::Ui,
    p: Palette,
    confirm_label: &str,
    confirm_color: Color32,
) -> Action {
    let mut action = Action::None;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let confirm = egui::Button::new(
            egui::RichText::new(confirm_label)
                .color(confirm_color)
                .font(theme::semibold(13.5)),
        )
        .fill(p.surface)
        .stroke(egui::Stroke::new(1.0, confirm_color))
        .rounding(egui::Rounding::same(10.0));
        if ui.add(confirm).clicked() {
            action = Action::Confirm;
        }
        if ui
            .add(egui::Button::new(
                egui::RichText::new("Cancel").color(p.text_secondary),
            ))
            .clicked()
        {
            action = Action::Cancel;
        }
    });
    action
}

impl App {
    /// Draw the active modal (if any). Returns true when it stays open.
    pub fn draw_modal(&mut self, ctx: &egui::Context) -> bool {
        let Some(modal) = self.modal.clone() else {
            return false;
        };
        let p = self.palette;
        match modal {
            Modal::AddConfig => widgets::modal(ctx, p, "add-config", 360.0, 430.0, |ui| {
                self.add_config_dialog(ui)
            }),
            Modal::Subscription => widgets::modal(ctx, p, "subscription", 390.0, 320.0, |ui| {
                self.subscription_dialog(ui)
            }),
            Modal::ManualAdd => {
                widgets::modal(ctx, p, "manual", 390.0, 380.0, |ui| self.manual_dialog(ui))
            }
            Modal::NewGroup => widgets::modal(ctx, p, "new-group", 340.0, 220.0, |ui| {
                self.new_group_dialog(ui)
            }),
            Modal::Sort => widgets::modal(ctx, p, "sort", 300.0, 240.0, |ui| self.sort_dialog(ui)),
            Modal::Passphrase => widgets::modal(ctx, p, "passphrase", 380.0, 300.0, |ui| {
                self.passphrase_dialog(ui)
            }),
            Modal::Export { .. } => {
                widgets::modal(ctx, p, "export", 440.0, 660.0, |ui| self.export_dialog(ui))
            }
            Modal::RenameGroup { .. } => {
                widgets::modal(ctx, p, "rename-group", 340.0, 200.0, |ui| {
                    self.rename_group_dialog(ui)
                })
            }
            Modal::EditProfile { .. } => {
                widgets::modal(ctx, p, "edit-profile", 380.0, 380.0, |ui| {
                    self.edit_profile_dialog(ui)
                })
            }
            Modal::ConfirmDeleteProfile { .. } => {
                widgets::modal(ctx, p, "del-profile", 340.0, 220.0, |ui| {
                    self.delete_profile_dialog(ui)
                })
            }
            Modal::ConfirmDeleteGroup { .. } => {
                widgets::modal(ctx, p, "del-group", 340.0, 220.0, |ui| {
                    self.delete_group_dialog(ui)
                })
            }
            Modal::Settings => widgets::modal(ctx, p, "settings", 430.0, 640.0, |ui| {
                self.settings_dialog(ui)
            }),
            Modal::PerApp => widgets::modal(ctx, p, "per-app", 430.0, 560.0, |ui| {
                self.per_app_dialog(ui)
            }),
            Modal::PublicKey => widgets::modal(ctx, p, "pubkey", 440.0, 320.0, |ui| {
                self.public_key_dialog(ui)
            }),
            Modal::Premium => widgets::modal(ctx, p, "premium", 380.0, 250.0, |ui| {
                self.premium_dialog(ui)
            }),
            Modal::Points => {
                widgets::modal(ctx, p, "points", 380.0, 230.0, |ui| self.points_dialog(ui))
            }
            Modal::Assets => {
                widgets::modal(ctx, p, "assets", 430.0, 360.0, |ui| self.assets_dialog(ui))
            }
            Modal::About => {
                widgets::modal(ctx, p, "about", 400.0, 250.0, |ui| self.about_dialog(ui))
            }
        }
    }
}

impl ModalState {
    pub fn new() -> Self {
        ModalState {
            sub_url: String::new(),
            sub_group: String::new(),
            sub_busy: false,
            manual_link: String::new(),
            manual_name: String::new(),
            manual_group: String::new(),
            group_name: String::new(),
            pass_entry: String::new(),
            pass_err: None,
            pending_locked: None,
            export: Default::default(),
            edit_id: String::new(),
            edit_name: String::new(),
            edit_group: String::new(),
            edit_udp: true,
            edit_insecure: false,
            set_socks: "2080".to_owned(),
            set_clash: "19090".to_owned(),
            set_dns: "8.8.8.8".to_owned(),
            set_engine: String::new(),
            set_quota: "100".to_owned(),
            set_autostart: false,
            set_launch: false,
            set_loglevel: 0,
            per_app_text: String::new(),
            per_app_mode: crate::settings::PerAppMode::Off,
        }
    }
}

pub(crate) fn secrecy_word(t: f32) -> &'static str {
    if t < 0.34 {
        "MINIMAL"
    } else if t < 0.67 {
        "MODERATE"
    } else {
        "MAXIMUM"
    }
}

pub(crate) fn field_label(ui: &mut egui::Ui, p: Palette, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .font(theme::body())
            .color(p.text_secondary),
    );
}

pub(crate) fn key_row(ui: &mut egui::Ui, p: Palette, title: &str, key: &str, id: &str) -> bool {
    field_label(ui, p, title);
    let mut clicked = false;
    ui.horizontal(|ui| {
        let mut display = key.to_owned();
        ui.add(
            egui::TextEdit::singleline(&mut display)
                .font(egui::TextStyle::Monospace)
                .desired_width(ui.available_width() - 60.0),
        );
        if widgets::icon_button(
            ui,
            egui::Rect::from_center_size(
                ui.min_rect().right_center() + Vec2::new(10.0, 0.0),
                Vec2::splat(22.0),
            ),
            p.cyan,
            icons::clipboard,
            id,
            "Copy",
        ) {
            clicked = true;
        }
    });
    ui.add_space(6.0);
    clicked
}

#[cfg(windows)]
pub(crate) fn pick_engine_binary() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter("sing-box", &["exe"])
        .pick_file()
}

#[cfg(not(windows))]
pub(crate) fn pick_engine_binary() -> Option<std::path::PathBuf> {
    None
}

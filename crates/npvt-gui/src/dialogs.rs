//! Modal dialogs: the "Add config" sheet from the reference design, the
//! export sheet (lock / secrecy / delivery), settings, per-app proxy, keys,
//! premium/points/about and confirmation prompts. Implemented as methods on
//! `App`.

use eframe::egui::{self, Color32, FontId, Sense, Vec2, Widget as _};

use crate::app::{App, Modal, ModalState};
use crate::icons;
use crate::theme::{self, Palette};
use crate::vault_actions;
use crate::widgets;

/// What a dialog's footer buttons decided this frame.
#[derive(PartialEq, Eq)]
enum Action {
    None,
    Confirm,
    Cancel,
}

fn footer(ui: &mut egui::Ui, p: Palette, busy: bool, confirm_label: &str) -> Action {
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

fn footer_custom(
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

    fn add_config_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.label(
            egui::RichText::new("Add config")
                .font(theme::bold(19.0))
                .color(p.text),
        );
        ui.add_space(14.0);
        let rows: [(icons::IconFn, &str); 6] = [
            (icons::rss, "Add subscription"),
            (icons::clipboard, "Import config from Clipboard"),
            (icons::qr, "Import config from QR Code"),
            (icons::download, "Import config file (.npvs / .npvt)"),
            (icons::plus, "Add config manually"),
            (icons::new_group_icon, "New connection group"),
        ];
        let mut picked = None;
        for (i, (draw, label)) in rows.into_iter().enumerate() {
            let (rect, resp) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), Sense::click());
            let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            let painter = ui.painter();
            if resp.hovered() {
                painter.rect_filled(rect, 8.0, p.surface);
            }
            let icon_rect = egui::Rect::from_center_size(
                egui::Pos2::new(rect.left() + 20.0, rect.center().y),
                Vec2::splat(22.0),
            );
            icons::paint_with(
                painter,
                icon_rect,
                p.text_secondary,
                draw,
                icons::ICON_STROKE,
            );
            widgets::label_at(
                painter,
                egui::Align2::LEFT_CENTER,
                egui::Pos2::new(rect.left() + 44.0, rect.center().y),
                label,
                FontId::new(14.0, egui::FontFamily::Proportional),
                p.text,
            );
            if resp.clicked() {
                picked = Some(i);
            }
            ui.add_space(2.0);
        }
        ui.add_space(6.0);
        let mut action = Action::None;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("Cancel")
                        .color(p.accent)
                        .font(theme::semibold(13.5)),
                ))
                .clicked()
            {
                action = Action::Cancel;
            }
        });
        if action == Action::Cancel {
            self.close_modal();
            return;
        }
        match picked {
            Some(0) => self.open_modal(Modal::Subscription),
            Some(1) => {
                self.close_modal();
                self.import_clipboard();
            }
            Some(2) => {
                self.close_modal();
                self.import_qr();
            }
            Some(3) => {
                self.close_modal();
                self.import_file_dialog();
            }
            Some(4) => self.open_modal(Modal::ManualAdd),
            Some(5) => self.open_modal(Modal::NewGroup),
            _ => {}
        }
    }

    fn subscription_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.label(
            egui::RichText::new("Add subscription")
                .font(theme::bold(19.0))
                .color(p.text),
        );
        ui.add_space(8.0);
        field_label(ui, p, "Subscription URL");
        egui::TextEdit::singleline(&mut self.ms.sub_url)
            .hint_text("https://example.com/sub")
            .desired_width(ui.available_width())
            .ui(ui);
        ui.add_space(6.0);
        field_label(ui, p, "Group name");
        egui::TextEdit::singleline(&mut self.ms.sub_group)
            .hint_text("Global Network")
            .desired_width(ui.available_width())
            .ui(ui);
        ui.add_space(12.0);
        match footer(ui, p, self.ms.sub_busy, "Add subscription") {
            Action::Confirm => {
                let url = self.ms.sub_url.trim().to_owned();
                let group = self.ms.sub_group.trim().to_owned();
                if url.is_empty() || group.is_empty() {
                    self.toast_now("URL and group name are required");
                } else {
                    self.ms.sub_busy = true;
                    self.fetch_subscription_async(url, group);
                }
            }
            Action::Cancel => self.close_modal(),
            Action::None => {}
        }
    }

    fn manual_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.label(
            egui::RichText::new("Add config manually")
                .font(theme::bold(19.0))
                .color(p.text),
        );
        ui.add_space(8.0);
        field_label(ui, p, "Name (optional)");
        egui::TextEdit::singleline(&mut self.ms.manual_name)
            .hint_text("My shop")
            .desired_width(ui.available_width())
            .ui(ui);
        ui.add_space(6.0);
        field_label(ui, p, "Share link");
        egui::TextEdit::multiline(&mut self.ms.manual_link)
            .hint_text("vless://... / vmess://... / trojan://... / ss://...")
            .desired_width(ui.available_width())
            .desired_rows(3)
            .font(egui::TextStyle::Monospace)
            .ui(ui);
        ui.add_space(6.0);
        field_label(ui, p, "Group (optional)");
        egui::TextEdit::singleline(&mut self.ms.manual_group)
            .hint_text("Saved")
            .desired_width(ui.available_width())
            .ui(ui);
        ui.add_space(12.0);
        match footer(ui, p, false, "Add config") {
            Action::Confirm => {
                let link = self.ms.manual_link.trim().to_owned();
                if link.is_empty() {
                    self.toast_now("Paste a share link first");
                    return;
                }
                let group = match self.ms.manual_group.trim() {
                    "" => None,
                    g => Some(g.to_owned()),
                };
                match vault_actions::import_one(&mut self.vault, &link, group.as_deref()) {
                    Ok(name) => {
                        self.persist_vault();
                        self.push_log("INFO", &format!("imported '{name}' manually"));
                        self.toast_now(format!("Imported {name}"));
                        self.close_modal();
                    }
                    Err(e) => self.toast_now(format!("Import failed: {e}")),
                }
            }
            Action::Cancel => self.close_modal(),
            Action::None => {}
        }
    }

    fn new_group_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.label(
            egui::RichText::new("New connection group")
                .font(theme::bold(19.0))
                .color(p.text),
        );
        ui.add_space(8.0);
        field_label(ui, p, "Group name");
        egui::TextEdit::singleline(&mut self.ms.group_name)
            .hint_text("Smart Route")
            .desired_width(ui.available_width())
            .ui(ui);
        ui.add_space(12.0);
        match footer(ui, p, false, "Create group") {
            Action::Confirm => {
                let name = self.ms.group_name.trim().to_owned();
                match npvt_core::vault::clean_name_input(&name) {
                    Ok(name) => {
                        if !self.settings.extra_groups.contains(&name) {
                            self.settings.extra_groups.push(name.clone());
                            self.settings.save();
                        }
                        self.expanded.insert(name);
                        self.toast_now("Group created");
                        self.close_modal();
                    }
                    Err(e) => self.toast_now(format!("Invalid name: {e}")),
                }
            }
            Action::Cancel => self.close_modal(),
            Action::None => {}
        }
    }

    fn rename_group_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.label(
            egui::RichText::new("Rename group")
                .font(theme::bold(19.0))
                .color(p.text),
        );
        ui.add_space(8.0);
        egui::TextEdit::singleline(&mut self.ms.group_name)
            .desired_width(ui.available_width())
            .ui(ui);
        ui.add_space(12.0);
        match footer(ui, p, false, "Rename") {
            Action::Confirm => {
                if let Some(Modal::RenameGroup { label }) = self.modal.clone() {
                    let new_name = self.ms.group_name.trim().to_owned();
                    if npvt_core::vault::clean_name_input(&new_name).is_ok() && new_name != label {
                        self.rename_group(&label, &new_name);
                    }
                    self.close_modal();
                }
            }
            Action::Cancel => self.close_modal(),
            Action::None => {}
        }
    }

    fn sort_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.label(
            egui::RichText::new("Sort configs")
                .font(theme::bold(19.0))
                .color(p.text),
        );
        ui.add_space(8.0);
        let options = ["Name", "Ping", "Recently added"];
        for (i, label) in options.iter().enumerate() {
            let selected = self.settings.sort as usize == i;
            let (rect, resp) = widgets::card(
                ui,
                p,
                42.0,
                Some(if selected { p.surface_hi } else { p.surface }),
                None,
            );
            widgets::label_at(
                ui.painter(),
                egui::Align2::LEFT_CENTER,
                egui::Pos2::new(rect.left() + 16.0, rect.center().y),
                label,
                FontId::new(13.5, egui::FontFamily::Proportional),
                if selected { p.accent } else { p.text },
            );
            if resp.clicked() {
                self.settings.sort = match i {
                    1 => crate::settings::SortMode::Ping,
                    2 => crate::settings::SortMode::Added,
                    _ => crate::settings::SortMode::Name,
                };
                self.settings.save();
                self.close_modal();
            }
            ui.add_space(4.0);
        }
    }

    fn passphrase_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.label(
            egui::RichText::new("Locked config")
                .font(theme::bold(19.0))
                .color(p.text),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "This .npvt container is protected with a passphrase. Enter it to import.",
            )
            .font(FontId::new(12.5, egui::FontFamily::Proportional))
            .color(p.text_secondary),
        );
        ui.add_space(10.0);
        egui::TextEdit::singleline(&mut self.ms.pass_entry)
            .hint_text("Passphrase")
            .password(true)
            .desired_width(ui.available_width())
            .ui(ui);
        if let Some(err) = &self.ms.pass_err {
            ui.label(
                egui::RichText::new(err)
                    .font(FontId::new(12.0, egui::FontFamily::Proportional))
                    .color(p.red),
            );
        }
        ui.add_space(12.0);
        match footer_custom(ui, p, "Unlock", p.accent) {
            Action::Confirm => {
                let Some(bytes) = self.ms.pending_locked.clone() else {
                    self.close_modal();
                    return;
                };
                let pass = self.ms.pass_entry.clone();
                match vault_actions::decode_container_passphrase(&bytes, &pass) {
                    Ok(decoded) => {
                        let mut profile = decoded.profile;
                        profile.id = npvt_core::profile::new_profile_id();
                        if profile.group.is_none() {
                            if let Some(hint) = decoded.policy.group_hint.clone() {
                                profile.group = Some(hint);
                            }
                        }
                        let name = profile.name.clone();
                        let _ = npvt_core::vault::upsert_profile(&mut self.vault, profile);
                        self.persist_vault();
                        self.push_log("INFO", &format!("imported locked container '{name}'"));
                        self.toast_now(format!("Imported {name}"));
                        self.ms.pending_locked = None;
                        self.ms.pass_entry.clear();
                        self.ms.pass_err = None;
                        self.close_modal();
                    }
                    Err(e) => self.ms.pass_err = Some(format!("Wrong passphrase: {e}")),
                }
            }
            Action::Cancel => {
                self.ms.pending_locked = None;
                self.close_modal();
            }
            Action::None => {}
        }
    }

    fn export_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
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
                        .font(FontId::new(12.0, egui::FontFamily::Proportional))
                        .color(p.cyan),
                ))
                .clicked()
            {
                self.export_link_to_clipboard();
            }
        });
        ui.add_space(10.0);

        field_label(ui, p, "Config Filename");
        egui::TextEdit::singleline(&mut self.ms.export.filename)
            .hint_text("my-shop.npvt")
            .desired_width(ui.available_width())
            .ui(ui);
        ui.add_space(12.0);

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
            field_label(ui, p, "Recipient public keys");
            egui::TextEdit::multiline(&mut self.ms.export.keys)
                .hint_text("one base64url X25519 key per line")
                .desired_width(ui.available_width())
                .desired_rows(3)
                .font(egui::TextStyle::Monospace)
                .ui(ui);
        } else if self.ms.export.lock == 0 && self.ms.export.deliver == 1 {
            field_label(ui, p, "Passphrase");
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
        ui.add_space(12.0);

        // Secrecy slider.
        let secrecy_label = secrecy_word(self.ms.export.secrecy);
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
                .font(FontId::new(12.0, egui::FontFamily::Proportional))
                .color(p.text_faint),
        );
        ui.add_space(10.0);
        if let Some(err) = &self.ms.export.err {
            ui.label(
                egui::RichText::new(err)
                    .font(FontId::new(12.0, egui::FontFamily::Proportional))
                    .color(p.red),
            );
        }
        ui.add_space(4.0);
    }

    fn delete_profile_dialog(&mut self, ui: &mut egui::Ui) {
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

    fn delete_group_dialog(&mut self, ui: &mut egui::Ui) {
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

    fn edit_profile_dialog(&mut self, ui: &mut egui::Ui) {
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

    fn settings_dialog(&mut self, ui: &mut egui::Ui) {
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

    fn per_app_dialog(&mut self, ui: &mut egui::Ui) {
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
                .font(FontId::new(12.0, egui::FontFamily::Proportional))
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

    fn public_key_dialog(&mut self, ui: &mut egui::Ui) {
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
            .font(FontId::new(12.0, egui::FontFamily::Proportional))
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

    fn premium_dialog(&mut self, ui: &mut egui::Ui) {
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

    fn points_dialog(&mut self, ui: &mut egui::Ui) {
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

    fn assets_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.label(
            egui::RichText::new("User assets")
                .font(theme::bold(19.0))
                .color(p.text),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Geo data files used by custom config routing.")
                .font(FontId::new(12.0, egui::FontFamily::Proportional))
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

    fn about_dialog(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(theme::spaced("NPV TUNNEL")).font(theme::mono(14.0)).color(p.accent));
            ui.label(egui::RichText::new(format!("v{} for Windows", env!("CARGO_PKG_VERSION"))).font(theme::mono(11.0)).color(p.text_faint));
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Security-hardened .npvt tunnel client.\nMIT licensed, source at github.com/librewolfsigma/npvtunnel")
                    .font(FontId::new(12.0, egui::FontFamily::Proportional))
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

fn secrecy_word(t: f32) -> &'static str {
    if t < 0.34 {
        "MINIMAL"
    } else if t < 0.67 {
        "MODERATE"
    } else {
        "MAXIMUM"
    }
}

fn field_label(ui: &mut egui::Ui, p: Palette, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .font(FontId::new(12.0, egui::FontFamily::Proportional))
            .color(p.text_secondary),
    );
}

fn key_row(ui: &mut egui::Ui, p: Palette, title: &str, key: &str, id: &str) -> bool {
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
fn pick_engine_binary() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter("sing-box", &["exe"])
        .pick_file()
}

#[cfg(not(windows))]
fn pick_engine_binary() -> Option<std::path::PathBuf> {
    None
}

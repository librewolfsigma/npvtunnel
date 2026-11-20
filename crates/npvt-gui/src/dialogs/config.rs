//! Config-entry dialogs: add sheet, subscription, manual link, groups,
//! sort order and the locked-container passphrase flow.

use eframe::egui::{self, FontId, Sense, Vec2, Widget as _};
use npvt_core::npvt::v2::Decoded;
use npvt_core::profile;
use npvt_core::vault;

use crate::app::{App, Modal};
use crate::icons;
use crate::theme;
use crate::vault_actions;
use crate::widgets;

use super::{field_label, footer, footer_custom, Action};

impl App {
    pub(crate) fn add_config_dialog(&mut self, ui: &mut egui::Ui) {
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

    pub(crate) fn subscription_dialog(&mut self, ui: &mut egui::Ui) {
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

    pub(crate) fn manual_dialog(&mut self, ui: &mut egui::Ui) {
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

    pub(crate) fn new_group_dialog(&mut self, ui: &mut egui::Ui) {
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

    pub(crate) fn rename_group_dialog(&mut self, ui: &mut egui::Ui) {
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

    pub(crate) fn sort_dialog(&mut self, ui: &mut egui::Ui) {
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

    pub(crate) fn passphrase_dialog(&mut self, ui: &mut egui::Ui) {
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
            ui.label(egui::RichText::new(err).font(theme::body()).color(p.red));
        }
        ui.add_space(12.0);
        match footer_custom(ui, p, "Unlock", p.accent) {
            Action::Confirm => {
                let Some(bytes) = self.ms.pending_locked.clone() else {
                    self.close_modal();
                    return;
                };
                self.confirm_passphrase_unlock(&bytes);
            }
            Action::Cancel => {
                self.ms.pending_locked = None;
                self.close_modal();
            }
            Action::None => {}
        }
    }

    /// Try the entered passphrase; import the profile on success and put
    /// the decode error inline under the field otherwise.
    pub(crate) fn confirm_passphrase_unlock(&mut self, bytes: &[u8]) {
        let pass = self.ms.pass_entry.clone();
        match vault_actions::decode_container_passphrase(bytes, &pass) {
            Ok(decoded) => self.store_unlocked_profile(decoded),
            Err(e) => self.ms.pass_err = Some(format!("Wrong passphrase: {e}")),
        }
    }

    /// Insert the decoded profile under a fresh id, honoring the group hint.
    fn store_unlocked_profile(&mut self, decoded: Decoded) {
        let mut profile = decoded.profile;
        profile.id = profile::new_profile_id();
        if profile.group.is_none() {
            profile.group = decoded.policy.group_hint.clone();
        }
        let name = profile.name.clone();
        let _ = vault::upsert_profile(&mut self.vault, profile);
        self.persist_vault();
        self.push_log("INFO", &format!("imported locked container '{name}'"));
        self.toast_now(format!("Imported {name}"));
        self.ms.pending_locked = None;
        self.ms.pass_entry.clear();
        self.ms.pass_err = None;
        self.close_modal();
    }
}

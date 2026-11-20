//! The CONFIGS group card: header, actions, member rows and the traffic
//! bar. Split out of pages.rs so every file stays well under the
//! file-size budget.

use eframe::egui::{self, FontId, Pos2, Rect, Sense, Stroke, Vec2};

use npvt_core::profile::Profile;

use crate::icons;
use crate::theme::{self, Palette};
use crate::widgets;

impl crate::app::App {
    pub(crate) fn draw_group_card(&mut self, ui: &mut egui::Ui, label: &str, members: &[Profile]) {
        let p = self.palette;
        let expanded = self.expanded.contains(label);
        let contains_active = members
            .iter()
            .any(|m| Some(&m.id) == self.settings.active_id.as_ref());
        let member_count = members.len();
        let extra = if expanded {
            self.group_traffic_extra(label)
        } else {
            0.0
        };
        let height = if expanded {
            56.0 + 44.0 * member_count as f32 + extra
        } else {
            56.0
        };

        let border = Stroke::new(
            if contains_active { 2.0 } else { 1.0 },
            if contains_active {
                p.border_sel
            } else {
                p.border
            },
        );
        let fill = if contains_active {
            p.surface_hi
        } else {
            p.surface
        };
        let (rect, resp) = widgets::card(ui, p, height, Some(fill), Some(border));
        {
            let painter = ui.painter();
            painter.add(egui::Shape::line_segment(
                [
                    Pos2::new(rect.left(), rect.top()),
                    Pos2::new(rect.left(), rect.top()),
                ],
                Stroke::NONE,
            ));
        }

        self.group_card_header(ui, p, rect, label, member_count, expanded);
        self.group_card_actions(ui, rect, label, member_count, contains_active, resp);
        if expanded {
            self.group_member_rows(ui, p, rect, members);
            if let Some((text, frac)) = self.group_traffic(label) {
                let y = rect.top() + 56.0 + 44.0 * member_count as f32;
                let bar_rect = Rect::from_min_size(
                    Pos2::new(rect.left() + 20.0, y + 10.0),
                    Vec2::new(rect.width() - 40.0, 6.0),
                );
                widgets::progress_bar(ui.painter(), bar_rect, frac, p.accent, p.cyan, p.border);
                widgets::label_at(
                    ui.painter(),
                    egui::Align2::LEFT_TOP,
                    Pos2::new(rect.left() + 20.0, y + 22.0),
                    text.as_str(),
                    theme::mono(10.5),
                    p.text_secondary,
                );
            }
        }
    }

    /// Card header: expand chevron, group label and the member subtitle.
    fn group_card_header(
        &mut self,
        ui: &mut egui::Ui,
        p: Palette,
        rect: Rect,
        label: &str,
        member_count: usize,
        expanded: bool,
    ) {
        let chev_rect = Rect::from_center_size(
            Pos2::new(rect.left() + 18.0, rect.top() + 28.0),
            Vec2::splat(16.0),
        );
        {
            let painter = ui.painter();
            icons::paint_with(
                painter,
                chev_rect,
                p.text_secondary,
                if expanded {
                    icons::chevron_down
                } else {
                    icons::chevron_right
                },
                icons::ICON_STROKE,
            );
            widgets::label_at(
                painter,
                egui::Align2::LEFT_TOP,
                Pos2::new(rect.left() + 34.0, rect.top() + 15.0),
                label,
                theme::semibold(14.5),
                p.text,
            );
            let sub = if label == "Saved" {
                format!("{member_count}")
            } else {
                format!("Fastest · {member_count} members")
            };
            widgets::label_at(
                painter,
                egui::Align2::LEFT_TOP,
                Pos2::new(rect.left() + 34.0, rect.top() + 35.0),
                sub.as_str(),
                theme::mono(10.5),
                p.text_secondary,
            );
        }
    }

    /// Right-aligned card actions: delete, rename, ping, set active; a
    /// plain card click toggles the expanded state.
    fn group_card_actions(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        label: &str,
        member_count: usize,
        contains_active: bool,
        resp: egui::Response,
    ) {
        let p = self.palette;
        // Actions (right to left).
        let mut action: Option<&str> = None;
        let action_y = rect.top() + 28.0;
        let mk = |dx: f32| {
            Rect::from_center_size(
                Pos2::new(rect.right() - 24.0 - dx, action_y),
                Vec2::splat(20.0),
            )
        };
        if widgets::icon_button(
            ui,
            mk(0.0),
            p.text_secondary,
            icons::trash,
            &format!("grp-{label}-trash"),
            "Delete group",
        ) {
            action = Some("trash");
        }
        if widgets::icon_button(
            ui,
            mk(28.0),
            p.text_secondary,
            icons::pencil,
            &format!("grp-{label}-edit"),
            "Rename group",
        ) {
            action = Some("edit");
        }
        if member_count > 0
            && widgets::icon_button(
                ui,
                mk(56.0),
                p.text_secondary,
                icons::speed,
                &format!("grp-{label}-speed"),
                "Ping group",
            )
        {
            action = Some("speed");
        }
        if member_count > 0
            && widgets::icon_button(
                ui,
                mk(84.0),
                if contains_active {
                    p.accent
                } else {
                    p.text_secondary
                },
                icons::pin,
                &format!("grp-{label}-pin"),
                "Set as active",
            )
        {
            action = Some("pin");
        }

        match action {
            Some("trash") => self.confirm_delete_group(label),
            Some("edit") => self.open_modal(crate::app::Modal::RenameGroup {
                label: label.to_owned(),
            }),
            Some("speed") => self.ping_group(label),
            Some("pin") => self.activate_group(label),
            _ => {
                if resp.clicked() {
                    if self.expanded.contains(label) {
                        self.expanded.remove(label);
                    } else {
                        self.expanded.insert(label.to_owned());
                    }
                }
            }
        }
    }

    /// Expanded content: one row per member config.
    fn group_member_rows(
        &mut self,
        ui: &mut egui::Ui,
        p: Palette,
        rect: Rect,
        members: &[Profile],
    ) {
        let mut y = rect.top() + 56.0;
        for member in members {
            let row = Rect::from_min_size(
                Pos2::new(rect.left() + 10.0, y),
                Vec2::new(rect.width() - 20.0, 40.0),
            );
            self.group_member_row(ui, p, row, member);
            y += 44.0;
        }
    }

    /// One member row: name, endpoint, ping badge, per-config buttons and
    /// the row-wide activate hit-test.
    fn group_member_row(&mut self, ui: &mut egui::Ui, p: Palette, row: Rect, member: &Profile) {
        let row_painter = ui.painter();
        let is_active = Some(&member.id) == self.settings.active_id.as_ref();
        widgets::label_at(
            row_painter,
            egui::Align2::LEFT_TOP,
            Pos2::new(row.left() + 8.0, row.top() + 4.0),
            &member.name,
            FontId::new(13.0, egui::FontFamily::Proportional),
            if is_active { p.accent } else { p.text },
        );
        widgets::label_at(
            row_painter,
            egui::Align2::LEFT_TOP,
            Pos2::new(row.left() + 8.0, row.top() + 21.0),
            &format!(
                "{}:{} · {}",
                member.server,
                member.port,
                member.protocol.as_str()
            ),
            theme::mono(9.5),
            p.text_faint,
        );
        let ping_text = match self.pings.get(&member.id) {
            Some(Some(ms)) => format!("{ms} ms"),
            Some(None) => "timeout".to_owned(),
            None => String::new(),
        };
        let ping_color = match self.pings.get(&member.id) {
            Some(Some(ms)) if *ms < 200 => p.accent,
            Some(Some(ms)) if *ms < 600 => p.amber,
            Some(Some(_)) => p.red,
            Some(None) => p.red,
            None => p.text_faint,
        };
        widgets::label_at(
            row_painter,
            egui::Align2::RIGHT_CENTER,
            Pos2::new(row.right() - 84.0, row.center().y),
            ping_text.as_str(),
            theme::mono(10.5),
            ping_color,
        );
        let del_rect = Rect::from_center_size(
            Pos2::new(row.right() - 12.0, row.center().y),
            Vec2::splat(16.0),
        );
        if widgets::icon_button(
            ui,
            del_rect,
            p.text_faint,
            icons::trash,
            &format!("del-{}", member.id),
            "Remove config",
        ) {
            self.confirm_delete_profile(&member.id);
        }
        let edit_rect = Rect::from_center_size(
            Pos2::new(row.right() - 34.0, row.center().y),
            Vec2::splat(16.0),
        );
        if widgets::icon_button(
            ui,
            edit_rect,
            p.text_faint,
            icons::pencil,
            &format!("edit-{}", member.id),
            "Edit config",
        ) {
            self.open_modal(crate::app::Modal::EditProfile {
                id: member.id.clone(),
            });
        }
        let share_rect = Rect::from_center_size(
            Pos2::new(row.right() - 56.0, row.center().y),
            Vec2::splat(16.0),
        );
        if widgets::icon_button(
            ui,
            share_rect,
            p.text_faint,
            icons::upload,
            &format!("share-{}", member.id),
            "Share config",
        ) {
            self.open_modal(crate::app::Modal::Export {
                profile_id: member.id.clone(),
            });
        }
        let row_resp = ui
            .interact(
                row,
                egui::Id::new(format!("row-{}", member.id)),
                Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if row_resp.clicked() {
            self.set_active_profile(&member.id);
        }
    }
}

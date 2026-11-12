//! Page renderers: HOME, CONFIGS, LOGS, MORE. Implemented as methods on the
//! `App` struct (see app.rs for state and actions).

use std::collections::BTreeSet;

use eframe::egui::{self, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2, Widget as _};
use npvt_core::profile::Profile;

use crate::engine_bridge;
use crate::icons;
use crate::net;
use crate::pages::sorting::sorted_groups;
use crate::theme;
use crate::vault_actions;
use crate::widgets;

pub mod sorting;

/// Shared page header: premium badge slot + "N P V  T U N N E L" + version.
pub fn header(ui: &mut egui::Ui, app: &mut crate::app::App, with_badge: bool) {
    let p = app.palette;
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 64.0), Sense::hover());
    let painter = ui.painter();
    if with_badge {
        let badge_rect = Rect::from_center_size(
            Pos2::new(rect.right() - 24.0, rect.center().y),
            Vec2::splat(36.0),
        );
        let resp = ui
            .interact(badge_rect, egui::Id::new("premium-badge"), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Premium");
        painter.add(egui::Shape::Rect(egui::epaint::RectShape::new(
            badge_rect,
            egui::Rounding::same(11.0),
            Color32::from_rgba_unmultiplied(0xFB, 0xBF, 0x24, 26),
            Stroke::NONE,
        )));
        icons::paint_with(
            painter,
            badge_rect.shrink(7.0),
            Color32::from_rgb(0xFB, 0xBF, 0x24),
            icons::medal_gold,
            2.2,
        );
        if resp.clicked() {
            app.open_modal(crate::app::Modal::Premium);
        }
    }
    widgets::label_at(
        painter,
        egui::Align2::CENTER_CENTER,
        Pos2::new(rect.center().x, rect.center().y - 9.0),
        &theme::spaced("NPV TUNNEL"),
        theme::mono(15.0),
        p.accent,
    );
    widgets::label_at(
        painter,
        egui::Align2::CENTER_CENTER,
        Pos2::new(rect.center().x, rect.center().y + 12.0),
        &format!("v{} (windows)", env!("CARGO_PKG_VERSION")),
        theme::mono(9.5),
        p.text_faint,
    );
}

impl crate::app::App {
    pub fn draw_home(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        header(ui, self, true);
        ui.add_space(26.0);

        // Power button (centered).
        let state = self.bridge.shared().state();
        let time = self.time;
        let toggle = ui
            .with_layout(
                egui::Layout::top_down_justified(egui::Align::Center),
                |ui| widgets::power_button(ui, p, state, time),
            )
            .inner;
        if toggle {
            self.toggle_connection();
        }
        ui.add_space(16.0);
        widgets::status_text(ui, p, state, time);

        // Live rate line while connecting/connected.
        if state == engine_bridge::STATE_CONNECTED || state == engine_bridge::STATE_CONNECTING {
            ui.add_space(4.0);
            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                let shared = self.bridge.shared();
                let text = format!(
                    "{}  |  {}",
                    engine_bridge::fmt_rate(
                        shared.down_bps.load(std::sync::atomic::Ordering::Relaxed)
                    ),
                    engine_bridge::fmt_rate(
                        shared.up_bps.load(std::sync::atomic::Ordering::Relaxed)
                    ),
                );
                ui.label(
                    egui::RichText::new(text)
                        .font(theme::mono(11.0))
                        .color(p.text_faint),
                );
            });
        }
        ui.add_space(28.0);

        // Engine error card.
        if state == engine_bridge::STATE_ERROR {
            if let Some(err) = self.bridge.shared().error() {
                let (rect, resp) =
                    widgets::card(ui, p, 92.0, Some(p.surface), Some(Stroke::new(1.0, p.red)));
                let painter = ui.painter();
                widgets::label_at(
                    painter,
                    egui::Align2::LEFT_TOP,
                    Pos2::new(rect.left() + 16.0, rect.top() + 12.0),
                    "CONNECTION ERROR",
                    theme::mono(10.5),
                    p.red,
                );
                let galley = ui.ctx().fonts(|f| {
                    f.layout(
                        err.clone(),
                        FontId::new(12.0, egui::FontFamily::Proportional),
                        p.text_secondary,
                        rect.width() - 32.0,
                    )
                });
                painter.galley(
                    Pos2::new(rect.left() + 16.0, rect.top() + 30.0),
                    galley,
                    p.text_secondary,
                );
                if resp.clicked() {
                    self.page = 2;
                }
                ui.add_space(12.0);
            }
        }

        // Active connection card.
        let active = self.active_profile();
        let group_label = active
            .as_ref()
            .map(|pr| vault_actions::group_label(pr))
            .unwrap_or_else(|| "No group".to_owned());
        let endpoint = active
            .as_ref()
            .map(|pr| format!("{}:{}", pr.server, pr.port))
            .unwrap_or_else(|| "Add a config in CONFIGS to get started".to_owned());
        let member_count = active
            .as_ref()
            .map(|pr| self.members_of_group(&vault_actions::group_label(pr)).len())
            .unwrap_or(0);
        let has_active = active.is_some();

        let (rect, resp) = widgets::card(
            ui,
            p,
            158.0,
            Some(p.surface),
            Some(Stroke::new(1.0, p.border)),
        );
        let painter = ui.painter();
        widgets::label_at(
            painter,
            egui::Align2::LEFT_TOP,
            Pos2::new(rect.left() + 18.0, rect.top() + 14.0),
            "ACTIVE CONNECTION GROUP",
            theme::mono(10.0),
            p.text_faint,
        );
        let icon_rect = Rect::from_center_size(
            Pos2::new(rect.left() + 30.0, rect.top() + 56.0),
            Vec2::splat(22.0),
        );
        icons::paint_with(painter, icon_rect, p.accent, icons::new_group_icon, 0.0);
        widgets::label_at(
            painter,
            egui::Align2::LEFT_CENTER,
            Pos2::new(rect.left() + 52.0, rect.top() + 50.0),
            if has_active {
                group_label.as_str()
            } else {
                "No active config"
            },
            theme::semibold(14.5),
            p.text,
        );
        widgets::label_at(
            painter,
            egui::Align2::LEFT_CENTER,
            Pos2::new(rect.left() + 52.0, rect.top() + 70.0),
            endpoint.as_str(),
            theme::mono(11.0),
            p.text_secondary,
        );
        widgets::label_at(
            painter,
            egui::Align2::RIGHT_CENTER,
            Pos2::new(rect.right() - 34.0, rect.top() + 60.0),
            if member_count > 1 { "Fastest" } else { "" },
            theme::mono(11.0),
            p.text_secondary,
        );
        let chev_rect = Rect::from_center_size(
            Pos2::new(rect.right() - 20.0, rect.top() + 58.0),
            Vec2::splat(18.0),
        );
        icons::paint_with(
            painter,
            chev_rect,
            p.accent,
            icons::chevron_right,
            icons::ICON_STROKE,
        );
        if resp.clicked() {
            self.page = 1;
        }

        // PING action.
        let ping_rect = Rect::from_center_size(
            Pos2::new(rect.center().x, rect.top() + 102.0),
            Vec2::new(70.0, 22.0),
        );
        let ping_resp = ui
            .interact(ping_rect, egui::Id::new("home-ping"), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        widgets::label_at(
            painter,
            egui::Align2::CENTER_CENTER,
            ping_rect.center(),
            "PING",
            theme::semibold(12.5),
            p.cyan,
        );
        if ping_resp.clicked() {
            self.ping_all();
        }
        painter.add(egui::Shape::line_segment(
            [
                Pos2::new(rect.left() + 14.0, rect.top() + 118.0),
                Pos2::new(rect.right() - 14.0, rect.top() + 118.0),
            ],
            Stroke::new(1.0, p.border),
        ));
        // Stats row.
        let shared = self.bridge.shared();
        let down =
            engine_bridge::fmt_mb(shared.down_total.load(std::sync::atomic::Ordering::Relaxed));
        let up = engine_bridge::fmt_mb(shared.up_total.load(std::sync::atomic::Ordering::Relaxed));
        let mid = rect.center().x;
        widgets::label_at(
            painter,
            egui::Align2::CENTER_CENTER,
            Pos2::new(mid - 70.0, rect.top() + 130.0),
            "DOWNLINK",
            theme::mono(9.5),
            p.text_faint,
        );
        widgets::label_at(
            painter,
            egui::Align2::CENTER_CENTER,
            Pos2::new(mid + 70.0, rect.top() + 130.0),
            "UPLINK",
            theme::mono(9.5),
            p.text_faint,
        );
        let arrow_down = Rect::from_center_size(
            Pos2::new(mid - 112.0, rect.top() + 147.0),
            Vec2::splat(15.0),
        );
        let arrow_up =
            Rect::from_center_size(Pos2::new(mid + 28.0, rect.top() + 147.0), Vec2::splat(15.0));
        icons::paint_with(painter, arrow_down, p.accent, icons::double_arrow_down, 2.0);
        icons::paint_with(painter, arrow_up, p.amber, icons::double_arrow_up, 2.0);
        widgets::label_at(
            painter,
            egui::Align2::LEFT_CENTER,
            Pos2::new(mid - 100.0, rect.top() + 147.0),
            down.as_str(),
            theme::mono(12.5),
            p.accent,
        );
        widgets::label_at(
            painter,
            egui::Align2::LEFT_CENTER,
            Pos2::new(mid + 40.0, rect.top() + 147.0),
            up.as_str(),
            theme::mono(12.5),
            p.amber,
        );
    }

    pub fn draw_configs(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        header(ui, self, false);

        // LAST PING row.
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::hover());
        let painter = ui.painter();
        widgets::label_at(
            painter,
            egui::Align2::LEFT_TOP,
            Pos2::new(rect.left() + 2.0, rect.top() + 4.0),
            "LAST PING",
            theme::mono(10.0),
            p.text_faint,
        );
        let last = self.last_ping_label();
        widgets::label_at(
            painter,
            egui::Align2::LEFT_TOP,
            Pos2::new(rect.left() + 2.0, rect.top() + 19.0),
            last.as_str(),
            FontId::new(13.0, egui::FontFamily::Proportional),
            p.text,
        );
        let ping_rect = Rect::from_center_size(
            Pos2::new(rect.right() - 26.0, rect.center().y),
            Vec2::new(52.0, 24.0),
        );
        let ping_resp = ui
            .interact(ping_rect, egui::Id::new("configs-ping"), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        widgets::label_at(
            painter,
            egui::Align2::CENTER_CENTER,
            ping_rect.center(),
            "PING",
            theme::semibold(13.5),
            p.cyan,
        );
        if ping_resp.clicked() {
            self.ping_all();
        }
        ui.add_space(6.0);

        // Search field.
        if self.search_open {
            ui.horizontal(|ui| {
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Search configs")
                    .desired_width(ui.available_width() - 34.0)
                    .ui(ui);
                let icon_rect = Rect::from_center_size(
                    ui.min_rect().right_center() + Vec2::new(12.0, 0.0),
                    Vec2::splat(18.0),
                );
                if widgets::icon_button(
                    ui,
                    icon_rect,
                    p.text_faint,
                    icons::x,
                    "search-close",
                    "Close search",
                ) {
                    self.search_open = false;
                    self.search.clear();
                }
            });
            ui.add_space(6.0);
        }

        // Group cards.
        let groups = sorted_groups(self);
        let filter = self.search.to_lowercase();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (label, members) in groups.iter() {
                    if !filter.is_empty()
                        && !label.to_lowercase().contains(&filter)
                        && !members.iter().any(|m| {
                            m.name.to_lowercase().contains(&filter)
                                || m.server.to_lowercase().contains(&filter)
                        })
                    {
                        continue;
                    }
                    self.draw_group_card(ui, label, members);
                    ui.add_space(10.0);
                }
                if groups.is_empty() {
                    ui.add_space(40.0);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("No configs yet")
                                .font(theme::semibold(15.0))
                                .color(p.text_secondary),
                        );
                        ui.label(
                            egui::RichText::new("Tap the green + to add your first config")
                                .font(FontId::new(12.5, egui::FontFamily::Proportional))
                                .color(p.text_faint),
                        );
                    });
                }
                // Leave room for the floating toolbar.
                ui.add_space(90.0);
            });

        self.draw_floating_toolbar(ui);
    }

    fn draw_group_card(&mut self, ui: &mut egui::Ui, label: &str, members: &[Profile]) {
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

        // Expanded content: member rows + traffic bar.
        if expanded {
            let mut y = rect.top() + 56.0;
            for member in members {
                let row = Rect::from_min_size(
                    Pos2::new(rect.left() + 10.0, y),
                    Vec2::new(rect.width() - 20.0, 40.0),
                );
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
                y += 44.0;
            }
            if let Some((text, frac)) = self.group_traffic(label) {
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

    fn draw_floating_toolbar(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let screen = ui.max_rect();
        let pill_rect = Rect::from_min_size(
            Pos2::new(screen.right() - 268.0, screen.bottom() - 104.0),
            Vec2::new(258.0, 54.0),
        );
        {
            let painter = ui.painter();
            painter.add(egui::Shape::Rect(egui::epaint::RectShape::new(
                pill_rect,
                egui::Rounding::same(27.0),
                Color32::from_rgb(0x10, 0x28, 0x18),
                Stroke::new(1.0, p.accent.gamma_multiply(0.35)),
            )));
        }
        let y = pill_rect.center().y;
        let mut action = None;
        let mk = |dx: f32| {
            Rect::from_center_size(
                Pos2::new(pill_rect.left() + 28.0 + dx, y),
                Vec2::splat(20.0),
            )
        };
        if widgets::icon_button(
            ui,
            mk(0.0),
            p.text_secondary,
            icons::chevron_right,
            "tb-expand",
            "Expand all",
        ) {
            action = Some("expand");
        }
        if widgets::icon_button(
            ui,
            mk(30.0),
            p.text_secondary,
            icons::x,
            "tb-collapse",
            "Collapse all",
        ) {
            action = Some("collapse");
        }
        if widgets::icon_button(
            ui,
            mk(60.0),
            p.text_secondary,
            icons::refresh,
            "tb-refresh",
            "Ping all",
        ) {
            action = Some("refresh");
        }
        if widgets::icon_button(
            ui,
            mk(90.0),
            if self.search_open {
                p.accent
            } else {
                p.text_secondary
            },
            icons::search,
            "tb-search",
            "Search",
        ) {
            action = Some("search");
        }
        if widgets::icon_button(
            ui,
            mk(120.0),
            p.text_secondary,
            icons::sliders,
            "tb-sort",
            "Sort",
        ) {
            action = Some("sort");
        }
        let plus_rect =
            Rect::from_center_size(Pos2::new(pill_rect.right() - 33.0, y), Vec2::splat(46.0));
        ui.painter()
            .circle_filled(plus_rect.center(), 23.0, p.accent);
        if widgets::icon_button(
            ui,
            plus_rect,
            Color32::from_rgb(0x08, 0x1A, 0x0D),
            icons::plus,
            "tb-add",
            "Add config",
        ) {
            action = Some("add");
        }
        match action {
            Some("expand") => {
                let labels: BTreeSet<String> = self.all_group_labels();
                self.expanded = labels;
            }
            Some("collapse") => self.expanded.clear(),
            Some("refresh") => self.ping_all(),
            Some("search") => self.search_open = !self.search_open,
            Some("sort") => self.open_modal(crate::app::Modal::Sort),
            Some("add") => self.open_modal(crate::app::Modal::AddConfig),
            _ => {}
        }
    }

    pub fn draw_logs(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        header(ui, self, false);
        if self.time - self.last_tail_refresh > 1.2 {
            self.last_tail_refresh = self.time;
            self.refresh_engine_tail();
        }

        ui.horizontal(|ui| {
            egui::TextEdit::singleline(&mut self.log_filter)
                .hint_text("Filter")
                .desired_width(ui.available_width() - 60.0)
                .font(egui::TextStyle::Monospace)
                .ui(ui);
            let icon_rect = Rect::from_center_size(
                ui.min_rect().right_center() + Vec2::new(14.0, 0.0),
                Vec2::splat(20.0),
            );
            if widgets::icon_button(
                ui,
                icon_rect,
                p.text_secondary,
                icons::x,
                "logs-clear",
                "Clear events",
            ) {
                self.event_log.clear();
            }
        });
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("Copy")
                        .font(theme::semibold(12.0))
                        .color(p.text_secondary),
                ))
                .clicked()
            {
                let text: String = self.visible_log_lines().join("\n");
                self.copy_to_clipboard(text);
            }
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("Open log folder")
                        .font(theme::semibold(12.0))
                        .color(p.text_secondary),
                ))
                .clicked()
            {
                self.open_log_folder();
            }
        });
        ui.add_space(8.0);

        let lines = self.visible_log_lines();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in lines {
                    let color = if line.contains(" ERROR") || line.contains(" error ") {
                        p.red
                    } else if line.contains(" WARN") || line.contains(" warn ") {
                        p.amber
                    } else {
                        p.text_secondary
                    };
                    ui.label(
                        egui::RichText::new(line)
                            .font(theme::mono(10.5))
                            .color(color),
                    );
                }
                if self.event_log.is_empty() && self.engine_tail.is_empty() {
                    ui.label(
                        egui::RichText::new("No log lines yet")
                            .font(theme::mono(11.0))
                            .color(p.text_faint),
                    );
                }
            });
    }

    pub fn draw_more(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        header(ui, self, false);
        ui.add_space(4.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.more_row(ui, icons::medal_gold, "Premium", "Subscribe to remove ads.") {
                    self.open_modal(crate::app::Modal::Premium);
                }
                widgets::divider(ui, p);

                widgets::section_header(ui, p, "APPEARANCE");
                let selected = self.settings.theme as usize;
                if let Some(i) = widgets::segmented(
                    ui,
                    p,
                    &["Light", "Dark", "Auto"],
                    selected,
                    &[icons::sun, icons::moon, icons::auto_theme],
                ) {
                    self.settings.theme = theme::ThemeMode::ALL[i];
                    self.settings.save();
                }
                ui.label(
                    egui::RichText::new("Auto follows your device's light or dark setting.")
                        .font(FontId::new(12.0, egui::FontFamily::Proportional))
                        .color(p.text_faint),
                );
                ui.add_space(10.0);

                widgets::section_header(ui, p, "SHARING");
                if self.more_row(
                    ui,
                    icons::qr,
                    "My public key",
                    "View or copy your public key.",
                ) {
                    self.open_modal(crate::app::Modal::PublicKey);
                }
                widgets::divider(ui, p);

                widgets::section_header(ui, p, "APP");
                if self.more_row(
                    ui,
                    icons::grid,
                    "Per-app Proxy",
                    "Choose to allow/disallow certain apps from being tunneled through the VPN.",
                ) {
                    self.open_modal(crate::app::Modal::PerApp);
                }
                widgets::divider(ui, p);
                if self.more_row(
                    ui,
                    icons::star,
                    "Points",
                    "Earn points by watching videos, spend them on rewards.",
                ) {
                    self.open_modal(crate::app::Modal::Points);
                }
                widgets::divider(ui, p);
                if self.more_row(ui, icons::gear, "Settings", "Customize the app's settings") {
                    self.open_modal(crate::app::Modal::Settings);
                }
                widgets::divider(ui, p);
                if self.more_row(
                    ui,
                    icons::folder,
                    "User assets",
                    "Download geo data files (geoip, geosite) used by custom config routing.",
                ) {
                    self.open_modal(crate::app::Modal::Assets);
                }
                widgets::divider(ui, p);
                if self.more_row(
                    ui,
                    icons::info,
                    "About",
                    &format!("NPVTunnel v{} for Windows · MIT", env!("CARGO_PKG_VERSION")),
                ) {
                    self.open_modal(crate::app::Modal::About);
                }
                widgets::divider(ui, p);
                if self.more_row(ui, icons::x, "Quit", "Close NPVTunnel") {
                    self.quit_requested = true;
                }
                ui.add_space(20.0);
            });
    }

    /// One selectable row in the MORE page. Returns true when clicked.
    fn more_row(
        &mut self,
        ui: &mut egui::Ui,
        draw: fn(&icons::IconCtx),
        title: &str,
        desc: &str,
    ) -> bool {
        let p = self.palette;
        let width = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 62.0), Sense::click());
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        let painter = ui.painter();
        if resp.hovered() {
            painter.rect_filled(rect, 6.0, p.surface);
        }
        let icon_rect = Rect::from_center_size(
            Pos2::new(rect.left() + 18.0, rect.center().y),
            Vec2::splat(24.0),
        );
        let color = if title == "Premium" {
            Color32::from_rgb(0xFB, 0xBF, 0x24)
        } else {
            p.accent
        };
        icons::paint_with(painter, icon_rect, color, draw, icons::ICON_STROKE);
        widgets::label_at(
            painter,
            egui::Align2::LEFT_TOP,
            Pos2::new(rect.left() + 48.0, rect.top() + 10.0),
            title,
            theme::semibold(14.5),
            p.text,
        );
        let galley = ui.ctx().fonts(|f| {
            f.layout(
                desc.to_owned(),
                FontId::new(12.0, egui::FontFamily::Proportional),
                p.text_faint,
                rect.width() - 60.0,
            )
        });
        painter.galley(
            Pos2::new(rect.left() + 48.0, rect.top() + 30.0),
            galley,
            p.text_faint,
        );
        resp.clicked()
    }
}

impl crate::app::App {
    /// Traffic display for a group card: subscription quota when known,
    /// otherwise this month's local usage against the configured quota.
    pub fn group_traffic(&self, label: &str) -> Option<(String, f32)> {
        if let Some(info) = self.settings.subscriptions.get(label) {
            if info.total > 0 {
                let used = info.upload + info.download;
                let text = format!("{} / {}", net::gb_str(used), net::gb_str(info.total));
                return Some((text, used as f32 / info.total as f32));
            }
        }
        if self.settings.quota_gb > 0 {
            let used = self.settings.monthly_down
                + self
                    .bridge
                    .shared()
                    .down_total
                    .load(std::sync::atomic::Ordering::Relaxed);
            let quota = self.settings.quota_bytes();
            let text = format!("{} / {} GB", net::gb_str(used), self.settings.quota_gb);
            return Some((text, used as f32 / quota as f32));
        }
        None
    }

    pub fn group_traffic_extra(&self, label: &str) -> f32 {
        if self.group_traffic(label).is_some() {
            34.0
        } else {
            0.0
        }
    }
}

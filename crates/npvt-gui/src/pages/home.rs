//! HOME: the power button, connection status, the active-connection card
//! and the live traffic readout - a faithful port of reference image 1 to a
//! desktop window.

use egui::{Pos2, Rect, Sense, Vec2};

use crate::app::App;
use crate::engine_ctrl::Status;
use crate::icons::{self, Icon};
use crate::theme::{self};
use crate::widgets::{self, header, PowerVisual};

/// Column width of the phone-like content column on desktop.
pub const CONTENT_W: f32 = 430.0;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette;
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.with_layout(
            egui::Layout::top_down_justified(egui::Align::Center),
            |ui| {
                ui.set_max_width(CONTENT_W);
                ui.add_space(4.0);
                let (back, badge) = header(ui, &p, false, true);
                if badge {
                    app.dialog = crate::app::Dialog::Premium;
                }
                let _ = back;

                ui.add_space(10.0);

                // --- Power button + status ------------------------------
                let status = app.shared.current_status();
                let visual = match &status {
                    Status::Connected { .. } => PowerVisual::On,
                    Status::Connecting { .. } => PowerVisual::Connecting,
                    Status::Disconnected => PowerVisual::Off,
                };
                let resp = widgets::power_button(ui, &p, &visual);
                if resp.clicked() {
                    app.toggle_power(ui.ctx());
                }
                ui.add_space(14.0);

                let (label, color) = match &status {
                    Status::Connected { .. } => ("[ CONNECTED ]", p.green),
                    Status::Connecting { label: _ } => ("[ CONNECTING ]", p.green),
                    Status::Disconnected => ("[ DISCONNECTED ]", p.text_faint),
                };
                ui.vertical_centered(|ui| {
                    ui.label(theme::mono_label_tracked(label, 14.0, color));
                });
                if let Status::Connecting { label } = &status {
                    ui.vertical_centered(|ui| {
                        ui.label(theme::mono(label.clone(), 10.5, p.text_faint));
                    });
                }
                if let Some(err) = app.shared.last_error.lock().ok().and_then(|g| g.clone()) {
                    ui.vertical_centered(|ui| {
                        ui.label(theme::mono(err, 10.0, p.red));
                    });
                }

                ui.add_space(18.0);

                // --- Active connection group card ------------------------
                let group = app.active_group().cloned();
                let active_label = group
                    .as_ref()
                    .and_then(|g| app.resolve_profile_id(g))
                    .and_then(|id| app.data.profiles.iter().find(|prof| prof.id == id).cloned());
                let card_h = 176.0;
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), card_h), Sense::click());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 12.0, p.card);
                painter.rect_stroke(rect, 12.0, egui::Stroke::new(1.0_f32, p.card_stroke));

                // Tracked label at the top of the card.
                let label_galley = painter.layout_no_wrap(
                    theme::tracked("ACTIVE CONNECTION GROUP"),
                    egui::FontId::new(10.0, egui::FontFamily::Name("mono".into())),
                    p.text_faint,
                );
                painter.galley(
                    Pos2::new(
                        rect.center().x - label_galley.size().x / 2.0,
                        rect.top() + 12.0,
                    ),
                    label_galley,
                    p.text_faint,
                );

                // Cluster icon + group name + chevron.
                let name = group
                    .as_ref()
                    .map(|g| g.name.clone())
                    .unwrap_or_else(|| "No group".into());
                let name_galley = painter.layout_no_wrap(
                    name.clone(),
                    egui::FontId::new(16.0, egui::FontFamily::Name("sans-bold".into())),
                    p.text,
                );
                let row_y = rect.top() + 52.0;
                let cluster_center = Pos2::new(rect.left() + 34.0, row_y + 12.0);
                painter.circle_filled(cluster_center, 9.0, p.green);
                painter.circle_filled(
                    Pos2::new(cluster_center.x + 13.0, cluster_center.y - 8.0),
                    6.0,
                    p.green_dim,
                );
                painter.circle_filled(
                    Pos2::new(cluster_center.x + 15.0, cluster_center.y + 7.0),
                    6.0,
                    p.green_dim,
                );
                let text_x = rect.left() + 64.0;
                painter.galley(Pos2::new(text_x, row_y), name_galley.clone(), p.text);
                let chev_rect = Rect::from_center_size(
                    Pos2::new(rect.right() - 26.0, row_y + 11.0),
                    Vec2::splat(22.0),
                );
                icons::paint(&painter, Icon::ChevronRight, chev_rect, p.text, 2.0);

                // Subtitle: profile/server + mode.
                let sub = active_label
                    .as_ref()
                    .map(|prof| prof.name.clone())
                    .unwrap_or_else(|| "No profile".into());
                let mode = group
                    .as_ref()
                    .map(|g| g.mode.as_label())
                    .unwrap_or("Fastest");
                painter.galley(
                    Pos2::new(text_x, row_y + 24.0),
                    painter.layout_no_wrap(
                        sub.clone(),
                        egui::FontId::new(12.5, egui::FontFamily::Proportional),
                        p.text_dim,
                    ),
                    p.text_dim,
                );
                let mode_galley = painter.layout_no_wrap(
                    mode.to_owned(),
                    egui::FontId::new(12.0, egui::FontFamily::Name("mono".into())),
                    p.text_dim,
                );
                painter.galley(
                    Pos2::new(rect.right() - 26.0 - mode_galley.size().x, row_y + 24.0),
                    mode_galley,
                    p.text_dim,
                );

                // PING button, centered.
                let ping_rect = Rect::from_center_size(
                    Pos2::new(rect.center().x, rect.top() + 108.0),
                    Vec2::new(120.0, 26.0),
                );
                let ping_galley = painter.layout_no_wrap(
                    theme::tracked("PING"),
                    egui::FontId::new(12.5, egui::FontFamily::Name("mono-med".into())),
                    p.cyan,
                );
                painter.galley(
                    Pos2::new(
                        ping_rect.center().x - ping_galley.size().x / 2.0,
                        ping_rect.center().y - ping_galley.size().y / 2.0,
                    ),
                    ping_galley,
                    p.cyan,
                );

                // Divider.
                let div_y = rect.top() + 128.0;
                painter.line_segment(
                    [
                        Pos2::new(rect.left() + 14.0, div_y),
                        Pos2::new(rect.right() - 14.0, div_y),
                    ],
                    egui::Stroke::new(1.0_f32, p.card_stroke),
                );

                // DOWNLINK / UPLINK columns.
                let (down, up) = app.shared.rates();
                let connected = matches!(status, Status::Connected { .. });
                let cols = [
                    ("DOWNLINK", Icon::ArrowDown, down, p.green),
                    ("UPLINK", Icon::ArrowUp, up, p.orange),
                ];
                for (i, (label_text, arrow, rate, color)) in cols.iter().enumerate() {
                    let cx = rect.left() + rect.width() * (0.3 + 0.4 * i as f32);
                    let lab = painter.layout_no_wrap(
                        theme::tracked(label_text),
                        egui::FontId::new(9.5, egui::FontFamily::Name("mono".into())),
                        p.text_faint,
                    );
                    painter.galley(
                        Pos2::new(cx - lab.size().x / 2.0, div_y + 10.0),
                        lab,
                        p.text_faint,
                    );
                    let val_galley = painter.layout_no_wrap(
                        crate::data::format_rate(if connected && *rate > 0.0 {
                            *rate
                        } else {
                            0.0
                        }),
                        egui::FontId::new(13.5, egui::FontFamily::Name("mono-med".into())),
                        if connected && *rate > 0.0 {
                            *color
                        } else {
                            p.text_faint
                        },
                    );
                    let arrow_rect = Rect::from_center_size(
                        Pos2::new(cx - val_galley.size().x / 2.0 - 12.0, div_y + 38.0),
                        Vec2::splat(14.0),
                    );
                    icons::paint(&painter, *arrow, arrow_rect, *color, 2.0);
                    painter.galley(
                        Pos2::new(cx - (val_galley.size().x + 22.0) / 2.0 + 22.0, div_y + 31.0),
                        val_galley,
                        p.text,
                    );
                }

                // Card interactions.
                if ui
                    .allocate_rect(rect, Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    app.page = crate::widgets::Page::Configs;
                }
                if ui.allocate_rect(ping_rect, Sense::click()).clicked() {
                    if let Some(g) = &group {
                        let gid = g.id.clone();
                        app.ping_group(&gid, ui.ctx());
                        app.toasts.show("Pinging group…");
                    }
                }

                ui.add_space(12.0);
                // Session info line (real data instead of the mock numbers).
                if let Status::Connected { label, since, .. } = &status {
                    let secs = if *since > 0 {
                        crate::engine_ctrl::unix_now().saturating_sub(*since)
                    } else {
                        0
                    };
                    ui.vertical_centered(|ui| {
                        ui.label(theme::mono(
                            format!("{label} · up {}m", secs / 60),
                            10.0,
                            p.text_faint,
                        ));
                    });
                }
            },
        );
    });
}

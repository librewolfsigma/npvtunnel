//! CONFIGS: the group library - LAST PING row, expandable group rows with
//! inline actions (speed/pencil/trash/pin/kebab), the Global Network usage
//! bar, the floating pill toolbar and the Add-config menu. Ports reference
//! images 2 and 4.

use egui::{Pos2, Rect, Sense, Vec2};

use crate::app::{App, Dialog, KebabKind};
use crate::data::{format_bytes, GroupMode};
use crate::engine_ctrl::PingOutcome;
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
                let (back, _) = widgets::header(ui, &p, false, false);
                let _ = back;

                // Search row (toggled from the pill toolbar).
                if app.search_open {
                    ui.horizontal(|ui| {
                        let field = egui::TextEdit::singleline(&mut app.search)
                            .hint_text(theme::mono("Search configs", 12.0, p.text_faint).text())
                            .desired_width(ui.available_width())
                            .font(egui::TextStyle::Monospace);
                        ui.add(field);
                        if icons::icon_button(ui, Icon::Close, 26.0, p.text_dim, None).clicked() {
                            app.search_open = false;
                            app.search.clear();
                        }
                    });
                }

                // LAST PING row.
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        widgets::tracked_label(ui, &p, "LAST PING");
                        let text = match app.groups.last_ping {
                            Some(t) => {
                                let dt = crate::engine_ctrl::unix_now().saturating_sub(t);
                                if dt < 60 {
                                    "just now".into()
                                } else if dt < 3600 {
                                    format!("{}m ago", dt / 60)
                                } else if dt < 86400 {
                                    format!("{}h ago", dt / 3600)
                                } else {
                                    format!("{}d ago", dt / 86400)
                                }
                            }
                            None => "Never".to_string(),
                        };
                        ui.label(theme::sans(text, 13.0, p.text_dim));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::text_button(ui, &p, "PING").clicked() {
                            app.ping_group(&active_group_id(app), ui.ctx());
                        }
                    });
                });
                ui.add_space(6.0);

                // Group rows.
                let group_ids: Vec<String> = app
                    .groups
                    .display_order()
                    .iter()
                    .map(|g| g.id.clone())
                    .collect();
                for gid in group_ids {
                    let Some(group) = app.groups.group(&gid).cloned() else {
                        continue;
                    };
                    let selected = app.groups.active_group.as_deref() == Some(gid.as_str());
                    let expanded = app.expanded.contains(&gid);
                    let member_count = group.members.len();
                    let members: Vec<crate::data::Group> = Vec::new();
                    let _ = members;
                    group_row(app, ui, &p, &group, selected, expanded, member_count);
                    if expanded {
                        member_rows(app, ui, &p, &group);
                    }
                }

                // Virtual "Saved" group.
                let saved = app.groups.saved_ids(&app.data);
                let saved_count = saved.len();
                let saved_expanded = app.expanded.contains("virtual:saved");
                virtual_row(
                    app,
                    ui,
                    &p,
                    "virtual:saved",
                    "Saved",
                    saved_count,
                    saved_expanded,
                );
                if saved_expanded {
                    let saved_profiles: Vec<npvt_core::profile::Profile> = saved
                        .iter()
                        .filter_map(|id| app.data.profiles.iter().find(|prof| &prof.id == id))
                        .cloned()
                        .collect();
                    profile_rows(app, ui, &p, &saved_profiles);
                }

                // Virtual "Global Network" group with the usage bar.
                let all = app.groups.all_ids(&app.data);
                let global_expanded = app.expanded.contains("virtual:global");
                virtual_row(
                    app,
                    ui,
                    &p,
                    "virtual:global",
                    "Global Network",
                    all.len(),
                    global_expanded,
                );
                if global_expanded {
                    let (down, up) = app.groups.total_usage();
                    widgets::usage_bar(ui, &p, down as f64, (down + up).max(1) as f64);
                    ui.label(theme::mono(
                        format!("{} / {}", format_bytes(down), format_bytes(down + up)),
                        11.5,
                        p.text_dim,
                    ));
                }

                ui.add_space(86.0); // room for the floating toolbar
            },
        );
    });

    // Floating pill toolbar + FAB (painted after the scroll area so it stays
    // on top and does not scroll).
    pill_toolbar(app, ui, &p);
}

fn active_group_id(app: &App) -> String {
    app.groups
        .active_group
        .clone()
        .or_else(|| app.groups.groups.first().map(|g| g.id.clone()))
        .unwrap_or_else(|| "virtual:global".into())
}

// ---- Row rendering -----------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn row_frame(ui: &mut egui::Ui, p: &Palette, height: f32, selected: bool) -> (Rect, egui::Painter) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 10.0, p.green_deep);
        painter.rect_stroke(rect, 10.0, egui::Stroke::new(1.4_f32, p.green_border));
    } else {
        painter.rect_filled(rect, 10.0, p.card);
        painter.rect_stroke(rect, 10.0, egui::Stroke::new(1.0_f32, p.card_stroke));
    }
    (rect, painter)
}

fn group_row(
    app: &mut App,
    ui: &mut egui::Ui,
    p: &Palette,
    group: &crate::data::Group,
    selected: bool,
    expanded: bool,
    member_count: usize,
) {
    let filtered = !app.search.is_empty()
        && !group
            .name
            .to_lowercase()
            .contains(&app.search.to_lowercase());
    if filtered {
        return;
    }
    let height = 58.0;
    let (rect, painter) = row_frame(ui, p, height, selected);

    // Expand chevron.
    let chev_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 20.0, rect.center().y),
        Vec2::splat(18.0),
    );
    icons::paint(
        &painter,
        if expanded {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        },
        chev_rect,
        p.text_dim,
        1.8,
    );

    // Name + subtitle.
    let name_x = rect.left() + 38.0;
    painter.galley(
        Pos2::new(name_x, rect.top() + 9.0),
        painter.layout_no_wrap(
            group.name.clone(),
            egui::FontId::new(15.0, egui::FontFamily::Name("sans-med".into())),
            p.text,
        ),
        p.text,
    );
    let sub = format!("{} · {} members", group.mode.as_label(), member_count);
    painter.galley(
        Pos2::new(name_x, rect.top() + 31.0),
        painter.layout_no_wrap(
            sub,
            egui::FontId::new(11.5, egui::FontFamily::Name("mono".into())),
            p.text_dim,
        ),
        p.text_dim,
    );

    // Right action cluster. Selected rows: speed/pencil/trash (+pin when
    // pinned). Collapsed rows: speed/kebab, matching the reference.
    let mut x = rect.right() - 12.0;
    let action_y = rect.center().y;
    let step = 30.0;
    let mut actions: Vec<Icon> = Vec::new();
    if selected || expanded {
        actions.push(Icon::Speed);
        actions.push(Icon::Pencil);
        actions.push(Icon::Trash);
        if group.pinned {
            actions.insert(0, Icon::Pin);
        }
    } else {
        actions.push(Icon::Speed);
        actions.push(Icon::Kebab);
    }
    for icon in actions.iter().rev() {
        let center = Pos2::new(x - 13.0, action_y);
        x -= step;
        let zone = Rect::from_center_size(center, Vec2::splat(26.0));
        icons::paint(&painter, *icon, zone.shrink(4.0), p.text_dim, 1.7);
        let hit = ui
            .allocate_rect(zone, Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if !hit.clicked() {
            continue;
        }
        match icon {
            Icon::Speed => app.ping_group(&group.id, ui.ctx()),
            Icon::Pencil => {
                app.dialog = Dialog::EditGroup {
                    group_id: group.id.clone(),
                    name: group.name.clone(),
                    mode: match group.mode {
                        GroupMode::Fastest => 0,
                        GroupMode::RoundRobin => 1,
                        GroupMode::Manual => 2,
                    },
                    members: group.members.clone(),
                };
            }
            Icon::Trash => {
                app.dialog = Dialog::ConfirmDeleteGroup {
                    group_id: group.id.clone(),
                };
            }
            Icon::Pin => {
                if let Some(g) = app.groups.group_mut(&group.id) {
                    g.pinned = !g.pinned;
                }
                app.groups.save();
            }
            Icon::Kebab => {
                app.dialog = Dialog::Kebab {
                    group_id: group.id.clone(),
                    kind: KebabKind::User,
                };
            }
            _ => {}
        }
    }

    // Whole-row interactions: click selects the group; chevron toggles.
    let row_resp = ui.allocate_rect(rect, Sense::click());
    if row_resp.clicked() {
        app.groups.active_group = Some(group.id.clone());
        app.groups.save();
    }
    let chev_hit = ui.allocate_rect(chev_rect, Sense::click());
    if chev_hit.clicked() {
        if expanded {
            app.expanded.remove(&group.id);
        } else {
            app.expanded.insert(group.id.clone());
        }
    }
}

fn virtual_row(
    app: &mut App,
    ui: &mut egui::Ui,
    p: &Palette,
    key: &str,
    name: &str,
    count: usize,
    expanded: bool,
) {
    if !app.search.is_empty() && !name.to_lowercase().contains(&app.search.to_lowercase()) {
        return;
    }
    let height = if expanded && key == "virtual:global" {
        84.0
    } else {
        52.0
    };
    let selected = app.groups.active_group == Some(key.to_string());
    let (rect, painter) = row_frame(ui, p, height, selected);
    let chev_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 20.0, rect.top() + 26.0),
        Vec2::splat(18.0),
    );
    icons::paint(
        &painter,
        if expanded {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        },
        chev_rect,
        p.text_dim,
        1.8,
    );
    let title = format!("{name} ({count})");
    painter.galley(
        Pos2::new(rect.left() + 38.0, rect.top() + 15.0),
        painter.layout_no_wrap(
            title,
            egui::FontId::new(15.0, egui::FontFamily::Name("sans-med".into())),
            p.text,
        ),
        p.text,
    );

    // Actions: speed + refresh + kebab for Global Network; speed + kebab for
    // Saved, matching the reference rows.
    let mut x = rect.right() - 12.0;
    let action_y = rect.top() + 26.0;
    let step = 30.0;
    let actions: Vec<Icon> = if key == "virtual:global" {
        vec![Icon::Speed, Icon::Refresh, Icon::Kebab]
    } else {
        vec![Icon::Speed, Icon::Kebab]
    };
    for icon in actions.iter().rev() {
        let center = Pos2::new(x - 13.0, action_y);
        x -= step;
        let zone = Rect::from_center_size(center, Vec2::splat(26.0));
        icons::paint(&painter, *icon, zone.shrink(4.0), p.text_dim, 1.7);
        let hit = ui
            .allocate_rect(zone, Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if !hit.clicked() {
            continue;
        }
        match icon {
            Icon::Speed => app.ping_group(key, ui.ctx()),
            Icon::Refresh => {
                app.shared.pings.lock().map(|mut g| g.clear()).unwrap_or(());
                app.ping_group(key, ui.ctx());
            }
            Icon::Kebab => {
                app.dialog = Dialog::Kebab {
                    group_id: key.to_string(),
                    kind: if key == "virtual:global" {
                        KebabKind::Global
                    } else {
                        KebabKind::Saved
                    },
                };
            }
            _ => {}
        }
    }

    let row_resp = ui.allocate_rect(rect, Sense::click());
    if row_resp.clicked() {
        app.groups.active_group = Some(key.to_string());
        app.groups.save();
    }
    if ui.allocate_rect(chev_rect, Sense::click()).clicked() {
        if expanded {
            app.expanded.remove(key);
        } else {
            app.expanded.insert(key.to_string());
        }
    }

    // Usage bar inside the expanded Global Network row.
    if expanded && key == "virtual:global" {
        let (down, up) = app.groups.total_usage();
        let bar_rect = Rect::from_min_max(
            Pos2::new(rect.left() + 38.0, rect.top() + 46.0),
            Pos2::new(rect.right() - 14.0, rect.top() + 52.0),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(bar_rect, 4.0, p.track);
        let total = (down + up).max(1) as f64;
        let frac = (down as f64 / total).clamp(0.0, 1.0);
        if frac > 0.0 {
            painter.rect_filled(
                Rect::from_min_max(
                    bar_rect.left_top(),
                    Pos2::new(
                        bar_rect.left() + bar_rect.width() * frac as f32,
                        bar_rect.bottom(),
                    ),
                ),
                4.0,
                p.green,
            );
        }
        painter.galley(
            Pos2::new(rect.left() + 38.0, rect.top() + 58.0),
            painter.layout_no_wrap(
                format!("{} / {}", format_bytes(down), format_bytes(down + up)),
                egui::FontId::new(11.5, egui::FontFamily::Name("mono".into())),
                p.text_dim,
            ),
            p.text_dim,
        );
    }
}

fn member_rows(app: &mut App, ui: &mut egui::Ui, p: &Palette, group: &crate::data::Group) {
    let profiles: Vec<npvt_core::profile::Profile> = group
        .members
        .iter()
        .filter_map(|id| app.data.profiles.iter().find(|prof| &prof.id == id))
        .cloned()
        .collect();
    profile_rows(app, ui, p, &profiles);
}

fn profile_rows(
    app: &mut App,
    ui: &mut egui::Ui,
    p: &Palette,
    profiles: &[npvt_core::profile::Profile],
) {
    let mut sorted: Vec<&npvt_core::profile::Profile> = profiles.iter().collect();
    match app.sort {
        crate::app::SortKey::Name => sorted.sort_by_key(|a| a.name.to_lowercase()),
        crate::app::SortKey::Created => {
            sorted.sort_by_key(|prof| std::cmp::Reverse(prof.created_at))
        }
        crate::app::SortKey::Ping => {}
    }
    let pings = app
        .shared
        .pings
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();
    if matches!(app.sort, crate::app::SortKey::Ping) {
        sorted.sort_by_key(|prof| {
            pings
                .get(&prof.id)
                .map(|o| o.rank())
                .unwrap_or(u32::MAX - 1)
        });
    }
    for prof in sorted {
        if !app.search.is_empty()
            && !prof
                .name
                .to_lowercase()
                .contains(&app.search.to_lowercase())
            && !prof
                .server
                .to_lowercase()
                .contains(&app.search.to_lowercase())
        {
            continue;
        }
        profile_row(app, ui, p, prof, &pings);
    }
}

fn profile_row(
    app: &mut App,
    ui: &mut egui::Ui,
    p: &Palette,
    prof: &npvt_core::profile::Profile,
    pings: &std::collections::BTreeMap<String, PingOutcome>,
) {
    let height = 46.0;
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().min(CONTENT_W) - 24.0, height),
        Sense::hover(),
    );
    let rect = Rect::from_min_max(
        Pos2::new(rect.left() + 24.0, rect.top()),
        rect.right_bottom(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 8.0, p.input_bg);

    let proto = prof.protocol.as_str();
    painter.galley(
        Pos2::new(rect.left() + 12.0, rect.top() + 6.0),
        painter.layout_no_wrap(
            prof.name.clone(),
            egui::FontId::new(13.5, egui::FontFamily::Name("sans-med".into())),
            p.text,
        ),
        p.text,
    );
    painter.galley(
        Pos2::new(rect.left() + 12.0, rect.top() + 25.0),
        painter.layout_no_wrap(
            format!("{proto} · {}:{}", prof.server, prof.port),
            egui::FontId::new(10.5, egui::FontFamily::Name("mono".into())),
            p.text_faint,
        ),
        p.text_faint,
    );

    // Ping chip.
    if let Some(outcome) = pings.get(&prof.id) {
        let text = outcome.short();
        let (r, g, b) = outcome.color();
        let chip = painter.layout_no_wrap(
            text,
            egui::FontId::new(10.5, egui::FontFamily::Name("mono-med".into())),
            egui::Color32::from_rgb(r, g, b),
        );
        painter.galley(
            Pos2::new(
                rect.right() - chip.size().x - 62.0,
                rect.center().y - chip.size().y / 2.0,
            ),
            chip,
            p.text_dim,
        );
    }

    // Actions: edit + delete.
    let mut x = rect.right() - 8.0;
    for icon in [Icon::Trash, Icon::Pencil] {
        let zone = Rect::from_center_size(Pos2::new(x - 13.0, rect.center().y), Vec2::splat(26.0));
        x -= 30.0;
        icons::paint(&painter, icon, zone.shrink(4.0), p.text_dim, 1.7);
        let hit = ui.allocate_rect(zone, Sense::click());
        if !hit.clicked() {
            continue;
        }
        if icon == Icon::Trash {
            app.dialog = Dialog::ConfirmDeleteProfile {
                profile_id: prof.id.clone(),
            };
        } else {
            let draft = crate::app::ManualDraft {
                name: prof.name.clone(),
                protocol_idx: npvt_core::profile::Protocol::ALL
                    .iter()
                    .position(|pr| *pr == prof.protocol)
                    .unwrap_or(0),
                server: prof.server.clone(),
                port: prof.port.to_string(),
                cred: match &prof.credential {
                    npvt_core::profile::Credential::Uuid(v)
                    | npvt_core::profile::Credential::Password(v)
                    | npvt_core::profile::Credential::Token(v) => v.clone(),
                    npvt_core::profile::Credential::SsMethod { password, .. } => password.clone(),
                    npvt_core::profile::Credential::None => String::new(),
                },
                ss_method: match &prof.credential {
                    npvt_core::profile::Credential::SsMethod { method, .. } => method.clone(),
                    _ => String::new(),
                },
                transport_idx: match prof.transport {
                    npvt_core::profile::Transport::Plain => 0,
                    npvt_core::profile::Transport::Tls => 1,
                    npvt_core::profile::Transport::Ws => 2,
                    npvt_core::profile::Transport::Grpc => 3,
                    npvt_core::profile::Transport::H2 => 4,
                },
                sni: prof.sni.clone().unwrap_or_default(),
                path: prof.path.clone().unwrap_or_default(),
                udp: prof.udp,
                insecure: prof.allow_insecure,
            };
            app.dialog = Dialog::EditProfile {
                profile_id: prof.id.clone(),
                draft,
            };
        }
    }

    // Click = make this profile the active pick (manual groups) / show info.
    let resp = ui.allocate_rect(rect, Sense::click());
    if resp.clicked() {
        if let Some(gid) = app.groups.active_group.clone() {
            app.groups.manual_pick.insert(gid, prof.id.clone());
            app.groups.save();
            app.toasts.show(format!("Selected {}", prof.name));
        }
    }
}

// ---- Floating pill toolbar ----------------------------------------------------

fn pill_toolbar(app: &mut App, ui: &mut egui::Ui, p: &Palette) {
    let screen = ui.ctx().screen_rect();
    let painter = ui.painter();
    let pill_w = 5.0 * 34.0 + 10.0;
    let pill_h = 46.0;
    let fab_r = 25.0;
    let y = screen.bottom() - 78.0;
    let pill_right = screen.right() - 76.0;
    let pill_rect =
        Rect::from_min_size(Pos2::new(pill_right - pill_w, y), Vec2::new(pill_w, pill_h));
    painter.rect_filled(pill_rect, 23.0, p.green_pill);
    painter.rect_stroke(pill_rect, 23.0, egui::Stroke::new(1.0_f32, p.green_dim));

    let icons_row = [
        Icon::Expand,
        Icon::Close,
        Icon::Refresh,
        Icon::Search,
        Icon::Sliders,
    ];
    for (i, icon) in icons_row.iter().enumerate() {
        let center = Pos2::new(
            pill_rect.left() + 17.0 + 34.0 * i as f32,
            pill_rect.center().y,
        );
        let zone = Rect::from_center_size(center, Vec2::splat(30.0));
        icons::paint(ui.painter(), *icon, zone.shrink(5.0), p.teal_text, 1.7);
        let hit = ui
            .allocate_rect(zone, Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if !hit.clicked() {
            continue;
        }
        match icon {
            Icon::Expand => {
                app.expanded.insert("virtual:saved".into());
                app.expanded.insert("virtual:global".into());
                for g in &app.groups.groups {
                    app.expanded.insert(g.id.clone());
                }
            }
            Icon::Close => {
                app.expanded.clear();
                app.search_open = false;
                app.search.clear();
            }
            Icon::Refresh => {
                app.shared.pings.lock().map(|mut g| g.clear()).unwrap_or(());
                app.ping_group(&active_group_id(app), ui.ctx());
            }
            Icon::Search => {
                app.search_open = !app.search_open;
            }
            Icon::Sliders => {
                app.dialog = Dialog::SortMenu;
            }
            _ => {}
        }
    }

    // FAB.
    let fab_center = Pos2::new(screen.right() - 40.0, y + pill_h / 2.0);
    let fab_rect = Rect::from_center_size(fab_center, Vec2::splat(fab_r * 2.0));
    ui.painter().circle_filled(fab_center, fab_r, p.green);
    icons::paint(
        ui.painter(),
        Icon::Plus,
        Rect::from_center_size(fab_center, Vec2::splat(22.0)),
        p.window_bg,
        2.4,
    );
    let fab_hit = ui
        .allocate_rect(fab_rect, Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if fab_hit.clicked() {
        app.dialog = Dialog::AddMenu;
    }
}

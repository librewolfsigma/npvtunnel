//! The reusable design-system widgets, each matching the reference design:
//! bottom navigation with the teal pill, the big circular power button, the
//! card frames, radio rows, the Light/Dark/Auto segmented control, the
//! gradient secrecy slider, the usage bar, modals and toasts.

use std::time::Duration;

use egui::{
    Align2, Color32, Context, Id, Layout, Order, Painter, Rect, Response, Rounding, Sense, Ui, Vec2,
};

use crate::icons::{self, Icon};
use crate::theme::{self, Palette};

// ---- Header ---------------------------------------------------------------

/// App header: optional back chevron, tracked green title + version, and the
/// optional premium badge on the right. Returns (back_clicked, badge_clicked).
pub fn header(ui: &mut Ui, p: &Palette, back: bool, premium: bool) -> (bool, bool) {
    let mut back_clicked = false;
    let mut badge_clicked = false;
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        if back {
            if icons::icon_button(ui, Icon::ChevronLeft, 30.0, p.text_dim, None).clicked() {
                back_clicked = true;
            }
        } else {
            ui.add_space(30.0);
        }
        let title_w = ui.available_width() - 30.0 - ui.spacing().item_spacing.x * 2.0;
        if premium {
            let badge_w = 40.0;
            let title_w = title_w - badge_w;
            let (rect, _) = ui.allocate_exact_size(Vec2::new(title_w, 44.0), Sense::hover());
            paint_title(ui.painter(), p, rect);
            if icons::icon_button(ui, Icon::Star, 34.0, p.gold, None).clicked() {
                badge_clicked = true;
            }
        } else {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(title_w, 44.0), Sense::hover());
            paint_title(ui.painter(), p, rect);
            ui.add_space(30.0);
        }
    });
    (back_clicked, badge_clicked)
}

fn paint_title(painter: &Painter, p: &Palette, rect: Rect) {
    let galley_title = painter.layout_no_wrap(
        theme::tracked("NPV TUNNEL"),
        egui::FontId::new(15.0, egui::FontFamily::Name("mono-bold".into())),
        p.green,
    );
    let galley_ver = painter.layout_no_wrap(
        format!("v{}", env!("CARGO_PKG_VERSION")),
        egui::FontId::new(9.5, egui::FontFamily::Name("mono".into())),
        p.text_faint,
    );
    let cx = rect.center().x;
    painter.galley(
        Pos2::new(cx - galley_title.size().x / 2.0, rect.top() + 6.0),
        galley_title,
        p.text,
    );
    painter.galley(
        Pos2::new(cx - galley_ver.size().x / 2.0, rect.top() + 26.0),
        galley_ver,
        p.text_dim,
    );
}

use egui::Pos2;

// ---- Power button -----------------------------------------------------------

pub enum PowerVisual {
    Off,
    Connecting,
    On,
}

/// The large circular connect button with its ring.
pub fn power_button(ui: &mut Ui, p: &Palette, visual: &PowerVisual) -> Response {
    let size = 128.0_f32.min(ui.available_width() * 0.5);
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let painter = ui.painter_at(rect.expand(4.0));
    let center = rect.center();
    let time = ui.input(|i| i.time) as f32;

    let (ring_color, glow) = match visual {
        PowerVisual::On => (p.green, 0.85),
        PowerVisual::Connecting => {
            let pulse = (time * 3.2).sin() * 0.5 + 0.5;
            (
                Color32::from_rgba_unmultiplied(63, 217, 127, (110.0 + pulse * 120.0) as u8),
                0.4 + pulse * 0.4,
            )
        }
        PowerVisual::Off => (Color32::from_rgba_unmultiplied(63, 217, 127, 90), 0.35),
    };

    // Soft glow ring.
    let glow_rect = Rect::from_center_size(center, Vec2::splat(size * 0.94));
    painter.circle_filled(
        center,
        glow_rect.width() / 2.0,
        Color32::from_rgba_unmultiplied(63, 217, 127, (glow * 26.0) as u8),
    );
    // Main ring.
    let ring_rect = Rect::from_center_size(center, Vec2::splat(size * 0.86));
    painter.circle_stroke(
        center,
        ring_rect.width() / 2.0,
        egui::Stroke::new(2.0_f32, ring_color),
    );
    // Power glyph.
    icons::paint(
        &painter,
        Icon::Power,
        Rect::from_center_size(center, Vec2::splat(size * 0.44)),
        ring_color,
        2.6,
    );
    resp
}

// ---- Bottom navigation ------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Configs,
    Logs,
    More,
}

impl Page {
    pub fn label(self) -> &'static str {
        match self {
            Page::Home => "HOME",
            Page::Configs => "CONFIGS",
            Page::Logs => "LOGS",
            Page::More => "MORE",
        }
    }
    pub fn icon(self) -> Icon {
        match self {
            Page::Home => Icon::Home,
            Page::Configs => Icon::Cloud,
            Page::Logs => Icon::Info,
            Page::More => Icon::Dots,
        }
    }
}

/// The fixed bottom navigation bar.
pub fn nav_bar(ui: &mut Ui, p: &Palette, page: &mut Page) {
    let height = 64.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, p.card);
    painter.line_segment(
        [rect.left_top(), rect.right_top()],
        egui::Stroke::new(1.0_f32, p.card_stroke),
    );
    let count = 4;
    let w = rect.width() / count as f32;
    for (i, item) in [Page::Home, Page::Configs, Page::Logs, Page::More]
        .into_iter()
        .enumerate()
    {
        let zone = Rect::from_min_max(
            Pos2::new(rect.left() + w * i as f32, rect.top()),
            Pos2::new(rect.left() + w * (i + 1) as f32, rect.bottom()),
        );
        let selected = *page == item;
        let color = if selected { p.teal_text } else { p.text_faint };
        // Teal pill behind the icon of the selected tab.
        if selected {
            let pill = Rect::from_center_size(
                Pos2::new(zone.center().x, rect.top() + 21.0),
                Vec2::new(w * 0.52, 32.0),
            );
            painter.rect_filled(pill, 16.0, p.teal_bg);
        }
        let icon_rect = Rect::from_center_size(
            Pos2::new(zone.center().x, rect.top() + 21.0),
            Vec2::splat(20.0),
        );
        icons::paint(&painter, item.icon(), icon_rect, color, 1.7);
        painter.galley(
            Pos2::new(
                zone.center().x
                    - painter
                        .layout_no_wrap(
                            item.label().to_owned(),
                            egui::FontId::new(9.5, egui::FontFamily::Name("mono-med".into())),
                            color,
                        )
                        .size()
                        .x
                        / 2.0,
                rect.top() + 38.0,
            ),
            painter.layout_no_wrap(
                item.label().to_owned(),
                egui::FontId::new(9.5, egui::FontFamily::Name("mono-med".into())),
                color,
            ),
            color,
        );
        let hit = ui
            .allocate_rect(zone, Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if hit.clicked() {
            *page = item;
        }
    }
}

// ---- Radio rows ---------------------------------------------------------------

/// A selectable radio row with title + description.
pub fn radio_row(ui: &mut Ui, p: &Palette, selected: bool, title: &str, desc: &str) -> Response {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let painter = ui.painter_at(rect);
    let ring_center = Pos2::new(rect.left() + 12.0, rect.center().y);
    let ring_color = if selected { p.green } else { p.text_faint };
    painter.circle_stroke(ring_center, 8.0, egui::Stroke::new(1.6_f32, ring_color));
    if selected {
        painter.circle_filled(ring_center, 4.2, p.green);
    }
    let text_x = rect.left() + 30.0;
    painter.galley(
        Pos2::new(text_x, rect.top() + 4.0),
        painter.layout_no_wrap(
            title.to_owned(),
            egui::FontId::new(14.0, egui::FontFamily::Name("sans-med".into())),
            if selected { p.text } else { p.text_dim },
        ),
        p.text,
    );
    painter.galley(
        Pos2::new(text_x, rect.top() + 24.0),
        painter.layout_no_wrap(
            desc.to_owned(),
            egui::FontId::new(11.5, egui::FontFamily::Proportional),
            p.text_faint,
        ),
        p.text_dim,
    );
    resp
}

// ---- Segmented control (Light / Dark / Auto) ---------------------------------

/// One segment of the segmented control.
pub fn segmented(ui: &mut Ui, p: &Palette, options: &[(&str, Icon)], selected: usize) -> Vec<bool> {
    let mut clicked = vec![false; options.len()];
    let height = 34.0;
    let width = ui.available_width().min(430.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 17.0, p.input_bg);
    painter.rect_stroke(rect, 17.0, egui::Stroke::new(1.0_f32, p.card_stroke));
    let w = width / options.len() as f32;
    for (i, (label, icon)) in options.iter().enumerate() {
        let zone = Rect::from_min_max(
            Pos2::new(rect.left() + w * i as f32, rect.top()),
            Pos2::new(rect.left() + w * (i + 1) as f32, rect.bottom()),
        );
        let is_sel = i == selected;
        if is_sel {
            let pill = zone.shrink2(Vec2::new(3.0, 3.0));
            painter.rect_filled(pill, 14.0, p.teal_bg);
        }
        let color = if is_sel { p.teal_text } else { p.text_dim };
        let text_galley = painter.layout_no_wrap(
            (*label).to_owned(),
            egui::FontId::new(12.5, egui::FontFamily::Name("sans-med".into())),
            color,
        );
        let icon_rect = Rect::from_center_size(
            Pos2::new(
                zone.center().x - text_galley.size().x / 2.0 - 12.0,
                zone.center().y,
            ),
            Vec2::splat(15.0),
        );
        icons::paint(&painter, *icon, icon_rect, color, 1.5);
        painter.galley(
            Pos2::new(
                zone.center().x - text_galley.size().x / 2.0 + 4.0,
                zone.center().y - text_galley.size().y / 2.0,
            ),
            text_galley,
            color,
        );
        let hit = ui.allocate_rect(zone, Sense::click());
        clicked[i] = hit.clicked();
    }
    clicked
}

// ---- Secrecy slider -----------------------------------------------------------

/// Gradient secrecy slider; returns true when the value changed.
pub fn secrecy_slider(ui: &mut Ui, p: &Palette, level: &mut usize) -> bool {
    let width = ui.available_width().min(430.0);
    let height = 26.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let track = Rect::from_min_max(
        Pos2::new(rect.left(), rect.center().y - 4.0),
        Pos2::new(rect.right(), rect.center().y + 4.0),
    );
    painter.rect_filled(track, 4.0, p.track);

    // Gradient: red -> amber -> green across the full track.
    let steps = 60;
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let color = secrecy_gradient(t, p);
        let seg = Rect::from_min_max(
            Pos2::new(track.left() + track.width() * t, track.top()),
            Pos2::new(
                track.left() + track.width() * ((i + 1) as f32 / steps as f32),
                track.bottom(),
            ),
        );
        let alpha = if (t * 4.0).round() as usize <= *level {
            255
        } else {
            70
        };
        painter.rect_filled(
            seg,
            0.0,
            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha),
        );
    }
    // Handle at the current level.
    let t = *level as f32 / 4.0;
    let handle = Pos2::new(track.left() + track.width() * t, track.center().y);
    painter.circle_filled(handle, 7.0, p.card);
    painter.circle_stroke(handle, 7.0, egui::Stroke::new(2.0_f32, p.text));
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    let mut changed = false;
    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
            let new_level = (t * 4.0).round() as usize;
            if new_level != *level {
                *level = new_level;
                changed = true;
            }
        }
    }
    changed
}

fn secrecy_gradient(t: f32, p: &Palette) -> Color32 {
    let red = p.red;
    let amber = p.orange;
    let green = p.green;
    if t < 0.5 {
        lerp_color(red, amber, t / 0.5)
    } else {
        lerp_color(amber, green, (t - 0.5) / 0.5)
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
    )
}

// ---- Usage bar -----------------------------------------------------------

/// The green+blue usage bar under an expanded group.
pub fn usage_bar(ui: &mut Ui, p: &Palette, used: f64, total: f64) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 8.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, p.track);
    let frac = if total > 0.0 {
        (used / total).clamp(0.0, 1.0)
    } else {
        0.0
    };
    if frac > 0.0 {
        let green_rect = Rect::from_min_max(
            rect.left_top(),
            Pos2::new(rect.left() + rect.width() * frac as f32, rect.bottom()),
        );
        painter.rect_filled(green_rect, 4.0, p.green);
        // Thin blue continuation, like the reference.
        let blue_end = (frac as f32 * 1.25).clamp(frac as f32, 1.0);
        if blue_end > frac as f32 {
            let blue_rect = Rect::from_min_max(
                Pos2::new(
                    rect.left() + rect.width() * frac as f32,
                    rect.center().y - 1.0,
                ),
                Pos2::new(rect.left() + rect.width() * blue_end, rect.center().y + 1.0),
            );
            painter.rect_filled(blue_rect, 1.0, p.cyan);
        }
    }
}

// ---- Modal frame -----------------------------------------------------------

/// Dim the screen and run `body` inside a centered rounded window.
pub fn modal<R>(
    ctx: &Context,
    p: &Palette,
    id: &str,
    width: f32,
    body: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let layer_id = egui::LayerId::new(Order::Middle, Id::new("npvt-scrim"));
    let layer = egui::Painter::new(ctx.clone(), layer_id, ctx.screen_rect());
    layer.rect_filled(ctx.screen_rect(), 0.0, p.scrim);
    let mut out = None;
    egui::Area::new(Id::new(id))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let frame = egui::Frame::default()
                .fill(p.card)
                .rounding(Rounding::same(16.0))
                .stroke(egui::Stroke::new(1.0_f32, p.card_stroke))
                .inner_margin(14.0)
                .shadow(egui::epaint::Shadow::NONE);
            let inner = frame.show(ui, |ui| {
                ui.set_max_width(width);
                ui.set_max_height(ui.ctx().screen_rect().height() * 0.82);
                egui::ScrollArea::vertical().show(ui, |ui| body(ui)).inner
            });
            out = Some(inner.inner);
        });
    out
}

/// Modal header: bold title left, optional green "Cancel"-style action right.
pub fn modal_header(ui: &mut Ui, p: &Palette, title: &str, action: Option<&str>) -> bool {
    let mut action_clicked = false;
    ui.horizontal(|ui| {
        ui.label(theme::sans_bold(title, 19.0, p.text));
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(action) = action {
                if ui
                    .add(
                        egui::Button::new(theme::mono_med(action, 13.0, p.green))
                            .fill(Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE),
                    )
                    .clicked()
                {
                    action_clicked = true;
                }
            }
        });
    });
    action_clicked
}

// ---- Toasts -----------------------------------------------------------------

struct Toast {
    message: String,
    kind: ToastKind,
    born: std::time::Instant,
}

#[derive(PartialEq, Clone, Copy)]
enum ToastKind {
    Good,
    Bad,
}

/// Floating toast queue shown above the nav bar.
#[derive(Default)]
pub struct Toasts {
    items: Vec<Toast>,
}

impl Toasts {
    pub fn show(&mut self, message: impl Into<String>) {
        self.items.push(Toast {
            message: message.into(),
            kind: ToastKind::Good,
            born: std::time::Instant::now(),
        });
        if self.items.len() > 3 {
            self.items.remove(0);
        }
    }
    pub fn error(&mut self, message: impl Into<String>) {
        self.items.push(Toast {
            message: message.into(),
            kind: ToastKind::Bad,
            born: std::time::Instant::now(),
        });
        if self.items.len() > 3 {
            self.items.remove(0);
        }
    }

    /// Draw active toasts; prunes expired ones.
    pub fn paint(&mut self, ctx: &Context, p: &Palette) {
        let now = std::time::Instant::now();
        self.items
            .retain(|t| now.duration_since(t.born).as_secs_f32() < 2.8);
        if self.items.is_empty() {
            return;
        }
        let screen = ctx.screen_rect();
        let layer_id = egui::LayerId::new(Order::Foreground, Id::new("npvt-toasts"));
        let painter = egui::Painter::new(ctx.clone(), layer_id, ctx.screen_rect());
        let mut y = screen.bottom() - 84.0;
        for toast in self.items.iter().rev() {
            let galley = painter.layout_no_wrap(
                toast.message.clone(),
                egui::FontId::new(12.5, egui::FontFamily::Name("mono-med".into())),
                if toast.kind == ToastKind::Good {
                    p.green
                } else {
                    p.red
                },
            );
            let w = galley.size().x + 36.0;
            let rect = Rect::from_center_size(
                Pos2::new(screen.center().x, y),
                Vec2::new(w.min(screen.width() - 24.0), 34.0),
            );
            painter.rect_filled(rect, 17.0, p.card);
            painter.rect_stroke(rect, 17.0, egui::Stroke::new(1.0_f32, p.card_stroke));
            painter.galley(
                Pos2::new(rect.left() + 16.0, rect.center().y - galley.size().y / 2.0),
                galley,
                p.text,
            );
            y -= 42.0;
        }
        ctx.request_repaint_after(Duration::from_millis(400));
    }
}

// ---- Small helpers -----------------------------------------------------------

/// Tracked label row used all over the design (`LAST PING`, `APPEARANCE`...).
pub fn tracked_label(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(theme::mono_label_tracked(text, 10.5, p.text_faint));
}

/// A filled primary button with the design's rounded look.
pub fn fill_button(ui: &mut Ui, p: &Palette, text: &str, enabled: bool) -> Response {
    let btn = egui::Button::new(theme::sans_med(
        text,
        14.0,
        if enabled { p.text } else { p.text_faint },
    ))
    .fill(if enabled { p.green_deep } else { p.input_bg })
    .stroke(egui::Stroke::new(
        1.0_f32,
        if enabled {
            p.green_border
        } else {
            p.card_stroke
        },
    ))
    .rounding(Rounding::same(10.0))
    .min_size(Vec2::new(ui.available_width(), 38.0));
    ui.add_enabled(enabled, btn)
}

/// Tracked text button (PING) in the design's cyan.
pub fn text_button(ui: &mut Ui, p: &Palette, text: &str) -> Response {
    let btn = egui::Button::new(theme::mono_med(theme::tracked(text), 12.5, p.cyan))
        .fill(Color32::TRANSPARENT)
        .stroke(egui::Stroke::NONE);
    ui.add(btn).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// One row of a menu/modal list: icon + label, hover highlight.
pub fn menu_item(ui: &mut Ui, p: &Palette, icon: Icon, label: &str, on_click: impl FnOnce()) {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), egui::Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        ui.painter_at(rect).rect_filled(rect, 8.0, p.green_deep);
    }
    let painter = ui.painter_at(rect);
    icons::paint(
        &painter,
        icon,
        Rect::from_center_size(
            Pos2::new(rect.left() + 18.0, rect.center().y),
            Vec2::splat(20.0),
        ),
        p.text_dim,
        1.7,
    );
    painter.galley(
        Pos2::new(rect.left() + 40.0, rect.center().y - 9.0),
        painter.layout_no_wrap(
            label.to_owned(),
            egui::FontId::new(13.5, egui::FontFamily::Proportional),
            p.text,
        ),
        p.text,
    );
    if resp.clicked() {
        on_click();
    }
}

/// Divider line.
pub fn divider(ui: &mut Ui, p: &Palette) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 1.0), Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        egui::Stroke::new(1.0_f32, p.card_stroke),
    );
}

/// One row of the MORE page: icon + title + subtitle.
pub fn more_row(ui: &mut Ui, p: &Palette, icon: Icon, title: &str, desc: &str) -> Response {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        ui.painter_at(rect).rect_filled(rect, 10.0, p.green_deep);
    }
    let painter = ui.painter_at(rect);
    icons::paint(
        &painter,
        icon,
        Rect::from_center_size(
            Pos2::new(rect.left() + 20.0, rect.center().y),
            Vec2::splat(22.0),
        ),
        p.green,
        1.7,
    );
    let text_x = rect.left() + 44.0;
    painter.galley(
        Pos2::new(text_x, rect.top() + 6.0),
        painter.layout_no_wrap(
            title.to_owned(),
            egui::FontId::new(15.0, egui::FontFamily::Name("sans-med".into())),
            p.text,
        ),
        p.text,
    );
    painter.galley(
        Pos2::new(text_x, rect.top() + 27.0),
        painter.layout_no_wrap(
            desc.to_owned(),
            egui::FontId::new(12.0, egui::FontFamily::Proportional),
            p.text_dim,
        ),
        p.text_dim,
    );
    resp
}

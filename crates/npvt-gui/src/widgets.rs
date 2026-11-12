//! Custom widgets replicating the reference design: cards, the bottom
//! navigation bar, the big power button, segmented controls, radios,
//! gradient sliders, modals and toasts.

use eframe::egui::{
    self,
    epaint::{PathShape, PathStroke},
    Color32, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke, Vec2,
};

use crate::icons;
use crate::theme::{self, Palette};

/// Allocate + paint a card; `fill` overrides the surface color.
pub fn card(
    ui: &mut egui::Ui,
    palette: Palette,
    height: f32,
    fill: Option<Color32>,
    border: Option<Stroke>,
) -> (Rect, egui::Response) {
    let desired = Vec2::new(ui.available_width(), height);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    let fill = fill.unwrap_or(palette.surface);
    let border = border.unwrap_or(Stroke::new(1.0, palette.border));
    ui.painter().add(Shape::Rect(egui::epaint::RectShape::new(
        rect,
        egui::Rounding::same(10.0),
        fill,
        border,
    )));
    (rect, response)
}

/// Paint the whole UI content of a card via painter coordinates.
pub fn card_frame(
    ui: &mut egui::Ui,
    palette: Palette,
    height: f32,
    fill: Color32,
    border: Stroke,
    paint: impl FnOnce(&egui::Painter, Rect),
) -> egui::Response {
    let (rect, response) = card(ui, palette, height, Some(fill), Some(border));
    paint(ui.painter(), rect);
    response
}

pub fn label_at(
    painter: &Painter,
    anchor: egui::Align2,
    pos: Pos2,
    text: &str,
    font: FontId,
    color: Color32,
) {
    painter.text(pos, anchor, text, font, color);
}

/// Bottom navigation bar with pill highlight for the active tab.
pub fn nav_bar(ui: &mut egui::Ui, palette: Palette, active: usize, clicked: &mut Option<usize>) {
    let tabs: [(&str, icons::IconFn); 4] = [
        ("HOME", icons::home),
        ("CONFIGS", icons::cloud),
        ("LOGS", icons::info),
        ("MORE", icons::ellipsis),
    ];
    let height = 76.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    ui.painter().add(Shape::Rect(egui::epaint::RectShape::new(
        rect,
        egui::Rounding::same(0.0),
        palette.bg,
        Stroke::new(1.0, palette.border),
    )));
    let tab_w = rect.width() / tabs.len() as f32;
    for (i, (name, draw)) in tabs.iter().enumerate() {
        let tab_rect = Rect::from_min_size(
            Pos2::new(rect.left() + tab_w * i as f32, rect.top()),
            Vec2::new(tab_w, height),
        );
        let icon_rect = Rect::from_center_size(
            Pos2::new(tab_rect.center().x, rect.top() + 22.0),
            Vec2::new(26.0, 26.0),
        );
        let pill = Rect::from_center_size(
            Pos2::new(tab_rect.center().x, rect.top() + 22.0),
            Vec2::new(56.0, 34.0),
        );
        let is_active = i == active;
        let painter = ui.painter();
        let (icon_color, text_color) = if is_active {
            painter.add(Shape::Rect(egui::epaint::RectShape::new(
                pill,
                egui::Rounding::same(17.0),
                palette.teal_pill,
                Stroke::NONE,
            )));
            (palette.teal_text, palette.teal_text)
        } else {
            (palette.text_faint, palette.text_faint)
        };
        icons::paint_with(painter, icon_rect, icon_color, *draw, icons::ICON_STROKE);
        label_at(
            painter,
            egui::Align2::CENTER_CENTER,
            Pos2::new(tab_rect.center().x, rect.top() + 52.0),
            name,
            theme::semibold(10.0),
            text_color,
        );
        let id = egui::Id::new("nav").with(i);
        let resp = ui
            .interact(tab_rect, id, Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.clicked() {
            *clicked = Some(i);
        }
    }
}

/// The big circular power button with glow/pulse animation.
pub fn power_button(ui: &mut egui::Ui, palette: Palette, state: u8, time: f64) -> bool {
    let size = Vec2::new(190.0, 190.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let painter = ui.painter();
    let c = rect.center();

    let connected = state == crate::engine_bridge::STATE_CONNECTED;
    let connecting = state == crate::engine_bridge::STATE_CONNECTING;
    let error = state == crate::engine_bridge::STATE_ERROR;

    // Animated ring.
    let base_r = 74.0;
    let mut ring_color = if connected {
        palette.accent
    } else if connecting {
        palette.amber
    } else if error {
        palette.red
    } else {
        palette.accent.gamma_multiply(0.85)
    };

    // Glow when connected (three soft strokes) + slow pulse.
    if connected || connecting {
        let pulse = (((time * 1.6).sin() * 0.5 + 0.5) * 6.0) as f32;
        for (i, alpha) in [0.10f32, 0.06, 0.03].into_iter().enumerate() {
            painter.add(Shape::circle_stroke(
                c,
                base_r + 6.0 + pulse + 6.0 * i as f32,
                Stroke::new(3.0 + 2.0 * i as f32, ring_color.gamma_multiply(alpha)),
            ));
        }
        if connected {
            ring_color = palette.accent_bright;
        }
    }

    // Button fill.
    let fill = if connected {
        palette.accent.gamma_multiply(0.18)
    } else {
        palette.surface
    };
    painter.add(Shape::Rect(egui::epaint::RectShape::new(
        Rect::from_center_size(c, Vec2::splat(base_r * 2.0)),
        egui::Rounding::same(base_r),
        fill,
        Stroke::new(if connected { 3.0 } else { 2.2 }, ring_color),
    )));

    // Power glyph.
    let glyph_rect = Rect::from_center_size(c, Vec2::splat(64.0));
    icons::paint_with(painter, glyph_rect, ring_color, icons::power, 4.2);
    response.clicked()
}

/// Status text like `[ CONNECTED ]`.
pub fn status_text(ui: &mut egui::Ui, palette: Palette, state: u8, time: f64) {
    let (word, color) = match state {
        crate::engine_bridge::STATE_CONNECTED => ("CONNECTED", palette.accent),
        crate::engine_bridge::STATE_CONNECTING => {
            if ((time * 2.0) as usize) % 2 == 0 {
                ("CONNECTING", palette.amber)
            } else {
                ("CONNECTING.", palette.amber)
            }
        }
        crate::engine_bridge::STATE_ERROR => ("ERROR", palette.red),
        _ => ("DISCONNECTED", palette.text_faint),
    };
    let text = format!("[ {word} ]");
    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
        ui.label(
            egui::RichText::new(&text)
                .font(theme::mono_bold(15.0))
                .color(color),
        );
    });
}

/// Segmented control (Light / Dark / Auto).
pub fn segmented(
    ui: &mut egui::Ui,
    palette: Palette,
    options: &[&str],
    selected: usize,
    icons_: &[fn(&icons::IconCtx)],
) -> Option<usize> {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 44.0), Sense::hover());
    let painter = ui.painter();
    painter.add(Shape::Rect(egui::epaint::RectShape::new(
        rect,
        egui::Rounding::same(14.0),
        palette.surface,
        Stroke::new(1.0, palette.border),
    )));
    let w = rect.width() / options.len() as f32;
    let mut clicked = None;
    for (i, label) in options.iter().enumerate() {
        let seg = Rect::from_min_size(
            Pos2::new(rect.left() + w * i as f32, rect.top()),
            Vec2::new(w, rect.height()),
        );
        let is_sel = i == selected;
        if is_sel {
            painter.add(Shape::Rect(egui::epaint::RectShape::new(
                seg.shrink2(Vec2::new(3.0, 3.0)),
                egui::Rounding::same(11.0),
                palette.teal_pill,
                Stroke::NONE,
            )));
        }
        let icon_rect = Rect::from_center_size(
            Pos2::new(seg.left() + 24.0, seg.center().y),
            Vec2::splat(17.0),
        );
        let text_color = if is_sel {
            palette.teal_text
        } else {
            palette.text_secondary
        };
        if let Some(draw) = icons_.get(i) {
            icons::paint_with(painter, icon_rect, text_color, *draw, icons::ICON_STROKE);
        }
        label_at(
            painter,
            egui::Align2::LEFT_CENTER,
            Pos2::new(seg.left() + 36.0, seg.center().y),
            label,
            theme::semibold(12.5),
            text_color,
        );
        let resp = ui
            .interact(seg, egui::Id::new("segmented").with(i), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.clicked() && !is_sel {
            clicked = Some(i);
        }
    }
    clicked
}

/// Custom radio row with title + description, green selection ring.
pub fn radio_row(
    ui: &mut egui::Ui,
    palette: Palette,
    title: &str,
    desc: &str,
    selected: bool,
) -> bool {
    let width = ui.available_width();
    let desc_lines = desc.len() as f32 / 38.0;
    let height = 46.0 + (desc_lines.floor() * 16.0).max(16.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let painter = ui.painter();
    let ring_c = Pos2::new(rect.left() + 16.0, rect.top() + 18.0);
    if selected {
        painter.circle_filled(ring_c, 8.5, palette.accent);
        painter.circle_stroke(ring_c, 8.5, Stroke::new(1.4, palette.accent));
        painter.circle_filled(ring_c, 3.4, palette.bg);
    } else {
        painter.circle_stroke(ring_c, 8.0, Stroke::new(1.6, palette.text_faint));
    }
    label_at(
        painter,
        egui::Align2::LEFT_TOP,
        Pos2::new(rect.left() + 34.0, rect.top() + 7.0),
        title,
        theme::semibold(14.5),
        if selected {
            palette.text
        } else {
            palette.text_secondary
        },
    );
    wrap_label_at(
        painter,
        ui,
        Rect::from_min_size(
            Pos2::new(rect.left() + 34.0, rect.top() + 28.0),
            Vec2::new(width - 40.0, height - 30.0),
        ),
        desc,
        FontId::new(12.0, egui::FontFamily::Proportional),
        palette.text_faint,
    );
    resp.clicked()
}

/// A disabled radio row (greyed out, like the "Share link" option).
pub fn radio_row_disabled(ui: &mut egui::Ui, palette: Palette, title: &str, desc: &str) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 50.0), Sense::hover());
    let painter = ui.painter();
    let dim = palette.text_faint.gamma_multiply(0.55);
    painter.circle_stroke(
        Pos2::new(rect.left() + 16.0, rect.top() + 18.0),
        8.0,
        Stroke::new(1.6, dim),
    );
    label_at(
        painter,
        egui::Align2::LEFT_TOP,
        Pos2::new(rect.left() + 34.0, rect.top() + 7.0),
        title,
        theme::semibold(14.5),
        dim,
    );
    label_at(
        painter,
        egui::Align2::LEFT_TOP,
        Pos2::new(rect.left() + 34.0, rect.top() + 30.0),
        desc,
        FontId::new(12.0, egui::FontFamily::Proportional),
        dim,
    );
}

/// Word-wrap a description into the given rect using egui's galley layout.
fn wrap_label_at(
    painter: &Painter,
    ui: &egui::Ui,
    rect: Rect,
    text: &str,
    font: FontId,
    color: Color32,
) {
    let galley = ui
        .ctx()
        .fonts(|f| f.layout(text.to_owned(), font, color, rect.width()));
    painter.galley(Pos2::new(rect.left(), rect.top()), galley, color);
}

/// Gradient progress bar (rounded, green->blue like the design).
pub fn progress_bar(
    painter: &Painter,
    rect: Rect,
    fraction: f32,
    c1: Color32,
    c2: Color32,
    track: Color32,
) {
    painter.add(Shape::Rect(egui::epaint::RectShape::new(
        rect,
        egui::Rounding::same(2.5),
        track,
        Stroke::NONE,
    )));
    let frac = fraction.clamp(0.0, 1.0);
    if frac <= 0.0 {
        return;
    }
    let fill_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width() * frac, rect.height()));
    let steps = 24;
    for i in 0..steps {
        let t0 = i as f32 / steps as f32;
        let t1 = (i + 1) as f32 / steps as f32;
        let x0 = fill_rect.left() + fill_rect.width() * t0;
        let x1 = fill_rect.left() + fill_rect.width() * t1;
        let color = lerp_color(c1, c2, (t0 + t1) * 0.5);
        painter.add(Shape::rect_filled(
            Rect::from_min_size(
                Pos2::new(x0, fill_rect.top()),
                Vec2::new(x1 - x0 + 0.5, fill_rect.height()),
            ),
            0.0,
            color,
        ));
    }
}

/// Multi-stop gradient slider for the export secrecy control.
pub fn gradient_slider(
    ui: &mut egui::Ui,
    palette: Palette,
    rect_height: f32,
    stops: &[Color32],
    fraction: f32,
) -> (f32, bool) {
    let width = ui.available_width();
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(width, rect_height), Sense::click_and_drag());
    let painter = ui.painter();
    let steps = 60;
    for i in 0..steps {
        let t = i as f32 / (steps - 1) as f32;
        let color = sample_gradient(stops, t);
        let x0 = rect.left() + rect.width() * (i as f32 / steps as f32);
        let x1 = rect.left() + rect.width() * ((i + 1) as f32 / steps as f32);
        painter.add(Shape::rect_filled(
            Rect::from_min_size(
                Pos2::new(x0, rect.top()),
                Vec2::new(x1 - x0 + 0.6, rect.height()),
            ),
            0.0,
            color,
        ));
    }
    // Unfilled remainder dimmed by an overlay.
    let fill_w = rect.width() * fraction.clamp(0.0, 1.0);
    if fill_w < rect.width() {
        painter.add(Shape::rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.left() + fill_w, rect.top()),
                Vec2::new(rect.width() - fill_w, rect.height()),
            ),
            0.0,
            palette.bg.gamma_multiply(0.72),
        ));
    }
    let mut changed = false;
    let mut value = fraction;
    if resp.dragged() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            changed = (t - fraction).abs() > 0.0001;
            value = t;
        }
    } else if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            changed = true;
            value = t;
        }
    }
    // Handle.
    painter.circle_filled(
        Pos2::new(rect.left() + rect.width() * value, rect.center().y),
        9.0,
        Color32::WHITE,
    );
    (value, changed)
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(
        a.r() + ((b.r() as f32 - a.r() as f32) * t) as u8,
        a.g() + ((b.g() as f32 - a.g() as f32) * t) as u8,
        a.b() + ((b.b() as f32 - a.b() as f32) * t) as u8,
        255,
    )
}

pub fn sample_gradient(stops: &[Color32], t: f32) -> Color32 {
    if stops.is_empty() {
        return Color32::GRAY;
    }
    if stops.len() == 1 {
        return stops[0];
    }
    let t = t.clamp(0.0, 1.0);
    let seg = (t * (stops.len() - 1) as f32).floor() as usize;
    let local_t = t * (stops.len() - 1) as f32 - seg as f32;
    let a = stops[seg.min(stops.len() - 2)];
    let b = stops[(seg + 1).min(stops.len() - 1)];
    lerp_color(a, b, local_t)
}

/// Push a modal: dim the screen, then paint a centered card with content.
/// Returns false when the backdrop was clicked or Esc was pressed (caller
/// closes the modal).
pub fn modal(
    ctx: &egui::Context,
    palette: Palette,
    id: &str,
    width: f32,
    height: f32,
    content: impl FnOnce(&mut egui::Ui),
) -> bool {
    let screen = ctx.screen_rect();
    let scrim_layer = egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new(format!("{id}-scrim")),
    );
    let scrim = ctx.layer_painter(scrim_layer);
    scrim.rect_filled(screen, 0.0, palette.scrim);

    let card_layer =
        egui::LayerId::new(egui::Order::Foreground, egui::Id::new(format!("{id}-card")));
    let painter = ctx.layer_painter(card_layer);
    let card_rect = Rect::from_center_size(
        screen.center(),
        Vec2::new(
            width.min(screen.width() - 24.0),
            height.min(screen.height() * 0.88),
        ),
    );
    painter.add(Shape::Rect(egui::epaint::RectShape::new(
        card_rect,
        egui::Rounding::same(14.0),
        palette.bg,
        Stroke::new(1.0, palette.border),
    )));
    let mut ui = egui::Ui::new(
        ctx.clone(),
        card_layer,
        egui::Id::new(format!("{id}-ui")),
        egui::UiBuilder::new().max_rect(card_rect.shrink(16.0)),
    );
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(&mut ui, |ui| content(ui));

    let clicked_outside = ctx.input(|i| {
        i.pointer.any_click()
            && i.pointer
                .interact_pos()
                .map(|p| !card_rect.contains(p))
                .unwrap_or(false)
    });
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    !(clicked_outside || esc)
}

/// Simple toast notification pinned to the bottom of the screen.
pub struct Toast {
    pub message: String,
    pub born: f64,
}

impl Toast {
    pub fn new(message: impl Into<String>, now: f64) -> Self {
        Toast {
            message: message.into(),
            born: now,
        }
    }

    pub fn expired(&self, now: f64) -> bool {
        now - self.born > 3.4
    }

    pub fn draw(&self, ctx: &egui::Context, palette: Palette) {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("toast"),
        ));
        let screen = ctx.screen_rect();
        let galley = ctx
            .fonts(|f| f.layout_no_wrap(self.message.clone(), theme::semibold(12.5), palette.text));
        let size = galley.size() + Vec2::new(28.0, 16.0);
        let rect =
            Rect::from_center_size(Pos2::new(screen.center().x, screen.bottom() - 96.0), size);
        painter.add(Shape::Rect(egui::epaint::RectShape::new(
            rect,
            egui::Rounding::same(10.0),
            palette.toast_bg,
            Stroke::new(1.0, palette.border_sel.gamma_multiply(0.6)),
        )));
        painter.galley(rect.center() - galley.size() / 2.0, galley, palette.text);
    }
}

/// An icon button (used for card actions and the floating toolbar).
pub fn icon_button(
    ui: &mut egui::Ui,
    rect: Rect,
    color: Color32,
    draw: fn(&icons::IconCtx),
    id_seed: &str,
    tooltip: &str,
) -> bool {
    let painter = ui.painter();
    let resp = ui
        .interact(rect, egui::Id::new(id_seed), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip);
    let hover = resp.hovered();
    let active = resp.is_pointer_button_down_on();
    let effective = if active {
        color.gamma_multiply(0.7)
    } else if hover {
        color.gamma_multiply(1.25)
    } else {
        color
    };
    icons::paint_with(painter, rect, effective, draw, icons::ICON_STROKE);
    resp.clicked()
}

/// Section header like "APPEARANCE" (mono, letter-spaced, faint).
pub fn section_header(ui: &mut egui::Ui, palette: Palette, text: &str) {
    ui.add_space(6.0);
    label_at(
        ui.painter(),
        egui::Align2::LEFT_TOP,
        ui.cursor().left_top() + Vec2::new(2.0, 2.0),
        &theme::spaced(text),
        theme::mono(10.5),
        palette.text_faint,
    );
    ui.add_space(22.0);
}

/// Divider line.
pub fn divider(ui: &mut egui::Ui, palette: Palette) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 1.0), Sense::hover());
    ui.painter().add(Shape::line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0, palette.border),
    ));
    ui.add_space(8.0);
}

/// Circular floating button (the big green "+" on configs).
pub fn round_button(
    ui: &mut egui::Ui,
    rect: Rect,
    bg: Color32,
    fg: Color32,
    draw: fn(&icons::IconCtx),
    id_seed: &str,
) -> bool {
    let painter = ui.painter();
    let resp = ui
        .interact(rect, egui::Id::new(id_seed), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    painter.circle_filled(rect.center(), rect.width() / 2.0, bg);
    icons::paint_with(painter, rect.shrink(rect.width() * 0.26), fg, draw, 2.4);
    resp.clicked()
}

/// Arc helper used by dials (kept for future gauges).
#[allow(dead_code)]
pub fn arc_points(center: Pos2, r: f32, a0_deg: f32, a1_deg: f32, n: usize) -> Vec<Pos2> {
    (0..=n)
        .map(|i| {
            let t = a0_deg + (a1_deg - a0_deg) * (i as f32 / n as f32);
            let rad = t.to_radians();
            Pos2::new(center.x + r * rad.cos(), center.y + r * rad.sin())
        })
        .collect()
}

/// Path stroke helper used by icons and custom controls.
#[allow(dead_code)]
pub fn stroke_path(painter: &Painter, pts: Vec<Pos2>, stroke: Stroke, closed: bool) {
    if pts.len() < 2 {
        return;
    }
    painter.add(Shape::Path(PathShape {
        points: pts,
        closed,
        fill: Color32::TRANSPARENT,
        stroke: PathStroke::from(stroke),
    }));
}

//! Reusable UI pieces: the power button, hand-drawn vector icons, cards,
//! badges, the sidebar nav item, modal scaffolding and number formatters.

use eframe::egui::{
    Area, Color32, Context, CornerRadius, CursorIcon, Frame, Id, Margin, Order, Painter, Pos2,
    Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::theme::{self, Palette};

// ── formatters ──────────────────────────────────────────────────────────────

/// Human-readable byte count, one decimal (like "46.7 MB").
pub fn fmt_bytes(bytes: u64) -> String {
    let b = bytes as f64;
    if b < 1024.0 {
        format!("{bytes} B")
    } else if b < 1024.0 * 1024.0 {
        format!("{:.1} KB", b / 1024.0)
    } else if b < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", b / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", b / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Human-readable byte rate ("1.2 MB/s", "340 KB/s").
pub fn fmt_rate(bps: f64) -> String {
    if bps < 1024.0 {
        format!("{bps:.0} B/s")
    } else if bps < 1024.0 * 1024.0 {
        format!("{:.1} KB/s", bps / 1024.0)
    } else if bps < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB/s", bps / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB/s", bps / (1024.0 * 1024.0 * 1024.0))
    }
}

/// "up 1h 05m" style uptime from a start unix timestamp.
pub fn fmt_uptime(started_unix: u64) -> String {
    let secs = npvt_core::profile::unix_now().saturating_sub(started_unix);
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("up {h}h {m:02}m")
    } else if m > 0 {
        format!("up {m}m {s:02}s")
    } else {
        format!("up {s}s")
    }
}

// ── icons (hand-drawn 12pt vector glyphs) ───────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Home,
    Configs,
    Logs,
    More,
}

/// Draw an icon glyph centered in `rect`.
pub fn draw_icon(p: &Painter, icon: Icon, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height()) * 0.5;
    let stroke = Stroke::new(1.6, color);
    match icon {
        Icon::Home => {
            // roof + body
            let top = Pos2::new(c.x, c.y - s * 0.9);
            let left = Pos2::new(c.x - s, c.y - s * 0.1);
            let right = Pos2::new(c.x + s, c.y - s * 0.1);
            p.line(vec![left, top, right], stroke);
            p.line(
                vec![
                    Pos2::new(c.x - s * 0.62, c.y - s * 0.35),
                    Pos2::new(c.x - s * 0.62, c.y + s * 0.85),
                    Pos2::new(c.x + s * 0.62, c.y + s * 0.85),
                    Pos2::new(c.x + s * 0.62, c.y - s * 0.35),
                ],
                stroke,
            );
        }
        Icon::Configs => {
            // stacked server cards
            for i in 0..3 {
                let y = c.y - s * 0.62 + i as f32 * s * 0.62;
                let r = Rect::from_min_size(
                    Pos2::new(c.x - s, y - s * 0.24),
                    Vec2::new(s * 2.0, s * 0.48),
                );
                p.rect_stroke(r, 3.0, stroke, StrokeKind::Middle);
                p.circle_filled(Pos2::new(c.x - s * 0.62, y), 1.3, color);
            }
        }
        Icon::Logs => {
            for i in 0..3 {
                let y = c.y - s * 0.6 + i as f32 * s * 0.6;
                let w = if i == 2 { s * 1.2 } else { s * 1.9 };
                p.line(
                    vec![
                        Pos2::new(c.x - s * 0.95, y),
                        Pos2::new(c.x - s * 0.95 + w, y),
                    ],
                    stroke,
                );
            }
        }
        Icon::More => {
            // 2x2 dots
            for dx in [-0.45f32, 0.45] {
                for dy in [-0.45f32, 0.45] {
                    p.circle_filled(Pos2::new(c.x + dx * s, c.y + dy * s), s * 0.34, color);
                }
            }
        }
    }
}

// ── power button ────────────────────────────────────────────────────────────

/// Visual state of the big power button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerVisual {
    Off,
    On,
    Busy,
}

/// The signature neon power button.
pub fn power_button(ui: &mut Ui, diameter: f32, visual: PowerVisual, enabled: bool) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::splat(diameter), Sense::click());
    if !enabled {
        response = response.interact(Sense::hover());
    }
    let p = ui.painter_at(rect.expand(34.0));
    let c = rect.center();
    let r = diameter * 0.5;
    let t = ui.input(|i| i.time) as f32;
    let hovered = response.hovered() && enabled;

    let (ring_color, fill_color, glyph_color) = match (visual, hovered) {
        (PowerVisual::On, _) => {
            let bright = Color32::from_rgb(46, 238, 116);
            (
                bright,
                Color32::from_rgba_unmultiplied(34, 221, 102, 26),
                bright,
            )
        }
        (PowerVisual::Busy, _) => (
            Color32::from_rgb(255, 180, 84),
            Color32::from_rgba_unmultiplied(255, 180, 84, 18),
            Color32::from_rgb(255, 180, 84),
        ),
        (PowerVisual::Off, true) => (
            Color32::from_rgb(120, 190, 140),
            Color32::from_rgba_unmultiplied(120, 190, 140, 16),
            Color32::from_rgb(140, 210, 160),
        ),
        (PowerVisual::Off, false) => (
            Color32::from_rgb(64, 82, 70),
            Color32::from_rgba_unmultiplied(64, 82, 70, 12),
            Color32::from_rgb(96, 118, 104),
        ),
    };

    // Outer glow (layered translucent rings).
    if visual != PowerVisual::Off {
        let breathe = ((t * 1.7).sin() * 0.5 + 0.5) * 6.0;
        for (i, alpha) in [10u8, 18, 28, 44].iter().enumerate() {
            let glow_r = r + 4.0 + (3 - i) as f32 * 7.0 + breathe;
            p.circle_filled(
                c,
                glow_r,
                Color32::from_rgba_unmultiplied(
                    ring_color.r(),
                    ring_color.g(),
                    ring_color.b(),
                    *alpha,
                ),
            );
        }
    }

    // Button body.
    p.circle_filled(c, r, fill_color);
    p.circle_stroke(c, r - 2.0, Stroke::new(2.0, ring_color));

    // Rotating progress arc while busy.
    if visual == PowerVisual::Busy {
        let phase = (t * 2.2) % 1.0 * std::f32::consts::TAU;
        let mut pts = Vec::new();
        for i in 0..=26 {
            let a = phase + (i as f32 / 26.0) * std::f32::consts::TAU * 0.32;
            pts.push(Pos2::new(
                c.x + a.cos() * (r + 10.0),
                c.y + a.sin() * (r + 10.0),
            ));
        }
        p.line(pts, Stroke::new(3.0, ring_color));
    }

    // Power glyph: open circle + vertical bar.
    let gr = r * 0.52;
    let gw = (r * 0.11).max(3.0);
    let gap = 76.0f32.to_radians();
    let start = std::f32::consts::FRAC_PI_2 + gap / 2.0;
    let end = std::f32::consts::FRAC_PI_2 - gap / 2.0 + std::f32::consts::TAU;
    let mut pts = Vec::new();
    let steps = 40;
    for i in 0..=steps {
        let a = start + (end - start) * (i as f32 / steps as f32);
        pts.push(Pos2::new(c.x + a.cos() * gr, c.y + a.sin() * gr));
    }
    p.line(pts, Stroke::new(gw, glyph_color));
    let bar_half = gw * 0.5;
    let bar_top = c.y - gr * 1.12;
    let bar_bottom = c.y - gr * 0.12;
    p.rect_filled(
        Rect::from_min_max(
            Pos2::new(c.x - bar_half, bar_top),
            Pos2::new(c.x + bar_half, bar_bottom),
        ),
        bar_half,
        glyph_color,
    );

    if enabled {
        response = response.on_hover_cursor(CursorIcon::PointingHand);
    }
    response
}

// ── cards & badges ──────────────────────────────────────────────────────────

/// A rounded content card.
pub fn card<R>(
    ui: &mut Ui,
    p: &Palette,
    add: impl FnOnce(&mut Ui) -> R,
) -> eframe::egui::InnerResponse<R> {
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.card_stroke))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(14))
        .show(ui, add)
}

/// A rounded content card with a highlighted accent edge (selected state).
pub fn card_active<R>(
    ui: &mut Ui,
    p: &Palette,
    add: impl FnOnce(&mut Ui) -> R,
) -> eframe::egui::InnerResponse<R> {
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.accent.gamma_multiply(0.7)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(14))
        .show(ui, add)
}

/// Card that highlights when `active`, and returns the outer response for
/// interaction detection.
pub fn card_active_or(
    ui: &mut Ui,
    p: &Palette,
    active: bool,
    add: impl FnOnce(&mut Ui),
) -> Response {
    let frame = if active {
        Frame::new()
            .fill(p.card)
            .stroke(Stroke::new(1.0, p.accent.gamma_multiply(0.7)))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::same(14))
    } else {
        Frame::new()
            .fill(p.card)
            .stroke(Stroke::new(1.0, p.card_stroke))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::same(14))
    };
    frame
        .show(ui, |ui| {
            ui.vertical(|ui| add(ui));
        })
        .response
}

/// Small pill badge with tinted background.
pub fn badge(ui: &mut Ui, _p: &Palette, color: Color32, text: &str) -> Response {
    let tint = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 30);
    Frame::new()
        .fill(tint)
        .corner_radius(CornerRadius::same(u8::MAX))
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.set_min_size(Vec2::ZERO);
            ui.label(theme::semi(text).size(10.5).color(color));
        })
        .response
        .on_hover_cursor(CursorIcon::Default)
}

/// A quiet secondary button (muted card fill).
pub fn ghost_button(ui: &mut Ui, p: &Palette, text: &str) -> Response {
    ui.add(
        eframe::egui::Button::new(theme::semi(text).size(12.0).color(p.text))
            .fill(p.card)
            .stroke(Stroke::new(1.0, p.card_stroke))
            .corner_radius(CornerRadius::same(8))
            .min_size(Vec2::new(0.0, 30.0)),
    )
}

/// Primary accent button (green, dark text).
pub fn primary_button(ui: &mut Ui, p: &Palette, text: &str) -> Response {
    ui.add(
        eframe::egui::Button::new(theme::semi(text).size(12.5).color(p.on_accent))
            .fill(p.accent)
            .corner_radius(CornerRadius::same(8))
            .min_size(Vec2::new(0.0, 30.0)),
    )
}

/// Danger button (soft red).
pub fn danger_button(ui: &mut Ui, p: &Palette, text: &str) -> Response {
    ui.add(
        eframe::egui::Button::new(theme::semi(text).size(12.0).color(p.danger))
            .fill(p.danger_soft)
            .stroke(Stroke::new(1.0, p.danger.gamma_multiply(0.6)))
            .corner_radius(CornerRadius::same(8))
            .min_size(Vec2::new(0.0, 30.0)),
    )
}

// ── modal scaffolding ───────────────────────────────────────────────────────

/// Show a centered modal with a dimmed backdrop. Returns the inner response
/// so the caller can detect interactions; clicks on the backdrop are
/// swallowed.
pub fn modal<R>(
    ctx: &Context,
    id_salt: &str,
    width: f32,
    p: &Palette,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<eframe::egui::InnerResponse<R>> {
    // Dim backdrop that also swallows outside clicks.
    let screen = ctx.viewport_rect();
    let backdrop = Area::new(Id::unique(format!("{id_salt}-backdrop")))
        .order(Order::Foreground)
        .fixed_pos(screen.left_top())
        .interactable(true)
        .show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_black_alpha(150));
            response
        });
    if backdrop.inner.clicked() {
        return None;
    }

    let area = Area::new(Id::unique(format!("{id_salt}-panel")))
        .order(Order::Foreground)
        .anchor(eframe::egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            Frame::new()
                .fill(p.panel)
                .stroke(Stroke::new(1.0, p.accent.gamma_multiply(0.5)))
                .corner_radius(CornerRadius::same(14))
                .outer_margin(Margin::same(0))
                .inner_margin(Margin::same(18))
                .show(ui, |ui| {
                    ui.set_min_width(width);
                    ui.set_max_width(width);
                    add(ui)
                })
        });
    Some(area.inner)
}

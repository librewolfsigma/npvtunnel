//! Vector icons drawn with `egui` painters (no emoji, no icon fonts - every
//! glyph below is strokes and polygons, so nothing can ever render as a
//! missing-glyph box) plus the generated window/taskbar icon.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // full icon set ships even when a page does not use every glyph yet
pub enum Icon {
    Power,
    Home,
    Cloud,
    Info,
    Dots,
    Kebab,
    ChevronRight,
    ChevronDown,
    ChevronLeft,
    Expand,
    Close,
    Refresh,
    Search,
    Sliders,
    Plus,
    Speed,
    Pencil,
    Trash,
    Pin,
    Download,
    Upload,
    Clipboard,
    Qr,
    Rss,
    Cluster,
    Key,
    Gear,
    Grid,
    Star,
    Folder,
    Sun,
    Moon,
    ArrowDown,
    ArrowUp,
    Copy,
    Check,
    Globe,
    Server,
}

/// Approximate an arc with a polyline (epaint 0.29 has no arc primitive).
fn arc_to(p: &Painter, center: Pos2, radius: f32, start: f32, end: f32, stroke: Stroke) {
    let steps = 40;
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let a = start + (end - start) * i as f32 / steps as f32;
            Pos2::new(center.x + radius * a.cos(), center.y + radius * a.sin())
        })
        .collect();
    p.add(Shape::line(points, stroke));
}

fn c(rect: Rect, fx: f32, fy: f32) -> Pos2 {
    Pos2::new(
        rect.left() + rect.width() * fx,
        rect.top() + rect.height() * fy,
    )
}

fn r(rect: Rect, fx0: f32, fy0: f32, fx1: f32, fy1: f32) -> Rect {
    Rect::from_min_max(c(rect, fx0, fy0), c(rect, fx1, fy1))
}

/// Draw `icon` inside `rect` with the given color and stroke weight.
pub fn paint(p: &Painter, icon: Icon, rect: Rect, color: Color32, weight: f32) {
    let st = Stroke::new(weight, color);
    let _fill_color = color;
    match icon {
        Icon::Power => {
            // Ring with a gap at the top + the vertical stem.
            let center = rect.center();
            let radius = rect.width() * 0.36;
            let gap = 0.62_f32; // radians of missing arc around the top
            let start = -std::f32::consts::FRAC_PI_2 + gap / 2.0;
            let end = -std::f32::consts::FRAC_PI_2 - gap / 2.0 + std::f32::consts::TAU;
            arc_to(p, center, radius, start, end, st);
            let stem_top = c(rect, 0.5, 0.13);
            let stem_bot = c(rect, 0.5, 0.47);
            p.line_segment([stem_top, stem_bot], st);
        }
        Icon::Home => {
            let apex = c(rect, 0.5, 0.16);
            let l = c(rect, 0.14, 0.52);
            let rr = c(rect, 0.86, 0.52);
            p.line_segment([l, apex], st);
            p.line_segment([apex, rr], st);
            p.line_segment([l, rr], st);
            let body = r(rect, 0.24, 0.52, 0.76, 0.86);
            p.add(Shape::Rect(egui::epaint::RectShape::new(
                body,
                2.0,
                Color32::TRANSPARENT,
                st,
            )));
            let door = r(rect, 0.42, 0.62, 0.58, 0.86);
            p.rect_filled(door, 1.0, color);
        }
        Icon::Cloud => {
            let a = r(rect, 0.18, 0.42, 0.50, 0.74);
            let b = r(rect, 0.36, 0.24, 0.74, 0.62);
            let c2 = r(rect, 0.52, 0.40, 0.86, 0.74);
            p.circle_filled(a.center(), a.width() / 2.0, color);
            p.circle_filled(b.center(), b.width() / 2.0, color);
            p.circle_filled(c2.center(), c2.width() / 2.0, color);
            let base = r(rect, 0.18, 0.56, 0.86, 0.74);
            p.rect_filled(base, 0.0, color);
        }
        Icon::Info => {
            let ring = r(rect, 0.10, 0.10, 0.90, 0.90);
            p.circle_stroke(ring.center(), ring.width() / 2.0, st);
            let dot = r(rect, 0.46, 0.24, 0.54, 0.32);
            p.circle_filled(dot.center(), dot.width() / 2.0, color);
            p.line_segment(
                [c(rect, 0.5, 0.44), c(rect, 0.5, 0.72)],
                Stroke::new(weight * 1.2, color),
            );
        }
        Icon::Dots => {
            for fx in [0.22, 0.5, 0.78] {
                p.circle_filled(c(rect, fx, 0.5), weight * 1.35, color);
            }
        }
        Icon::Kebab => {
            for fy in [0.22, 0.5, 0.78] {
                p.circle_filled(c(rect, 0.5, fy), weight * 1.35, color);
            }
        }
        Icon::ChevronRight => {
            p.line_segment([c(rect, 0.32, 0.2), c(rect, 0.68, 0.5)], st);
            p.line_segment([c(rect, 0.68, 0.5), c(rect, 0.32, 0.8)], st);
        }
        Icon::ChevronLeft => {
            p.line_segment([c(rect, 0.68, 0.2), c(rect, 0.32, 0.5)], st);
            p.line_segment([c(rect, 0.32, 0.5), c(rect, 0.68, 0.8)], st);
        }
        Icon::ChevronDown => {
            p.line_segment([c(rect, 0.2, 0.34), c(rect, 0.5, 0.68)], st);
            p.line_segment([c(rect, 0.5, 0.68), c(rect, 0.8, 0.34)], st);
        }
        Icon::Expand => {
            p.line_segment([c(rect, 0.3, 0.2), c(rect, 0.55, 0.5)], st);
            p.line_segment([c(rect, 0.55, 0.5), c(rect, 0.3, 0.8)], st);
            p.line_segment([c(rect, 0.62, 0.34), c(rect, 0.78, 0.5)], st);
            p.line_segment([c(rect, 0.78, 0.5), c(rect, 0.62, 0.66)], st);
        }
        Icon::Close => {
            p.line_segment([c(rect, 0.24, 0.24), c(rect, 0.76, 0.76)], st);
            p.line_segment([c(rect, 0.76, 0.24), c(rect, 0.24, 0.76)], st);
        }
        Icon::Refresh => {
            let center = rect.center();
            let radius = rect.width() * 0.32;
            let start = 0.4_f32;
            let end = start + std::f32::consts::TAU - 1.1;
            arc_to(p, center, radius, start, end, st);
            let tip = Pos2::new(center.x + radius * end.cos(), center.y + radius * end.sin());
            let dir = Vec2::new(end.cos(), end.sin());
            let n = Vec2::new(-dir.y, dir.x);
            let a = tip + dir * weight * 3.0 - n * weight * 3.4;
            let b = tip + dir * weight * 3.0 + n * weight * 3.4;
            p.line_segment([a, tip], st);
            p.line_segment([b, tip], st);
        }
        Icon::Search => {
            let lens = r(rect, 0.16, 0.16, 0.62, 0.62);
            p.circle_stroke(lens.center(), lens.width() / 2.0, st);
            p.line_segment([c(rect, 0.60, 0.60), c(rect, 0.84, 0.84)], st);
        }
        Icon::Sliders => {
            for (i, fy) in [0.28, 0.5, 0.72].iter().enumerate() {
                let y = rect.top() + rect.height() * fy;
                p.line_segment(
                    [c(rect, 0.14, *fy), c(rect, 0.86, *fy)],
                    Stroke::new(weight * 0.8, color),
                );
                let fx = match i {
                    0 => 0.62,
                    1 => 0.34,
                    _ => 0.52,
                };
                let knob = Pos2::new(rect.left() + rect.width() * fx, y);
                p.circle_filled(knob, weight * 1.9, color);
            }
        }
        Icon::Plus => {
            p.line_segment([c(rect, 0.5, 0.18), c(rect, 0.5, 0.82)], st);
            p.line_segment([c(rect, 0.18, 0.5), c(rect, 0.82, 0.5)], st);
        }
        Icon::Speed => {
            let center = c(rect, 0.5, 0.62);
            let radius = rect.width() * 0.36;
            let start = std::f32::consts::PI * 1.05;
            let end = std::f32::consts::PI * 1.95;
            arc_to(p, center, radius, start, end, st);
            let needle = Pos2::new(
                center.x + radius * 0.72 * 0.5_f32.cos(),
                center.y + radius * 0.72 * 0.5_f32.sin(),
            );
            p.line_segment([center, needle], st);
        }
        Icon::Pencil => {
            let body0 = c(rect, 0.24, 0.76);
            let body1 = c(rect, 0.66, 0.34);
            let tip = c(rect, 0.20, 0.80);
            let w = Vec2::new(0.06 * rect.width(), -0.06 * rect.height());
            p.line_segment([body0, body1 + w], Stroke::new(weight * 1.3, color));
            p.line_segment(
                [body0 + w, body1 + w * 2.0],
                Stroke::new(weight * 1.3, color),
            );
            p.line_segment([body1 + w * 2.0, body1], Stroke::new(weight * 1.3, color));
            p.line_segment([tip, body0], Stroke::new(weight * 1.3, color));
        }
        Icon::Trash => {
            let lid = r(rect, 0.24, 0.18, 0.76, 0.24);
            p.rect_filled(lid, 2.0, color);
            let bin = r(rect, 0.30, 0.30, 0.70, 0.82);
            p.add(Shape::Rect(egui::epaint::RectShape::new(
                bin,
                3.0,
                Color32::TRANSPARENT,
                st,
            )));
            p.line_segment([c(rect, 0.44, 0.42), c(rect, 0.44, 0.70)], st);
            p.line_segment([c(rect, 0.56, 0.42), c(rect, 0.56, 0.70)], st);
            p.line_segment([c(rect, 0.42, 0.18), c(rect, 0.42, 0.12)], st);
            p.line_segment([c(rect, 0.42, 0.12), c(rect, 0.58, 0.12)], st);
            p.line_segment([c(rect, 0.58, 0.12), c(rect, 0.58, 0.18)], st);
        }
        Icon::Pin => {
            let head = r(rect, 0.36, 0.14, 0.64, 0.46);
            p.add(Shape::Rect(egui::epaint::RectShape::new(
                head,
                4.0,
                Color32::TRANSPARENT,
                st,
            )));
            p.line_segment([c(rect, 0.5, 0.46), c(rect, 0.5, 0.62)], st);
            p.line_segment([c(rect, 0.36, 0.66), c(rect, 0.64, 0.66)], st);
        }
        Icon::Download => {
            p.line_segment([c(rect, 0.5, 0.16), c(rect, 0.5, 0.62)], st);
            p.line_segment([c(rect, 0.32, 0.46), c(rect, 0.5, 0.64)], st);
            p.line_segment([c(rect, 0.5, 0.64), c(rect, 0.68, 0.46)], st);
            p.line_segment([c(rect, 0.2, 0.78), c(rect, 0.8, 0.78)], st);
        }
        Icon::Upload => {
            p.line_segment([c(rect, 0.5, 0.64), c(rect, 0.5, 0.18)], st);
            p.line_segment([c(rect, 0.32, 0.36), c(rect, 0.5, 0.18)], st);
            p.line_segment([c(rect, 0.5, 0.18), c(rect, 0.68, 0.36)], st);
            p.line_segment([c(rect, 0.2, 0.8), c(rect, 0.8, 0.8)], st);
        }
        Icon::Clipboard => {
            let board = r(rect, 0.24, 0.18, 0.76, 0.86);
            p.add(Shape::Rect(egui::epaint::RectShape::new(
                board,
                4.0,
                Color32::TRANSPARENT,
                st,
            )));
            let clip = r(rect, 0.4, 0.10, 0.6, 0.24);
            p.rect_filled(clip, 2.0, color);
            p.line_segment([c(rect, 0.36, 0.46), c(rect, 0.64, 0.46)], st);
            p.line_segment([c(rect, 0.36, 0.6), c(rect, 0.64, 0.6)], st);
        }
        Icon::Qr => {
            let cells = [
                (0.16, 0.16),
                (0.30, 0.16),
                (0.16, 0.30),
                (0.44, 0.16),
                (0.72, 0.16),
                (0.86, 0.30),
                (0.16, 0.44),
                (0.16, 0.72),
                (0.30, 0.86),
                (0.44, 0.58),
                (0.58, 0.72),
                (0.72, 0.58),
                (0.86, 0.72),
                (0.58, 0.86),
            ];
            for (fx, fy) in cells {
                let d = rect.width() * 0.12;
                let center = c(rect, fx, fy);
                p.rect_filled(Rect::from_center_size(center, Vec2::splat(d)), 1.0, color);
            }
        }
        Icon::Rss => {
            let center = rect.center();
            let radius = rect.width() * 0.36;
            let start = -std::f32::consts::FRAC_PI_2 + 0.55;
            let end = -std::f32::consts::FRAC_PI_2 - 0.55 + std::f32::consts::TAU;
            arc_to(p, center, radius, start, end, st);
            arc_to(p, center, radius * 0.55, start + 0.2, end - 0.2, st);
            p.circle_filled(c(rect, 0.36, 0.68), weight * 1.6, color);
        }
        Icon::Cluster => {
            let main = c(rect, 0.44, 0.52);
            p.circle_filled(main, rect.width() * 0.16, color);
            for (fx, fy) in [(0.68, 0.3), (0.72, 0.62)] {
                p.circle_filled(c(rect, fx, fy), rect.width() * 0.11, color);
            }
        }
        Icon::Key => {
            let ring = r(rect, 0.16, 0.36, 0.5, 0.7);
            p.circle_stroke(ring.center(), ring.width() / 2.0, st);
            p.line_segment([c(rect, 0.48, 0.52), c(rect, 0.84, 0.52)], st);
            p.line_segment([c(rect, 0.72, 0.52), c(rect, 0.72, 0.66)], st);
            p.line_segment([c(rect, 0.84, 0.52), c(rect, 0.84, 0.66)], st);
        }
        Icon::Gear => {
            let center = rect.center();
            let radius = rect.width() * 0.28;
            p.circle_stroke(center, radius, st);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0 + 0.3;
                let inner = Pos2::new(center.x + radius * a.cos(), center.y + radius * a.sin());
                let outer = Pos2::new(
                    center.x + radius * 1.55 * a.cos(),
                    center.y + radius * 1.55 * a.sin(),
                );
                p.line_segment([inner, outer], Stroke::new(weight * 1.2, color));
            }
        }
        Icon::Grid => {
            for (fx, fy) in [
                (0.2, 0.2),
                (0.5, 0.2),
                (0.8, 0.2),
                (0.2, 0.5),
                (0.5, 0.5),
                (0.8, 0.5),
                (0.2, 0.8),
                (0.5, 0.8),
                (0.8, 0.8),
            ] {
                let d = rect.width() * 0.14;
                let center = c(rect, fx, fy);
                p.rect_filled(Rect::from_center_size(center, Vec2::splat(d)), 1.5, color);
            }
        }
        Icon::Star => {
            let center = rect.center();
            let outer = rect.width() * 0.4;
            let inner = outer * 0.42;
            let mut pts = Vec::with_capacity(10);
            for i in 0..10 {
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / 10.0;
                let rad = if i % 2 == 0 { outer } else { inner };
                pts.push(Pos2::new(
                    center.x + rad * a.cos(),
                    center.y + rad * a.sin(),
                ));
            }
            p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        Icon::Folder => {
            let body = r(rect, 0.14, 0.3, 0.86, 0.8);
            p.add(Shape::Rect(egui::epaint::RectShape::new(
                body,
                4.0,
                Color32::TRANSPARENT,
                st,
            )));
            let tab = r(rect, 0.14, 0.2, 0.44, 0.32);
            p.add(Shape::Rect(egui::epaint::RectShape::new(
                tab,
                3.0,
                Color32::TRANSPARENT,
                st,
            )));
        }
        Icon::Sun => {
            let center = rect.center();
            let radius = rect.width() * 0.24;
            p.circle_stroke(center, radius, st);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let from = Pos2::new(
                    center.x + radius * 1.5 * a.cos(),
                    center.y + radius * 1.5 * a.sin(),
                );
                let to = Pos2::new(
                    center.x + radius * 2.1 * a.cos(),
                    center.y + radius * 2.1 * a.sin(),
                );
                p.line_segment([from, to], st);
            }
        }
        Icon::Moon => {
            let center = c(rect, 0.5, 0.5);
            let radius = rect.width() * 0.34;
            arc_to(
                p,
                center,
                radius,
                0.9_f32,
                0.9 + std::f32::consts::TAU - 2.4,
                st,
            );
            p.line_segment(
                [c(rect, 0.34, 0.24), c(rect, 0.34, 0.76)],
                Stroke::new(weight * 1.2, color),
            );
        }
        Icon::ArrowDown => {
            p.line_segment([c(rect, 0.34, 0.3), c(rect, 0.5, 0.5)], st);
            p.line_segment([c(rect, 0.5, 0.5), c(rect, 0.66, 0.3)], st);
        }
        Icon::ArrowUp => {
            p.line_segment([c(rect, 0.34, 0.5), c(rect, 0.5, 0.3)], st);
            p.line_segment([c(rect, 0.5, 0.3), c(rect, 0.66, 0.5)], st);
        }
        Icon::Copy => {
            let back = r(rect, 0.3, 0.14, 0.82, 0.66);
            p.add(Shape::Rect(egui::epaint::RectShape::new(
                back,
                3.0,
                Color32::TRANSPARENT,
                st,
            )));
            let front = r(rect, 0.18, 0.34, 0.7, 0.86);
            p.add(Shape::Rect(egui::epaint::RectShape::new(
                front,
                3.0,
                Color32::TRANSPARENT,
                st,
            )));
        }
        Icon::Check => {
            p.line_segment([c(rect, 0.22, 0.52), c(rect, 0.44, 0.74)], st);
            p.line_segment([c(rect, 0.44, 0.74), c(rect, 0.8, 0.28)], st);
        }
        Icon::Globe => {
            let ring = r(rect, 0.1, 0.1, 0.9, 0.9);
            p.circle_stroke(ring.center(), ring.width() / 2.0, st);
            p.line_segment([c(rect, 0.1, 0.5), c(rect, 0.9, 0.5)], st);
            arc_to(p, ring.center(), ring.width() / 2.0, -1.2_f32, 1.2, st);
            arc_to(
                p,
                ring.center(),
                ring.width() / 2.0,
                std::f32::consts::PI - 1.2,
                std::f32::consts::PI + 1.2,
                st,
            );
        }
        Icon::Server => {
            let top = r(rect, 0.16, 0.2, 0.84, 0.46);
            let bottom = r(rect, 0.16, 0.54, 0.84, 0.8);
            for b in [top, bottom] {
                p.add(Shape::Rect(egui::epaint::RectShape::new(
                    b,
                    3.0,
                    Color32::TRANSPARENT,
                    st,
                )));
                p.circle_filled(c(b, 0.26, 0.5), weight * 1.2, color);
            }
        }
    }
}

/// A small square button that paints `icon` and reports clicks.
pub fn icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    size: f32,
    color: Color32,
    hover_bg: Option<Color32>,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        let bg = hover_bg.unwrap_or(Color32::from_rgba_unmultiplied(120, 140, 130, 36));
        ui.painter().rect_filled(rect, 6.0, bg);
    }
    paint(
        ui.painter(),
        icon,
        rect.shrink(size * 0.22),
        color,
        size * 0.075,
    );
    resp
}

/// Render the 64x64 window/taskbar icon: dark tile, green ring, power glyph.
pub fn app_icon() -> egui::IconData {
    let s = 64usize;
    let mut rgba = vec![0u8; s * s * 4];
    let center = (s as f32 - 1.0) / 2.0;
    let radius = 21.0_f32;
    for y in 0..s {
        for x in 0..s {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = (y * s + x) * 4;
            // Dark rounded-square background.
            let in_tile = x.abs_diff(32) < 30 && y.abs_diff(32) < 30;
            if in_tile {
                rgba[idx] = 7;
                rgba[idx + 1] = 11;
                rgba[idx + 2] = 10;
                rgba[idx + 3] = 255;
            }
            // Ring (with the top gap) in the accent green.
            let angle = dy.atan2(dx);
            let top_gap = (angle + std::f32::consts::FRAC_PI_2).rem_euclid(std::f32::consts::TAU);
            let in_gap = top_gap > std::f32::consts::TAU - 0.45 || top_gap < 0.45;
            let ring_w = 3.4;
            if (dist - radius).abs() < ring_w && !in_gap {
                rgba[idx] = 63;
                rgba[idx + 1] = 217;
                rgba[idx + 2] = 127;
                rgba[idx + 3] = 255;
            }
            // Power stem.
            if dx.abs() < 3.0 && dy < -6.0 && dy > -radius - 2.0 {
                rgba[idx] = 63;
                rgba[idx + 1] = 217;
                rgba[idx + 2] = 127;
                rgba[idx + 3] = 255;
            }
        }
    }
    egui::IconData {
        width: s as u32,
        height: s as u32,
        rgba,
    }
}

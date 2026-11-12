//! Hand-drawn vector icons (epaint primitives) matching the reference
//! design's outline style. Every icon is drawn inside the given rect with a
//! uniform stroke so the set stays visually consistent.

use eframe::egui::{
    epaint::{PathShape, PathStroke},
    Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2,
};

pub const ICON_STROKE: f32 = 1.7;

/// Function-pointer shape used across widget/page tables.
pub type IconFn = fn(&IconCtx);

fn seg(painter: &Painter, pts: Vec<Pos2>, stroke: Stroke, closed: bool) {
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

fn line(painter: &Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    painter.add(Shape::line_segment([a, b], stroke));
}

fn circle(painter: &Painter, c: Pos2, r: f32, stroke: Stroke) {
    painter.add(Shape::circle_stroke(c, r, stroke));
}

fn dot(painter: &Painter, c: Pos2, r: f32, color: Color32) {
    painter.circle_filled(c, r, color);
}

/// A tiny icon painter context: gives out an inset square to draw in.
pub struct IconCtx<'a> {
    pub painter: &'a Painter,
    pub rect: Rect,
    pub stroke: Stroke,
}

impl<'a> IconCtx<'a> {
    pub fn new(painter: &'a Painter, rect: Rect, color: Color32, width: f32) -> Self {
        Self {
            painter,
            rect,
            stroke: Stroke::new(width, color),
        }
    }

    fn p(&self, fx: f32, fy: f32) -> Pos2 {
        Pos2::new(
            self.rect.left() + self.rect.width() * fx,
            self.rect.top() + self.rect.height() * fy,
        )
    }

    pub fn color(&self) -> Color32 {
        self.stroke.color
    }
}

pub fn power(ctx: &IconCtx) {
    let r = ctx.rect.width() * 0.34;
    let c = ctx.rect.center();
    let a = (17.0f32).to_radians();
    // Ring with a gap at the top, like the reference power glyph.
    let pts: Vec<Pos2> = (0..=48)
        .map(|i| {
            let t = -90.0 + a + (360.0 - 2.0 * a) * (i as f32 / 48.0);
            let rad = t.to_radians();
            Pos2::new(c.x + r * rad.cos(), c.y + r * rad.sin())
        })
        .collect();
    seg(ctx.painter, pts, ctx.stroke, false);
    let w = ctx.rect.width() * 0.09;
    line(
        ctx.painter,
        Pos2::new(c.x, c.y - r - w * 0.35),
        Pos2::new(c.x, c.y - r * 0.28),
        Stroke::new(w, ctx.stroke.color),
    );
}

pub fn chevron_right(ctx: &IconCtx) {
    let w = ctx.rect.width() * 0.26;
    let c = ctx.rect.center();
    seg(
        ctx.painter,
        vec![
            Pos2::new(c.x - w * 0.4, c.y - w),
            Pos2::new(c.x + w * 0.45, c.y),
            Pos2::new(c.x - w * 0.4, c.y + w),
        ],
        ctx.stroke,
        false,
    );
}

pub fn chevron_down(ctx: &IconCtx) {
    let w = ctx.rect.width() * 0.26;
    let c = ctx.rect.center();
    seg(
        ctx.painter,
        vec![
            Pos2::new(c.x - w, c.y - w * 0.4),
            Pos2::new(c.x, c.y + w * 0.45),
            Pos2::new(c.x + w, c.y - w * 0.4),
        ],
        ctx.stroke,
        false,
    );
}

pub fn chevron_left(ctx: &IconCtx) {
    let w = ctx.rect.width() * 0.26;
    let c = ctx.rect.center();
    seg(
        ctx.painter,
        vec![
            Pos2::new(c.x + w * 0.4, c.y - w),
            Pos2::new(c.x - w * 0.45, c.y),
            Pos2::new(c.x + w * 0.4, c.y + w),
        ],
        ctx.stroke,
        false,
    );
}

/// The connection-group mark used by the "New connection group" row.
pub fn new_group_icon(ctx: &IconCtx) {
    cluster(ctx, ctx.stroke.color);
}

/// Gold medal with the color baked in (for the premium row).
pub fn medal_gold(ctx: &IconCtx) {
    let gold = Color32::from_rgb(0xFB, 0xBF, 0x24);
    medal(ctx, gold);
}

pub fn double_arrow_down(ctx: &IconCtx) {
    let w = ctx.rect.width() * 0.28;
    let c = ctx.rect.center();
    for dy in [-0.16f32, 0.14] {
        seg(
            ctx.painter,
            vec![
                Pos2::new(c.x - w, c.y + dy * ctx.rect.height() - w * 0.55),
                Pos2::new(c.x, c.y + dy * ctx.rect.height() + w * 0.45),
                Pos2::new(c.x + w, c.y + dy * ctx.rect.height() - w * 0.55),
            ],
            ctx.stroke,
            false,
        );
    }
}

pub fn double_arrow_up(ctx: &IconCtx) {
    let w = ctx.rect.width() * 0.28;
    let c = ctx.rect.center();
    for dy in [-0.14f32, 0.16] {
        seg(
            ctx.painter,
            vec![
                Pos2::new(c.x - w, c.y + dy * ctx.rect.height() + w * 0.55),
                Pos2::new(c.x, c.y + dy * ctx.rect.height() - w * 0.45),
                Pos2::new(c.x + w, c.y + dy * ctx.rect.height() + w * 0.55),
            ],
            ctx.stroke,
            false,
        );
    }
}

pub fn cluster(ctx: &IconCtx, color: Color32) {
    // Three filled dots: the connection-group mark from the design.
    let c = ctx.rect.center();
    let r = ctx.rect.width() * 0.16;
    dot(
        ctx.painter,
        Pos2::new(c.x - r * 1.35, c.y + r * 1.05),
        r,
        color,
    );
    dot(
        ctx.painter,
        Pos2::new(c.x + r * 1.35, c.y + r * 1.05),
        r,
        color,
    );
    dot(ctx.painter, Pos2::new(c.x, c.y - r * 0.95), r, color);
}

pub fn speed(ctx: &IconCtx) {
    let c = ctx.rect.center();
    let r = ctx.rect.width() * 0.36;
    let pts: Vec<Pos2> = (0..=24)
        .map(|i| {
            let t = 150.0 + 240.0 * (i as f32 / 24.0);
            let rad = t.to_radians();
            Pos2::new(c.x + r * rad.cos(), c.y + r * rad.sin())
        })
        .collect();
    seg(ctx.painter, pts, ctx.stroke, false);
    // Needle pointing up-right.
    seg(
        ctx.painter,
        vec![c, Pos2::new(c.x + r * 0.62, c.y - r * 0.62)],
        ctx.stroke,
        false,
    );
}

pub fn trash(ctx: &IconCtx) {
    let r = ctx.rect;
    let c = r.center();
    let w = r.width();
    line(
        ctx.painter,
        ctx.p(0.22, 0.24),
        ctx.p(0.78, 0.24),
        ctx.stroke,
    );
    line(
        ctx.painter,
        Pos2::new(c.x, r.top() + w * 0.12),
        Pos2::new(c.x, r.top() + w * 0.24),
        ctx.stroke,
    );
    // Body: two vertical sides + bottom.
    let l = ctx.p(0.3, 0.24);
    let rr = ctx.p(0.7, 0.24);
    let bl = ctx.p(0.34, 0.85);
    let br = ctx.p(0.66, 0.85);
    line(ctx.painter, l, bl, ctx.stroke);
    line(ctx.painter, rr, br, ctx.stroke);
    line(ctx.painter, bl, br, ctx.stroke);
    line(
        ctx.painter,
        ctx.p(0.44, 0.42),
        ctx.p(0.44, 0.68),
        ctx.stroke,
    );
    line(
        ctx.painter,
        ctx.p(0.56, 0.42),
        ctx.p(0.56, 0.68),
        ctx.stroke,
    );
}

pub fn pencil(ctx: &IconCtx) {
    let r = ctx.rect;
    let a = ctx.p(0.22, 0.78);
    let b = ctx.p(0.72, 0.28);
    let n = ctx.p(0.80, 0.20);
    let tip = ctx.p(0.17, 0.83);
    seg(ctx.painter, vec![a, b], ctx.stroke, false);
    // Pencil body (two parallel lines).
    let off = r.width() * 0.11;
    seg(
        ctx.painter,
        vec![
            Pos2::new(a.x - off * 0.4, a.y + off),
            Pos2::new(b.x - off, b.y + off * 0.35),
        ],
        ctx.stroke,
        false,
    );
    // Nib triangle.
    seg(
        ctx.painter,
        vec![tip, a, Pos2::new(a.x + off, a.y + off)],
        ctx.stroke,
        true,
    );
    line(ctx.painter, b, n, ctx.stroke);
}

pub fn pin(ctx: &IconCtx) {
    let c = ctx.rect.center();
    circle(
        ctx.painter,
        Pos2::new(c.x, c.y - ctx.rect.height() * 0.12),
        ctx.rect.width() * 0.22,
        ctx.stroke,
    );
    line(
        ctx.painter,
        Pos2::new(c.x, c.y + ctx.rect.height() * 0.08),
        Pos2::new(c.x, c.y + ctx.rect.height() * 0.34),
        ctx.stroke,
    );
}

pub fn refresh(ctx: &IconCtx) {
    let c = ctx.rect.center();
    let r = ctx.rect.width() * 0.33;
    let pts: Vec<Pos2> = (0..=40)
        .map(|i| {
            let t = -40.0 + 290.0 * (i as f32 / 40.0);
            let rad = t.to_radians();
            Pos2::new(c.x + r * rad.cos(), c.y + r * rad.sin())
        })
        .collect();
    let end = pts[0];
    seg(ctx.painter, pts, ctx.stroke, false);
    // Arrow head.
    let w = ctx.rect.width() * 0.12;
    seg(
        ctx.painter,
        vec![
            Pos2::new(end.x - w, end.y - w * 0.3),
            end,
            Pos2::new(end.x + w * 0.5, end.y + w),
        ],
        ctx.stroke,
        false,
    );
}

pub fn search(ctx: &IconCtx) {
    let c = Pos2::new(
        ctx.rect.left() + ctx.rect.width() * 0.42,
        ctx.rect.top() + ctx.rect.height() * 0.42,
    );
    let r = ctx.rect.width() * 0.26;
    circle(ctx.painter, c, r, ctx.stroke);
    line(
        ctx.painter,
        Pos2::new(c.x + r * 0.75, c.y + r * 0.75),
        ctx.p(0.84, 0.84),
        ctx.stroke,
    );
}

pub fn sliders(ctx: &IconCtx) {
    line(ctx.painter, ctx.p(0.18, 0.3), ctx.p(0.82, 0.3), ctx.stroke);
    line(ctx.painter, ctx.p(0.18, 0.7), ctx.p(0.82, 0.7), ctx.stroke);
    dot(ctx.painter, ctx.p(0.62, 0.3), 2.6, ctx.stroke.color);
    dot(ctx.painter, ctx.p(0.36, 0.7), 2.6, ctx.stroke.color);
}

pub fn plus(ctx: &IconCtx) {
    let c = ctx.rect.center();
    let w = ctx.rect.width() * 0.32;
    line(
        ctx.painter,
        Pos2::new(c.x - w, c.y),
        Pos2::new(c.x + w, c.y),
        ctx.stroke,
    );
    line(
        ctx.painter,
        Pos2::new(c.x, c.y - w),
        Pos2::new(c.x, c.y + w),
        ctx.stroke,
    );
}

pub fn x(ctx: &IconCtx) {
    let c = ctx.rect.center();
    let w = ctx.rect.width() * 0.28;
    line(
        ctx.painter,
        Pos2::new(c.x - w, c.y - w),
        Pos2::new(c.x + w, c.y + w),
        ctx.stroke,
    );
    line(
        ctx.painter,
        Pos2::new(c.x - w, c.y + w),
        Pos2::new(c.x + w, c.y - w),
        ctx.stroke,
    );
}

pub fn home(ctx: &IconCtx) {
    seg(
        ctx.painter,
        vec![ctx.p(0.2, 0.5), ctx.p(0.5, 0.22), ctx.p(0.8, 0.5)],
        ctx.stroke,
        false,
    );
    seg(
        ctx.painter,
        vec![
            ctx.p(0.28, 0.46),
            ctx.p(0.28, 0.8),
            ctx.p(0.72, 0.8),
            ctx.p(0.72, 0.46),
        ],
        ctx.stroke,
        false,
    );
}

pub fn cloud(ctx: &IconCtx) {
    let pts: Vec<Pos2> = (0..=32)
        .map(|i| {
            let t = -180.0 + 180.0 * (i as f32 / 32.0);
            let rad = t.to_radians();
            Pos2::new(
                ctx.rect.center().x + ctx.rect.width() * 0.22 * rad.cos(),
                ctx.rect.center().y
                    + ctx.rect.height() * 0.05
                    + ctx.rect.height() * 0.22 * rad.sin() * 0.6,
            )
        })
        .collect();
    seg(ctx.painter, pts, ctx.stroke, false);
    let c = Pos2::new(
        ctx.rect.center().x - ctx.rect.width() * 0.14,
        ctx.rect.center().y + ctx.rect.height() * 0.02,
    );
    let r = ctx.rect.width() * 0.2;
    let pts: Vec<Pos2> = (0..=32)
        .map(|i| {
            let t = -160.0 + 320.0 * (i as f32 / 32.0);
            let rad = t.to_radians();
            Pos2::new(c.x + r * rad.cos(), c.y + r * rad.sin() * 0.9)
        })
        .collect();
    seg(ctx.painter, pts, ctx.stroke, false);
    line(ctx.painter, ctx.p(0.3, 0.66), ctx.p(0.72, 0.66), ctx.stroke);
}

pub fn info(ctx: &IconCtx) {
    let c = ctx.rect.center();
    circle(ctx.painter, c, ctx.rect.width() * 0.38, ctx.stroke);
    line(
        ctx.painter,
        Pos2::new(c.x, c.y - ctx.rect.height() * 0.16),
        Pos2::new(c.x, c.y + ctx.rect.height() * 0.18),
        ctx.stroke,
    );
    dot(
        ctx.painter,
        Pos2::new(c.x, c.y - ctx.rect.height() * 0.2),
        1.6,
        ctx.stroke.color,
    );
}

pub fn ellipsis(ctx: &IconCtx) {
    let c = ctx.rect.center();
    let gap = ctx.rect.width() * 0.22;
    for dx in [-1.0f32, 0.0, 1.0] {
        dot(
            ctx.painter,
            Pos2::new(c.x + dx * gap, c.y),
            ctx.rect.width() * 0.055,
            ctx.stroke.color,
        );
    }
}

pub fn qr(ctx: &IconCtx) {
    let s = ctx.rect.width() * 0.3;
    let tl = Pos2::new(ctx.rect.left(), ctx.rect.top());
    for (ox, oy) in [
        (0.0, 0.0),
        (ctx.rect.width() - s, 0.0),
        (0.0, ctx.rect.height() - s),
    ] {
        let r = Rect::from_min_size(Pos2::new(tl.x + ox, tl.y + oy), Vec2::new(s, s));
        ctx.painter.add(Shape::rect_stroke(r, 2.0, ctx.stroke));
        ctx.painter.add(Shape::rect_filled(
            r.shrink(s * 0.32),
            1.0,
            ctx.stroke.color,
        ));
    }
    dot(ctx.painter, ctx.p(0.62, 0.62), 2.0, ctx.stroke.color);
    dot(ctx.painter, ctx.p(0.78, 0.62), 2.0, ctx.stroke.color);
    dot(ctx.painter, ctx.p(0.62, 0.78), 2.0, ctx.stroke.color);
    dot(ctx.painter, ctx.p(0.8, 0.82), 2.0, ctx.stroke.color);
}

pub fn clipboard(ctx: &IconCtx) {
    let r = Rect::from_min_size(
        ctx.p(0.24, 0.2),
        Vec2::new(ctx.rect.width() * 0.52, ctx.rect.height() * 0.62),
    );
    ctx.painter.add(Shape::rect_stroke(r, 3.0, ctx.stroke));
    let clip = Rect::from_min_size(
        ctx.p(0.36, 0.12),
        Vec2::new(ctx.rect.width() * 0.28, ctx.rect.height() * 0.16),
    );
    ctx.painter
        .add(Shape::rect_filled(clip, 2.0, ctx.stroke.color));
    line(
        ctx.painter,
        ctx.p(0.38, 0.48),
        ctx.p(0.62, 0.48),
        ctx.stroke,
    );
    line(
        ctx.painter,
        ctx.p(0.38, 0.62),
        ctx.p(0.62, 0.62),
        ctx.stroke,
    );
}

pub fn download(ctx: &IconCtx) {
    let c = Pos2::new(
        ctx.rect.center().x,
        ctx.rect.top() + ctx.rect.height() * 0.4,
    );
    let w = ctx.rect.width() * 0.26;
    line(
        ctx.painter,
        Pos2::new(c.x, ctx.rect.top() + ctx.rect.height() * 0.16),
        Pos2::new(c.x, c.y + ctx.rect.height() * 0.14),
        ctx.stroke,
    );
    seg(
        ctx.painter,
        vec![
            Pos2::new(c.x - w, c.y - w * 0.4),
            Pos2::new(c.x, c.y + ctx.rect.height() * 0.16),
            Pos2::new(c.x + w, c.y - w * 0.4),
        ],
        ctx.stroke,
        false,
    );
    line(
        ctx.painter,
        ctx.p(0.24, 0.78),
        ctx.p(0.76, 0.78),
        ctx.stroke,
    );
}

pub fn upload(ctx: &IconCtx) {
    let c = Pos2::new(
        ctx.rect.center().x,
        ctx.rect.top() + ctx.rect.height() * 0.46,
    );
    let w = ctx.rect.width() * 0.26;
    line(
        ctx.painter,
        Pos2::new(c.x, ctx.rect.top() + ctx.rect.height() * 0.16),
        Pos2::new(c.x, c.y + ctx.rect.height() * 0.2),
        ctx.stroke,
    );
    seg(
        ctx.painter,
        vec![
            Pos2::new(c.x - w, c.y - w * 0.4 + ctx.rect.height() * 0.02),
            Pos2::new(c.x, ctx.rect.top() + ctx.rect.height() * 0.18),
            Pos2::new(c.x + w, c.y - w * 0.4 + ctx.rect.height() * 0.02),
        ],
        ctx.stroke,
        false,
    );
    line(
        ctx.painter,
        ctx.p(0.24, 0.82),
        ctx.p(0.76, 0.82),
        ctx.stroke,
    );
}

pub fn rss(ctx: &IconCtx) {
    let c = Pos2::new(
        ctx.rect.left() + ctx.rect.width() * 0.3,
        ctx.rect.bottom() - ctx.rect.height() * 0.3,
    );
    let r1 = ctx.rect.width() * 0.14;
    let r2 = ctx.rect.width() * 0.34;
    let r3 = ctx.rect.width() * 0.54;
    dot(ctx.painter, c, 2.4, ctx.stroke.color);
    circle(ctx.painter, c, r1, ctx.stroke);
    circle(ctx.painter, c, r2, ctx.stroke);
    circle(ctx.painter, c, r3, ctx.stroke);
}

pub fn medal(ctx: &IconCtx, color: Color32) {
    let c = Pos2::new(
        ctx.rect.center().x,
        ctx.rect.top() + ctx.rect.height() * 0.38,
    );
    let r = ctx.rect.width() * 0.24;
    // Ribbon tails.
    seg(
        ctx.painter,
        vec![Pos2::new(c.x - r * 0.7, c.y + r * 0.6), ctx.p(0.3, 0.9)],
        Stroke::new(2.2, color),
        false,
    );
    seg(
        ctx.painter,
        vec![Pos2::new(c.x + r * 0.7, c.y + r * 0.6), ctx.p(0.7, 0.9)],
        Stroke::new(2.2, color),
        false,
    );
    circle(ctx.painter, c, r, Stroke::new(2.2, color));
    // Star inside.
    dot(ctx.painter, c, r * 0.42, color);
}

pub fn star(ctx: &IconCtx) {
    let c = ctx.rect.center();
    let r = ctx.rect.width() * 0.36;
    let mut pts = Vec::new();
    for i in 0..10 {
        let rad = if i % 2 == 0 { r } else { r * 0.45 };
        let t = (-90.0 + 36.0 * i as f32).to_radians();
        pts.push(Pos2::new(c.x + rad * t.cos(), c.y + rad * t.sin()));
    }
    seg(ctx.painter, pts, ctx.stroke, true);
}

pub fn gear(ctx: &IconCtx) {
    let c = ctx.rect.center();
    let r = ctx.rect.width() * 0.3;
    circle(ctx.painter, c, r * 0.55, ctx.stroke);
    for i in 0..8 {
        let t = (45.0 * i as f32).to_radians();
        let a = Pos2::new(c.x + r * 0.72 * t.cos(), c.y + r * 0.72 * t.sin());
        let b = Pos2::new(c.x + r * 1.15 * t.cos(), c.y + r * 1.15 * t.sin());
        line(ctx.painter, a, b, ctx.stroke);
    }
}

pub fn folder(ctx: &IconCtx) {
    seg(
        ctx.painter,
        vec![
            ctx.p(0.2, 0.3),
            ctx.p(0.42, 0.3),
            ctx.p(0.5, 0.4),
            ctx.p(0.8, 0.4),
            ctx.p(0.8, 0.74),
            ctx.p(0.2, 0.74),
        ],
        ctx.stroke,
        true,
    );
}

pub fn grid(ctx: &IconCtx) {
    for dx in [0.0f32, 0.4, 0.8] {
        for dy in [0.0f32, 0.4, 0.8] {
            dot(
                ctx.painter,
                ctx.p(0.16 + dx * 0.85, 0.16 + dy * 0.85),
                ctx.rect.width() * 0.075,
                ctx.stroke.color,
            );
        }
    }
}

pub fn sun(ctx: &IconCtx) {
    let c = ctx.rect.center();
    circle(ctx.painter, c, ctx.rect.width() * 0.18, ctx.stroke);
    for i in 0..8 {
        let t = (45.0 * i as f32).to_radians();
        let a = Pos2::new(
            c.x + ctx.rect.width() * 0.26 * t.cos(),
            c.y + ctx.rect.width() * 0.26 * t.sin(),
        );
        let b = Pos2::new(
            c.x + ctx.rect.width() * 0.36 * t.cos(),
            c.y + ctx.rect.width() * 0.36 * t.sin(),
        );
        line(ctx.painter, a, b, ctx.stroke);
    }
}

pub fn moon(ctx: &IconCtx) {
    let c = ctx.rect.center();
    let r = ctx.rect.width() * 0.3;
    let pts: Vec<Pos2> = (0..=40)
        .map(|i| {
            let t = -110.0 + 320.0 * (i as f32 / 40.0);
            let rad = t.to_radians();
            Pos2::new(c.x + r * rad.cos(), c.y + r * rad.sin())
        })
        .collect();
    seg(ctx.painter, pts, ctx.stroke, false);
    line(
        ctx.painter,
        Pos2::new(c.x + r * 0.55, c.y - r * 0.75),
        Pos2::new(c.x + r * 0.55, c.y + r * 0.75),
        ctx.stroke,
    );
}

pub fn auto_theme(ctx: &IconCtx) {
    let c = ctx.rect.center();
    let r = ctx.rect.width() * 0.3;
    // Right half arc + straight edge = half-filled circle ("auto").
    let mut pts: Vec<Pos2> = (0..=20)
        .map(|i| {
            let t = -90.0 + 180.0 * (i as f32 / 20.0);
            let rad = t.to_radians();
            Pos2::new(c.x + r * rad.cos(), c.y + r * rad.sin())
        })
        .collect();
    pts.push(Pos2::new(c.x - r, c.y));
    pts.push(Pos2::new(c.x - r, c.y - r));
    seg(ctx.painter, pts, ctx.stroke, false);
    seg(
        ctx.painter,
        vec![ctx.p(0.5, 0.2), ctx.p(0.5, 0.8)],
        ctx.stroke,
        false,
    );
}

pub fn paint_with(
    painter: &Painter,
    rect: Rect,
    color: Color32,
    draw: impl FnOnce(&IconCtx),
    width: f32,
) {
    let ctx = IconCtx::new(painter, rect, color, width);
    draw(&ctx);
}

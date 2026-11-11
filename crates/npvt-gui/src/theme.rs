//! Visual identity: the exact dark green "terminal" look of the NPV Tunnel
//! design, plus a light variant for the Light/Auto appearance setting.
//!
//! Fonts are embedded at compile time:
//! - JetBrains Mono (Regular/Medium/Bold) for every tracked terminal-style
//!   label (`[ CONNECTED ]`, `ACTIVE CONNECTION GROUP`, version, logs);
//! - Roboto (Regular/Medium/Bold) for UI text.
//!
//! egui resolves font *families* only, so bold weights live in named
//! families ("sans-bold", "mono-bold") and every bold label goes through
//! [`mono_bold`] / [`sans_bold`] helpers - no reliance on synthesis.

use egui::{
    epaint::Shadow, Color32, Context, FontData, FontDefinitions, FontFamily, Margin, Rounding,
    Stroke, Style,
};

/// One resolved palette. All colors come straight from the reference design.
#[derive(Clone, Copy)]
pub struct Palette {
    pub window_bg: Color32,
    pub card: Color32,
    pub card_stroke: Color32,
    pub input_bg: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub green: Color32,
    pub green_dim: Color32,
    pub green_deep: Color32,
    pub green_border: Color32,
    pub green_pill: Color32,
    pub teal_bg: Color32,
    pub teal_text: Color32,
    pub cyan: Color32,
    pub orange: Color32,
    pub gold: Color32,
    pub red: Color32,
    pub scrim: Color32,
    pub track: Color32,
}

/// The reference dark palette.
pub fn dark() -> Palette {
    Palette {
        window_bg: Color32::from_rgb(5, 8, 7),
        card: Color32::from_rgb(13, 18, 16),
        card_stroke: Color32::from_rgb(28, 38, 34),
        input_bg: Color32::from_rgb(10, 14, 13),
        text: Color32::from_rgb(236, 244, 240),
        text_dim: Color32::from_rgb(146, 160, 154),
        text_faint: Color32::from_rgb(94, 106, 101),
        green: Color32::from_rgb(63, 217, 127),
        green_dim: Color32::from_rgb(30, 122, 76),
        green_deep: Color32::from_rgb(14, 34, 24),
        green_border: Color32::from_rgb(74, 222, 128),
        green_pill: Color32::from_rgb(17, 46, 32),
        teal_bg: Color32::from_rgb(18, 64, 61),
        teal_text: Color32::from_rgb(126, 217, 200),
        cyan: Color32::from_rgb(56, 189, 248),
        orange: Color32::from_rgb(245, 158, 11),
        gold: Color32::from_rgb(234, 179, 8),
        red: Color32::from_rgb(248, 113, 113),
        scrim: Color32::from_rgba_unmultiplied(0, 0, 0, 168),
        track: Color32::from_rgb(24, 32, 29),
    }
}

/// Inverted variant for the Light/Auto appearance setting (same layout).
pub fn light() -> Palette {
    Palette {
        window_bg: Color32::from_rgb(243, 246, 244),
        card: Color32::from_rgb(255, 255, 255),
        card_stroke: Color32::from_rgb(223, 231, 227),
        input_bg: Color32::from_rgb(250, 251, 250),
        text: Color32::from_rgb(16, 33, 27),
        text_dim: Color32::from_rgb(96, 112, 106),
        text_faint: Color32::from_rgb(140, 154, 148),
        green: Color32::from_rgb(14, 138, 74),
        green_dim: Color32::from_rgb(63, 217, 127),
        green_deep: Color32::from_rgb(224, 244, 231),
        green_border: Color32::from_rgb(14, 138, 74),
        green_pill: Color32::from_rgb(220, 242, 228),
        teal_bg: Color32::from_rgb(204, 234, 230),
        teal_text: Color32::from_rgb(11, 105, 98),
        cyan: Color32::from_rgb(2, 132, 199),
        orange: Color32::from_rgb(194, 120, 3),
        gold: Color32::from_rgb(180, 133, 9),
        red: Color32::from_rgb(220, 38, 38),
        scrim: Color32::from_rgba_unmultiplied(20, 30, 26, 110),
        track: Color32::from_rgb(226, 233, 230),
    }
}

/// Which theme the user picked in MORE > Appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ThemeMode {
    Light,
    Dark,
    Auto,
}

impl ThemeMode {
    #[allow(dead_code)] // stable settings-file vocabulary
    pub fn as_str(self) -> &'static str {
        match self {
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
            ThemeMode::Auto => "auto",
        }
    }
    #[allow(dead_code)] // stable settings-file vocabulary
    pub fn parse(value: &str) -> Self {
        match value {
            "light" => ThemeMode::Light,
            "auto" => ThemeMode::Auto,
            _ => ThemeMode::Dark,
        }
    }
}

/// Insert a space between every character so short terminal-style labels
/// render with the wide tracking of the reference design.
pub fn tracked(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if chars.peek().is_some() {
            out.push(' ');
        }
    }
    out
}

/// Register the embedded fonts and the named weight families.
pub fn set_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts
            .font_data
            .insert(name.to_owned(), FontData::from_static(bytes));
    };
    add(
        "jb-reg",
        include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    );
    add(
        "jb-med",
        include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf"),
    );
    add(
        "jb-bold",
        include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
    );
    add(
        "rb-reg",
        include_bytes!("../assets/fonts/Roboto-Regular.ttf"),
    );
    add(
        "rb-med",
        include_bytes!("../assets/fonts/Roboto-Medium.ttf"),
    );
    add("rb-bold", include_bytes!("../assets/fonts/Roboto-Bold.ttf"));

    let prop = fonts.families.entry(FontFamily::Proportional).or_default();
    prop.clear();
    prop.push("rb-reg".into());
    prop.push("rb-med".into());
    prop.push("rb-bold".into());

    let mono = fonts.families.entry(FontFamily::Monospace).or_default();
    mono.clear();
    mono.push("jb-reg".into());
    mono.push("rb-reg".into());

    let named = &mut fonts.families;
    named.insert(
        FontFamily::Name("sans-med".into()),
        vec!["rb-med".into(), "rb-reg".into()],
    );
    named.insert(
        FontFamily::Name("sans-bold".into()),
        vec!["rb-bold".into(), "rb-med".into(), "rb-reg".into()],
    );
    named.insert(
        FontFamily::Name("mono".into()),
        vec!["jb-reg".into(), "rb-reg".into()],
    );
    named.insert(
        FontFamily::Name("mono-med".into()),
        vec!["jb-med".into(), "jb-reg".into()],
    );
    named.insert(
        FontFamily::Name("mono-bold".into()),
        vec!["jb-bold".into(), "jb-med".into(), "jb-reg".into()],
    );
    ctx.set_fonts(fonts);
}

/// Apply palette + spacing to the context.
pub fn set_visuals(ctx: &Context, p: &Palette) {
    let mut style = Style::default();
    let v = &mut style.visuals;
    v.panel_fill = p.window_bg;
    v.window_fill = p.card;
    v.extreme_bg_color = p.input_bg;
    v.override_text_color = Some(p.text);
    v.window_stroke = Stroke::new(1.0_f32, p.card_stroke);
    v.window_rounding = Rounding::same(14.0);
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.selection.stroke = Stroke::new(1.0_f32, p.green);
    v.selection.bg_fill = p.green_deep;
    v.hyperlink_color = p.cyan;
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.fg_stroke = Stroke::new(1.0_f32, p.text);
        w.bg_stroke = Stroke::new(1.0_f32, p.card_stroke);
        w.rounding = Rounding::same(10.0);
    }
    v.widgets.noninteractive.bg_fill = p.card;
    v.widgets.inactive.bg_fill = p.input_bg;
    v.widgets.hovered.bg_fill = p.green_deep;
    v.widgets.active.bg_fill = p.green_deep;
    v.widgets.hovered.weak_bg_fill = p.green_deep;
    v.widgets.inactive.weak_bg_fill = p.input_bg;
    v.widgets.noninteractive.weak_bg_fill = p.card;
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.menu_margin = Margin::same(6.0);
    style.spacing.interact_size = egui::vec2(40.0, 30.0);
    let visuals_copy = style.visuals.clone();
    ctx.all_styles_mut(|s| *s = style.clone());
    ctx.set_visuals(visuals_copy);
}

// ---- RichText builders -------------------------------------------------

/// Regular UI text.
pub fn sans(text: impl Into<String>, size: f32, color: Color32) -> egui::RichText {
    egui::RichText::new(text)
        .size(size)
        .color(color)
        .family(FontFamily::Proportional)
}

/// Medium-weight UI text.
pub fn sans_med(text: impl Into<String>, size: f32, color: Color32) -> egui::RichText {
    egui::RichText::new(text)
        .size(size)
        .color(color)
        .family(FontFamily::Name("sans-med".into()))
}

/// Bold UI text.
pub fn sans_bold(text: impl Into<String>, size: f32, color: Color32) -> egui::RichText {
    egui::RichText::new(text)
        .size(size)
        .color(color)
        .family(FontFamily::Name("sans-bold".into()))
}

/// Terminal monospace text.
pub fn mono(text: impl Into<String>, size: f32, color: Color32) -> egui::RichText {
    egui::RichText::new(text)
        .size(size)
        .color(color)
        .family(FontFamily::Name("mono".into()))
}

/// Medium monospace text.
pub fn mono_med(text: impl Into<String>, size: f32, color: Color32) -> egui::RichText {
    egui::RichText::new(text)
        .size(size)
        .color(color)
        .family(FontFamily::Name("mono-med".into()))
}

/// Bold monospace text.
pub fn mono_bold(text: impl Into<String>, size: f32, color: Color32) -> egui::RichText {
    egui::RichText::new(text)
        .size(size)
        .color(color)
        .family(FontFamily::Name("mono-bold".into()))
}

/// Tracked regular monospace label, as in `ACTIVE CONNECTION GROUP`.
pub fn mono_label_tracked(text: impl Into<String>, size: f32, color: Color32) -> egui::RichText {
    mono(tracked(text.into().as_str()), size, color)
}

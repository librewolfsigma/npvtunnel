//! Visual language of NPV TUNNEL: brand palette, Inter fonts, egui styling.
//!
//! Dark is the flagship look (near-black surfaces with a neon green accent,
//! like the brand material); a matching light variant exists for the Auto
//! theme, which follows the Windows personalization setting.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Margin, Stroke, TextStyle,
    Visuals,
};

/// Brand palette.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub card: Color32,
    pub card_hover: Color32,
    pub card_stroke: Color32,
    pub field: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub teal: Color32,
    pub danger: Color32,
    pub danger_soft: Color32,
    pub warn: Color32,
}

pub const DARK: Palette = Palette {
    bg: Color32::from_rgb(10, 15, 11),
    panel: Color32::from_rgb(12, 18, 13),
    card: Color32::from_rgb(18, 26, 20),
    card_hover: Color32::from_rgb(24, 35, 27),
    card_stroke: Color32::from_rgb(30, 43, 34),
    field: Color32::from_rgb(8, 12, 9),
    text: Color32::from_rgb(232, 241, 234),
    muted: Color32::from_rgb(141, 162, 149),
    accent: Color32::from_rgb(34, 221, 102),
    accent_soft: Color32::from_rgb(16, 46, 27),
    on_accent: Color32::from_rgb(4, 32, 16),
    teal: Color32::from_rgb(42, 212, 176),
    danger: Color32::from_rgb(255, 107, 107),
    danger_soft: Color32::from_rgb(56, 22, 22),
    warn: Color32::from_rgb(255, 180, 84),
};

pub const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(243, 247, 244),
    panel: Color32::from_rgb(255, 255, 255),
    card: Color32::from_rgb(255, 255, 255),
    card_hover: Color32::from_rgb(238, 245, 239),
    card_stroke: Color32::from_rgb(214, 226, 217),
    field: Color32::from_rgb(240, 245, 241),
    text: Color32::from_rgb(21, 33, 26),
    muted: Color32::from_rgb(94, 114, 102),
    accent: Color32::from_rgb(15, 165, 82),
    accent_soft: Color32::from_rgb(223, 243, 230),
    on_accent: Color32::from_rgb(255, 255, 255),
    teal: Color32::from_rgb(14, 158, 136),
    danger: Color32::from_rgb(214, 69, 69),
    danger_soft: Color32::from_rgb(250, 228, 228),
    warn: Color32::from_rgb(192, 123, 29),
};

/// Register Inter and make it the default proportional family.
pub fn install_fonts(cc: &eframe::CreationContext<'_>) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        (
            "inter-regular",
            &include_bytes!("../../../assets/Inter-Regular.ttf")[..],
        ),
        (
            "inter-medium",
            &include_bytes!("../../../assets/Inter-Medium.ttf")[..],
        ),
        (
            "inter-semibold",
            &include_bytes!("../../../assets/Inter-SemiBold.ttf")[..],
        ),
        (
            "inter-bold",
            &include_bytes!("../../../assets/Inter-Bold.ttf")[..],
        ),
    ] {
        fonts.font_data.insert(
            name.to_string(),
            std::sync::Arc::new(FontData::from_static(bytes)),
        );
    }
    for name in [
        "inter-regular",
        "inter-medium",
        "inter-semibold",
        "inter-bold",
    ] {
        if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
            list.insert(0, name.to_string());
        }
    }
    // Dedicated families for headings/labels so weight is explicit.
    fonts.families.insert(
        FontFamily::Name("SemiBold".into()),
        vec!["inter-semibold".into(), "inter-regular".into()],
    );
    fonts.families.insert(
        FontFamily::Name("Bold".into()),
        vec!["inter-bold".into(), "inter-semibold".into()],
    );
    fonts.families.insert(
        FontFamily::Name("Medium".into()),
        vec!["inter-medium".into(), "inter-regular".into()],
    );
    cc.egui_ctx.set_fonts(fonts);
}

/// Build a `Visuals` set from the palette and apply it plus global spacing.
pub fn apply(ctx: &egui::Context, p: &Palette, dark: bool) {
    let mut vis = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    vis.dark_mode = dark;
    vis.window_fill = p.bg;
    vis.panel_fill = p.panel;
    vis.extreme_bg_color = p.field;
    vis.faint_bg_color = p.card;
    vis.hyperlink_color = p.teal;
    vis.selection.stroke = Stroke::new(1.0, p.accent);
    vis.window_stroke = Stroke::new(1.0, p.card_stroke);
    let w = &mut vis.widgets;
    for (widget, base, _hover, stroke) in [
        (&mut w.noninteractive, p.card, p.card, p.card_stroke),
        (&mut w.inactive, p.card, p.card_hover, p.card_stroke),
        (&mut w.hovered, p.card_hover, p.card_hover, p.accent),
        (&mut w.active, p.accent_soft, p.accent_soft, p.accent),
    ] {
        widget.weak_bg_fill = base;
        widget.bg_fill = base;
        widget.bg_stroke = Stroke::new(1.0, stroke);
        widget.fg_stroke = Stroke::new(1.0, p.text);
        widget.corner_radius = CornerRadius::same(8);
    }
    w.active.bg_fill = p.accent;
    w.active.weak_bg_fill = p.accent;
    w.active.fg_stroke = Stroke::new(1.0, p.on_accent);
    vis.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);

    let theme = if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(theme);
    ctx.style_mut_of(theme, |style| {
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 7.0);
        style.spacing.menu_margin = Margin::same(8);
        style.spacing.menu_spacing = 2.0;
        style.spacing.icon_width = 18.0;
        style.spacing.icon_width_inner = 12.0;
        style.spacing.icon_spacing = 6.0;
        style.spacing.scroll.bar_width = 8.0;
        style.text_styles.insert(
            TextStyle::Heading,
            eframe::egui::FontId::new(22.0, FontFamily::Name("Bold".into())),
        );
        style.text_styles.insert(
            TextStyle::Body,
            eframe::egui::FontId::new(13.5, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Button,
            eframe::egui::FontId::new(13.0, FontFamily::Name("Medium".into())),
        );
        style.text_styles.insert(
            TextStyle::Small,
            eframe::egui::FontId::new(11.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Monospace,
            eframe::egui::FontId::new(12.5, FontFamily::Monospace),
        );
    });
    ctx.set_visuals_of(theme, vis);
}

/// Convenience: heading font family text.
pub fn semi(text: impl Into<String>) -> eframe::egui::RichText {
    eframe::egui::RichText::new(text).family(FontFamily::Name("SemiBold".into()))
}

/// Convenience: bold-family text.
pub fn bold(text: impl Into<String>) -> eframe::egui::RichText {
    eframe::egui::RichText::new(text).family(FontFamily::Name("Bold".into()))
}

/// Convenience: monospace text.
pub fn mono(text: impl Into<String>) -> eframe::egui::RichText {
    eframe::egui::RichText::new(text).family(FontFamily::Monospace)
}

//! Visual theme: palette, fonts and egui style application.
//!
//! The palette mirrors the reference design: near-black backgrounds with a
//! faint green tint, a mint/green accent for the tunnel state, teal for the
//! active navigation item, cyan for ping actions and amber for upload stats.

use eframe::egui;

/// Raw RGB helper for building palette entries.
#[derive(Debug, Clone, Copy)]
pub struct Color(pub u32);

impl Color {
    pub const fn c32(self) -> egui::Color32 {
        let v = self.0;
        egui::Color32::from_rgb((v >> 16) as u8, ((v >> 8) & 0xff) as u8, (v & 0xff) as u8)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum ThemeMode {
    Light,
    #[default]
    Dark,
    Auto,
}

impl ThemeMode {
    /// All modes in display order.
    pub const ALL: [ThemeMode; 3] = [ThemeMode::Light, ThemeMode::Dark, ThemeMode::Auto];
}

/// A resolved color set for one appearance.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg: egui::Color32,
    pub surface: egui::Color32,
    pub surface_hi: egui::Color32,
    pub border: egui::Color32,
    pub border_sel: egui::Color32,
    pub text: egui::Color32,
    pub text_secondary: egui::Color32,
    pub text_faint: egui::Color32,
    pub accent: egui::Color32,
    pub accent_bright: egui::Color32,
    pub teal_pill: egui::Color32,
    pub teal_text: egui::Color32,
    pub cyan: egui::Color32,
    pub amber: egui::Color32,
    pub red: egui::Color32,
    pub scrim: egui::Color32,
    pub toast_bg: egui::Color32,
}

pub const DARK: Palette = Palette {
    bg: Color(0x0A0D0A).c32(),
    surface: Color(0x101510).c32(),
    surface_hi: Color(0x0F2413).c32(),
    border: Color(0x1D241D).c32(),
    border_sel: Color(0x22C55E).c32(),
    text: Color(0xF2F5F2).c32(),
    text_secondary: Color(0x98A29A).c32(),
    text_faint: Color(0x6B746B).c32(),
    accent: Color(0x22C55E).c32(),
    accent_bright: Color(0x4ADE80).c32(),
    teal_pill: Color(0x0E5F58).c32(),
    teal_text: Color(0x5EEAD4).c32(),
    cyan: Color(0x22D3EE).c32(),
    amber: Color(0xFBBF24).c32(),
    red: Color(0xF87171).c32(),
    scrim: egui::Color32::from_black_alpha(180),
    toast_bg: Color(0x1A221A).c32(),
};

pub const LIGHT: Palette = Palette {
    bg: Color(0xF3F6F3).c32(),
    surface: Color(0xFFFFFF).c32(),
    surface_hi: Color(0xE7F7EC).c32(),
    border: Color(0xD9E0D9).c32(),
    border_sel: Color(0x16A34A).c32(),
    text: Color(0x16211A).c32(),
    text_secondary: Color(0x5C685F).c32(),
    text_faint: Color(0x8B968D).c32(),
    accent: Color(0x16A34A).c32(),
    accent_bright: Color(0x22C55E).c32(),
    teal_pill: Color(0x0F766E).c32(),
    teal_text: Color(0x0F766E).c32(),
    cyan: Color(0x0891B2).c32(),
    amber: Color(0xB45309).c32(),
    red: Color(0xDC2626).c32(),
    scrim: egui::Color32::from_black_alpha(110),
    toast_bg: Color(0xE4EFE6).c32(),
};

const MONO_SPACED_FAMILY: &str = "inter_sb";
const BOLD_FAMILY: &str = "inter_bold";
const MONO_BOLD_FAMILY: &str = "mono_bold";

/// Load the bundled fonts (Inter for text, JetBrains Mono for the technical
/// labels) and map egui text styles onto them.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts
            .font_data
            .insert(name.to_owned(), egui::FontData::from_owned(bytes.to_vec()));
    };
    add(
        "inter_regular",
        include_bytes!("../assets/fonts/Inter-Regular.otf"),
    );
    add(
        "inter_semibold",
        include_bytes!("../assets/fonts/Inter-SemiBold.otf"),
    );
    add(
        "inter_bold",
        include_bytes!("../assets/fonts/Inter-Bold.otf"),
    );
    add(
        "jbm_regular",
        include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    );
    add(
        "jbm_bold",
        include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
    );

    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "inter_regular".to_owned());
    fonts
        .families
        .insert(egui::FontFamily::Name(MONO_SPACED_FAMILY.into()), {
            vec!["inter_semibold".to_owned()]
        });
    fonts
        .families
        .insert(egui::FontFamily::Name(BOLD_FAMILY.into()), {
            vec!["inter_bold".to_owned()]
        });
    fonts
        .families
        .insert(egui::FontFamily::Name(MONO_BOLD_FAMILY.into()), {
            vec!["jbm_bold".to_owned()]
        });
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "jbm_regular".to_owned());

    ctx.set_fonts(fonts);
}

/// Resolve `Auto` into a concrete palette.
pub fn resolve(mode: ThemeMode) -> Palette {
    match mode {
        ThemeMode::Light => LIGHT,
        ThemeMode::Dark => DARK,
        ThemeMode::Auto => {
            if system_prefers_light() {
                LIGHT
            } else {
                DARK
            }
        }
    }
}

/// Apply palette + typography to the egui context.
pub fn apply(ctx: &egui::Context, mode: ThemeMode) {
    let p = resolve(mode);
    install_fonts(ctx);

    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (
            egui::TextStyle::Body,
            egui::FontId::new(13.5, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Small,
            egui::FontId::new(11.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Button,
            egui::FontId::new(13.5, egui::FontFamily::Name(MONO_SPACED_FAMILY.into())),
        ),
        (
            egui::TextStyle::Heading,
            egui::FontId::new(19.0, egui::FontFamily::Name(BOLD_FAMILY.into())),
        ),
        (
            egui::TextStyle::Monospace,
            egui::FontId::new(12.5, egui::FontFamily::Monospace),
        ),
    ]
    .into();

    let mut v = egui::Visuals::dark();
    if mode == ThemeMode::Light || (mode == ThemeMode::Auto && system_prefers_light()) {
        v = egui::Visuals::light();
    }
    v.override_text_color = Some(p.text);
    v.window_fill = p.bg;
    v.panel_fill = p.bg;
    v.extreme_bg_color = p.surface;
    v.faint_bg_color = p.surface;
    v.selection.bg_fill = p.accent.gamma_multiply(0.35);
    v.selection.stroke = egui::Stroke::new(1.0, p.accent);
    v.widgets.noninteractive.bg_fill = p.surface;
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, p.border);
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, p.text_secondary);
    v.widgets.inactive.bg_fill = p.surface;
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, p.border);
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, p.text);
    v.widgets.hovered.bg_fill = p.surface_hi;
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, p.border_sel);
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, p.text);
    v.widgets.active.bg_fill = p.surface_hi;
    v.widgets.active.fg_stroke = egui::Stroke::new(1.0, p.accent);
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0, p.accent);
    style.visuals = v;
    ctx.set_style(style);
}

/// Font for the spaced monospace-style labels ("N P V   T U N N E L").
pub fn mono(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Monospace)
}

pub fn mono_bold(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(MONO_BOLD_FAMILY.into()))
}

pub fn semibold(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(MONO_SPACED_FAMILY.into()))
}

pub fn bold(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(BOLD_FAMILY.into()))
}

/// Letter-space a string by inserting thin gaps (the design's wide tracking).
pub fn spaced(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for (i, ch) in text.chars().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

#[cfg(windows)]
fn system_prefers_light() -> bool {
    let output = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "/v",
            "AppsUseLightTheme",
        ])
        .output();
    match output {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.to_ascii_lowercase().contains("0x1")
        }
        Err(_) => false,
    }
}

#[cfg(not(windows))]
fn system_prefers_light() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaced_inserts_gaps() {
        assert_eq!(spaced("NPV"), "N P V");
        assert_eq!(spaced(""), "");
    }

    #[test]
    fn palette_conversion() {
        let green = Color(0x22C55E);
        assert_eq!(green.c32(), egui::Color32::from_rgb(0x22, 0xC5, 0x5E));
    }
}

//! Design tokens, bundled OFL typography, WCAG 2.x contrast checks, and egui theme builder.

use egui::{Color32, FontData, FontDefinitions, FontFamily, Rounding, Stroke, Visuals};

use crate::prefs::ThemePreference;

/// Color palette for Light or Dark mode matching `DESIGN.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub dark_mode: bool,
    pub bg_canvas: Color32,
    pub bg_surface: Color32,
    pub bg_subtle: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub border_subtle: Color32,
    pub border_interactive: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_text: Color32,
    pub on_accent: Color32,
    pub emerald_text: Color32,
    pub emerald_bg: Color32,
    pub emerald_border: Color32,
    pub amber_text: Color32,
    pub amber_bg: Color32,
    pub amber_border: Color32,
    pub red_text: Color32,
    pub red_bg: Color32,
    pub red_border: Color32,
    pub blue_text: Color32,
    pub blue_bg: Color32,
    pub blue_border: Color32,
}

impl Palette {
    pub const LIGHT: Self = Self {
        dark_mode: false,
        bg_canvas: Color32::from_rgb(0xF8, 0xFA, 0xFC),
        bg_surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        bg_subtle: Color32::from_rgb(0xF1, 0xF5, 0xF9),
        text_primary: Color32::from_rgb(0x0F, 0x17, 0x2A),
        text_secondary: Color32::from_rgb(0x47, 0x55, 0x69),
        border_subtle: Color32::from_rgb(0xCB, 0xD5, 0xE1),
        border_interactive: Color32::from_rgb(0x73, 0x82, 0x96),
        accent: Color32::from_rgb(0x1D, 0x4E, 0xD8),
        accent_hover: Color32::from_rgb(0x1E, 0x40, 0xAF),
        accent_text: Color32::from_rgb(0x1D, 0x4E, 0xD8),
        on_accent: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        emerald_text: Color32::from_rgb(0x04, 0x78, 0x57),
        emerald_bg: Color32::from_rgb(0xEC, 0xFD, 0xF5),
        emerald_border: Color32::from_rgb(0x05, 0x96, 0x69),
        amber_text: Color32::from_rgb(0xB4, 0x53, 0x09),
        amber_bg: Color32::from_rgb(0xFF, 0xFB, 0xEB),
        amber_border: Color32::from_rgb(0xD9, 0x77, 0x06),
        red_text: Color32::from_rgb(0xB9, 0x1C, 0x1C),
        red_bg: Color32::from_rgb(0xFE, 0xF2, 0xF2),
        red_border: Color32::from_rgb(0xDC, 0x26, 0x26),
        blue_text: Color32::from_rgb(0x1D, 0x4E, 0xD8),
        blue_bg: Color32::from_rgb(0xEF, 0xF6, 0xFF),
        blue_border: Color32::from_rgb(0x25, 0x63, 0xEB),
    };

    pub const DARK: Self = Self {
        dark_mode: true,
        bg_canvas: Color32::from_rgb(0x0B, 0x11, 0x20),
        bg_surface: Color32::from_rgb(0x11, 0x18, 0x27),
        bg_subtle: Color32::from_rgb(0x1E, 0x29, 0x3B),
        text_primary: Color32::from_rgb(0xF8, 0xFA, 0xFC),
        text_secondary: Color32::from_rgb(0x94, 0xA3, 0xB8),
        border_subtle: Color32::from_rgb(0x33, 0x41, 0x55),
        border_interactive: Color32::from_rgb(0x64, 0x74, 0x8B),
        accent: Color32::from_rgb(0x1D, 0x4E, 0xD8),
        accent_hover: Color32::from_rgb(0x25, 0x63, 0xEB),
        accent_text: Color32::from_rgb(0x60, 0xA5, 0xFA),
        on_accent: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        emerald_text: Color32::from_rgb(0x34, 0xD3, 0x99),
        emerald_bg: Color32::from_rgb(0x06, 0x4E, 0x3B),
        emerald_border: Color32::from_rgb(0x10, 0xB9, 0x81),
        amber_text: Color32::from_rgb(0xFB, 0xBF, 0x24),
        amber_bg: Color32::from_rgb(0x45, 0x1A, 0x03),
        amber_border: Color32::from_rgb(0xF5, 0x9E, 0x0B),
        red_text: Color32::from_rgb(0xF8, 0x71, 0x71),
        red_bg: Color32::from_rgb(0x45, 0x0A, 0x0A),
        red_border: Color32::from_rgb(0xEF, 0x44, 0x44),
        blue_text: Color32::from_rgb(0xBF, 0xDB, 0xFE),
        blue_bg: Color32::from_rgb(0x1E, 0x3A, 0x8A),
        blue_border: Color32::from_rgb(0x60, 0xA5, 0xFA),
    };
}

/// Shared spacing, radius, and responsive layout constants from `DESIGN.md`.
pub struct ThemeTokens;

impl ThemeTokens {
    pub const SPACE_XS: f32 = 4.0;
    pub const SPACE_SM: f32 = 8.0;
    pub const SPACE_MD: f32 = 12.0;
    pub const SPACE_LG: f32 = 16.0;
    pub const SPACE_XL: f32 = 20.0;
    pub const SPACE_2XL: f32 = 24.0;

    pub const RADIUS_CARD: f32 = 8.0;
    pub const RADIUS_CONTROL: f32 = 6.0;
    pub const RADIUS_TAG: f32 = 4.0;

    pub const BREAKPOINT_WIDE: f32 = 860.0;
    pub const BREAKPOINT_MEDIUM: f32 = 580.0;

    pub const FONT_SEMIBOLD_FAMILY: &'static str = "semibold";

    pub fn resolve_palette(pref: ThemePreference, system_dark: bool) -> &'static Palette {
        match pref {
            ThemePreference::Light => &Palette::LIGHT,
            ThemePreference::Dark => &Palette::DARK,
            ThemePreference::System => {
                if system_dark {
                    &Palette::DARK
                } else {
                    &Palette::LIGHT
                }
            }
        }
    }
}

/// Register bundled Plus Jakarta Sans (Regular & SemiBold) and JetBrains Mono fonts.
pub fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "plus_jakarta_regular".to_owned(),
        FontData::from_static(include_bytes!(
            "../assets/fonts/PlusJakartaSans-Regular.ttf"
        )),
    );
    fonts.font_data.insert(
        "plus_jakarta_semibold".to_owned(),
        FontData::from_static(include_bytes!(
            "../assets/fonts/PlusJakartaSans-SemiBold.ttf"
        )),
    );
    fonts.font_data.insert(
        "jetbrains_mono_regular".to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")),
    );

    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "plus_jakarta_regular".to_owned());

    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "jetbrains_mono_regular".to_owned());

    fonts.families.insert(
        FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()),
        vec![
            "plus_jakarta_semibold".to_owned(),
            "plus_jakarta_regular".to_owned(),
        ],
    );

    ctx.set_fonts(fonts);
}

/// Apply the Calm Academic Utility Light or Dark theme to an `egui::Context`.
pub fn apply_theme(ctx: &egui::Context, pref: ThemePreference) -> &'static Palette {
    let system_dark = ctx.style().visuals.dark_mode;
    let palette = ThemeTokens::resolve_palette(pref, system_dark);

    let mut visuals = if palette.dark_mode {
        Visuals::dark()
    } else {
        Visuals::light()
    };

    let card_rounding = Rounding::same(ThemeTokens::RADIUS_CARD);
    let control_rounding = Rounding::same(ThemeTokens::RADIUS_CONTROL);

    visuals.override_text_color = Some(palette.text_primary);
    visuals.panel_fill = palette.bg_canvas;
    visuals.window_fill = palette.bg_surface;
    visuals.window_rounding = card_rounding;
    visuals.window_stroke = Stroke::new(1.0_f32, palette.border_subtle);
    visuals.extreme_bg_color = palette.bg_surface;
    visuals.faint_bg_color = palette.bg_subtle;
    visuals.selection.bg_fill = palette.accent;
    visuals.selection.stroke = Stroke::new(1.0_f32, palette.on_accent);
    visuals.hyperlink_color = palette.accent_text;

    visuals.widgets.noninteractive.bg_fill = palette.bg_surface;
    visuals.widgets.noninteractive.weak_bg_fill = palette.bg_subtle;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, palette.border_subtle);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, palette.text_primary);
    visuals.widgets.noninteractive.rounding = control_rounding;

    visuals.widgets.inactive.bg_fill = palette.bg_surface;
    visuals.widgets.inactive.weak_bg_fill = palette.bg_subtle;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, palette.border_interactive);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, palette.text_primary);
    visuals.widgets.inactive.rounding = control_rounding;

    visuals.widgets.hovered.bg_fill = palette.bg_subtle;
    visuals.widgets.hovered.weak_bg_fill = palette.bg_subtle;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.2_f32, palette.accent_text);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, palette.text_primary);
    visuals.widgets.hovered.rounding = control_rounding;

    visuals.widgets.active.bg_fill = palette.accent;
    visuals.widgets.active.weak_bg_fill = palette.accent_hover;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, palette.accent_text);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, palette.on_accent);
    visuals.widgets.active.rounding = control_rounding;

    ctx.set_visuals(visuals);
    palette
}

fn srgb_to_linear(channel: u8) -> f32 {
    let c = channel as f32 / 255.0;
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn relative_luminance(color: Color32) -> f32 {
    0.2126 * srgb_to_linear(color.r())
        + 0.7152 * srgb_to_linear(color.g())
        + 0.0722 * srgb_to_linear(color.b())
}

/// Compute the WCAG 2.x contrast ratio (`1.0` to `21.0`) between two opaque sRGB colors.
pub fn contrast_ratio(fg: Color32, bg: Color32) -> f32 {
    let l1 = relative_luminance(fg);
    let l2 = relative_luminance(bg);
    let (hi, lo) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_palette_wcag(p: &Palette, name: &str) {
        // Normal text >= 4.5:1
        assert!(
            contrast_ratio(p.text_primary, p.bg_surface) >= 4.5,
            "{name} text_primary/bg_surface"
        );
        assert!(
            contrast_ratio(p.text_primary, p.bg_canvas) >= 4.5,
            "{name} text_primary/bg_canvas"
        );
        assert!(
            contrast_ratio(p.text_secondary, p.bg_surface) >= 4.5,
            "{name} text_secondary/bg_surface"
        );
        assert!(
            contrast_ratio(p.text_secondary, p.bg_canvas) >= 4.5,
            "{name} text_secondary/bg_canvas"
        );
        assert!(
            contrast_ratio(p.accent_text, p.bg_surface) >= 4.5,
            "{name} accent_text/bg_surface"
        );
        assert!(
            contrast_ratio(p.on_accent, p.accent) >= 4.5,
            "{name} on_accent/accent"
        );

        // Interactive control borders >= 3.0:1
        assert!(
            contrast_ratio(p.border_interactive, p.bg_surface) >= 3.0,
            "{name} border_interactive/bg_surface"
        );
        assert!(
            contrast_ratio(p.border_interactive, p.bg_canvas) >= 3.0,
            "{name} border_interactive/bg_canvas"
        );
        assert!(
            contrast_ratio(p.border_interactive, p.bg_subtle) >= 3.0,
            "{name} border_interactive/bg_subtle"
        );

        // Semantic status badges: text >= 4.5:1, border >= 3.0:1
        for (label, text, bg, border) in [
            ("emerald", p.emerald_text, p.emerald_bg, p.emerald_border),
            ("amber", p.amber_text, p.amber_bg, p.amber_border),
            ("red", p.red_text, p.red_bg, p.red_border),
            ("blue", p.blue_text, p.blue_bg, p.blue_border),
        ] {
            assert!(
                contrast_ratio(text, bg) >= 4.5,
                "{name} {label} text/bg < 4.5"
            );
            assert!(
                contrast_ratio(border, p.bg_surface) >= 3.0,
                "{name} {label} border/surface < 3.0"
            );
            assert!(
                contrast_ratio(border, bg) >= 3.0,
                "{name} {label} border/bg < 3.0"
            );
        }
    }

    #[test]
    fn light_and_dark_palettes_pass_r25_wcag_thresholds() {
        assert_palette_wcag(&Palette::LIGHT, "LIGHT");
        assert_palette_wcag(&Palette::DARK, "DARK");
    }

    #[test]
    fn bundled_fonts_have_valid_ttf_headers() {
        let fonts: [&[u8]; 3] = [
            include_bytes!("../assets/fonts/PlusJakartaSans-Regular.ttf"),
            include_bytes!("../assets/fonts/PlusJakartaSans-SemiBold.ttf"),
            include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
        ];
        for bytes in fonts {
            assert!(bytes.len() > 10_000);
            assert_eq!(&bytes[..4], &[0x00, 0x01, 0x00, 0x00]);
        }
    }
}

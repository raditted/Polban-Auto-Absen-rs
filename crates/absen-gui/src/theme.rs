//! Design tokens, bundled OFL typography, WCAG 2.x contrast checks, and egui theme builder.

use egui::{Color32, FontData, FontDefinitions, FontFamily, Rounding, Stroke, Visuals};

use crate::prefs::{DarkThemePreset, LightThemePreset, ThemePreference};

/// Combined theme mode and per-mode preset configuration for live preview and persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AppearanceConfig {
    pub mode: ThemePreference,
    pub light_preset: LightThemePreset,
    pub dark_preset: DarkThemePreset,
}

impl From<ThemePreference> for AppearanceConfig {
    fn from(mode: ThemePreference) -> Self {
        Self {
            mode,
            light_preset: LightThemePreset::DefaultLight,
            dark_preset: DarkThemePreset::DefaultDark,
        }
    }
}

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

    pub const LEGACY_LIGHT: Self = Self {
        dark_mode: false,
        bg_canvas: Color32::from_rgb(0xF3, 0xF4, 0xF6),
        bg_surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        bg_subtle: Color32::from_rgb(0xE5, 0xE7, 0xEB),
        text_primary: Color32::from_rgb(0x11, 0x18, 0x27),
        text_secondary: Color32::from_rgb(0x37, 0x41, 0x51),
        border_subtle: Color32::from_rgb(0xD1, 0xD5, 0xDB),
        border_interactive: Color32::from_rgb(0x52, 0x58, 0x66),
        accent: Color32::from_rgb(0x00, 0x5F, 0xB8),
        accent_hover: Color32::from_rgb(0x00, 0x4C, 0x94),
        accent_text: Color32::from_rgb(0x00, 0x54, 0xA3),
        on_accent: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        emerald_text: Color32::from_rgb(0x06, 0x5F, 0x46),
        emerald_bg: Color32::from_rgb(0xEC, 0xFD, 0xF5),
        emerald_border: Color32::from_rgb(0x04, 0x78, 0x57),
        amber_text: Color32::from_rgb(0x92, 0x40, 0x0E),
        amber_bg: Color32::from_rgb(0xFF, 0xFB, 0xEB),
        amber_border: Color32::from_rgb(0xB4, 0x53, 0x09),
        red_text: Color32::from_rgb(0x99, 0x1B, 0x1B),
        red_bg: Color32::from_rgb(0xFE, 0xF2, 0xF2),
        red_border: Color32::from_rgb(0xB9, 0x1C, 0x1C),
        blue_text: Color32::from_rgb(0x1E, 0x40, 0xAF),
        blue_bg: Color32::from_rgb(0xEF, 0xF6, 0xFF),
        blue_border: Color32::from_rgb(0x1D, 0x4E, 0xD8),
    };

    pub const CATPPUCCIN_LIGHT: Self = Self {
        dark_mode: false,
        bg_canvas: Color32::from_rgb(0xE6, 0xE9, 0xEF),
        bg_surface: Color32::from_rgb(0xEF, 0xF1, 0xF5),
        bg_subtle: Color32::from_rgb(0xDC, 0xE0, 0xE8),
        text_primary: Color32::from_rgb(0x4C, 0x4F, 0x69),
        text_secondary: Color32::from_rgb(0x45, 0x48, 0x5E),
        border_subtle: Color32::from_rgb(0xBC, 0xC0, 0xCC),
        border_interactive: Color32::from_rgb(0x5C, 0x5F, 0x77),
        accent: Color32::from_rgb(0x1E, 0x66, 0xF5),
        accent_hover: Color32::from_rgb(0x17, 0x52, 0xC7),
        accent_text: Color32::from_rgb(0x1A, 0x4F, 0xBA),
        on_accent: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        emerald_text: Color32::from_rgb(0x18, 0x63, 0x1A),
        emerald_bg: Color32::from_rgb(0xE8, 0xF5, 0xE9),
        emerald_border: Color32::from_rgb(0x1E, 0x7B, 0x20),
        amber_text: Color32::from_rgb(0x8A, 0x4B, 0x08),
        amber_bg: Color32::from_rgb(0xFF, 0xF8, 0xE6),
        amber_border: Color32::from_rgb(0x9E, 0x58, 0x0A),
        red_text: Color32::from_rgb(0x9E, 0x0B, 0x22),
        red_bg: Color32::from_rgb(0xFD, 0xED, 0xF0),
        red_border: Color32::from_rgb(0xB8, 0x0D, 0x28),
        blue_text: Color32::from_rgb(0x1A, 0x4F, 0xBA),
        blue_bg: Color32::from_rgb(0xEA, 0xF1, 0xFE),
        blue_border: Color32::from_rgb(0x1E, 0x66, 0xF5),
    };

    pub const EVERFOREST_LIGHT: Self = Self {
        dark_mode: false,
        bg_canvas: Color32::from_rgb(0xF4, 0xF0, 0xD9),
        bg_surface: Color32::from_rgb(0xFD, 0xF6, 0xE3),
        bg_subtle: Color32::from_rgb(0xEF, 0xEB, 0xD4),
        text_primary: Color32::from_rgb(0x3A, 0x46, 0x4C),
        text_secondary: Color32::from_rgb(0x4E, 0x5B, 0x62),
        border_subtle: Color32::from_rgb(0xD8, 0xD3, 0xBA),
        border_interactive: Color32::from_rgb(0x63, 0x72, 0x6C),
        accent: Color32::from_rgb(0x3F, 0x6E, 0x1E),
        accent_hover: Color32::from_rgb(0x32, 0x58, 0x17),
        accent_text: Color32::from_rgb(0x39, 0x64, 0x1B),
        on_accent: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        emerald_text: Color32::from_rgb(0x1E, 0x68, 0x3D),
        emerald_bg: Color32::from_rgb(0xEC, 0xF8, 0xEE),
        emerald_border: Color32::from_rgb(0x23, 0x7A, 0x48),
        amber_text: Color32::from_rgb(0x8C, 0x4A, 0x05),
        amber_bg: Color32::from_rgb(0xFD, 0xF6, 0xE3),
        amber_border: Color32::from_rgb(0xA6, 0x59, 0x07),
        red_text: Color32::from_rgb(0xA8, 0x1E, 0x1B),
        red_bg: Color32::from_rgb(0xFD, 0xEF, 0xED),
        red_border: Color32::from_rgb(0xC1, 0x25, 0x22),
        blue_text: Color32::from_rgb(0x1E, 0x5E, 0x7A),
        blue_bg: Color32::from_rgb(0xEC, 0xF6, 0xFA),
        blue_border: Color32::from_rgb(0x23, 0x70, 0x90),
    };

    pub const GRUVBOX_MATERIAL_LIGHT: Self = Self {
        dark_mode: false,
        bg_canvas: Color32::from_rgb(0xF2, 0xE5, 0xBC),
        bg_surface: Color32::from_rgb(0xFB, 0xF1, 0xC7),
        bg_subtle: Color32::from_rgb(0xEB, 0xDB, 0xB2),
        text_primary: Color32::from_rgb(0x3C, 0x38, 0x36),
        text_secondary: Color32::from_rgb(0x50, 0x49, 0x45),
        border_subtle: Color32::from_rgb(0xD5, 0xC4, 0xA1),
        border_interactive: Color32::from_rgb(0x66, 0x5C, 0x54),
        accent: Color32::from_rgb(0xAF, 0x3A, 0x03),
        accent_hover: Color32::from_rgb(0x8F, 0x2F, 0x02),
        accent_text: Color32::from_rgb(0x8F, 0x2F, 0x02),
        on_accent: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        emerald_text: Color32::from_rgb(0x2F, 0x5E, 0x16),
        emerald_bg: Color32::from_rgb(0xF0, 0xF7, 0xE6),
        emerald_border: Color32::from_rgb(0x3A, 0x73, 0x1C),
        amber_text: Color32::from_rgb(0x7D, 0x42, 0x05),
        amber_bg: Color32::from_rgb(0xFB, 0xF1, 0xC7),
        amber_border: Color32::from_rgb(0x94, 0x50, 0x06),
        red_text: Color32::from_rgb(0x9D, 0x00, 0x06),
        red_bg: Color32::from_rgb(0xFC, 0xED, 0xEA),
        red_border: Color32::from_rgb(0xB5, 0x0E, 0x12),
        blue_text: Color32::from_rgb(0x07, 0x55, 0x67),
        blue_bg: Color32::from_rgb(0xEA, 0xF5, 0xF7),
        blue_border: Color32::from_rgb(0x09, 0x68, 0x7D),
    };

    pub const ONE_LIGHT: Self = Self {
        dark_mode: false,
        bg_canvas: Color32::from_rgb(0xF0, 0xF0, 0xF1),
        bg_surface: Color32::from_rgb(0xFA, 0xFA, 0xFA),
        bg_subtle: Color32::from_rgb(0xE5, 0xE5, 0xE6),
        text_primary: Color32::from_rgb(0x38, 0x3A, 0x42),
        text_secondary: Color32::from_rgb(0x4B, 0x4E, 0x59),
        border_subtle: Color32::from_rgb(0xD0, 0xD0, 0xD2),
        border_interactive: Color32::from_rgb(0x5E, 0x61, 0x6E),
        accent: Color32::from_rgb(0x2F, 0x5A, 0xF3),
        accent_hover: Color32::from_rgb(0x23, 0x47, 0xC7),
        accent_text: Color32::from_rgb(0x23, 0x47, 0xC7),
        on_accent: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        emerald_text: Color32::from_rgb(0x1E, 0x6B, 0x24),
        emerald_bg: Color32::from_rgb(0xEC, 0xF9, 0xED),
        emerald_border: Color32::from_rgb(0x25, 0x80, 0x2C),
        amber_text: Color32::from_rgb(0x8C, 0x4B, 0x05),
        amber_bg: Color32::from_rgb(0xFF, 0xFA, 0xEA),
        amber_border: Color32::from_rgb(0xA6, 0x59, 0x07),
        red_text: Color32::from_rgb(0xA6, 0x1D, 0x19),
        red_bg: Color32::from_rgb(0xFD, 0xEF, 0xEE),
        red_border: Color32::from_rgb(0xC2, 0x23, 0x1F),
        blue_text: Color32::from_rgb(0x23, 0x47, 0xC7),
        blue_bg: Color32::from_rgb(0xED, 0xF2, 0xFE),
        blue_border: Color32::from_rgb(0x2F, 0x5A, 0xF3),
    };

    pub const SOLARIZED_LIGHT: Self = Self {
        dark_mode: false,
        bg_canvas: Color32::from_rgb(0xF5, 0xEF, 0xDC),
        bg_surface: Color32::from_rgb(0xFD, 0xF6, 0xE3),
        bg_subtle: Color32::from_rgb(0xEE, 0xE8, 0xD5),
        text_primary: Color32::from_rgb(0x07, 0x36, 0x42),
        text_secondary: Color32::from_rgb(0x41, 0x55, 0x5C),
        border_subtle: Color32::from_rgb(0xD3, 0xCB, 0xB8),
        border_interactive: Color32::from_rgb(0x58, 0x6E, 0x75),
        accent: Color32::from_rgb(0x17, 0x64, 0x9E),
        accent_hover: Color32::from_rgb(0x11, 0x50, 0x7F),
        accent_text: Color32::from_rgb(0x14, 0x5A, 0x8E),
        on_accent: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        emerald_text: Color32::from_rgb(0x2F, 0x62, 0x00),
        emerald_bg: Color32::from_rgb(0xF0, 0xF8, 0xE6),
        emerald_border: Color32::from_rgb(0x3B, 0x7A, 0x00),
        amber_text: Color32::from_rgb(0x85, 0x4B, 0x00),
        amber_bg: Color32::from_rgb(0xFD, 0xF6, 0xE3),
        amber_border: Color32::from_rgb(0x9E, 0x59, 0x00),
        red_text: Color32::from_rgb(0xA6, 0x1C, 0x19),
        red_bg: Color32::from_rgb(0xFD, 0xEF, 0xEE),
        red_border: Color32::from_rgb(0xBF, 0x22, 0x1E),
        blue_text: Color32::from_rgb(0x14, 0x5A, 0x8E),
        blue_bg: Color32::from_rgb(0xEC, 0xF5, 0xFC),
        blue_border: Color32::from_rgb(0x17, 0x64, 0x9E),
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

    pub const LEGACY_DARK: Self = Self {
        dark_mode: true,
        bg_canvas: Color32::from_rgb(0x16, 0x16, 0x18),
        bg_surface: Color32::from_rgb(0x1E, 0x1E, 0x1E),
        bg_subtle: Color32::from_rgb(0x2A, 0x2A, 0x2D),
        text_primary: Color32::from_rgb(0xD4, 0xD4, 0xD4),
        text_secondary: Color32::from_rgb(0xA6, 0xA6, 0xAD),
        border_subtle: Color32::from_rgb(0x3C, 0x3C, 0x3C),
        border_interactive: Color32::from_rgb(0x85, 0x85, 0x8F),
        accent: Color32::from_rgb(0x37, 0x94, 0xFF),
        accent_hover: Color32::from_rgb(0x58, 0xA6, 0xFF),
        accent_text: Color32::from_rgb(0x58, 0xA6, 0xFF),
        on_accent: Color32::from_rgb(0x0D, 0x11, 0x17),
        emerald_text: Color32::from_rgb(0x6E, 0xE7, 0xB7),
        emerald_bg: Color32::from_rgb(0x06, 0x4E, 0x3B),
        emerald_border: Color32::from_rgb(0x34, 0xD3, 0x99),
        amber_text: Color32::from_rgb(0xFC, 0xD3, 0x4D),
        amber_bg: Color32::from_rgb(0x45, 0x1A, 0x03),
        amber_border: Color32::from_rgb(0xFB, 0xBF, 0x24),
        red_text: Color32::from_rgb(0xFC, 0xA5, 0xA5),
        red_bg: Color32::from_rgb(0x45, 0x0A, 0x0A),
        red_border: Color32::from_rgb(0xF8, 0x71, 0x71),
        blue_text: Color32::from_rgb(0xBF, 0xDB, 0xFE),
        blue_bg: Color32::from_rgb(0x1E, 0x3A, 0x8A),
        blue_border: Color32::from_rgb(0x60, 0xA5, 0xFA),
    };

    pub const CATPPUCCIN_DARK: Self = Self {
        dark_mode: true,
        bg_canvas: Color32::from_rgb(0x18, 0x18, 0x25),
        bg_surface: Color32::from_rgb(0x1E, 0x1E, 0x2E),
        bg_subtle: Color32::from_rgb(0x31, 0x32, 0x44),
        text_primary: Color32::from_rgb(0xCD, 0xD6, 0xF4),
        text_secondary: Color32::from_rgb(0xBA, 0xC2, 0xDE),
        border_subtle: Color32::from_rgb(0x45, 0x47, 0x5A),
        border_interactive: Color32::from_rgb(0x8B, 0x91, 0xB0),
        accent: Color32::from_rgb(0xCB, 0xA6, 0xF7),
        accent_hover: Color32::from_rgb(0xD8, 0xBA, 0xFA),
        accent_text: Color32::from_rgb(0xCB, 0xA6, 0xF7),
        on_accent: Color32::from_rgb(0x11, 0x11, 0x1B),
        emerald_text: Color32::from_rgb(0xA6, 0xE3, 0xA1),
        emerald_bg: Color32::from_rgb(0x06, 0x4E, 0x3B),
        emerald_border: Color32::from_rgb(0x34, 0xD3, 0x99),
        amber_text: Color32::from_rgb(0xF9, 0xE2, 0xAF),
        amber_bg: Color32::from_rgb(0x45, 0x1A, 0x03),
        amber_border: Color32::from_rgb(0xFB, 0xBF, 0x24),
        red_text: Color32::from_rgb(0xF3, 0x8B, 0xA8),
        red_bg: Color32::from_rgb(0x45, 0x0A, 0x0A),
        red_border: Color32::from_rgb(0xF8, 0x71, 0x71),
        blue_text: Color32::from_rgb(0xBF, 0xDB, 0xFE),
        blue_bg: Color32::from_rgb(0x1E, 0x3A, 0x8A),
        blue_border: Color32::from_rgb(0x89, 0xB4, 0xFA),
    };

    pub const DRACULA: Self = Self {
        dark_mode: true,
        bg_canvas: Color32::from_rgb(0x1E, 0x1F, 0x29),
        bg_surface: Color32::from_rgb(0x28, 0x2A, 0x36),
        bg_subtle: Color32::from_rgb(0x34, 0x37, 0x46),
        text_primary: Color32::from_rgb(0xF8, 0xF8, 0xF2),
        text_secondary: Color32::from_rgb(0xBF, 0xBF, 0xCE),
        border_subtle: Color32::from_rgb(0x44, 0x47, 0x5A),
        border_interactive: Color32::from_rgb(0x8E, 0x93, 0xB0),
        accent: Color32::from_rgb(0xBD, 0x93, 0xF9),
        accent_hover: Color32::from_rgb(0xCC, 0xAA, 0xFC),
        accent_text: Color32::from_rgb(0xBD, 0x93, 0xF9),
        on_accent: Color32::from_rgb(0x19, 0x1A, 0x21),
        emerald_text: Color32::from_rgb(0x6E, 0xE7, 0xB7),
        emerald_bg: Color32::from_rgb(0x06, 0x4E, 0x3B),
        emerald_border: Color32::from_rgb(0x34, 0xD3, 0x99),
        amber_text: Color32::from_rgb(0xF1, 0xFA, 0x8C),
        amber_bg: Color32::from_rgb(0x45, 0x1A, 0x03),
        amber_border: Color32::from_rgb(0xFB, 0xBF, 0x24),
        red_text: Color32::from_rgb(0xFC, 0xA5, 0xA5),
        red_bg: Color32::from_rgb(0x45, 0x0A, 0x0A),
        red_border: Color32::from_rgb(0xF8, 0x71, 0x71),
        blue_text: Color32::from_rgb(0x8B, 0xE9, 0xFD),
        blue_bg: Color32::from_rgb(0x1E, 0x3A, 0x8A),
        blue_border: Color32::from_rgb(0x8B, 0xE9, 0xFD),
    };

    pub const EVERFOREST_DARK: Self = Self {
        dark_mode: true,
        bg_canvas: Color32::from_rgb(0x23, 0x2A, 0x2E),
        bg_surface: Color32::from_rgb(0x2D, 0x35, 0x3B),
        bg_subtle: Color32::from_rgb(0x34, 0x3F, 0x44),
        text_primary: Color32::from_rgb(0xD3, 0xC6, 0xAA),
        text_secondary: Color32::from_rgb(0xB4, 0xBE, 0xB0),
        border_subtle: Color32::from_rgb(0x47, 0x52, 0x58),
        border_interactive: Color32::from_rgb(0x8E, 0x9D, 0x94),
        accent: Color32::from_rgb(0xA7, 0xC0, 0x80),
        accent_hover: Color32::from_rgb(0xB5, 0xCC, 0x92),
        accent_text: Color32::from_rgb(0xA7, 0xC0, 0x80),
        on_accent: Color32::from_rgb(0x1E, 0x23, 0x26),
        emerald_text: Color32::from_rgb(0x83, 0xC0, 0x92),
        emerald_bg: Color32::from_rgb(0x06, 0x4E, 0x3B),
        emerald_border: Color32::from_rgb(0x34, 0xD3, 0x99),
        amber_text: Color32::from_rgb(0xDB, 0xBC, 0x7F),
        amber_bg: Color32::from_rgb(0x45, 0x1A, 0x03),
        amber_border: Color32::from_rgb(0xFB, 0xBF, 0x24),
        red_text: Color32::from_rgb(0xFC, 0xA5, 0xA5),
        red_bg: Color32::from_rgb(0x45, 0x0A, 0x0A),
        red_border: Color32::from_rgb(0xF8, 0x71, 0x71),
        blue_text: Color32::from_rgb(0xBF, 0xDB, 0xFE),
        blue_bg: Color32::from_rgb(0x1E, 0x3A, 0x8A),
        blue_border: Color32::from_rgb(0x7F, 0xBB, 0xB3),
    };

    pub const GRUVBOX_MATERIAL_DARK: Self = Self {
        dark_mode: true,
        bg_canvas: Color32::from_rgb(0x1D, 0x20, 0x21),
        bg_surface: Color32::from_rgb(0x28, 0x28, 0x28),
        bg_subtle: Color32::from_rgb(0x32, 0x30, 0x2F),
        text_primary: Color32::from_rgb(0xD4, 0xBE, 0x98),
        text_secondary: Color32::from_rgb(0xBA, 0xAA, 0x8C),
        border_subtle: Color32::from_rgb(0x45, 0x40, 0x3D),
        border_interactive: Color32::from_rgb(0x92, 0x83, 0x74),
        accent: Color32::from_rgb(0xA9, 0xB6, 0x65),
        accent_hover: Color32::from_rgb(0xB8, 0xC4, 0x78),
        accent_text: Color32::from_rgb(0xA9, 0xB6, 0x65),
        on_accent: Color32::from_rgb(0x1D, 0x20, 0x21),
        emerald_text: Color32::from_rgb(0x98, 0xCC, 0x8E),
        emerald_bg: Color32::from_rgb(0x06, 0x4E, 0x3B),
        emerald_border: Color32::from_rgb(0x34, 0xD3, 0x99),
        amber_text: Color32::from_rgb(0xE8, 0xB8, 0x64),
        amber_bg: Color32::from_rgb(0x45, 0x1A, 0x03),
        amber_border: Color32::from_rgb(0xFB, 0xBF, 0x24),
        red_text: Color32::from_rgb(0xFC, 0xA5, 0xA5),
        red_bg: Color32::from_rgb(0x45, 0x0A, 0x0A),
        red_border: Color32::from_rgb(0xF8, 0x71, 0x71),
        blue_text: Color32::from_rgb(0xBF, 0xDB, 0xFE),
        blue_bg: Color32::from_rgb(0x1E, 0x3A, 0x8A),
        blue_border: Color32::from_rgb(0x7D, 0xAE, 0xA3),
    };

    pub const MONOKAI: Self = Self {
        dark_mode: true,
        bg_canvas: Color32::from_rgb(0x1E, 0x1F, 0x1C),
        bg_surface: Color32::from_rgb(0x27, 0x28, 0x22),
        bg_subtle: Color32::from_rgb(0x38, 0x38, 0x30),
        text_primary: Color32::from_rgb(0xF8, 0xF8, 0xF2),
        text_secondary: Color32::from_rgb(0xC2, 0xC2, 0xB4),
        border_subtle: Color32::from_rgb(0x49, 0x48, 0x3E),
        border_interactive: Color32::from_rgb(0x96, 0x95, 0x84),
        accent: Color32::from_rgb(0xA6, 0xE2, 0x2E),
        accent_hover: Color32::from_rgb(0xB7, 0xEB, 0x4B),
        accent_text: Color32::from_rgb(0xA6, 0xE2, 0x2E),
        on_accent: Color32::from_rgb(0x1E, 0x1F, 0x1C),
        emerald_text: Color32::from_rgb(0x6E, 0xE7, 0xB7),
        emerald_bg: Color32::from_rgb(0x06, 0x4E, 0x3B),
        emerald_border: Color32::from_rgb(0x34, 0xD3, 0x99),
        amber_text: Color32::from_rgb(0xE6, 0xDB, 0x74),
        amber_bg: Color32::from_rgb(0x45, 0x1A, 0x03),
        amber_border: Color32::from_rgb(0xFB, 0xBF, 0x24),
        red_text: Color32::from_rgb(0xFC, 0xA5, 0xA5),
        red_bg: Color32::from_rgb(0x45, 0x0A, 0x0A),
        red_border: Color32::from_rgb(0xF8, 0x71, 0x71),
        blue_text: Color32::from_rgb(0x66, 0xD9, 0xEF),
        blue_bg: Color32::from_rgb(0x1E, 0x3A, 0x8A),
        blue_border: Color32::from_rgb(0x66, 0xD9, 0xEF),
    };

    pub const NORD: Self = Self {
        dark_mode: true,
        bg_canvas: Color32::from_rgb(0x24, 0x29, 0x33),
        bg_surface: Color32::from_rgb(0x2E, 0x34, 0x40),
        bg_subtle: Color32::from_rgb(0x3B, 0x42, 0x52),
        text_primary: Color32::from_rgb(0xEC, 0xEF, 0xF4),
        text_secondary: Color32::from_rgb(0xC2, 0xCA, 0xDA),
        border_subtle: Color32::from_rgb(0x4C, 0x56, 0x6A),
        border_interactive: Color32::from_rgb(0x94, 0xA0, 0xB8),
        accent: Color32::from_rgb(0x88, 0xC0, 0xD0),
        accent_hover: Color32::from_rgb(0x9C, 0xCC, 0xDA),
        accent_text: Color32::from_rgb(0x88, 0xC0, 0xD0),
        on_accent: Color32::from_rgb(0x1E, 0x22, 0x2A),
        emerald_text: Color32::from_rgb(0xA3, 0xBE, 0x8C),
        emerald_bg: Color32::from_rgb(0x06, 0x4E, 0x3B),
        emerald_border: Color32::from_rgb(0x34, 0xD3, 0x99),
        amber_text: Color32::from_rgb(0xEB, 0xCB, 0x8B),
        amber_bg: Color32::from_rgb(0x45, 0x1A, 0x03),
        amber_border: Color32::from_rgb(0xFB, 0xBF, 0x24),
        red_text: Color32::from_rgb(0xFC, 0xA5, 0xA5),
        red_bg: Color32::from_rgb(0x45, 0x0A, 0x0A),
        red_border: Color32::from_rgb(0xF8, 0x71, 0x71),
        blue_text: Color32::from_rgb(0xBF, 0xDB, 0xFE),
        blue_bg: Color32::from_rgb(0x1E, 0x3A, 0x8A),
        blue_border: Color32::from_rgb(0x88, 0xC0, 0xD0),
    };

    pub const ONE_DARK_PRO: Self = Self {
        dark_mode: true,
        bg_canvas: Color32::from_rgb(0x21, 0x25, 0x2B),
        bg_surface: Color32::from_rgb(0x28, 0x2C, 0x34),
        bg_subtle: Color32::from_rgb(0x32, 0x38, 0x42),
        text_primary: Color32::from_rgb(0xDC, 0xDF, 0xE4),
        text_secondary: Color32::from_rgb(0xAB, 0xB2, 0xBF),
        border_subtle: Color32::from_rgb(0x4B, 0x52, 0x63),
        border_interactive: Color32::from_rgb(0x8C, 0x95, 0xA6),
        accent: Color32::from_rgb(0x61, 0xAF, 0xEF),
        accent_hover: Color32::from_rgb(0x7A, 0xBE, 0xF4),
        accent_text: Color32::from_rgb(0x61, 0xAF, 0xEF),
        on_accent: Color32::from_rgb(0x18, 0x1A, 0x1F),
        emerald_text: Color32::from_rgb(0x98, 0xC3, 0x79),
        emerald_bg: Color32::from_rgb(0x06, 0x4E, 0x3B),
        emerald_border: Color32::from_rgb(0x34, 0xD3, 0x99),
        amber_text: Color32::from_rgb(0xE5, 0xC0, 0x7B),
        amber_bg: Color32::from_rgb(0x45, 0x1A, 0x03),
        amber_border: Color32::from_rgb(0xFB, 0xBF, 0x24),
        red_text: Color32::from_rgb(0xFC, 0xA5, 0xA5),
        red_bg: Color32::from_rgb(0x45, 0x0A, 0x0A),
        red_border: Color32::from_rgb(0xF8, 0x71, 0x71),
        blue_text: Color32::from_rgb(0xBF, 0xDB, 0xFE),
        blue_bg: Color32::from_rgb(0x1E, 0x3A, 0x8A),
        blue_border: Color32::from_rgb(0x61, 0xAF, 0xEF),
    };

    pub const TOKYO_NIGHT: Self = Self {
        dark_mode: true,
        bg_canvas: Color32::from_rgb(0x16, 0x16, 0x1E),
        bg_surface: Color32::from_rgb(0x1A, 0x1B, 0x26),
        bg_subtle: Color32::from_rgb(0x24, 0x28, 0x3B),
        text_primary: Color32::from_rgb(0xC0, 0xCA, 0xF5),
        text_secondary: Color32::from_rgb(0x9A, 0xA5, 0xCE),
        border_subtle: Color32::from_rgb(0x3B, 0x42, 0x61),
        border_interactive: Color32::from_rgb(0x78, 0x82, 0xAA),
        accent: Color32::from_rgb(0x7A, 0xA2, 0xF7),
        accent_hover: Color32::from_rgb(0x8F, 0xB3, 0xFA),
        accent_text: Color32::from_rgb(0x7A, 0xA2, 0xF7),
        on_accent: Color32::from_rgb(0x15, 0x16, 0x1E),
        emerald_text: Color32::from_rgb(0x9E, 0xCE, 0x6A),
        emerald_bg: Color32::from_rgb(0x06, 0x4E, 0x3B),
        emerald_border: Color32::from_rgb(0x34, 0xD3, 0x99),
        amber_text: Color32::from_rgb(0xE0, 0xAF, 0x68),
        amber_bg: Color32::from_rgb(0x45, 0x1A, 0x03),
        amber_border: Color32::from_rgb(0xFB, 0xBF, 0x24),
        red_text: Color32::from_rgb(0xFC, 0xA5, 0xA5),
        red_bg: Color32::from_rgb(0x45, 0x0A, 0x0A),
        red_border: Color32::from_rgb(0xF8, 0x71, 0x71),
        blue_text: Color32::from_rgb(0xBF, 0xDB, 0xFE),
        blue_bg: Color32::from_rgb(0x1E, 0x3A, 0x8A),
        blue_border: Color32::from_rgb(0x7A, 0xA2, 0xF7),
    };

    pub const SOLARIZED_DARK: Self = Self {
        dark_mode: true,
        bg_canvas: Color32::from_rgb(0x00, 0x21, 0x2B),
        bg_surface: Color32::from_rgb(0x00, 0x2B, 0x36),
        bg_subtle: Color32::from_rgb(0x07, 0x36, 0x42),
        text_primary: Color32::from_rgb(0xFD, 0xF6, 0xE3),
        text_secondary: Color32::from_rgb(0x93, 0xA1, 0xA1),
        border_subtle: Color32::from_rgb(0x28, 0x4B, 0x54),
        border_interactive: Color32::from_rgb(0x6D, 0x87, 0x8C),
        accent: Color32::from_rgb(0x2E, 0xB8, 0xAC),
        accent_hover: Color32::from_rgb(0x45, 0xC9, 0xBE),
        accent_text: Color32::from_rgb(0x3B, 0xC4, 0xB8),
        on_accent: Color32::from_rgb(0x00, 0x1E, 0x26),
        emerald_text: Color32::from_rgb(0x6E, 0xE7, 0xB7),
        emerald_bg: Color32::from_rgb(0x06, 0x4E, 0x3B),
        emerald_border: Color32::from_rgb(0x34, 0xD3, 0x99),
        amber_text: Color32::from_rgb(0xFC, 0xD3, 0x4D),
        amber_bg: Color32::from_rgb(0x45, 0x1A, 0x03),
        amber_border: Color32::from_rgb(0xFB, 0xBF, 0x24),
        red_text: Color32::from_rgb(0xFC, 0xA5, 0xA5),
        red_bg: Color32::from_rgb(0x45, 0x0A, 0x0A),
        red_border: Color32::from_rgb(0xF8, 0x71, 0x71),
        blue_text: Color32::from_rgb(0xBF, 0xDB, 0xFE),
        blue_bg: Color32::from_rgb(0x1E, 0x3A, 0x8A),
        blue_border: Color32::from_rgb(0x60, 0xA5, 0xFA),
    };

    pub const VESPER: Self = Self {
        dark_mode: true,
        bg_canvas: Color32::from_rgb(0x0B, 0x0B, 0x0B),
        bg_surface: Color32::from_rgb(0x10, 0x10, 0x10),
        bg_subtle: Color32::from_rgb(0x1C, 0x1C, 0x1C),
        text_primary: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        text_secondary: Color32::from_rgb(0xA0, 0xA0, 0xA0),
        border_subtle: Color32::from_rgb(0x2E, 0x2E, 0x2E),
        border_interactive: Color32::from_rgb(0x76, 0x76, 0x76),
        accent: Color32::from_rgb(0xFF, 0xC7, 0x99),
        accent_hover: Color32::from_rgb(0xFF, 0xD4, 0xB0),
        accent_text: Color32::from_rgb(0xFF, 0xC7, 0x99),
        on_accent: Color32::from_rgb(0x10, 0x10, 0x10),
        emerald_text: Color32::from_rgb(0x99, 0xFF, 0xE4),
        emerald_bg: Color32::from_rgb(0x06, 0x4E, 0x3B),
        emerald_border: Color32::from_rgb(0x34, 0xD3, 0x99),
        amber_text: Color32::from_rgb(0xFF, 0xC7, 0x99),
        amber_bg: Color32::from_rgb(0x45, 0x1A, 0x03),
        amber_border: Color32::from_rgb(0xFB, 0xBF, 0x24),
        red_text: Color32::from_rgb(0xFF, 0x80, 0x80),
        red_bg: Color32::from_rgb(0x45, 0x0A, 0x0A),
        red_border: Color32::from_rgb(0xF8, 0x71, 0x71),
        blue_text: Color32::from_rgb(0xBF, 0xDB, 0xFE),
        blue_bg: Color32::from_rgb(0x1E, 0x3A, 0x8A),
        blue_border: Color32::from_rgb(0x60, 0xA5, 0xFA),
    };

    /// Return the static Light `Palette` corresponding to `preset`.
    pub const fn light_for_preset(preset: LightThemePreset) -> &'static Palette {
        match preset {
            LightThemePreset::DefaultLight => &Self::LIGHT,
            LightThemePreset::LegacyLight => &Self::LEGACY_LIGHT,
            LightThemePreset::Catppuccin => &Self::CATPPUCCIN_LIGHT,
            LightThemePreset::Everforest => &Self::EVERFOREST_LIGHT,
            LightThemePreset::GruvboxMaterial => &Self::GRUVBOX_MATERIAL_LIGHT,
            LightThemePreset::OneLight => &Self::ONE_LIGHT,
            LightThemePreset::SolarizedLight => &Self::SOLARIZED_LIGHT,
        }
    }

    /// Return the static Dark `Palette` corresponding to `preset`.
    pub const fn dark_for_preset(preset: DarkThemePreset) -> &'static Palette {
        match preset {
            DarkThemePreset::DefaultDark => &Self::DARK,
            DarkThemePreset::LegacyDark => &Self::LEGACY_DARK,
            DarkThemePreset::Catppuccin => &Self::CATPPUCCIN_DARK,
            DarkThemePreset::Dracula => &Self::DRACULA,
            DarkThemePreset::Everforest => &Self::EVERFOREST_DARK,
            DarkThemePreset::GruvboxMaterial => &Self::GRUVBOX_MATERIAL_DARK,
            DarkThemePreset::Monokai => &Self::MONOKAI,
            DarkThemePreset::Nord => &Self::NORD,
            DarkThemePreset::OneDarkPro => &Self::ONE_DARK_PRO,
            DarkThemePreset::TokyoNight => &Self::TOKYO_NIGHT,
            DarkThemePreset::SolarizedDark => &Self::SOLARIZED_DARK,
            DarkThemePreset::Vesper => &Self::VESPER,
        }
    }

    /// Format an opaque `Color32` as an uppercase 6-digit hex string (e.g. `"#2D353B"`).
    pub fn hex_rgb(c: Color32) -> String {
        format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
    }
}

/// Shared spacing, radius, accessibility, and responsive layout constants from `DESIGN.md`.
pub struct ThemeTokens;

impl ThemeTokens {
    pub const SPACE_XS: f32 = 4.0;
    pub const SPACE_SM: f32 = 8.0;
    pub const SPACE_MD: f32 = 12.0;
    pub const SPACE_LG: f32 = 16.0;
    pub const SPACE_XL: f32 = 20.0;
    pub const SPACE_2XL: f32 = 24.0;

    pub const RADIUS_MODAL: f32 = 14.0;
    pub const RADIUS_CARD: f32 = 10.0;
    pub const RADIUS_CONTROL: f32 = 8.0;
    pub const RADIUS_TAG: f32 = 6.0;

    pub const MIN_TARGET_HEIGHT: f32 = 40.0;
    pub const FOCUS_STROKE_WIDTH: f32 = 2.0;

    pub const MAX_CONTAINER_WIDTH: f32 = 1280.0;
    pub const BREAKPOINT_WIDE: f32 = 860.0;
    pub const BREAKPOINT_HEADER_STACK: f32 = 880.0;
    pub const BREAKPOINT_MEDIUM: f32 = 600.0;

    pub const FONT_SEMIBOLD_FAMILY: &'static str = "semibold";

    pub fn resolve_palette(pref: ThemePreference, system_dark: bool) -> &'static Palette {
        Self::resolve_appearance(AppearanceConfig::from(pref), system_dark)
    }

    pub fn resolve_appearance(appearance: AppearanceConfig, system_dark: bool) -> &'static Palette {
        match appearance.mode {
            ThemePreference::Light => Palette::light_for_preset(appearance.light_preset),
            ThemePreference::Dark => Palette::dark_for_preset(appearance.dark_preset),
            ThemePreference::System => {
                if system_dark {
                    Palette::dark_for_preset(appearance.dark_preset)
                } else {
                    Palette::light_for_preset(appearance.light_preset)
                }
            }
        }
    }
}

/// Resolve whether the UI is currently dark for `pref` using the OS theme reported by `egui`.
pub fn effective_dark_mode(ctx: &egui::Context, pref: ThemePreference) -> bool {
    match pref {
        ThemePreference::Light => false,
        ThemePreference::Dark => true,
        ThemePreference::System => ctx
            .input(|i| i.raw.system_theme)
            .map(|t| t == egui::Theme::Dark)
            .unwrap_or(false),
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
    apply_appearance(ctx, AppearanceConfig::from(pref))
}

/// Apply the full `AppearanceConfig` (mode plus Light/Dark theme presets) to an `egui::Context`.
pub fn apply_appearance(ctx: &egui::Context, appearance: AppearanceConfig) -> &'static Palette {
    let system_dark = ctx
        .input(|i| i.raw.system_theme)
        .map(|t| t == egui::Theme::Dark)
        .unwrap_or(false);
    let palette = ThemeTokens::resolve_appearance(appearance, system_dark);

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
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.5_f32, palette.accent_text);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, palette.text_primary);
    visuals.widgets.hovered.rounding = control_rounding;

    visuals.widgets.active.bg_fill = palette.accent;
    visuals.widgets.active.weak_bg_fill = palette.accent_hover;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, palette.accent_text);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, palette.on_accent);
    visuals.widgets.active.rounding = control_rounding;

    visuals.widgets.open.bg_fill = palette.bg_subtle;
    visuals.widgets.open.weak_bg_fill = palette.bg_subtle;
    visuals.widgets.open.bg_stroke = Stroke::new(1.5_f32, palette.accent_text);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, palette.text_primary);
    visuals.widgets.open.rounding = control_rounding;

    ctx.style_mut(|style| {
        style.visuals = visuals;
        style.spacing.item_spacing = egui::vec2(ThemeTokens::SPACE_SM, ThemeTokens::SPACE_SM);
        style.spacing.button_padding = egui::vec2(ThemeTokens::SPACE_MD, ThemeTokens::SPACE_SM);
    });

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
    use std::path::{Path, PathBuf};

    fn assert_text_bg(fg: Color32, bg: Color32, label: &str) {
        let ratio = contrast_ratio(fg, bg);
        assert!(
            ratio >= 4.5,
            "{label}: contrast {ratio:.4} is below WCAG AA 4.5"
        );
    }

    fn assert_border_bg(fg: Color32, bg: Color32, label: &str) {
        let ratio = contrast_ratio(fg, bg);
        assert!(
            ratio >= 3.0,
            "{label}: contrast {ratio:.4} is below WCAG AA UI 3.0"
        );
    }

    fn assert_palette_wcag(p: &Palette, name: &str) {
        for (bg_name, bg) in [
            ("bg_canvas", p.bg_canvas),
            ("bg_surface", p.bg_surface),
            ("bg_subtle", p.bg_subtle),
        ] {
            assert_text_bg(
                p.text_primary,
                bg,
                &format!("{name} text_primary/{bg_name}"),
            );
            assert_text_bg(
                p.text_secondary,
                bg,
                &format!("{name} text_secondary/{bg_name}"),
            );
            assert_text_bg(p.accent_text, bg, &format!("{name} accent_text/{bg_name}"));

            assert_border_bg(
                p.border_interactive,
                bg,
                &format!("{name} border_interactive/{bg_name}"),
            );
            // Focus ring uses accent_text.
            assert_border_bg(p.accent_text, bg, &format!("{name} focus_ring/{bg_name}"));
        }

        assert_text_bg(p.on_accent, p.accent, &format!("{name} on_accent/accent"));
        assert_text_bg(
            p.on_accent,
            p.accent_hover,
            &format!("{name} on_accent/accent_hover"),
        );

        for (label, text, bg, border) in [
            ("emerald", p.emerald_text, p.emerald_bg, p.emerald_border),
            ("amber", p.amber_text, p.amber_bg, p.amber_border),
            ("red", p.red_text, p.red_bg, p.red_border),
            ("blue", p.blue_text, p.blue_bg, p.blue_border),
        ] {
            assert_text_bg(text, bg, &format!("{name} {label} text/bg"));
            assert_text_bg(
                text,
                p.bg_surface,
                &format!("{name} {label} text/bg_surface"),
            );
            assert_text_bg(text, p.bg_canvas, &format!("{name} {label} text/bg_canvas"));
            assert_border_bg(border, bg, &format!("{name} {label} border/bg"));
            assert_border_bg(
                border,
                p.bg_surface,
                &format!("{name} {label} border/bg_surface"),
            );
            assert_border_bg(
                border,
                p.bg_canvas,
                &format!("{name} {label} border/bg_canvas"),
            );
        }
    }

    #[test]
    fn light_and_dark_palettes_pass_r25_wcag_thresholds() {
        assert_palette_wcag(&Palette::LIGHT, "LIGHT");
        assert_palette_wcag(&Palette::DARK, "DARK");
    }

    #[test]
    fn all_19_antigravity_light_and_dark_presets_pass_r25_wcag_thresholds() {
        assert_eq!(LightThemePreset::ALL.len(), 7);
        for preset in LightThemePreset::ALL {
            let palette = Palette::light_for_preset(preset);
            assert!(!palette.dark_mode, "expected light mode for {:?}", preset);
            assert_palette_wcag(palette, &format!("Light::{}", preset.label()));
        }

        assert_eq!(DarkThemePreset::ALL.len(), 12);
        for preset in DarkThemePreset::ALL {
            let palette = Palette::dark_for_preset(preset);
            assert!(palette.dark_mode, "expected dark mode for {:?}", preset);
            assert_palette_wcag(palette, &format!("Dark::{}", preset.label()));
        }

        // Verify Everforest Dark canonical token hex strings match Antigravity appearance reference
        let everforest = Palette::dark_for_preset(DarkThemePreset::Everforest);
        assert_eq!(Palette::hex_rgb(everforest.bg_surface), "#2D353B");
        assert_eq!(Palette::hex_rgb(everforest.text_primary), "#D3C6AA");
        assert_eq!(Palette::hex_rgb(everforest.accent), "#A7C080");

        let cfg = AppearanceConfig {
            mode: ThemePreference::System,
            light_preset: LightThemePreset::SolarizedLight,
            dark_preset: DarkThemePreset::Everforest,
        };
        assert_eq!(
            ThemeTokens::resolve_appearance(cfg, false),
            &Palette::SOLARIZED_LIGHT
        );
        assert_eq!(
            ThemeTokens::resolve_appearance(cfg, true),
            &Palette::EVERFOREST_DARK
        );
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

    fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect_rs_files(&path, out);
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    out.push(path);
                }
            }
        }
    }

    #[test]
    fn r02_zero_em_dash_or_double_hyphen_in_gui_sources_and_design_doc() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        collect_rs_files(&manifest_dir.join("src"), &mut files);
        collect_rs_files(&manifest_dir.join("tests"), &mut files);
        let design_doc = manifest_dir.join("..").join("..").join("DESIGN.md");
        if design_doc.is_file() {
            files.push(design_doc);
        }
        assert!(!files.is_empty(), "expected files to scan for R-02");

        let em_dash = '\u{2014}';
        let double_hyphen = [" ", "-", "-", " "].concat();

        for path in files {
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed reading {}: {e}", path.display()));
            assert!(
                !content.contains(em_dash),
                "R-02 violation: found U+2014 em dash in {}",
                path.display()
            );
            assert!(
                !content.contains(&double_hyphen),
                "R-02 violation: found double-hyphen separator in {}",
                path.display()
            );
        }
    }
}

//! Slim top window control bar for Linux tiling and borderless compositors/WMs.

use egui::{Color32, CursorIcon, FontFamily, Pos2, Rect, Rounding, Sense, Vec2};

use crate::theme::{contrast_ratio, Palette, ThemeTokens};
use crate::ui::{icons, paint_focus_ring_in_rect};

/// Height of the custom top window control bar in logical pixels.
pub const TITLEBAR_HEIGHT: f32 = 32.0;
/// Width of each top-right window control button (`Minimize`, `Maximize/Restore`, `Close`).
pub const CONTROL_BUTTON_WIDTH: f32 = 42.0;

/// High-contrast danger red fill on Close hover (`4.83:1` >= WCAG AA `4.5:1` with `#FFFFFF`).
pub const CLOSE_HOVER_BG: Color32 = Color32::from_rgb(0xDC, 0x26, 0x26);
/// Pressed danger red fill on Close (`5.91:1` >= WCAG AA `4.5:1` with `#FFFFFF`).
pub const CLOSE_PRESSED_BG: Color32 = Color32::from_rgb(0xB9, 0x1C, 0x1C);

/// Resolve the Close button hover fill so `#FFFFFF` stroke always meets WCAG AA `>= 4.5:1`.
pub fn close_hover_bg(palette: &Palette) -> Color32 {
    if contrast_ratio(Color32::WHITE, palette.red_border) >= 4.5 {
        palette.red_border
    } else {
        CLOSE_HOVER_BG
    }
}

/// User action emitted by the custom window control bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitlebarAction {
    StartDrag,
    Minimize,
    ToggleMaximize,
    Close,
}

#[derive(Debug, Clone, Copy)]
enum ControlKind {
    Minimize,
    Maximize { is_maximized: bool },
    Close { minimize_to_tray_active: bool },
}

/// Render the slim `32px` top window control bar with left drag region and flush
/// top-right `Minimize`, `Maximize/Restore`, and `Close` vector buttons.
pub fn show(
    ctx: &egui::Context,
    palette: &Palette,
    is_maximized: bool,
    minimize_to_tray_active: bool,
) -> Option<TitlebarAction> {
    let mut action = None;

    egui::TopBottomPanel::top("linux_window_titlebar")
        .exact_height(TITLEBAR_HEIGHT)
        .resizable(false)
        .show_separator_line(true)
        .frame(
            egui::Frame::none()
                .fill(palette.bg_surface)
                .inner_margin(egui::Margin::ZERO),
        )
        .show(ctx, |ui| {
            let full_rect = ui.available_rect_before_wrap();
            let controls_w = CONTROL_BUTTON_WIDTH * 3.0;
            let drag_w = (full_rect.width() - controls_w).max(0.0);

            ui.spacing_mut().item_spacing = Vec2::ZERO;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;

                if drag_w > 0.0 {
                    let (drag_rect, drag_resp) = ui.allocate_exact_size(
                        Vec2::new(drag_w, TITLEBAR_HEIGHT),
                        Sense::click_and_drag(),
                    );
                    if ui.is_rect_visible(drag_rect) && drag_rect.width() >= 120.0 {
                        let galley = ui.painter().layout_no_wrap(
                            "Polban Presensi".to_string(),
                            egui::FontId::new(
                                12.0,
                                FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()),
                            ),
                            palette.text_secondary,
                        );
                        let pos = Pos2::new(
                            drag_rect.left() + ThemeTokens::SPACE_MD,
                            drag_rect.center().y - galley.size().y * 0.5,
                        );
                        ui.painter().galley(pos, galley, palette.text_secondary);
                    }
                    if drag_resp.drag_started_by(egui::PointerButton::Primary) {
                        action = Some(TitlebarAction::StartDrag);
                    } else if drag_resp.double_clicked() {
                        action = Some(TitlebarAction::ToggleMaximize);
                    }
                }

                for kind in [
                    ControlKind::Minimize,
                    ControlKind::Maximize { is_maximized },
                    ControlKind::Close {
                        minimize_to_tray_active,
                    },
                ] {
                    if let Some(btn_action) = render_control_button(ui, palette, kind) {
                        action = Some(btn_action);
                    }
                }
            });
        });

    action
}

fn render_control_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    kind: ControlKind,
) -> Option<TitlebarAction> {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(CONTROL_BUTTON_WIDTH, TITLEBAR_HEIGHT),
        Sense::click(),
    );
    let response = response.on_hover_cursor(CursorIcon::PointingHand);

    let (tooltip, emitted) = match kind {
        ControlKind::Minimize => ("Perkecil", TitlebarAction::Minimize),
        ControlKind::Maximize { is_maximized } => (
            if is_maximized {
                "Kembalikan Ukuran"
            } else {
                "Perbesar"
            },
            TitlebarAction::ToggleMaximize,
        ),
        ControlKind::Close {
            minimize_to_tray_active,
        } => (
            if minimize_to_tray_active {
                "Sembunyikan ke System Tray"
            } else {
                "Tutup Aplikasi"
            },
            TitlebarAction::Close,
        ),
    };

    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let pressed = response.is_pointer_button_down_on();
        let (fill, fg) = match kind {
            ControlKind::Close { .. } => {
                if pressed {
                    (CLOSE_PRESSED_BG, Color32::WHITE)
                } else if hovered {
                    (close_hover_bg(palette), Color32::WHITE)
                } else {
                    (Color32::TRANSPARENT, palette.text_secondary)
                }
            }
            ControlKind::Minimize | ControlKind::Maximize { .. } => {
                if pressed || hovered {
                    (palette.bg_subtle, palette.text_primary)
                } else {
                    (Color32::TRANSPARENT, palette.text_secondary)
                }
            }
        };

        if fill != Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, Rounding::ZERO, fill);
        }

        let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(10.0));
        match kind {
            ControlKind::Minimize => icons::draw_win_minimize(ui.painter(), icon_rect, fg),
            ControlKind::Maximize { is_maximized } => {
                if is_maximized {
                    icons::draw_win_restore(ui.painter(), icon_rect, fg);
                } else {
                    icons::draw_win_maximize(ui.painter(), icon_rect, fg);
                }
            }
            ControlKind::Close { .. } => icons::draw_win_close(ui.painter(), icon_rect, fg),
        }

        if response.has_focus() {
            paint_focus_ring_in_rect(ui, rect.shrink(2.0), palette, 4.0);
        }
    }

    let clicked = response.clicked();
    response.on_hover_text(tooltip);
    if clicked {
        Some(emitted)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::{DarkThemePreset, LightThemePreset};

    fn assert_titlebar_wcag(palette: &Palette, name: &str) {
        let hover_red = close_hover_bg(palette);
        assert!(
            contrast_ratio(palette.text_secondary, palette.bg_surface) >= 4.5,
            "{name}: idle icon contrast below 4.5"
        );
        assert!(
            contrast_ratio(palette.text_primary, palette.bg_subtle) >= 4.5,
            "{name}: hover icon contrast below 4.5"
        );
        assert!(
            contrast_ratio(Color32::WHITE, hover_red) >= 4.5,
            "{name}: close hover icon contrast below 4.5"
        );
        assert!(
            contrast_ratio(Color32::WHITE, CLOSE_PRESSED_BG) >= 4.5,
            "{name}: close pressed icon contrast below 4.5"
        );
        assert!(
            contrast_ratio(palette.accent_text, palette.bg_surface) >= 3.0,
            "{name}: focus ring contrast below 3.0"
        );
    }

    #[test]
    fn all_19_presets_meet_titlebar_wcag_aa_contrast() {
        for preset in LightThemePreset::ALL {
            assert_titlebar_wcag(
                Palette::light_for_preset(preset),
                &format!("Light::{}", preset.label()),
            );
        }
        for preset in DarkThemePreset::ALL {
            assert_titlebar_wcag(
                Palette::dark_for_preset(preset),
                &format!("Dark::{}", preset.label()),
            );
        }
    }
}

//! Desktop GUI view components and shared Calm Academic Utility primitives.

pub mod activity;
pub mod header;
pub mod icons;
pub mod jadwal;
pub mod settings;

use std::borrow::Cow;

use egui::{Color32, CursorIcon, FontFamily, Pos2, Rect, RichText, Rounding, Sense, Stroke, Vec2};

use crate::theme::{Palette, ThemeTokens};

/// Active tab when viewport width is in the Compact breakpoint (`< 580px`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompactTab {
    #[default]
    Jadwal,
    Activity,
}

/// Semantic color category mapped to WCAG-verified `Palette` triplets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticTone {
    Emerald,
    Amber,
    Red,
    Blue,
    Neutral,
}

impl SemanticTone {
    /// Return `(text, background, border)` colors from `palette`.
    pub fn colors(self, palette: &Palette) -> (Color32, Color32, Color32) {
        match self {
            Self::Emerald => (
                palette.emerald_text,
                palette.emerald_bg,
                palette.emerald_border,
            ),
            Self::Amber => (palette.amber_text, palette.amber_bg, palette.amber_border),
            Self::Red => (palette.red_text, palette.red_bg, palette.red_border),
            Self::Blue => (palette.blue_text, palette.blue_bg, palette.blue_border),
            Self::Neutral => (
                palette.text_secondary,
                palette.bg_subtle,
                palette.border_interactive,
            ),
        }
    }
}

/// Visual emphasis variant for interactive buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonTone {
    Primary,
    Secondary,
    Danger,
}

/// Sanitize any legacy double-hyphen separator into `": "` per R-02.
pub fn sanitize_dash(text: &str) -> Cow<'_, str> {
    let needle = [" ", "-", "-", " "].concat();
    if text.contains(&needle) || text.contains('\u{2014}') {
        Cow::Owned(text.replace(&needle, ": ").replace('\u{2014}', ": "))
    } else {
        Cow::Borrowed(text)
    }
}

/// SemiBold UI heading/label text helper.
pub fn semibold_text(text: impl Into<String>, size: f32, color: Color32) -> RichText {
    RichText::new(text.into())
        .family(FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()))
        .size(size)
        .color(color)
}

/// Monospace numeric/code text helper (`JetBrains Mono`).
pub fn mono_text(text: impl Into<String>, size: f32, color: Color32) -> RichText {
    RichText::new(text.into())
        .family(FontFamily::Monospace)
        .size(size)
        .color(color)
}

/// Draw a high-contrast 2px keyboard focus ring around `response` when focused.
pub fn paint_focus_ring(
    ui: &egui::Ui,
    response: &egui::Response,
    palette: &Palette,
    rounding: f32,
) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(2.0),
            Rounding::same(rounding + 1.5),
            Stroke::new(ThemeTokens::FOCUS_STROKE_WIDTH, palette.accent_text),
        );
    }
}

/// Render an accessible action button with minimum 44px height, optional vector icon,
/// clear disabled appearance, pointing/not-allowed cursor, and keyboard focus ring.
pub fn action_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    tone: ButtonTone,
    label: &str,
    icon_fn: Option<fn(&egui::Painter, Rect, Color32)>,
    enabled: bool,
    min_width: f32,
) -> egui::Response {
    let inner = ui.add_enabled_ui(enabled, |ui| {
        let text_size = 13.5_f32;
        let galley = ui.painter().layout_no_wrap(
            label.to_string(),
            egui::FontId::new(
                text_size,
                FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()),
            ),
            palette.text_primary,
        );
        let icon_w = if icon_fn.is_some() { 16.0 } else { 0.0 };
        let icon_gap = if icon_fn.is_some() && !label.is_empty() {
            ThemeTokens::SPACE_SM
        } else {
            0.0
        };
        let pad_x = if label.is_empty() {
            ThemeTokens::SPACE_MD
        } else {
            ThemeTokens::SPACE_LG
        };
        let desired_w = (pad_x * 2.0 + icon_w + icon_gap + galley.size().x).max(min_width);
        let desired_size = Vec2::new(desired_w, ThemeTokens::MIN_TARGET_HEIGHT);

        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());
        let response = if enabled {
            response.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            if ui.rect_contains_pointer(rect) {
                ui.ctx().set_cursor_icon(CursorIcon::NotAllowed);
            }
            response
        };

        if ui.is_rect_visible(rect) {
            let hovered = enabled && response.hovered();
            let pressed = enabled && response.is_pointer_button_down_on();
            let (fill, border, fg) = if !enabled {
                (
                    palette.bg_subtle,
                    Stroke::new(1.0_f32, palette.border_subtle),
                    palette.text_secondary,
                )
            } else {
                match tone {
                    ButtonTone::Primary => {
                        let bg = if pressed || hovered {
                            palette.accent_hover
                        } else {
                            palette.accent
                        };
                        (bg, Stroke::new(1.0_f32, bg), palette.on_accent)
                    }
                    ButtonTone::Secondary => {
                        let bg = if pressed || hovered {
                            palette.bg_subtle
                        } else {
                            palette.bg_surface
                        };
                        let stroke_color = if hovered {
                            palette.accent_text
                        } else {
                            palette.border_interactive
                        };
                        (bg, Stroke::new(1.2_f32, stroke_color), palette.text_primary)
                    }
                    ButtonTone::Danger => {
                        let bg = if hovered || pressed {
                            palette.red_bg
                        } else {
                            palette.bg_surface
                        };
                        (
                            bg,
                            Stroke::new(1.2_f32, palette.red_border),
                            palette.red_text,
                        )
                    }
                }
            };

            let rounding = Rounding::same(ThemeTokens::RADIUS_CONTROL);
            ui.painter().rect(rect, rounding, fill, border);

            let content_w = icon_w + icon_gap + galley.size().x;
            let start_x = rect.center().x - content_w * 0.5;
            if let Some(draw_icon) = icon_fn {
                let icon_rect = Rect::from_center_size(
                    Pos2::new(start_x + icon_w * 0.5, rect.center().y),
                    Vec2::splat(icon_w),
                );
                draw_icon(ui.painter(), icon_rect, fg);
            }
            if !label.is_empty() {
                let text_galley = ui.painter().layout_no_wrap(
                    label.to_string(),
                    egui::FontId::new(
                        text_size,
                        FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()),
                    ),
                    fg,
                );
                let text_pos = Pos2::new(
                    start_x + icon_w + icon_gap,
                    rect.center().y - text_galley.size().y * 0.5,
                );
                ui.painter().galley(text_pos, text_galley, fg);
            }
            paint_focus_ring(ui, &response, palette, ThemeTokens::RADIUS_CONTROL);
        }

        response
    });
    inner.inner
}

/// Render a 4px-radius status badge pairing a vector icon and text label (never color alone).
pub fn status_badge(
    ui: &mut egui::Ui,
    palette: &Palette,
    tone: SemanticTone,
    icon_fn: fn(&egui::Painter, Rect, Color32),
    label: &str,
) {
    let (fg, bg, border) = tone.colors(palette);
    egui::Frame::none()
        .fill(bg)
        .stroke(Stroke::new(1.0_f32, border))
        .rounding(Rounding::same(ThemeTokens::RADIUS_TAG))
        .inner_margin(egui::Margin::symmetric(
            ThemeTokens::SPACE_SM,
            ThemeTokens::SPACE_XS,
        ))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = ThemeTokens::SPACE_XS;
                icons::paint_icon(ui, 13.0, fg, icon_fn);
                ui.label(semibold_text(label, 11.5, fg));
            });
        });
}

/// Render a labeled single-line text input with >= 44px height, 6px rounding, and focus ring.
pub fn labeled_text_input(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    value: &mut String,
    hint: &str,
    monospace: bool,
    width: f32,
) -> egui::Response {
    ui.label(semibold_text(label, 12.5, palette.text_primary));
    ui.add_space(ThemeTokens::SPACE_XS);
    let mut edit = egui::TextEdit::singleline(value)
        .hint_text(RichText::new(hint).color(palette.text_secondary))
        .desired_width(width)
        .margin(egui::Margin::symmetric(ThemeTokens::SPACE_MD, 12.0));
    if monospace {
        edit = edit.font(egui::FontId::new(13.0, FontFamily::Monospace));
    } else {
        edit = edit.font(egui::FontId::new(13.0, FontFamily::Proportional));
    }
    let response = ui
        .add_sized(Vec2::new(width, ThemeTokens::MIN_TARGET_HEIGHT), edit)
        .on_hover_cursor(CursorIcon::Text);
    paint_focus_ring(ui, &response, palette, ThemeTokens::RADIUS_CONTROL);
    response
}

/// Render a full-row accessible toggle control with >= 44px hit area and focus ring.
pub fn toggle_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    checked: &mut bool,
    label: &str,
    helper: Option<&str>,
) -> egui::Response {
    let row_w = ui.available_width();
    let row_h = if helper.is_some() {
        54.0
    } else {
        ThemeTokens::MIN_TARGET_HEIGHT
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(row_w, row_h), Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    if response.clicked() {
        *checked = !*checked;
    }

    if ui.is_rect_visible(rect) {
        let fill = if response.hovered() {
            palette.bg_subtle
        } else {
            palette.bg_surface
        };
        let border_col = if response.hovered() {
            palette.accent_text
        } else {
            palette.border_subtle
        };
        ui.painter().rect(
            rect,
            Rounding::same(ThemeTokens::RADIUS_CONTROL),
            fill,
            Stroke::new(1.0_f32, border_col),
        );

        let box_rect = Rect::from_center_size(
            Pos2::new(rect.left() + ThemeTokens::SPACE_LG + 9.0, rect.center().y),
            Vec2::splat(18.0),
        );
        let (box_fill, box_border) = if *checked {
            (palette.accent, palette.accent)
        } else {
            (palette.bg_surface, palette.border_interactive)
        };
        ui.painter().rect(
            box_rect,
            Rounding::same(ThemeTokens::RADIUS_TAG),
            box_fill,
            Stroke::new(1.5_f32, box_border),
        );
        if *checked {
            icons::draw_check(ui.painter(), box_rect, palette.on_accent);
        }

        let text_left = box_rect.right() + ThemeTokens::SPACE_MD;
        let max_text_w = (rect.right() - ThemeTokens::SPACE_MD - text_left).max(40.0);
        let label_galley = ui.painter().layout(
            label.to_string(),
            egui::FontId::new(13.0, FontFamily::Proportional),
            palette.text_primary,
            max_text_w,
        );
        if let Some(help) = helper {
            let help_galley = ui.painter().layout(
                help.to_string(),
                egui::FontId::new(11.5, FontFamily::Proportional),
                palette.text_secondary,
                max_text_w,
            );
            let total_h = label_galley.size().y + 2.0 + help_galley.size().y;
            let top_y = rect.center().y - total_h * 0.5;
            ui.painter().galley(
                Pos2::new(text_left, top_y),
                label_galley.clone(),
                palette.text_primary,
            );
            ui.painter().galley(
                Pos2::new(text_left, top_y + label_galley.size().y + 2.0),
                help_galley,
                palette.text_secondary,
            );
        } else {
            let y = rect.center().y - label_galley.size().y * 0.5;
            ui.painter()
                .galley(Pos2::new(text_left, y), label_galley, palette.text_primary);
        }

        paint_focus_ring(ui, &response, palette, ThemeTokens::RADIUS_CONTROL);
    }

    response
}

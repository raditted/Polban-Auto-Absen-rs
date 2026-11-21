//! Desktop GUI view components and shared Calm Academic Utility primitives.

pub mod activity;
pub mod header;
pub mod icons;
pub mod jadwal;
pub mod proof;
pub mod settings;

use std::borrow::Cow;

use egui::{Color32, CursorIcon, FontFamily, Pos2, Rect, RichText, Rounding, Sense, Stroke, Vec2};

use crate::theme::{Palette, ThemeTokens};

/// Active tab when viewport width is below the wide split breakpoint (kept for API compatibility).
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
                palette.border_subtle,
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
    Ghost,
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

/// Draw a full-width 1px hairline horizontal divider.
pub fn hairline_divider(ui: &mut egui::Ui, color: Color32) {
    let avail_w = ui.available_width().max(1.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, 1.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().line_segment(
            [
                Pos2::new(rect.left(), rect.center().y),
                Pos2::new(rect.right(), rect.center().y),
            ],
            Stroke::new(1.0_f32, color),
        );
    }
}

/// Draw a high-contrast 2px keyboard focus ring around `response` when focused.
pub fn paint_focus_ring(
    ui: &mut egui::Ui,
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

/// Render an accessible action button with consistent height, optional vector icon,
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
        let text_size = 13.0_f32;
        let galley = ui.painter().layout_no_wrap(
            label.to_string(),
            egui::FontId::new(
                text_size,
                FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()),
            ),
            palette.text_primary,
        );
        let icon_w = if icon_fn.is_some() { 15.0 } else { 0.0 };
        let icon_gap = if icon_fn.is_some() && !label.is_empty() {
            ThemeTokens::SPACE_SM
        } else {
            0.0
        };
        let desired_w = if label.is_empty() {
            ThemeTokens::MIN_TARGET_HEIGHT.max(min_width)
        } else {
            (ThemeTokens::SPACE_LG * 2.0 + icon_w + icon_gap + galley.size().x).max(min_width)
        };
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
                        (bg, Stroke::new(1.1_f32, stroke_color), palette.text_primary)
                    }
                    ButtonTone::Danger => {
                        let bg = if hovered || pressed {
                            palette.red_bg
                        } else {
                            palette.bg_surface
                        };
                        (
                            bg,
                            Stroke::new(1.1_f32, palette.red_border),
                            palette.red_text,
                        )
                    }
                    ButtonTone::Ghost => {
                        let bg = if hovered || pressed {
                            palette.bg_subtle
                        } else {
                            Color32::TRANSPARENT
                        };
                        (bg, Stroke::NONE, palette.text_secondary)
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

/// Compute the rendered width of `status_badge` so callers can reserve exact space without overlap.
pub fn status_badge_width(ui: &egui::Ui, label: &str) -> f32 {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::new(
            11.5,
            FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()),
        ),
        Color32::WHITE,
    );
    // Horizontal padding (2 * 8) + icon (12) + gap (5) + text + safety margin (2)
    ThemeTokens::SPACE_SM * 2.0 + 12.0 + 5.0 + galley.size().x + 2.0
}

/// Render a compact pill status badge pairing a vector icon and text label (never color alone).
pub fn status_badge(
    ui: &mut egui::Ui,
    palette: &Palette,
    tone: SemanticTone,
    icon_fn: fn(&egui::Painter, Rect, Color32),
    label: &str,
) -> egui::Response {
    let (fg, bg, border) = tone.colors(palette);
    egui::Frame::none()
        .fill(bg)
        .stroke(Stroke::new(1.0_f32, border))
        .rounding(Rounding::same(ThemeTokens::RADIUS_TAG))
        .inner_margin(egui::Margin::symmetric(ThemeTokens::SPACE_SM, 4.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                icons::paint_icon(ui, 12.0, fg, icon_fn);
                ui.label(semibold_text(label, 11.5, fg));
            });
        })
        .response
}

/// Render a labeled single-line text input with consistent 40px height, 8px rounding, and focus ring.
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
    ui.add_space(5.0);
    let mut edit = egui::TextEdit::singleline(value)
        .hint_text(RichText::new(hint).color(palette.text_secondary))
        .desired_width(width)
        .margin(egui::Margin::symmetric(ThemeTokens::SPACE_MD, 10.0));
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

/// Function pointer signature for procedural vector icon painters.
pub type IconPaintFn = fn(&egui::Painter, Rect, Color32);

/// Render a 3-way pill segmented control; returns the newly selected index if clicked.
pub fn segmented_control<const N: usize>(
    ui: &mut egui::Ui,
    palette: &Palette,
    selected: usize,
    options: [(&str, IconPaintFn); N],
) -> Option<usize> {
    let mut clicked_idx = None;
    let total_w = ui.available_width();
    let pad = 4.0;
    let inner_w = (total_w - pad * 2.0).max(120.0);
    let seg_w = inner_w / (N as f32);
    let seg_h = 34.0;

    egui::Frame::none()
        .fill(palette.bg_subtle)
        .stroke(Stroke::new(1.0_f32, palette.border_subtle))
        .rounding(Rounding::same(ThemeTokens::RADIUS_CARD))
        .inner_margin(egui::Margin::same(pad))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (idx, (label, icon_fn)) in options.into_iter().enumerate() {
                    let is_active = idx == selected;
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::new(seg_w, seg_h), Sense::click());
                    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
                    if resp.clicked() {
                        clicked_idx = Some(idx);
                    }
                    if ui.is_rect_visible(rect) {
                        let (fill, border, fg) = if is_active {
                            (
                                palette.bg_surface,
                                Stroke::new(1.0_f32, palette.border_interactive),
                                palette.accent_text,
                            )
                        } else if resp.hovered() {
                            (
                                palette.bg_surface.linear_multiply(0.7),
                                Stroke::NONE,
                                palette.text_primary,
                            )
                        } else {
                            (Color32::TRANSPARENT, Stroke::NONE, palette.text_secondary)
                        };
                        ui.painter().rect(
                            rect,
                            Rounding::same(ThemeTokens::RADIUS_CONTROL - 1.0),
                            fill,
                            border,
                        );

                        let galley = ui.painter().layout_no_wrap(
                            label.to_string(),
                            egui::FontId::new(
                                12.5,
                                FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()),
                            ),
                            fg,
                        );
                        let icon_w = 14.0;
                        let gap = 6.0;
                        let total_content = icon_w + gap + galley.size().x;
                        let start_x = rect.center().x - total_content * 0.5;
                        let icon_rect = Rect::from_center_size(
                            Pos2::new(start_x + icon_w * 0.5, rect.center().y),
                            Vec2::splat(icon_w),
                        );
                        icon_fn(ui.painter(), icon_rect, fg);
                        ui.painter().galley(
                            Pos2::new(
                                start_x + icon_w + gap,
                                rect.center().y - galley.size().y * 0.5,
                            ),
                            galley,
                            fg,
                        );
                        paint_focus_ring(ui, &resp, palette, ThemeTokens::RADIUS_CONTROL - 1.0);
                    }
                }
            });
        });

    clicked_idx
}

/// Render a clean settings toggle row with a custom pill switch on the right (no outer card border).
pub fn switch_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    checked: &mut bool,
    label: &str,
) -> egui::Response {
    let row_w = ui.available_width();
    let row_h = 44.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(row_w, row_h), Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    if response.clicked() {
        *checked = !*checked;
    }

    if ui.is_rect_visible(rect) {
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, Rounding::ZERO, palette.bg_subtle);
        }

        let switch_w = 38.0;
        let switch_h = 22.0;
        let right_pad = ThemeTokens::SPACE_MD;
        let left_pad = ThemeTokens::SPACE_MD;
        let switch_rect = Rect::from_center_size(
            Pos2::new(rect.right() - right_pad - switch_w * 0.5, rect.center().y),
            Vec2::new(switch_w, switch_h),
        );

        let max_text_w =
            (switch_rect.left() - ThemeTokens::SPACE_MD - (rect.left() + left_pad)).max(60.0);
        let label_galley = ui.painter().layout(
            label.to_string(),
            egui::FontId::new(13.0, FontFamily::Proportional),
            palette.text_primary,
            max_text_w,
        );
        let text_y = rect.center().y - label_galley.size().y * 0.5;
        ui.painter().galley(
            Pos2::new(rect.left() + left_pad, text_y),
            label_galley,
            palette.text_primary,
        );

        let track_fill = if *checked {
            palette.accent
        } else {
            palette.border_interactive
        };
        ui.painter()
            .rect_filled(switch_rect, Rounding::same(switch_h * 0.5), track_fill);

        let knob_r = 8.0;
        let knob_x = if *checked {
            switch_rect.right() - 3.0 - knob_r
        } else {
            switch_rect.left() + 3.0 + knob_r
        };
        ui.painter().circle_filled(
            Pos2::new(knob_x, switch_rect.center().y),
            knob_r,
            Color32::WHITE,
        );

        paint_focus_ring(ui, &response, palette, ThemeTokens::RADIUS_CONTROL);
    }

    response
}

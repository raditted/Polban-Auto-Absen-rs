//! Activity history panel displaying recent monitoring and attendance events.

use egui::{RichText, Rounding, Stroke, Vec2};

use crate::bridge::{ActivityItem, ActivityKind, GuiSnapshot};
use crate::theme::{Palette, ThemeTokens};
use crate::ui::{icons, mono_text, sanitize_dash, semibold_text, status_badge, SemanticTone};

fn activity_meta(
    kind: ActivityKind,
) -> (
    SemanticTone,
    fn(&egui::Painter, egui::Rect, egui::Color32),
    &'static str,
) {
    match kind {
        ActivityKind::Ok => (SemanticTone::Emerald, icons::draw_check, "Berhasil"),
        ActivityKind::Fail => (SemanticTone::Red, icons::draw_alert, "Gagal"),
        ActivityKind::Warn => (SemanticTone::Amber, icons::draw_alert, "Peringatan"),
        ActivityKind::Info => (SemanticTone::Blue, icons::draw_refresh, "Info"),
        ActivityKind::Skip => (SemanticTone::Neutral, icons::draw_clock, "Lewati"),
    }
}

pub fn show(
    ui: &mut egui::Ui,
    snapshot: &GuiSnapshot,
    palette: &Palette,
    compact: bool,
    scrollable: bool,
) {
    egui::Frame::none()
        .fill(palette.bg_surface)
        .stroke(Stroke::new(1.0_f32, palette.border_subtle))
        .rounding(Rounding::same(ThemeTokens::RADIUS_CARD))
        .inner_margin(egui::Margin::same(if compact {
            ThemeTokens::SPACE_MD
        } else {
            ThemeTokens::SPACE_LG
        }))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            ui.horizontal(|ui| {
                ui.label(semibold_text(
                    "Riwayat Aktivitas",
                    14.5,
                    palette.text_primary,
                ));
                if !snapshot.activities.is_empty() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{} catatan", snapshot.activities.len()))
                                .size(12.0)
                                .color(palette.text_secondary),
                        );
                    });
                }
            });

            ui.add_space(ThemeTokens::SPACE_MD);

            if snapshot.activities.is_empty() {
                egui::Frame::none()
                    .fill(palette.bg_subtle)
                    .stroke(Stroke::new(1.0_f32, palette.border_subtle))
                    .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
                    .inner_margin(egui::Margin::same(ThemeTokens::SPACE_LG))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new("Belum ada aktivitas pemantauan pada sesi ini.")
                                .size(13.0)
                                .color(palette.text_secondary),
                        );
                    });
            } else if scrollable {
                egui::ScrollArea::vertical()
                    .id_salt("activity_list_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        render_activity_list(ui, &snapshot.activities, palette);
                    });
            } else {
                render_activity_list(ui, &snapshot.activities, palette);
            }
        });
}

fn render_activity_list(ui: &mut egui::Ui, items: &[ActivityItem], palette: &Palette) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(ThemeTokens::SPACE_SM);
        for item in items.iter().rev() {
            render_activity_row(ui, item, palette);
        }
    });
}

fn render_activity_row(ui: &mut egui::Ui, item: &ActivityItem, palette: &Palette) {
    let (tone, icon_fn, kind_label) = activity_meta(item.kind);
    let clean_msg = sanitize_dash(&item.message);
    let clean_code = item.code.trim_matches(|c| c == '[' || c == ']').trim();

    egui::Frame::none()
        .fill(palette.bg_subtle)
        .stroke(Stroke::new(1.0_f32, palette.border_subtle))
        .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
        .inner_margin(egui::Margin::same(ThemeTokens::SPACE_MD))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::splat(ThemeTokens::SPACE_SM);
                status_badge(ui, palette, tone, icon_fn, kind_label);
                if !item.timestamp.trim().is_empty() {
                    ui.label(mono_text(&item.timestamp, 12.0, palette.text_secondary));
                }
                if !clean_code.is_empty() {
                    ui.label(mono_text(clean_code, 11.5, palette.text_secondary));
                }
            });
            ui.add_space(ThemeTokens::SPACE_XS);
            ui.label(
                RichText::new(clean_msg.as_ref())
                    .size(12.5)
                    .color(palette.text_primary),
            );
        });
}

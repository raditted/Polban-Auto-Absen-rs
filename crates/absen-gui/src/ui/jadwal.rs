//! Schedule card panel displaying Polban courses and attendance status badges.

use egui::{FontFamily, RichText};

use crate::bridge::GuiSnapshot;
use crate::theme::{Palette, ThemeTokens};

pub fn show(ui: &mut egui::Ui, snapshot: &GuiSnapshot, palette: &Palette, _compact: bool) {
    egui::Frame::none()
        .fill(palette.bg_surface)
        .stroke(egui::Stroke::new(1.0_f32, palette.border_subtle))
        .rounding(ThemeTokens::RADIUS_CARD)
        .inner_margin(egui::Margin::same(ThemeTokens::SPACE_LG))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new("Jadwal Kuliah")
                    .family(FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()))
                    .size(14.0)
                    .color(palette.text_primary),
            );
            ui.add_space(ThemeTokens::SPACE_SM);

            if snapshot.courses.is_empty() {
                ui.label(
                    RichText::new("Belum ada data jadwal. Tekan Mulai untuk memindai.")
                        .size(13.0)
                        .color(palette.text_secondary),
                );
            } else {
                for course in &snapshot.courses {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&course.nama_mk)
                                .size(13.0)
                                .color(palette.text_primary),
                        );
                        ui.label(
                            RichText::new(&course.jam)
                                .family(FontFamily::Monospace)
                                .size(12.5)
                                .color(palette.text_secondary),
                        );
                        ui.label(
                            RichText::new(&course.status_label)
                                .size(11.5)
                                .color(palette.accent_text),
                        );
                    });
                }
            }
        });
}

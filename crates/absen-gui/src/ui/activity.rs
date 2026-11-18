//! Activity log panel displaying recent engine events.

use egui::{FontFamily, RichText};

use crate::bridge::GuiSnapshot;
use crate::theme::{Palette, ThemeTokens};

pub fn show(ui: &mut egui::Ui, snapshot: &GuiSnapshot, palette: &Palette) {
    egui::Frame::none()
        .fill(palette.bg_surface)
        .stroke(egui::Stroke::new(1.0_f32, palette.border_subtle))
        .rounding(ThemeTokens::RADIUS_CARD)
        .inner_margin(egui::Margin::same(ThemeTokens::SPACE_LG))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new("Aktivitas")
                    .family(FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()))
                    .size(14.0)
                    .color(palette.text_primary),
            );
            ui.add_space(ThemeTokens::SPACE_SM);

            if snapshot.activities.is_empty() {
                ui.label(
                    RichText::new("Belum ada aktivitas tercatat.")
                        .size(13.0)
                        .color(palette.text_secondary),
                );
            } else {
                for item in &snapshot.activities {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(&item.timestamp)
                                .family(FontFamily::Monospace)
                                .size(12.0)
                                .color(palette.text_secondary),
                        );
                        ui.label(
                            RichText::new(&item.message)
                                .size(12.5)
                                .color(palette.text_primary),
                        );
                    });
                }
            }
        });
}

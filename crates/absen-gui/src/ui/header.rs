//! Header bar showing identity, live monitoring status, countdown, and primary actions.

use egui::{FontFamily, RichText};

use crate::bridge::GuiSnapshot;
use crate::theme::{Palette, ThemeTokens};

/// Actions emitted by the header bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderAction {
    StartMonitoring,
    StopMonitoring,
    ScanNow,
    OpenSettings,
    CycleTheme,
}

pub fn show(
    ui: &mut egui::Ui,
    snapshot: &GuiSnapshot,
    palette: &Palette,
    width: f32,
) -> Option<HeaderAction> {
    let mut action = None;
    let compact = width < ThemeTokens::BREAKPOINT_MEDIUM;

    egui::Frame::none()
        .fill(palette.bg_surface)
        .stroke(egui::Stroke::new(1.0_f32, palette.border_subtle))
        .rounding(ThemeTokens::RADIUS_CARD)
        .inner_margin(egui::Margin::same(if compact {
            ThemeTokens::SPACE_MD
        } else {
            ThemeTokens::SPACE_LG
        }))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("Polban Presensi")
                            .family(FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()))
                            .size(18.0)
                            .color(palette.text_primary),
                    );
                    let subtitle = match &snapshot.user {
                        Some(u) if !u.nama.is_empty() => format!("{} ({})", u.nama, u.nim),
                        _ => snapshot.status.label().to_string(),
                    };
                    ui.label(
                        RichText::new(subtitle)
                            .size(12.5)
                            .color(palette.text_secondary),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Pengaturan").clicked() {
                        action = Some(HeaderAction::OpenSettings);
                    }
                    if ui.button("Tema").clicked() {
                        action = Some(HeaderAction::CycleTheme);
                    }
                    if snapshot.status.is_active() {
                        if ui.button("Scan Sekarang").clicked() {
                            action = Some(HeaderAction::ScanNow);
                        }
                        if ui.button("Berhenti").clicked() {
                            action = Some(HeaderAction::StopMonitoring);
                        }
                    } else if ui.button("Mulai").clicked() {
                        action = Some(HeaderAction::StartMonitoring);
                    }

                    ui.label(
                        RichText::new(format!("{}s", snapshot.countdown_secs))
                            .family(FontFamily::Monospace)
                            .size(16.0)
                            .color(palette.accent_text),
                    );
                });
            });
        });

    action
}

//! Schedule panel displaying today's Polban courses, status badges, loading, error, and empty states.

use absen_core::text::{jam_ringkas, nama_dosen};
use absen_core::Color;
use egui::{RichText, Rounding, Stroke, Vec2};

use crate::bridge::{CourseItem, EngineStatus, GuiSnapshot};
use crate::theme::{Palette, ThemeTokens};
use crate::ui::{
    action_button, icons, mono_text, sanitize_dash, semibold_text, status_badge, ButtonTone,
    SemanticTone,
};

/// Recovery or monitoring action triggered from the schedule panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JadwalAction {
    OpenSettings,
    RetryConnection,
    StartMonitoring,
    ScanNow,
}

fn course_badge_meta(
    color: Color,
) -> (
    SemanticTone,
    fn(&egui::Painter, egui::Rect, egui::Color32),
    &'static str,
) {
    match color {
        Color::Green => (
            SemanticTone::Emerald,
            icons::draw_check,
            color.status_label(),
        ),
        Color::Blue => (SemanticTone::Blue, icons::draw_play, color.status_label()),
        Color::Orange => (SemanticTone::Amber, icons::draw_clock, color.status_label()),
        Color::Red => (SemanticTone::Red, icons::draw_alert, color.status_label()),
        Color::Unknown => (
            SemanticTone::Neutral,
            icons::draw_alert,
            color.status_label(),
        ),
    }
}

pub fn show(
    ui: &mut egui::Ui,
    snapshot: &GuiSnapshot,
    palette: &Palette,
    compact: bool,
    scrollable: bool,
) -> Option<JadwalAction> {
    let mut action = None;

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

            ui.horizontal_wrapped(|ui| {
                ui.label(semibold_text(
                    "Jadwal Kuliah Hari Ini",
                    14.5,
                    palette.text_primary,
                ));
                if !snapshot.courses.is_empty() {
                    let ready_count = snapshot
                        .courses
                        .iter()
                        .filter(|c| c.color == Color::Blue)
                        .count();
                    let summary = if ready_count > 0 {
                        format!(
                            "{} matkul, {} siap absen",
                            snapshot.courses.len(),
                            ready_count
                        )
                    } else {
                        format!("{} matkul", snapshot.courses.len())
                    };
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(summary)
                                .size(12.0)
                                .color(palette.text_secondary),
                        );
                    });
                }
            });

            ui.add_space(ThemeTokens::SPACE_MD);

            if matches!(
                snapshot.status,
                EngineStatus::AuthFailed | EngineStatus::NetworkError
            ) {
                show_error_banner(ui, snapshot, palette, &mut action);
                if !snapshot.courses.is_empty() {
                    ui.add_space(ThemeTokens::SPACE_MD);
                }
            }

            if snapshot.courses.is_empty() {
                if matches!(
                    snapshot.status,
                    EngineStatus::Connecting | EngineStatus::ScanningNow
                ) && snapshot.last_error.is_none()
                {
                    show_loading_state(ui, palette);
                } else if !matches!(
                    snapshot.status,
                    EngineStatus::AuthFailed | EngineStatus::NetworkError
                ) {
                    show_empty_state(ui, snapshot, palette, &mut action);
                }
            } else if scrollable {
                egui::ScrollArea::vertical()
                    .id_salt("jadwal_list_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        render_course_list(ui, &snapshot.courses, palette, compact);
                    });
            } else {
                render_course_list(ui, &snapshot.courses, palette, compact);
            }
        });

    action
}

fn show_error_banner(
    ui: &mut egui::Ui,
    snapshot: &GuiSnapshot,
    palette: &Palette,
    action: &mut Option<JadwalAction>,
) {
    let is_auth = snapshot.status == EngineStatus::AuthFailed;
    let title = if is_auth {
        "Autentikasi Portal Gagal"
    } else {
        "Gangguan Jaringan ke Portal"
    };
    let detail = snapshot
        .last_error
        .as_deref()
        .map(|s| sanitize_dash(s).into_owned())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            if is_auth {
                "NIM atau kata sandi ditolak oleh portal akademik. Periksa kredensial Anda."
                    .to_string()
            } else {
                "Tidak dapat menjangkau akademik.polban.ac.id. Periksa koneksi internet Anda."
                    .to_string()
            }
        });

    egui::Frame::none()
        .fill(palette.red_bg)
        .stroke(Stroke::new(1.5_f32, palette.red_border))
        .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
        .inner_margin(egui::Margin::same(ThemeTokens::SPACE_MD))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                icons::paint_icon(ui, 16.0, palette.red_text, icons::draw_alert);
                ui.label(semibold_text(title, 13.5, palette.red_text));
            });
            ui.add_space(ThemeTokens::SPACE_XS);
            ui.label(RichText::new(detail).size(12.5).color(palette.red_text));
            ui.add_space(ThemeTokens::SPACE_MD);

            if is_auth {
                if action_button(
                    ui,
                    palette,
                    ButtonTone::Primary,
                    "Periksa Kredensial",
                    Some(icons::draw_settings),
                    true,
                    0.0,
                )
                .clicked()
                {
                    *action = Some(JadwalAction::OpenSettings);
                }
            } else if action_button(
                ui,
                palette,
                ButtonTone::Primary,
                "Coba Lagi Sekarang",
                Some(icons::draw_refresh),
                true,
                0.0,
            )
            .clicked()
            {
                *action = Some(JadwalAction::RetryConnection);
            }
        });
}

fn show_loading_state(ui: &mut egui::Ui, palette: &Palette) {
    egui::Frame::none()
        .fill(palette.bg_subtle)
        .stroke(Stroke::new(1.0_f32, palette.border_subtle))
        .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
        .inner_margin(egui::Margin::same(ThemeTokens::SPACE_LG))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spinner();
                ui.add_space(ThemeTokens::SPACE_SM);
                ui.label(
                    RichText::new(
                        "Menghubungkan ke akademik.polban.ac.id dan memuat jadwal perkuliahan hari ini...",
                    )
                    .size(13.0)
                    .color(palette.text_primary),
                );
            });
        });
}

fn show_empty_state(
    ui: &mut egui::Ui,
    snapshot: &GuiSnapshot,
    palette: &Palette,
    action: &mut Option<JadwalAction>,
) {
    let has_scanned = !snapshot.last_scan.is_empty() || snapshot.status == EngineStatus::Monitoring;

    egui::Frame::none()
        .fill(palette.bg_subtle)
        .stroke(Stroke::new(1.0_f32, palette.border_subtle))
        .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
        .inner_margin(egui::Margin::same(ThemeTokens::SPACE_LG))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let message = if has_scanned {
                "Tidak ada jadwal perkuliahan terdaftar untuk hari ini di portal akademik."
            } else {
                "Pemantauan belum dijalankan. Tekan Mulai Pantau untuk memuat jadwal perkuliahan hari ini."
            };
            ui.label(
                RichText::new(message)
                    .size(13.0)
                    .color(palette.text_secondary),
            );
            ui.add_space(ThemeTokens::SPACE_MD);

            if snapshot.status.is_active() {
                if action_button(
                    ui,
                    palette,
                    ButtonTone::Secondary,
                    "Cek Sekarang",
                    Some(icons::draw_refresh),
                    true,
                    0.0,
                )
                .clicked()
                {
                    *action = Some(JadwalAction::ScanNow);
                }
            } else if action_button(
                ui,
                palette,
                ButtonTone::Primary,
                "Mulai Pantau",
                Some(icons::draw_play),
                true,
                0.0,
            )
            .clicked()
            {
                *action = Some(JadwalAction::StartMonitoring);
            }
        });
}

fn render_course_list(ui: &mut egui::Ui, courses: &[CourseItem], palette: &Palette, compact: bool) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(ThemeTokens::SPACE_SM);
        for course in courses {
            render_course_card(ui, course, palette, compact);
        }
    });
}

fn render_course_card(ui: &mut egui::Ui, course: &CourseItem, palette: &Palette, compact: bool) {
    let (tone, badge_icon, default_label) = course_badge_meta(course.color);
    let badge_label = if course.status_label.trim().is_empty() {
        default_label
    } else {
        course.status_label.as_str()
    };
    let card_border = if course.color == Color::Blue {
        palette.blue_border
    } else {
        palette.border_subtle
    };
    let card_fill = if course.color == Color::Blue {
        palette.bg_surface
    } else {
        palette.bg_subtle
    };
    let clean_dosen = if course.dosen.trim().is_empty() {
        String::new()
    } else {
        nama_dosen(&course.dosen)
    };
    let time_display = if compact {
        jam_ringkas(&course.jam).into_owned()
    } else {
        course.jam.clone()
    };

    egui::Frame::none()
        .fill(card_fill)
        .stroke(Stroke::new(1.0_f32, card_border))
        .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
        .inner_margin(egui::Margin::same(ThemeTokens::SPACE_MD))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            if compact {
                ui.label(semibold_text(&course.nama_mk, 13.5, palette.text_primary));
                if !clean_dosen.is_empty() {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(&clean_dosen)
                            .size(12.0)
                            .color(palette.text_secondary),
                    );
                }
                ui.add_space(ThemeTokens::SPACE_SM);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(ThemeTokens::SPACE_SM);
                    if !time_display.is_empty() {
                        ui.label(mono_text(&time_display, 12.5, palette.text_primary));
                    }
                    status_badge(ui, palette, tone, badge_icon, badge_label);
                });
            } else {
                ui.horizontal(|ui| {
                    let avail = ui.available_width();
                    let right_reserve = 200.0_f32.min(avail * 0.42);
                    let left_w = (avail - right_reserve - ThemeTokens::SPACE_MD).max(140.0);

                    ui.allocate_ui(Vec2::new(left_w, 0.0), |ui| {
                        ui.vertical(|ui| {
                            ui.label(semibold_text(&course.nama_mk, 13.5, palette.text_primary));
                            if !clean_dosen.is_empty() {
                                ui.add_space(2.0);
                                ui.label(
                                    RichText::new(&clean_dosen)
                                        .size(12.0)
                                        .color(palette.text_secondary),
                                );
                            }
                        });
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        status_badge(ui, palette, tone, badge_icon, badge_label);
                        if !time_display.is_empty() {
                            ui.add_space(ThemeTokens::SPACE_SM);
                            ui.label(mono_text(&time_display, 12.5, palette.text_primary));
                        }
                    });
                });
            }

            if !course.reason.trim().is_empty() {
                ui.add_space(ThemeTokens::SPACE_XS);
                ui.label(
                    RichText::new(sanitize_dash(&course.reason).into_owned())
                        .size(11.5)
                        .color(palette.text_secondary),
                );
            }
        });
}

//! Schedule panel displaying today's Polban courses, status badges, loading, error, and empty states.

use absen_core::text::nama_dosen;
use absen_core::Color;
use egui::{Align, Layout, RichText, Rounding, Stroke, Vec2};

use crate::bridge::{CourseItem, EngineStatus, GuiSnapshot};
use crate::theme::{Palette, ThemeTokens};
use crate::ui::{
    action_button, icons, mono_text, sanitize_dash, semibold_text, status_badge,
    status_badge_width, ButtonTone, SemanticTone,
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
        Color::Green => (SemanticTone::Emerald, icons::draw_check, "Sudah Absen"),
        Color::Blue => (SemanticTone::Blue, icons::draw_play, "Siap Absen"),
        Color::Orange => (SemanticTone::Amber, icons::draw_clock, "Belum Buka"),
        Color::Red => (SemanticTone::Neutral, icons::draw_clock, "Belum Waktunya"),
        Color::Unknown => (SemanticTone::Neutral, icons::draw_clock, "Dilewati"),
    }
}

/// Format raw schedule time strings like `"07:00:00 s.d. 08:40:00"` into `"07:00 \u{2013} 08:40"`.
fn format_clean_time(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let normalized = trimmed
        .replace(" s.d. ", " - ")
        .replace(" s/d ", " - ")
        .replace(" - ", "|");
    if let Some((start, end)) = normalized.split_once('|') {
        let s = trim_seconds(start.trim());
        let e = trim_seconds(end.trim());
        return format!("{s} \u{2013} {e}");
    }
    trim_seconds(trimmed).to_string()
}

fn trim_seconds(t: &str) -> &str {
    let bytes = t.as_bytes();
    if bytes.len() == 8 && bytes[2] == b':' && bytes[5] == b':' {
        &t[..5]
    } else {
        t
    }
}

/// Return a concise, non-redundant note for the card footer only when helpful.
fn helpful_course_note(course: &CourseItem) -> Option<String> {
    match course.color {
        Color::Green => None,
        Color::Blue => Some("Sesi presensi terbuka".to_string()),
        Color::Orange => Some("Dosen belum membuka sesi".to_string()),
        Color::Red => Some("Di luar jam presensi".to_string()),
        Color::Unknown => Some("Dilewati demi keamanan".to_string()),
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
                    15.0,
                    palette.text_primary,
                ));
                if !snapshot.courses.is_empty() {
                    let green_count = snapshot
                        .courses
                        .iter()
                        .filter(|c| c.color == Color::Green)
                        .count();
                    let ready_count = snapshot
                        .courses
                        .iter()
                        .filter(|c| c.color == Color::Blue)
                        .count();
                    let summary = if ready_count > 0 {
                        format!(
                            "{}/{} hadir · {} siap absen",
                            green_count,
                            snapshot.courses.len(),
                            ready_count
                        )
                    } else {
                        format!("{}/{} hadir", green_count, snapshot.courses.len())
                    };
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
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
                        render_course_list(ui, &snapshot.courses, palette);
                    });
            } else {
                render_course_list(ui, &snapshot.courses, palette);
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
        .stroke(Stroke::new(1.2_f32, palette.red_border))
        .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
        .inner_margin(egui::Margin::same(ThemeTokens::SPACE_MD))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                icons::paint_icon(ui, 15.0, palette.red_text, icons::draw_alert);
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

fn render_course_list(ui: &mut egui::Ui, courses: &[CourseItem], palette: &Palette) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(ThemeTokens::SPACE_SM);
        for course in courses {
            render_course_card(ui, course, palette);
        }
    });
}

fn render_course_card(ui: &mut egui::Ui, course: &CourseItem, palette: &Palette) {
    let (tone, badge_icon, default_label) = course_badge_meta(course.color);
    let badge_label = default_label;
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
    let time_display = format_clean_time(&course.jam);
    let note = helpful_course_note(course);

    egui::Frame::none()
        .fill(card_fill)
        .stroke(Stroke::new(1.0_f32, card_border))
        .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
        .inner_margin(egui::Margin::same(ThemeTokens::SPACE_MD))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // Row 1: Title strictly constrained to remaining width + semantic pill pinned right.
            let avail = ui.available_width();
            let badge_w = status_badge_width(ui, badge_label);
            let gap = ThemeTokens::SPACE_MD;
            let title_w = (avail - badge_w - gap).max(96.0);

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                ui.allocate_ui_with_layout(
                    Vec2::new(title_w, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_max_width(title_w);
                        let resp = ui.add(
                            egui::Label::new(semibold_text(
                                &course.nama_mk,
                                13.5,
                                palette.text_primary,
                            ))
                            .wrap(),
                        );
                        if course.nama_mk.len() > 28 {
                            resp.on_hover_text(&course.nama_mk);
                        }
                    },
                );

                ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                    status_badge(ui, palette, tone, badge_icon, badge_label);
                });
            });

            // Row 2: Lecturer name
            if !clean_dosen.is_empty() {
                ui.add_space(3.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(&clean_dosen)
                            .size(12.5)
                            .color(palette.text_secondary),
                    )
                    .wrap(),
                );
            }

            // Row 3: Clock + clean time range, and optional concise note
            if !time_display.is_empty() || note.is_some() {
                ui.add_space(ThemeTokens::SPACE_SM);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(ThemeTokens::SPACE_SM, 4.0);
                    if !time_display.is_empty() {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            icons::paint_icon(ui, 13.0, palette.text_secondary, icons::draw_clock);
                            ui.label(mono_text(&time_display, 12.0, palette.text_primary));
                        });
                    }
                    if let Some(note_text) = &note {
                        if !time_display.is_empty() {
                            ui.label(RichText::new("·").size(12.0).color(palette.text_secondary));
                        }
                        ui.label(
                            RichText::new(sanitize_dash(note_text).into_owned())
                                .size(12.0)
                                .color(palette.text_secondary),
                        );
                    }
                });
            }
        });
}

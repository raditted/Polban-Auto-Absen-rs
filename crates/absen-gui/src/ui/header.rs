//! Header bar showing student identity, live engine status, countdown, and primary actions.

use egui::{RichText, Vec2};

use crate::bridge::{EngineStatus, GuiSnapshot};
use crate::prefs::ThemePreference;
use crate::theme::{Palette, ThemeTokens};
use crate::ui::{
    action_button, icons, mono_text, sanitize_dash, semibold_text, status_badge, ButtonTone,
    SemanticTone,
};

/// Actions emitted by the header bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderAction {
    StartMonitoring,
    StopMonitoring,
    ScanNow,
    OpenSettings,
    CycleTheme,
}

fn status_visuals(
    status: EngineStatus,
) -> (
    SemanticTone,
    fn(&egui::Painter, egui::Rect, egui::Color32),
    &'static str,
) {
    match status {
        EngineStatus::Idle => (SemanticTone::Neutral, icons::draw_clock, status.label()),
        EngineStatus::Connecting => (SemanticTone::Blue, icons::draw_refresh, status.label()),
        EngineStatus::Monitoring => (SemanticTone::Emerald, icons::draw_check, status.label()),
        EngineStatus::ScanningNow => (SemanticTone::Blue, icons::draw_refresh, status.label()),
        EngineStatus::Stopped => (SemanticTone::Neutral, icons::draw_stop, status.label()),
        EngineStatus::AuthFailed => (SemanticTone::Red, icons::draw_alert, status.label()),
        EngineStatus::NetworkError => (SemanticTone::Red, icons::draw_alert, status.label()),
    }
}

fn theme_meta(
    pref: ThemePreference,
) -> (fn(&egui::Painter, egui::Rect, egui::Color32), &'static str) {
    match pref {
        ThemePreference::System => (icons::draw_monitor, "Sistem"),
        ThemePreference::Light => (icons::draw_sun, "Terang"),
        ThemePreference::Dark => (icons::draw_moon, "Gelap"),
    }
}

fn identity_line(snapshot: &GuiSnapshot, configured_nim: &str) -> String {
    if let Some(u) = &snapshot.user {
        if !u.nama.is_empty() {
            if u.jurusan.is_empty() {
                return format!("{} ({})", u.nama, u.nim);
            }
            return format!("{} ({}) | {}", u.nama, u.nim, u.jurusan);
        }
    }
    if !configured_nim.trim().is_empty() {
        format!("NIM {}", configured_nim.trim())
    } else {
        "Portal Akademik Polban".to_string()
    }
}

pub fn show(
    ui: &mut egui::Ui,
    snapshot: &GuiSnapshot,
    configured_nim: &str,
    interval_secs: u64,
    theme_pref: ThemePreference,
    palette: &Palette,
    width: f32,
) -> Option<HeaderAction> {
    let mut action = None;
    let is_wide = width >= ThemeTokens::BREAKPOINT_WIDE;
    let is_compact = width < ThemeTokens::BREAKPOINT_MEDIUM;
    let has_account = !configured_nim.trim().is_empty();
    let is_active = snapshot.status.is_active();
    let (tone, st_icon, st_label) = status_visuals(snapshot.status);
    let (theme_icon, theme_label) = theme_meta(theme_pref);

    egui::Frame::none()
        .fill(palette.bg_surface)
        .stroke(egui::Stroke::new(1.0_f32, palette.border_subtle))
        .rounding(ThemeTokens::RADIUS_CARD)
        .inner_margin(egui::Margin::same(if is_compact {
            ThemeTokens::SPACE_MD
        } else {
            ThemeTokens::SPACE_LG
        }))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            if is_wide {
                show_wide_header(
                    ui,
                    snapshot,
                    configured_nim,
                    interval_secs,
                    palette,
                    has_account,
                    is_active,
                    (tone, st_icon, st_label),
                    (theme_icon, theme_label),
                    &mut action,
                );
            } else if !is_compact {
                show_medium_header(
                    ui,
                    snapshot,
                    configured_nim,
                    palette,
                    has_account,
                    is_active,
                    (tone, st_icon, st_label),
                    (theme_icon, theme_label),
                    &mut action,
                );
            } else {
                show_compact_header(
                    ui,
                    snapshot,
                    configured_nim,
                    palette,
                    has_account,
                    is_active,
                    (tone, st_icon, st_label),
                    (theme_icon, theme_label),
                    &mut action,
                );
            }
        });

    action
}

#[allow(clippy::too_many_arguments)]
fn show_wide_header(
    ui: &mut egui::Ui,
    snapshot: &GuiSnapshot,
    configured_nim: &str,
    interval_secs: u64,
    palette: &Palette,
    has_account: bool,
    is_active: bool,
    status_info: (
        SemanticTone,
        fn(&egui::Painter, egui::Rect, egui::Color32),
        &str,
    ),
    theme_info: (fn(&egui::Painter, egui::Rect, egui::Color32), &str),
    action: &mut Option<HeaderAction>,
) {
    let (tone, st_icon, st_label) = status_info;
    let (theme_icon, theme_label) = theme_info;

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(semibold_text("Polban Presensi", 18.0, palette.text_primary));
                ui.add_space(ThemeTokens::SPACE_SM);
                status_badge(ui, palette, tone, st_icon, st_label);
            });
            ui.add_space(ThemeTokens::SPACE_XS);
            ui.label(
                RichText::new(identity_line(snapshot, configured_nim))
                    .size(12.5)
                    .color(palette.text_secondary),
            );
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = ThemeTokens::SPACE_SM;

            if action_button(
                ui,
                palette,
                ButtonTone::Secondary,
                "Pengaturan",
                Some(icons::draw_settings),
                true,
                0.0,
            )
            .clicked()
            {
                *action = Some(HeaderAction::OpenSettings);
            }

            if action_button(
                ui,
                palette,
                ButtonTone::Secondary,
                theme_label,
                Some(theme_icon),
                true,
                0.0,
            )
            .on_hover_text("Ubah mode tema (Sistem, Terang, Gelap)")
            .clicked()
            {
                *action = Some(HeaderAction::CycleTheme);
            }

            let scan_resp = action_button(
                ui,
                palette,
                ButtonTone::Secondary,
                "Cek Sekarang",
                Some(icons::draw_refresh),
                is_active,
                0.0,
            );
            if scan_resp.clicked() {
                *action = Some(HeaderAction::ScanNow);
            } else if !is_active {
                scan_resp.on_disabled_hover_text(
                    "Mulai pemantauan terlebih dahulu untuk cek manual (F5)",
                );
            }

            if is_active {
                if action_button(
                    ui,
                    palette,
                    ButtonTone::Danger,
                    "Hentikan",
                    Some(icons::draw_stop),
                    true,
                    0.0,
                )
                .clicked()
                {
                    *action = Some(HeaderAction::StopMonitoring);
                }
            } else {
                let start_resp = action_button(
                    ui,
                    palette,
                    ButtonTone::Primary,
                    "Mulai Pantau",
                    Some(icons::draw_play),
                    has_account,
                    0.0,
                );
                if start_resp.clicked() {
                    *action = Some(HeaderAction::StartMonitoring);
                } else if !has_account {
                    start_resp.on_disabled_hover_text(
                        "Masukkan NIM dan kata sandi pada kartu masuk terlebih dahulu",
                    );
                }
            }

            ui.add_space(ThemeTokens::SPACE_XS);
            let metric_text = if is_active {
                format!("{}s", snapshot.countdown_secs)
            } else if snapshot.cycle > 0 {
                format!("#{}", snapshot.cycle)
            } else {
                "Siaga".to_string()
            };
            ui.label(mono_text(
                metric_text,
                18.0,
                if is_active {
                    palette.accent_text
                } else {
                    palette.text_secondary
                },
            ));
        });
    });

    ui.add_space(ThemeTokens::SPACE_MD);
    ui.painter().line_segment(
        [
            egui::pos2(ui.min_rect().left(), ui.cursor().top()),
            egui::pos2(ui.max_rect().right(), ui.cursor().top()),
        ],
        egui::Stroke::new(1.0_f32, palette.border_subtle),
    );
    ui.add_space(ThemeTokens::SPACE_SM);

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = ThemeTokens::SPACE_LG;
        let last_scan = if snapshot.last_scan.is_empty() {
            "Belum ada".to_string()
        } else {
            sanitize_dash(&snapshot.last_scan).into_owned()
        };
        let eff_interval = if snapshot.interval_secs > 0 {
            snapshot.interval_secs
        } else {
            interval_secs
        };
        ui.label(mono_text(
            format!("Siklus: {}", snapshot.cycle),
            12.0,
            palette.text_secondary,
        ));
        ui.label(mono_text(
            format!("Terakhir cek: {last_scan}"),
            12.0,
            palette.text_secondary,
        ));
        ui.label(mono_text(
            format!("Interval: {eff_interval}d"),
            12.0,
            palette.text_secondary,
        ));
        if !snapshot.auth_status.is_empty() {
            ui.label(
                RichText::new(format!("Portal: {}", sanitize_dash(&snapshot.auth_status)))
                    .size(12.0)
                    .color(palette.text_secondary),
            );
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn show_medium_header(
    ui: &mut egui::Ui,
    snapshot: &GuiSnapshot,
    configured_nim: &str,
    palette: &Palette,
    has_account: bool,
    is_active: bool,
    status_info: (
        SemanticTone,
        fn(&egui::Painter, egui::Rect, egui::Color32),
        &str,
    ),
    theme_info: (fn(&egui::Painter, egui::Rect, egui::Color32), &str),
    action: &mut Option<HeaderAction>,
) {
    let (tone, st_icon, st_label) = status_info;
    let (theme_icon, theme_label) = theme_info;

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(semibold_text("Polban Presensi", 18.0, palette.text_primary));
            ui.label(
                RichText::new(identity_line(snapshot, configured_nim))
                    .size(12.5)
                    .color(palette.text_secondary),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            status_badge(ui, palette, tone, st_icon, st_label);
            if is_active {
                ui.add_space(ThemeTokens::SPACE_SM);
                ui.label(mono_text(
                    format!("{}s", snapshot.countdown_secs),
                    17.0,
                    palette.accent_text,
                ));
            }
        });
    });

    ui.add_space(ThemeTokens::SPACE_MD);

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(ThemeTokens::SPACE_SM);
        if is_active {
            if action_button(
                ui,
                palette,
                ButtonTone::Danger,
                "Hentikan",
                Some(icons::draw_stop),
                true,
                0.0,
            )
            .clicked()
            {
                *action = Some(HeaderAction::StopMonitoring);
            }
        } else if action_button(
            ui,
            palette,
            ButtonTone::Primary,
            "Mulai Pantau",
            Some(icons::draw_play),
            has_account,
            0.0,
        )
        .clicked()
        {
            *action = Some(HeaderAction::StartMonitoring);
        }

        if action_button(
            ui,
            palette,
            ButtonTone::Secondary,
            "Cek Sekarang",
            Some(icons::draw_refresh),
            is_active,
            0.0,
        )
        .clicked()
        {
            *action = Some(HeaderAction::ScanNow);
        }

        if action_button(
            ui,
            palette,
            ButtonTone::Secondary,
            theme_label,
            Some(theme_icon),
            true,
            0.0,
        )
        .clicked()
        {
            *action = Some(HeaderAction::CycleTheme);
        }

        if action_button(
            ui,
            palette,
            ButtonTone::Secondary,
            "Pengaturan",
            Some(icons::draw_settings),
            true,
            0.0,
        )
        .clicked()
        {
            *action = Some(HeaderAction::OpenSettings);
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn show_compact_header(
    ui: &mut egui::Ui,
    snapshot: &GuiSnapshot,
    configured_nim: &str,
    palette: &Palette,
    has_account: bool,
    is_active: bool,
    status_info: (
        SemanticTone,
        fn(&egui::Painter, egui::Rect, egui::Color32),
        &str,
    ),
    theme_info: (fn(&egui::Painter, egui::Rect, egui::Color32), &str),
    action: &mut Option<HeaderAction>,
) {
    let (tone, st_icon, st_label) = status_info;
    let (theme_icon, theme_label) = theme_info;
    let avail = ui.available_width();

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(semibold_text("Polban Presensi", 16.5, palette.text_primary));
            let short_id = match &snapshot.user {
                Some(u) if !u.nim.is_empty() => format!("NIM {}", u.nim),
                _ if !configured_nim.trim().is_empty() => format!("NIM {}", configured_nim.trim()),
                _ => "Portal Polban".to_string(),
            };
            ui.label(mono_text(short_id, 12.0, palette.text_secondary));
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = ThemeTokens::SPACE_SM;
            if action_button(
                ui,
                palette,
                ButtonTone::Secondary,
                "",
                Some(icons::draw_settings),
                true,
                ThemeTokens::MIN_TARGET_HEIGHT,
            )
            .on_hover_text("Pengaturan (Ctrl+,)")
            .clicked()
            {
                *action = Some(HeaderAction::OpenSettings);
            }

            if action_button(
                ui,
                palette,
                ButtonTone::Secondary,
                "",
                Some(theme_icon),
                true,
                ThemeTokens::MIN_TARGET_HEIGHT,
            )
            .on_hover_text(format!("Tema: {theme_label}"))
            .clicked()
            {
                *action = Some(HeaderAction::CycleTheme);
            }
        });
    });

    ui.add_space(ThemeTokens::SPACE_SM);

    ui.horizontal(|ui| {
        status_badge(ui, palette, tone, st_icon, st_label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if is_active {
                ui.label(mono_text(
                    format!("{}s", snapshot.countdown_secs),
                    15.0,
                    palette.accent_text,
                ));
            } else if !snapshot.last_scan.is_empty() {
                ui.label(mono_text(
                    snapshot.last_scan.clone(),
                    11.5,
                    palette.text_secondary,
                ));
            }
        });
    });

    ui.add_space(ThemeTokens::SPACE_SM);

    let half_w = ((avail - ThemeTokens::SPACE_SM) * 0.5).max(120.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = ThemeTokens::SPACE_SM;
        if is_active {
            if action_button(
                ui,
                palette,
                ButtonTone::Danger,
                "Hentikan",
                Some(icons::draw_stop),
                true,
                half_w,
            )
            .clicked()
            {
                *action = Some(HeaderAction::StopMonitoring);
            }
        } else if action_button(
            ui,
            palette,
            ButtonTone::Primary,
            "Mulai Pantau",
            Some(icons::draw_play),
            has_account,
            half_w,
        )
        .clicked()
        {
            *action = Some(HeaderAction::StartMonitoring);
        }

        if action_button(
            ui,
            palette,
            ButtonTone::Secondary,
            "Cek Sekarang",
            Some(icons::draw_refresh),
            is_active,
            half_w,
        )
        .clicked()
        {
            *action = Some(HeaderAction::ScanNow);
        }
    });
}

//! Header bar showing student identity, live engine status, scannable metrics, and primary actions.

use absen_core::Color;
use egui::{Align, Layout, Pos2, RichText, Sense, Vec2};

use crate::bridge::{EngineStatus, GuiSnapshot};
use crate::theme::{Palette, ThemeTokens};
use crate::ui::{
    action_button, hairline_divider, icons, mono_text, semibold_text, status_badge, ButtonTone,
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

fn portal_connection_label(status: EngineStatus, palette: &Palette) -> (egui::Color32, &'static str) {
    match status {
        EngineStatus::Monitoring | EngineStatus::ScanningNow => {
            (palette.emerald_border, "Portal Terhubung")
        }
        EngineStatus::Connecting => (palette.blue_border, "Menghubungkan Portal"),
        EngineStatus::AuthFailed => (palette.red_border, "Autentikasi Gagal"),
        EngineStatus::NetworkError => (palette.red_border, "Gangguan Jaringan"),
        EngineStatus::Idle | EngineStatus::Stopped => (palette.text_secondary, "Portal Siaga"),
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

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    snapshot: &GuiSnapshot,
    configured_nim: &str,
    interval_secs: u64,
    is_dark: bool,
    palette: &Palette,
    width: f32,
) -> Option<HeaderAction> {
    let mut action = None;
    let stacked_actions = width < ThemeTokens::BREAKPOINT_HEADER_STACK;
    let is_compact = width < ThemeTokens::BREAKPOINT_MEDIUM;
    let has_account = !configured_nim.trim().is_empty();
    let is_active = snapshot.status.is_active();
    let (tone, st_icon, st_label) = status_visuals(snapshot.status);

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

            if stacked_actions {
                show_identity_block(ui, snapshot, configured_nim, palette, (tone, st_icon, st_label));
                ui.add_space(ThemeTokens::SPACE_MD);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(ThemeTokens::SPACE_SM);
                    render_action_buttons(
                        ui,
                        palette,
                        has_account,
                        is_active,
                        is_dark,
                        &mut action,
                    );
                });
            } else {
                ui.horizontal(|ui| {
                    let avail = ui.available_width();
                    let actions_reserve = 385.0;
                    let left_w = (avail - actions_reserve - ThemeTokens::SPACE_MD).max(200.0);

                    ui.allocate_ui_with_layout(
                        Vec2::new(left_w, 0.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_max_width(left_w);
                            show_identity_block(
                                ui,
                                snapshot,
                                configured_nim,
                                palette,
                                (tone, st_icon, st_label),
                            );
                        },
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = ThemeTokens::SPACE_SM;
                        render_action_buttons_rtl(
                            ui,
                            palette,
                            has_account,
                            is_active,
                            is_dark,
                            &mut action,
                        );
                    });
                });
            }

            ui.add_space(ThemeTokens::SPACE_MD);
            hairline_divider(ui, palette.border_subtle);
            ui.add_space(ThemeTokens::SPACE_SM);

            show_status_metric_strip(ui, snapshot, interval_secs, palette);
        });

    action
}

fn show_identity_block(
    ui: &mut egui::Ui,
    snapshot: &GuiSnapshot,
    configured_nim: &str,
    palette: &Palette,
    status_info: (
        SemanticTone,
        fn(&egui::Painter, egui::Rect, egui::Color32),
        &str,
    ),
) {
    let (tone, st_icon, st_label) = status_info;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(ThemeTokens::SPACE_SM, ThemeTokens::SPACE_XS);
        ui.label(semibold_text("Polban Presensi", 18.0, palette.text_primary));
        status_badge(ui, palette, tone, st_icon, st_label);
    });
    ui.add_space(3.0);
    ui.add(
        egui::Label::new(
            RichText::new(identity_line(snapshot, configured_nim))
                .size(12.5)
                .color(palette.text_secondary),
        )
        .wrap(),
    );
}

fn render_theme_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    is_dark: bool,
    action: &mut Option<HeaderAction>,
) {
    let (icon_fn, tooltip) = if is_dark {
        (
            icons::draw_sun as fn(&egui::Painter, egui::Rect, egui::Color32),
            "Ubah ke Mode Terang",
        )
    } else {
        (
            icons::draw_moon as fn(&egui::Painter, egui::Rect, egui::Color32),
            "Ubah ke Mode Gelap",
        )
    };
    if action_button(
        ui,
        palette,
        ButtonTone::Secondary,
        "",
        Some(icon_fn),
        true,
        0.0,
    )
    .on_hover_text(tooltip)
    .clicked()
    {
        *action = Some(HeaderAction::CycleTheme);
    }
}

fn render_action_buttons(
    ui: &mut egui::Ui,
    palette: &Palette,
    has_account: bool,
    is_active: bool,
    is_dark: bool,
    action: &mut Option<HeaderAction>,
) {
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
        scan_resp.on_disabled_hover_text("Mulai pemantauan terlebih dahulu untuk cek manual (F5)");
    }

    render_theme_button(ui, palette, is_dark, action);

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
}

fn render_action_buttons_rtl(
    ui: &mut egui::Ui,
    palette: &Palette,
    has_account: bool,
    is_active: bool,
    is_dark: bool,
    action: &mut Option<HeaderAction>,
) {
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

    render_theme_button(ui, palette, is_dark, action);

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
        scan_resp.on_disabled_hover_text("Mulai pemantauan terlebih dahulu untuk cek manual (F5)");
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
}

fn metric_separator(ui: &mut egui::Ui, palette: &Palette) {
    ui.label(RichText::new("·").size(12.5).color(palette.text_secondary));
}

fn show_status_metric_strip(
    ui: &mut egui::Ui,
    snapshot: &GuiSnapshot,
    interval_secs: u64,
    palette: &Palette,
) {
    let (dot_color, portal_text) = portal_connection_label(snapshot.status, palette);
    let green_count = snapshot
        .courses
        .iter()
        .filter(|c| c.color == Color::Green)
        .count();
    let total_courses = snapshot.courses.len();
    let eff_interval = if snapshot.interval_secs > 0 {
        snapshot.interval_secs
    } else {
        interval_secs
    };

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(ThemeTokens::SPACE_SM, 4.0);

        // Portal connection indicator with crisp status dot
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
            if ui.is_rect_visible(dot_rect) {
                ui.painter().circle_filled(
                    Pos2::new(dot_rect.center().x, dot_rect.center().y),
                    3.8,
                    dot_color,
                );
            }
            ui.label(semibold_text(portal_text, 12.0, palette.text_primary));
        });

        if total_courses > 0 {
            metric_separator(ui, palette);
            ui.label(
                RichText::new(format!("Hadir {green_count}/{total_courses}"))
                    .size(12.0)
                    .color(palette.text_secondary),
            );
        }

        if snapshot.status.is_active() {
            metric_separator(ui, palette);
            if snapshot.status == EngineStatus::ScanningNow {
                ui.label(semibold_text(
                    "Memindai sekarang...",
                    12.0,
                    palette.accent_text,
                ));
            } else {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    icons::paint_icon(ui, 12.0, palette.accent_text, icons::draw_clock);
                    ui.label(
                        RichText::new("Cek berikutnya")
                            .size(12.0)
                            .color(palette.text_secondary),
                    );
                    ui.label(mono_text(
                        format!("{} dtk", snapshot.countdown_secs),
                        12.0,
                        palette.accent_text,
                    ));
                });
            }
        } else if eff_interval > 0 {
            metric_separator(ui, palette);
            ui.label(
                RichText::new(format!("Interval {eff_interval} dtk"))
                    .size(12.0)
                    .color(palette.text_secondary),
            );
        }

        if !snapshot.last_scan.is_empty() {
            metric_separator(ui, palette);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(
                    RichText::new("Terakhir")
                        .size(12.0)
                        .color(palette.text_secondary),
                );
                ui.label(mono_text(
                    &snapshot.last_scan,
                    12.0,
                    palette.text_primary,
                ));
            });
        }

        if snapshot.cycle > 0 {
            metric_separator(ui, palette);
            ui.label(mono_text(
                format!("Siklus #{}", snapshot.cycle),
                12.0,
                palette.text_secondary,
            ));
        }
    });
}

//! Header bar showing student identity, live engine status, scannable metrics, and primary actions.

use absen_core::Color;
use egui::{
    Align, CursorIcon, FontFamily, Layout, Pos2, Rect, RichText, Rounding, Sense, Stroke, Vec2,
};

use crate::bridge::{EngineStatus, GuiSnapshot};
use crate::theme::{Palette, ThemeTokens};
use crate::ui::{
    action_button, hairline_divider, icons, mono_text, paint_focus_ring, semibold_text,
    status_badge, ButtonTone, SemanticTone,
};
use crate::updater::{AssetStrategy, UpdateStatus};

/// Actions emitted by the header bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderAction {
    StartMonitoring,
    StopMonitoring,
    ScanNow,
    OpenProofs,
    OpenSettings,
    CycleTheme,
    StartUpdate,
    RestartToUpdate,
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

fn portal_connection_label(
    status: EngineStatus,
    palette: &Palette,
) -> (egui::Color32, &'static str) {
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
    update_status: &UpdateStatus,
    proof_btn_label: &str,
) -> Option<HeaderAction> {
    let mut action = None;
    let is_active = snapshot.status.is_active();
    let stacked_actions =
        width < ThemeTokens::BREAKPOINT_HEADER_STACK || (!is_active && width < 980.0);
    let is_compact = width < ThemeTokens::BREAKPOINT_MEDIUM;
    let has_account = !configured_nim.trim().is_empty();
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
                show_identity_block(
                    ui,
                    snapshot,
                    configured_nim,
                    palette,
                    (tone, st_icon, st_label),
                    update_status,
                    &mut action,
                );
                ui.add_space(ThemeTokens::SPACE_MD);
                let single_row_needed = if is_active { 560.0 } else { 670.0 };
                if ui.available_width() >= single_row_needed {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::splat(ThemeTokens::SPACE_SM);
                        render_primary_action_buttons(
                            ui,
                            palette,
                            has_account,
                            is_active,
                            &mut action,
                        );
                        render_secondary_action_buttons(
                            ui,
                            palette,
                            is_dark,
                            proof_btn_label,
                            &mut action,
                        );
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::splat(ThemeTokens::SPACE_SM);
                        render_primary_action_buttons(
                            ui,
                            palette,
                            has_account,
                            is_active,
                            &mut action,
                        );
                    });
                    ui.add_space(ThemeTokens::SPACE_SM);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::splat(ThemeTokens::SPACE_SM);
                        render_secondary_action_buttons(
                            ui,
                            palette,
                            is_dark,
                            proof_btn_label,
                            &mut action,
                        );
                    });
                }
            } else {
                ui.horizontal(|ui| {
                    let avail = ui.available_width();
                    let actions_reserve = if is_active { 535.0 } else { 660.0 };
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
                                update_status,
                                &mut action,
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
                            proof_btn_label,
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
    status_info: (SemanticTone, fn(&egui::Painter, Rect, egui::Color32), &str),
    update_status: &UpdateStatus,
    action: &mut Option<HeaderAction>,
) {
    let (tone, st_icon, st_label) = status_info;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(ThemeTokens::SPACE_SM, ThemeTokens::SPACE_XS);
        ui.label(semibold_text("Polban Presensi", 18.0, palette.text_primary));
        status_badge(ui, palette, tone, st_icon, st_label);
        render_update_pill(ui, palette, update_status, action);
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

fn update_pill_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    tone: SemanticTone,
    icon_fn: fn(&egui::Painter, Rect, egui::Color32),
    label: &str,
    interactive: bool,
    tooltip: &str,
) -> bool {
    let (fg, bg, border) = tone.colors(palette);
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::new(
            11.5,
            FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()),
        ),
        fg,
    );
    let pad_x = ThemeTokens::SPACE_SM;
    let pad_y = 4.0;
    let icon_w = 12.0;
    let gap = 5.0;
    let desired = Vec2::new(
        pad_x * 2.0 + icon_w + gap + galley.size().x,
        galley.size().y.max(icon_w) + pad_y * 2.0,
    );
    let sense = if interactive {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, mut resp) = ui.allocate_exact_size(desired, sense);
    if interactive {
        resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    }
    if ui.is_rect_visible(rect) {
        let hovered = interactive && resp.hovered();
        let stroke_color = if hovered { fg } else { border };
        ui.painter().rect(
            rect,
            Rounding::same(ThemeTokens::RADIUS_TAG),
            bg,
            Stroke::new(if hovered { 1.4_f32 } else { 1.0_f32 }, stroke_color),
        );
        let icon_rect = Rect::from_center_size(
            Pos2::new(rect.left() + pad_x + icon_w * 0.5, rect.center().y),
            Vec2::splat(icon_w),
        );
        icon_fn(ui.painter(), icon_rect, fg);
        ui.painter().galley(
            Pos2::new(
                rect.left() + pad_x + icon_w + gap,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            fg,
        );
        if interactive {
            paint_focus_ring(ui, &resp, palette, ThemeTokens::RADIUS_TAG);
        }
    }
    let clicked = interactive && resp.clicked();
    if !tooltip.is_empty() {
        resp.on_hover_text(tooltip);
    }
    clicked
}

pub(crate) fn available_update_tooltip(version: &str, strategy: AssetStrategy) -> String {
    if strategy == AssetStrategy::OpenReleasePage {
        format!("Versi v{version} tersedia. Klik untuk membuka halaman rilis.")
    } else {
        format!("Versi v{version} tersedia. Klik untuk mengunduh pembaruan di latar belakang.")
    }
}

fn render_update_pill(
    ui: &mut egui::Ui,
    palette: &Palette,
    update_status: &UpdateStatus,
    action: &mut Option<HeaderAction>,
) {
    match update_status {
        UpdateStatus::Idle | UpdateStatus::Checking | UpdateStatus::UpToDate => {}
        UpdateStatus::Available(candidate) => {
            let label = format!("Update v{}", candidate.version);
            let tooltip = available_update_tooltip(&candidate.version, candidate.strategy);
            let icon_fn = if candidate.strategy == AssetStrategy::OpenReleasePage {
                icons::draw_external_link
            } else {
                icons::draw_download
            };
            if update_pill_button(
                ui,
                palette,
                SemanticTone::Blue,
                icon_fn,
                &label,
                true,
                &tooltip,
            ) {
                *action = Some(HeaderAction::StartUpdate);
            }
        }
        UpdateStatus::Downloading {
            version,
            progress_pct,
            verifying,
        } => {
            let label = if *verifying {
                "Memverifikasi...".to_string()
            } else if let Some(pct) = progress_pct {
                format!("Mengunduh {pct}%")
            } else {
                "Mengunduh...".to_string()
            };
            let tooltip = format!(
                "Mengunduh pembaruan v{version} di latar belakang. Aplikasi tetap dapat digunakan."
            );
            update_pill_button(
                ui,
                palette,
                SemanticTone::Blue,
                icons::draw_download,
                &label,
                false,
                &tooltip,
            );
        }
        UpdateStatus::ReadyToRestart { version, .. } => {
            let tooltip = format!(
                "Pembaruan v{version} selesai diverifikasi. Klik untuk restart dan menerapkan update."
            );
            if update_pill_button(
                ui,
                palette,
                SemanticTone::Emerald,
                icons::draw_refresh,
                "Restart untuk Update",
                true,
                &tooltip,
            ) {
                *action = Some(HeaderAction::RestartToUpdate);
            }
        }
        UpdateStatus::Installing { version } => {
            let tooltip = format!("Memasang pembaruan v{version}...");
            update_pill_button(
                ui,
                palette,
                SemanticTone::Blue,
                icons::draw_refresh,
                "Memasang...",
                false,
                &tooltip,
            );
        }
        UpdateStatus::Failed {
            message,
            retry_candidate,
        } => {
            let interactive = retry_candidate.is_some();
            if update_pill_button(
                ui,
                palette,
                SemanticTone::Red,
                icons::draw_alert,
                "Gagal Update - Coba Lagi",
                interactive,
                message,
            ) && interactive
            {
                *action = Some(HeaderAction::StartUpdate);
            }
        }
    }
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

fn render_proofs_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    proof_btn_label: &str,
    action: &mut Option<HeaderAction>,
) {
    if action_button(
        ui,
        palette,
        ButtonTone::Secondary,
        proof_btn_label,
        Some(icons::draw_shield_check),
        true,
        0.0,
    )
    .on_hover_text("Buka bukti kehadiran tersimpan (14 hari terakhir)")
    .clicked()
    {
        *action = Some(HeaderAction::OpenProofs);
    }
}

fn render_primary_action_buttons(
    ui: &mut egui::Ui,
    palette: &Palette,
    has_account: bool,
    is_active: bool,
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
            "Jalankan Presensi Otomatis",
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
}

fn render_secondary_action_buttons(
    ui: &mut egui::Ui,
    palette: &Palette,
    is_dark: bool,
    proof_btn_label: &str,
    action: &mut Option<HeaderAction>,
) {
    render_proofs_button(ui, palette, proof_btn_label, action);

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
    proof_btn_label: &str,
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

    render_proofs_button(ui, palette, proof_btn_label, action);

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
            "Jalankan Presensi Otomatis",
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
                ui.label(mono_text(&snapshot.last_scan, 12.0, palette.text_primary));
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

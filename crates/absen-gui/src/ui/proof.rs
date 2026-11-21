//! On-demand attendance proof ("bukti kehadiran absen") preview modal.

use std::sync::OnceLock;

use absen_core::{AttendanceProof, ProofVerification};
use egui::{
    Align, Color32, CursorIcon, FontFamily, Layout, RichText, Rounding, Sense, Stroke, Vec2,
};

use crate::bridge::{EngineStatus, GuiSnapshot, ProofPreview};
use crate::theme::{Palette, ThemeTokens};
use crate::ui::{
    action_button, hairline_divider, icons, mono_text, paint_focus_ring, semibold_text, ButtonTone,
};
use crate::updater::is_safe_http_url;

/// User actions emitted from the attendance proof modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofAction {
    Close,
    SelectIndex(usize),
    CopyReceipt,
    SaveImage,
    CaptureProofScreenshot(String),
    CaptureLivePortalPreview,
    ExitLivePortalPreview,
    ToggleImageZoom,
    Reverify,
    OpenPortal(String),
    ToggleTechnical,
    EnterSamplePreview,
    ExitSamplePreview,
}

/// View-model for the lazy-loaded screenshot preview inside the proof modal.
#[derive(Clone, Copy, Default)]
pub struct ProofScreenshotView<'a> {
    pub texture: Option<&'a egui::TextureHandle>,
    pub pixel_size: Option<[usize; 2]>,
    pub source_label: Option<&'a str>,
    pub is_capturing: bool,
    pub zoomed: bool,
    pub live_preview_active: bool,
    pub live_captured_at: Option<&'a str>,
    pub live_error: Option<&'a str>,
    pub save_in_progress: bool,
    pub save_feedback_active: bool,
    pub save_error: Option<&'a str>,
    pub can_save_image: bool,
}

fn sample_preview() -> &'static ProofPreview {
    static SAMPLE: OnceLock<ProofPreview> = OnceLock::new();
    SAMPLE.get_or_init(|| {
        ProofPreview::from_proof(AttendanceProof {
            proof_id: "CONTOH-PREVIEW".to_string(),
            sig: "Algoritma & Struktur Data|KO078N-Contoh Dosen|07:00:00 s.d. 09:30:00".to_string(),
            key: "Algoritma & Struktur Data|KO078N-Contoh Dosen".to_string(),
            date: "2026-11-21".to_string(),
            recorded_at: "2026-11-21T07:05:12+07:00".to_string(),
            verified_at: Some("2026-11-21T07:06:00+07:00".to_string()),
            server_date_header: Some("Sat, 21 Nov 2026 00:05:12 GMT".to_string()),
            verification: ProofVerification::PortalVerified,
            nim: "251511000".to_string(),
            student_nama: "Mahasiswa Contoh".to_string(),
            jurusan: "D3-Teknik Informatika".to_string(),
            nama_mk: "Algoritma & Struktur Data".to_string(),
            mk_code: "25IF201".to_string(),
            dosen: "KO078N-Dosen Pengampu Contoh".to_string(),
            jam: "07:00:00 s.d. 09:30:00".to_string(),
            tp: "T".to_string(),
            ja: "1".to_string(),
            jb: "3".to_string(),
            kls: "1A".to_string(),
            target_url: "https://akademik.polban.ac.id/ajar/absen".to_string(),
            endpoint_url: Some("https://akademik.polban.ac.id/ajar/absen/absensi_awal".to_string()),
            http_status: Some(200),
            server_response: Some(r#"{"status":"ok"}"#.to_string()),
            server_button_label: Some("Sudah Absen".to_string()),
            attempt: Some(1),
        })
    })
}

fn compact_nav_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    enabled: bool,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        let size = Vec2::new(28.0, 28.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let response = if enabled {
            response.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            response
        };
        if ui.is_rect_visible(rect) {
            let hovered = enabled && response.hovered();
            let fill = if hovered {
                palette.bg_subtle
            } else {
                palette.bg_surface
            };
            let stroke_col = if !enabled {
                palette.border_subtle
            } else if hovered {
                palette.accent_text
            } else {
                palette.border_interactive
            };
            let fg = if enabled {
                palette.text_primary
            } else {
                palette.text_secondary
            };
            ui.painter().rect(
                rect,
                Rounding::same(ThemeTokens::RADIUS_TAG),
                fill,
                Stroke::new(1.0_f32, stroke_col),
            );
            let galley = ui.painter().layout_no_wrap(
                label.to_string(),
                egui::FontId::new(
                    13.0,
                    FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()),
                ),
                fg,
            );
            let pos = egui::Pos2::new(
                rect.center().x - galley.size().x * 0.5,
                rect.center().y - galley.size().y * 0.5,
            );
            ui.painter().galley(pos, galley, fg);
            paint_focus_ring(ui, &response, palette, ThemeTokens::RADIUS_TAG);
        }
        response
    })
    .inner
}

fn render_header_pill(
    ui: &mut egui::Ui,
    label: &str,
    fg: Color32,
    bg: Color32,
    border: Color32,
    mono: bool,
) {
    egui::Frame::none()
        .fill(bg)
        .stroke(Stroke::new(1.0_f32, border))
        .rounding(Rounding::same(ThemeTokens::RADIUS_TAG))
        .inner_margin(egui::Margin::symmetric(8.0, 3.0))
        .show(ui, |ui| {
            let text = if mono {
                mono_text(label, 11.5, fg)
            } else {
                semibold_text(label, 11.5, fg)
            };
            ui.label(text);
        });
}

fn render_sample_notice_banner(ui: &mut egui::Ui, palette: &Palette) {
    egui::Frame::none()
        .fill(palette.amber_bg)
        .stroke(Stroke::new(1.2_f32, palette.amber_border))
        .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
        .inner_margin(egui::Margin::same(ThemeTokens::SPACE_MD))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = ThemeTokens::SPACE_SM;
                icons::paint_icon(ui, 15.0, palette.amber_text, icons::draw_alert);
                ui.add(
                    egui::Label::new(semibold_text(
                        "PRATINJAU CONTOH · Bukan bukti kehadiran asli",
                        13.0,
                        palette.amber_text,
                    ))
                    .wrap(),
                );
            });
            ui.add_space(ThemeTokens::SPACE_XS);
            ui.add(
                egui::Label::new(
                    RichText::new(
                        "Tampilan berikut hanya ilustrasi format bukti kehadiran dan tidak tersimpan ke riwayat presensi.",
                    )
                    .size(12.0)
                    .color(palette.amber_text),
                )
                .wrap(),
            );
        });
}

fn compact_pill_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    enabled: bool,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        let galley = ui.painter().layout_no_wrap(
            label.to_string(),
            egui::FontId::new(
                11.5,
                FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()),
            ),
            palette.text_primary,
        );
        let desired = Vec2::new(galley.size().x + 18.0, 26.0);
        let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
        let response = if enabled {
            response.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            response
        };
        if ui.is_rect_visible(rect) {
            let hovered = enabled && response.hovered();
            let fill = if !enabled || hovered {
                palette.bg_subtle
            } else {
                palette.bg_surface
            };
            let stroke_col = if !enabled {
                palette.border_subtle
            } else if hovered {
                palette.accent_text
            } else {
                palette.border_interactive
            };
            let fg = if enabled {
                palette.text_primary
            } else {
                palette.text_secondary
            };
            ui.painter().rect(
                rect,
                Rounding::same(ThemeTokens::RADIUS_TAG),
                fill,
                Stroke::new(1.0_f32, stroke_col),
            );
            let text_galley = ui.painter().layout_no_wrap(
                label.to_string(),
                egui::FontId::new(
                    11.5,
                    FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()),
                ),
                fg,
            );
            let pos = egui::Pos2::new(
                rect.center().x - text_galley.size().x * 0.5,
                rect.center().y - text_galley.size().y * 0.5,
            );
            ui.painter().galley(pos, text_galley, fg);
            paint_focus_ring(ui, &response, palette, ThemeTokens::RADIUS_TAG);
        }
        response
    })
    .inner
}

struct ShotSectionCtx<'a> {
    has_portal_snapshot: bool,
    proof_id_for_capture: Option<&'a str>,
    is_sample: bool,
}

fn render_screenshot_section(
    ui: &mut egui::Ui,
    palette: &Palette,
    body_w: f32,
    ctx_info: ShotSectionCtx<'_>,
    shot: &ProofScreenshotView<'_>,
    action: &mut Option<ProofAction>,
) {
    let ShotSectionCtx {
        has_portal_snapshot,
        proof_id_for_capture,
        is_sample,
    } = ctx_info;
    let inner_w = (body_w - ThemeTokens::SPACE_MD * 2.0).max(160.0);
    egui::Frame::none()
        .fill(palette.bg_subtle)
        .stroke(Stroke::new(1.0_f32, palette.border_subtle))
        .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
        .inner_margin(egui::Margin::same(ThemeTokens::SPACE_MD))
        .show(ui, |ui| {
            ui.set_width(inner_w);
            ui.set_max_width(inner_w);
            ui.spacing_mut().item_spacing.y = ThemeTokens::SPACE_SM;

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(ThemeTokens::SPACE_SM, 6.0);
                ui.label(semibold_text(
                    "Pratinjau Halaman Absen",
                    13.0,
                    palette.text_primary,
                ));

                if let Some(src) = shot.source_label {
                    render_header_pill(
                        ui,
                        src,
                        palette.blue_text,
                        palette.blue_bg,
                        palette.blue_border,
                        false,
                    );
                }

                if shot.texture.is_some() {
                    let zoom_lbl = if shot.zoomed {
                        "Sesuaikan"
                    } else {
                        "Perbesar"
                    };
                    if compact_pill_button(ui, palette, zoom_lbl, true).clicked() {
                        *action = Some(ProofAction::ToggleImageZoom);
                    }
                }

                if !is_sample && !shot.live_preview_active && has_portal_snapshot {
                    if let Some(pid) = proof_id_for_capture {
                        let btn_lbl = if shot.texture.is_some() {
                            "Perbarui Screenshot"
                        } else {
                            "Ambil Screenshot"
                        };
                        if compact_pill_button(ui, palette, btn_lbl, !shot.is_capturing).clicked() {
                            *action = Some(ProofAction::CaptureProofScreenshot(pid.to_string()));
                        }
                    }
                }
            });

            if let Some(ts) = shot.live_captured_at {
                if shot.live_preview_active && !ts.trim().is_empty() {
                    ui.add(
                        egui::Label::new(mono_text(
                            format!("Snapshot portal: {ts}"),
                            11.0,
                            palette.text_secondary,
                        ))
                        .wrap(),
                    );
                }
            }

            if shot.is_capturing {
                egui::Frame::none()
                    .fill(palette.bg_canvas)
                    .stroke(Stroke::new(1.0_f32, palette.border_subtle))
                    .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
                    .inner_margin(egui::Margin::same(ThemeTokens::SPACE_LG))
                    .show(ui, |ui| {
                        ui.set_width((inner_w - ThemeTokens::SPACE_LG * 2.0).max(120.0));
                        ui.vertical_centered(|ui| {
                            icons::paint_icon(ui, 18.0, palette.accent_text, icons::draw_clock);
                            ui.add_space(4.0);
                            ui.label(RichText::new("Menyiapkan pratinjau halaman absen...").size(12.5).color(palette.text_secondary));
                        });
                    });
            } else if let Some(tex) = shot.texture {
                let [pw, ph] = shot.pixel_size.unwrap_or_else(|| tex.size());
                let pw_f = (pw.max(1)) as f32;
                let ph_f = (ph.max(1)) as f32;
                let aspect = (pw_f / ph_f).max(0.2);

                egui::Frame::none()
                    .fill(palette.bg_canvas)
                    .stroke(Stroke::new(1.0_f32, palette.border_subtle))
                    .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
                    .inner_margin(egui::Margin::same(6.0))
                    .show(ui, |ui| {
                        let frame_inner_w = (inner_w - 14.0).max(120.0);
                        ui.set_width(frame_inner_w);
                        ui.set_max_width(frame_inner_w);
                        if shot.zoomed {
                            let natural_w = pw_f.max(frame_inner_w).min(1120.0);
                            let natural_h = (natural_w / aspect).max(40.0);
                            egui::ScrollArea::both()
                                .id_salt("proof_img_zoom_scroll")
                                .max_width(frame_inner_w)
                                .max_height(360.0)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::Image::new((
                                            tex.id(),
                                            Vec2::new(natural_w, natural_h),
                                        ))
                                        .rounding(Rounding::same(4.0)),
                                    );
                                });
                        } else {
                            let max_fit_h = 300.0;
                            let draw_w = frame_inner_w.min(max_fit_h * aspect);
                            let draw_h = (draw_w / aspect).max(40.0);
                            ui.vertical_centered(|ui| {
                                ui.add(
                                    egui::Image::new((tex.id(), Vec2::new(draw_w, draw_h)))
                                        .rounding(Rounding::same(4.0)),
                                );
                            });
                        }
                    });
            } else if let Some(err) = shot.live_error {
                ui.add(
                    egui::Label::new(
                        RichText::new(err)
                            .size(12.0)
                            .color(palette.red_text),
                    )
                    .wrap(),
                );
            } else {
                egui::Frame::none()
                    .fill(palette.bg_canvas)
                    .stroke(Stroke::new(1.0_f32, palette.border_subtle))
                    .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
                    .inner_margin(egui::Margin::same(ThemeTokens::SPACE_MD))
                    .show(ui, |ui| {
                        ui.set_width((inner_w - ThemeTokens::SPACE_MD * 2.0).max(120.0));
                        let msg = if has_portal_snapshot {
                            "Screenshot halaman absen belum diambil untuk bukti ini. Klik Ambil Screenshot di atas untuk merender dari HTML terakhir."
                        } else {
                            "Screenshot otomatis dibuat saat bukti terverifikasi pada pemantauan aktif."
                        };
                        ui.add(
                            egui::Label::new(
                                RichText::new(msg).size(12.0).color(palette.text_secondary),
                            )
                            .wrap(),
                        );
                    });
            }

            if let Some(save_err) = shot.save_error {
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("Gagal menyimpan gambar: {save_err}"))
                            .size(12.0)
                            .color(palette.red_text),
                    )
                    .wrap(),
                );
            }
        });
}

fn render_empty_state(
    ui: &mut egui::Ui,
    palette: &Palette,
    has_portal_snapshot: bool,
    captured_at: Option<&str>,
) {
    egui::Frame::none()
        .fill(palette.bg_subtle)
        .stroke(Stroke::new(1.0_f32, palette.border_subtle))
        .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
        .inner_margin(egui::Margin::same(ThemeTokens::SPACE_XL))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = ThemeTokens::SPACE_SM;

                let badge_size = Vec2::splat(48.0);
                let (rect, _) = ui.allocate_exact_size(badge_size, Sense::hover());
                if ui.is_rect_visible(rect) {
                    ui.painter().rect(
                        rect,
                        Rounding::same(24.0),
                        palette.bg_surface,
                        Stroke::new(1.0_f32, palette.border_subtle),
                    );
                    let icon_rect = egui::Rect::from_center_size(rect.center(), Vec2::splat(24.0));
                    icons::draw_shield_check(ui.painter(), icon_rect, palette.accent_text);
                }

                ui.add_space(ThemeTokens::SPACE_XS);
                ui.add(
                    egui::Label::new(semibold_text(
                        "Belum Ada Bukti Kehadiran Tersimpan",
                        15.0,
                        palette.text_primary,
                    ))
                    .wrap(),
                );
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            "Setiap presensi yang berhasil dicatat atau terdeteksi sudah hadir di portal otomatis menyimpan status server dan verifikasi portal secara lokal hingga 14 hari tanpa permintaan jaringan tambahan.",
                        )
                        .size(12.5)
                        .color(palette.text_secondary),
                    )
                    .wrap(),
                );
                if has_portal_snapshot {
                    let info = match captured_at {
                        Some(ts) if !ts.trim().is_empty() => format!(
                            "Portal terhubung (snapshot terakhir: {ts}). Gunakan Preview Halaman Absen untuk melihat tampilan jadwal terkini."
                        ),
                        _ => "Portal terhubung. Gunakan Preview Halaman Absen untuk melihat tampilan jadwal terkini.".to_string(),
                    };
                    ui.add(
                        egui::Label::new(
                            RichText::new(info).size(12.0).color(palette.accent_text),
                        )
                        .wrap(),
                    );
                }
            });
        });
}

fn render_verification_banner(
    ui: &mut egui::Ui,
    proof: &ProofPreview,
    is_today: bool,
    palette: &Palette,
) {
    let (fill, border, title_fg, body_fg, icon_fn) = match proof.verification {
        ProofVerification::PortalVerified => (
            palette.emerald_bg,
            palette.emerald_border,
            palette.emerald_text,
            palette.emerald_text,
            icons::draw_check as fn(&egui::Painter, egui::Rect, Color32),
        ),
        ProofVerification::PostAccepted => (
            palette.blue_bg,
            palette.blue_border,
            palette.blue_text,
            palette.blue_text,
            icons::draw_clock as fn(&egui::Painter, egui::Rect, Color32),
        ),
        ProofVerification::ObservedPresent => (
            palette.bg_subtle,
            palette.emerald_border,
            palette.emerald_text,
            palette.text_secondary,
            icons::draw_check as fn(&egui::Painter, egui::Rect, Color32),
        ),
    };

    let explanation: &str = if proof.verification == ProofVerification::PostAccepted && !is_today {
        "Permintaan presensi diterima server pada hari tersebut. Verifikasi ulang di portal hanya tersedia untuk jadwal hari ini."
    } else {
        &proof.banner_explanation
    };

    egui::Frame::none()
        .fill(fill)
        .stroke(Stroke::new(1.1_f32, border))
        .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
        .inner_margin(egui::Margin::same(ThemeTokens::SPACE_MD))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = ThemeTokens::SPACE_SM;
                icons::paint_icon(ui, 15.0, title_fg, icon_fn);
                ui.label(semibold_text(proof.verification_label, 13.5, title_fg));
            });
            ui.add_space(ThemeTokens::SPACE_XS);
            ui.add(egui::Label::new(RichText::new(explanation).size(12.5).color(body_fg)).wrap());
        });
}

fn detail_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    body_w: f32,
    render_val: impl FnOnce(&mut egui::Ui, f32),
) {
    let label_w = if body_w >= 410.0 { 148.0 } else { 0.0 };
    if label_w > 0.0 {
        let val_w = (body_w - label_w - ThemeTokens::SPACE_MD).max(120.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = ThemeTokens::SPACE_MD;
            ui.allocate_ui_with_layout(
                Vec2::new(label_w, 0.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_max_width(label_w);
                    ui.label(
                        RichText::new(label)
                            .size(12.5)
                            .color(palette.text_secondary),
                    );
                },
            );
            ui.allocate_ui_with_layout(Vec2::new(val_w, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_max_width(val_w);
                render_val(ui, val_w);
            });
        });
    } else {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(
                RichText::new(label)
                    .size(12.0)
                    .color(palette.text_secondary),
            );
            render_val(ui, body_w);
        });
    }
}

fn render_receipt_details(ui: &mut egui::Ui, proof: &ProofPreview, palette: &Palette, body_w: f32) {
    let card_inner_w = (body_w - ThemeTokens::SPACE_MD * 2.0).max(180.0);
    egui::Frame::none()
        .fill(palette.bg_subtle)
        .stroke(Stroke::new(1.0_f32, palette.border_subtle))
        .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
        .inner_margin(egui::Margin::same(ThemeTokens::SPACE_MD))
        .show(ui, |ui| {
            ui.set_width(card_inner_w);
            ui.spacing_mut().item_spacing.y = ThemeTokens::SPACE_SM;

            detail_row(ui, palette, "Mata Kuliah", card_inner_w, |ui, _| {
                ui.add(
                    egui::Label::new(semibold_text(&proof.nama_mk, 13.0, palette.text_primary))
                        .wrap(),
                );
                if !proof.course_meta_line.is_empty() {
                    ui.add(
                        egui::Label::new(mono_text(
                            &proof.course_meta_line,
                            11.5,
                            palette.text_secondary,
                        ))
                        .wrap(),
                    );
                }
            });

            if !proof.dosen.is_empty() || !proof.jam_display.is_empty() {
                hairline_divider(ui, palette.border_subtle);
                detail_row(ui, palette, "Dosen & Jam", card_inner_w, |ui, _| {
                    if !proof.dosen.is_empty() {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&proof.dosen)
                                    .size(12.5)
                                    .color(palette.text_primary),
                            )
                            .wrap(),
                        );
                    }
                    if !proof.jam_display.is_empty() {
                        ui.label(mono_text(&proof.jam_display, 12.0, palette.text_secondary));
                    }
                });
            }

            if !proof.student_line.is_empty() {
                hairline_divider(ui, palette.border_subtle);
                detail_row(ui, palette, "Mahasiswa", card_inner_w, |ui, _| {
                    if !proof.student_nama.is_empty() {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&proof.student_nama)
                                    .size(12.5)
                                    .color(palette.text_primary),
                            )
                            .wrap(),
                        );
                    }
                    if !proof.student_sub_line.is_empty() {
                        ui.add(
                            egui::Label::new(mono_text(
                                &proof.student_sub_line,
                                11.5,
                                palette.text_secondary,
                            ))
                            .wrap(),
                        );
                    }
                });
            }

            if !proof.recorded_date_time.is_empty() {
                hairline_divider(ui, palette.border_subtle);
                detail_row(ui, palette, "Waktu Tercatat", card_inner_w, |ui, _| {
                    ui.label(mono_text(
                        &proof.recorded_date_time,
                        12.5,
                        palette.text_primary,
                    ));
                });
            }

            if let Some(verified_hms) = &proof.verified_time_hms {
                hairline_divider(ui, palette.border_subtle);
                detail_row(ui, palette, "Verifikasi Portal", card_inner_w, |ui, _| {
                    ui.label(mono_text(verified_hms, 12.5, palette.emerald_text));
                });
            }

            if let Some(server_date) = &proof.server_date_header {
                hairline_divider(ui, palette.border_subtle);
                detail_row(ui, palette, "Waktu Server HTTP", card_inner_w, |ui, _| {
                    ui.add(
                        egui::Label::new(mono_text(server_date, 11.5, palette.text_secondary))
                            .wrap(),
                    );
                });
            }

            if !proof.http_summary.is_empty() {
                hairline_divider(ui, palette.border_subtle);
                detail_row(ui, palette, "Status Portal", card_inner_w, |ui, _| {
                    ui.add(
                        egui::Label::new(mono_text(
                            &proof.http_summary,
                            11.5,
                            palette.text_primary,
                        ))
                        .wrap(),
                    );
                });
            }
        });
}

fn render_technical_section(
    ui: &mut egui::Ui,
    proof: &ProofPreview,
    palette: &Palette,
    expanded: bool,
    body_w: f32,
    action: &mut Option<ProofAction>,
) {
    let toggle_resp = egui::Frame::none()
        .fill(palette.bg_surface)
        .stroke(Stroke::new(1.0_f32, palette.border_subtle))
        .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
        .inner_margin(egui::Margin::symmetric(ThemeTokens::SPACE_MD, 9.0))
        .show(ui, |ui| {
            ui.set_width((body_w - ThemeTokens::SPACE_MD * 2.0).max(160.0));
            ui.horizontal(|ui| {
                ui.label(semibold_text(
                    "Detail teknis server",
                    12.5,
                    palette.text_primary,
                ));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let hint = if expanded { "Sembunyikan" } else { "Tampilkan" };
                    ui.label(RichText::new(hint).size(12.0).color(palette.accent_text));
                });
            });
        })
        .response;

    let id = ui.make_persistent_id("proof_tech_toggle");
    let click_resp = ui
        .interact(toggle_resp.rect, id, Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    paint_focus_ring(ui, &click_resp, palette, ThemeTokens::RADIUS_CONTROL);
    if click_resp.clicked() {
        *action = Some(ProofAction::ToggleTechnical);
    }

    if expanded {
        ui.add_space(ThemeTokens::SPACE_SM);
        let box_inner_w = (body_w - ThemeTokens::SPACE_MD * 2.0).max(160.0);
        egui::Frame::none()
            .fill(palette.bg_canvas)
            .stroke(Stroke::new(1.0_f32, palette.border_subtle))
            .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
            .inner_margin(egui::Margin::same(ThemeTokens::SPACE_MD))
            .show(ui, |ui| {
                ui.set_width(box_inner_w);
                ui.spacing_mut().item_spacing.y = 6.0;

                if !proof.target_url.is_empty() {
                    ui.label(semibold_text("Target URL", 11.5, palette.text_secondary));
                    ui.add(
                        egui::Label::new(mono_text(&proof.target_url, 11.5, palette.text_primary))
                            .wrap(),
                    );
                }
                if let Some(ep) = &proof.endpoint_url {
                    ui.label(semibold_text("Endpoint POST", 11.5, palette.text_secondary));
                    ui.add(egui::Label::new(mono_text(ep, 11.5, palette.text_primary)).wrap());
                }
                if let Some(resp) = &proof.server_response {
                    ui.label(semibold_text(
                        "Cuplikan Respon Server",
                        11.5,
                        palette.text_secondary,
                    ));
                    ui.add(egui::Label::new(mono_text(resp, 11.5, palette.text_primary)).wrap());
                }
            });
    }
}

/// Render the attendance proof modal when `modal_open` is true (supports 14-day history, empty state, and sample preview).
#[allow(clippy::too_many_arguments)]
pub fn show(
    ctx: &egui::Context,
    snapshot: &GuiSnapshot,
    modal_open: bool,
    active_key: Option<&str>,
    sample_mode: bool,
    copy_feedback_active: bool,
    tech_expanded: bool,
    shot: &ProofScreenshotView<'_>,
    palette: &Palette,
) -> Option<ProofAction> {
    if !modal_open {
        return None;
    }

    enum ModalView<'a> {
        Real {
            proof: &'a ProofPreview,
            current_idx: usize,
            total_proofs: usize,
            is_today: bool,
        },
        Sample(&'static ProofPreview),
        Empty,
    }

    let view = if !snapshot.proofs.is_empty() {
        let proof = active_key
            .and_then(|k| snapshot.proof_by_key(k))
            .or_else(|| snapshot.proofs.first())?;
        let current_idx = snapshot
            .proofs
            .iter()
            .position(|p| p.proof_id == proof.proof_id && p.sig == proof.sig)
            .unwrap_or(0);
        let is_today = !snapshot.today_date.is_empty() && proof.date == snapshot.today_date;
        ModalView::Real {
            proof,
            current_idx,
            total_proofs: snapshot.proofs.len(),
            is_today,
        }
    } else if sample_mode {
        ModalView::Sample(sample_preview())
    } else {
        ModalView::Empty
    };

    let mut action = None;
    let screen = ctx.screen_rect();
    let modal_w = (screen.width() - 32.0).clamp(320.0, 760.0);
    let modal_h = (screen.height() - 48.0).clamp(280.0, 760.0);
    let any_popup_open = ctx.memory(|m| m.any_popup_open());

    let backdrop_resp = egui::Area::new(egui::Id::new("proof_backdrop"))
        .order(egui::Order::Middle)
        .fade_in(false)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            let scrim_alpha = if palette.dark_mode { 175 } else { 135 };
            ui.painter()
                .rect_filled(rect, Rounding::ZERO, Color32::from_black_alpha(scrim_alpha));
            resp
        });
    if backdrop_resp.inner.clicked() && !any_popup_open {
        action = Some(ProofAction::Close);
    }

    let shadow = egui::epaint::Shadow {
        offset: Vec2::new(0.0, 12.0),
        blur: 32.0,
        spread: 0.0,
        color: Color32::from_black_alpha(if palette.dark_mode { 145 } else { 64 }),
    };

    let header_approx_h = 88.0;
    let footer_approx_h = 72.0;
    let body_h = (modal_h - header_approx_h - footer_approx_h - 4.0).max(120.0);
    let is_scanning = snapshot.status == EngineStatus::ScanningNow;

    egui::Area::new(egui::Id::new("proof_modal"))
        .order(egui::Order::Foreground)
        .fade_in(false)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(palette.bg_surface)
                .stroke(Stroke::new(1.0_f32, palette.border_subtle))
                .rounding(Rounding::same(ThemeTokens::RADIUS_MODAL))
                .shadow(shadow)
                .inner_margin(egui::Margin::ZERO)
                .show(ui, |ui| {
                    ui.set_width(modal_w);
                    ui.set_max_width(modal_w);
                    ui.set_max_height(modal_h);
                    ui.spacing_mut().item_spacing.y = 0.0;

                    // Sticky Header
                    egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(20.0, 14.0))
                        .show(ui, |ui| {
                            let header_inner_w = modal_w - 40.0;
                            ui.set_width(header_inner_w);
                            let right_reserve = match &view {
                                ModalView::Real { total_proofs, .. } if *total_proofs > 1 => 156.0,
                                _ => 44.0,
                            };
                            let left_w =
                                (header_inner_w - right_reserve - ThemeTokens::SPACE_SM).max(160.0);

                            ui.horizontal(|ui| {
                                ui.allocate_ui_with_layout(
                                    Vec2::new(left_w, 0.0),
                                    Layout::top_down(Align::Min),
                                    |ui| {
                                        ui.set_max_width(left_w);
                                        ui.horizontal_wrapped(|ui| {
                                            ui.spacing_mut().item_spacing =
                                                Vec2::new(ThemeTokens::SPACE_SM, 4.0);
                                            ui.label(semibold_text(
                                                "Bukti Kehadiran",
                                                17.0,
                                                palette.text_primary,
                                            ));

                                            match &view {
                                                ModalView::Real {
                                                    proof, is_today, ..
                                                } => {
                                                    if *is_today {
                                                        render_header_pill(
                                                            ui,
                                                            "Hari ini",
                                                            palette.emerald_text,
                                                            palette.emerald_bg,
                                                            palette.emerald_border,
                                                            false,
                                                        );
                                                    } else if !proof.date.is_empty() {
                                                        render_header_pill(
                                                            ui,
                                                            &proof.date,
                                                            palette.text_secondary,
                                                            palette.bg_subtle,
                                                            palette.border_subtle,
                                                            true,
                                                        );
                                                    }
                                                    if !proof.proof_id.is_empty() {
                                                        render_header_pill(
                                                            ui,
                                                            &proof.proof_id,
                                                            palette.text_secondary,
                                                            palette.bg_subtle,
                                                            palette.border_subtle,
                                                            true,
                                                        );
                                                    }
                                                }
                                                ModalView::Sample(_) => {
                                                    render_header_pill(
                                                        ui,
                                                        "Contoh",
                                                        palette.amber_text,
                                                        palette.amber_bg,
                                                        palette.amber_border,
                                                        false,
                                                    );
                                                }
                                                ModalView::Empty => {}
                                            }
                                        });
                                    },
                                );

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if action_button(
                                        ui,
                                        palette,
                                        ButtonTone::Ghost,
                                        "",
                                        Some(icons::draw_close),
                                        true,
                                        0.0,
                                    )
                                    .on_hover_text("Tutup (Esc)")
                                    .clicked()
                                    {
                                        action = Some(ProofAction::Close);
                                    }

                                    if let ModalView::Real {
                                        current_idx,
                                        total_proofs,
                                        ..
                                    } = view
                                    {
                                        if total_proofs > 1 {
                                            ui.add_space(4.0);
                                            if compact_nav_button(
                                                ui,
                                                palette,
                                                ">",
                                                current_idx + 1 < total_proofs,
                                            )
                                            .on_hover_text("Bukti berikutnya")
                                            .clicked()
                                            {
                                                action =
                                                    Some(ProofAction::SelectIndex(current_idx + 1));
                                            }
                                            ui.label(mono_text(
                                                format!("{}/{}", current_idx + 1, total_proofs),
                                                12.0,
                                                palette.text_secondary,
                                            ));
                                            if compact_nav_button(ui, palette, "<", current_idx > 0)
                                                .on_hover_text("Bukti sebelumnya")
                                                .clicked()
                                            {
                                                action =
                                                    Some(ProofAction::SelectIndex(current_idx - 1));
                                            }
                                        }
                                    }
                                });
                            });
                        });

                    hairline_divider(ui, palette.border_subtle);

                    // Scrollable Body
                    let scroll_salt = match &view {
                        ModalView::Real { proof, .. } => proof.proof_id.as_str(),
                        ModalView::Sample(_) => "sample_preview",
                        ModalView::Empty => "empty_state",
                    };
                    egui::ScrollArea::vertical()
                        .id_salt(("proof_modal_scroll", scroll_salt))
                        .max_height(body_h)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            egui::Frame::none()
                                .inner_margin(egui::Margin {
                                    left: 20.0,
                                    right: 24.0,
                                    top: 16.0,
                                    bottom: 16.0,
                                })
                                .show(ui, |ui| {
                                    let body_w = (modal_w - 58.0).max(220.0);
                                    ui.set_width(body_w);
                                    ui.set_max_width(body_w);
                                    ui.spacing_mut().item_spacing.y = ThemeTokens::SPACE_MD;

                                    match &view {
                                        ModalView::Real {
                                            proof, is_today, ..
                                        } => {
                                            render_verification_banner(
                                                ui, proof, *is_today, palette,
                                            );
                                            render_screenshot_section(
                                                ui,
                                                palette,
                                                body_w,
                                                ShotSectionCtx {
                                                    has_portal_snapshot: snapshot
                                                        .has_portal_snapshot,
                                                    proof_id_for_capture: Some(
                                                        proof.proof_id.as_str(),
                                                    ),
                                                    is_sample: false,
                                                },
                                                shot,
                                                &mut action,
                                            );
                                            render_receipt_details(ui, proof, palette, body_w);
                                            if !proof.target_url.is_empty()
                                                || proof.endpoint_url.is_some()
                                                || proof.server_response.is_some()
                                            {
                                                render_technical_section(
                                                    ui,
                                                    proof,
                                                    palette,
                                                    tech_expanded,
                                                    body_w,
                                                    &mut action,
                                                );
                                            }
                                        }
                                        ModalView::Sample(proof) => {
                                            render_sample_notice_banner(ui, palette);
                                            render_verification_banner(ui, proof, true, palette);
                                            render_screenshot_section(
                                                ui,
                                                palette,
                                                body_w,
                                                ShotSectionCtx {
                                                    has_portal_snapshot: false,
                                                    proof_id_for_capture: None,
                                                    is_sample: true,
                                                },
                                                shot,
                                                &mut action,
                                            );
                                            render_receipt_details(ui, proof, palette, body_w);
                                            render_technical_section(
                                                ui,
                                                proof,
                                                palette,
                                                tech_expanded,
                                                body_w,
                                                &mut action,
                                            );
                                        }
                                        ModalView::Empty => {
                                            if shot.live_preview_active {
                                                render_screenshot_section(
                                                    ui,
                                                    palette,
                                                    body_w,
                                                    ShotSectionCtx {
                                                        has_portal_snapshot: snapshot
                                                            .has_portal_snapshot,
                                                        proof_id_for_capture: None,
                                                        is_sample: false,
                                                    },
                                                    shot,
                                                    &mut action,
                                                );
                                            } else {
                                                render_empty_state(
                                                    ui,
                                                    palette,
                                                    snapshot.has_portal_snapshot,
                                                    snapshot.portal_snapshot_captured_at.as_deref(),
                                                );
                                            }
                                        }
                                    }
                                });
                        });

                    hairline_divider(ui, palette.border_subtle);

                    // Sticky Footer
                    egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(20.0, 14.0))
                        .show(ui, |ui| {
                            ui.set_width(modal_w - 40.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::splat(ThemeTokens::SPACE_SM);

                                match &view {
                                    ModalView::Real {
                                        proof, is_today, ..
                                    } => {
                                        let copy_label = if copy_feedback_active {
                                            "Tersalin"
                                        } else {
                                            "Salin Bukti"
                                        };
                                        let copy_icon = if copy_feedback_active {
                                            icons::draw_check
                                        } else {
                                            icons::draw_copy
                                        };
                                        let copy_tone = if copy_feedback_active {
                                            ButtonTone::Secondary
                                        } else {
                                            ButtonTone::Primary
                                        };
                                        if action_button(
                                            ui,
                                            palette,
                                            copy_tone,
                                            copy_label,
                                            Some(copy_icon),
                                            true,
                                            0.0,
                                        )
                                        .clicked()
                                        {
                                            action = Some(ProofAction::CopyReceipt);
                                        }

                                        let save_label = if shot.save_in_progress {
                                            "Menyimpan..."
                                        } else if shot.save_feedback_active {
                                            "Gambar tersimpan"
                                        } else {
                                            "Simpan Gambar"
                                        };
                                        let save_icon = if shot.save_feedback_active {
                                            icons::draw_check
                                        } else {
                                            icons::draw_download
                                        };
                                        if action_button(
                                            ui,
                                            palette,
                                            ButtonTone::Secondary,
                                            save_label,
                                            Some(save_icon),
                                            shot.can_save_image && !shot.save_in_progress,
                                            0.0,
                                        )
                                        .clicked()
                                        {
                                            action = Some(ProofAction::SaveImage);
                                        }

                                        if proof.verification == ProofVerification::PostAccepted
                                            && *is_today
                                            && action_button(
                                                ui,
                                                palette,
                                                ButtonTone::Secondary,
                                                "Verifikasi Ulang",
                                                Some(icons::draw_refresh),
                                                !is_scanning,
                                                0.0,
                                            )
                                            .clicked()
                                        {
                                            action = Some(ProofAction::Reverify);
                                        }

                                        if is_safe_http_url(&proof.target_url)
                                            && action_button(
                                                ui,
                                                palette,
                                                ButtonTone::Secondary,
                                                "Buka Portal",
                                                Some(icons::draw_external_link),
                                                true,
                                                0.0,
                                            )
                                            .clicked()
                                        {
                                            action = Some(ProofAction::OpenPortal(
                                                proof.target_url.clone(),
                                            ));
                                        }
                                    }
                                    ModalView::Sample(_) => {
                                        if action_button(
                                            ui,
                                            palette,
                                            ButtonTone::Secondary,
                                            "Kembali",
                                            None,
                                            true,
                                            0.0,
                                        )
                                        .clicked()
                                        {
                                            action = Some(ProofAction::ExitSamplePreview);
                                        }
                                    }
                                    ModalView::Empty => {
                                        if shot.live_preview_active {
                                            let save_label = if shot.save_in_progress {
                                                "Menyimpan..."
                                            } else if shot.save_feedback_active {
                                                "Gambar tersimpan"
                                            } else {
                                                "Simpan Gambar"
                                            };
                                            let save_icon = if shot.save_feedback_active {
                                                icons::draw_check
                                            } else {
                                                icons::draw_download
                                            };
                                            if action_button(
                                                ui,
                                                palette,
                                                ButtonTone::Primary,
                                                save_label,
                                                Some(save_icon),
                                                shot.can_save_image && !shot.save_in_progress,
                                                0.0,
                                            )
                                            .clicked()
                                            {
                                                action = Some(ProofAction::SaveImage);
                                            }
                                            if snapshot.has_portal_snapshot
                                                && action_button(
                                                    ui,
                                                    palette,
                                                    ButtonTone::Secondary,
                                                    "Perbarui Screenshot",
                                                    Some(icons::draw_refresh),
                                                    !shot.is_capturing,
                                                    0.0,
                                                )
                                                .clicked()
                                            {
                                                action =
                                                    Some(ProofAction::CaptureLivePortalPreview);
                                            }
                                            if action_button(
                                                ui,
                                                palette,
                                                ButtonTone::Secondary,
                                                "Kembali",
                                                None,
                                                true,
                                                0.0,
                                            )
                                            .clicked()
                                            {
                                                action = Some(ProofAction::ExitLivePortalPreview);
                                            }
                                        } else {
                                            if snapshot.has_portal_snapshot
                                                && action_button(
                                                    ui,
                                                    palette,
                                                    ButtonTone::Primary,
                                                    "Preview Halaman Absen",
                                                    Some(icons::draw_eye),
                                                    true,
                                                    0.0,
                                                )
                                                .clicked()
                                            {
                                                action =
                                                    Some(ProofAction::CaptureLivePortalPreview);
                                            }
                                            if action_button(
                                                ui,
                                                palette,
                                                ButtonTone::Secondary,
                                                "Lihat Contoh Bukti",
                                                Some(icons::draw_eye),
                                                true,
                                                0.0,
                                            )
                                            .clicked()
                                            {
                                                action = Some(ProofAction::EnterSamplePreview);
                                            }
                                        }
                                    }
                                }

                                if action_button(
                                    ui,
                                    palette,
                                    ButtonTone::Ghost,
                                    "Tutup",
                                    None,
                                    true,
                                    0.0,
                                )
                                .clicked()
                                {
                                    action = Some(ProofAction::Close);
                                }
                            });
                        });
                });
        });

    action
}

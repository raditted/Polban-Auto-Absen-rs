//! Activity timeline panel displaying recent monitoring and attendance events.

use std::borrow::Cow;

use egui::{Align, Color32, CursorIcon, Layout, Pos2, RichText, Rounding, Sense, Stroke, Vec2};

use crate::bridge::{ActivityItem, ActivityKind, GuiSnapshot};
use crate::theme::{Palette, ThemeTokens};
use crate::ui::{hairline_divider, mono_text, sanitize_dash, semibold_text};

/// User action triggered from the activity timeline panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityAction {
    OpenProof(String),
}

fn activity_dot_color(kind: ActivityKind, palette: &Palette) -> egui::Color32 {
    match kind {
        ActivityKind::Ok => palette.emerald_border,
        ActivityKind::Fail => palette.red_border,
        ActivityKind::Warn => palette.amber_border,
        ActivityKind::Info => palette.blue_border,
        ActivityKind::Skip => palette.text_secondary,
    }
}

fn friendly_code_label(raw_code: &str) -> Option<&'static str> {
    let clean = raw_code.trim_matches(|c| c == '[' || c == ']').trim();
    match clean {
        "LOGIN" => Some("Portal"),
        "SCAN" => Some("Pemindaian"),
        "ENGINE" => Some("Mesin"),
        "ABSEN" => Some("Presensi"),
        "CONFIG" => Some("Konfigurasi"),
        "ERROR" => Some("Gangguan"),
        _ => None,
    }
}

fn friendly_message(raw: &str) -> Cow<'_, str> {
    let cleaned = sanitize_dash(raw);
    let trimmed = cleaned.trim();
    if trimmed.eq_ignore_ascii_case("Authenticated") {
        Cow::Borrowed("Terautentikasi di portal akademik.")
    } else if let Some(rest) = trimmed.strip_prefix("HTTP aktif.") {
        let rest = rest.trim();
        if rest.is_empty() {
            Cow::Borrowed("Pemantauan HTTP aktif.")
        } else {
            Cow::Owned(format!(
                "Pemantauan aktif ({})",
                rest.replace("interval=", "interval ")
                    .replace("s loop=True", " dtk)")
                    .replace("s loop=False", " dtk, sekali jalan)")
                    .trim_end_matches(')')
            ))
        }
    } else {
        cleaned
    }
}

pub fn show(
    ui: &mut egui::Ui,
    snapshot: &GuiSnapshot,
    palette: &Palette,
    compact: bool,
    scrollable: bool,
) -> Option<ActivityAction> {
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

            ui.horizontal(|ui| {
                ui.label(semibold_text(
                    "Riwayat Aktivitas",
                    15.0,
                    palette.text_primary,
                ));
                if !snapshot.activities.is_empty() {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
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
                        render_activity_list(ui, snapshot, palette, &mut action);
                    });
            } else {
                render_activity_list(ui, snapshot, palette, &mut action);
            }
        });

    action
}

fn render_activity_list(
    ui: &mut egui::Ui,
    snapshot: &GuiSnapshot,
    palette: &Palette,
    action: &mut Option<ActivityAction>,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let total = snapshot.activities.len();
        for (idx, item) in snapshot.activities.iter().rev().enumerate() {
            render_activity_row(ui, idx, item, snapshot, palette, action);
            if idx + 1 < total {
                hairline_divider(ui, palette.border_subtle);
            }
        }
    });
}

fn render_activity_row(
    ui: &mut egui::Ui,
    idx: usize,
    item: &ActivityItem,
    snapshot: &GuiSnapshot,
    palette: &Palette,
    action: &mut Option<ActivityAction>,
) {
    let dot_color = activity_dot_color(item.kind, palette);
    let clean_msg = friendly_message(&item.message);
    let category = friendly_code_label(&item.code);
    let proof_target = (item.kind == ActivityKind::Ok && item.code == "ABSEN")
        .then_some(item.proof_key.as_deref())
        .flatten()
        .filter(|k| snapshot.proof_by_key(k).is_some());

    let row_id = proof_target.map(|k| ui.make_persistent_id(("activity_proof_row", idx, k)));
    let is_hovered = row_id
        .map(|id| ui.ctx().data(|d| d.get_temp::<bool>(id).unwrap_or(false)))
        .unwrap_or(false);
    let row_fill = if is_hovered {
        palette.bg_subtle
    } else {
        Color32::TRANSPARENT
    };
    let h_margin = if proof_target.is_some() { 4.0 } else { 0.0 };

    let frame_resp = egui::Frame::none()
        .fill(row_fill)
        .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
        .inner_margin(egui::Margin::symmetric(h_margin, 10.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let avail = ui.available_width();
            let time_w = if item.timestamp.trim().is_empty() {
                0.0
            } else {
                64.0
            };
            let dot_col_w = 16.0;
            let body_w = (avail - dot_col_w - time_w - ThemeTokens::SPACE_SM * 2.0).max(110.0);

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = ThemeTokens::SPACE_SM;

                // Left colored timeline dot
                let (dot_rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 18.0), Sense::hover());
                if ui.is_rect_visible(dot_rect) {
                    ui.painter().circle_filled(
                        Pos2::new(dot_rect.center().x, dot_rect.top() + 8.0),
                        4.0,
                        dot_color,
                    );
                }

                // Middle category (+ optional Bukti tag) + wrapped message
                ui.allocate_ui_with_layout(
                    Vec2::new(body_w, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_max_width(body_w);
                        if category.is_some() || proof_target.is_some() {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                if let Some(cat) = category {
                                    ui.label(semibold_text(cat, 12.0, palette.text_secondary));
                                }
                                if proof_target.is_some() {
                                    egui::Frame::none()
                                        .fill(palette.emerald_bg)
                                        .stroke(Stroke::new(1.0_f32, palette.emerald_border))
                                        .rounding(Rounding::same(ThemeTokens::RADIUS_TAG))
                                        .inner_margin(egui::Margin::symmetric(6.0, 1.5))
                                        .show(ui, |ui| {
                                            ui.label(semibold_text(
                                                "Bukti",
                                                11.0,
                                                palette.emerald_text,
                                            ));
                                        });
                                }
                            });
                            ui.add_space(2.0);
                        }
                        ui.add(
                            egui::Label::new(
                                RichText::new(clean_msg.as_ref())
                                    .size(12.5)
                                    .color(palette.text_primary),
                            )
                            .wrap(),
                        );
                    },
                );

                // Right-aligned tabular timestamp
                if !item.timestamp.trim().is_empty() {
                    ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                        ui.label(mono_text(&item.timestamp, 11.5, palette.text_secondary));
                    });
                }
            });
        })
        .response;

    if let (Some(key), Some(id)) = (proof_target, row_id) {
        let click_resp = ui
            .interact(frame_resp.rect, id, Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        ui.ctx()
            .data_mut(|d| d.insert_temp(id, click_resp.hovered()));
        if click_resp.clicked() {
            *action = Some(ActivityAction::OpenProof(key.to_string()));
        }
    }
}

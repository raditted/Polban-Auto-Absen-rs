//! Onboarding card and Settings modal for credentials, theme, desktop behavior, and diagnostics.

use std::fmt;

use egui::{
    Align, Color32, CursorIcon, FontFamily, Id, Layout, Pos2, Rect, RichText, Rounding, Sense,
    Stroke, Vec2,
};

use crate::credentials::CredentialBackend;
use crate::prefs::{AppPrefs, DarkThemePreset, LightThemePreset, ThemePreference};
use crate::theme::{AppearanceConfig, Palette, ThemeTokens};
use crate::ui::{
    action_button, hairline_divider, icons, labeled_text_input, paint_focus_ring,
    segmented_control, semibold_text, status_badge, switch_row, ButtonTone, SemanticTone,
};

/// Editable settings and onboarding form state; `Debug` redacts the password buffer.
#[derive(Clone, Default)]
pub struct SettingsState {
    pub open: bool,
    pub nim: String,
    pub password: String,
    pub show_password: bool,
    pub base_url: String,
    pub interval: u64,
    pub interval_text: String,
    pub delay_min: f64,
    pub delay_min_text: String,
    pub delay_max: f64,
    pub delay_max_text: String,
    pub max_retry: u32,
    pub max_retry_text: String,
    pub retry_wait: f64,
    pub theme: ThemePreference,
    pub light_preset: LightThemePreset,
    pub dark_preset: DarkThemePreset,
    pub minimize_to_tray: bool,
    pub autostart: bool,
    pub notifications_enabled: bool,
    pub auto_start_monitoring: bool,
    pub status_message: Option<String>,
    pub status_is_error: bool,
}

impl fmt::Debug for SettingsState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SettingsState")
            .field("open", &self.open)
            .field("nim", &self.nim)
            .field("password", &"[REDACTED]")
            .field("base_url", &self.base_url)
            .field("interval", &self.interval)
            .field("theme", &self.theme)
            .field("light_preset", &self.light_preset)
            .field("dark_preset", &self.dark_preset)
            .finish()
    }
}

impl SettingsState {
    pub fn from_prefs(prefs: &AppPrefs, password: String) -> Self {
        Self {
            open: false,
            nim: prefs.nim.clone(),
            password,
            show_password: false,
            base_url: prefs.base_url.clone(),
            interval: prefs.interval,
            interval_text: prefs.interval.to_string(),
            delay_min: prefs.delay_min,
            delay_min_text: format!("{:.1}", prefs.delay_min),
            delay_max: prefs.delay_max,
            delay_max_text: format!("{:.1}", prefs.delay_max),
            max_retry: prefs.max_retry,
            max_retry_text: prefs.max_retry.to_string(),
            retry_wait: prefs.retry_wait,
            theme: prefs.theme,
            light_preset: prefs.light_preset,
            dark_preset: prefs.dark_preset,
            minimize_to_tray: prefs.minimize_to_tray,
            autostart: prefs.autostart,
            notifications_enabled: prefs.notifications_enabled,
            auto_start_monitoring: prefs.auto_start_monitoring,
            status_message: None,
            status_is_error: false,
        }
    }

    /// Build the live `AppearanceConfig` from the form's current mode and theme presets.
    pub fn appearance(&self) -> AppearanceConfig {
        AppearanceConfig {
            mode: self.theme,
            light_preset: self.light_preset,
            dark_preset: self.dark_preset,
        }
    }

    /// Validate and sync editable numeric strings into typed fields.
    pub fn validate_and_sync(&mut self) -> Result<(), String> {
        let interval: u64 = self.interval_text.trim().parse().map_err(|_| {
            "Interval pemantauan harus berupa angka bulat (minimal 15 detik).".to_string()
        })?;
        if interval < 15 {
            return Err(
                "Interval pemantauan minimal 15 detik agar tidak membebani portal.".to_string(),
            );
        }

        let max_retry: u32 = self.max_retry_text.trim().parse().map_err(|_| {
            "Maksimum percobaan ulang harus berupa angka antara 1 sampai 10.".to_string()
        })?;
        if !(1..=10).contains(&max_retry) {
            return Err("Maksimum percobaan ulang harus bernilai antara 1 sampai 10.".to_string());
        }

        let delay_min: f64 = self
            .delay_min_text
            .trim()
            .parse()
            .map_err(|_| "Jeda klik minimum harus berupa angka desimal valid.".to_string())?;
        let delay_max: f64 = self
            .delay_max_text
            .trim()
            .parse()
            .map_err(|_| "Jeda klik maksimum harus berupa angka desimal valid.".to_string())?;
        if !delay_min.is_finite() || delay_min < 0.0 {
            return Err("Jeda klik minimum tidak boleh negatif.".to_string());
        }
        if !delay_max.is_finite() || delay_max < delay_min {
            return Err(
                "Jeda klik maksimum harus lebih besar atau sama dengan jeda minimum.".to_string(),
            );
        }

        let url = self.base_url.trim().trim_end_matches('/');
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err("URL portal akademik harus diawali http:// atau https://.".to_string());
        }

        self.interval = interval;
        self.max_retry = max_retry;
        self.delay_min = delay_min;
        self.delay_max = delay_max;
        Ok(())
    }

    pub fn apply_to_prefs(&mut self, prefs: &mut AppPrefs) {
        let _ = self.validate_and_sync();
        prefs.nim = self.nim.trim().to_string();
        prefs.base_url = self.base_url.trim().trim_end_matches('/').to_string();
        prefs.target_url = format!("{}/ajar/absen", prefs.base_url);
        prefs.interval = self.interval.max(15);
        prefs.delay_min = self.delay_min.max(0.0);
        prefs.delay_max = self.delay_max.max(prefs.delay_min);
        prefs.max_retry = self.max_retry.clamp(1, 10);
        prefs.retry_wait = self.retry_wait.max(0.0);
        prefs.theme = self.theme;
        prefs.light_preset = self.light_preset;
        prefs.dark_preset = self.dark_preset;
        prefs.minimize_to_tray = self.minimize_to_tray;
        prefs.autostart = self.autostart;
        prefs.notifications_enabled = self.notifications_enabled;
        prefs.auto_start_monitoring = self.auto_start_monitoring;
    }
}

/// User action returned by the settings modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAction {
    Close,
    Save,
    ImportDotenv,
    ExportDiagnosticHtml,
    PreviewTheme(AppearanceConfig),
}

/// User action returned by the inline first-run onboarding card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardingAction {
    SaveAndStart,
    ImportDotenv,
}

fn render_credential_fields(
    ui: &mut egui::Ui,
    state: &mut SettingsState,
    backend: CredentialBackend,
    palette: &Palette,
    show_import_inline: bool,
    action: &mut Option<SettingsAction>,
) {
    let full_w = ui.available_width();
    labeled_text_input(
        ui,
        palette,
        "NIM Mahasiswa",
        &mut state.nim,
        "Contoh: 251511001",
        true,
        full_w,
    );

    ui.add_space(ThemeTokens::SPACE_MD);

    ui.label(semibold_text(
        "Kata Sandi Akademik",
        12.5,
        palette.text_primary,
    ));
    ui.add_space(5.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = ThemeTokens::SPACE_SM;
        let toggle_w = if state.show_password { 130.0 } else { 96.0 };
        let input_w = (full_w - toggle_w - ThemeTokens::SPACE_SM).max(120.0);
        let edit = egui::TextEdit::singleline(&mut state.password)
            .hint_text(RichText::new("Masukkan kata sandi portal").color(palette.text_secondary))
            .password(!state.show_password)
            .desired_width(input_w)
            .margin(egui::Margin::symmetric(ThemeTokens::SPACE_MD, 10.0))
            .font(egui::FontId::new(13.0, egui::FontFamily::Proportional));
        let resp = ui
            .add_sized(Vec2::new(input_w, ThemeTokens::MIN_TARGET_HEIGHT), edit)
            .on_hover_cursor(egui::CursorIcon::Text);
        paint_focus_ring(ui, &resp, palette, ThemeTokens::RADIUS_CONTROL);

        let (eye_icon, eye_label) = if state.show_password {
            (
                icons::draw_eye_off as fn(&egui::Painter, egui::Rect, egui::Color32),
                "Sembunyikan",
            )
        } else {
            (
                icons::draw_eye as fn(&egui::Painter, egui::Rect, egui::Color32),
                "Lihat",
            )
        };
        if action_button(
            ui,
            palette,
            ButtonTone::Secondary,
            eye_label,
            Some(eye_icon),
            true,
            toggle_w,
        )
        .clicked()
        {
            state.show_password = !state.show_password;
        }
    });

    ui.add_space(ThemeTokens::SPACE_SM);

    let (badge_tone, badge_icon, badge_label) = match backend {
        CredentialBackend::OsKeyring => (
            SemanticTone::Emerald,
            icons::draw_lock as fn(&egui::Painter, egui::Rect, egui::Color32),
            "Disimpan di Keychain Sistem",
        ),
        CredentialBackend::MemoryFallback => (
            SemanticTone::Amber,
            icons::draw_alert as fn(&egui::Painter, egui::Rect, egui::Color32),
            "Memori Sesi",
        ),
    };

    if show_import_inline {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(ThemeTokens::SPACE_SM);
            status_badge(ui, palette, badge_tone, badge_icon, badge_label);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if action_button(
                    ui,
                    palette,
                    ButtonTone::Secondary,
                    "Impor dari .env",
                    Some(icons::draw_folder),
                    true,
                    0.0,
                )
                .clicked()
                {
                    *action = Some(SettingsAction::ImportDotenv);
                }
            });
        });
        if backend == CredentialBackend::MemoryFallback {
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Keychain sistem tidak tersedia; kredensial aktif selama sesi berjalan.",
                )
                .size(11.5)
                .color(palette.text_secondary),
            );
        }
    } else {
        ui.horizontal_wrapped(|ui| {
            status_badge(ui, palette, badge_tone, badge_icon, badge_label);
            if backend == CredentialBackend::MemoryFallback {
                ui.label(
                    RichText::new(
                        "Keychain sistem tidak tersedia; kredensial aktif selama sesi berjalan.",
                    )
                    .size(11.5)
                    .color(palette.text_secondary),
                );
            }
        });
    }
}

fn render_status_banner(ui: &mut egui::Ui, state: &SettingsState, palette: &Palette) {
    if let Some(msg) = &state.status_message {
        let (fg, bg, border) = if state.status_is_error {
            (palette.red_text, palette.red_bg, palette.red_border)
        } else {
            (
                palette.emerald_text,
                palette.emerald_bg,
                palette.emerald_border,
            )
        };
        egui::Frame::none()
            .fill(bg)
            .stroke(Stroke::new(1.0_f32, border))
            .rounding(Rounding::same(ThemeTokens::RADIUS_CONTROL))
            .inner_margin(egui::Margin::same(ThemeTokens::SPACE_MD))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(msg).size(12.5).color(fg));
            });
    }
}

fn draw_chevron_down(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width().min(rect.height()) * 0.25);
    let stroke = Stroke::new(1.5_f32, color);
    let p0 = Pos2::new(r.left(), r.center().y - r.height() * 0.20);
    let p1 = Pos2::new(r.center().x, r.center().y + r.height() * 0.26);
    let p2 = Pos2::new(r.right(), r.center().y - r.height() * 0.20);
    painter.add(egui::Shape::line(vec![p0, p1, p2], stroke));
}

fn render_color_token_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    token_color: Color32,
) {
    let row_w = ui.available_width();
    let row_h = 34.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(row_w, row_h), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }

    let left_pad = ThemeTokens::SPACE_MD;
    let right_pad = ThemeTokens::SPACE_MD;
    let label_galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::new(13.0, FontFamily::Proportional),
        palette.text_primary,
    );
    ui.painter().galley(
        Pos2::new(
            rect.left() + left_pad,
            rect.center().y - label_galley.size().y * 0.5,
        ),
        label_galley,
        palette.text_primary,
    );

    let hex_str = Palette::hex_rgb(token_color);
    let hex_galley = ui.painter().layout_no_wrap(
        hex_str,
        egui::FontId::new(12.0, FontFamily::Monospace),
        palette.text_primary,
    );
    let swatch_size = 14.0;
    let inner_gap = 7.0;
    let pad_x = 9.0;
    let pill_w = pad_x * 2.0 + swatch_size + inner_gap + hex_galley.size().x;
    let pill_h = 24.0;
    let pill_rect = Rect::from_min_size(
        Pos2::new(
            rect.right() - right_pad - pill_w,
            rect.center().y - pill_h * 0.5,
        ),
        Vec2::new(pill_w, pill_h),
    );
    ui.painter().rect(
        pill_rect,
        Rounding::same(ThemeTokens::RADIUS_CONTROL),
        palette.bg_subtle,
        Stroke::new(1.0_f32, palette.border_subtle),
    );

    let swatch_rect = Rect::from_min_size(
        Pos2::new(
            pill_rect.left() + pad_x,
            pill_rect.center().y - swatch_size * 0.5,
        ),
        Vec2::splat(swatch_size),
    );
    ui.painter().rect(
        swatch_rect,
        Rounding::same(3.5),
        token_color,
        Stroke::new(1.0_f32, palette.border_interactive),
    );

    ui.painter().galley(
        Pos2::new(
            swatch_rect.right() + inner_gap,
            pill_rect.center().y - hex_galley.size().y * 0.5,
        ),
        hex_galley,
        palette.text_primary,
    );
}

fn render_preset_dropdown_row<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    palette: &Palette,
    id_salt: &str,
    current: &mut T,
    all_presets: &[T],
    label_fn: fn(T) -> &'static str,
) -> bool {
    let mut changed = false;
    let row_w = ui.available_width();
    let row_h = 36.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(row_w, row_h), Sense::hover());

    let left_pad = ThemeTokens::SPACE_MD;
    let right_pad = ThemeTokens::SPACE_MD;
    let pill_w = 168.0_f32.min((row_w - 88.0).max(116.0));
    let pill_h = 28.0;
    let pill_rect = Rect::from_min_size(
        Pos2::new(
            rect.right() - right_pad - pill_w,
            rect.center().y - pill_h * 0.5,
        ),
        Vec2::new(pill_w, pill_h),
    );

    let popup_id = Id::new(id_salt);
    let trigger_resp = ui
        .interact(pill_rect, popup_id.with("trigger"), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    if trigger_resp.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(popup_id));
    }

    let is_open = ui.memory(|mem| mem.is_popup_open(popup_id));

    if ui.is_rect_visible(rect) {
        let label_galley = ui.painter().layout_no_wrap(
            "Preset".to_string(),
            egui::FontId::new(13.0, FontFamily::Proportional),
            palette.text_primary,
        );
        ui.painter().galley(
            Pos2::new(
                rect.left() + left_pad,
                rect.center().y - label_galley.size().y * 0.5,
            ),
            label_galley,
            palette.text_primary,
        );

        let (pill_fill, pill_stroke) = if is_open || trigger_resp.hovered() {
            (
                palette.bg_subtle,
                Stroke::new(1.2_f32, palette.accent_text),
            )
        } else {
            (
                palette.bg_surface,
                Stroke::new(1.0_f32, palette.border_interactive),
            )
        };
        ui.painter().rect(
            pill_rect,
            Rounding::same(ThemeTokens::RADIUS_CONTROL),
            pill_fill,
            pill_stroke,
        );

        let val_galley = ui.painter().layout_no_wrap(
            label_fn(*current).to_string(),
            egui::FontId::new(
                12.5,
                FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()),
            ),
            palette.text_primary,
        );
        ui.painter().galley(
            Pos2::new(
                pill_rect.left() + 10.0,
                pill_rect.center().y - val_galley.size().y * 0.5,
            ),
            val_galley,
            palette.text_primary,
        );

        let chev_rect = Rect::from_center_size(
            Pos2::new(pill_rect.right() - 14.0, pill_rect.center().y),
            Vec2::splat(12.0),
        );
        draw_chevron_down(ui.painter(), chev_rect, palette.text_secondary);
        paint_focus_ring(ui, &trigger_resp, palette, ThemeTokens::RADIUS_CONTROL);
    }

    if is_open {
        let screen = ui.ctx().screen_rect();
        let item_h = 28.0;
        let menu_w = 184.0_f32.min((screen.width() - 24.0).max(140.0));
        let menu_h = (all_presets.len() as f32) * item_h + 8.0;
        let menu_x = (pill_rect.right() - menu_w)
            .clamp(screen.left() + 8.0, (screen.right() - menu_w - 8.0).max(8.0));
        let preferred_y = if pill_rect.bottom() + 4.0 + menu_h <= screen.bottom() - 8.0 {
            pill_rect.bottom() + 4.0
        } else if pill_rect.top() - 4.0 - menu_h >= screen.top() + 8.0 {
            pill_rect.top() - 4.0 - menu_h
        } else {
            (screen.bottom() - menu_h - 8.0).max(screen.top() + 8.0)
        };

        let shadow = egui::epaint::Shadow {
            offset: Vec2::new(0.0, 6.0),
            blur: 18.0,
            spread: 0.0,
            color: Color32::from_black_alpha(if palette.dark_mode { 140 } else { 56 }),
        };

        let area_resp = egui::Area::new(popup_id.with("menu"))
            .order(egui::Order::Tooltip)
            .fade_in(false)
            .fixed_pos(Pos2::new(menu_x, preferred_y))
            .show(ui.ctx(), |ui| {
                egui::Frame::none()
                    .fill(palette.bg_surface)
                    .stroke(Stroke::new(1.0_f32, palette.border_interactive))
                    .rounding(Rounding::same(ThemeTokens::RADIUS_CARD))
                    .shadow(shadow)
                    .inner_margin(egui::Margin::same(4.0))
                    .show(ui, |ui| {
                        let inner_w = menu_w - 8.0;
                        ui.set_width(inner_w);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for &preset in all_presets {
                            let is_selected = preset == *current;
                            let (item_rect, item_resp) = ui.allocate_exact_size(
                                Vec2::new(inner_w, item_h),
                                Sense::click(),
                            );
                            let item_resp = item_resp.on_hover_cursor(CursorIcon::PointingHand);
                            if item_resp.clicked() {
                                *current = preset;
                                changed = true;
                                ui.memory_mut(|mem| mem.close_popup());
                            }
                            if ui.is_rect_visible(item_rect) {
                                if is_selected || item_resp.hovered() {
                                    ui.painter().rect_filled(
                                        item_rect,
                                        Rounding::same(6.0),
                                        palette.bg_subtle,
                                    );
                                }
                                let fg = if is_selected {
                                    palette.accent_text
                                } else {
                                    palette.text_primary
                                };
                                let font_id = if is_selected {
                                    egui::FontId::new(
                                        12.5,
                                        FontFamily::Name(
                                            ThemeTokens::FONT_SEMIBOLD_FAMILY.into(),
                                        ),
                                    )
                                } else {
                                    egui::FontId::new(12.5, FontFamily::Proportional)
                                };
                                let text_galley = ui.painter().layout_no_wrap(
                                    label_fn(preset).to_string(),
                                    font_id,
                                    fg,
                                );
                                ui.painter().galley(
                                    Pos2::new(
                                        item_rect.left() + 10.0,
                                        item_rect.center().y - text_galley.size().y * 0.5,
                                    ),
                                    text_galley,
                                    fg,
                                );
                                if is_selected {
                                    let check_rect = Rect::from_center_size(
                                        Pos2::new(item_rect.right() - 14.0, item_rect.center().y),
                                        Vec2::splat(13.0),
                                    );
                                    icons::draw_check(ui.painter(), check_rect, fg);
                                }
                                paint_focus_ring(ui, &item_resp, palette, 6.0);
                            }
                        }
                    });
            });

        let clicked_outside = ui.input(|i| {
            i.pointer.any_pressed()
                && i.pointer.interact_pos().is_some_and(|pos| {
                    !pill_rect.contains(pos) && !area_resp.response.rect.contains(pos)
                })
        });
        if !trigger_resp.clicked() && clicked_outside {
            ui.memory_mut(|mem| mem.close_popup());
        }
    }

    changed
}

/// Render the inline first-run onboarding card directly in the main area when no account is set.
pub fn show_onboarding(
    ui: &mut egui::Ui,
    state: &mut SettingsState,
    backend: CredentialBackend,
    palette: &Palette,
    compact: bool,
) -> Option<OnboardingAction> {
    let mut action = None;
    let max_card_w = ui.available_width().min(540.0);

    ui.vertical_centered(|ui| {
        ui.allocate_ui(Vec2::new(max_card_w, 0.0), |ui| {
            egui::Frame::none()
                .fill(palette.bg_surface)
                .stroke(Stroke::new(1.0_f32, palette.border_subtle))
                .rounding(Rounding::same(ThemeTokens::RADIUS_CARD))
                .inner_margin(egui::Margin::same(if compact {
                    ThemeTokens::SPACE_LG
                } else {
                    ThemeTokens::SPACE_2XL
                }))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    ui.label(semibold_text(
                        "Masuk ke Portal Akademik Polban",
                        18.0,
                        palette.text_primary,
                    ));
                    ui.add_space(ThemeTokens::SPACE_XS);
                    ui.label(
                        RichText::new(
                            "Masukkan NIM dan kata sandi portal akademik untuk memantau jadwal perkuliahan dan mencatat kehadiran otomatis.",
                        )
                        .size(13.0)
                        .color(palette.text_secondary),
                    );

                    ui.add_space(ThemeTokens::SPACE_LG);
                    let mut unused = None;
                    render_credential_fields(ui, state, backend, palette, false, &mut unused);

                    if state.status_message.is_some() {
                        ui.add_space(ThemeTokens::SPACE_MD);
                        render_status_banner(ui, state, palette);
                    }

                    ui.add_space(ThemeTokens::SPACE_LG);
                    if compact {
                        let w = ui.available_width();
                        if action_button(
                            ui,
                            palette,
                            ButtonTone::Primary,
                            "Simpan & Mulai Pantau",
                            Some(icons::draw_play),
                            true,
                            w,
                        )
                        .clicked()
                        {
                            action = Some(OnboardingAction::SaveAndStart);
                        }
                        ui.add_space(ThemeTokens::SPACE_SM);
                        if action_button(
                            ui,
                            palette,
                            ButtonTone::Secondary,
                            "Impor dari .env",
                            Some(icons::draw_folder),
                            true,
                            w,
                        )
                        .clicked()
                        {
                            action = Some(OnboardingAction::ImportDotenv);
                        }
                    } else {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::splat(ThemeTokens::SPACE_SM);
                            if action_button(
                                ui,
                                palette,
                                ButtonTone::Primary,
                                "Simpan & Mulai Pantau",
                                Some(icons::draw_play),
                                true,
                                0.0,
                            )
                            .clicked()
                            {
                                action = Some(OnboardingAction::SaveAndStart);
                            }
                            if action_button(
                                ui,
                                palette,
                                ButtonTone::Secondary,
                                "Impor dari .env",
                                Some(icons::draw_folder),
                                true,
                                0.0,
                            )
                            .clicked()
                            {
                                action = Some(OnboardingAction::ImportDotenv);
                            }
                        });
                    }
                });
        });
    });

    action
}

pub fn show(
    ctx: &egui::Context,
    state: &mut SettingsState,
    backend: CredentialBackend,
    palette: &Palette,
) -> Option<SettingsAction> {
    if !state.open {
        return None;
    }

    let mut action = None;
    let screen = ctx.screen_rect();
    let modal_w = (screen.width() - 32.0).clamp(320.0, 560.0);
    let modal_h = (screen.height() - 48.0).clamp(280.0, 680.0);
    let any_popup_open = ctx.memory(|m| m.any_popup_open());

    // Full-window dimmed backdrop scrim (click outside modal to close)
    let backdrop_resp = egui::Area::new(egui::Id::new("settings_backdrop"))
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
        action = Some(SettingsAction::Close);
    }

    let shadow = egui::epaint::Shadow {
        offset: Vec2::new(0.0, 12.0),
        blur: 32.0,
        spread: 0.0,
        color: Color32::from_black_alpha(if palette.dark_mode { 145 } else { 64 }),
    };

    let header_approx_h = 74.0;
    let footer_approx_h = 72.0;
    let body_h = (modal_h - header_approx_h - footer_approx_h - 4.0).max(120.0);

    egui::Area::new(egui::Id::new("settings_modal"))
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
                            ui.set_width(modal_w - 40.0);
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(semibold_text(
                                        "Pengaturan",
                                        17.0,
                                        palette.text_primary,
                                    ));
                                    ui.add_space(2.0);
                                    ui.label(
                                        RichText::new(
                                            "Akun portal, tampilan, dan integrasi desktop",
                                        )
                                        .size(12.0)
                                        .color(palette.text_secondary),
                                    );
                                });
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
                                        action = Some(SettingsAction::Close);
                                    }
                                });
                            });
                        });

                    hairline_divider(ui, palette.border_subtle);

                    // Scrollable Body with generous right padding for scrollbar clearance
                    egui::ScrollArea::vertical()
                        .id_salt("settings_modal_scroll")
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

                                    // 1. Account Section
                                    ui.label(semibold_text(
                                        "Akun Portal Akademik",
                                        13.5,
                                        palette.text_primary,
                                    ));
                                    ui.add_space(ThemeTokens::SPACE_SM);
                                    render_credential_fields(
                                        ui,
                                        state,
                                        backend,
                                        palette,
                                        true,
                                        &mut action,
                                    );

                                    ui.add_space(ThemeTokens::SPACE_LG);
                                    hairline_divider(ui, palette.border_subtle);
                                    ui.add_space(ThemeTokens::SPACE_LG);

                                    // 2. Theme Segmented Control & Antigravity Appearance Cards
                                    ui.label(semibold_text(
                                        "Tema Tampilan",
                                        13.5,
                                        palette.text_primary,
                                    ));
                                    ui.add_space(ThemeTokens::SPACE_SM);
                                    let selected_idx = match state.theme {
                                        ThemePreference::System => 0,
                                        ThemePreference::Light => 1,
                                        ThemePreference::Dark => 2,
                                    };
                                    if let Some(new_idx) = segmented_control(
                                        ui,
                                        palette,
                                        selected_idx,
                                        [
                                            ("Sistem", icons::draw_monitor),
                                            ("Terang", icons::draw_sun),
                                            ("Gelap", icons::draw_moon),
                                        ],
                                    ) {
                                        let next_theme = match new_idx {
                                            0 => ThemePreference::System,
                                            1 => ThemePreference::Light,
                                            _ => ThemePreference::Dark,
                                        };
                                        state.theme = next_theme;
                                        action =
                                            Some(SettingsAction::PreviewTheme(state.appearance()));
                                    }

                                    ui.add_space(ThemeTokens::SPACE_MD);

                                    // Light Theme Card
                                    ui.label(semibold_text(
                                        "Light Theme",
                                        12.5,
                                        palette.text_primary,
                                    ));
                                    ui.add_space(6.0);
                                    egui::Frame::none()
                                        .fill(palette.bg_surface)
                                        .stroke(Stroke::new(1.0_f32, palette.border_subtle))
                                        .rounding(Rounding::same(ThemeTokens::RADIUS_CARD))
                                        .inner_margin(egui::Margin::ZERO)
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.spacing_mut().item_spacing.y = 0.0;
                                            if render_preset_dropdown_row(
                                                ui,
                                                palette,
                                                "light_theme_preset_popup",
                                                &mut state.light_preset,
                                                &LightThemePreset::ALL,
                                                LightThemePreset::label,
                                            ) {
                                                action = Some(SettingsAction::PreviewTheme(
                                                    state.appearance(),
                                                ));
                                            }
                                            let light_palette =
                                                Palette::light_for_preset(state.light_preset);
                                            hairline_divider(ui, palette.border_subtle);
                                            render_color_token_row(
                                                ui,
                                                palette,
                                                "Background",
                                                light_palette.bg_surface,
                                            );
                                            hairline_divider(ui, palette.border_subtle);
                                            render_color_token_row(
                                                ui,
                                                palette,
                                                "Foreground",
                                                light_palette.text_primary,
                                            );
                                            hairline_divider(ui, palette.border_subtle);
                                            render_color_token_row(
                                                ui,
                                                palette,
                                                "Accent",
                                                light_palette.accent,
                                            );
                                        });

                                    ui.add_space(ThemeTokens::SPACE_MD);

                                    // Dark Theme Card
                                    ui.label(semibold_text(
                                        "Dark Theme",
                                        12.5,
                                        palette.text_primary,
                                    ));
                                    ui.add_space(6.0);
                                    egui::Frame::none()
                                        .fill(palette.bg_surface)
                                        .stroke(Stroke::new(1.0_f32, palette.border_subtle))
                                        .rounding(Rounding::same(ThemeTokens::RADIUS_CARD))
                                        .inner_margin(egui::Margin::ZERO)
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.spacing_mut().item_spacing.y = 0.0;
                                            if render_preset_dropdown_row(
                                                ui,
                                                palette,
                                                "dark_theme_preset_popup",
                                                &mut state.dark_preset,
                                                &DarkThemePreset::ALL,
                                                DarkThemePreset::label,
                                            ) {
                                                action = Some(SettingsAction::PreviewTheme(
                                                    state.appearance(),
                                                ));
                                            }
                                            let dark_palette =
                                                Palette::dark_for_preset(state.dark_preset);
                                            hairline_divider(ui, palette.border_subtle);
                                            render_color_token_row(
                                                ui,
                                                palette,
                                                "Background",
                                                dark_palette.bg_surface,
                                            );
                                            hairline_divider(ui, palette.border_subtle);
                                            render_color_token_row(
                                                ui,
                                                palette,
                                                "Foreground",
                                                dark_palette.text_primary,
                                            );
                                            hairline_divider(ui, palette.border_subtle);
                                            render_color_token_row(
                                                ui,
                                                palette,
                                                "Accent",
                                                dark_palette.accent,
                                            );
                                        });

                                    ui.add_space(ThemeTokens::SPACE_LG);
                                    hairline_divider(ui, palette.border_subtle);
                                    ui.add_space(ThemeTokens::SPACE_LG);

                                    // 3. Desktop Integration Grouped Card with Toggle Switches
                                    ui.label(semibold_text(
                                        "Integrasi Desktop",
                                        13.5,
                                        palette.text_primary,
                                    ));
                                    ui.add_space(ThemeTokens::SPACE_SM);
                                    egui::Frame::none()
                                        .fill(palette.bg_surface)
                                        .stroke(Stroke::new(1.0_f32, palette.border_subtle))
                                        .rounding(Rounding::same(ThemeTokens::RADIUS_CARD))
                                        .inner_margin(egui::Margin::ZERO)
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.spacing_mut().item_spacing.y = 0.0;
                                            switch_row(
                                                ui,
                                                palette,
                                                &mut state.minimize_to_tray,
                                                "Tetap berjalan di system tray saat jendela ditutup",
                                            );
                                            hairline_divider(ui, palette.border_subtle);
                                            switch_row(
                                                ui,
                                                palette,
                                                &mut state.notifications_enabled,
                                                "Tampilkan notifikasi sistem",
                                            );
                                            hairline_divider(ui, palette.border_subtle);
                                            switch_row(
                                                ui,
                                                palette,
                                                &mut state.autostart,
                                                "Jalankan otomatis saat login sistem operasi",
                                            );
                                            hairline_divider(ui, palette.border_subtle);
                                            switch_row(
                                                ui,
                                                palette,
                                                &mut state.auto_start_monitoring,
                                                "Mulai pemantauan otomatis saat aplikasi dibuka",
                                            );
                                        });

                                    ui.add_space(ThemeTokens::SPACE_LG);
                                    hairline_divider(ui, palette.border_subtle);
                                    ui.add_space(ThemeTokens::SPACE_LG);

                                    // 4. Advanced & Diagnostics (true 2-column aligned grid)
                                    ui.label(semibold_text(
                                        "Pengaturan Lanjutan & Diagnostik",
                                        13.5,
                                        palette.text_primary,
                                    ));
                                    ui.add_space(ThemeTokens::SPACE_SM);

                                    if body_w >= 380.0 {
                                        ui.columns(2, |cols| {
                                            let col_w0 = cols[0].available_width();
                                            labeled_text_input(
                                                &mut cols[0],
                                                palette,
                                                "Interval pemantauan (dtk, min 15)",
                                                &mut state.interval_text,
                                                "60",
                                                true,
                                                col_w0,
                                            );
                                            let col_w1 = cols[1].available_width();
                                            labeled_text_input(
                                                &mut cols[1],
                                                palette,
                                                "Maks percobaan ulang (1..10)",
                                                &mut state.max_retry_text,
                                                "5",
                                                true,
                                                col_w1,
                                            );
                                        });

                                        ui.add_space(ThemeTokens::SPACE_MD);

                                        ui.columns(2, |cols| {
                                            let col_w0 = cols[0].available_width();
                                            labeled_text_input(
                                                &mut cols[0],
                                                palette,
                                                "Jeda klik minimum (dtk)",
                                                &mut state.delay_min_text,
                                                "1.2",
                                                true,
                                                col_w0,
                                            );
                                            let col_w1 = cols[1].available_width();
                                            labeled_text_input(
                                                &mut cols[1],
                                                palette,
                                                "Jeda klik maksimum (dtk)",
                                                &mut state.delay_max_text,
                                                "2.6",
                                                true,
                                                col_w1,
                                            );
                                        });
                                    } else {
                                        let w = ui.available_width();
                                        labeled_text_input(
                                            ui,
                                            palette,
                                            "Interval pemantauan (dtk, min 15)",
                                            &mut state.interval_text,
                                            "60",
                                            true,
                                            w,
                                        );
                                        ui.add_space(ThemeTokens::SPACE_SM);
                                        labeled_text_input(
                                            ui,
                                            palette,
                                            "Maks percobaan ulang (1..10)",
                                            &mut state.max_retry_text,
                                            "5",
                                            true,
                                            w,
                                        );
                                        ui.add_space(ThemeTokens::SPACE_SM);
                                        labeled_text_input(
                                            ui,
                                            palette,
                                            "Jeda klik minimum (dtk)",
                                            &mut state.delay_min_text,
                                            "1.2",
                                            true,
                                            w,
                                        );
                                        ui.add_space(ThemeTokens::SPACE_SM);
                                        labeled_text_input(
                                            ui,
                                            palette,
                                            "Jeda klik maksimum (dtk)",
                                            &mut state.delay_max_text,
                                            "2.6",
                                            true,
                                            w,
                                        );
                                    }

                                    ui.add_space(ThemeTokens::SPACE_MD);
                                    labeled_text_input(
                                        ui,
                                        palette,
                                        "URL Portal Akademik",
                                        &mut state.base_url,
                                        "https://akademik.polban.ac.id",
                                        true,
                                        ui.available_width(),
                                    );

                                    ui.add_space(ThemeTokens::SPACE_MD);
                                    if action_button(
                                        ui,
                                        palette,
                                        ButtonTone::Secondary,
                                        "Simpan HTML Halaman Absen (Diagnostik)",
                                        Some(icons::draw_download),
                                        true,
                                        0.0,
                                    )
                                    .clicked()
                                    {
                                        action = Some(SettingsAction::ExportDiagnosticHtml);
                                    }

                                    if state.status_message.is_some() {
                                        ui.add_space(ThemeTokens::SPACE_MD);
                                        render_status_banner(ui, state, palette);
                                    }
                                });
                        });

                    hairline_divider(ui, palette.border_subtle);

                    // Sticky Footer
                    egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(20.0, 14.0))
                        .show(ui, |ui| {
                            ui.set_width(modal_w - 40.0);
                            ui.horizontal(|ui| {
                                if state.status_is_error {
                                    if let Some(msg) = &state.status_message {
                                        ui.allocate_ui(
                                            Vec2::new((modal_w - 290.0).max(80.0), 0.0),
                                            |ui| {
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(msg)
                                                            .size(11.5)
                                                            .color(palette.red_text),
                                                    )
                                                    .truncate(),
                                                );
                                            },
                                        );
                                    }
                                }
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.spacing_mut().item_spacing.x = ThemeTokens::SPACE_SM;
                                    if action_button(
                                        ui,
                                        palette,
                                        ButtonTone::Primary,
                                        "Simpan Pengaturan",
                                        Some(icons::draw_check),
                                        true,
                                        0.0,
                                    )
                                    .clicked()
                                    {
                                        action = Some(SettingsAction::Save);
                                    }
                                    if action_button(
                                        ui,
                                        palette,
                                        ButtonTone::Secondary,
                                        "Batal",
                                        None,
                                        true,
                                        0.0,
                                    )
                                    .clicked()
                                    {
                                        action = Some(SettingsAction::Close);
                                    }
                                });
                            });
                        });
                });
        });

    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_validation_enforces_safe_bounds() {
        let mut st = SettingsState::from_prefs(&AppPrefs::default(), "secret".to_string());
        assert!(st.validate_and_sync().is_ok());

        st.interval_text = "10".to_string();
        assert!(st.validate_and_sync().is_err());
        st.interval_text = "15".to_string();
        assert!(st.validate_and_sync().is_ok());

        st.max_retry_text = "0".to_string();
        assert!(st.validate_and_sync().is_err());
        st.max_retry_text = "11".to_string();
        assert!(st.validate_and_sync().is_err());
        st.max_retry_text = "5".to_string();
        assert!(st.validate_and_sync().is_ok());

        st.delay_min_text = "3.0".to_string();
        st.delay_max_text = "2.0".to_string();
        assert!(st.validate_and_sync().is_err());
        st.delay_max_text = "3.5".to_string();
        assert!(st.validate_and_sync().is_ok());

        st.base_url = "ftp://example.com".to_string();
        assert!(st.validate_and_sync().is_err());
    }
}

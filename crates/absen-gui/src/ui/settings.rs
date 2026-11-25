//! Onboarding card and Settings modal for credentials, theme, desktop behavior, and diagnostics.

use std::fmt;

use egui::{
    Align, Color32, CursorIcon, FontFamily, Id, Layout, Pos2, Rect, RichText, Rounding, Sense,
    Stroke, Vec2,
};

use crate::credentials::CredentialBackend;
use crate::prefs::{
    clamp_notification_volume, clamp_ui_zoom_pct, AppPrefs, DarkThemePreset, LightThemePreset,
    ThemePreference, DEFAULT_NOTIFICATION_VOLUME, DEFAULT_UI_ZOOM_PCT, MAX_NOTIFICATION_VOLUME,
    MAX_UI_ZOOM_PCT, MIN_NOTIFICATION_VOLUME, MIN_UI_ZOOM_PCT,
};
use crate::theme::{AppearanceConfig, Palette, ThemeTokens};
use crate::ui::{
    action_button, hairline_divider, icons, labeled_text_input, mono_text, paint_focus_ring,
    segmented_control, semibold_text, status_badge, switch_row, ButtonTone, SemanticTone,
};
use crate::updater::{UpdateStatus, CURRENT_VERSION};

/// Editable settings and onboarding form state; `Debug` redacts the password buffer.
#[derive(Clone)]
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
    pub ui_zoom_pct: u16,
    pub minimize_to_tray: bool,
    pub autostart: bool,
    pub notifications_enabled: bool,
    pub notification_sound: bool,
    pub notification_volume: u8,
    pub auto_start_monitoring: bool,
    pub status_message: Option<String>,
    pub status_is_error: bool,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self::from_prefs(&AppPrefs::default(), String::new())
    }
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
            .field("ui_zoom_pct", &self.ui_zoom_pct)
            .field("notification_sound", &self.notification_sound)
            .field("notification_volume", &self.notification_volume)
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
            ui_zoom_pct: clamp_ui_zoom_pct(prefs.ui_zoom_pct),
            minimize_to_tray: prefs.minimize_to_tray,
            autostart: prefs.autostart,
            notifications_enabled: prefs.notifications_enabled,
            notification_sound: prefs.notification_sound,
            notification_volume: clamp_notification_volume(prefs.notification_volume),
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
        self.ui_zoom_pct = clamp_ui_zoom_pct(self.ui_zoom_pct);
        self.notification_volume = clamp_notification_volume(self.notification_volume);
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
        prefs.ui_zoom_pct = clamp_ui_zoom_pct(self.ui_zoom_pct);
        prefs.minimize_to_tray = self.minimize_to_tray;
        prefs.autostart = self.autostart;
        prefs.notifications_enabled = self.notifications_enabled;
        prefs.notification_sound = self.notification_sound;
        prefs.notification_volume = clamp_notification_volume(self.notification_volume);
        prefs.auto_start_monitoring = self.auto_start_monitoring;
    }
}

/// Host capability flags passed to [`show`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsHost {
    pub backend: CredentialBackend,
    pub hide_to_tray_supported: bool,
}

/// User action returned by the settings modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAction {
    Close,
    Save,
    ImportDotenv,
    ExportDiagnosticHtml,
    PreviewTheme(AppearanceConfig),
    PreviewZoom(u16),
    TestNotification,
    CheckForUpdates,
    StartUpdate,
    RestartToUpdate,
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

fn render_color_token_row(ui: &mut egui::Ui, palette: &Palette, label: &str, token_color: Color32) {
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
            (palette.bg_subtle, Stroke::new(1.2_f32, palette.accent_text))
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
        let menu_x = (pill_rect.right() - menu_w).clamp(
            screen.left() + 8.0,
            (screen.right() - menu_w - 8.0).max(8.0),
        );
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
                            let (item_rect, item_resp) =
                                ui.allocate_exact_size(Vec2::new(inner_w, item_h), Sense::click());
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
                                        FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()),
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

struct SliderSpec {
    id_salt: &'static str,
    min: u16,
    max: u16,
    step: u16,
    keyboard_step: u16,
    default_tick: Option<u16>,
    enabled: bool,
    min_label: &'static str,
    max_label: &'static str,
}

struct SliderOutcome {
    commit_preview: bool,
}

fn render_mono_pill(ui: &mut egui::Ui, palette: &Palette, text: &str, active: bool) {
    let fg = if active {
        palette.text_primary
    } else {
        palette.text_secondary
    };
    egui::Frame::none()
        .fill(palette.bg_subtle)
        .stroke(Stroke::new(1.0_f32, palette.border_subtle))
        .rounding(Rounding::same(ThemeTokens::RADIUS_TAG))
        .inner_margin(egui::Margin::symmetric(8.0, 3.0))
        .show(ui, |ui| {
            ui.label(mono_text(text, 12.0, fg));
        });
}

fn compact_reset_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    enabled: bool,
) -> egui::Response {
    let inner = ui.add_enabled_ui(enabled, |ui| {
        let galley = ui.painter().layout_no_wrap(
            label.to_string(),
            egui::FontId::new(
                12.0,
                FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()),
            ),
            palette.text_primary,
        );
        let desired_size = Vec2::new((galley.size().x + 20.0).max(52.0), 28.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());
        let response = if enabled {
            response.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            if ui.rect_contains_pointer(rect) {
                ui.ctx().set_cursor_icon(CursorIcon::NotAllowed);
            }
            response
        };

        if ui.is_rect_visible(rect) {
            let hovered = enabled && response.hovered();
            let pressed = enabled && response.is_pointer_button_down_on();
            let (fill, border, fg) = if !enabled {
                (
                    palette.bg_subtle,
                    Stroke::new(1.0_f32, palette.border_subtle),
                    palette.text_secondary,
                )
            } else {
                let bg = if pressed || hovered {
                    palette.bg_subtle
                } else {
                    palette.bg_surface
                };
                let stroke_color = if hovered {
                    palette.accent_text
                } else {
                    palette.border_interactive
                };
                (bg, Stroke::new(1.1_f32, stroke_color), palette.text_primary)
            };

            ui.painter()
                .rect(rect, Rounding::same(ThemeTokens::RADIUS_TAG), fill, border);
            let text_galley = ui.painter().layout_no_wrap(
                label.to_string(),
                egui::FontId::new(
                    12.0,
                    FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()),
                ),
                fg,
            );
            let text_pos = Pos2::new(
                rect.center().x - text_galley.size().x * 0.5,
                rect.center().y - text_galley.size().y * 0.5,
            );
            ui.painter().galley(text_pos, text_galley, fg);
            paint_focus_ring(ui, &response, palette, ThemeTokens::RADIUS_TAG);
        }

        response
    });
    inner.inner
}

fn render_themed_slider(
    ui: &mut egui::Ui,
    palette: &Palette,
    value: &mut u16,
    spec: SliderSpec,
) -> SliderOutcome {
    let mut outcome = SliderOutcome {
        commit_preview: false,
    };

    let avail_w = ui.available_width().max(120.0);
    let label_font = egui::FontId::new(11.5, FontFamily::Monospace);
    let min_galley = ui.painter().layout_no_wrap(
        spec.min_label.to_string(),
        label_font.clone(),
        palette.text_secondary,
    );
    let max_galley = ui.painter().layout_no_wrap(
        spec.max_label.to_string(),
        label_font,
        palette.text_secondary,
    );
    let gap = ThemeTokens::SPACE_SM;
    let left_w = min_galley.size().x.ceil();
    let right_w = max_galley.size().x.ceil();
    let slider_w = (avail_w - left_w - right_w - gap * 2.0).max(72.0);
    let row_h = 28.0;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;

        let (left_rect, _) = ui.allocate_exact_size(Vec2::new(left_w, row_h), Sense::hover());
        if ui.is_rect_visible(left_rect) {
            ui.painter().galley(
                Pos2::new(
                    left_rect.left(),
                    left_rect.center().y - min_galley.size().y * 0.5,
                ),
                min_galley,
                palette.text_secondary,
            );
        }

        let (_, rect) = ui.allocate_space(Vec2::new(slider_w, row_h));
        let sense = if spec.enabled {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        };
        let mut response = ui.interact(rect, ui.id().with(spec.id_salt), sense);
        if spec.enabled {
            response = response.on_hover_cursor(CursorIcon::PointingHand);
        } else if ui.rect_contains_pointer(rect) {
            ui.ctx().set_cursor_icon(CursorIcon::NotAllowed);
        }

        let thumb_pad = 9.0_f32;
        let usable_left = rect.left() + thumb_pad;
        let usable_right = (rect.right() - thumb_pad).max(usable_left + 1.0);
        let usable_w = (usable_right - usable_left).max(1.0);
        let span = f32::from(spec.max.saturating_sub(spec.min).max(1));

        if spec.enabled {
            if response.clicked() || response.drag_started() {
                response.request_focus();
            }
            if let Some(pos) = response.interact_pointer_pos() {
                let raw_t = ((pos.x - usable_left) / usable_w).clamp(0.0, 1.0);
                let step = f32::from(spec.step.max(1));
                let raw = f32::from(spec.min) + raw_t * span;
                let snapped =
                    ((raw - f32::from(spec.min)) / step).round() * step + f32::from(spec.min);
                let next = (snapped.round() as u16).clamp(spec.min, spec.max);
                *value = next;
            }
            if response.clicked() || response.drag_stopped() {
                outcome.commit_preview = true;
            }
            if response.has_focus() {
                let mut next = *value;
                if ui.input(|i| {
                    i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::ArrowUp)
                }) {
                    next = next.saturating_add(spec.keyboard_step).min(spec.max);
                }
                if ui.input(|i| {
                    i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowDown)
                }) {
                    next = next.saturating_sub(spec.keyboard_step).max(spec.min);
                }
                if ui.input(|i| i.key_pressed(egui::Key::Home)) {
                    next = spec.min;
                }
                if ui.input(|i| i.key_pressed(egui::Key::End)) {
                    next = spec.max;
                }
                if next != *value {
                    *value = next;
                    outcome.commit_preview = true;
                }
            }
        }

        if ui.is_rect_visible(rect) {
            let clamped = (*value).clamp(spec.min, spec.max);
            let t = f32::from(clamped - spec.min) / span;
            let thumb_x = usable_left + t * usable_w;

            let track_h = 6.0_f32;
            let track_rect = Rect::from_min_max(
                Pos2::new(usable_left, rect.center().y - track_h * 0.5),
                Pos2::new(usable_right, rect.center().y + track_h * 0.5),
            );
            let track_border = if spec.enabled {
                palette.border_interactive
            } else {
                palette.border_subtle
            };
            ui.painter().rect(
                track_rect,
                Rounding::same(3.0),
                palette.bg_subtle,
                Stroke::new(1.0_f32, track_border),
            );

            if let Some(tick) = spec.default_tick {
                if tick > spec.min && tick < spec.max {
                    let tick_t = f32::from(tick - spec.min) / span;
                    let tick_x = usable_left + tick_t * usable_w;
                    ui.painter().line_segment(
                        [
                            Pos2::new(tick_x, track_rect.top() - 3.0),
                            Pos2::new(tick_x, track_rect.bottom() + 3.0),
                        ],
                        Stroke::new(1.5_f32, track_border),
                    );
                }
            }

            if clamped > spec.min {
                let active_rect = Rect::from_min_max(
                    track_rect.min,
                    Pos2::new(
                        thumb_x.max(track_rect.left() + 3.0).min(track_rect.right()),
                        track_rect.max.y,
                    ),
                );
                let active_fill = if spec.enabled {
                    palette.accent_text
                } else {
                    palette.border_subtle
                };
                ui.painter()
                    .rect_filled(active_rect, Rounding::same(3.0), active_fill);
            }

            let thumb_r = if spec.enabled && (response.hovered() || response.dragged()) {
                8.5
            } else {
                7.5
            };
            let (thumb_fill, thumb_stroke) = if spec.enabled {
                (
                    palette.accent_text,
                    Stroke::new(2.0_f32, palette.bg_surface),
                )
            } else {
                (
                    palette.bg_subtle,
                    Stroke::new(1.5_f32, palette.border_interactive),
                )
            };
            ui.painter().circle(
                Pos2::new(thumb_x, rect.center().y),
                thumb_r,
                thumb_fill,
                thumb_stroke,
            );
            paint_focus_ring(ui, &response, palette, ThemeTokens::RADIUS_CONTROL);
        }

        let (right_rect, _) = ui.allocate_exact_size(Vec2::new(right_w, row_h), Sense::hover());
        if ui.is_rect_visible(right_rect) {
            ui.painter().galley(
                Pos2::new(
                    right_rect.right() - max_galley.size().x,
                    right_rect.center().y - max_galley.size().y * 0.5,
                ),
                max_galley,
                palette.text_secondary,
            );
        }
    });

    outcome
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
                            "Simpan & Jalankan Presensi Otomatis",
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
                                "Simpan & Jalankan Presensi Otomatis",
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
    host: SettingsHost,
    palette: &Palette,
    update_status: &UpdateStatus,
    checking_update: bool,
    update_note: Option<&(String, bool)>,
) -> Option<SettingsAction> {
    let backend = host.backend;
    let hide_to_tray_supported = host.hide_to_tray_supported;
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

                                    ui.add_space(ThemeTokens::SPACE_MD);

                                    // UI Zoom In/Out Card
                                    egui::Frame::none()
                                        .fill(palette.bg_surface)
                                        .stroke(Stroke::new(1.0_f32, palette.border_subtle))
                                        .rounding(Rounding::same(ThemeTokens::RADIUS_CARD))
                                        .inner_margin(egui::Margin::same(ThemeTokens::SPACE_MD))
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.spacing_mut().item_spacing.y = 0.0;

                                            let row_w = ui.available_width();
                                            ui.horizontal(|ui| {
                                                let right_reserve = 122.0;
                                                let left_w = (row_w - right_reserve).max(96.0);
                                                ui.allocate_ui_with_layout(
                                                    Vec2::new(left_w, 28.0),
                                                    Layout::left_to_right(Align::Center),
                                                    |ui| {
                                                        ui.add(
                                                            egui::Label::new(semibold_text(
                                                                "Skala Tampilan (Zoom)",
                                                                13.0,
                                                                palette.text_primary,
                                                            ))
                                                            .wrap(),
                                                        );
                                                    },
                                                );
                                                ui.with_layout(
                                                    Layout::right_to_left(Align::Center),
                                                    |ui| {
                                                        ui.spacing_mut().item_spacing.x =
                                                            ThemeTokens::SPACE_SM;
                                                        let can_reset = state.ui_zoom_pct
                                                            != DEFAULT_UI_ZOOM_PCT;
                                                        let reset_resp = compact_reset_button(
                                                            ui, palette, "Reset", can_reset,
                                                        );
                                                        if can_reset && reset_resp.clicked() {
                                                            state.ui_zoom_pct = DEFAULT_UI_ZOOM_PCT;
                                                            action =
                                                                Some(SettingsAction::PreviewZoom(
                                                                    DEFAULT_UI_ZOOM_PCT,
                                                                ));
                                                        }
                                                        let zoom_badge =
                                                            format!("{}%", state.ui_zoom_pct);
                                                        render_mono_pill(
                                                            ui,
                                                            palette,
                                                            &zoom_badge,
                                                            true,
                                                        );
                                                    },
                                                );
                                            });

                                            ui.add_space(ThemeTokens::SPACE_SM);
                                            let zoom_slider = render_themed_slider(
                                                ui,
                                                palette,
                                                &mut state.ui_zoom_pct,
                                                SliderSpec {
                                                    id_salt: "ui_zoom_pct_slider",
                                                    min: MIN_UI_ZOOM_PCT,
                                                    max: MAX_UI_ZOOM_PCT,
                                                    step: 5,
                                                    keyboard_step: 5,
                                                    default_tick: Some(DEFAULT_UI_ZOOM_PCT),
                                                    enabled: true,
                                                    min_label: "70%",
                                                    max_label: "150%",
                                                },
                                            );
                                            if zoom_slider.commit_preview {
                                                action = Some(SettingsAction::PreviewZoom(
                                                    state.ui_zoom_pct,
                                                ));
                                            }

                                            ui.add_space(6.0);
                                            let zoom_hint = if cfg!(target_os = "macos") {
                                                "Pintasan: gunakan Cmd + / Cmd - untuk zoom in/out, atau Cmd + 0 untuk reset ke 100%."
                                            } else {
                                                "Pintasan: gunakan Ctrl + / Ctrl - untuk zoom in/out, atau Ctrl + 0 untuk reset ke 100%."
                                            };
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(zoom_hint)
                                                        .size(11.5)
                                                        .color(palette.text_secondary),
                                                )
                                                .wrap(),
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
                                            if !hide_to_tray_supported {
                                                egui::Frame::none()
                                                    .inner_margin(egui::Margin {
                                                        left: ThemeTokens::SPACE_MD,
                                                        right: ThemeTokens::SPACE_MD,
                                                        top: 0.0,
                                                        bottom: ThemeTokens::SPACE_SM,
                                                    })
                                                    .show(ui, |ui| {
                                                        ui.add(
                                                            egui::Label::new(
                                                                RichText::new(
                                                                    "Menutup jendela tetap menghentikan aplikasi saat berjalan di sesi Wayland native atau ketika layanan system tray (mis. AppIndicator/StatusNotifier) tidak tersedia.",
                                                                )
                                                                .size(11.5)
                                                                .color(palette.text_secondary),
                                                            )
                                                            .wrap(),
                                                        );
                                                    });
                                            }
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
                                                &mut state.notification_sound,
                                                "Putar suara notifikasi",
                                            );
                                            hairline_divider(ui, palette.border_subtle);
                                            egui::Frame::none()
                                                .inner_margin(egui::Margin::symmetric(
                                                    ThemeTokens::SPACE_MD,
                                                    ThemeTokens::SPACE_SM + 2.0,
                                                ))
                                                .show(ui, |ui| {
                                                    ui.set_width(ui.available_width());
                                                    ui.spacing_mut().item_spacing.y = 0.0;
                                                    let sound_on = state.notification_sound;
                                                    ui.add_enabled_ui(sound_on, |ui| {
                                                        let row_w = ui.available_width();
                                                        let vol_label_color = if sound_on {
                                                            palette.text_primary
                                                        } else {
                                                            palette.text_secondary
                                                        };
                                                        let vol_readout =
                                                            if state.notification_volume == 0 {
                                                                "Bisu (0%)".to_string()
                                                            } else {
                                                                format!(
                                                                    "{}%",
                                                                    state.notification_volume
                                                                )
                                                            };
                                                        ui.horizontal(|ui| {
                                                            let right_reserve = 92.0;
                                                            let left_w =
                                                                (row_w - right_reserve).max(96.0);
                                                            ui.allocate_ui_with_layout(
                                                                Vec2::new(left_w, 24.0),
                                                                Layout::left_to_right(
                                                                    Align::Center,
                                                                ),
                                                                |ui| {
                                                                    ui.add(
                                                                        egui::Label::new(
                                                                            RichText::new(
                                                                                "Volume suara notifikasi",
                                                                            )
                                                                            .size(13.0)
                                                                            .color(vol_label_color),
                                                                        )
                                                                        .wrap(),
                                                                    );
                                                                },
                                                            );
                                                            ui.with_layout(
                                                                Layout::right_to_left(
                                                                    Align::Center,
                                                                ),
                                                                |ui| {
                                                                    render_mono_pill(
                                                                        ui,
                                                                        palette,
                                                                        &vol_readout,
                                                                        sound_on
                                                                            && state
                                                                                .notification_volume
                                                                                > 0,
                                                                    );
                                                                },
                                                            );
                                                        });

                                                        ui.add_space(6.0);
                                                        let mut vol_u16 =
                                                            u16::from(state.notification_volume);
                                                        render_themed_slider(
                                                            ui,
                                                            palette,
                                                            &mut vol_u16,
                                                            SliderSpec {
                                                                id_salt:
                                                                    "notification_volume_slider",
                                                                min: u16::from(
                                                                    MIN_NOTIFICATION_VOLUME,
                                                                ),
                                                                max: u16::from(
                                                                    MAX_NOTIFICATION_VOLUME,
                                                                ),
                                                                step: 1,
                                                                keyboard_step: 5,
                                                                default_tick: Some(u16::from(
                                                                    DEFAULT_NOTIFICATION_VOLUME,
                                                                )),
                                                                enabled: sound_on,
                                                                min_label: "0%",
                                                                max_label: "100%",
                                                            },
                                                        );
                                                        state.notification_volume =
                                                            clamp_notification_volume(
                                                                vol_u16 as u8,
                                                            );
                                                    });
                                                });
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

                                    ui.add_space(ThemeTokens::SPACE_SM);
                                    if action_button(
                                        ui,
                                        palette,
                                        ButtonTone::Secondary,
                                        "Uji Notifikasi",
                                        Some(icons::draw_bell),
                                        true,
                                        0.0,
                                    )
                                    .clicked()
                                    {
                                        action = Some(SettingsAction::TestNotification);
                                    }

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

                                    ui.add_space(ThemeTokens::SPACE_LG);
                                    hairline_divider(ui, palette.border_subtle);
                                    ui.add_space(ThemeTokens::SPACE_MD);

                                    ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().item_spacing = Vec2::new(
                                            ThemeTokens::SPACE_SM,
                                            ThemeTokens::SPACE_XS,
                                        );
                                        ui.label(semibold_text(
                                            "Versi Aplikasi",
                                            13.0,
                                            palette.text_primary,
                                        ));
                                        ui.label(mono_text(
                                            format!("v{CURRENT_VERSION}"),
                                            12.5,
                                            palette.accent_text,
                                        ));
                                    });
                                    ui.add_space(ThemeTokens::SPACE_SM);
                                    ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().item_spacing = Vec2::splat(ThemeTokens::SPACE_SM);
                                        let is_downloading = matches!(
                                            update_status,
                                            UpdateStatus::Downloading { .. }
                                        );
                                        let check_label = if checking_update {
                                            "Memeriksa..."
                                        } else {
                                            "Cek Pembaruan"
                                        };
                                        if action_button(
                                            ui,
                                            palette,
                                            ButtonTone::Secondary,
                                            check_label,
                                            Some(icons::draw_refresh),
                                            !checking_update && !is_downloading,
                                            0.0,
                                        )
                                        .clicked()
                                        {
                                            action = Some(SettingsAction::CheckForUpdates);
                                        }

                                        match update_status {
                                            UpdateStatus::Available(candidate) => {
                                                let lbl = format!("Update v{}", candidate.version);
                                                if action_button(
                                                    ui,
                                                    palette,
                                                    ButtonTone::Primary,
                                                    &lbl,
                                                    Some(icons::draw_download),
                                                    true,
                                                    0.0,
                                                )
                                                .clicked()
                                                {
                                                    action = Some(SettingsAction::StartUpdate);
                                                }
                                            }
                                            UpdateStatus::Downloading {
                                                progress_pct,
                                                verifying,
                                                ..
                                            } => {
                                                let lbl = if *verifying {
                                                    "Memverifikasi...".to_string()
                                                } else if let Some(pct) = progress_pct {
                                                    format!("Mengunduh {pct}%")
                                                } else {
                                                    "Mengunduh...".to_string()
                                                };
                                                action_button(
                                                    ui,
                                                    palette,
                                                    ButtonTone::Secondary,
                                                    &lbl,
                                                    Some(icons::draw_download),
                                                    false,
                                                    0.0,
                                                );
                                            }
                                            UpdateStatus::ReadyToRestart { .. } => {
                                                if action_button(
                                                    ui,
                                                    palette,
                                                    ButtonTone::Primary,
                                                    "Restart untuk Update",
                                                    Some(icons::draw_refresh),
                                                    true,
                                                    0.0,
                                                )
                                                .clicked()
                                                {
                                                    action = Some(SettingsAction::RestartToUpdate);
                                                }
                                            }
                                            UpdateStatus::Failed {
                                                retry_candidate: Some(_),
                                                ..
                                            } if action_button(
                                                ui,
                                                palette,
                                                ButtonTone::Danger,
                                                "Coba Lagi Update",
                                                Some(icons::draw_alert),
                                                true,
                                                0.0,
                                            )
                                            .clicked() =>
                                            {
                                                action = Some(SettingsAction::StartUpdate);
                                            }
                                            _ => {}
                                        }
                                    });

                                    if let Some((note_text, note_err)) = update_note {
                                        ui.add_space(ThemeTokens::SPACE_XS);
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(note_text)
                                                    .size(12.0)
                                                    .color(if *note_err {
                                                        palette.red_text
                                                    } else {
                                                        palette.text_secondary
                                                    }),
                                            )
                                            .wrap(),
                                        );
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

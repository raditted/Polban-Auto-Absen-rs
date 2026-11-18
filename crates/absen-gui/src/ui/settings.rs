//! Onboarding card and Settings modal for credentials, theme, desktop behavior, and diagnostics.

use std::fmt;

use egui::{RichText, Rounding, Stroke, Vec2};

use crate::credentials::CredentialBackend;
use crate::prefs::{AppPrefs, ThemePreference};
use crate::theme::{Palette, ThemeTokens};
use crate::ui::{
    action_button, icons, labeled_text_input, semibold_text, status_badge, toggle_row, ButtonTone,
    SemanticTone,
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
            minimize_to_tray: prefs.minimize_to_tray,
            autostart: prefs.autostart,
            notifications_enabled: prefs.notifications_enabled,
            auto_start_monitoring: prefs.auto_start_monitoring,
            status_message: None,
            status_is_error: false,
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
    ui.add_space(ThemeTokens::SPACE_XS);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = ThemeTokens::SPACE_SM;
        let toggle_w = 128.0_f32.min((full_w * 0.36).max(96.0));
        let input_w = (full_w - toggle_w - ThemeTokens::SPACE_SM).max(120.0);
        let edit = egui::TextEdit::singleline(&mut state.password)
            .hint_text(RichText::new("Masukkan kata sandi portal").color(palette.text_secondary))
            .password(!state.show_password)
            .desired_width(input_w)
            .margin(egui::Margin::symmetric(ThemeTokens::SPACE_MD, 12.0))
            .font(egui::FontId::new(13.0, egui::FontFamily::Proportional));
        let resp = ui
            .add_sized(Vec2::new(input_w, ThemeTokens::MIN_TARGET_HEIGHT), edit)
            .on_hover_cursor(egui::CursorIcon::Text);
        crate::ui::paint_focus_ring(ui, &resp, palette, ThemeTokens::RADIUS_CONTROL);

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
                    render_credential_fields(ui, state, backend, palette);

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
    let max_w = (screen.width() - 32.0).clamp(300.0, 620.0);
    let max_h = (screen.height() - 32.0).max(280.0);

    egui::Window::new("Pengaturan")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .fixed_size(Vec2::new(max_w, max_h))
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(palette.bg_surface)
                .stroke(Stroke::new(1.0_f32, palette.border_subtle))
                .rounding(Rounding::same(ThemeTokens::RADIUS_CARD))
                .inner_margin(egui::Margin::same(ThemeTokens::SPACE_LG)),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("settings_modal_scroll")
                .max_height((max_h - 92.0).max(160.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    ui.label(semibold_text(
                        "Akun Portal Akademik",
                        14.0,
                        palette.text_primary,
                    ));
                    ui.add_space(ThemeTokens::SPACE_SM);
                    render_credential_fields(ui, state, backend, palette);

                    ui.add_space(ThemeTokens::SPACE_SM);
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
                        action = Some(SettingsAction::ImportDotenv);
                    }

                    ui.add_space(ThemeTokens::SPACE_MD);
                    ui.label(semibold_text("Tema Tampilan", 14.0, palette.text_primary));
                    ui.add_space(ThemeTokens::SPACE_SM);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::splat(ThemeTokens::SPACE_SM);
                        for (pref, label, icon_fn) in [
                            (
                                ThemePreference::System,
                                "Mengikuti Sistem",
                                icons::draw_monitor
                                    as fn(&egui::Painter, egui::Rect, egui::Color32),
                            ),
                            (ThemePreference::Light, "Terang", icons::draw_sun),
                            (ThemePreference::Dark, "Gelap", icons::draw_moon),
                        ] {
                            let tone = if state.theme == pref {
                                ButtonTone::Primary
                            } else {
                                ButtonTone::Secondary
                            };
                            if action_button(ui, palette, tone, label, Some(icon_fn), true, 0.0)
                                .clicked()
                            {
                                state.theme = pref;
                            }
                        }
                    });

                    ui.add_space(ThemeTokens::SPACE_MD);
                    ui.label(semibold_text(
                        "Integrasi Desktop",
                        14.0,
                        palette.text_primary,
                    ));
                    ui.add_space(ThemeTokens::SPACE_SM);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = ThemeTokens::SPACE_SM;
                        toggle_row(
                            ui,
                            palette,
                            &mut state.minimize_to_tray,
                            "Tetap berjalan di system tray saat jendela ditutup",
                            None,
                        );
                        toggle_row(
                            ui,
                            palette,
                            &mut state.notifications_enabled,
                            "Tampilkan notifikasi sistem",
                            None,
                        );
                        toggle_row(
                            ui,
                            palette,
                            &mut state.autostart,
                            "Jalankan otomatis saat login sistem operasi",
                            None,
                        );
                        toggle_row(
                            ui,
                            palette,
                            &mut state.auto_start_monitoring,
                            "Mulai pemantauan otomatis saat aplikasi dibuka",
                            None,
                        );
                    });

                    ui.add_space(ThemeTokens::SPACE_MD);
                    ui.label(semibold_text(
                        "Pengaturan Lanjutan & Diagnostik",
                        14.0,
                        palette.text_primary,
                    ));
                    ui.add_space(ThemeTokens::SPACE_SM);

                    let half = ((ui.available_width() - ThemeTokens::SPACE_SM) * 0.5).max(120.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::splat(ThemeTokens::SPACE_SM);
                        ui.allocate_ui(Vec2::new(half, 0.0), |ui| {
                            ui.vertical(|ui| {
                                labeled_text_input(
                                    ui,
                                    palette,
                                    "Interval pemantauan (detik, min 15)",
                                    &mut state.interval_text,
                                    "60",
                                    true,
                                    half,
                                );
                            });
                        });
                        ui.allocate_ui(Vec2::new(half, 0.0), |ui| {
                            ui.vertical(|ui| {
                                labeled_text_input(
                                    ui,
                                    palette,
                                    "Maks percobaan ulang (1..10)",
                                    &mut state.max_retry_text,
                                    "5",
                                    true,
                                    half,
                                );
                            });
                        });
                    });

                    ui.add_space(ThemeTokens::SPACE_SM);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::splat(ThemeTokens::SPACE_SM);
                        ui.allocate_ui(Vec2::new(half, 0.0), |ui| {
                            ui.vertical(|ui| {
                                labeled_text_input(
                                    ui,
                                    palette,
                                    "Jeda klik minimum (detik)",
                                    &mut state.delay_min_text,
                                    "1.2",
                                    true,
                                    half,
                                );
                            });
                        });
                        ui.allocate_ui(Vec2::new(half, 0.0), |ui| {
                            ui.vertical(|ui| {
                                labeled_text_input(
                                    ui,
                                    palette,
                                    "Jeda klik maksimum (detik)",
                                    &mut state.delay_max_text,
                                    "2.6",
                                    true,
                                    half,
                                );
                            });
                        });
                    });

                    ui.add_space(ThemeTokens::SPACE_SM);
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

            ui.add_space(ThemeTokens::SPACE_MD);
            ui.horizontal(|ui| {
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
                if action_button(ui, palette, ButtonTone::Secondary, "Tutup", None, true, 0.0)
                    .clicked()
                {
                    action = Some(SettingsAction::Close);
                }
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

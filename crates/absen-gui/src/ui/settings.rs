//! Settings modal / form state for NIM, password, polling interval, and OS options.

use std::fmt;

use egui::{FontFamily, RichText};

use crate::prefs::{AppPrefs, ThemePreference};
use crate::theme::{Palette, ThemeTokens};

/// Editable settings form state; `Debug` redacts the password buffer.
#[derive(Clone, Default)]
pub struct SettingsState {
    pub open: bool,
    pub nim: String,
    pub password: String,
    pub show_password: bool,
    pub base_url: String,
    pub interval: u64,
    pub delay_min: f64,
    pub delay_max: f64,
    pub max_retry: u32,
    pub retry_wait: f64,
    pub theme: ThemePreference,
    pub minimize_to_tray: bool,
    pub autostart: bool,
    pub notifications_enabled: bool,
    pub auto_start_monitoring: bool,
    pub status_message: Option<String>,
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
            delay_min: prefs.delay_min,
            delay_max: prefs.delay_max,
            max_retry: prefs.max_retry,
            retry_wait: prefs.retry_wait,
            theme: prefs.theme,
            minimize_to_tray: prefs.minimize_to_tray,
            autostart: prefs.autostart,
            notifications_enabled: prefs.notifications_enabled,
            auto_start_monitoring: prefs.auto_start_monitoring,
            status_message: None,
        }
    }

    pub fn apply_to_prefs(&self, prefs: &mut AppPrefs) {
        prefs.nim = self.nim.trim().to_string();
        prefs.base_url = self.base_url.trim().trim_end_matches('/').to_string();
        prefs.target_url = format!("{}/ajar/absen", prefs.base_url);
        prefs.interval = self.interval.max(5);
        prefs.delay_min = self.delay_min.max(0.0);
        prefs.delay_max = self.delay_max.max(prefs.delay_min);
        prefs.max_retry = self.max_retry.max(1);
        prefs.retry_wait = self.retry_wait.max(0.0);
        prefs.theme = self.theme;
        prefs.minimize_to_tray = self.minimize_to_tray;
        prefs.autostart = self.autostart;
        prefs.notifications_enabled = self.notifications_enabled;
        prefs.auto_start_monitoring = self.auto_start_monitoring;
    }
}

/// User action returned by the settings dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAction {
    Close,
    Save,
    ImportDotenv,
}

pub fn show(
    ctx: &egui::Context,
    state: &mut SettingsState,
    palette: &Palette,
) -> Option<SettingsAction> {
    if !state.open {
        return None;
    }

    let mut action = None;
    egui::Window::new("Pengaturan")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_min_width(360.0);
            ui.label(
                RichText::new("Kredensial & Jadwal Pemantauan")
                    .family(FontFamily::Name(ThemeTokens::FONT_SEMIBOLD_FAMILY.into()))
                    .size(14.0)
                    .color(palette.text_primary),
            );
            ui.add_space(ThemeTokens::SPACE_SM);

            ui.horizontal(|ui| {
                ui.label("NIM:");
                ui.text_edit_singleline(&mut state.nim);
            });
            ui.horizontal(|ui| {
                ui.label("Password:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.password).password(!state.show_password),
                );
            });
            ui.checkbox(&mut state.show_password, "Tampilkan password");
            ui.checkbox(&mut state.minimize_to_tray, "Perkecil ke tray saat ditutup");
            ui.checkbox(&mut state.notifications_enabled, "Notifikasi desktop");
            ui.checkbox(&mut state.autostart, "Jalankan otomatis saat login OS");

            if let Some(msg) = &state.status_message {
                ui.add_space(ThemeTokens::SPACE_XS);
                ui.label(RichText::new(msg).size(12.0).color(palette.text_secondary));
            }

            ui.add_space(ThemeTokens::SPACE_MD);
            ui.horizontal(|ui| {
                if ui.button("Impor .env").clicked() {
                    action = Some(SettingsAction::ImportDotenv);
                }
                if ui.button("Simpan").clicked() {
                    action = Some(SettingsAction::Save);
                }
                if ui.button("Tutup").clicked() {
                    action = Some(SettingsAction::Close);
                }
            });
        });

    action
}

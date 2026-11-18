//! Top-level `eframe::App` state, shortcuts, and responsive 3-breakpoint orchestration.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use crate::bridge::{EngineController, GuiSnapshot};
use crate::credentials::{open_default_store, CredentialStore};
use crate::icon::IconStatus;
use crate::platform::{AutostartManager, Notifier, TrayCommand, TrayManager};
use crate::prefs::{self, AppPrefs, ThemePreference};
use crate::theme::{apply_theme, configure_fonts, ThemeTokens};
use crate::ui::header::HeaderAction;
use crate::ui::jadwal::JadwalAction;
use crate::ui::settings::{OnboardingAction, SettingsAction, SettingsState};
use crate::ui::{action_button, activity, header, jadwal, settings, ButtonTone, CompactTab};

/// Main desktop application state implementing `eframe::App`.
pub struct DesktopApp {
    prefs: AppPrefs,
    creds: Box<dyn CredentialStore>,
    controller: EngineController,
    tray: Option<TrayManager>,
    notifier: Notifier,
    autostart: AutostartManager,
    settings: SettingsState,
    compact_tab: CompactTab,
    start_minimized: bool,
    minimized_applied: bool,
    quit_requested: bool,
    headless: bool,
    test_snapshot: Option<GuiSnapshot>,
    diag_rx: Option<mpsc::Receiver<Result<PathBuf, String>>>,
    external_show_flag: Option<Arc<AtomicBool>>,
}

impl DesktopApp {
    /// Production desktop constructor loading OS preferences, keyring, and tray icon.
    pub fn new(cc: &eframe::CreationContext<'_>, start_minimized: bool) -> Self {
        configure_fonts(&cc.egui_ctx);

        let mut prefs = prefs::load().unwrap_or_default();
        apply_theme(&cc.egui_ctx, prefs.theme);

        let autostart = AutostartManager::new("Polban Presensi");
        if let Ok(os_autostart) = autostart.is_enabled() {
            prefs.autostart = os_autostart;
        }

        let creds = open_default_store();
        let initial_password = if prefs.nim.trim().is_empty() {
            String::new()
        } else {
            creds
                .get_password(&prefs.nim)
                .ok()
                .flatten()
                .unwrap_or_default()
        };

        let settings = SettingsState::from_prefs(&prefs, initial_password);
        let notifier = Notifier::new(prefs.notifications_enabled);
        let mut tray = TrayManager::new().ok();
        if let Some(t) = &mut tray {
            t.set_egui_waker(cc.egui_ctx.clone());
        }

        let mut controller = EngineController::new();
        controller.set_notifier(notifier.clone());
        if start_minimized && tray.is_some() {
            controller.set_window_visible(false);
        }

        let mut app = Self {
            prefs,
            creds,
            controller,
            tray,
            notifier,
            autostart,
            settings,
            compact_tab: CompactTab::Jadwal,
            start_minimized,
            minimized_applied: false,
            quit_requested: false,
            headless: false,
            test_snapshot: None,
            diag_rx: None,
            external_show_flag: None,
        };

        if !app.prefs.nim.trim().is_empty()
            && app.prefs.auto_start_monitoring
            && !app.settings.password.is_empty()
        {
            app.handle_start_monitoring(&cc.egui_ctx);
        }

        app
    }

    /// Headless constructor for deterministic UI tests without touching OS prefs or tray.
    pub fn new_headless(
        ctx: &egui::Context,
        prefs: AppPrefs,
        creds: Box<dyn CredentialStore>,
    ) -> Self {
        configure_fonts(ctx);
        apply_theme(ctx, prefs.theme);

        let initial_password = if prefs.nim.trim().is_empty() {
            String::new()
        } else {
            creds
                .get_password(&prefs.nim)
                .ok()
                .flatten()
                .unwrap_or_default()
        };

        let settings = SettingsState::from_prefs(&prefs, initial_password);
        let notifier = Notifier::new(prefs.notifications_enabled);
        let autostart = AutostartManager::new("Polban Presensi");
        let mut controller = EngineController::new();
        controller.set_notifier(notifier.clone());

        Self {
            prefs,
            creds,
            controller,
            tray: None,
            notifier,
            autostart,
            settings,
            compact_tab: CompactTab::Jadwal,
            start_minimized: false,
            minimized_applied: true,
            quit_requested: false,
            headless: true,
            test_snapshot: None,
            diag_rx: None,
            external_show_flag: None,
        }
    }

    /// Attach a cross-process wakeup flag used by the single-instance listener.
    pub fn set_external_show_flag(&mut self, flag: Arc<AtomicBool>) {
        self.external_show_flag = Some(flag);
    }

    fn reveal_window(&self, ctx: &egui::Context) {
        self.controller.set_window_visible(true);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    /// Inject or clear a deterministic `GuiSnapshot` for headless UI testing.
    pub fn set_test_snapshot(&mut self, snapshot: Option<GuiSnapshot>) {
        self.test_snapshot = snapshot;
    }

    pub fn is_settings_open(&self) -> bool {
        self.settings.open
    }

    pub fn prefs(&self) -> &AppPrefs {
        &self.prefs
    }

    pub fn compact_tab(&self) -> CompactTab {
        self.compact_tab
    }

    fn current_snapshot(&self) -> GuiSnapshot {
        self.test_snapshot
            .clone()
            .unwrap_or_else(|| self.controller.snapshot())
    }

    fn persist_prefs(&self) {
        if !self.headless {
            let _ = prefs::save(&self.prefs);
        }
    }

    fn cycle_theme(&mut self, ctx: &egui::Context) {
        self.prefs.theme = match self.prefs.theme {
            ThemePreference::System => ThemePreference::Light,
            ThemePreference::Light => ThemePreference::Dark,
            ThemePreference::Dark => ThemePreference::System,
        };
        self.settings.theme = self.prefs.theme;
        apply_theme(ctx, self.prefs.theme);
        self.persist_prefs();
    }

    fn resolve_password(&self) -> String {
        if !self.prefs.nim.trim().is_empty() {
            if let Ok(Some(pw)) = self.creds.get_password(&self.prefs.nim) {
                if !pw.is_empty() {
                    return pw;
                }
            }
        }
        self.settings.password.clone()
    }

    fn handle_start_monitoring(&mut self, ctx: &egui::Context) {
        if self.prefs.nim.trim().is_empty() {
            self.settings.status_message =
                Some("Isi NIM dan kata sandi portal terlebih dahulu.".to_string());
            self.settings.status_is_error = true;
            return;
        }
        let pw = self.resolve_password();
        if pw.is_empty() {
            self.settings.open = true;
            self.settings.status_message = Some("Kata sandi portal belum tersimpan.".to_string());
            self.settings.status_is_error = true;
            return;
        }
        let cfg = self.prefs.to_core_config(pw);
        let wake_ctx = ctx.clone();
        if let Err(err) = self.controller.start(
            cfg,
            true,
            Some(Box::new(move || wake_ctx.request_repaint())),
        ) {
            self.settings.status_message = Some(format!("Gagal memulai pemantauan: {err}"));
            self.settings.status_is_error = true;
        }
    }

    fn handle_tray_commands(&mut self, ctx: &egui::Context) {
        let commands = match &self.tray {
            Some(tray) => tray.poll_commands(),
            None => Vec::new(),
        };
        for cmd in commands {
            match cmd {
                TrayCommand::ShowWindow => {
                    self.reveal_window(ctx);
                }
                TrayCommand::ToggleMonitoring => {
                    if self.controller.is_running() {
                        self.controller.stop();
                    } else {
                        self.handle_start_monitoring(ctx);
                    }
                }
                TrayCommand::ScanNow => {
                    let _ = self.controller.scan_now();
                }
                TrayCommand::Quit => {
                    self.quit_requested = true;
                    self.controller.stop();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context, snapshot_active: bool) {
        if self.settings.open
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.settings.open = false;
        }

        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma)) {
            self.settings.open = true;
        }

        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::R)) {
            if self.controller.is_running() || snapshot_active {
                self.controller.stop();
            } else if !self.prefs.nim.trim().is_empty() {
                self.handle_start_monitoring(ctx);
            }
        }

        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F5))
            && (self.controller.is_running() || snapshot_active)
        {
            let _ = self.controller.scan_now();
        }

        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Q)) {
            self.quit_requested = true;
            self.controller.stop();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn apply_dotenv_result(&mut self, res: prefs::DotenvImportResult) {
        let pw = res.password.unwrap_or_default();
        let nim = res.prefs.nim.trim().to_string();

        let mut merged_prefs = self.prefs.clone();
        merged_prefs.nim = nim.clone();
        merged_prefs.base_url = res.prefs.base_url;
        merged_prefs.target_url = res.prefs.target_url;
        merged_prefs.interval = res.prefs.interval;
        merged_prefs.delay_min = res.prefs.delay_min;
        merged_prefs.delay_max = res.prefs.delay_max;
        merged_prefs.max_retry = res.prefs.max_retry;
        merged_prefs.retry_wait = res.prefs.retry_wait;

        let open = self.settings.open;
        let mut next = SettingsState::from_prefs(&merged_prefs, pw.clone());
        next.open = open;

        let mut cred_saved = false;
        if !nim.is_empty() && !pw.is_empty() {
            if let Err(err) = self.creds.set_password(&nim, &pw) {
                next.status_message = Some(format!(
                    "File .env diimpor dari {}, namun gagal menyimpan kata sandi ke penyimpanan aman: {err}",
                    res.source_path.display()
                ));
                next.status_is_error = true;
                self.settings = next;
                return;
            }
            cred_saved = true;
        }

        next.status_message = Some(if cred_saved {
            format!(
                "Kredensial berhasil diimpor dari {} dan disimpan.",
                res.source_path.display()
            )
        } else {
            format!(
                "Konfigurasi berhasil diimpor dari {}.",
                res.source_path.display()
            )
        });
        next.status_is_error = false;
        self.settings = next;
    }

    fn handle_import_dotenv(&mut self) {
        let import_result = if self.headless {
            prefs::detect_env_candidates()
                .into_iter()
                .next()
                .map(|path| prefs::import_dotenv_file(&path))
        } else {
            match prefs::pick_dotenv_and_import() {
                Some(res) => Some(res),
                None => prefs::detect_env_candidates()
                    .into_iter()
                    .next()
                    .map(|path| prefs::import_dotenv_file(&path)),
            }
        };

        let Some(result) = import_result else {
            self.settings.status_message = Some(
                "Tidak ada file .env yang dipilih atau ditemukan di direktori standar.".to_string(),
            );
            self.settings.status_is_error = true;
            return;
        };

        match result {
            Ok(res) => self.apply_dotenv_result(res),
            Err(err) => {
                self.settings.status_message = Some(format!("Gagal mengimpor .env: {err}"));
                self.settings.status_is_error = true;
            }
        }
    }

    fn handle_export_diagnostic_html(&mut self, ctx: &egui::Context) {
        if self.diag_rx.is_some() {
            self.settings.status_message =
                Some("Ekspor HTML diagnostik sedang berjalan, harap tunggu...".to_string());
            self.settings.status_is_error = false;
            return;
        }
        if let Err(msg) = self.settings.validate_and_sync() {
            self.settings.status_message = Some(msg);
            self.settings.status_is_error = true;
            return;
        }
        let nim = self.settings.nim.trim();
        let pw = if !self.settings.password.is_empty() {
            self.settings.password.clone()
        } else {
            self.resolve_password()
        };
        if nim.is_empty() || pw.is_empty() {
            self.settings.status_message = Some(
                "NIM dan kata sandi harus diisi sebelum mengunduh HTML diagnostik.".to_string(),
            );
            self.settings.status_is_error = true;
            return;
        }
        if self.headless {
            self.settings.status_message = Some(
                "Ekspor HTML diagnostik hanya tersedia pada mode desktop interaktif.".to_string(),
            );
            self.settings.status_is_error = false;
            return;
        }
        let Some(out_path) = prefs::pick_diagnostic_html_save_path() else {
            return;
        };

        let mut temp_prefs = self.prefs.clone();
        self.settings.apply_to_prefs(&mut temp_prefs);
        let cfg = temp_prefs.to_core_config(pw);

        let (tx, rx) = mpsc::channel();
        let wake_ctx = ctx.clone();
        match std::thread::Builder::new()
            .name("absen-diag-export".to_string())
            .spawn(move || {
                let res = EngineController::dump_diagnostic_html(&cfg, &out_path);
                let _ = tx.send(res);
                wake_ctx.request_repaint();
            }) {
            Ok(_) => {
                self.diag_rx = Some(rx);
                self.settings.status_message =
                    Some("Mengambil HTML halaman absen untuk diagnostik...".to_string());
                self.settings.status_is_error = false;
            }
            Err(err) => {
                self.settings.status_message =
                    Some(format!("Gagal memulai ekspor HTML diagnostik: {err}"));
                self.settings.status_is_error = true;
            }
        }
    }

    fn poll_diagnostic_export(&mut self) {
        let result = match &self.diag_rx {
            Some(rx) => match rx.try_recv() {
                Ok(res) => Some(res),
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                    "Proses ekspor HTML diagnostik berhenti sebelum selesai.".to_string(),
                )),
                Err(mpsc::TryRecvError::Empty) => None,
            },
            None => None,
        };
        if let Some(res) = result {
            self.diag_rx = None;
            match res {
                Ok(saved) => {
                    self.settings.status_message =
                        Some(format!("HTML diagnostik tersimpan ke {}.", saved.display()));
                    self.settings.status_is_error = false;
                }
                Err(err) => {
                    self.settings.status_message =
                        Some(format!("Gagal menyimpan HTML diagnostik: {err}"));
                    self.settings.status_is_error = true;
                }
            }
        }
    }

    fn save_onboarding_and_start(&mut self, ctx: &egui::Context) {
        if self.settings.nim.trim().is_empty() {
            self.settings.status_message = Some("NIM Mahasiswa wajib diisi.".to_string());
            self.settings.status_is_error = true;
            return;
        }
        if self.settings.password.is_empty() {
            self.settings.status_message = Some("Kata Sandi Akademik wajib diisi.".to_string());
            self.settings.status_is_error = true;
            return;
        }
        if self.save_settings_inner(ctx, false) {
            self.handle_start_monitoring(ctx);
        }
    }

    fn save_settings_inner(&mut self, ctx: &egui::Context, close_modal: bool) -> bool {
        if let Err(msg) = self.settings.validate_and_sync() {
            self.settings.status_message = Some(msg);
            self.settings.status_is_error = true;
            return false;
        }

        self.settings.apply_to_prefs(&mut self.prefs);
        if !self.prefs.nim.is_empty() && !self.settings.password.is_empty() {
            if let Err(e) = self
                .creds
                .set_password(&self.prefs.nim, &self.settings.password)
            {
                self.settings.status_message = Some(format!("Gagal menyimpan kredensial: {e}"));
                self.settings.status_is_error = true;
                return false;
            }
        }
        let mut autostart_err = None;
        if !self.headless {
            if let Err(e) = prefs::save(&self.prefs) {
                self.settings.status_message = Some(format!("Gagal menyimpan preferensi: {e}"));
                self.settings.status_is_error = true;
                return false;
            }
            if let Err(e) = self.autostart.set_enabled(self.prefs.autostart) {
                autostart_err = Some(e);
            }
        }
        self.notifier.set_enabled(self.prefs.notifications_enabled);
        self.controller.set_notifier(self.notifier.clone());
        apply_theme(ctx, self.prefs.theme);
        if let Some(err) = autostart_err {
            self.settings.status_message = Some(format!(
                "Pengaturan tersimpan, namun gagal mengatur autostart: {err}"
            ));
            self.settings.status_is_error = true;
            return true;
        }
        self.settings.status_message = Some("Pengaturan tersimpan.".to_string());
        self.settings.status_is_error = false;
        if close_modal {
            self.settings.open = false;
        }
        true
    }

    /// Execute one UI frame against `ctx` (shared by `eframe::App::update` and headless tests).
    pub fn update_ui(&mut self, ctx: &egui::Context) {
        if self.start_minimized && !self.minimized_applied {
            self.minimized_applied = true;
            if self.tray.is_some() {
                self.controller.set_window_visible(false);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        }

        if let Some(flag) = &self.external_show_flag {
            if flag.swap(false, Ordering::AcqRel) {
                self.reveal_window(ctx);
            }
        }

        self.handle_tray_commands(ctx);
        self.poll_diagnostic_export();

        if ctx.input(|i| i.viewport().close_requested())
            && !self.quit_requested
            && self.prefs.minimize_to_tray
            && self.tray.is_some()
        {
            self.controller.set_window_visible(false);
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        let palette = apply_theme(ctx, self.prefs.theme);
        let snapshot = self.current_snapshot();
        self.handle_shortcuts(ctx, snapshot.status.is_active());

        if let Some(tray) = &mut self.tray {
            let icon_status = match snapshot.status {
                crate::bridge::EngineStatus::Monitoring
                | crate::bridge::EngineStatus::ScanningNow
                | crate::bridge::EngineStatus::Connecting => IconStatus::Monitoring,
                crate::bridge::EngineStatus::AuthFailed
                | crate::bridge::EngineStatus::NetworkError => IconStatus::Error,
                _ => IconStatus::Idle,
            };
            tray.update_status(icon_status, snapshot.status.is_active());
        }

        let screen_w = ctx.screen_rect().width();
        let outer_margin = if screen_w < ThemeTokens::BREAKPOINT_MEDIUM {
            ThemeTokens::SPACE_MD
        } else {
            ThemeTokens::SPACE_LG
        };

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(palette.bg_canvas)
                    .inner_margin(egui::Margin::same(outer_margin)),
            )
            .show(ctx, |ui| {
                let width = ui.available_width();
                if let Some(action) = header::show(
                    ui,
                    &snapshot,
                    &self.prefs.nim,
                    self.prefs.interval,
                    self.prefs.theme,
                    palette,
                    width,
                ) {
                    match action {
                        HeaderAction::StartMonitoring => self.handle_start_monitoring(ctx),
                        HeaderAction::StopMonitoring => self.controller.stop(),
                        HeaderAction::ScanNow => {
                            let _ = self.controller.scan_now();
                        }
                        HeaderAction::OpenSettings => {
                            self.settings.open = true;
                        }
                        HeaderAction::CycleTheme => self.cycle_theme(ctx),
                    }
                }

                ui.add_space(ThemeTokens::SPACE_LG);

                if self.prefs.nim.trim().is_empty() {
                    let compact = width < ThemeTokens::BREAKPOINT_MEDIUM;
                    let backend = self.creds.backend();
                    egui::ScrollArea::vertical()
                        .id_salt("onboarding_scroll")
                        .show(ui, |ui| {
                            if let Some(onb_action) = settings::show_onboarding(
                                ui,
                                &mut self.settings,
                                backend,
                                palette,
                                compact,
                            ) {
                                match onb_action {
                                    OnboardingAction::SaveAndStart => {
                                        self.save_onboarding_and_start(ctx);
                                    }
                                    OnboardingAction::ImportDotenv => {
                                        self.handle_import_dotenv();
                                    }
                                }
                            }
                        });
                    return;
                }

                let mut jadwal_action = None;
                if width >= ThemeTokens::BREAKPOINT_WIDE {
                    let gutter = ThemeTokens::SPACE_LG;
                    let left_w = (width - gutter) * 0.62;
                    let right_w = (width - gutter - left_w).max(200.0);
                    let total_rect = ui.available_rect_before_wrap();
                    let body_h = total_rect.height().max(200.0);
                    let left_rect =
                        egui::Rect::from_min_size(total_rect.min, egui::vec2(left_w, body_h));
                    let right_rect = egui::Rect::from_min_size(
                        egui::pos2(total_rect.left() + left_w + gutter, total_rect.top()),
                        egui::vec2(right_w, body_h),
                    );
                    ui.allocate_rect(total_rect, egui::Sense::hover());
                    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(left_rect), |ui| {
                        jadwal_action = jadwal::show(ui, &snapshot, palette, false, true);
                    });
                    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(right_rect), |ui| {
                        activity::show(ui, &snapshot, palette, false, true);
                    });
                } else if width >= ThemeTokens::BREAKPOINT_MEDIUM {
                    egui::ScrollArea::vertical()
                        .id_salt("medium_main_scroll")
                        .show(ui, |ui| {
                            jadwal_action = jadwal::show(ui, &snapshot, palette, false, false);
                            ui.add_space(ThemeTokens::SPACE_LG);
                            activity::show(ui, &snapshot, palette, false, false);
                        });
                } else {
                    let tab_w = ((width - ThemeTokens::SPACE_SM) * 0.5).max(110.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = ThemeTokens::SPACE_SM;
                        if action_button(
                            ui,
                            palette,
                            if self.compact_tab == CompactTab::Jadwal {
                                ButtonTone::Primary
                            } else {
                                ButtonTone::Secondary
                            },
                            "Jadwal",
                            None,
                            true,
                            tab_w,
                        )
                        .clicked()
                        {
                            self.compact_tab = CompactTab::Jadwal;
                        }
                        if action_button(
                            ui,
                            palette,
                            if self.compact_tab == CompactTab::Activity {
                                ButtonTone::Primary
                            } else {
                                ButtonTone::Secondary
                            },
                            "Aktivitas",
                            None,
                            true,
                            tab_w,
                        )
                        .clicked()
                        {
                            self.compact_tab = CompactTab::Activity;
                        }
                    });
                    ui.add_space(ThemeTokens::SPACE_MD);
                    egui::ScrollArea::vertical()
                        .id_salt("compact_main_scroll")
                        .show(ui, |ui| match self.compact_tab {
                            CompactTab::Jadwal => {
                                jadwal_action = jadwal::show(ui, &snapshot, palette, true, false);
                            }
                            CompactTab::Activity => {
                                activity::show(ui, &snapshot, palette, true, false);
                            }
                        });
                }

                if let Some(act) = jadwal_action {
                    match act {
                        JadwalAction::OpenSettings => self.settings.open = true,
                        JadwalAction::RetryConnection | JadwalAction::StartMonitoring => {
                            self.handle_start_monitoring(ctx);
                        }
                        JadwalAction::ScanNow => {
                            let _ = self.controller.scan_now();
                        }
                    }
                }
            });

        let backend = self.creds.backend();
        if let Some(action) = settings::show(ctx, &mut self.settings, backend, palette) {
            match action {
                SettingsAction::Close => self.settings.open = false,
                SettingsAction::Save => {
                    self.save_settings_inner(ctx, true);
                }
                SettingsAction::ImportDotenv => {
                    self.handle_import_dotenv();
                }
                SettingsAction::ExportDiagnosticHtml => {
                    self.handle_export_diagnostic_html(ctx);
                }
            }
        }
    }
}

impl eframe::App for DesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_ui(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::MemoryFallbackStore;

    #[test]
    fn apply_dotenv_result_persists_password_to_credential_store() {
        let ctx = egui::Context::default();
        let creds = MemoryFallbackStore::new();
        let mut app = DesktopApp::new_headless(&ctx, AppPrefs::default(), Box::new(creds));

        let res = prefs::DotenvImportResult {
            prefs: AppPrefs {
                nim: "251511099".to_string(),
                interval: 45,
                ..AppPrefs::default()
            },
            password: Some("rahasia#123".to_string()),
            source_path: PathBuf::from(".env"),
        };

        app.apply_dotenv_result(res);
        assert_eq!(app.settings.nim, "251511099");
        assert_eq!(app.settings.password, "rahasia#123");
        assert_eq!(app.settings.interval, 45);
        assert!(!app.settings.status_is_error);
        assert_eq!(
            app.creds.get_password("251511099").unwrap().as_deref(),
            Some("rahasia#123")
        );
    }

    #[test]
    fn reveal_window_restores_controller_window_visibility() {
        let ctx = egui::Context::default();
        let app = DesktopApp::new_headless(
            &ctx,
            AppPrefs::default(),
            Box::new(MemoryFallbackStore::new()),
        );
        app.controller.set_window_visible(false);
        assert!(!app.controller.is_window_visible());

        app.reveal_window(&ctx);
        assert!(app.controller.is_window_visible());
    }
}

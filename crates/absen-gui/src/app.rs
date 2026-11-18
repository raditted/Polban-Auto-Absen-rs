//! Top-level `eframe::App` state, shortcuts, and responsive 3-breakpoint orchestration.

use absen_core::AbsenClient;

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
}

impl DesktopApp {
    /// Production desktop constructor loading OS preferences, keyring, and tray icon.
    pub fn new(cc: &eframe::CreationContext<'_>, start_minimized: bool) -> Self {
        configure_fonts(&cc.egui_ctx);

        let prefs = prefs::load().unwrap_or_default();
        apply_theme(&cc.egui_ctx, prefs.theme);

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
        let autostart = AutostartManager::new("Polban Presensi");
        let tray = TrayManager::new().ok();

        let mut app = Self {
            prefs,
            creds,
            controller: EngineController::new(),
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

        Self {
            prefs,
            creds,
            controller: EngineController::new(),
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
        }
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
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
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

    fn handle_import_dotenv(&mut self) {
        let candidate = prefs::detect_env_candidates()
            .into_iter()
            .next()
            .or_else(|| {
                if self.headless {
                    None
                } else {
                    rfd::FileDialog::new()
                        .set_title("Pilih file .env Polban Presensi")
                        .pick_file()
                }
            });

        let Some(path) = candidate else {
            self.settings.status_message =
                Some("Tidak ditemukan file .env di direktori standar.".to_string());
            self.settings.status_is_error = true;
            return;
        };

        match prefs::import_dotenv_file(&path) {
            Ok(res) => {
                let pw = res.password.unwrap_or_default();
                let open = self.settings.open;
                let mut next = SettingsState::from_prefs(&res.prefs, pw);
                next.open = open;
                next.status_message = Some(format!(
                    "Kredensial berhasil diimpor dari {}.",
                    res.source_path.display()
                ));
                next.status_is_error = false;
                self.settings = next;
            }
            Err(err) => {
                self.settings.status_message = Some(format!("Gagal mengimpor .env: {err}"));
                self.settings.status_is_error = true;
            }
        }
    }

    fn handle_export_diagnostic_html(&mut self) {
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
        let Some(out_path) = rfd::FileDialog::new()
            .set_title("Simpan HTML Halaman Absen (Diagnostik)")
            .set_file_name("absen-diagnostik.html")
            .save_file()
        else {
            return;
        };

        let mut temp_prefs = self.prefs.clone();
        self.settings.apply_to_prefs(&mut temp_prefs);
        let cfg = temp_prefs.to_core_config(pw);
        let export_result = match AbsenClient::new(&cfg.base_url) {
            Ok(client) => client
                .dump_absen_html(&cfg, &out_path)
                .map_err(|e| e.to_string()),
            Err(err) => Err(err.to_string()),
        };
        match export_result {
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
        if !self.headless {
            if let Err(e) = prefs::save(&self.prefs) {
                self.settings.status_message = Some(format!("Gagal menyimpan preferensi: {e}"));
                self.settings.status_is_error = true;
                return false;
            }
            let _ = self.autostart.set_enabled(self.prefs.autostart);
        }
        self.notifier.set_enabled(self.prefs.notifications_enabled);
        apply_theme(ctx, self.prefs.theme);
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
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        }

        self.handle_tray_commands(ctx);

        if ctx.input(|i| i.viewport().close_requested())
            && !self.quit_requested
            && self.prefs.minimize_to_tray
            && self.tray.is_some()
        {
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
                    self.handle_export_diagnostic_html();
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

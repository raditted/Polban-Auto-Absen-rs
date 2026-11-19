//! Top-level `eframe::App` state, shortcuts, and responsive 3-breakpoint orchestration.

use std::cell::{Ref, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use crate::bridge::{EngineController, GuiSnapshot};
use crate::credentials::{open_default_store, CredentialStore};
use crate::icon::IconStatus;
#[cfg(target_os = "windows")]
use crate::platform::win32;
use crate::platform::{AutostartManager, Notifier, TrayCommand, TrayManager};
use crate::prefs::{self, AppPrefs, DotenvImportResult, PrefsError, ThemePreference};
use crate::theme::{
    apply_appearance, configure_fonts, effective_dark_mode, AppearanceConfig, ThemeTokens,
};
use crate::ui::header::HeaderAction;
use crate::ui::jadwal::JadwalAction;
use crate::ui::settings::{OnboardingAction, SettingsAction, SettingsState};
use crate::ui::{activity, header, jadwal, settings, CompactTab};
use crate::updater::{UpdateManager, UpdateStatus};

struct AppCore {
    prefs: AppPrefs,
    creds: Box<dyn CredentialStore>,
    controller: EngineController,
    tray: Option<TrayManager>,
    notifier: Notifier,
    autostart: AutostartManager,
    settings: SettingsState,
    updater: UpdateManager,
    compact_tab: CompactTab,
    start_minimized: bool,
    minimized_applied: bool,
    quit_requested: bool,
    headless: bool,
    test_snapshot: Option<GuiSnapshot>,
    diag_rx: Option<mpsc::Receiver<Result<PathBuf, String>>>,
    import_pick_rx: Option<mpsc::Receiver<Option<Result<DotenvImportResult, PrefsError>>>>,
    diag_pick_rx: Option<mpsc::Receiver<(absen_core::Config, Option<PathBuf>)>>,
    external_show_flag: Option<Arc<AtomicBool>>,
    release_instance_lock: Option<Arc<dyn Fn() + Send + Sync>>,
    native_hwnd: isize,
    win32_msg_hwnd: isize,
}

impl AppCore {
    fn reveal_window(&self, ctx: &egui::Context) {
        self.controller.set_window_visible(true);
        #[cfg(target_os = "windows")]
        win32::restore_and_focus_window(self.native_hwnd);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }

    fn current_snapshot(&self) -> GuiSnapshot {
        self.test_snapshot
            .clone()
            .unwrap_or_else(|| self.controller.snapshot())
    }

    fn sync_tray_status(&mut self) {
        let snapshot = self.current_snapshot();
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
    }

    fn persist_prefs(&self) {
        if !self.headless {
            let _ = prefs::save(&self.prefs);
        }
    }

    fn prefs_appearance(&self) -> AppearanceConfig {
        AppearanceConfig {
            mode: self.prefs.theme,
            light_preset: self.prefs.light_preset,
            dark_preset: self.prefs.dark_preset,
        }
    }

    fn active_appearance(&self) -> AppearanceConfig {
        if self.settings.open {
            self.settings.appearance()
        } else {
            self.prefs_appearance()
        }
    }

    fn open_settings(&mut self) {
        self.settings.theme = self.prefs.theme;
        self.settings.light_preset = self.prefs.light_preset;
        self.settings.dark_preset = self.prefs.dark_preset;
        self.settings.open = true;
    }

    fn close_settings(&mut self, ctx: &egui::Context) {
        self.settings.open = false;
        self.settings.theme = self.prefs.theme;
        self.settings.light_preset = self.prefs.light_preset;
        self.settings.dark_preset = self.prefs.dark_preset;
        apply_appearance(ctx, self.prefs_appearance());
    }

    fn toggle_theme(&mut self, ctx: &egui::Context) {
        let is_dark = effective_dark_mode(ctx, self.prefs.theme);
        self.prefs.theme = if is_dark {
            ThemePreference::Light
        } else {
            ThemePreference::Dark
        };
        self.settings.theme = self.prefs.theme;
        apply_appearance(ctx, self.prefs_appearance());
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
        #[cfg(target_os = "windows")]
        let wake_msg_hwnd = self.win32_msg_hwnd;
        if let Err(err) = self.controller.start(
            cfg,
            true,
            Some(Box::new(move || {
                #[cfg(target_os = "windows")]
                if wake_msg_hwnd != 0 {
                    win32::post_app_message(wake_msg_hwnd, win32::WM_APP_SYNC_TRAY);
                }
                wake_ctx.request_repaint();
            })),
        ) {
            self.settings.status_message = Some(format!("Gagal memulai pemantauan: {err}"));
            self.settings.status_is_error = true;
        }
    }

    fn perform_quit(&mut self, ctx: &egui::Context) {
        self.quit_requested = true;
        self.controller.stop();
        self.persist_prefs();
        self.tray.take();

        #[cfg(target_os = "windows")]
        {
            if self.native_hwnd != 0 && !win32::is_window_visible(self.native_hwnd) {
                win32::destroy_window(self.native_hwnd);
                return;
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            ctx.request_repaint();
            win32::post_close_window(self.native_hwnd);
        }

        #[cfg(not(target_os = "windows"))]
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            ctx.request_repaint();
        }
    }

    fn execute_tray_command(&mut self, cmd: TrayCommand, ctx: &egui::Context) {
        match cmd {
            TrayCommand::ShowWindow => {
                self.reveal_window(ctx);
            }
            TrayCommand::ToggleMonitoring => {
                if self.controller.is_running() {
                    self.controller.stop();
                } else {
                    self.handle_start_monitoring(ctx);
                    if !self.controller.is_running() {
                        self.reveal_window(ctx);
                    }
                }
                self.sync_tray_status();
                ctx.request_repaint();
            }
            TrayCommand::ScanNow => {
                let _ = self.controller.scan_now();
                ctx.request_repaint();
            }
            TrayCommand::Quit => {
                self.perform_quit(ctx);
            }
        }
    }

    fn handle_tray_commands(&mut self, ctx: &egui::Context) {
        let commands = match &self.tray {
            Some(tray) => tray.poll_commands(),
            None => Vec::new(),
        };
        for cmd in commands {
            self.execute_tray_command(cmd, ctx);
            if self.quit_requested {
                break;
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context, snapshot_active: bool) {
        if self.settings.open
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.close_settings(ctx);
        }

        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma)) {
            self.open_settings();
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
            self.perform_quit(ctx);
        }
    }

    fn apply_dotenv_result(&mut self, res: DotenvImportResult) {
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

    fn finish_dotenv_import(
        &mut self,
        import_result: Option<Result<DotenvImportResult, PrefsError>>,
    ) {
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

    fn handle_import_dotenv(&mut self, ctx: &egui::Context) {
        if self.headless {
            let import_result = prefs::detect_env_candidates()
                .into_iter()
                .next()
                .map(|path| prefs::import_dotenv_file(&path));
            self.finish_dotenv_import(import_result);
            return;
        }

        #[cfg(target_os = "windows")]
        {
            if self.import_pick_rx.is_some() {
                return;
            }
            let (tx, rx) = mpsc::channel();
            let wake_ctx = ctx.clone();
            if std::thread::Builder::new()
                .name("polban-dotenv-picker".to_string())
                .spawn(move || {
                    let picked = match prefs::pick_dotenv_and_import() {
                        Some(res) => Some(res),
                        None => prefs::detect_env_candidates()
                            .into_iter()
                            .next()
                            .map(|path| prefs::import_dotenv_file(&path)),
                    };
                    let _ = tx.send(picked);
                    wake_ctx.request_repaint();
                })
                .is_ok()
            {
                self.import_pick_rx = Some(rx);
                return;
            }
        }

        let _ = ctx;
        let import_result = match prefs::pick_dotenv_and_import() {
            Some(res) => Some(res),
            None => prefs::detect_env_candidates()
                .into_iter()
                .next()
                .map(|path| prefs::import_dotenv_file(&path)),
        };
        self.finish_dotenv_import(import_result);
    }

    fn spawn_diagnostic_export(
        &mut self,
        ctx: &egui::Context,
        cfg: absen_core::Config,
        out_path: PathBuf,
    ) {
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

    fn handle_export_diagnostic_html(&mut self, ctx: &egui::Context) {
        if self.diag_rx.is_some() || self.diag_pick_rx.is_some() {
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

        let mut temp_prefs = self.prefs.clone();
        self.settings.apply_to_prefs(&mut temp_prefs);
        let cfg = temp_prefs.to_core_config(pw);

        #[cfg(target_os = "windows")]
        {
            let (tx, rx) = mpsc::channel();
            let wake_ctx = ctx.clone();
            match std::thread::Builder::new()
                .name("polban-diag-picker".to_string())
                .spawn(move || {
                    let path = prefs::pick_diagnostic_html_save_path();
                    let _ = tx.send((cfg, path));
                    wake_ctx.request_repaint();
                }) {
                Ok(_) => self.diag_pick_rx = Some(rx),
                Err(err) => {
                    self.settings.status_message =
                        Some(format!("Gagal membuka dialog penyimpanan: {err}"));
                    self.settings.status_is_error = true;
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        if let Some(out_path) = prefs::pick_diagnostic_html_save_path() {
            self.spawn_diagnostic_export(ctx, cfg, out_path);
        }
    }

    fn handle_restart_to_update(&mut self, ctx: &egui::Context) {
        let plan = match &self.updater.status {
            UpdateStatus::ReadyToRestart { plan, .. } => plan.clone(),
            _ => return,
        };
        self.controller.stop();
        self.persist_prefs();
        self.tray.take();

        let release_cb = self.release_instance_lock.clone();
        let release_fn = || {
            if let Some(cb) = &release_cb {
                cb();
            }
        };

        if self.headless {
            release_fn();
            self.quit_requested = true;
            return;
        }

        match crate::updater::apply_update_and_restart(&plan, &release_fn) {
            Ok(()) => {
                std::process::exit(0);
            }
            Err(err) => {
                self.updater.manual_note = Some((err.clone(), true));
                self.updater.status = UpdateStatus::Failed {
                    message: err,
                    retry_candidate: None,
                };
                ctx.request_repaint();
            }
        }
    }

    fn poll_background_tasks(&mut self, ctx: &egui::Context) {
        self.updater.poll();

        if let Some(rx) = &self.import_pick_rx {
            match rx.try_recv() {
                Ok(picked) => {
                    self.import_pick_rx = None;
                    self.finish_dotenv_import(picked);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.import_pick_rx = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }

        if let Some(rx) = &self.diag_pick_rx {
            match rx.try_recv() {
                Ok((cfg, Some(out_path))) => {
                    self.diag_pick_rx = None;
                    self.spawn_diagnostic_export(ctx, cfg, out_path);
                }
                Ok((_, None)) | Err(mpsc::TryRecvError::Disconnected) => {
                    self.diag_pick_rx = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }

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
        apply_appearance(ctx, self.prefs_appearance());
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

    fn update_ui(&mut self, ctx: &egui::Context) {
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
        if self.quit_requested {
            return;
        }
        self.poll_background_tasks(ctx);

        if ctx.input(|i| i.viewport().close_requested())
            && !self.quit_requested
            && self.prefs.minimize_to_tray
            && self.tray.is_some()
        {
            self.controller.set_window_visible(false);
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        let palette = apply_appearance(ctx, self.active_appearance());
        let is_dark = palette.dark_mode;
        let snapshot = self.current_snapshot();
        self.handle_shortcuts(ctx, snapshot.status.is_active());
        if self.quit_requested {
            return;
        }

        self.sync_tray_status();

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
                let avail_rect = ui.available_rect_before_wrap();
                let avail_w = avail_rect.width();
                let container_w = avail_w.min(ThemeTokens::MAX_CONTAINER_WIDTH);
                let side_pad = ((avail_w - container_w) * 0.5).max(0.0);
                let container_rect = egui::Rect::from_min_size(
                    egui::pos2(avail_rect.left() + side_pad, avail_rect.top()),
                    egui::vec2(container_w, avail_rect.height().max(120.0)),
                );
                ui.allocate_rect(avail_rect, egui::Sense::hover());

                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(container_rect), |ui| {
                    ui.set_width(container_w);
                    let width = container_w;
                    if let Some(action) = header::show(
                        ui,
                        &snapshot,
                        &self.prefs.nim,
                        self.prefs.interval,
                        is_dark,
                        palette,
                        width,
                        &self.updater.status,
                    ) {
                        match action {
                            HeaderAction::StartMonitoring => self.handle_start_monitoring(ctx),
                            HeaderAction::StopMonitoring => self.controller.stop(),
                            HeaderAction::ScanNow => {
                                let _ = self.controller.scan_now();
                            }
                            HeaderAction::OpenSettings => self.open_settings(),
                            HeaderAction::CycleTheme => self.toggle_theme(ctx),
                            HeaderAction::StartUpdate => self.updater.start_download(ctx),
                            HeaderAction::RestartToUpdate => self.handle_restart_to_update(ctx),
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
                                            self.handle_import_dotenv(ctx);
                                        }
                                    }
                                }
                            });
                        return;
                    }

                    let mut jadwal_action = None;
                    if width >= ThemeTokens::BREAKPOINT_WIDE {
                        let gutter = ThemeTokens::SPACE_LG;
                        let left_w = (width - gutter) * 0.60;
                        let right_w = (width - gutter - left_w).max(240.0);
                        let total_rect = ui.available_rect_before_wrap();
                        let body_h = total_rect.height().max(220.0);
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
                    } else {
                        let compact = width < ThemeTokens::BREAKPOINT_MEDIUM;
                        egui::ScrollArea::vertical()
                            .id_salt("stacked_main_scroll")
                            .show(ui, |ui| {
                                jadwal_action =
                                    jadwal::show(ui, &snapshot, palette, compact, false);
                                ui.add_space(ThemeTokens::SPACE_LG);
                                activity::show(ui, &snapshot, palette, compact, false);
                            });
                    }

                    if let Some(act) = jadwal_action {
                        match act {
                            JadwalAction::OpenSettings => self.open_settings(),
                            JadwalAction::RetryConnection | JadwalAction::StartMonitoring => {
                                self.handle_start_monitoring(ctx);
                            }
                            JadwalAction::ScanNow => {
                                let _ = self.controller.scan_now();
                            }
                        }
                    }
                });
            });

        let backend = self.creds.backend();
        if let Some(action) = settings::show(
            ctx,
            &mut self.settings,
            backend,
            palette,
            &self.updater.status,
            self.updater.checking_manual,
            self.updater.manual_note.as_ref(),
        ) {
            match action {
                SettingsAction::Close => self.close_settings(ctx),
                SettingsAction::Save => {
                    self.save_settings_inner(ctx, true);
                }
                SettingsAction::ImportDotenv => {
                    self.handle_import_dotenv(ctx);
                }
                SettingsAction::ExportDiagnosticHtml => {
                    self.handle_export_diagnostic_html(ctx);
                }
                SettingsAction::PreviewTheme(appearance) => {
                    apply_appearance(ctx, appearance);
                }
                SettingsAction::CheckForUpdates => {
                    self.updater.start_manual_check(ctx);
                }
                SettingsAction::StartUpdate => {
                    self.updater.start_download(ctx);
                }
                SettingsAction::RestartToUpdate => {
                    self.handle_restart_to_update(ctx);
                }
            }
        }
    }
}

/// Main desktop application state implementing `eframe::App`.
pub struct DesktopApp {
    core: Rc<RefCell<AppCore>>,
    #[cfg(target_os = "windows")]
    msg_window: Option<win32::MessageWindow>,
}

impl DesktopApp {
    /// Production desktop constructor loading OS preferences, keyring, and tray icon.
    pub fn new(cc: &eframe::CreationContext<'_>, start_minimized: bool) -> Self {
        configure_fonts(&cc.egui_ctx);

        let mut prefs = prefs::load().unwrap_or_default();
        apply_appearance(
            &cc.egui_ctx,
            AppearanceConfig {
                mode: prefs.theme,
                light_preset: prefs.light_preset,
                dark_preset: prefs.dark_preset,
            },
        );

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
        let tray = TrayManager::new().ok();

        let mut controller = EngineController::new();
        controller.set_notifier(notifier.clone());
        if start_minimized && tray.is_some() {
            controller.set_window_visible(false);
        }

        #[cfg(target_os = "windows")]
        let native_hwnd = win32::extract_hwnd(cc).unwrap_or(0);
        #[cfg(not(target_os = "windows"))]
        let native_hwnd = 0;

        let core = Rc::new(RefCell::new(AppCore {
            prefs,
            creds,
            controller,
            tray,
            notifier,
            autostart,
            settings,
            updater: UpdateManager::new(),
            compact_tab: CompactTab::Jadwal,
            start_minimized,
            minimized_applied: false,
            quit_requested: false,
            headless: false,
            test_snapshot: None,
            diag_rx: None,
            import_pick_rx: None,
            diag_pick_rx: None,
            external_show_flag: None,
            release_instance_lock: None,
            native_hwnd,
            win32_msg_hwnd: 0,
        }));

        #[cfg(target_os = "windows")]
        let msg_window = {
            let msg_weak = Rc::downgrade(&core);
            let msg_ctx = cc.egui_ctx.clone();
            win32::MessageWindow::new(move |msg| {
                if let Some(rc) = msg_weak.upgrade() {
                    if let Ok(mut c) = rc.try_borrow_mut() {
                        match msg {
                            win32::WM_APP_SHOW_WINDOW => c.reveal_window(&msg_ctx),
                            win32::WM_APP_TRAY_CMD => {
                                c.handle_tray_commands(&msg_ctx);
                                if !c.quit_requested {
                                    c.sync_tray_status();
                                }
                            }
                            win32::WM_APP_SYNC_TRAY => {
                                c.sync_tray_status();
                            }
                            _ => {}
                        }
                    }
                }
            })
            .ok()
        };

        #[cfg(target_os = "windows")]
        let win32_msg_hwnd = msg_window.as_ref().map_or(0, |w| w.hwnd());
        #[cfg(not(target_os = "windows"))]
        let win32_msg_hwnd = 0;

        {
            let mut c = core.borrow_mut();
            c.win32_msg_hwnd = win32_msg_hwnd;

            let waker_ctx = cc.egui_ctx.clone();
            if let Some(t) = &mut c.tray {
                t.set_waker(Arc::new(move |cmd| {
                    #[cfg(target_os = "windows")]
                    if win32_msg_hwnd != 0 {
                        win32::post_app_message(win32_msg_hwnd, win32::WM_APP_TRAY_CMD);
                    }
                    #[cfg(not(target_os = "windows"))]
                    if matches!(cmd, TrayCommand::ShowWindow) {
                        waker_ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        waker_ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                        waker_ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    let _ = cmd;
                    waker_ctx.request_repaint();
                }));
            }

            if !c.prefs.nim.trim().is_empty()
                && c.prefs.auto_start_monitoring
                && !c.settings.password.is_empty()
            {
                c.handle_start_monitoring(&cc.egui_ctx);
            }
            c.updater.start_silent_check(&cc.egui_ctx);
        }

        Self {
            core,
            #[cfg(target_os = "windows")]
            msg_window,
        }
    }

    /// Headless constructor for deterministic UI tests without touching OS prefs or tray.
    pub fn new_headless(
        ctx: &egui::Context,
        prefs: AppPrefs,
        creds: Box<dyn CredentialStore>,
    ) -> Self {
        configure_fonts(ctx);
        apply_appearance(
            ctx,
            AppearanceConfig {
                mode: prefs.theme,
                light_preset: prefs.light_preset,
                dark_preset: prefs.dark_preset,
            },
        );

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

        let core = Rc::new(RefCell::new(AppCore {
            prefs,
            creds,
            controller,
            tray: None,
            notifier,
            autostart,
            settings,
            updater: UpdateManager::new(),
            compact_tab: CompactTab::Jadwal,
            start_minimized: false,
            minimized_applied: true,
            quit_requested: false,
            headless: true,
            test_snapshot: None,
            diag_rx: None,
            import_pick_rx: None,
            diag_pick_rx: None,
            external_show_flag: None,
            release_instance_lock: None,
            native_hwnd: 0,
            win32_msg_hwnd: 0,
        }));

        Self {
            core,
            #[cfg(target_os = "windows")]
            msg_window: None,
        }
    }

    /// Returns the hidden Win32 message window HWND used for out-of-band UI wakeups.
    #[cfg(target_os = "windows")]
    pub fn win32_msg_hwnd(&self) -> Option<isize> {
        self.msg_window.as_ref().map(|w| w.hwnd())
    }

    /// Attach a cross-process wakeup flag used by the single-instance listener.
    pub fn set_external_show_flag(&mut self, flag: Arc<AtomicBool>) {
        self.core.borrow_mut().external_show_flag = Some(flag);
    }

    /// Register a callback that synchronously releases the single-instance lock before restart.
    pub fn set_release_instance_lock(&mut self, cb: Arc<dyn Fn() + Send + Sync>) {
        self.core.borrow_mut().release_instance_lock = Some(cb);
    }

    /// Inject a deterministic `UpdateStatus` for headless UI testing.
    pub fn set_update_status_for_test(&mut self, status: UpdateStatus) {
        self.core.borrow_mut().updater.status = status;
    }

    /// Read the current `UpdateStatus` (used in tests).
    pub fn update_status(&self) -> UpdateStatus {
        self.core.borrow().updater.status.clone()
    }

    /// Inject or clear a deterministic `GuiSnapshot` for headless UI testing.
    pub fn set_test_snapshot(&mut self, snapshot: Option<GuiSnapshot>) {
        self.core.borrow_mut().test_snapshot = snapshot;
    }

    pub fn is_settings_open(&self) -> bool {
        self.core.borrow().settings.open
    }

    pub fn prefs(&self) -> Ref<'_, AppPrefs> {
        Ref::map(self.core.borrow(), |c| &c.prefs)
    }

    pub fn compact_tab(&self) -> CompactTab {
        self.core.borrow().compact_tab
    }

    /// Execute one UI frame against `ctx` (shared by `eframe::App::update` and headless tests).
    pub fn update_ui(&mut self, ctx: &egui::Context) {
        self.core.borrow_mut().update_ui(ctx);
    }
}

impl eframe::App for DesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_ui(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Ok(mut c) = self.core.try_borrow_mut() {
            c.controller.stop();
            c.persist_prefs();
            c.tray.take();
        }
        #[cfg(target_os = "windows")]
        self.msg_window.take();
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
        let app = DesktopApp::new_headless(&ctx, AppPrefs::default(), Box::new(creds));

        let res = prefs::DotenvImportResult {
            prefs: AppPrefs {
                nim: "251511099".to_string(),
                interval: 45,
                ..AppPrefs::default()
            },
            password: Some("rahasia#123".to_string()),
            source_path: PathBuf::from(".env"),
        };

        let mut core = app.core.borrow_mut();
        core.apply_dotenv_result(res);
        assert_eq!(core.settings.nim, "251511099");
        assert_eq!(core.settings.password, "rahasia#123");
        assert_eq!(core.settings.interval, 45);
        assert!(!core.settings.status_is_error);
        assert_eq!(
            core.creds.get_password("251511099").unwrap().as_deref(),
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
        let core = app.core.borrow();
        core.controller.set_window_visible(false);
        assert!(!core.controller.is_window_visible());

        core.reveal_window(&ctx);
        assert!(core.controller.is_window_visible());
    }

    #[test]
    fn execute_tray_commands_handle_show_toggle_scan_and_quit() {
        let ctx = egui::Context::default();
        let app = DesktopApp::new_headless(
            &ctx,
            AppPrefs::default(),
            Box::new(MemoryFallbackStore::new()),
        );
        let mut core = app.core.borrow_mut();

        core.controller.set_window_visible(false);
        core.execute_tray_command(TrayCommand::ShowWindow, &ctx);
        assert!(core.controller.is_window_visible());

        // ToggleMonitoring when NIM is empty opens onboarding/status error and reveals window.
        core.controller.set_window_visible(false);
        core.execute_tray_command(TrayCommand::ToggleMonitoring, &ctx);
        assert!(core.controller.is_window_visible());
        assert!(core.settings.status_is_error);

        // ScanNow when stopped does not panic or quit.
        core.execute_tray_command(TrayCommand::ScanNow, &ctx);
        assert!(!core.quit_requested);

        // Quit sets quit_requested and clears tray handle cleanly.
        core.execute_tray_command(TrayCommand::Quit, &ctx);
        assert!(core.quit_requested);
        assert!(core.tray.is_none());
    }

    #[test]
    fn theme_toggle_and_settings_preview_stay_synchronized() {
        let ctx = egui::Context::default();
        let app = DesktopApp::new_headless(
            &ctx,
            AppPrefs {
                theme: ThemePreference::System,
                ..AppPrefs::default()
            },
            Box::new(MemoryFallbackStore::new()),
        );
        let mut core = app.core.borrow_mut();

        // System defaults to Light in headless context -> toggle flips to Dark and syncs settings.
        core.toggle_theme(&ctx);
        assert_eq!(core.prefs.theme, ThemePreference::Dark);
        assert_eq!(core.settings.theme, ThemePreference::Dark);

        // Opening settings keeps theme and presets synced; previewing changes and closing reverts.
        core.open_settings();
        assert_eq!(core.settings.theme, ThemePreference::Dark);
        assert_eq!(
            core.settings.light_preset,
            prefs::LightThemePreset::DefaultLight
        );
        assert_eq!(
            core.settings.dark_preset,
            prefs::DarkThemePreset::DefaultDark
        );
        core.settings.theme = ThemePreference::Light;
        core.settings.light_preset = prefs::LightThemePreset::SolarizedLight;
        core.settings.dark_preset = prefs::DarkThemePreset::Everforest;
        core.close_settings(&ctx);
        assert_eq!(core.prefs.theme, ThemePreference::Dark);
        assert_eq!(core.settings.theme, ThemePreference::Dark);
        assert_eq!(
            core.settings.light_preset,
            prefs::LightThemePreset::DefaultLight
        );
        assert_eq!(
            core.settings.dark_preset,
            prefs::DarkThemePreset::DefaultDark
        );
    }
}

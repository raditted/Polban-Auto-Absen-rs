//! Top-level `eframe::App` state, shortcuts, and responsive 3-breakpoint orchestration.

use std::cell::{Ref, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crate::bridge::{EngineController, GuiSnapshot, LivePreviewResult};
use crate::credentials::{open_default_store, CredentialStore};
use crate::icon::IconStatus;
#[cfg(target_os = "windows")]
use crate::platform::win32;
use crate::platform::{
    detect_window_backend, needs_window_controls, AutostartManager, Notifier, TrayCommand,
    TrayManager, WindowBackend,
};
use crate::prefs::{self, AppPrefs, DotenvImportResult, PrefsError, ThemePreference};
use crate::screenshot::{
    default_screenshot_filename, render_sample_attendance_png, write_png_atomic, ScreenshotSource,
};
use crate::theme::{
    apply_appearance, configure_fonts, effective_dark_mode, AppearanceConfig, ThemeTokens,
};
use crate::ui::activity::ActivityAction;
use crate::ui::header::HeaderAction;
use crate::ui::jadwal::JadwalAction;
use crate::ui::proof::{ProofAction, ProofScreenshotView};
use crate::ui::settings::{OnboardingAction, SettingsAction, SettingsState};
use crate::ui::titlebar::{self, TitlebarAction};
use crate::ui::{activity, header, jadwal, proof, settings, CompactTab};
use crate::updater::{is_safe_http_url, open_url_in_browser, UpdateManager, UpdateStatus};

struct LoadedProofTexture {
    key: String,
    revision: u64,
    texture: egui::TextureHandle,
    size: [usize; 2],
    source_label: String,
    png_bytes: Arc<Vec<u8>>,
}

struct ImgDecodeResult {
    key: String,
    revision: u64,
    source_label: String,
    png_bytes: Arc<Vec<u8>>,
    image: Result<(egui::ColorImage, [usize; 2]), String>,
}

fn decode_png_rgba(png_bytes: &[u8]) -> Result<(egui::ColorImage, [usize; 2]), String> {
    let dyn_img = image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png)
        .map_err(|e| format!("gagal membaca PNG: {e}"))?;
    let rgba = dyn_img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let pixels = rgba.into_raw();
    Ok((
        egui::ColorImage::from_rgba_unmultiplied(size, &pixels),
        size,
    ))
}

fn spawn_png_decode_bytes(
    ctx: &egui::Context,
    key: String,
    revision: u64,
    source_label: String,
    png_bytes: Arc<Vec<u8>>,
) -> Option<mpsc::Receiver<ImgDecodeResult>> {
    let (tx, rx) = mpsc::channel();
    let wake_ctx = ctx.clone();
    std::thread::Builder::new()
        .name("absen-img-decode".to_string())
        .spawn(move || {
            let image = decode_png_rgba(&png_bytes);
            let _ = tx.send(ImgDecodeResult {
                key,
                revision,
                source_label,
                png_bytes,
                image,
            });
            wake_ctx.request_repaint();
        })
        .ok()?;
    Some(rx)
}

fn spawn_png_decode_from_path(
    ctx: &egui::Context,
    key: String,
    revision: u64,
    path: PathBuf,
) -> Option<mpsc::Receiver<ImgDecodeResult>> {
    let (tx, rx) = mpsc::channel();
    let wake_ctx = ctx.clone();
    std::thread::Builder::new()
        .name("absen-img-decode".to_string())
        .spawn(move || {
            let (png_bytes, source_label, image) = match std::fs::read(&path) {
                Ok(raw) => {
                    let bytes = Arc::new(raw);
                    match decode_png_rgba(&bytes) {
                        Ok((img, size)) => {
                            let label = if size[0] == 960 {
                                ScreenshotSource::NativeFallback.badge_label().to_string()
                            } else {
                                ScreenshotSource::HeadlessBrowser.badge_label().to_string()
                            };
                            (bytes, label, Ok((img, size)))
                        }
                        Err(e) => (bytes, String::new(), Err(e)),
                    }
                }
                Err(e) => (
                    Arc::new(Vec::new()),
                    String::new(),
                    Err(format!("gagal membaca file PNG: {e}")),
                ),
            };
            let _ = tx.send(ImgDecodeResult {
                key,
                revision,
                source_label,
                png_bytes,
                image,
            });
            wake_ctx.request_repaint();
        })
        .ok()?;
    Some(rx)
}

type SavePickResult = (Arc<Vec<u8>>, Option<PathBuf>);

struct AppCore {
    prefs: AppPrefs,
    creds: Box<dyn CredentialStore>,
    controller: EngineController,
    tray: Option<TrayManager>,
    notifier: Notifier,
    autostart: AutostartManager,
    settings: SettingsState,
    updater: UpdateManager,
    proof_modal_open: bool,
    proof_modal_key: Option<String>,
    proof_sample_mode: bool,
    proof_copy_feedback_until: Option<Instant>,
    proof_tech_expanded: bool,
    active_proof_texture: Option<LoadedProofTexture>,
    proof_texture_loading: Option<(String, u64)>,
    img_decode_rx: Option<mpsc::Receiver<ImgDecodeResult>>,
    live_preview_rx: Option<mpsc::Receiver<LivePreviewResult>>,
    live_preview_active: bool,
    live_preview_capturing: bool,
    live_preview_captured_at: Option<String>,
    live_preview_error: Option<String>,
    proof_image_zoom: bool,
    save_pick_rx: Option<mpsc::Receiver<SavePickResult>>,
    save_write_rx: Option<mpsc::Receiver<Result<PathBuf, String>>>,
    proof_save_feedback_until: Option<Instant>,
    proof_save_error: Option<String>,
    test_save_path: Option<PathBuf>,
    cached_proof_count: usize,
    cached_proof_btn_label: String,
    compact_tab: CompactTab,
    start_minimized: bool,
    minimized_applied: bool,
    quit_requested: bool,
    headless: bool,
    headless_tray_active: bool,
    hide_to_tray_supported: bool,
    show_window_controls: bool,
    window_backend: WindowBackend,
    test_snapshot: Option<GuiSnapshot>,
    diag_rx: Option<mpsc::Receiver<Result<PathBuf, String>>>,
    import_pick_rx: Option<mpsc::Receiver<Option<Result<DotenvImportResult, PrefsError>>>>,
    diag_pick_rx: Option<mpsc::Receiver<(absen_core::Config, Option<PathBuf>)>>,
    external_show_flag: Option<Arc<AtomicBool>>,
    release_instance_lock: Option<Arc<dyn Fn() + Send + Sync>>,
    #[cfg(target_os = "windows")]
    native_hwnd: isize,
    #[cfg(target_os = "windows")]
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

    fn effective_hide_to_tray_supported(&self) -> bool {
        if self.headless {
            return self.hide_to_tray_supported;
        }
        self.tray
            .as_ref()
            .is_some_and(|t| t.supports_hide_to_tray(self.window_backend))
    }

    fn can_hide_to_tray(&self) -> bool {
        if self.headless {
            self.hide_to_tray_supported && self.headless_tray_active
        } else {
            self.effective_hide_to_tray_supported()
        }
    }

    fn handle_window_close(&mut self, ctx: &egui::Context, from_viewport_close: bool) {
        if !self.quit_requested && self.prefs.minimize_to_tray {
            self.refresh_tray_host_status();
            if self.can_hide_to_tray() {
                self.controller.set_window_visible(false);
                if from_viewport_close {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                self.notifier.notify_minimized_to_tray();
                return;
            }
        }
        if !from_viewport_close {
            self.perform_quit(ctx);
        }
    }

    fn handle_titlebar_action(&mut self, ctx: &egui::Context, action: TitlebarAction) {
        match action {
            TitlebarAction::StartDrag => {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            TitlebarAction::Minimize => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            TitlebarAction::ToggleMaximize => {
                let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
            }
            TitlebarAction::Close => {
                self.handle_window_close(ctx, false);
            }
        }
    }

    fn refresh_tray_host_status(&self) {
        if !self.headless {
            if let Some(tray) = &self.tray {
                tray.refresh_host_status(self.window_backend);
            }
        }
    }

    fn open_settings(&mut self) {
        self.refresh_tray_host_status();
        self.close_proof_modal();
        self.settings.theme = self.prefs.theme;
        self.settings.light_preset = self.prefs.light_preset;
        self.settings.dark_preset = self.prefs.dark_preset;
        self.settings.ui_zoom_pct = prefs::clamp_ui_zoom_pct(self.prefs.ui_zoom_pct);
        self.settings.notifications_enabled = self.prefs.notifications_enabled;
        self.settings.notification_sound = self.prefs.notification_sound;
        self.settings.notification_volume = self.prefs.normalized_notification_volume();
        self.settings.open = true;
    }

    fn clear_proof_image_state(&mut self) {
        self.active_proof_texture = None;
        self.proof_texture_loading = None;
        self.img_decode_rx = None;
        self.live_preview_rx = None;
        self.live_preview_active = false;
        self.live_preview_capturing = false;
        self.live_preview_captured_at = None;
        self.live_preview_error = None;
        self.proof_image_zoom = false;
        self.proof_save_feedback_until = None;
        self.proof_save_error = None;
    }

    fn open_proof_modal(&mut self, key: String) {
        self.settings.open = false;
        self.proof_modal_open = true;
        self.proof_modal_key = Some(key);
        self.proof_sample_mode = false;
        self.proof_copy_feedback_until = None;
        self.proof_tech_expanded = false;
        self.clear_proof_image_state();
    }

    fn open_proofs_from_header(&mut self, snapshot: &GuiSnapshot) {
        self.settings.open = false;
        self.proof_modal_open = true;
        self.proof_sample_mode = false;
        self.proof_copy_feedback_until = None;
        self.proof_tech_expanded = false;
        self.clear_proof_image_state();
        self.proof_modal_key = snapshot.proofs.first().map(|p| {
            if !p.proof_id.is_empty() {
                p.proof_id.clone()
            } else {
                p.sig.clone()
            }
        });
    }

    fn close_proof_modal(&mut self) {
        self.proof_modal_open = false;
        self.proof_modal_key = None;
        self.proof_sample_mode = false;
        self.proof_copy_feedback_until = None;
        self.proof_tech_expanded = false;
        self.clear_proof_image_state();
    }

    fn close_settings(&mut self, ctx: &egui::Context) {
        self.settings.open = false;
        self.settings.theme = self.prefs.theme;
        self.settings.light_preset = self.prefs.light_preset;
        self.settings.dark_preset = self.prefs.dark_preset;
        self.settings.ui_zoom_pct = prefs::clamp_ui_zoom_pct(self.prefs.ui_zoom_pct);
        self.settings.notifications_enabled = self.prefs.notifications_enabled;
        self.settings.notification_sound = self.prefs.notification_sound;
        self.settings.notification_volume = self.prefs.normalized_notification_volume();
        ctx.set_zoom_factor(self.prefs.normalized_ui_zoom());
        let palette = apply_appearance(ctx, self.prefs_appearance());
        self.notifier.set_palette(*palette);
        self.notifier
            .set_sound_enabled(self.prefs.notification_sound);
        self.notifier
            .set_sound_volume(self.prefs.normalized_notification_volume());
        self.controller.set_notifier(self.notifier.clone());
    }

    fn toggle_theme(&mut self, ctx: &egui::Context) {
        let is_dark = effective_dark_mode(ctx, self.prefs.theme);
        self.prefs.theme = if is_dark {
            ThemePreference::Light
        } else {
            ThemePreference::Dark
        };
        self.settings.theme = self.prefs.theme;
        let palette = apply_appearance(ctx, self.prefs_appearance());
        self.notifier.set_palette(*palette);
        self.controller.set_notifier(self.notifier.clone());
        self.persist_prefs();
    }

    fn handle_test_notification(&mut self, ctx: &egui::Context) {
        let system_dark = ctx
            .input(|i| i.raw.system_theme)
            .map(|t| t == egui::Theme::Dark)
            .unwrap_or(false);
        let palette = *ThemeTokens::resolve_appearance(self.settings.appearance(), system_dark);
        let volume = prefs::clamp_notification_volume(self.settings.notification_volume);
        if self.notifier.is_recorded() {
            self.notifier.set_palette(palette);
            self.notifier
                .set_sound_enabled(self.settings.notification_sound);
            self.notifier.set_sound_volume(volume);
            self.notifier.notify_test();
        } else if !self.headless {
            let mut live = Notifier::new(true);
            live.set_sound_enabled(self.settings.notification_sound);
            live.set_sound_volume(volume);
            live.set_palette(palette);
            #[cfg(target_os = "windows")]
            live.set_wake_hwnd(self.win32_msg_hwnd);
            if let Some(cb) = self.notifier.wake_callback() {
                live.set_wake_callback(cb);
            }
            live.notify_test();
        }
        #[cfg(target_os = "windows")]
        let status_msg = "Notifikasi uji ditampilkan di pojok kanan bawah.";
        #[cfg(target_os = "macos")]
        let status_msg = "Notifikasi uji ditampilkan di pojok kanan atas.";
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let status_msg = "Notifikasi uji dikirim ke sistem.";
        self.settings.status_message = Some(status_msg.to_string());
        self.settings.status_is_error = false;
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

    fn consume_zoom_key(input: &mut egui::InputState, key: egui::Key) -> bool {
        let matches_non_alt = input.events.iter().any(|ev| {
            matches!(
                ev,
                egui::Event::Key {
                    key: ev_key,
                    modifiers,
                    pressed: true,
                    ..
                } if *ev_key == key
                    && modifiers.matches_logically(egui::Modifiers::COMMAND)
                    && !modifiers.alt
            )
        });
        if matches_non_alt {
            input.consume_key(egui::Modifiers::COMMAND, key)
        } else {
            false
        }
    }

    fn apply_keyboard_zoom(&mut self, ctx: &egui::Context, next_zoom: u16) {
        let clamped = prefs::clamp_ui_zoom_pct(next_zoom);
        let changed = self.prefs.ui_zoom_pct != clamped || self.settings.ui_zoom_pct != clamped;
        self.prefs.ui_zoom_pct = clamped;
        self.settings.ui_zoom_pct = clamped;
        ctx.set_zoom_factor(prefs::zoom_pct_to_factor(clamped));
        if changed {
            self.persist_prefs();
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context, snapshot_active: bool) {
        if self.proof_modal_open
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.close_proof_modal();
        } else if self.settings.open
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.close_settings(ctx);
        }

        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma)) {
            self.open_settings();
        }

        const KEYBOARD_ZOOM_STEP: u16 = 10;
        let base_zoom = if self.settings.open {
            self.settings.ui_zoom_pct
        } else {
            self.prefs.ui_zoom_pct
        };
        if ctx.input_mut(|i| Self::consume_zoom_key(i, egui::Key::Num0)) {
            self.apply_keyboard_zoom(ctx, prefs::DEFAULT_UI_ZOOM_PCT);
        } else if ctx.input_mut(|i| {
            Self::consume_zoom_key(i, egui::Key::Plus)
                || Self::consume_zoom_key(i, egui::Key::Equals)
        }) {
            self.apply_keyboard_zoom(ctx, base_zoom.saturating_add(KEYBOARD_ZOOM_STEP));
        } else if ctx.input_mut(|i| Self::consume_zoom_key(i, egui::Key::Minus)) {
            self.apply_keyboard_zoom(ctx, base_zoom.saturating_sub(KEYBOARD_ZOOM_STEP));
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
        if open {
            next.ui_zoom_pct = self.settings.ui_zoom_pct;
            next.notification_volume = self.settings.notification_volume;
        }

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

        if let Some(rx) = &self.live_preview_rx {
            match rx.try_recv() {
                Ok(Ok((png_bytes, source, captured_at))) => {
                    self.live_preview_rx = None;
                    self.live_preview_capturing = false;
                    self.live_preview_captured_at = Some(captured_at);
                    self.live_preview_error = None;
                    if self.proof_modal_open && self.live_preview_active {
                        let key = "live-preview".to_string();
                        self.proof_texture_loading = Some((key.clone(), 0));
                        self.img_decode_rx = spawn_png_decode_bytes(
                            ctx,
                            key,
                            0,
                            source.badge_label().to_string(),
                            Arc::new(png_bytes),
                        );
                    }
                }
                Ok(Err(err)) => {
                    self.live_preview_rx = None;
                    self.live_preview_capturing = false;
                    self.live_preview_error = Some(err);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.live_preview_rx = None;
                    self.live_preview_capturing = false;
                    self.live_preview_error =
                        Some("Proses render pratinjau berhenti sebelum selesai.".to_string());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }

        if let Some(rx) = &self.img_decode_rx {
            match rx.try_recv() {
                Ok(decoded) => {
                    self.img_decode_rx = None;
                    let still_wanted = self.proof_modal_open
                        && self.proof_texture_loading.as_ref()
                            == Some(&(decoded.key.clone(), decoded.revision));
                    if still_wanted {
                        match decoded.image {
                            Ok((color_img, size)) => {
                                let texture = ctx.load_texture(
                                    &decoded.key,
                                    color_img,
                                    egui::TextureOptions::LINEAR,
                                );
                                self.active_proof_texture = Some(LoadedProofTexture {
                                    key: decoded.key,
                                    revision: decoded.revision,
                                    texture,
                                    size,
                                    source_label: decoded.source_label,
                                    png_bytes: decoded.png_bytes,
                                });
                            }
                            Err(err) => {
                                if self.live_preview_active {
                                    self.live_preview_error = Some(err);
                                }
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.img_decode_rx = None;
                    self.proof_texture_loading = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }

        if let Some(rx) = &self.save_pick_rx {
            match rx.try_recv() {
                Ok((bytes, Some(dest_path))) => {
                    self.save_pick_rx = None;
                    self.spawn_proof_image_write(ctx, bytes, dest_path);
                }
                Ok((_, None)) | Err(mpsc::TryRecvError::Disconnected) => {
                    self.save_pick_rx = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }

        if let Some(rx) = &self.save_write_rx {
            match rx.try_recv() {
                Ok(Ok(_path)) => {
                    self.save_write_rx = None;
                    let duration = Duration::from_secs(3);
                    self.proof_save_feedback_until = Some(Instant::now() + duration);
                    self.proof_save_error = None;
                    ctx.request_repaint_after(duration);
                }
                Ok(Err(err)) => {
                    self.save_write_rx = None;
                    self.proof_save_error = Some(err);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.save_write_rx = None;
                    self.proof_save_error =
                        Some("Proses penyimpanan gambar berhenti sebelum selesai.".to_string());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }

    fn spawn_proof_image_write(
        &mut self,
        ctx: &egui::Context,
        bytes: Arc<Vec<u8>>,
        dest_path: PathBuf,
    ) {
        let (tx, rx) = mpsc::channel();
        let wake_ctx = ctx.clone();
        match std::thread::Builder::new()
            .name("polban-shot-save-write".to_string())
            .spawn(move || {
                let res = write_png_atomic(&dest_path, &bytes).map(|()| dest_path);
                let _ = tx.send(res);
                wake_ctx.request_repaint();
            }) {
            Ok(_) => {
                self.save_write_rx = Some(rx);
                self.proof_save_error = None;
            }
            Err(err) => {
                self.proof_save_error = Some(format!("Gagal memulai thread penyimpanan: {err}"));
            }
        }
    }

    fn handle_save_proof_image(&mut self, ctx: &egui::Context, default_filename: String) {
        if self.proof_sample_mode || self.save_pick_rx.is_some() || self.save_write_rx.is_some() {
            return;
        }
        let Some(tex) = &self.active_proof_texture else {
            return;
        };
        let bytes = Arc::clone(&tex.png_bytes);
        self.proof_save_error = None;

        if let Some(dest) = self.test_save_path.clone() {
            self.spawn_proof_image_write(ctx, bytes, dest);
            return;
        }
        if self.headless {
            return;
        }

        let (tx, rx) = mpsc::channel();
        let wake_ctx = ctx.clone();
        match std::thread::Builder::new()
            .name("polban-shot-save-picker".to_string())
            .spawn(move || {
                let path = prefs::pick_proof_image_save_path(&default_filename);
                let _ = tx.send((bytes, path));
                wake_ctx.request_repaint();
            }) {
            Ok(_) => self.save_pick_rx = Some(rx),
            Err(err) => {
                self.proof_save_error =
                    Some(format!("Gagal membuka dialog penyimpanan gambar: {err}"));
            }
        }
    }

    fn ensure_proof_texture_loaded(&mut self, ctx: &egui::Context, snapshot: &GuiSnapshot) {
        if !self.proof_modal_open {
            if self.active_proof_texture.is_some() {
                self.active_proof_texture = None;
            }
            return;
        }

        if !snapshot.proofs.is_empty() {
            let active = self
                .proof_modal_key
                .as_deref()
                .and_then(|k| snapshot.proof_by_key(k))
                .or_else(|| snapshot.proofs.first());
            let Some(proof) = active else {
                return;
            };
            let target_key = format!("proof:{}", proof.proof_id);
            let rev = snapshot.screenshot_revision;
            if self
                .active_proof_texture
                .as_ref()
                .is_some_and(|t| t.key == target_key && t.revision == rev)
            {
                return;
            }
            if self.proof_texture_loading.as_ref() == Some(&(target_key.clone(), rev)) {
                return;
            }
            self.active_proof_texture = None;
            if snapshot.capturing_proof_id.as_deref() == Some(proof.proof_id.as_str()) {
                return;
            }
            let png_path = self.controller.screenshot_path_for(&proof.proof_id);
            if png_path.is_file() {
                self.proof_texture_loading = Some((target_key.clone(), rev));
                self.img_decode_rx = spawn_png_decode_from_path(ctx, target_key, rev, png_path);
            } else {
                self.proof_texture_loading = None;
            }
        } else if self.proof_sample_mode {
            let target_key = "sample".to_string();
            if self
                .active_proof_texture
                .as_ref()
                .is_some_and(|t| t.key == target_key)
            {
                return;
            }
            if self.proof_texture_loading.as_ref() == Some(&(target_key.clone(), 0)) {
                return;
            }
            self.active_proof_texture = None;
            self.proof_texture_loading = Some((target_key.clone(), 0));
            let bytes = Arc::new(render_sample_attendance_png().to_vec());
            self.img_decode_rx = spawn_png_decode_bytes(
                ctx,
                target_key,
                0,
                ScreenshotSource::NativeFallback.badge_label().to_string(),
                bytes,
            );
        } else if !self.live_preview_active && self.active_proof_texture.is_some() {
            self.active_proof_texture = None;
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
        let palette = apply_appearance(ctx, self.prefs_appearance());
        ctx.set_zoom_factor(self.prefs.normalized_ui_zoom());
        self.notifier.set_enabled(self.prefs.notifications_enabled);
        self.notifier
            .set_sound_enabled(self.prefs.notification_sound);
        self.notifier
            .set_sound_volume(self.prefs.normalized_notification_volume());
        self.notifier.set_palette(*palette);
        #[cfg(target_os = "windows")]
        self.notifier.set_wake_hwnd(self.win32_msg_hwnd);
        self.controller.set_notifier(self.notifier.clone());
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
            if self.can_hide_to_tray() {
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

        if ctx.input(|i| i.viewport().close_requested()) {
            self.handle_window_close(ctx, true);
        }

        let palette = apply_appearance(ctx, self.active_appearance());
        let is_dark = palette.dark_mode;
        let snapshot = self.current_snapshot();
        let proof_count = snapshot.proofs.len();
        if proof_count != self.cached_proof_count {
            self.cached_proof_count = proof_count;
            self.cached_proof_btn_label = if proof_count == 0 {
                "Bukti".to_string()
            } else {
                format!("Bukti ({proof_count})")
            };
        }
        self.handle_shortcuts(ctx, snapshot.status.is_active());
        if self.quit_requested {
            return;
        }

        self.sync_tray_status();

        if self.show_window_controls {
            let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
            let minimize_to_tray_active =
                !self.quit_requested && self.prefs.minimize_to_tray && self.can_hide_to_tray();
            if let Some(action) =
                titlebar::show(ctx, palette, is_maximized, minimize_to_tray_active)
            {
                self.handle_titlebar_action(ctx, action);
                if self.quit_requested {
                    return;
                }
            }
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
                        &self.cached_proof_btn_label,
                    ) {
                        match action {
                            HeaderAction::StartMonitoring => self.handle_start_monitoring(ctx),
                            HeaderAction::StopMonitoring => self.controller.stop(),
                            HeaderAction::ScanNow => {
                                let _ = self.controller.scan_now();
                            }
                            HeaderAction::OpenProofs => self.open_proofs_from_header(&snapshot),
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
                    let mut activity_action = None;
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
                            activity_action = activity::show(ui, &snapshot, palette, false, true);
                        });
                    } else {
                        let compact = width < ThemeTokens::BREAKPOINT_MEDIUM;
                        egui::ScrollArea::vertical()
                            .id_salt("stacked_main_scroll")
                            .show(ui, |ui| {
                                jadwal_action =
                                    jadwal::show(ui, &snapshot, palette, compact, false);
                                ui.add_space(ThemeTokens::SPACE_LG);
                                activity_action =
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
                            JadwalAction::OpenProof(key) => {
                                self.open_proof_modal(key);
                            }
                        }
                    }

                    if let Some(ActivityAction::OpenProof(key)) = activity_action {
                        self.open_proof_modal(key);
                    }
                });
            });

        let copy_feedback_active = if let Some(until) = self.proof_copy_feedback_until {
            let now = Instant::now();
            if now >= until {
                self.proof_copy_feedback_until = None;
                false
            } else {
                ctx.request_repaint_after(until.saturating_duration_since(now));
                true
            }
        } else {
            false
        };

        let save_feedback_active = if let Some(until) = self.proof_save_feedback_until {
            let now = Instant::now();
            if now >= until {
                self.proof_save_feedback_until = None;
                false
            } else {
                ctx.request_repaint_after(until.saturating_duration_since(now));
                true
            }
        } else {
            false
        };

        if self.proof_modal_open {
            if !snapshot.proofs.is_empty() {
                self.proof_sample_mode = false;
                self.live_preview_active = false;
                let valid_key = self
                    .proof_modal_key
                    .as_deref()
                    .is_some_and(|k| snapshot.proof_by_key(k).is_some());
                if !valid_key {
                    self.proof_modal_key = snapshot.proofs.first().map(|p| {
                        if !p.proof_id.is_empty() {
                            p.proof_id.clone()
                        } else {
                            p.sig.clone()
                        }
                    });
                }
            } else {
                self.proof_modal_key = None;
            }

            self.ensure_proof_texture_loaded(ctx, &snapshot);

            let active_proof_for_shot = if !snapshot.proofs.is_empty() {
                self.proof_modal_key
                    .as_deref()
                    .and_then(|k| snapshot.proof_by_key(k))
                    .or_else(|| snapshot.proofs.first())
            } else {
                None
            };
            let is_capturing_shot = if let Some(p) = active_proof_for_shot {
                snapshot.capturing_proof_id.as_deref() == Some(p.proof_id.as_str())
            } else {
                self.live_preview_capturing
            };
            let shot_view = ProofScreenshotView {
                texture: self.active_proof_texture.as_ref().map(|t| &t.texture),
                pixel_size: self.active_proof_texture.as_ref().map(|t| t.size),
                source_label: self
                    .active_proof_texture
                    .as_ref()
                    .map(|t| t.source_label.as_str()),
                is_capturing: is_capturing_shot,
                zoomed: self.proof_image_zoom,
                live_preview_active: self.live_preview_active,
                live_captured_at: self.live_preview_captured_at.as_deref(),
                live_error: self.live_preview_error.as_deref(),
                save_in_progress: self.save_pick_rx.is_some() || self.save_write_rx.is_some(),
                save_feedback_active,
                save_error: self.proof_save_error.as_deref(),
                can_save_image: !self.proof_sample_mode && self.active_proof_texture.is_some(),
            };

            if let Some(action) = proof::show(
                ctx,
                &snapshot,
                self.proof_modal_open,
                self.proof_modal_key.as_deref(),
                self.proof_sample_mode,
                copy_feedback_active,
                self.proof_tech_expanded,
                &shot_view,
                palette,
            ) {
                match action {
                    ProofAction::Close => self.close_proof_modal(),
                    ProofAction::SelectIndex(idx) => {
                        if let Some(target) = snapshot.proofs.get(idx) {
                            let next_key = if !target.proof_id.is_empty() {
                                target.proof_id.clone()
                            } else {
                                target.sig.clone()
                            };
                            if self.proof_modal_key.as_deref() != Some(next_key.as_str()) {
                                self.proof_modal_key = Some(next_key);
                                self.proof_copy_feedback_until = None;
                                self.clear_proof_image_state();
                            }
                        }
                    }
                    ProofAction::CopyReceipt => {
                        if !self.proof_sample_mode {
                            if let Some(p) = self
                                .proof_modal_key
                                .as_deref()
                                .and_then(|k| snapshot.proof_by_key(k))
                            {
                                ctx.copy_text(p.receipt_text.clone());
                                let duration = Duration::from_secs(2);
                                self.proof_copy_feedback_until = Some(Instant::now() + duration);
                                ctx.request_repaint_after(duration);
                            }
                        }
                    }
                    ProofAction::SaveImage => {
                        let default_name = if let Some(p) = active_proof_for_shot {
                            default_screenshot_filename(Some(&p.nama_mk), &p.date)
                        } else {
                            default_screenshot_filename(None, &snapshot.today_date)
                        };
                        self.handle_save_proof_image(ctx, default_name);
                    }
                    ProofAction::CaptureProofScreenshot(proof_id) => {
                        self.active_proof_texture = None;
                        self.proof_texture_loading = None;
                        self.controller.request_proof_screenshot(proof_id);
                    }
                    ProofAction::CaptureLivePortalPreview => {
                        self.proof_sample_mode = false;
                        self.live_preview_active = true;
                        self.live_preview_error = None;
                        self.active_proof_texture = None;
                        self.proof_texture_loading = None;
                        self.proof_save_error = None;
                        if let Some(rx) = self.controller.request_live_portal_preview() {
                            self.live_preview_capturing = true;
                            self.live_preview_rx = Some(rx);
                        } else {
                            self.live_preview_capturing = false;
                            self.live_preview_error = Some(
                                "Snapshot HTML portal belum tersedia dari siklus pemantauan."
                                    .to_string(),
                            );
                        }
                    }
                    ProofAction::ExitLivePortalPreview => {
                        self.clear_proof_image_state();
                    }
                    ProofAction::ToggleImageZoom => {
                        self.proof_image_zoom = !self.proof_image_zoom;
                    }
                    ProofAction::Reverify => {
                        let _ = self.controller.scan_now();
                    }
                    ProofAction::OpenPortal(url) => {
                        if !self.proof_sample_mode && !self.headless && is_safe_http_url(&url) {
                            std::thread::spawn(move || {
                                let _ = open_url_in_browser(&url);
                            });
                        }
                    }
                    ProofAction::ToggleTechnical => {
                        self.proof_tech_expanded = !self.proof_tech_expanded;
                    }
                    ProofAction::EnterSamplePreview => {
                        self.clear_proof_image_state();
                        self.proof_sample_mode = true;
                        self.proof_tech_expanded = false;
                        self.proof_copy_feedback_until = None;
                    }
                    ProofAction::ExitSamplePreview => {
                        self.clear_proof_image_state();
                        self.proof_sample_mode = false;
                        self.proof_tech_expanded = false;
                        self.proof_copy_feedback_until = None;
                    }
                }
            }
        }

        let host = settings::SettingsHost {
            backend: self.creds.backend(),
            hide_to_tray_supported: self.effective_hide_to_tray_supported(),
        };
        if let Some(action) = settings::show(
            ctx,
            &mut self.settings,
            host,
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
                    let palette = apply_appearance(ctx, appearance);
                    self.notifier.set_palette(*palette);
                    self.controller.set_notifier(self.notifier.clone());
                }
                SettingsAction::PreviewZoom(zoom_pct) => {
                    ctx.set_zoom_factor(prefs::zoom_pct_to_factor(zoom_pct));
                }
                SettingsAction::TestNotification => {
                    self.handle_test_notification(ctx);
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
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);

        let mut prefs = prefs::load().unwrap_or_default();
        cc.egui_ctx.set_zoom_factor(prefs.normalized_ui_zoom());
        let initial_palette = apply_appearance(
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
        let mut notifier = Notifier::new(prefs.notifications_enabled);
        notifier.set_sound_enabled(prefs.notification_sound);
        notifier.set_sound_volume(prefs.normalized_notification_volume());
        notifier.set_palette(*initial_palette);
        let window_backend = detect_window_backend(cc);
        let tray = TrayManager::new_for_backend(window_backend).ok();
        let hide_to_tray_supported = tray
            .as_ref()
            .is_some_and(|t| t.supports_hide_to_tray(window_backend));

        let mut controller = EngineController::new();
        controller.set_notifier(notifier.clone());
        let shot_wake_ctx = cc.egui_ctx.clone();
        controller.set_wake_ui(Some(Arc::new(move || {
            shot_wake_ctx.request_repaint();
        })));
        let state_file = prefs
            .state_file
            .clone()
            .unwrap_or_else(prefs::default_state_file_path);
        controller.preload_proofs(&state_file);
        let initial_proof_count = controller.snapshot().proofs.len();
        let initial_proof_btn_label = if initial_proof_count == 0 {
            "Bukti".to_string()
        } else {
            format!("Bukti ({initial_proof_count})")
        };
        if start_minimized && hide_to_tray_supported {
            controller.set_window_visible(false);
        }

        #[cfg(target_os = "windows")]
        let native_hwnd = win32::extract_hwnd(cc).unwrap_or(0);

        let core = Rc::new(RefCell::new(AppCore {
            prefs,
            creds,
            controller,
            tray,
            notifier,
            autostart,
            settings,
            updater: UpdateManager::new(),
            proof_modal_open: false,
            proof_modal_key: None,
            proof_sample_mode: false,
            proof_copy_feedback_until: None,
            proof_tech_expanded: false,
            active_proof_texture: None,
            proof_texture_loading: None,
            img_decode_rx: None,
            live_preview_rx: None,
            live_preview_active: false,
            live_preview_capturing: false,
            live_preview_captured_at: None,
            live_preview_error: None,
            proof_image_zoom: false,
            save_pick_rx: None,
            save_write_rx: None,
            proof_save_feedback_until: None,
            proof_save_error: None,
            test_save_path: None,
            cached_proof_count: initial_proof_count,
            cached_proof_btn_label: initial_proof_btn_label,
            compact_tab: CompactTab::Jadwal,
            start_minimized,
            minimized_applied: false,
            quit_requested: false,
            headless: false,
            headless_tray_active: false,
            hide_to_tray_supported,
            show_window_controls: needs_window_controls(),
            window_backend,
            test_snapshot: None,
            diag_rx: None,
            import_pick_rx: None,
            diag_pick_rx: None,
            external_show_flag: None,
            release_instance_lock: None,
            #[cfg(target_os = "windows")]
            native_hwnd,
            #[cfg(target_os = "windows")]
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

        {
            let mut c = core.borrow_mut();
            #[cfg(target_os = "windows")]
            {
                c.win32_msg_hwnd = win32_msg_hwnd;
                c.notifier.set_wake_hwnd(win32_msg_hwnd);
                let synced_notifier = c.notifier.clone();
                c.controller.set_notifier(synced_notifier);
            }

            #[cfg(target_os = "macos")]
            {
                let toast_ctx = cc.egui_ctx.clone();
                c.notifier.set_wake_callback(Arc::new(move || {
                    toast_ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    toast_ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    toast_ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    toast_ctx.request_repaint();
                }));
                let synced_notifier = c.notifier.clone();
                c.controller.set_notifier(synced_notifier);
            }

            let waker_ctx = cc.egui_ctx.clone();
            let host_wake_ctx = cc.egui_ctx.clone();
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
                t.set_host_change_waker(Arc::new(move || {
                    host_wake_ctx.request_repaint();
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
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        ctx.set_zoom_factor(prefs.normalized_ui_zoom());
        let initial_palette = apply_appearance(
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
        let mut notifier = Notifier::new(prefs.notifications_enabled);
        notifier.set_sound_enabled(prefs.notification_sound);
        notifier.set_sound_volume(prefs.normalized_notification_volume());
        notifier.set_palette(*initial_palette);
        let autostart = AutostartManager::new("Polban Presensi");
        let mut controller = EngineController::new();
        controller.set_notifier(notifier.clone());
        if let Some(sf) = &prefs.state_file {
            controller.set_screenshot_state_file(sf);
        }

        let core = Rc::new(RefCell::new(AppCore {
            prefs,
            creds,
            controller,
            tray: None,
            notifier,
            autostart,
            settings,
            updater: UpdateManager::new(),
            proof_modal_open: false,
            proof_modal_key: None,
            proof_sample_mode: false,
            proof_copy_feedback_until: None,
            proof_tech_expanded: false,
            active_proof_texture: None,
            proof_texture_loading: None,
            img_decode_rx: None,
            live_preview_rx: None,
            live_preview_active: false,
            live_preview_capturing: false,
            live_preview_captured_at: None,
            live_preview_error: None,
            proof_image_zoom: false,
            save_pick_rx: None,
            save_write_rx: None,
            proof_save_feedback_until: None,
            proof_save_error: None,
            test_save_path: None,
            cached_proof_count: 0,
            cached_proof_btn_label: "Bukti".to_string(),
            compact_tab: CompactTab::Jadwal,
            start_minimized: false,
            minimized_applied: true,
            quit_requested: false,
            headless: true,
            headless_tray_active: false,
            hide_to_tray_supported: true,
            show_window_controls: false,
            window_backend: WindowBackend::Other,
            test_snapshot: None,
            diag_rx: None,
            import_pick_rx: None,
            diag_pick_rx: None,
            external_show_flag: None,
            release_instance_lock: None,
            #[cfg(target_os = "windows")]
            native_hwnd: 0,
            #[cfg(target_os = "windows")]
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

    /// Enable simulated tray availability for headless close-to-tray tests.
    pub fn set_headless_tray_active_for_test(&mut self, active: bool) {
        self.core.borrow_mut().headless_tray_active = active;
    }

    /// Override whether hiding the window to the tray is supported (e.g. simulate Wayland).
    pub fn set_hide_to_tray_supported_for_test(&mut self, supported: bool) {
        self.core.borrow_mut().hide_to_tray_supported = supported;
    }

    /// Enable or disable the in-app Linux window control bar in headless tests.
    pub fn set_show_window_controls_for_test(&mut self, show: bool) {
        self.core.borrow_mut().show_window_controls = show;
    }

    /// Returns whether the in-app Linux window control bar is active.
    pub fn show_window_controls(&self) -> bool {
        self.core.borrow().show_window_controls
    }

    /// Override the `Notifier` (for example with `Notifier::new_recorded`) in tests.
    pub fn set_notifier_for_test(&mut self, notifier: Notifier) {
        let mut core = self.core.borrow_mut();
        core.notifier = notifier.clone();
        core.controller.set_notifier(notifier);
    }

    /// Read the active notifier sound volume (`0..=100`) in tests.
    pub fn notifier_sound_volume_for_test(&self) -> u8 {
        self.core.borrow().notifier.sound_volume()
    }

    /// Read the active Settings form UI zoom percentage (`70..=150`) in tests.
    pub fn settings_ui_zoom_pct_for_test(&self) -> u16 {
        self.core.borrow().settings.ui_zoom_pct
    }

    /// Read the active Settings form notification volume (`0..=100`) in tests.
    pub fn settings_notification_volume_for_test(&self) -> u8 {
        self.core.borrow().settings.notification_volume
    }

    /// Inject or clear a deterministic `GuiSnapshot` for headless UI testing.
    pub fn set_test_snapshot(&mut self, snapshot: Option<GuiSnapshot>) {
        self.core.borrow_mut().test_snapshot = snapshot;
    }

    pub fn is_settings_open(&self) -> bool {
        self.core.borrow().settings.open
    }

    pub fn is_proof_modal_open(&self) -> bool {
        self.core.borrow().proof_modal_open
    }

    pub fn is_proof_sample_mode(&self) -> bool {
        self.core.borrow().proof_sample_mode
    }

    pub fn proof_modal_key(&self) -> Option<String> {
        self.core.borrow().proof_modal_key.clone()
    }

    pub fn is_proof_tech_expanded(&self) -> bool {
        self.core.borrow().proof_tech_expanded
    }

    /// Configure a deterministic destination PNG path for headless save-image tests.
    pub fn set_test_save_path(&mut self, path: Option<PathBuf>) {
        self.core.borrow_mut().test_save_path = path;
    }

    /// Returns whether a GPU screenshot texture is currently loaded for the open proof modal.
    pub fn has_loaded_proof_texture(&self) -> bool {
        self.core.borrow().active_proof_texture.is_some()
    }

    /// Returns whether the screenshot preview inside the proof modal is zoomed in.
    pub fn is_proof_image_zoomed(&self) -> bool {
        self.core.borrow().proof_image_zoom
    }

    /// Returns whether the live portal preview mode is active inside the proof modal.
    pub fn is_live_preview_active(&self) -> bool {
        self.core.borrow().live_preview_active
    }

    /// Access the underlying `EngineController` in integration tests.
    pub fn controller(&self) -> Ref<'_, EngineController> {
        Ref::map(self.core.borrow(), |c| &c.controller)
    }

    /// Mutably access the underlying `EngineController` in integration tests.
    pub fn controller_mut(&mut self) -> std::cell::RefMut<'_, EngineController> {
        std::cell::RefMut::map(self.core.borrow_mut(), |c| &mut c.controller)
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

    #[test]
    fn close_to_tray_sends_notification_only_when_minimized_to_tray() {
        let make_close_input = || {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(960.0, 640.0),
                )),
                ..Default::default()
            };
            raw.viewports.insert(
                egui::ViewportId::ROOT,
                egui::ViewportInfo {
                    events: vec![egui::ViewportEvent::Close],
                    ..Default::default()
                },
            );
            raw
        };

        // 1. minimize_to_tray == true -> hides window and sends background tray notification.
        let ctx1 = egui::Context::default();
        let mut app1 = DesktopApp::new_headless(
            &ctx1,
            AppPrefs {
                minimize_to_tray: true,
                notifications_enabled: true,
                ..AppPrefs::default()
            },
            Box::new(MemoryFallbackStore::new()),
        );
        let (notifier1, log1) = Notifier::new_recorded(true);
        app1.set_notifier_for_test(notifier1);
        app1.set_headless_tray_active_for_test(true);

        let _ = ctx1.run(make_close_input(), |c| app1.update_ui(c));
        assert!(!app1.core.borrow().controller.is_window_visible());
        let events1 = log1.lock().unwrap().clone();
        assert_eq!(events1.len(), 1);
        assert_eq!(events1[0].0, "Berjalan di Latar Belakang");
        assert!(events1[0].1.contains("system tray"));

        // 2. minimize_to_tray == false -> does NOT send close-to-tray notification.
        let ctx2 = egui::Context::default();
        let mut app2 = DesktopApp::new_headless(
            &ctx2,
            AppPrefs {
                minimize_to_tray: false,
                notifications_enabled: true,
                ..AppPrefs::default()
            },
            Box::new(MemoryFallbackStore::new()),
        );
        let (notifier2, log2) = Notifier::new_recorded(true);
        app2.set_notifier_for_test(notifier2);
        app2.set_headless_tray_active_for_test(true);

        let _ = ctx2.run(make_close_input(), |c| app2.update_ui(c));
        assert!(log2.lock().unwrap().is_empty());

        // 3. Quitting from tray sets quit_requested -> does NOT send close-to-tray notification.
        let ctx3 = egui::Context::default();
        let mut app3 = DesktopApp::new_headless(
            &ctx3,
            AppPrefs {
                minimize_to_tray: true,
                notifications_enabled: true,
                ..AppPrefs::default()
            },
            Box::new(MemoryFallbackStore::new()),
        );
        let (notifier3, log3) = Notifier::new_recorded(true);
        app3.set_notifier_for_test(notifier3);
        app3.set_headless_tray_active_for_test(true);
        app3.core
            .borrow_mut()
            .execute_tray_command(TrayCommand::Quit, &ctx3);

        let _ = ctx3.run(make_close_input(), |c| app3.update_ui(c));
        assert!(log3.lock().unwrap().is_empty());
    }

    #[test]
    fn close_to_tray_falls_back_to_normal_close_when_hide_unsupported() {
        let make_close_input = || {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(960.0, 640.0),
                )),
                ..Default::default()
            };
            raw.viewports.insert(
                egui::ViewportId::ROOT,
                egui::ViewportInfo {
                    events: vec![egui::ViewportEvent::Close],
                    ..Default::default()
                },
            );
            raw
        };

        // Supported (e.g. Windows/macOS/X11): cancels close and hides window.
        let ctx_ok = egui::Context::default();
        let mut app_ok = DesktopApp::new_headless(
            &ctx_ok,
            AppPrefs {
                minimize_to_tray: true,
                notifications_enabled: true,
                ..AppPrefs::default()
            },
            Box::new(MemoryFallbackStore::new()),
        );
        let (notifier_ok, log_ok) = Notifier::new_recorded(true);
        app_ok.set_notifier_for_test(notifier_ok);
        app_ok.set_headless_tray_active_for_test(true);
        app_ok.set_hide_to_tray_supported_for_test(true);

        let out_ok = ctx_ok.run(make_close_input(), |c| app_ok.update_ui(c));
        let cmds_ok = &out_ok.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(cmds_ok
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::CancelClose)));
        assert!(cmds_ok
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Visible(false))));
        assert!(!app_ok.core.borrow().controller.is_window_visible());
        assert_eq!(log_ok.lock().unwrap().len(), 1);

        // Unsupported (e.g. Linux native Wayland): allows normal close without CancelClose/Visible(false).
        let ctx_wayland = egui::Context::default();
        let mut app_wayland = DesktopApp::new_headless(
            &ctx_wayland,
            AppPrefs {
                minimize_to_tray: true,
                notifications_enabled: true,
                ..AppPrefs::default()
            },
            Box::new(MemoryFallbackStore::new()),
        );
        let (notifier_wayland, log_wayland) = Notifier::new_recorded(true);
        app_wayland.set_notifier_for_test(notifier_wayland);
        app_wayland.set_headless_tray_active_for_test(true);
        app_wayland.set_hide_to_tray_supported_for_test(false);

        let out_wayland = ctx_wayland.run(make_close_input(), |c| app_wayland.update_ui(c));
        let cmds_wayland = &out_wayland.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(!cmds_wayland
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::CancelClose)));
        assert!(!cmds_wayland
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Visible(false))));
        assert!(app_wayland.core.borrow().controller.is_window_visible());
        assert!(log_wayland.lock().unwrap().is_empty());
    }

    #[test]
    fn start_minimized_falls_back_to_minimized_when_hide_unsupported() {
        let make_input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(960.0, 640.0),
            )),
            ..Default::default()
        };

        // Supported: hides to tray with Visible(false).
        let ctx_ok = egui::Context::default();
        let mut app_ok = DesktopApp::new_headless(
            &ctx_ok,
            AppPrefs::default(),
            Box::new(MemoryFallbackStore::new()),
        );
        app_ok.set_headless_tray_active_for_test(true);
        app_ok.set_hide_to_tray_supported_for_test(true);
        {
            let mut core = app_ok.core.borrow_mut();
            core.start_minimized = true;
            core.minimized_applied = false;
        }
        let out_ok = ctx_ok.run(make_input(), |c| app_ok.update_ui(c));
        let cmds_ok = &out_ok.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(cmds_ok
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Visible(false))));
        assert!(!app_ok.core.borrow().controller.is_window_visible());

        // Unsupported (Wayland): falls back to Minimized(true) instead of Visible(false).
        let ctx_wayland = egui::Context::default();
        let mut app_wayland = DesktopApp::new_headless(
            &ctx_wayland,
            AppPrefs::default(),
            Box::new(MemoryFallbackStore::new()),
        );
        app_wayland.set_headless_tray_active_for_test(true);
        app_wayland.set_hide_to_tray_supported_for_test(false);
        {
            let mut core = app_wayland.core.borrow_mut();
            core.start_minimized = true;
            core.minimized_applied = false;
        }
        let out_wayland = ctx_wayland.run(make_input(), |c| app_wayland.update_ui(c));
        let cmds_wayland = &out_wayland.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(cmds_wayland
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Minimized(true))));
        assert!(!cmds_wayland
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Visible(false))));
        assert!(app_wayland.core.borrow().controller.is_window_visible());
    }

    #[test]
    fn close_and_start_minimized_fall_back_when_tray_unavailable() {
        let make_close_input = || {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(960.0, 640.0),
                )),
                ..Default::default()
            };
            raw.viewports.insert(
                egui::ViewportId::ROOT,
                egui::ViewportInfo {
                    events: vec![egui::ViewportEvent::Close],
                    ..Default::default()
                },
            );
            raw
        };

        let ctx = egui::Context::default();
        let mut app = DesktopApp::new_headless(
            &ctx,
            AppPrefs {
                minimize_to_tray: true,
                notifications_enabled: true,
                ..AppPrefs::default()
            },
            Box::new(MemoryFallbackStore::new()),
        );
        let (notifier, log) = Notifier::new_recorded(true);
        app.set_notifier_for_test(notifier);
        app.set_headless_tray_active_for_test(false);
        app.set_hide_to_tray_supported_for_test(true);
        {
            let mut core = app.core.borrow_mut();
            core.start_minimized = true;
            core.minimized_applied = false;
        }

        let out = ctx.run(make_close_input(), |c| app.update_ui(c));
        let cmds = &out.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(cmds
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Minimized(true))));
        assert!(!cmds
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::CancelClose)));
        assert!(!cmds
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Visible(false))));
        assert!(app.core.borrow().controller.is_window_visible());
        assert!(log.lock().unwrap().is_empty());
    }

    #[test]
    fn titlebar_actions_handle_drag_minimize_maximize_and_close_paths() {
        let make_input = |maximized: bool| {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(960.0, 640.0),
                )),
                ..Default::default()
            };
            raw.viewports.insert(
                egui::ViewportId::ROOT,
                egui::ViewportInfo {
                    maximized: Some(maximized),
                    ..Default::default()
                },
            );
            raw
        };

        // 1. Minimize & StartDrag emit Minimized(true) and StartDrag.
        let ctx = egui::Context::default();
        let app = DesktopApp::new_headless(
            &ctx,
            AppPrefs::default(),
            Box::new(MemoryFallbackStore::new()),
        );
        let out_min = ctx.run(make_input(false), |c| {
            let mut core = app.core.borrow_mut();
            core.handle_titlebar_action(c, TitlebarAction::Minimize);
            core.handle_titlebar_action(c, TitlebarAction::StartDrag);
        });
        let cmds_min = &out_min.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(cmds_min
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Minimized(true))));
        assert!(cmds_min
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::StartDrag)));

        // 2. ToggleMaximize emits Maximized(true) when unmaximized, Maximized(false) when maximized.
        let out_max = ctx.run(make_input(false), |c| {
            app.core
                .borrow_mut()
                .handle_titlebar_action(c, TitlebarAction::ToggleMaximize);
        });
        assert!(out_max.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Maximized(true))));

        let out_restore = ctx.run(make_input(true), |c| {
            app.core
                .borrow_mut()
                .handle_titlebar_action(c, TitlebarAction::ToggleMaximize);
        });
        assert!(out_restore.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Maximized(false))));

        // 3. Close hides to tray + sends notification when minimize_to_tray & tray supported.
        let ctx_hide = egui::Context::default();
        let mut app_hide = DesktopApp::new_headless(
            &ctx_hide,
            AppPrefs {
                minimize_to_tray: true,
                notifications_enabled: true,
                ..AppPrefs::default()
            },
            Box::new(MemoryFallbackStore::new()),
        );
        let (notifier_hide, log_hide) = Notifier::new_recorded(true);
        app_hide.set_notifier_for_test(notifier_hide);
        app_hide.set_headless_tray_active_for_test(true);
        app_hide.set_hide_to_tray_supported_for_test(true);

        let out_hide = ctx_hide.run(make_input(false), |c| {
            app_hide
                .core
                .borrow_mut()
                .handle_titlebar_action(c, TitlebarAction::Close);
        });
        let cmds_hide = &out_hide.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(cmds_hide
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Visible(false))));
        assert!(!cmds_hide
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Close)));
        assert!(!app_hide.core.borrow().quit_requested);
        assert!(!app_hide.core.borrow().controller.is_window_visible());
        assert_eq!(log_hide.lock().unwrap().len(), 1);

        // 4. Close performs clean quit when minimize_to_tray is false or hide unsupported.
        for (min_to_tray, supported) in [(false, true), (true, false)] {
            let ctx_quit = egui::Context::default();
            let mut app_quit = DesktopApp::new_headless(
                &ctx_quit,
                AppPrefs {
                    minimize_to_tray: min_to_tray,
                    notifications_enabled: true,
                    ..AppPrefs::default()
                },
                Box::new(MemoryFallbackStore::new()),
            );
            let (notifier_quit, log_quit) = Notifier::new_recorded(true);
            app_quit.set_notifier_for_test(notifier_quit);
            app_quit.set_headless_tray_active_for_test(true);
            app_quit.set_hide_to_tray_supported_for_test(supported);

            let out_quit = ctx_quit.run(make_input(false), |c| {
                app_quit
                    .core
                    .borrow_mut()
                    .handle_titlebar_action(c, TitlebarAction::Close);
            });
            let cmds_quit = &out_quit.viewport_output[&egui::ViewportId::ROOT].commands;
            assert!(cmds_quit
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::Close)));
            assert!(!cmds_quit
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::Visible(false))));
            assert!(app_quit.core.borrow().quit_requested);
            assert!(log_quit.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn keyboard_zoom_shortcuts_step_clamp_sync_settings_and_disable_builtin_zoom() {
        let make_key_input = |key: egui::Key, modifiers: egui::Modifiers| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1040.0, 680.0),
            )),
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        };
        let empty_input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1040.0, 680.0),
            )),
            ..Default::default()
        };

        let ctx = egui::Context::default();
        let mut app = DesktopApp::new_headless(
            &ctx,
            AppPrefs {
                nim: "251511025".to_string(),
                ..AppPrefs::default()
            },
            Box::new(MemoryFallbackStore::new()),
        );
        assert!(
            ctx.options(|o| !o.zoom_with_keyboard),
            "built-in egui zoom_with_keyboard must be disabled to prevent double zooming"
        );

        app.core.borrow_mut().open_settings();

        // Ctrl+= zooms +10% (100 -> 110) and syncs both prefs and open settings state.
        let _ = ctx.run(
            make_key_input(egui::Key::Equals, egui::Modifiers::COMMAND),
            |c| app.update_ui(c),
        );
        let _ = ctx.run(empty_input(), |c| app.update_ui(c));
        assert_eq!(app.prefs().ui_zoom_pct, 110);
        assert_eq!(app.settings_ui_zoom_pct_for_test(), 110);
        assert!((ctx.zoom_factor() - 1.10).abs() < 1e-5);

        // Ctrl+Shift+Plus steps up and clamps strictly at 150%.
        for _ in 0..6 {
            let _ = ctx.run(
                make_key_input(
                    egui::Key::Plus,
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                ),
                |c| app.update_ui(c),
            );
            let _ = ctx.run(empty_input(), |c| app.update_ui(c));
        }
        assert_eq!(app.prefs().ui_zoom_pct, 150);
        assert_eq!(app.settings_ui_zoom_pct_for_test(), 150);
        assert!((ctx.zoom_factor() - 1.50).abs() < 1e-5);

        // AltGr (COMMAND + ALT) must not alter zoom.
        let _ = ctx.run(
            make_key_input(
                egui::Key::Minus,
                egui::Modifiers::COMMAND | egui::Modifiers::ALT,
            ),
            |c| app.update_ui(c),
        );
        let _ = ctx.run(empty_input(), |c| app.update_ui(c));
        assert_eq!(app.prefs().ui_zoom_pct, 150);

        // Ctrl+0 resets cleanly to 100%.
        let _ = ctx.run(
            make_key_input(egui::Key::Num0, egui::Modifiers::COMMAND),
            |c| app.update_ui(c),
        );
        let _ = ctx.run(empty_input(), |c| app.update_ui(c));
        assert_eq!(app.prefs().ui_zoom_pct, 100);
        assert_eq!(app.settings_ui_zoom_pct_for_test(), 100);
        assert!((ctx.zoom_factor() - 1.00).abs() < 1e-5);

        // Ctrl+- steps down and clamps strictly at 70%.
        for _ in 0..5 {
            let _ = ctx.run(
                make_key_input(egui::Key::Minus, egui::Modifiers::COMMAND),
                |c| app.update_ui(c),
            );
            let _ = ctx.run(empty_input(), |c| app.update_ui(c));
        }
        assert_eq!(app.prefs().ui_zoom_pct, 70);
        assert_eq!(app.settings_ui_zoom_pct_for_test(), 70);
        assert!((ctx.zoom_factor() - 0.70).abs() < 1e-5);
    }

    #[test]
    fn settings_zoom_preview_reverts_on_cancel_and_persists_on_save() {
        let empty_input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1040.0, 680.0),
            )),
            ..Default::default()
        };

        let ctx = egui::Context::default();
        let app = DesktopApp::new_headless(
            &ctx,
            AppPrefs {
                nim: "251511025".to_string(),
                ui_zoom_pct: 100,
                ..AppPrefs::default()
            },
            Box::new(MemoryFallbackStore::new()),
        );

        // Preview 135% while Settings is open, then cancel -> reverts to 100% (1.0).
        {
            let mut core = app.core.borrow_mut();
            core.open_settings();
            core.settings.ui_zoom_pct = 135;
        }
        ctx.set_zoom_factor(prefs::zoom_pct_to_factor(135));
        let _ = ctx.run(empty_input(), |_| {});
        assert!((ctx.zoom_factor() - 1.35).abs() < 1e-5);

        {
            let mut core = app.core.borrow_mut();
            core.close_settings(&ctx);
        }
        let _ = ctx.run(empty_input(), |_| {});
        assert_eq!(app.prefs().ui_zoom_pct, 100);
        assert_eq!(app.settings_ui_zoom_pct_for_test(), 100);
        assert!((ctx.zoom_factor() - 1.0).abs() < 1e-5);

        // Reopen, set 135%, and save -> persists and keeps zoom at 1.35.
        {
            let mut core = app.core.borrow_mut();
            core.open_settings();
            core.settings.ui_zoom_pct = 135;
            assert!(core.save_settings_inner(&ctx, true));
        }
        let _ = ctx.run(empty_input(), |_| {});
        assert!(!app.is_settings_open());
        assert_eq!(app.prefs().ui_zoom_pct, 135);
        assert!((ctx.zoom_factor() - 1.35).abs() < 1e-5);
    }

    #[test]
    fn settings_notification_volume_wires_into_test_notification_and_save_revert() {
        let ctx = egui::Context::default();
        let mut app = DesktopApp::new_headless(
            &ctx,
            AppPrefs {
                nim: "251511025".to_string(),
                notification_sound: true,
                notification_volume: 85,
                ..AppPrefs::default()
            },
            Box::new(MemoryFallbackStore::new()),
        );
        assert_eq!(app.notifier_sound_volume_for_test(), 85);

        let (recorded, log) = Notifier::new_recorded(true);
        app.set_notifier_for_test(recorded);

        {
            let mut core = app.core.borrow_mut();
            core.open_settings();
            assert_eq!(core.settings.notification_volume, 85);
            core.settings.notification_volume = 35;
            core.handle_test_notification(&ctx);
        }
        assert_eq!(app.notifier_sound_volume_for_test(), 35);
        assert_eq!(log.lock().unwrap().len(), 1);

        // Cancel reverts settings & notifier volume back to saved prefs (85).
        {
            let mut core = app.core.borrow_mut();
            core.close_settings(&ctx);
        }
        assert_eq!(app.settings_notification_volume_for_test(), 85);
        assert_eq!(app.notifier_sound_volume_for_test(), 85);
        assert_eq!(app.prefs().notification_volume, 85);

        // Save at 0% persists and wires to notifier.
        {
            let mut core = app.core.borrow_mut();
            core.open_settings();
            core.settings.notification_volume = 0;
            assert!(core.save_settings_inner(&ctx, true));
        }
        assert_eq!(app.prefs().notification_volume, 0);
        assert_eq!(app.notifier_sound_volume_for_test(), 0);
    }
}

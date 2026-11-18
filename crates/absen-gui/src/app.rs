//! Top-level `eframe::App` state and responsive layout orchestration.

use crate::bridge::EngineController;
use crate::credentials::{open_default_store, CredentialStore};
use crate::icon::IconStatus;
use crate::platform::{AutostartManager, Notifier, TrayCommand, TrayManager};
use crate::prefs::{self, AppPrefs, ThemePreference};
use crate::theme::{apply_theme, configure_fonts, ThemeTokens};
use crate::ui::header::HeaderAction;
use crate::ui::settings::{SettingsAction, SettingsState};
use crate::ui::{activity, header, jadwal, settings, CompactTab};

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
}

impl DesktopApp {
    pub fn new(cc: &eframe::CreationContext<'_>, start_minimized: bool) -> Self {
        configure_fonts(&cc.egui_ctx);

        let prefs = prefs::load().unwrap_or_default();
        apply_theme(&cc.egui_ctx, prefs.theme);

        let creds = open_default_store();
        let initial_password = if prefs.nim.is_empty() {
            String::new()
        } else {
            creds
                .get_password(&prefs.nim)
                .ok()
                .flatten()
                .unwrap_or_default()
        };

        let mut settings = SettingsState::from_prefs(&prefs, initial_password);
        if prefs.nim.is_empty() {
            settings.open = true;
        }

        let notifier = Notifier::new(prefs.notifications_enabled);
        let autostart = AutostartManager::new("Polban Presensi");
        let tray = TrayManager::new().ok();

        Self {
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
        let _ = prefs::save(&self.prefs);
    }

    fn handle_start_monitoring(&mut self, ctx: &egui::Context) {
        if self.prefs.nim.trim().is_empty() {
            self.settings.open = true;
            self.settings.status_message =
                Some("Isi NIM dan password terlebih dahulu.".to_string());
            return;
        }
        let pw = self
            .creds
            .get_password(&self.prefs.nim)
            .ok()
            .flatten()
            .unwrap_or_else(|| self.settings.password.clone());
        if pw.is_empty() {
            self.settings.open = true;
            self.settings.status_message = Some("Password belum tersimpan.".to_string());
            return;
        }
        let cfg = self.prefs.to_core_config(pw);
        let wake_ctx = ctx.clone();
        let _ = self.controller.start(
            cfg,
            true,
            Some(Box::new(move || wake_ctx.request_repaint())),
        );
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

    fn save_settings(&mut self, ctx: &egui::Context) {
        self.settings.apply_to_prefs(&mut self.prefs);
        if !self.prefs.nim.is_empty() && !self.settings.password.is_empty() {
            if let Err(e) = self
                .creds
                .set_password(&self.prefs.nim, &self.settings.password)
            {
                self.settings.status_message = Some(format!("Gagal simpan kredensial: {e}"));
                return;
            }
        }
        if let Err(e) = prefs::save(&self.prefs) {
            self.settings.status_message = Some(format!("Gagal simpan preferensi: {e}"));
            return;
        }
        self.notifier.set_enabled(self.prefs.notifications_enabled);
        let _ = self.autostart.set_enabled(self.prefs.autostart);
        apply_theme(ctx, self.prefs.theme);
        self.settings.status_message = Some("Pengaturan tersimpan.".to_string());
        self.settings.open = false;
    }
}

impl eframe::App for DesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
        let snapshot = self.controller.snapshot();

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

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(palette.bg_canvas)
                    .inner_margin(egui::Margin::same(ThemeTokens::SPACE_LG)),
            )
            .show(ctx, |ui| {
                let width = ui.available_width();
                if let Some(action) = header::show(ui, &snapshot, palette, width) {
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

                if width >= ThemeTokens::BREAKPOINT_WIDE {
                    let gutter = ThemeTokens::SPACE_LG;
                    let left_w = (width - gutter) * 0.60;
                    let right_w = width - gutter - left_w;
                    ui.horizontal_top(|ui| {
                        ui.allocate_ui(egui::vec2(left_w, ui.available_height()), |ui| {
                            jadwal::show(ui, &snapshot, palette, false);
                        });
                        ui.add_space(gutter);
                        ui.allocate_ui(egui::vec2(right_w, ui.available_height()), |ui| {
                            activity::show(ui, &snapshot, palette);
                        });
                    });
                } else if width >= ThemeTokens::BREAKPOINT_MEDIUM {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        jadwal::show(ui, &snapshot, palette, false);
                        ui.add_space(ThemeTokens::SPACE_LG);
                        activity::show(ui, &snapshot, palette);
                    });
                } else {
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(self.compact_tab == CompactTab::Jadwal, "Jadwal")
                            .clicked()
                        {
                            self.compact_tab = CompactTab::Jadwal;
                        }
                        if ui
                            .selectable_label(self.compact_tab == CompactTab::Activity, "Aktivitas")
                            .clicked()
                        {
                            self.compact_tab = CompactTab::Activity;
                        }
                    });
                    ui.add_space(ThemeTokens::SPACE_SM);
                    egui::ScrollArea::vertical().show(ui, |ui| match self.compact_tab {
                        CompactTab::Jadwal => jadwal::show(ui, &snapshot, palette, true),
                        CompactTab::Activity => activity::show(ui, &snapshot, palette),
                    });
                }
            });

        if let Some(action) = settings::show(ctx, &mut self.settings, palette) {
            match action {
                SettingsAction::Close => self.settings.open = false,
                SettingsAction::Save => self.save_settings(ctx),
                SettingsAction::ImportDotenv => {
                    if let Some(first) = prefs::detect_env_candidates().into_iter().next() {
                        if let Ok(res) = prefs::import_dotenv_file(&first) {
                            let pw = res.password.unwrap_or_default();
                            let open = self.settings.open;
                            self.settings = SettingsState::from_prefs(&res.prefs, pw);
                            self.settings.open = open;
                            self.settings.status_message =
                                Some(format!("Diimpor dari {}", res.source_path.display()));
                        }
                    } else {
                        self.settings.status_message =
                            Some("Tidak ditemukan file .env di direktori standar.".to_string());
                    }
                }
            }
        }
    }
}

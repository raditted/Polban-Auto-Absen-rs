//! System tray icon and menu abstraction powered by `tray-icon`.

use std::sync::{mpsc, Arc, Mutex, Once, OnceLock};

use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent, TrayIconId,
};

use crate::icon::{tray_rgba, IconStatus};

/// Commands triggered from the OS system tray menu or click action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    ShowWindow,
    ToggleMonitoring,
    ScanNow,
    Quit,
}

/// Optional callback invoked on the background event thread when a tray command arrives.
pub type TrayWaker = Arc<dyn Fn(TrayCommand) + Send + Sync>;

#[derive(Default)]
struct GlobalTrayRouter {
    tray_id: Option<TrayIconId>,
    show_id: Option<MenuId>,
    toggle_id: Option<MenuId>,
    scan_id: Option<MenuId>,
    quit_id: Option<MenuId>,
    tx: Option<mpsc::Sender<TrayCommand>>,
    waker: Option<TrayWaker>,
}

static ROUTER: OnceLock<Mutex<GlobalTrayRouter>> = OnceLock::new();
static HANDLERS_INIT: Once = Once::new();

fn router() -> &'static Mutex<GlobalTrayRouter> {
    ROUTER.get_or_init(|| Mutex::new(GlobalTrayRouter::default()))
}

fn dispatch_tray_command(cmd: TrayCommand) {
    let (tx, waker) = match router().lock() {
        Ok(guard) => (guard.tx.clone(), guard.waker.clone()),
        Err(_) => return,
    };
    if let Some(tx) = tx {
        let _ = tx.send(cmd);
    }
    if let Some(waker) = waker {
        waker(cmd);
    }
}

fn ensure_handlers_registered() {
    HANDLERS_INIT.call_once(|| {
        MenuEvent::set_event_handler(Some(|event: MenuEvent| {
            let cmd = match router().lock() {
                Ok(guard) => {
                    if guard.show_id.as_ref() == Some(&event.id) {
                        Some(TrayCommand::ShowWindow)
                    } else if guard.toggle_id.as_ref() == Some(&event.id) {
                        Some(TrayCommand::ToggleMonitoring)
                    } else if guard.scan_id.as_ref() == Some(&event.id) {
                        Some(TrayCommand::ScanNow)
                    } else if guard.quit_id.as_ref() == Some(&event.id) {
                        Some(TrayCommand::Quit)
                    } else {
                        None
                    }
                }
                Err(_) => None,
            };
            if let Some(cmd) = cmd {
                dispatch_tray_command(cmd);
            }
        }));

        TrayIconEvent::set_event_handler(Some(|event: TrayIconEvent| {
            let matches_action = matches!(
                &event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            );
            if !matches_action {
                return;
            }
            let is_our_icon = router()
                .lock()
                .map(|g| g.tray_id.as_ref() == Some(event.id()))
                .unwrap_or(false);
            if is_our_icon {
                dispatch_tray_command(TrayCommand::ShowWindow);
            }
        }));
    });
}

fn build_tray_icon_image(status: IconStatus) -> Result<Icon, String> {
    let (rgba, width, height) = tray_rgba(32, Some(status));
    Icon::from_rgba(rgba, width, height).map_err(|e| format!("Gagal membuat ikon tray: {e}"))
}

fn tooltip_text(status: IconStatus, monitoring: bool) -> &'static str {
    match status {
        IconStatus::Error => "Polban Presensi: Gangguan",
        IconStatus::Monitoring => "Polban Presensi: Memantau",
        IconStatus::Idle if monitoring => "Polban Presensi: Memantau",
        IconStatus::Idle => "Polban Presensi: Siaga",
    }
}

/// Manages the native OS tray icon and menu items on the main UI thread.
pub struct TrayManager {
    tray_icon: TrayIcon,
    toggle_item: MenuItem,
    rx: Mutex<mpsc::Receiver<TrayCommand>>,
    last_status: Option<IconStatus>,
    monitoring: bool,
}

impl TrayManager {
    /// Initialize the native OS tray icon and context menu.
    ///
    /// Returns `Err` cleanly in headless/CI environments without panicking.
    pub fn new() -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        gtk::init().map_err(|e| format!("Gagal inisialisasi GTK untuk tray: {e}"))?;

        let show_item = MenuItem::new("Buka Polban Presensi", true, None);
        let toggle_item = MenuItem::new("Mulai Pemantauan", true, None);
        let scan_item = MenuItem::new("Cek Sekarang", true, None);
        let separator = PredefinedMenuItem::separator();
        let quit_item = MenuItem::new("Keluar", true, None);

        let menu = Menu::new();
        menu.append_items(&[&show_item, &toggle_item, &scan_item, &separator, &quit_item])
            .map_err(|e| format!("Gagal menyusun menu system tray: {e}"))?;

        let initial_icon = build_tray_icon_image(IconStatus::Idle)?;
        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(tooltip_text(IconStatus::Idle, false))
            .with_icon(initial_icon)
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| format!("Gagal membuat ikon system tray: {e}"))?;

        let (tx, rx) = mpsc::channel();
        if let Ok(mut guard) = router().lock() {
            guard.tray_id = Some(tray_icon.id().clone());
            guard.show_id = Some(show_item.id().clone());
            guard.toggle_id = Some(toggle_item.id().clone());
            guard.scan_id = Some(scan_item.id().clone());
            guard.quit_id = Some(quit_item.id().clone());
            guard.tx = Some(tx);
            guard.waker = None;
        }
        ensure_handlers_registered();

        Ok(Self {
            tray_icon,
            toggle_item,
            rx: Mutex::new(rx),
            last_status: Some(IconStatus::Idle),
            monitoring: false,
        })
    }

    /// Register a custom callback invoked whenever a tray event enqueues a `TrayCommand`.
    pub fn set_waker(&mut self, waker: TrayWaker) {
        if let Ok(mut guard) = router().lock() {
            guard.waker = Some(waker);
        }
    }

    /// Register an `egui::Context` waker that immediately shows/focuses the window
    /// on `ShowWindow` and requests a repaint for any tray command.
    pub fn set_egui_waker(&mut self, ctx: egui::Context) {
        self.set_waker(Arc::new(move |cmd| {
            if matches!(cmd, TrayCommand::ShowWindow) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            ctx.request_repaint();
        }));
    }

    /// Update tray icon, tooltip, and dynamic toggle label only when state changes.
    pub fn update_status(&mut self, status: IconStatus, monitoring: bool) {
        if self.last_status == Some(status) && self.monitoring == monitoring {
            return;
        }
        self.last_status = Some(status);
        self.monitoring = monitoring;

        let toggle_label = if monitoring {
            "Hentikan Pemantauan"
        } else {
            "Mulai Pemantauan"
        };
        self.toggle_item.set_text(toggle_label);

        if let Ok(icon) = build_tray_icon_image(status) {
            let _ = self.tray_icon.set_icon(Some(icon));
        }
        let _ = self
            .tray_icon
            .set_tooltip(Some(tooltip_text(status, monitoring)));
    }

    /// Drain all pending tray commands without blocking.
    pub fn poll_commands(&self) -> Vec<TrayCommand> {
        let Ok(guard) = self.rx.lock() else {
            return Vec::new();
        };
        let mut cmds = Vec::new();
        while let Ok(cmd) = guard.try_recv() {
            cmds.push(cmd);
        }
        cmds
    }

    pub fn last_status(&self) -> Option<IconStatus> {
        self.last_status
    }

    pub fn is_monitoring(&self) -> bool {
        self.monitoring
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_text_contains_no_em_dash_or_double_dash() {
        for status in [IconStatus::Idle, IconStatus::Monitoring, IconStatus::Error] {
            for monitoring in [false, true] {
                let text = tooltip_text(status, monitoring);
                assert!(!text.contains('\u{2014}'));
                assert!(!text.contains(" -- "));
            }
        }
    }

    #[test]
    fn tray_icon_image_builds_cleanly_for_all_statuses() {
        for status in [IconStatus::Idle, IconStatus::Monitoring, IconStatus::Error] {
            assert!(build_tray_icon_image(status).is_ok());
        }
    }
}

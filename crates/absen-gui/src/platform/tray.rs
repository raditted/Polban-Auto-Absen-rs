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

/// Optional callback invoked when a tray event enqueues a `TrayCommand`.
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

impl GlobalTrayRouter {
    fn command_for_menu_id(&self, id: &MenuId) -> Option<TrayCommand> {
        if self.show_id.as_ref() == Some(id) {
            Some(TrayCommand::ShowWindow)
        } else if self.toggle_id.as_ref() == Some(id) {
            Some(TrayCommand::ToggleMonitoring)
        } else if self.scan_id.as_ref() == Some(id) {
            Some(TrayCommand::ScanNow)
        } else if self.quit_id.as_ref() == Some(id) {
            Some(TrayCommand::Quit)
        } else {
            None
        }
    }

    fn command_for_tray_event(&self, event: &TrayIconEvent) -> Option<TrayCommand> {
        let matches_action = matches!(
            event,
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            }
        );
        if matches_action && self.tray_id.as_ref() == Some(event.id()) {
            Some(TrayCommand::ShowWindow)
        } else {
            None
        }
    }
}

static ROUTER: OnceLock<Mutex<GlobalTrayRouter>> = OnceLock::new();
static HANDLERS_INIT: Once = Once::new();

fn router() -> &'static Mutex<GlobalTrayRouter> {
    ROUTER.get_or_init(|| Mutex::new(GlobalTrayRouter::default()))
}

fn dispatch_tray_command(cmd: TrayCommand) {
    let (tx, waker) = match router().lock() {
        Ok(guard) => (guard.tx.clone(), guard.waker.clone()),
        Err(poisoned) => {
            let guard = poisoned.into_inner();
            (guard.tx.clone(), guard.waker.clone())
        }
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
                Ok(guard) => guard.command_for_menu_id(&event.id),
                Err(poisoned) => poisoned.into_inner().command_for_menu_id(&event.id),
            };
            if let Some(cmd) = cmd {
                dispatch_tray_command(cmd);
            }
        }));

        TrayIconEvent::set_event_handler(Some(|event: TrayIconEvent| {
            let cmd = match router().lock() {
                Ok(guard) => guard.command_for_tray_event(&event),
                Err(poisoned) => poisoned.into_inner().command_for_tray_event(&event),
            };
            if let Some(cmd) = cmd {
                dispatch_tray_command(cmd);
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

        ensure_handlers_registered();

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
        let mut guard = router().lock().unwrap_or_else(|e| e.into_inner());
        guard.tray_id = Some(tray_icon.id().clone());
        guard.show_id = Some(show_item.id().clone());
        guard.toggle_id = Some(toggle_item.id().clone());
        guard.scan_id = Some(scan_item.id().clone());
        guard.quit_id = Some(quit_item.id().clone());
        guard.tx = Some(tx);
        guard.waker = None;
        drop(guard);

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
        let mut guard = router().lock().unwrap_or_else(|e| e.into_inner());
        guard.waker = Some(waker);
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
        let guard = self.rx.lock().unwrap_or_else(|e| e.into_inner());
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tray_icon::dpi::PhysicalPosition;
    use tray_icon::Rect;

    #[test]
    fn tooltip_text_contains_no_em_dash_or_double_dash() {
        for status in [IconStatus::Idle, IconStatus::Monitoring, IconStatus::Error] {
            for monitoring in [false, true] {
                let text = tooltip_text(status, monitoring);
                assert!(!text.contains('\u{2014}'));
                assert!(!text.contains(concat!(" ", "-", "-", " ")));
            }
        }
    }

    #[test]
    fn tray_icon_image_builds_cleanly_for_all_statuses() {
        for status in [IconStatus::Idle, IconStatus::Monitoring, IconStatus::Error] {
            assert!(build_tray_icon_image(status).is_ok());
        }
    }

    #[test]
    fn router_maps_menu_ids_and_tray_clicks_accurately() {
        let router = GlobalTrayRouter {
            tray_id: Some(TrayIconId::new("tray-main")),
            show_id: Some(MenuId::new("show")),
            toggle_id: Some(MenuId::new("toggle")),
            scan_id: Some(MenuId::new("scan")),
            quit_id: Some(MenuId::new("quit")),
            tx: None,
            waker: None,
        };

        assert_eq!(
            router.command_for_menu_id(&MenuId::new("show")),
            Some(TrayCommand::ShowWindow)
        );
        assert_eq!(
            router.command_for_menu_id(&MenuId::new("toggle")),
            Some(TrayCommand::ToggleMonitoring)
        );
        assert_eq!(
            router.command_for_menu_id(&MenuId::new("scan")),
            Some(TrayCommand::ScanNow)
        );
        assert_eq!(
            router.command_for_menu_id(&MenuId::new("quit")),
            Some(TrayCommand::Quit)
        );
        assert_eq!(router.command_for_menu_id(&MenuId::new("unknown")), None);

        let left_up = TrayIconEvent::Click {
            id: TrayIconId::new("tray-main"),
            position: PhysicalPosition::default(),
            rect: Rect::default(),
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
        };
        assert_eq!(
            router.command_for_tray_event(&left_up),
            Some(TrayCommand::ShowWindow)
        );

        let left_dbl = TrayIconEvent::DoubleClick {
            id: TrayIconId::new("tray-main"),
            position: PhysicalPosition::default(),
            rect: Rect::default(),
            button: MouseButton::Left,
        };
        assert_eq!(
            router.command_for_tray_event(&left_dbl),
            Some(TrayCommand::ShowWindow)
        );

        let right_up = TrayIconEvent::Click {
            id: TrayIconId::new("tray-main"),
            position: PhysicalPosition::default(),
            rect: Rect::default(),
            button: MouseButton::Right,
            button_state: MouseButtonState::Up,
        };
        assert_eq!(router.command_for_tray_event(&right_up), None);

        let other_icon = TrayIconEvent::Click {
            id: TrayIconId::new("other"),
            position: PhysicalPosition::default(),
            rect: Rect::default(),
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
        };
        assert_eq!(router.command_for_tray_event(&other_icon), None);
    }

    #[test]
    fn dispatch_tray_command_delivers_to_channel_and_waker() {
        let (tx, rx) = mpsc::channel();
        let wakes = Arc::new(AtomicUsize::new(0));
        let wakes_clone = Arc::clone(&wakes);

        {
            let mut guard = router().lock().unwrap_or_else(|e| e.into_inner());
            guard.tx = Some(tx);
            guard.waker = Some(Arc::new(move |_| {
                wakes_clone.fetch_add(1, Ordering::SeqCst);
            }));
        }

        dispatch_tray_command(TrayCommand::Quit);
        assert_eq!(rx.try_recv().ok(), Some(TrayCommand::Quit));
        assert_eq!(wakes.load(Ordering::SeqCst), 1);

        let mut guard = router().lock().unwrap_or_else(|e| e.into_inner());
        guard.tx = None;
        guard.waker = None;
    }
}

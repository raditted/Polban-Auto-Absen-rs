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

/// Runtime windowing display backend detected from `eframe::CreationContext`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowBackend {
    X11,
    Wayland,
    Other,
}

/// Availability of hiding the main window to the system tray (`Visible(false)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HideToTrayAvailability {
    Supported,
    WaylandUnsupported,
    MissingTrayHost,
    TrayUnavailable,
}

impl HideToTrayAvailability {
    /// Returns `true` when closing/minimizing can safely hide the window into the tray.
    pub const fn is_supported(self) -> bool {
        matches!(self, Self::Supported)
    }
}

/// Inspect the actual runtime display backend from `eframe::CreationContext`.
pub fn detect_window_backend(cc: &eframe::CreationContext<'_>) -> WindowBackend {
    #[cfg(target_os = "linux")]
    {
        use raw_window_handle::{HasDisplayHandle as _, RawDisplayHandle};

        match cc.display_handle().ok().map(|h| h.as_raw()) {
            Some(RawDisplayHandle::Wayland(_)) => WindowBackend::Wayland,
            Some(RawDisplayHandle::Xlib(_)) | Some(RawDisplayHandle::Xcb(_)) => WindowBackend::X11,
            _ => WindowBackend::Other,
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = cc;
        WindowBackend::Other
    }
}

/// Compute whether hiding the window to the system tray is safe and supported.
pub fn resolve_hide_to_tray_availability(
    is_linux: bool,
    window_backend: WindowBackend,
    tray_initialized: bool,
    tray_host_present: bool,
) -> HideToTrayAvailability {
    if !tray_initialized {
        return HideToTrayAvailability::TrayUnavailable;
    }
    if !is_linux {
        return HideToTrayAvailability::Supported;
    }
    match window_backend {
        WindowBackend::Wayland => HideToTrayAvailability::WaylandUnsupported,
        WindowBackend::X11 | WindowBackend::Other => {
            if tray_host_present {
                HideToTrayAvailability::Supported
            } else {
                HideToTrayAvailability::MissingTrayHost
            }
        }
    }
}

/// Determine from OS and environment variables whether hiding the window to the
/// system tray (`ViewportCommand::Visible(false)`) is supported when no
/// `CreationContext` handle is available.
pub fn supports_hide_to_tray_from_env(
    is_linux: bool,
    wayland_display: Option<&str>,
    winit_unix_backend: Option<&str>,
) -> bool {
    if !is_linux {
        return true;
    }
    let has_wayland = wayland_display.is_some_and(|v| !v.trim().is_empty());
    let forced_x11 = winit_unix_backend.is_some_and(|v| v.trim().eq_ignore_ascii_case("x11"));
    !has_wayland || forced_x11
}

/// Returns `true` when the active platform/windowing backend can reliably hide
/// and restore the main window from the system tray based on environment variables.
pub fn supports_hide_to_tray() -> bool {
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    let winit_unix_backend = std::env::var("WINIT_UNIX_BACKEND").ok();
    supports_hide_to_tray_from_env(
        cfg!(target_os = "linux"),
        wayland_display.as_deref(),
        winit_unix_backend.as_deref(),
    )
}

fn build_tray_icon_and_menu() -> Result<(TrayIcon, MenuItem, mpsc::Receiver<TrayCommand>), String> {
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

    Ok((tray_icon, toggle_item, rx))
}

fn apply_tray_status_update(
    tray_icon: &TrayIcon,
    toggle_item: &MenuItem,
    status: IconStatus,
    monitoring: bool,
) {
    let toggle_label = if monitoring {
        "Hentikan Pemantauan"
    } else {
        "Mulai Pemantauan"
    };
    toggle_item.set_text(toggle_label);

    if let Ok(icon) = build_tray_icon_image(status) {
        let _ = tray_icon.set_icon(Some(icon));
    }
    let _ = tray_icon.set_tooltip(Some(tooltip_text(status, monitoring)));
}

#[cfg(target_os = "linux")]
mod linux_tray {
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::{Duration, Instant};

    use gtk::gio;
    use gtk::glib;
    use gtk::glib::prelude::ToVariant;
    use tray_icon::menu::MenuItem;
    use tray_icon::TrayIcon;

    use super::{apply_tray_status_update, build_tray_icon_and_menu, TrayCommand, WindowBackend};
    use crate::icon::IconStatus;

    const SNI_WATCHER_BUS_NAME: &str = "org.kde.StatusNotifierWatcher";

    pub(super) type HostChangeWaker = Arc<dyn Fn() + Send + Sync>;

    struct GtkTrayObjects {
        tray_icon: TrayIcon,
        toggle_item: MenuItem,
        unwatch_dbus: Option<Box<dyn FnOnce()>>,
    }

    impl Drop for GtkTrayObjects {
        fn drop(&mut self) {
            if let Some(unwatch) = self.unwatch_dbus.take() {
                unwatch();
            }
        }
    }

    thread_local! {
        static GTK_TRAY_OBJECTS: RefCell<Option<GtkTrayObjects>> = const { RefCell::new(None) };
    }

    fn query_sni_watcher_owned() -> Option<bool> {
        let conn = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()?;
        let params = glib::Variant::tuple_from_iter([SNI_WATCHER_BUS_NAME.to_variant()]);
        let reply_ty = glib::VariantTy::new("(b)").ok()?;
        let reply = conn
            .call_sync(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "NameHasOwner",
                Some(&params),
                Some(reply_ty),
                gio::DBusCallFlags::NONE,
                200,
                gio::Cancellable::NONE,
            )
            .ok()?;
        reply.try_child_get::<bool>(0).ok().flatten()
    }

    fn has_x11_system_tray() -> bool {
        let has_display = std::env::var_os("DISPLAY").is_some_and(|v| !v.is_empty());
        if !has_display {
            return false;
        }
        let Ok(xlib) = x11_dl::xlib::Xlib::open() else {
            return false;
        };
        // SAFETY: `XInitThreads` is idempotent and safe before `XOpenDisplay(NULL)`,
        // and any opened `Display*` handle is closed immediately before returning.
        unsafe {
            (xlib.XInitThreads)();
            let display = (xlib.XOpenDisplay)(std::ptr::null());
            if display.is_null() {
                return false;
            }
            let screen = (xlib.XDefaultScreen)(display);
            let Ok(atom_name) = std::ffi::CString::new(format!("_NET_SYSTEM_TRAY_S{screen}"))
            else {
                (xlib.XCloseDisplay)(display);
                return false;
            };
            let atom = (xlib.XInternAtom)(display, atom_name.as_ptr(), 1);
            let owner = if atom != 0 {
                (xlib.XGetSelectionOwner)(display, atom)
            } else {
                0
            };
            (xlib.XCloseDisplay)(display);
            owner != 0
        }
    }

    fn notify_host_change(waker_slot: &Arc<Mutex<Option<HostChangeWaker>>>) {
        let cb = waker_slot.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(wake) = cb {
            wake();
        }
    }

    /// Runs GTK initialization, tray construction, D-Bus tray host tracking, and
    /// `gtk::main()` on a dedicated `"polban-gtk-tray"` thread as required by
    /// `tray-icon` / `libappindicator` under `eframe`.
    pub(super) struct LinuxTrayBackend {
        shutdown_flag: Arc<AtomicBool>,
        sni_host_present: Arc<AtomicBool>,
        x11_tray_present: Arc<AtomicBool>,
        host_change_waker: Arc<Mutex<Option<HostChangeWaker>>>,
        thread_handle: Option<std::thread::JoinHandle<()>>,
    }

    impl LinuxTrayBackend {
        pub(super) fn spawn(
            window_backend: WindowBackend,
        ) -> Result<(Self, mpsc::Receiver<TrayCommand>), String> {
            let (init_tx, init_rx) =
                mpsc::sync_channel::<Result<mpsc::Receiver<TrayCommand>, String>>(1);
            let shutdown_flag = Arc::new(AtomicBool::new(false));
            let sni_host_present = Arc::new(AtomicBool::new(false));
            let x11_tray_present = Arc::new(AtomicBool::new(false));
            let host_change_waker: Arc<Mutex<Option<HostChangeWaker>>> = Arc::new(Mutex::new(None));

            let shutdown_thread = Arc::clone(&shutdown_flag);
            let sni_thread = Arc::clone(&sni_host_present);
            let x11_tray_thread = Arc::clone(&x11_tray_present);
            let waker_thread = Arc::clone(&host_change_waker);

            let handle = std::thread::Builder::new()
                .name("polban-gtk-tray".to_string())
                .spawn(move || {
                    if gtk::is_initialized() && !gtk::is_initialized_main_thread() {
                        let _ = init_tx
                            .send(Err("GTK sudah diinisialisasi di thread lain.".to_string()));
                        return;
                    }
                    if let Err(err) = gtk::init() {
                        let _ =
                            init_tx.send(Err(format!("Gagal inisialisasi GTK untuk tray: {err}")));
                        return;
                    }

                    let session_bus_available =
                        gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).is_ok();
                    let initial_sni = if session_bus_available {
                        query_sni_watcher_owned().unwrap_or(false)
                    } else {
                        false
                    };
                    sni_thread.store(initial_sni, Ordering::Release);
                    if !initial_sni && window_backend == WindowBackend::X11 {
                        x11_tray_thread.store(has_x11_system_tray(), Ordering::Release);
                    }

                    match build_tray_icon_and_menu() {
                        Ok((tray_icon, toggle_item, cmd_rx)) => {
                            if shutdown_thread.load(Ordering::Acquire) {
                                drop(tray_icon);
                                drop(toggle_item);
                                let _ = init_tx.send(Err(
                                    "System tray dihentikan sebelum selesai inisialisasi."
                                        .to_string(),
                                ));
                                return;
                            }

                            let unwatch_dbus: Option<Box<dyn FnOnce()>> = if session_bus_available {
                                let sni_appeared = Arc::clone(&sni_thread);
                                let waker_appeared = Arc::clone(&waker_thread);
                                let sni_vanished = Arc::clone(&sni_thread);
                                let x11_vanished = Arc::clone(&x11_tray_thread);
                                let waker_vanished = Arc::clone(&waker_thread);
                                let id = gio::bus_watch_name(
                                    gio::BusType::Session,
                                    SNI_WATCHER_BUS_NAME,
                                    gio::BusNameWatcherFlags::NONE,
                                    move |_, _, _| {
                                        let prev = sni_appeared.swap(true, Ordering::AcqRel);
                                        if !prev {
                                            notify_host_change(&waker_appeared);
                                        }
                                    },
                                    move |_, _| {
                                        let prev = sni_vanished.swap(false, Ordering::AcqRel);
                                        if window_backend == WindowBackend::X11 {
                                            x11_vanished
                                                .store(has_x11_system_tray(), Ordering::Release);
                                        }
                                        if prev {
                                            notify_host_change(&waker_vanished);
                                        }
                                    },
                                );
                                Some(Box::new(move || gio::bus_unwatch_name(id)))
                            } else {
                                None
                            };

                            GTK_TRAY_OBJECTS.with(|slot| {
                                *slot.borrow_mut() = Some(GtkTrayObjects {
                                    tray_icon,
                                    toggle_item,
                                    unwatch_dbus,
                                });
                            });

                            let _ = init_tx.send(Ok(cmd_rx));
                            if !shutdown_thread.load(Ordering::Acquire) {
                                gtk::main();
                            }

                            GTK_TRAY_OBJECTS.with(|slot| {
                                slot.borrow_mut().take();
                            });
                        }
                        Err(err) => {
                            let _ = init_tx.send(Err(err));
                        }
                    }
                })
                .map_err(|e| format!("Gagal membuat thread tray GTK: {e}"))?;

            match init_rx.recv() {
                Ok(Ok(cmd_rx)) => Ok((
                    Self {
                        shutdown_flag,
                        sni_host_present,
                        x11_tray_present,
                        host_change_waker,
                        thread_handle: Some(handle),
                    },
                    cmd_rx,
                )),
                Ok(Err(err)) => {
                    let _ = handle.join();
                    Err(err)
                }
                Err(_) => {
                    let _ = handle.join();
                    Err("Thread tray GTK berhenti sebelum inisialisasi selesai.".to_string())
                }
            }
        }

        pub(super) fn set_host_change_waker(&self, waker: HostChangeWaker) {
            let mut guard = self
                .host_change_waker
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *guard = Some(waker);
        }

        pub(super) fn is_tray_host_present(&self, window_backend: WindowBackend) -> bool {
            if self.sni_host_present.load(Ordering::Acquire) {
                return true;
            }
            window_backend == WindowBackend::X11 && self.x11_tray_present.load(Ordering::Acquire)
        }

        pub(super) fn refresh_host_status(&self, window_backend: WindowBackend) {
            if window_backend == WindowBackend::X11
                && !self.sni_host_present.load(Ordering::Acquire)
            {
                self.x11_tray_present
                    .store(has_x11_system_tray(), Ordering::Release);
            }
        }

        pub(super) fn update_status(&self, status: IconStatus, monitoring: bool) {
            if self.shutdown_flag.load(Ordering::Acquire) {
                return;
            }
            let _ = glib::idle_add_once(move || {
                GTK_TRAY_OBJECTS.with(|slot| {
                    if let Some(objs) = slot.borrow().as_ref() {
                        apply_tray_status_update(
                            &objs.tray_icon,
                            &objs.toggle_item,
                            status,
                            monitoring,
                        );
                    }
                });
            });
        }
    }

    impl Drop for LinuxTrayBackend {
        fn drop(&mut self) {
            self.shutdown_flag.store(true, Ordering::Release);
            if let Ok(mut guard) = self.host_change_waker.lock() {
                *guard = None;
            }
            let _ = glib::idle_add_once(|| {
                GTK_TRAY_OBJECTS.with(|slot| {
                    slot.borrow_mut().take();
                });
                if gtk::main_level() > 0 {
                    gtk::main_quit();
                }
            });
            if let Some(handle) = self.thread_handle.take() {
                let deadline = Instant::now() + Duration::from_millis(500);
                while !handle.is_finished() && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(10));
                }
                if handle.is_finished() {
                    let _ = handle.join();
                }
            }
        }
    }
}

/// Manages the native OS tray icon and menu items.
pub struct TrayManager {
    #[cfg(not(target_os = "linux"))]
    tray_icon: TrayIcon,
    #[cfg(not(target_os = "linux"))]
    toggle_item: MenuItem,
    #[cfg(target_os = "linux")]
    linux: linux_tray::LinuxTrayBackend,
    rx: Mutex<mpsc::Receiver<TrayCommand>>,
    last_status: Option<IconStatus>,
    monitoring: bool,
}

impl TrayManager {
    /// Initialize the native OS tray icon and context menu for the detected display backend.
    ///
    /// Returns `Err` cleanly in headless/CI environments without panicking.
    pub fn new_for_backend(window_backend: WindowBackend) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        let (linux, rx) = linux_tray::LinuxTrayBackend::spawn(window_backend)?;

        #[cfg(not(target_os = "linux"))]
        let (tray_icon, toggle_item, rx) = {
            let _ = window_backend;
            build_tray_icon_and_menu()?
        };

        Ok(Self {
            #[cfg(not(target_os = "linux"))]
            tray_icon,
            #[cfg(not(target_os = "linux"))]
            toggle_item,
            #[cfg(target_os = "linux")]
            linux,
            rx: Mutex::new(rx),
            last_status: Some(IconStatus::Idle),
            monitoring: false,
        })
    }

    /// Initialize the native OS tray icon and context menu.
    ///
    /// Returns `Err` cleanly in headless/CI environments without panicking.
    pub fn new() -> Result<Self, String> {
        Self::new_for_backend(WindowBackend::Other)
    }

    /// Register a custom callback invoked whenever a tray event enqueues a `TrayCommand`.
    pub fn set_waker(&mut self, waker: TrayWaker) {
        let mut guard = router().lock().unwrap_or_else(|e| e.into_inner());
        guard.waker = Some(waker);
    }

    /// Register a callback invoked when the OS tray host availability changes at runtime.
    pub fn set_host_change_waker(&mut self, waker: Arc<dyn Fn() + Send + Sync>) {
        #[cfg(target_os = "linux")]
        self.linux.set_host_change_waker(waker);
        #[cfg(not(target_os = "linux"))]
        let _ = waker;
    }

    /// Refresh cached X11 legacy tray manager state when no StatusNotifierWatcher is present.
    pub fn refresh_host_status(&self, window_backend: WindowBackend) {
        #[cfg(target_os = "linux")]
        self.linux.refresh_host_status(window_backend);
        #[cfg(not(target_os = "linux"))]
        let _ = window_backend;
    }

    /// Determine whether hiding the window to the tray is safe for `window_backend`.
    pub fn hide_to_tray_availability(
        &self,
        window_backend: WindowBackend,
    ) -> HideToTrayAvailability {
        #[cfg(target_os = "linux")]
        {
            resolve_hide_to_tray_availability(
                true,
                window_backend,
                true,
                self.linux.is_tray_host_present(window_backend),
            )
        }
        #[cfg(not(target_os = "linux"))]
        {
            resolve_hide_to_tray_availability(false, window_backend, true, true)
        }
    }

    /// Convenience boolean wrapper around [`Self::hide_to_tray_availability`].
    pub fn supports_hide_to_tray(&self, window_backend: WindowBackend) -> bool {
        self.hide_to_tray_availability(window_backend)
            .is_supported()
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

        #[cfg(target_os = "linux")]
        self.linux.update_status(status, monitoring);

        #[cfg(not(target_os = "linux"))]
        apply_tray_status_update(&self.tray_icon, &self.toggle_item, status, monitoring);
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

    #[test]
    fn supports_hide_to_tray_from_env_detects_wayland_xwayland_and_non_linux() {
        // Non-Linux platforms (Windows/macOS) always support hiding to tray.
        assert!(supports_hide_to_tray_from_env(false, None, None));
        assert!(supports_hide_to_tray_from_env(
            false,
            Some("wayland-0"),
            None
        ));
        assert!(supports_hide_to_tray_from_env(
            false,
            Some("wayland-0"),
            Some("wayland")
        ));

        // Linux pure X11 (WAYLAND_DISPLAY absent or blank).
        assert!(supports_hide_to_tray_from_env(true, None, None));
        assert!(supports_hide_to_tray_from_env(true, Some(""), None));
        assert!(supports_hide_to_tray_from_env(true, Some("   "), None));

        // Linux native Wayland (WAYLAND_DISPLAY set and WINIT_UNIX_BACKEND != "x11").
        assert!(!supports_hide_to_tray_from_env(
            true,
            Some("wayland-0"),
            None
        ));
        assert!(!supports_hide_to_tray_from_env(
            true,
            Some("wayland-1"),
            Some("wayland")
        ));
        assert!(!supports_hide_to_tray_from_env(
            true,
            Some("wayland-0"),
            Some("")
        ));

        // Linux XWayland forced via WINIT_UNIX_BACKEND=x11 (case-insensitive).
        assert!(supports_hide_to_tray_from_env(
            true,
            Some("wayland-0"),
            Some("x11")
        ));
        assert!(supports_hide_to_tray_from_env(
            true,
            Some("wayland-0"),
            Some("X11")
        ));
        assert!(supports_hide_to_tray_from_env(
            true,
            Some("wayland-0"),
            Some(" x11 ")
        ));
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn supports_hide_to_tray_returns_true_on_non_linux() {
        assert!(supports_hide_to_tray());
    }

    #[test]
    fn resolve_hide_to_tray_availability_handles_all_backends_and_tray_hosts() {
        // Non-Linux platforms always allow hiding whenever the tray initialized.
        assert_eq!(
            resolve_hide_to_tray_availability(false, WindowBackend::Other, true, false),
            HideToTrayAvailability::Supported
        );
        assert_eq!(
            resolve_hide_to_tray_availability(false, WindowBackend::Other, false, false),
            HideToTrayAvailability::TrayUnavailable
        );

        // Linux native Wayland never hides even when a StatusNotifier tray host exists.
        assert_eq!(
            resolve_hide_to_tray_availability(true, WindowBackend::Wayland, true, true),
            HideToTrayAvailability::WaylandUnsupported
        );
        assert!(!HideToTrayAvailability::WaylandUnsupported.is_supported());

        // Linux X11/XWayland with an active tray host (SNI or XEmbed) supports hide-to-tray.
        assert_eq!(
            resolve_hide_to_tray_availability(true, WindowBackend::X11, true, true),
            HideToTrayAvailability::Supported
        );
        assert!(HideToTrayAvailability::Supported.is_supported());

        // Linux X11/XWayland without any tray host (e.g. GNOME without AppIndicator) must not hide.
        assert_eq!(
            resolve_hide_to_tray_availability(true, WindowBackend::X11, true, false),
            HideToTrayAvailability::MissingTrayHost
        );
        assert!(!HideToTrayAvailability::MissingTrayHost.is_supported());

        // Linux when TrayManager itself failed to initialize.
        assert_eq!(
            resolve_hide_to_tray_availability(true, WindowBackend::X11, false, true),
            HideToTrayAvailability::TrayUnavailable
        );
    }
}

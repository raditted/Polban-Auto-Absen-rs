#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Desktop GUI entrypoint for `polban-absen-gui`.

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use absen_gui::{icon, DesktopApp};

const SINGLE_INSTANCE_ADDR: &str = "127.0.0.1:58742";

enum SingleInstanceOutcome {
    Primary(SingleInstanceHandle),
    SecondaryNotified,
    Unavailable,
}

struct SingleInstanceHandle {
    local_addr: Option<SocketAddr>,
    ui_ctx: Arc<Mutex<Option<egui::Context>>>,
    win32_msg_hwnd: Arc<AtomicIsize>,
    show_flag: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    thread_handle: Option<std::thread::JoinHandle<()>>,
}

impl SingleInstanceHandle {
    fn release(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(addr) = self.local_addr.take() {
            let _ = TcpStream::connect_timeout(&addr, Duration::from_millis(150));
        }
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for SingleInstanceHandle {
    fn drop(&mut self) {
        self.release();
    }
}

enum SingleInstanceBind {
    Primary(TcpListener),
    SecondaryNotified,
    Unavailable,
}

fn bind_single_instance_at(addr: &str) -> SingleInstanceBind {
    match TcpListener::bind(addr) {
        Ok(listener) => SingleInstanceBind::Primary(listener),
        Err(err) if err.kind() == ErrorKind::AddrInUse => {
            if let Ok(sock_addr) = addr.parse::<SocketAddr>() {
                if let Ok(mut stream) =
                    TcpStream::connect_timeout(&sock_addr, Duration::from_millis(500))
                {
                    let _ = stream.write_all(b"SHOW\n");
                    let _ = stream.flush();
                }
            }
            SingleInstanceBind::SecondaryNotified
        }
        Err(_) => SingleInstanceBind::Unavailable,
    }
}

fn start_single_instance_listener(bind: SingleInstanceBind) -> SingleInstanceOutcome {
    let listener = match bind {
        SingleInstanceBind::Primary(listener) => listener,
        SingleInstanceBind::SecondaryNotified => return SingleInstanceOutcome::SecondaryNotified,
        SingleInstanceBind::Unavailable => return SingleInstanceOutcome::Unavailable,
    };

    let local_addr = listener.local_addr().ok();
    let ui_ctx: Arc<Mutex<Option<egui::Context>>> = Arc::new(Mutex::new(None));
    let win32_msg_hwnd = Arc::new(AtomicIsize::new(0));
    let show_flag = Arc::new(AtomicBool::new(false));
    let shutdown = Arc::new(AtomicBool::new(false));

    let thread_ctx = Arc::clone(&ui_ctx);
    let thread_msg_hwnd = Arc::clone(&win32_msg_hwnd);
    let thread_flag = Arc::clone(&show_flag);
    let thread_shutdown = Arc::clone(&shutdown);

    let thread_handle = match std::thread::Builder::new()
        .name("polban-single-instance".to_string())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                if thread_shutdown.load(Ordering::Acquire) {
                    break;
                }
                let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));
                let mut buf = [0u8; 16];
                let n = (&stream).read(&mut buf).unwrap_or(0);
                if n == 0 || buf[..n].starts_with(b"SHOW") {
                    thread_flag.store(true, Ordering::Release);
                    #[cfg(target_os = "windows")]
                    {
                        let msg_hwnd = thread_msg_hwnd.load(Ordering::Acquire);
                        if msg_hwnd != 0 {
                            absen_gui::platform::win32::post_app_message(
                                msg_hwnd,
                                absen_gui::platform::win32::WM_APP_SHOW_WINDOW,
                            );
                        }
                    }
                    let _ = &thread_msg_hwnd;
                    if let Ok(guard) = thread_ctx.lock() {
                        if let Some(ctx) = guard.as_ref() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                            ctx.request_repaint();
                        }
                    }
                }
            }
        }) {
        Ok(h) => h,
        Err(_) => return SingleInstanceOutcome::Unavailable,
    };

    SingleInstanceOutcome::Primary(SingleInstanceHandle {
        local_addr,
        ui_ctx,
        win32_msg_hwnd,
        show_flag,
        shutdown,
        thread_handle: Some(thread_handle),
    })
}

#[cfg(test)]
fn acquire_single_instance_at(addr: &str) -> SingleInstanceOutcome {
    start_single_instance_listener(bind_single_instance_at(addr))
}

struct CliArgs {
    minimized: bool,
    show_help: bool,
    show_version: bool,
    unknown: Option<String>,
}

fn parse_args<I, S>(args: I) -> CliArgs
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut parsed = CliArgs {
        minimized: false,
        show_help: false,
        show_version: false,
        unknown: None,
    };

    for arg in args.into_iter().skip(1) {
        match arg.as_ref() {
            "--minimized" => parsed.minimized = true,
            "-h" | "--help" => parsed.show_help = true,
            "-V" | "--version" => parsed.show_version = true,
            other => {
                parsed.unknown = Some(other.to_string());
                break;
            }
        }
    }
    parsed
}

fn print_help() {
    println!(
        "polban-absen-gui {}\n\
         Aplikasi desktop Polban Presensi.\n\n\
         PENGGUNAAN:\n\
         \tpolban-absen-gui [OPSI]\n\n\
         OPSI:\n\
         \t--minimized    Mulai dalam keadaan diperkecil ke system tray\n\
         \t-h, --help     Tampilkan bantuan ini\n\
         \t-V, --version  Tampilkan versi aplikasi",
        env!("CARGO_PKG_VERSION")
    );
}

#[cfg(any(target_os = "linux", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinuxWindowBackend {
    Auto,
    X11,
    Wayland,
}

#[cfg(any(target_os = "linux", test))]
fn select_linux_window_backend_from(
    wayland_available: bool,
    x11_available: bool,
    winit_unix_backend: Option<&str>,
) -> LinuxWindowBackend {
    let requested = winit_unix_backend.map(str::trim).filter(|v| !v.is_empty());

    if let Some(req) = requested {
        if req.eq_ignore_ascii_case("wayland") {
            if wayland_available {
                return LinuxWindowBackend::Wayland;
            }
            if x11_available {
                return LinuxWindowBackend::X11;
            }
            return LinuxWindowBackend::Auto;
        }
        if req.eq_ignore_ascii_case("x11") {
            if x11_available {
                return LinuxWindowBackend::X11;
            }
            if wayland_available {
                return LinuxWindowBackend::Wayland;
            }
            return LinuxWindowBackend::Auto;
        }
    }

    if x11_available {
        LinuxWindowBackend::X11
    } else if wayland_available {
        LinuxWindowBackend::Wayland
    } else {
        LinuxWindowBackend::Auto
    }
}

#[cfg(any(target_os = "linux", test))]
fn is_valid_xft_dpi(dpi: f64) -> bool {
    dpi.is_finite() && dpi > 0.0 && (dpi / 96.0).is_normal()
}

#[cfg(any(target_os = "linux", test))]
fn match_xft_dpi_key_priority(key: &str) -> Option<u8> {
    match key.trim() {
        "Xft.dpi" => Some(3),
        "*Xft.dpi" | "*.Xft.dpi" | "Xft*dpi" | "?ft.dpi" => Some(2),
        "*dpi" | "*.dpi" | "?.dpi" => Some(1),
        _ => None,
    }
}

#[cfg(any(target_os = "linux", test))]
fn has_trailing_line_continuation(line: &str) -> bool {
    let trailing_backslashes = line.bytes().rev().take_while(|&b| b == b'\\').count();
    trailing_backslashes % 2 == 1
}

#[cfg(any(target_os = "linux", test))]
fn parse_xft_dpi_from_resource_manager(raw: &str) -> Option<f64> {
    let mut best: Option<(u8, String)> = None;
    let mut logical_line = String::new();

    let mut process_entry = |entry: &str| {
        let Some((key, value)) = entry.split_once(':') else {
            return;
        };
        let Some(priority) = match_xft_dpi_key_priority(key) else {
            return;
        };
        let should_replace = best
            .as_ref()
            .is_none_or(|(prev_prio, _)| priority >= *prev_prio);
        if should_replace {
            best = Some((priority, value.trim().to_string()));
        }
    };

    for raw_line in raw.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if logical_line.is_empty() {
            let trimmed_start = line.trim_start_matches([' ', '\t']);
            if trimmed_start.is_empty()
                || trimmed_start.starts_with('!')
                || trimmed_start.starts_with('#')
            {
                continue;
            }
        }
        if has_trailing_line_continuation(line) {
            logical_line.push_str(&line[..line.len() - 1]);
            continue;
        }
        if logical_line.is_empty() {
            process_entry(line);
        } else {
            logical_line.push_str(line);
            process_entry(&logical_line);
            logical_line.clear();
        }
    }

    if !logical_line.is_empty() {
        process_entry(&logical_line);
    }

    best.and_then(|(_, value)| value.parse::<f64>().ok())
        .filter(|&dpi| is_valid_xft_dpi(dpi))
}

#[cfg(any(target_os = "linux", test))]
fn parse_xft_dpi_from_xsettings(data: &[u8]) -> Option<f64> {
    fn take_bytes<'a>(buf: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
        if n > buf.len() {
            return None;
        }
        let (head, tail) = buf.split_at(n);
        *buf = tail;
        Some(head)
    }

    fn read_u16(buf: &mut &[u8], little_endian: bool) -> Option<u16> {
        let bytes: [u8; 2] = take_bytes(buf, 2)?.try_into().ok()?;
        Some(if little_endian {
            u16::from_le_bytes(bytes)
        } else {
            u16::from_be_bytes(bytes)
        })
    }

    fn read_i32(buf: &mut &[u8], little_endian: bool) -> Option<i32> {
        let bytes: [u8; 4] = take_bytes(buf, 4)?.try_into().ok()?;
        Some(if little_endian {
            i32::from_le_bytes(bytes)
        } else {
            i32::from_be_bytes(bytes)
        })
    }

    let &endian = data.first()?;
    let little_endian = match endian {
        b'l' => true,
        b'B' => false,
        _ => cfg!(target_endian = "little"),
    };
    let mut rest = data.get(8..)?;

    let total_settings = read_i32(&mut rest, little_endian)?;
    if total_settings <= 0 {
        return None;
    }

    for _ in 0..total_settings {
        let ty = *take_bytes(&mut rest, 1)?.first()?;
        let _pad = take_bytes(&mut rest, 1)?;
        let name_len = read_u16(&mut rest, little_endian)? as usize;
        let name = take_bytes(&mut rest, name_len)?;
        let name_pad = (4 - (name_len % 4)) % 4;
        let _ = take_bytes(&mut rest, name_pad)?;
        let _serial = take_bytes(&mut rest, 4)?;

        match ty {
            0 => {
                let val = read_i32(&mut rest, little_endian)?;
                if name == b"Xft/DPI" {
                    let dpi = f64::from(val) / 1024.0;
                    return is_valid_xft_dpi(dpi).then_some(dpi);
                }
            }
            1 => {
                let str_len = read_i32(&mut rest, little_endian)?;
                if str_len < 0 {
                    return None;
                }
                let str_len = str_len as usize;
                let _ = take_bytes(&mut rest, str_len)?;
                let str_pad = (4 - (str_len % 4)) % 4;
                let _ = take_bytes(&mut rest, str_pad)?;
            }
            2 => {
                let _ = take_bytes(&mut rest, 8)?;
            }
            _ => return None,
        }
    }
    None
}

#[cfg(any(target_os = "linux", test))]
fn resolve_linux_x11_scale_factor_override(
    backend: LinuxWindowBackend,
    winit_x11_scale_factor: Option<&str>,
    configured_xft_dpi: Option<f64>,
) -> Option<&'static str> {
    if backend != LinuxWindowBackend::X11 {
        return None;
    }
    if winit_x11_scale_factor.is_some_and(|v| !v.trim().is_empty()) {
        return None;
    }
    if configured_xft_dpi.is_some_and(is_valid_xft_dpi) {
        return None;
    }
    // Without explicit `WINIT_X11_SCALE_FACTOR` or `Xft.dpi`/`Xft/DPI` (common on
    // Hyprland/Niri/Sway XWayland and bare X11 WMs), winit falls back to raw EDID
    // millimeter DPI (~1.6x on 14" 1080p) instead of matching 1.0x compositor/toolkit scale.
    Some("1")
}

#[cfg(target_os = "linux")]
unsafe extern "C" fn ignore_x11_error(
    _display: *mut x11_dl::xlib::Display,
    _event: *mut x11_dl::xlib::XErrorEvent,
) -> std::os::raw::c_int {
    0
}

#[cfg(target_os = "linux")]
unsafe fn read_xft_dpi_from_xsettings_x11(
    xlib: &x11_dl::xlib::Xlib,
    display: *mut x11_dl::xlib::Display,
) -> Option<f64> {
    let screen = (xlib.XDefaultScreen)(display);
    let screen_atom_name = std::ffi::CString::new(format!("_XSETTINGS_S{screen}")).ok()?;
    let screen_atom = (xlib.XInternAtom)(display, screen_atom_name.as_ptr(), 1);
    if screen_atom == 0 {
        return None;
    }
    let owner = (xlib.XGetSelectionOwner)(display, screen_atom);
    if owner == 0 {
        return None;
    }
    let settings_atom = (xlib.XInternAtom)(display, c"_XSETTINGS_SETTINGS".as_ptr(), 1);
    if settings_atom == 0 {
        return None;
    }

    let prev_handler = (xlib.XSetErrorHandler)(Some(ignore_x11_error));
    let mut actual_type: std::os::raw::c_ulong = 0;
    let mut actual_format: std::os::raw::c_int = 0;
    let mut nitems: std::os::raw::c_ulong = 0;
    let mut bytes_after: std::os::raw::c_ulong = 0;
    let mut prop_ptr: *mut std::os::raw::c_uchar = std::ptr::null_mut();

    let status = (xlib.XGetWindowProperty)(
        display,
        owner,
        settings_atom,
        0,
        65536,
        0,
        settings_atom,
        &mut actual_type,
        &mut actual_format,
        &mut nitems,
        &mut bytes_after,
        &mut prop_ptr,
    );
    (xlib.XSetErrorHandler)(prev_handler);

    let result = if status == std::os::raw::c_int::from(x11_dl::xlib::Success)
        && actual_format == 8
        && !prop_ptr.is_null()
        && nitems > 0
    {
        let bytes = std::slice::from_raw_parts(prop_ptr, nitems as usize);
        parse_xft_dpi_from_xsettings(bytes)
    } else {
        None
    };

    if !prop_ptr.is_null() {
        (xlib.XFree)(prop_ptr.cast());
    }
    result
}

#[cfg(target_os = "linux")]
fn read_xft_dpi_from_xresources_files(has_root_resource_manager: bool) -> Option<f64> {
    let mut dpi = None;
    if !has_root_resource_manager {
        if let Some(home) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
            let home_path = std::path::PathBuf::from(home);
            for path in [home_path.join(".Xresources"), home_path.join(".Xdefaults")] {
                if let Ok(contents) = std::fs::read_to_string(&path) {
                    dpi = parse_xft_dpi_from_resource_manager(&contents);
                    break;
                }
            }
        }
    }
    if let Some(xenv) = std::env::var_os("XENVIRONMENT").filter(|v| !v.is_empty()) {
        if let Ok(contents) = std::fs::read_to_string(xenv) {
            if let Some(override_dpi) = parse_xft_dpi_from_resource_manager(&contents) {
                dpi = Some(override_dpi);
            }
        }
    }
    dpi
}

#[cfg(target_os = "linux")]
fn probe_x11_display_and_xft_dpi(query_dpi: bool) -> (bool, Option<f64>) {
    let Ok(xlib) = x11_dl::xlib::Xlib::open() else {
        return (false, None);
    };
    // SAFETY: `XInitThreads` is idempotent and runs on the main thread before any
    // worker thread or `winit` display initialization. `XResourceManagerString`
    // returns an Xlib-owned pointer valid until `XCloseDisplay` (must not be freed).
    unsafe {
        (xlib.XInitThreads)();
        let display = (xlib.XOpenDisplay)(std::ptr::null());
        if display.is_null() {
            return (false, None);
        }

        let xft_dpi = if query_dpi {
            let from_xsettings = read_xft_dpi_from_xsettings_x11(&xlib, display);
            if from_xsettings.is_some() {
                from_xsettings
            } else {
                let rm_ptr = (xlib.XResourceManagerString)(display);
                let (has_root_rm, from_rm) = if rm_ptr.is_null() {
                    (false, None)
                } else {
                    let rm_text = std::ffi::CStr::from_ptr(rm_ptr).to_string_lossy();
                    let has_rm = !rm_text.is_empty();
                    let parsed = has_rm
                        .then(|| parse_xft_dpi_from_resource_manager(&rm_text))
                        .flatten();
                    (has_rm, parsed)
                };
                from_rm.or_else(|| read_xft_dpi_from_xresources_files(has_root_rm))
            }
        } else {
            None
        };

        (xlib.XCloseDisplay)(display);
        (true, xft_dpi)
    }
}

#[cfg(target_os = "linux")]
fn resolve_linux_window_backend_and_xft_dpi(query_dpi: bool) -> (LinuxWindowBackend, Option<f64>) {
    let has_env = |name: &str| {
        std::env::var(name)
            .ok()
            .is_some_and(|v| !v.trim().is_empty())
    };
    let wayland_available = has_env("WAYLAND_DISPLAY") || has_env("WAYLAND_SOCKET");
    let display_set = has_env("DISPLAY");
    let requested = std::env::var("WINIT_UNIX_BACKEND").ok();

    let explicit_wayland = requested
        .as_deref()
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("wayland"));
    let (x11_available, xft_dpi) = if (explicit_wayland && wayland_available) || !display_set {
        (false, None)
    } else {
        probe_x11_display_and_xft_dpi(query_dpi)
    };

    let backend =
        select_linux_window_backend_from(wayland_available, x11_available, requested.as_deref());
    (backend, xft_dpi)
}

#[cfg(target_os = "linux")]
fn configure_linux_x11_scale_factor() -> LinuxWindowBackend {
    let explicit_scale = std::env::var("WINIT_X11_SCALE_FACTOR").ok();
    let need_dpi_probe = !explicit_scale
        .as_deref()
        .is_some_and(|v| !v.trim().is_empty());
    let (backend, xft_dpi) = resolve_linux_window_backend_and_xft_dpi(need_dpi_probe);

    if let Some(scale) =
        resolve_linux_x11_scale_factor_override(backend, explicit_scale.as_deref(), xft_dpi)
    {
        // SAFETY: Called on the main thread before spawning `polban-single-instance`
        // or creating `winit`'s `EventLoop`.
        std::env::set_var("WINIT_X11_SCALE_FACTOR", scale);
    }

    backend
}

fn main() -> eframe::Result<()> {
    let args = parse_args(std::env::args());

    if let Some(bad) = args.unknown {
        eprintln!("Argumen tidak dikenal: {bad}");
        print_help();
        std::process::exit(2);
    }

    if args.show_help {
        print_help();
        return Ok(());
    }

    if args.show_version {
        println!("polban-absen-gui {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    absen_gui::updater::cleanup_old_executables();

    let instance_bind = bind_single_instance_at(SINGLE_INSTANCE_ADDR);
    if matches!(instance_bind, SingleInstanceBind::SecondaryNotified) {
        return Ok(());
    }

    // Configure X11 scaling on the main thread while still single-threaded
    // (before spawning `polban-single-instance` or creating the `winit` event loop).
    #[cfg(target_os = "linux")]
    let linux_backend = configure_linux_x11_scale_factor();

    let instance_guard = match start_single_instance_listener(instance_bind) {
        SingleInstanceOutcome::Primary(handle) => Some(Arc::new(Mutex::new(Some(handle)))),
        SingleInstanceOutcome::SecondaryNotified => return Ok(()),
        SingleInstanceOutcome::Unavailable => None,
    };

    let show_bridge = instance_guard.as_ref().and_then(|shared| {
        let guard = shared.lock().ok()?;
        let h = guard.as_ref()?;
        Some((
            Arc::clone(&h.ui_ctx),
            Arc::clone(&h.win32_msg_hwnd),
            Arc::clone(&h.show_flag),
        ))
    });
    let release_cb: Option<Arc<dyn Fn() + Send + Sync>> = instance_guard.as_ref().map(|shared| {
        let shared_clone = Arc::clone(shared);
        let cb: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            if let Ok(mut guard) = shared_clone.lock() {
                if let Some(mut handle) = guard.take() {
                    handle.release();
                }
            }
        });
        cb
    });

    let viewport = egui::ViewportBuilder::default()
        .with_title("Polban Presensi")
        .with_inner_size([1040.0, 680.0])
        .with_min_inner_size([640.0, 520.0])
        .with_icon(icon::egui_icon(256, None));
    #[cfg(target_os = "linux")]
    let viewport = viewport.with_app_id("polban-presensi");

    #[cfg(target_os = "linux")]
    let event_loop_builder: Option<eframe::EventLoopBuilderHook> = {
        Some(Box::new(move |builder| {
            use winit::platform::wayland::EventLoopBuilderExtWayland as _;
            use winit::platform::x11::EventLoopBuilderExtX11 as _;
            match linux_backend {
                LinuxWindowBackend::X11 => {
                    builder.with_x11();
                }
                LinuxWindowBackend::Wayland => {
                    builder.with_wayland();
                }
                LinuxWindowBackend::Auto => {}
            }
        }))
    };
    #[cfg(not(target_os = "linux"))]
    let event_loop_builder: Option<eframe::EventLoopBuilderHook> = None;

    let options = eframe::NativeOptions {
        viewport,
        event_loop_builder,
        ..Default::default()
    };

    let start_minimized = args.minimized;
    let res = eframe::run_native(
        "Polban Presensi",
        options,
        Box::new(move |cc| {
            if let Some((ctx_slot, _, _)) = &show_bridge {
                if let Ok(mut guard) = ctx_slot.lock() {
                    *guard = Some(cc.egui_ctx.clone());
                }
            }
            let mut app = DesktopApp::new(cc, start_minimized);
            if let Some((_, msg_hwnd_slot, flag)) = show_bridge {
                #[cfg(target_os = "windows")]
                if let Some(hwnd) = app.win32_msg_hwnd() {
                    msg_hwnd_slot.store(hwnd, Ordering::Release);
                }
                let _ = &msg_hwnd_slot;
                app.set_external_show_flag(flag);
            }
            if let Some(cb) = release_cb {
                app.set_release_instance_lock(cb);
            }
            Ok(Box::new(app))
        }),
    );
    drop(instance_guard);
    res
}

#[cfg(test)]
mod tests {
    use super::{
        acquire_single_instance_at, parse_args, parse_xft_dpi_from_resource_manager,
        parse_xft_dpi_from_xsettings, resolve_linux_x11_scale_factor_override,
        select_linux_window_backend_from, LinuxWindowBackend, SingleInstanceOutcome,
    };
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    #[test]
    fn parses_xft_dpi_from_resource_manager_entries() {
        assert_eq!(parse_xft_dpi_from_resource_manager(""), None);
        assert_eq!(
            parse_xft_dpi_from_resource_manager("Xft.antialias:\t1\nXft.dpi:\t96\n"),
            Some(96.0)
        );
        assert_eq!(
            parse_xft_dpi_from_resource_manager("  Xft.dpi : \t 120.5  \r\n"),
            Some(120.5)
        );
        assert_eq!(
            parse_xft_dpi_from_resource_manager("! Xft.dpi: 192\n# Xft.dpi: 192\nXft.dpi: 144\n"),
            Some(144.0)
        );
        assert_eq!(
            parse_xft_dpi_from_resource_manager("Xft.dpi: \\\n  144\n"),
            Some(144.0)
        );
        // Line continuation on another resource must not accidentally expose Xft.dpi.
        assert_eq!(
            parse_xft_dpi_from_resource_manager("Some.value: hello \\\nXft.dpi: 192\n"),
            None
        );
        // Unrelated app-specific DPI entries must not match Xft.dpi.
        assert_eq!(
            parse_xft_dpi_from_resource_manager("rofi.dpi: 144\nURxvt.dpi: 192\nXft.dpi_alt: 144"),
            None
        );
        // Specificity & override rules.
        assert_eq!(
            parse_xft_dpi_from_resource_manager("Xft.dpi: 144\n*.dpi: 96\n"),
            Some(144.0)
        );
        assert_eq!(
            parse_xft_dpi_from_resource_manager("*.dpi: 96\nXft.dpi: 144\n"),
            Some(144.0)
        );
        assert_eq!(
            parse_xft_dpi_from_resource_manager("*Xft.dpi: 120\n"),
            Some(120.0)
        );
        assert_eq!(
            parse_xft_dpi_from_resource_manager("Xft.dpi: 96\nXft.dpi: 144\n"),
            Some(144.0)
        );
        // Invalid, zero, negative, or non-finite values return None.
        for bad in [
            "Xft.dpi: 0",
            "Xft.dpi: -96",
            "Xft.dpi: NaN",
            "Xft.dpi: inf",
            "Xft.dpi: -inf",
            "Xft.dpi: auto",
            "Xft.dpi:",
            "Xft.dpi: 144\nXft.dpi: 0",
        ] {
            assert_eq!(
                parse_xft_dpi_from_resource_manager(bad),
                None,
                "invalid resource entry {bad:?} should return None"
            );
        }
    }

    fn build_xsettings_payload(little_endian: bool, entries: &[(u8, &[u8], &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(if little_endian { b'l' } else { b'B' });
        out.extend_from_slice(&[0u8; 3]);
        out.extend_from_slice(&1u32.to_le_bytes()); // serial
        let count = entries.len() as i32;
        out.extend_from_slice(&if little_endian {
            count.to_le_bytes()
        } else {
            count.to_be_bytes()
        });
        for &(ty, name, value_bytes) in entries {
            out.push(ty);
            out.push(0); // pad
            let name_len = name.len() as u16;
            out.extend_from_slice(&if little_endian {
                name_len.to_le_bytes()
            } else {
                name_len.to_be_bytes()
            });
            out.extend_from_slice(name);
            let name_pad = (4 - (name.len() % 4)) % 4;
            out.extend(std::iter::repeat_n(0u8, name_pad));
            out.extend_from_slice(&1u32.to_le_bytes()); // serial
            out.extend_from_slice(value_bytes);
        }
        out
    }

    #[test]
    fn parses_xft_dpi_from_xsettings_payload() {
        assert_eq!(parse_xft_dpi_from_xsettings(&[]), None);
        assert_eq!(parse_xft_dpi_from_xsettings(b"l\0\0\0"), None);

        // Little-endian with preceding String and Color entries before Xft/DPI = 144 * 1024.
        let mut str_val = Vec::new();
        str_val.extend_from_slice(&3i32.to_le_bytes());
        str_val.extend_from_slice(b"rgb\0");
        let le_payload = build_xsettings_payload(
            true,
            &[
                (1, b"Xft/RGBA", &str_val),
                (2, b"Net/FallbackColor", &[0u8; 8]),
                (0, b"Xft/DPI", &(144 * 1024i32).to_le_bytes()),
            ],
        );
        assert_eq!(parse_xft_dpi_from_xsettings(&le_payload), Some(144.0));

        // Big-endian Xft/DPI = 120 * 1024.
        let be_payload =
            build_xsettings_payload(false, &[(0, b"Xft/DPI", &(120 * 1024i32).to_be_bytes())]);
        assert_eq!(parse_xft_dpi_from_xsettings(&be_payload), Some(120.0));

        // Unset sentinel (-1) or zero DPI returns None.
        let unset_payload =
            build_xsettings_payload(true, &[(0, b"Xft/DPI", &(-1i32).to_le_bytes())]);
        assert_eq!(parse_xft_dpi_from_xsettings(&unset_payload), None);

        let zero_payload = build_xsettings_payload(true, &[(0, b"Xft/DPI", &0i32.to_le_bytes())]);
        assert_eq!(parse_xft_dpi_from_xsettings(&zero_payload), None);
    }

    #[test]
    fn linux_x11_scale_override_respects_user_env_xft_dpi_and_backend() {
        // Native Wayland and Auto never receive X11 scale override.
        assert_eq!(
            resolve_linux_x11_scale_factor_override(LinuxWindowBackend::Wayland, None, None),
            None
        );
        assert_eq!(
            resolve_linux_x11_scale_factor_override(LinuxWindowBackend::Auto, None, None),
            None
        );

        // Explicit WINIT_X11_SCALE_FACTOR always wins.
        for user_val in ["1", "1.5", "2", "randr"] {
            assert_eq!(
                resolve_linux_x11_scale_factor_override(
                    LinuxWindowBackend::X11,
                    Some(user_val),
                    None
                ),
                None
            );
            assert_eq!(
                resolve_linux_x11_scale_factor_override(
                    LinuxWindowBackend::X11,
                    Some(user_val),
                    Some(144.0)
                ),
                None
            );
        }

        // Valid Xft.dpi (e.g. GNOME/KDE/XFCE/xrdb) lets winit scale via Xft.dpi.
        for dpi in [96.0, 120.0, 144.0, 192.0] {
            assert_eq!(
                resolve_linux_x11_scale_factor_override(LinuxWindowBackend::X11, None, Some(dpi)),
                None
            );
        }

        // Absent or invalid Xft.dpi on X11/XWayland defaults scale to 1.0 instead of raw EDID mm.
        assert_eq!(
            resolve_linux_x11_scale_factor_override(LinuxWindowBackend::X11, None, None),
            Some("1")
        );
        assert_eq!(
            resolve_linux_x11_scale_factor_override(LinuxWindowBackend::X11, Some(""), None),
            Some("1")
        );
        assert_eq!(
            resolve_linux_x11_scale_factor_override(LinuxWindowBackend::X11, Some("  "), None),
            Some("1")
        );
        for bad_dpi in [0.0, -96.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                resolve_linux_x11_scale_factor_override(
                    LinuxWindowBackend::X11,
                    None,
                    Some(bad_dpi)
                ),
                Some("1")
            );
        }
    }

    #[test]
    fn linux_window_backend_selection_prefers_x11_when_reachable_and_falls_back_safely() {
        // Pure X11 desktop -> X11.
        assert_eq!(
            select_linux_window_backend_from(false, true, None),
            LinuxWindowBackend::X11
        );

        // Wayland + XWayland (Hyprland, Niri, Sway, KDE, GNOME) -> X11/XWayland by default.
        assert_eq!(
            select_linux_window_backend_from(true, true, None),
            LinuxWindowBackend::X11
        );

        // Pure Wayland without XWayland / broken DISPLAY -> Wayland fallback.
        assert_eq!(
            select_linux_window_backend_from(true, false, None),
            LinuxWindowBackend::Wayland
        );

        // Explicit WINIT_UNIX_BACKEND=wayland honors Wayland when available.
        assert_eq!(
            select_linux_window_backend_from(true, true, Some("wayland")),
            LinuxWindowBackend::Wayland
        );
        assert_eq!(
            select_linux_window_backend_from(true, true, Some(" WAYLAND ")),
            LinuxWindowBackend::Wayland
        );

        // Explicit WINIT_UNIX_BACKEND=wayland on X11-only falls back safely to X11.
        assert_eq!(
            select_linux_window_backend_from(false, true, Some("wayland")),
            LinuxWindowBackend::X11
        );

        // Explicit WINIT_UNIX_BACKEND=x11 uses X11 when reachable, otherwise falls back to Wayland.
        assert_eq!(
            select_linux_window_backend_from(true, true, Some("x11")),
            LinuxWindowBackend::X11
        );
        assert_eq!(
            select_linux_window_backend_from(true, false, Some("x11")),
            LinuxWindowBackend::Wayland
        );

        // Headless (neither display backend present) -> Auto.
        assert_eq!(
            select_linux_window_backend_from(false, false, None),
            LinuxWindowBackend::Auto
        );
    }

    #[test]
    fn parses_supported_flags() {
        let a = parse_args(["polban-absen-gui", "--minimized"]);
        assert!(a.minimized);
        assert!(!a.show_help);
        assert!(!a.show_version);
        assert!(a.unknown.is_none());

        let h = parse_args(["polban-absen-gui", "--help"]);
        assert!(h.show_help);

        let v = parse_args(["polban-absen-gui", "--version"]);
        assert!(v.show_version);

        let u = parse_args(["polban-absen-gui", "--bad"]);
        assert_eq!(u.unknown.as_deref(), Some("--bad"));
    }

    #[test]
    fn single_instance_guard_notifies_primary_on_second_launch() {
        let primary = match acquire_single_instance_at("127.0.0.1:0") {
            SingleInstanceOutcome::Primary(h) => h,
            _ => panic!("expected primary single-instance lock"),
        };
        let addr = primary.local_addr.expect("bound local addr").to_string();
        assert!(!primary.show_flag.load(Ordering::Acquire));

        let second = acquire_single_instance_at(&addr);
        assert!(matches!(second, SingleInstanceOutcome::SecondaryNotified));

        let deadline = Instant::now() + Duration::from_secs(2);
        while !primary.show_flag.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(primary.show_flag.load(Ordering::Acquire));
    }

    #[test]
    fn single_instance_release_frees_port_for_restart() {
        let mut primary = match acquire_single_instance_at("127.0.0.1:0") {
            SingleInstanceOutcome::Primary(h) => h,
            _ => panic!("expected primary single-instance lock"),
        };
        let addr = primary.local_addr.expect("bound local addr").to_string();
        primary.release();

        let respawned = acquire_single_instance_at(&addr);
        assert!(
            matches!(respawned, SingleInstanceOutcome::Primary(_)),
            "port must be immediately reusable after release() for self-update restart"
        );
    }
}

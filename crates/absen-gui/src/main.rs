#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Desktop GUI entrypoint for `polban-absen-gui`.

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
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
    show_flag: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
}

impl Drop for SingleInstanceHandle {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(addr) = self.local_addr {
            let _ = TcpStream::connect_timeout(&addr, Duration::from_millis(100));
        }
    }
}

fn acquire_single_instance_at(addr: &str) -> SingleInstanceOutcome {
    match TcpListener::bind(addr) {
        Ok(listener) => {
            let local_addr = listener.local_addr().ok();
            let ui_ctx: Arc<Mutex<Option<egui::Context>>> = Arc::new(Mutex::new(None));
            let show_flag = Arc::new(AtomicBool::new(false));
            let shutdown = Arc::new(AtomicBool::new(false));

            let thread_ctx = Arc::clone(&ui_ctx);
            let thread_flag = Arc::clone(&show_flag);
            let thread_shutdown = Arc::clone(&shutdown);

            if std::thread::Builder::new()
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
                })
                .is_err()
            {
                return SingleInstanceOutcome::Unavailable;
            }

            SingleInstanceOutcome::Primary(SingleInstanceHandle {
                local_addr,
                ui_ctx,
                show_flag,
                shutdown,
            })
        }
        Err(err) if err.kind() == ErrorKind::AddrInUse => {
            if let Ok(sock_addr) = addr.parse::<SocketAddr>() {
                if let Ok(mut stream) =
                    TcpStream::connect_timeout(&sock_addr, Duration::from_millis(500))
                {
                    let _ = stream.write_all(b"SHOW\n");
                    let _ = stream.flush();
                }
            }
            SingleInstanceOutcome::SecondaryNotified
        }
        Err(_) => SingleInstanceOutcome::Unavailable,
    }
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

    #[cfg(target_os = "linux")]
    if let Err(err) = gtk::init() {
        eprintln!("Peringatan: gagal menginisialisasi GTK untuk tray: {err}");
    }

    let instance_guard = match acquire_single_instance_at(SINGLE_INSTANCE_ADDR) {
        SingleInstanceOutcome::Primary(handle) => Some(handle),
        SingleInstanceOutcome::SecondaryNotified => return Ok(()),
        SingleInstanceOutcome::Unavailable => None,
    };

    let show_bridge = instance_guard
        .as_ref()
        .map(|h| (Arc::clone(&h.ui_ctx), Arc::clone(&h.show_flag)));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Polban Presensi")
            .with_inner_size([980.0, 660.0])
            .with_min_inner_size([420.0, 480.0])
            .with_icon(icon::egui_icon(64, None)),
        ..Default::default()
    };

    let start_minimized = args.minimized;
    let res = eframe::run_native(
        "Polban Presensi",
        options,
        Box::new(move |cc| {
            if let Some((ctx_slot, _)) = &show_bridge {
                if let Ok(mut guard) = ctx_slot.lock() {
                    *guard = Some(cc.egui_ctx.clone());
                }
            }
            let mut app = DesktopApp::new(cc, start_minimized);
            if let Some((_, flag)) = show_bridge {
                app.set_external_show_flag(flag);
            }
            Ok(Box::new(app))
        }),
    );
    drop(instance_guard);
    res
}

#[cfg(test)]
mod tests {
    use super::{acquire_single_instance_at, parse_args, SingleInstanceOutcome};
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

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
}

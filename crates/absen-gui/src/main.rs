#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Desktop GUI entrypoint for `polban-absen-gui`.

use absen_gui::{icon, DesktopApp};

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

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Polban Presensi")
            .with_inner_size([980.0, 660.0])
            .with_min_inner_size([420.0, 480.0])
            .with_icon(icon::egui_icon(64, None)),
        ..Default::default()
    };

    let start_minimized = args.minimized;
    eframe::run_native(
        "Polban Presensi",
        options,
        Box::new(move |cc| Ok(Box::new(DesktopApp::new(cc, start_minimized)))),
    )
}

#[cfg(test)]
mod tests {
    use super::parse_args;

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
}

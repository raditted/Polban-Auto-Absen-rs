//! CLI entrypoint for `polban-absen`.

mod dashboard;
mod platform;

use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};

use absen_core::{AbsenClient, Config, DumpError, EngineError};
use anyhow::Context;
use clap::Parser;
use dashboard::plain::PlainReporter;

#[derive(Debug, Parser)]
#[command(
    name = "polban-absen",
    version,
    about = "Polban Auto Absen (Rust v3.0)"
)]
pub struct Cli {
    /// Engine yang dipakai (hanya 'http' didukung di versi Rust)
    #[arg(long, value_name = "ENGINE")]
    pub engine: Option<String>,

    /// Sekali jalan lalu exit
    #[arg(long)]
    pub once: bool,

    /// Simpan HTML halaman absen lalu exit (diagnostik)
    #[arg(long, num_args = 0..=1, value_name = "FILE")]
    pub dump: Option<Option<PathBuf>>,

    /// Mode plain text (tanpa live dashboard)
    #[arg(long)]
    pub plain: bool,

    /// Dashboard digambar di layar biasa (tanpa alternate screen)
    #[arg(long)]
    pub no_alt_screen: bool,

    /// Pakai box ASCII untuk terminal lama
    #[arg(long)]
    pub ascii: bool,
}

fn validate_engine(engine: &str) -> Result<(), i32> {
    if engine.eq_ignore_ascii_case("pw") {
        eprintln!(
            "ERROR: engine 'pw' (Playwright) tidak didukung di versi Rust. Gunakan --engine http."
        );
        return Err(1);
    }
    if !engine.eq_ignore_ascii_case("http") {
        eprintln!("ERROR: engine '{engine}' tidak dikenal. Pilihan yang tersedia: http");
        return Err(1);
    }
    Ok(())
}

fn default_dump_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("absen-dump.html")))
        .unwrap_or_else(|| PathBuf::from("absen-dump.html"))
}

fn run_cli() -> anyhow::Result<i32> {
    let cli = Cli::parse();

    if let Some(ref eng) = cli.engine {
        if let Err(code) = validate_engine(eng) {
            return Ok(code);
        }
    }

    let (mut cfg, env_path_used) = Config::load().context("gagal memuat konfigurasi")?;
    let selected_engine = cli.engine.as_deref().unwrap_or(&cfg.engine).to_string();
    if let Err(code) = validate_engine(&selected_engine) {
        return Ok(code);
    }
    cfg.engine = selected_engine;

    if cfg.username.is_empty() || cfg.password.is_empty() {
        eprintln!("ERROR: POLBAN_USERNAME / POLBAN_PASSWORD belum terisi.\n");
        if let Some(path) = env_path_used {
            eprintln!(
                "  .env ditemukan di: {} (tapi nilai kosong)",
                path.display()
            );
        } else {
            eprintln!("  .env tidak ditemukan. Salin .env.example -> .env lalu isi.");
        }
        return Ok(1);
    }

    let client = AbsenClient::new(&cfg.base_url).context("gagal membuat HTTP client")?;

    if let Some(dump_arg) = cli.dump {
        let out_path = dump_arg.unwrap_or_else(default_dump_path);
        return match client.dump_absen_html(&cfg, &out_path) {
            Ok(saved) => {
                println!("HTML halaman absen disimpan ke: {}", saved.display());
                Ok(0)
            }
            Err(DumpError::LoginFailed) => {
                eprintln!("Login GAGAL -- cek NIM/password.");
                Ok(2)
            }
            Err(e) => {
                eprintln!("ERROR: {e}");
                Ok(1)
            }
        };
    }

    // F4 will attach TUI dashboard when `!use_plain`.
    let _use_plain = cli.plain || !std::io::stdout().is_terminal();
    let _alt_screen = !cli.no_alt_screen && !cli.once;
    let _ascii = cli.ascii;

    let reporter = Arc::new(PlainReporter::new(&cfg.engine, cfg.interval));
    reporter.start();

    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
    ctrlc::set_handler(move || {
        let _ = shutdown_tx.send(());
    })
    .context("gagal memasang handler Ctrl+C")?;

    let loop_mode = !cli.once;
    let cfg_clone = cfg.clone();
    let client_clone = client.clone();
    let reporter_clone = Arc::clone(&reporter);

    let worker = std::thread::spawn(move || {
        absen_core::run(
            &cfg_clone,
            &client_clone,
            loop_mode,
            &*reporter_clone,
            &shutdown_rx,
        )
    });

    let engine_res = worker
        .join()
        .map_err(|_| anyhow::anyhow!("worker thread mengalami panic"))?;

    reporter.render_summary();

    match engine_res {
        Ok(()) => {
            if cli.once {
                println!("[AutoAbsen] Selesai (sekali jalan).");
            } else {
                println!("[AutoAbsen] Dihentikan. Sampai jumpa!");
            }
            Ok(0)
        }
        Err(EngineError::LoginFailed) => Ok(2),
        Err(e) => {
            eprintln!("ERROR: {e}");
            Ok(1)
        }
    }
}

fn main() {
    match run_cli() {
        Ok(code) => {
            if code != 0 {
                std::process::exit(code);
            }
        }
        Err(e) => {
            eprintln!("ERROR: {e:#}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_help_and_flags_parse_cleanly() {
        Cli::command().debug_assert();

        let parsed = Cli::try_parse_from([
            "polban-absen",
            "--engine",
            "http",
            "--once",
            "--plain",
            "--no-alt-screen",
            "--ascii",
            "--dump",
            "out.html",
        ])
        .unwrap();
        assert_eq!(parsed.engine.as_deref(), Some("http"));
        assert!(parsed.once);
        assert!(parsed.plain);
        assert!(parsed.no_alt_screen);
        assert!(parsed.ascii);
        assert_eq!(parsed.dump, Some(Some(PathBuf::from("out.html"))));

        let dump_no_arg = Cli::try_parse_from(["polban-absen", "--dump", "--once"]).unwrap();
        assert_eq!(dump_no_arg.dump, Some(None));
        assert!(dump_no_arg.once);

        let pw_arg = Cli::try_parse_from(["polban-absen", "--engine", "pw"]).unwrap();
        assert_eq!(pw_arg.engine.as_deref(), Some("pw"));
        assert_eq!(validate_engine("pw"), Err(1));
    }
}

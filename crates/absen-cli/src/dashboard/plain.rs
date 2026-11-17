//! Plain-text reporter matching `PlainDashboard` in `windows/tui.py`.

use std::io::Write;
use std::sync::Mutex;

use absen_core::{Event, Reporter};
use chrono::Local;

use crate::dashboard::state::DashboardState;
use crate::platform;

struct PlainInner {
    state: DashboardState,
    jadwal_printed: bool,
}

/// Line-oriented stdout reporter for `--plain`, non-TTY, or `--once` fallback.
pub struct PlainReporter {
    inner: Mutex<PlainInner>,
}

impl PlainReporter {
    pub fn new(engine: impl Into<String>, interval: u64) -> Self {
        let mut state = DashboardState::default();
        state.engine = engine.into();
        state.interval = interval;
        Self {
            inner: Mutex::new(PlainInner {
                state,
                jadwal_printed: false,
            }),
        }
    }

    pub fn start(&self) {
        platform::print_ascii_banner();
    }

    pub fn render_summary(&self) {
        let Ok(guard) = self.inner.lock() else {
            return;
        };
        let st = &guard.state;
        println!(
            "\nRingkasan: {} siklus | {} berhasil | {} gagal",
            st.cycle_count, st.stat_ok, st.stat_fail
        );
        let _ = std::io::stdout().flush();
    }
}

impl Reporter for PlainReporter {
    fn emit(&self, event: Event) {
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        let outcome = guard.state.apply(event);

        if outcome.user_info_updated {
            let st = &guard.state;
            let info = &st.user_info;
            let garis = "=".repeat(56);
            println!("{garis}");
            if !info.nama.is_empty() {
                println!("  Nama    : {}", info.nama);
            }
            let nim = if info.nim.is_empty() { "-" } else { &info.nim };
            println!("  NIM     : {nim}");
            if !info.jurusan.is_empty() {
                println!("  Jurusan : {}", info.jurusan);
            }
            println!("  Waktu   : {}", Local::now().format("%Y-%m-%d %H:%M:%S"));
            println!("  Engine  : {}   Interval : {}s", st.engine, st.interval);
            println!("{garis}");
            let _ = std::io::stdout().flush();
        }

        if let Some(entry) = outcome.new_log {
            println!(
                "[{}] {} [{}] {}",
                entry.timestamp,
                entry.level.tag(),
                entry.code,
                entry.message
            );
            let _ = std::io::stdout().flush();
        }

        if outcome.jadwal_updated && (outcome.jadwal_changed || !guard.jadwal_printed) {
            guard.jadwal_printed = true;
            print_jadwal_list(&guard.state);
        }
    }
}

fn print_jadwal_list(st: &DashboardState) {
    println!("[{}] Jadwal hari ini:", Local::now().format("%H:%M:%S"));
    if st.jadwal.is_empty() {
        println!("     (tidak ada baris jadwal terbaca)");
    } else {
        for (idx, mk) in st.jadwal.iter().enumerate() {
            let label = mk.status.status_label();
            let jam = if mk.jam.is_empty() {
                String::new()
            } else {
                format!("  {}", mk.jam)
            };
            let dosen = if mk.dosen.is_empty() {
                String::new()
            } else {
                format!("  [{}]", mk.dosen)
            };
            println!(
                "     {}. {}{}  -> {}{}",
                idx + 1,
                mk.nama,
                jam,
                label,
                dosen
            );
        }
    }
    println!();
    let _ = std::io::stdout().flush();
}

//! Attendance polling & retry engine with interruptible sleep and session re-login.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::Duration;

use chrono::Local;
use rand::Rng;
use thiserror::Error;

use crate::client::{AbsenClient, ClientError, LoginOutcome};
use crate::config::Config;
use crate::event::{Event, Reporter};
use crate::model::{Action, Color, LogLevel, Skip};
use crate::parser::{find_rows, is_login_page, parse_user_header};
use crate::state::ClickedState;

const FAILURE_MARKERS: [&[u8]; 8] = [
    b"gagal",
    b"error",
    b"tidak bisa",
    b"bukan waktu",
    b"sudah absen",
    b"belum dibuka",
    b"invalid",
    b"false",
];

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("login gagal -- kredensial ditolak")]
    LoginFailed,
    #[error(transparent)]
    Client(#[from] ClientError),
}

/// Fast, zero-allocation heuristic matching Python `_response_indicates_success`:
/// returns `true` if status is 2xx and `body` contains none of the CodeIgniter
/// failure markers (checked ASCII case-insensitively).
pub fn response_indicates_success(status: u16, body: &str) -> bool {
    if !(200..300).contains(&status) {
        return false;
    }
    let hay = body.as_bytes();
    for needle in FAILURE_MARKERS {
        if hay.len() >= needle.len()
            && hay
                .windows(needle.len())
                .any(|w| w.eq_ignore_ascii_case(needle))
        {
            return false;
        }
    }
    true
}

fn truncate_str(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

fn emit_log(reporter: &dyn Reporter, code: &str, message: impl Into<String>, level: LogLevel) {
    reporter.emit(Event::Log {
        code: code.to_string(),
        message: message.into(),
        level,
    });
}

/// Tracks schedule status across scan cycles to detect meaningful changes for `JADWAL` logs.
#[derive(Default)]
struct JadwalTracker {
    prev_states: HashMap<String, Color>,
    absen_sukses: HashSet<String>,
}

impl JadwalTracker {
    fn update(&mut self, actions: &[Action], skips: &[Skip]) -> bool {
        let mut current: HashMap<String, Color> =
            HashMap::with_capacity(actions.len() + skips.len());

        for skip in skips {
            let (nama_raw, _, _) = crate::text::split_sig(&skip.sig);
            let nama = truncate_str(nama_raw, 48).to_string();
            let color = if self.absen_sukses.contains(&nama) {
                Color::Green
            } else {
                skip.color
            };
            current.insert(nama, color);
        }

        for act in actions {
            let nama = truncate_str(&act.nama_mk, 48).to_string();
            let color = if self.absen_sukses.contains(&nama) {
                Color::Green
            } else {
                Color::Blue
            };
            current.insert(nama, color);
        }

        let mut changed = false;
        for (nama, color) in &current {
            if let Some(old) = self.prev_states.get(nama) {
                if old != color {
                    changed = true;
                }
            }
        }
        if !self.prev_states.is_empty() && self.prev_states.len() != current.len() {
            changed = true;
        }

        self.prev_states = current;
        changed
    }

    fn mark_success(&mut self, nama: &str) {
        let trimmed = nama.trim();
        if !trimmed.is_empty() {
            self.absen_sukses.insert(trimmed.to_string());
        }
    }
}

fn interruptible_sleep(shutdown_rx: &Receiver<()>, duration: Duration) -> bool {
    if duration.is_zero() {
        return matches!(
            shutdown_rx.try_recv(),
            Ok(()) | Err(TryRecvError::Disconnected)
        );
    }
    match shutdown_rx.recv_timeout(duration) {
        Ok(()) | Err(RecvTimeoutError::Disconnected) => true,
        Err(RecvTimeoutError::Timeout) => false,
    }
}

fn jitter_sleep(cfg: &Config, shutdown_rx: &Receiver<()>) -> bool {
    let (low, high) = if cfg.delay_min <= cfg.delay_max {
        (cfg.delay_min, cfg.delay_max)
    } else {
        (cfg.delay_max, cfg.delay_min)
    };
    if high <= 0.0 {
        return interruptible_sleep(shutdown_rx, Duration::ZERO);
    }
    let low = low.max(0.0);
    let secs = if (high - low).abs() < f64::EPSILON {
        low
    } else {
        rand::thread_rng().gen_range(low..=high)
    };
    interruptible_sleep(shutdown_rx, Duration::from_secs_f64(secs))
}

fn retry_wait_sleep(cfg: &Config, shutdown_rx: &Receiver<()>) -> bool {
    if cfg.retry_wait <= 0.0 {
        return interruptible_sleep(shutdown_rx, Duration::ZERO);
    }
    interruptible_sleep(shutdown_rx, Duration::from_secs_f64(cfg.retry_wait))
}

fn countdown_sleep(seconds: u64, reporter: &dyn Reporter, shutdown_rx: &Receiver<()>) -> bool {
    for remaining in (1..=seconds).rev() {
        reporter.emit(Event::Countdown(remaining.min(u32::MAX as u64) as u32));
        if interruptible_sleep(shutdown_rx, Duration::from_secs(1)) {
            return true;
        }
    }
    reporter.emit(Event::Countdown(0));
    false
}

fn perform_login(
    cfg: &Config,
    client: &AbsenClient,
    reporter: &dyn Reporter,
) -> Result<Option<String>, ClientError> {
    match client.login(cfg)? {
        LoginOutcome::Authenticated { html } => {
            reporter.emit(Event::AuthStatus("OK -- Authenticated".to_string()));
            emit_log(reporter, "LOGIN", "Authenticated", LogLevel::Ok);
            Ok(Some(html))
        }
        LoginOutcome::Failed { redirect_url } => {
            reporter.emit(Event::AuthStatus("GAGAL -- Kredensial salah".to_string()));
            emit_log(
                reporter,
                "LOGIN",
                format!("GAGAL -- dilempar ke {redirect_url}"),
                LogLevel::Fail,
            );
            Ok(None)
        }
    }
}

fn still_blue(client: &AbsenClient, cfg: &Config, action: &Action) -> bool {
    let Ok((_url, html)) = client.get_target(cfg) else {
        return true;
    };
    if is_login_page(&html) {
        return true;
    }
    let (actions, _) = find_rows(&html, &cfg.absen_endpoint());
    actions
        .iter()
        .any(|a| a.key == action.key || a.sig.starts_with(&action.key))
}

fn build_scan_summary(actions: &[Action], skips: &[Skip]) -> String {
    let mut green = 0usize;
    let mut orange = 0usize;
    let mut red = 0usize;
    for s in skips {
        match s.color {
            Color::Green => green += 1,
            Color::Orange => orange += 1,
            Color::Red => red += 1,
            Color::Blue | Color::Unknown => {}
        }
    }
    format!(
        "{} siap absen | {green} sudah absen | {orange} belum buka | {red} belum waktunya",
        actions.len()
    )
}

#[allow(clippy::too_many_arguments)]
fn run_once(
    cfg: &Config,
    client: &AbsenClient,
    clicked: &mut ClickedState,
    tracker: &mut JadwalTracker,
    reporter: &dyn Reporter,
    shutdown_rx: &Receiver<()>,
    pre_fetched_html: Option<String>,
    first_run: bool,
) -> Result<bool, ClientError> {
    let html = match pre_fetched_html {
        Some(h) if !is_login_page(&h) => h,
        _ => {
            let (_url, fetched) = client.get_target(cfg)?;
            if !is_login_page(&fetched) {
                fetched
            } else {
                emit_log(
                    reporter,
                    "SESSION",
                    "Sesi habis -- re-login...",
                    LogLevel::Warn,
                );
                reporter.emit(Event::AuthStatus("Sesi habis -- re-login...".to_string()));
                match perform_login(cfg, client, reporter)? {
                    Some(new_html) => new_html,
                    None => return Ok(false),
                }
            }
        }
    };

    let (actions, skips) = find_rows(&html, &cfg.absen_endpoint());
    let changed = tracker.update(&actions, &skips);
    let summary = build_scan_summary(&actions, &skips);

    reporter.emit(Event::Jadwal {
        actions: actions.clone(),
        skips,
    });
    let scan_time = Local::now().format("%H:%M:%S");
    reporter.emit(Event::LastScan(format!("{scan_time} ({summary})")));

    if first_run {
        emit_log(
            reporter,
            "SCAN",
            format!("Scan awal: {summary}"),
            LogLevel::Info,
        );
    } else if !actions.is_empty() {
        emit_log(
            reporter,
            "SCAN",
            format!("Ditemukan {} matkul siap absen!", actions.len()),
            LogLevel::Ok,
        );
    } else if changed {
        emit_log(
            reporter,
            "JADWAL",
            format!("Status jadwal berubah: {summary}"),
            LogLevel::Info,
        );
    }

    if actions.is_empty() {
        return Ok(true);
    }

    for action in actions {
        if interruptible_sleep(shutdown_rx, Duration::ZERO) {
            return Ok(true);
        }
        if clicked.contains(&action.sig) {
            continue;
        }
        emit_log(
            reporter,
            "ABSEN",
            format!("target: {}", truncate_str(&action.label, 70)),
            LogLevel::Info,
        );

        let mut done = false;
        for attempt in 1..=cfg.max_retry {
            if jitter_sleep(cfg, shutdown_rx) {
                return Ok(true);
            }
            let (status, body) = match client.post_absen(cfg, &action) {
                Ok(res) => res,
                Err(e) => {
                    emit_log(
                        reporter,
                        "ERROR",
                        format!(
                            "request gagal (percobaan {attempt}): {e} -- {}",
                            truncate_str(&action.sig, 40)
                        ),
                        LogLevel::Fail,
                    );
                    continue;
                }
            };

            if retry_wait_sleep(cfg, shutdown_rx) {
                return Ok(true);
            }

            if response_indicates_success(status, &body) {
                emit_log(
                    reporter,
                    "ABSEN",
                    format!(
                        "BERHASIL (percobaan {attempt}): {}",
                        truncate_str(&action.nama_mk, 55)
                    ),
                    LogLevel::Ok,
                );
                reporter.emit(Event::MarkResult {
                    sig: action.sig.clone(),
                    success: true,
                    nama: action.nama_mk.clone(),
                });
                tracker.mark_success(&action.nama_mk);
                if let Err(e) = clicked.insert_and_save(action.sig.clone()) {
                    emit_log(
                        reporter,
                        "STATE",
                        format!("gagal simpan state: {e}"),
                        LogLevel::Warn,
                    );
                }
                done = true;
                break;
            }

            if !still_blue(client, cfg, &action) {
                emit_log(
                    reporter,
                    "ABSEN",
                    format!(
                        "BERHASIL (percobaan {attempt}, dikonfirmasi): {}",
                        truncate_str(&action.nama_mk, 50)
                    ),
                    LogLevel::Ok,
                );
                reporter.emit(Event::MarkResult {
                    sig: action.sig.clone(),
                    success: true,
                    nama: action.nama_mk.clone(),
                });
                tracker.mark_success(&action.nama_mk);
                if let Err(e) = clicked.insert_and_save(action.sig.clone()) {
                    emit_log(
                        reporter,
                        "STATE",
                        format!("gagal simpan state: {e}"),
                        LogLevel::Warn,
                    );
                }
                done = true;
                break;
            }

            emit_log(
                reporter,
                "RETRY",
                format!(
                    "percobaan {attempt}/{} masih biru, tekan lagi -- {}",
                    cfg.max_retry,
                    truncate_str(&action.nama_mk, 45)
                ),
                LogLevel::Warn,
            );
        }

        if !done {
            emit_log(
                reporter,
                "ABSEN",
                format!(
                    "GAGAL setelah {} percobaan: {}. Kemungkinan request ditolak server / bukan waktu absen.",
                    cfg.max_retry,
                    truncate_str(&action.nama_mk, 50)
                ),
                LogLevel::Fail,
            );
            reporter.emit(Event::MarkResult {
                sig: action.sig,
                success: false,
                nama: action.nama_mk,
            });
        }
    }

    Ok(true)
}

/// Run the full HTTP attendance engine until completion (`!loop_mode`) or `shutdown_rx`.
pub fn run(
    cfg: &Config,
    client: &AbsenClient,
    loop_mode: bool,
    reporter: &dyn Reporter,
    shutdown_rx: &Receiver<()>,
) -> Result<(), EngineError> {
    reporter.emit(Event::Engine {
        name: cfg.engine.clone(),
        interval: cfg.interval,
    });

    let initial_html = match perform_login(cfg, client, reporter)? {
        Some(h) => h,
        None => return Err(EngineError::LoginFailed),
    };

    let user_info = parse_user_header(&initial_html, &cfg.username);
    reporter.emit(Event::UserInfo(user_info));

    if let Some(path) = &cfg.env_path_used {
        emit_log(
            reporter,
            "CONFIG",
            format!("{} (NIM {})", path.display(), cfg.username),
            LogLevel::Info,
        );
    }

    let mut clicked = ClickedState::load(&cfg.state_file);
    let mut tracker = JadwalTracker::default();
    let loop_label = if loop_mode { "True" } else { "False" };
    emit_log(
        reporter,
        "ENGINE",
        format!("HTTP aktif. interval={}s loop={loop_label}", cfg.interval),
        LogLevel::Info,
    );

    let mut first = true;
    let mut reusable_html = Some(initial_html);

    loop {
        if interruptible_sleep(shutdown_rx, Duration::ZERO) {
            break;
        }
        reporter.emit(Event::CycleIncrement);
        if let Err(e) = run_once(
            cfg,
            client,
            &mut clicked,
            &mut tracker,
            reporter,
            shutdown_rx,
            reusable_html.take(),
            first,
        ) {
            emit_log(reporter, "ERROR", format!("siklus: {e}"), LogLevel::Fail);
        }
        first = false;
        if !loop_mode {
            break;
        }
        if countdown_sleep(cfg.interval, reporter, shutdown_rx) {
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_indicates_success_checks_status_and_markers() {
        assert!(response_indicates_success(200, "OK"));
        assert!(response_indicates_success(201, r#"{"status":true}"#));
        assert!(!response_indicates_success(500, "OK"));
        assert!(!response_indicates_success(302, "OK"));
        assert!(!response_indicates_success(200, "Absen GAGAL diproses"));
        assert!(!response_indicates_success(200, "Error: timeout"));
        assert!(!response_indicates_success(200, "Tidak Bisa absen"));
        assert!(!response_indicates_success(200, "Bukan waktu kuliah"));
        assert!(!response_indicates_success(200, "Mahasiswa Sudah Absen"));
        assert!(!response_indicates_success(200, "Sesi belum dibuka"));
        assert!(!response_indicates_success(200, "Invalid token"));
        assert!(!response_indicates_success(200, r#"{"ok":false}"#));
    }
}

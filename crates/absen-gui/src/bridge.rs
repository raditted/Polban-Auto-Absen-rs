//! Thread-safe bridge between `absen-core` worker thread and the `egui` UI loop.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;

use absen_core::engine::{run_with_commands, EngineError};
use absen_core::text::{nama_dosen, split_sig};
use absen_core::{AbsenClient, Color, Config, EngineCommand, Event, LogLevel, Reporter, UserInfo};
use chrono::Local;

use crate::platform::Notifier;

const MAX_ACTIVITY_ITEMS: usize = 50;

/// High-level lifecycle status of the background attendance engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EngineStatus {
    #[default]
    Idle,
    Connecting,
    Monitoring,
    ScanningNow,
    Stopped,
    AuthFailed,
    NetworkError,
}

impl EngineStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Idle => "Siaga",
            Self::Connecting => "Menghubungkan",
            Self::Monitoring => "Memantau",
            Self::ScanningNow => "Memindai",
            Self::Stopped => "Dihentikan",
            Self::AuthFailed => "Login Gagal",
            Self::NetworkError => "Gangguan Jaringan",
        }
    }

    pub const fn is_active(self) -> bool {
        matches!(
            self,
            Self::Connecting | Self::Monitoring | Self::ScanningNow
        )
    }
}

/// UI-ready schedule item derived from `absen_core::Action` and `absen_core::Skip`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseItem {
    pub sig: String,
    pub nama_mk: String,
    pub dosen: String,
    pub jam: String,
    pub color: Color,
    pub status_label: String,
    pub reason: String,
    pub action_ready: bool,
}

/// Severity kind for GUI activity log rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityKind {
    Ok,
    Fail,
    Warn,
    Info,
    Skip,
}

impl From<LogLevel> for ActivityKind {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Ok => Self::Ok,
            LogLevel::Fail => Self::Fail,
            LogLevel::Warn => Self::Warn,
            LogLevel::Info => Self::Info,
            LogLevel::Skip => Self::Skip,
        }
    }
}

/// Single timestamped entry in the GUI activity panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityItem {
    pub timestamp: String,
    pub code: String,
    pub message: String,
    pub kind: ActivityKind,
}

/// Immutable snapshot read by the UI each frame without blocking the engine thread.
#[derive(Debug, Clone, Default)]
pub struct GuiSnapshot {
    pub status: EngineStatus,
    pub auth_status: String,
    pub user: Option<UserInfo>,
    pub cycle: u64,
    pub countdown_secs: u32,
    pub interval_secs: u64,
    pub last_scan: String,
    pub courses: Vec<CourseItem>,
    pub activities: Vec<ActivityItem>,
    pub last_error: Option<String>,
}

/// Sanitize upstream messages to comply with R-02 (no em dash or double-dash separator).
fn sanitize_text(input: &str) -> String {
    input
        .replace(" \u{2014} ", ": ")
        .replace('\u{2014}', ", ")
        .replace(concat!(" ", "-", "-", " "), ": ")
}

fn color_status_label(color: Color) -> String {
    match color {
        Color::Blue => "Siap Absen",
        Color::Green => "Sudah Absen",
        Color::Orange => "Belum Buka",
        Color::Red => "Belum Waktunya",
        Color::Unknown => "Tidak Dikenali",
    }
    .to_string()
}

fn clean_skip_reason(color: Color, raw_reason: &str) -> String {
    match color {
        Color::Green => "Sudah absen (dihitung berhasil)".to_string(),
        Color::Orange => "Belum buka: dosen belum membuka sesi absen".to_string(),
        Color::Red => "Belum waktunya: di luar jam absensi".to_string(),
        Color::Unknown => "Warna tidak dikenali: dilewati demi keamanan".to_string(),
        Color::Blue => sanitize_text(raw_reason),
    }
}

fn push_activity(activities: &mut Vec<ActivityItem>, item: ActivityItem) {
    let mut ring: VecDeque<ActivityItem> = VecDeque::from(std::mem::take(activities));
    while ring.len() >= MAX_ACTIVITY_ITEMS {
        ring.pop_front();
    }
    ring.push_back(item);
    *activities = ring.into_iter().collect();
}

struct BridgeReporter {
    snapshot: Arc<Mutex<GuiSnapshot>>,
    notifier: Arc<Mutex<Notifier>>,
    wake_ui: Option<Arc<dyn Fn() + Send + Sync>>,
    window_visible: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
    sticky_sigs: Mutex<HashSet<String>>,
    sticky_names: Mutex<HashSet<String>>,
}

impl BridgeReporter {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn wake(&self) {
        if self.is_cancelled() {
            return;
        }
        if let Some(cb) = &self.wake_ui {
            cb();
        }
    }

    fn record_sticky_success(&self, sig: &str, nama: &str) {
        if let Ok(mut sigs) = self.sticky_sigs.lock() {
            if !sig.trim().is_empty() {
                sigs.insert(sig.trim().to_string());
            }
        }
        if let Ok(mut names) = self.sticky_names.lock() {
            if !nama.trim().is_empty() {
                names.insert(nama.trim().to_string());
            }
        }
    }

    fn is_sticky_success(&self, sig: &str, nama: &str) -> bool {
        let sig_hit = self
            .sticky_sigs
            .lock()
            .map(|g| !sig.trim().is_empty() && g.contains(sig.trim()))
            .unwrap_or(false);
        if sig_hit {
            return true;
        }
        self.sticky_names
            .lock()
            .map(|g| !nama.trim().is_empty() && g.contains(nama.trim()))
            .unwrap_or(false)
    }

    fn notify_attendance(&self, course_name: &str) {
        let notifier = self.notifier.lock().map(|g| g.clone()).ok();
        if let Some(n) = notifier {
            n.notify_attendance_success(course_name);
        }
    }
}

impl Reporter for BridgeReporter {
    fn emit(&self, event: Event) {
        if self.is_cancelled() {
            return;
        }
        match event {
            Event::Engine { interval, .. } => {
                if let Ok(mut snap) = self.snapshot.lock() {
                    snap.interval_secs = interval;
                }
                self.wake();
            }
            Event::UserInfo(user) => {
                if let Ok(mut snap) = self.snapshot.lock() {
                    snap.user = Some(user);
                }
                self.wake();
            }
            Event::AuthStatus(raw_status) => {
                let cleaned = sanitize_text(&raw_status);
                if let Ok(mut snap) = self.snapshot.lock() {
                    snap.auth_status = cleaned.clone();
                    if raw_status.starts_with("OK") || raw_status.contains("Authenticated") {
                        snap.status = EngineStatus::Monitoring;
                        snap.last_error = None;
                    } else if raw_status.starts_with("GAGAL") {
                        snap.status = EngineStatus::AuthFailed;
                        snap.last_error = Some(cleaned);
                    } else if raw_status.contains("re-login") {
                        snap.status = EngineStatus::Connecting;
                    }
                }
                self.wake();
            }
            Event::CycleIncrement => {
                if let Ok(mut snap) = self.snapshot.lock() {
                    snap.cycle += 1;
                    snap.status = EngineStatus::ScanningNow;
                }
                self.wake();
            }
            Event::Jadwal { actions, skips } => {
                let mut courses = Vec::with_capacity(actions.len() + skips.len());

                for act in actions {
                    let sticky = self.is_sticky_success(&act.sig, &act.nama_mk);
                    let (color, reason, action_ready) = if sticky {
                        (
                            Color::Green,
                            "Presensi berhasil dicatat pada sesi ini".to_string(),
                            false,
                        )
                    } else {
                        (
                            Color::Blue,
                            "Sesi absensi terbuka, siap dicatat otomatis".to_string(),
                            true,
                        )
                    };
                    courses.push(CourseItem {
                        sig: act.sig,
                        nama_mk: act.nama_mk,
                        dosen: nama_dosen(&act.dosen),
                        jam: act.jam,
                        color,
                        status_label: color_status_label(color),
                        reason,
                        action_ready,
                    });
                }

                for skip in skips {
                    let (nama_raw, dosen_raw, jam_raw) = split_sig(&skip.sig);
                    let sticky = self.is_sticky_success(&skip.sig, nama_raw);
                    let (color, reason) = if sticky {
                        (
                            Color::Green,
                            "Presensi berhasil dicatat pada sesi ini".to_string(),
                        )
                    } else {
                        (skip.color, clean_skip_reason(skip.color, skip.reason))
                    };
                    courses.push(CourseItem {
                        sig: skip.sig.clone(),
                        nama_mk: nama_raw.to_string(),
                        dosen: nama_dosen(dosen_raw),
                        jam: jam_raw.to_string(),
                        color,
                        status_label: color_status_label(color),
                        reason,
                        action_ready: false,
                    });
                }

                if let Ok(mut snap) = self.snapshot.lock() {
                    snap.courses = courses;
                    if snap.status == EngineStatus::ScanningNow {
                        snap.status = EngineStatus::Monitoring;
                    }
                }
                self.wake();
            }
            Event::LastScan(raw_scan) => {
                let cleaned = sanitize_text(&raw_scan);
                if let Ok(mut snap) = self.snapshot.lock() {
                    snap.last_scan = cleaned;
                    if matches!(
                        snap.status,
                        EngineStatus::ScanningNow | EngineStatus::Connecting
                    ) {
                        snap.status = EngineStatus::Monitoring;
                        snap.last_error = None;
                    }
                }
                self.wake();
            }
            Event::MarkResult { sig, success, nama } => {
                if success {
                    self.record_sticky_success(&sig, &nama);
                }
                if let Ok(mut snap) = self.snapshot.lock() {
                    for course in &mut snap.courses {
                        let same_row = course.sig == sig
                            || (!nama.trim().is_empty() && course.nama_mk == nama.trim());
                        if same_row {
                            if success {
                                course.color = Color::Green;
                                course.status_label = color_status_label(Color::Green);
                                course.reason =
                                    "Presensi berhasil dicatat pada sesi ini".to_string();
                                course.action_ready = false;
                            } else {
                                course.reason =
                                    "Percobaan presensi gagal, akan dicoba ulang pada siklus berikutnya"
                                        .to_string();
                            }
                        }
                    }

                    let msg = if success {
                        format!("Presensi berhasil dicatat untuk {}", nama.trim())
                    } else {
                        format!("Presensi gagal dicatat untuk {}", nama.trim())
                    };
                    push_activity(
                        &mut snap.activities,
                        ActivityItem {
                            timestamp: Local::now().format("%H:%M:%S").to_string(),
                            code: "ABSEN".to_string(),
                            message: sanitize_text(&msg),
                            kind: if success {
                                ActivityKind::Ok
                            } else {
                                ActivityKind::Fail
                            },
                        },
                    );
                }
                if success {
                    self.notify_attendance(&nama);
                }
                self.wake();
            }
            Event::Countdown(secs) => {
                if let Ok(mut snap) = self.snapshot.lock() {
                    snap.countdown_secs = secs;
                    if secs > 0 && snap.status == EngineStatus::ScanningNow {
                        snap.status = EngineStatus::Monitoring;
                    }
                }
                // Performance optimization: when minimized/hidden in tray, avoid waking
                // the UI every second so idle CPU/GPU usage remains 0.0%.
                if self.window_visible.load(Ordering::Relaxed) {
                    self.wake();
                }
            }
            Event::Log {
                code,
                message,
                level,
            } => {
                let cleaned = sanitize_text(&message);
                if let Ok(mut snap) = self.snapshot.lock() {
                    if code == "ERROR" && level == LogLevel::Fail && cleaned.starts_with("siklus:")
                    {
                        snap.status = EngineStatus::NetworkError;
                        snap.last_error = Some(cleaned.clone());
                    }
                    push_activity(
                        &mut snap.activities,
                        ActivityItem {
                            timestamp: Local::now().format("%H:%M:%S").to_string(),
                            code,
                            message: cleaned,
                            kind: level.into(),
                        },
                    );
                }
                self.wake();
            }
        }
    }
}

/// Controller owning the engine worker thread and command channel.
pub struct EngineController {
    snapshot: Arc<Mutex<GuiSnapshot>>,
    notifier: Arc<Mutex<Notifier>>,
    window_visible: Arc<AtomicBool>,
    cmd_tx: Option<mpsc::Sender<EngineCommand>>,
    worker: Option<JoinHandle<()>>,
    run_cancelled: Option<Arc<AtomicBool>>,
}

impl Default for EngineController {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineController {
    pub fn new() -> Self {
        Self {
            snapshot: Arc::new(Mutex::new(GuiSnapshot::default())),
            notifier: Arc::new(Mutex::new(Notifier::new(true))),
            window_visible: Arc::new(AtomicBool::new(true)),
            cmd_tx: None,
            worker: None,
            run_cancelled: None,
        }
    }

    pub fn snapshot(&self) -> GuiSnapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn is_running(&self) -> bool {
        self.worker
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
    }

    /// Control whether `Countdown` ticks wake the UI event loop.
    pub fn set_window_visible(&self, visible: bool) {
        self.window_visible.store(visible, Ordering::Relaxed);
    }

    /// Returns whether the window is currently marked visible for countdown wakeups.
    pub fn is_window_visible(&self) -> bool {
        self.window_visible.load(Ordering::Relaxed)
    }

    /// Update the notifier used by the worker thread for attendance and auth alerts.
    pub fn set_notifier(&mut self, notifier: Notifier) {
        let mut guard = self.notifier.lock().unwrap_or_else(|e| e.into_inner());
        *guard = notifier;
    }

    /// Start the background attendance engine on a dedicated worker thread.
    pub fn start(
        &mut self,
        cfg: Config,
        loop_mode: bool,
        wake_ui: Option<Box<dyn Fn() + Send + Sync>>,
    ) -> Result<(), String> {
        if self.is_running() {
            return Err("engine sudah berjalan".to_string());
        }
        if let Some(old_handle) = self.worker.take() {
            let _ = old_handle.join();
        }
        self.cmd_tx = None;
        self.run_cancelled = None;

        let client = AbsenClient::new(&cfg.base_url).map_err(|e| sanitize_text(&e.to_string()))?;

        {
            let mut snap = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
            snap.status = EngineStatus::Connecting;
            snap.auth_status = "Menghubungkan ke portal akademik...".to_string();
            snap.user = None;
            snap.cycle = 0;
            snap.countdown_secs = 0;
            snap.interval_secs = cfg.interval;
            snap.last_scan.clear();
            snap.courses.clear();
            snap.last_error = None;
        }

        let (tx, rx) = mpsc::channel();
        let snapshot = Arc::clone(&self.snapshot);
        let notifier = Arc::clone(&self.notifier);
        let window_visible = Arc::clone(&self.window_visible);
        let cancelled = Arc::new(AtomicBool::new(false));
        let run_cancelled = Arc::clone(&cancelled);
        let wake_arc: Option<Arc<dyn Fn() + Send + Sync>> = wake_ui.map(Arc::from);

        let reporter = BridgeReporter {
            snapshot: Arc::clone(&snapshot),
            notifier: Arc::clone(&notifier),
            wake_ui: wake_arc.clone(),
            window_visible,
            cancelled: Arc::clone(&cancelled),
            sticky_sigs: Mutex::new(HashSet::new()),
            sticky_names: Mutex::new(HashSet::new()),
        };

        let handle = std::thread::Builder::new()
            .name("absen-engine-worker".to_string())
            .spawn(move || {
                let result = run_with_commands(&cfg, &client, loop_mode, &reporter, &rx);
                if cancelled.load(Ordering::Acquire) {
                    return;
                }
                match result {
                    Ok(()) => {
                        let mut snap = snapshot.lock().unwrap_or_else(|e| e.into_inner());
                        snap.countdown_secs = 0;
                        if !matches!(
                            snap.status,
                            EngineStatus::AuthFailed | EngineStatus::NetworkError
                        ) {
                            snap.status = EngineStatus::Stopped;
                        }
                    }
                    Err(EngineError::LoginFailed) => {
                        let detail =
                            "NIM atau kata sandi ditolak oleh portal akademik.".to_string();
                        {
                            let mut snap = snapshot.lock().unwrap_or_else(|e| e.into_inner());
                            snap.status = EngineStatus::AuthFailed;
                            snap.countdown_secs = 0;
                            snap.last_error = Some(format!("Login gagal: {detail}"));
                        }
                        if let Ok(n) = notifier.lock() {
                            n.notify_auth_failed(&detail);
                        }
                    }
                    Err(EngineError::Client(err)) => {
                        let detail = sanitize_text(&err.to_string());
                        let mut snap = snapshot.lock().unwrap_or_else(|e| e.into_inner());
                        snap.status = EngineStatus::NetworkError;
                        snap.countdown_secs = 0;
                        snap.last_error = Some(format!("Gangguan jaringan: {detail}"));
                    }
                }
                if let Some(cb) = &wake_arc {
                    cb();
                }
            })
            .map_err(|e| format!("Gagal menjalankan thread engine: {e}"))?;

        self.cmd_tx = Some(tx);
        self.worker = Some(handle);
        self.run_cancelled = Some(run_cancelled);
        Ok(())
    }

    pub fn scan_now(&self) -> bool {
        self.cmd_tx
            .as_ref()
            .is_some_and(|tx| tx.send(EngineCommand::ScanNow).is_ok())
    }

    /// Signal the worker thread to stop without blocking the UI thread if an HTTP request is in flight.
    pub fn stop(&mut self) {
        if let Some(flag) = self.run_cancelled.take() {
            flag.store(true, Ordering::Release);
        }
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(EngineCommand::Stop);
        }
        if let Some(handle) = self.worker.take() {
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(120);
            while !handle.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            if handle.is_finished() {
                let _ = handle.join();
            } else {
                let _ = std::thread::Builder::new()
                    .name("absen-engine-reaper".to_string())
                    .spawn(move || {
                        let _ = handle.join();
                    });
            }
        }
        let mut snap = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        snap.countdown_secs = 0;
        if !matches!(
            snap.status,
            EngineStatus::AuthFailed | EngineStatus::NetworkError
        ) {
            snap.status = EngineStatus::Stopped;
        }
    }

    /// Log in with `cfg` and write raw portal HTML to `out_path` for diagnostics.
    pub fn dump_diagnostic_html(cfg: &Config, out_path: &Path) -> Result<PathBuf, String> {
        let client = AbsenClient::new(&cfg.base_url).map_err(|e| sanitize_text(&e.to_string()))?;
        client
            .dump_absen_html(cfg, out_path)
            .map_err(|e| sanitize_text(&e.to_string()))
    }
}

impl Drop for EngineController {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_text_removes_double_dash_and_em_dash() {
        assert_eq!(
            sanitize_text(concat!("OK", " ", "-", "-", " ", "Authenticated")),
            "OK: Authenticated"
        );
        assert_eq!(
            sanitize_text("belum buka \u{2014} tunggu"),
            "belum buka: tunggu"
        );
        assert_eq!(sanitize_text("a\u{2014}b"), "a, b");
    }

    #[test]
    fn activity_ring_buffer_caps_at_fifty_items() {
        let mut items = Vec::new();
        for i in 0..75 {
            push_activity(
                &mut items,
                ActivityItem {
                    timestamp: "12:00:00".to_string(),
                    code: "SCAN".to_string(),
                    message: format!("Pesan {i}"),
                    kind: ActivityKind::Info,
                },
            );
        }
        assert_eq!(items.len(), 50);
        assert_eq!(items.first().unwrap().message, "Pesan 25");
        assert_eq!(items.last().unwrap().message, "Pesan 74");
    }

    #[test]
    fn cancelled_reporter_ignores_late_events_and_stop_is_non_blocking() {
        let snapshot = Arc::new(Mutex::new(GuiSnapshot::default()));
        let notifier = Arc::new(Mutex::new(Notifier::new(false)));
        let window_visible = Arc::new(AtomicBool::new(true));
        let cancelled = Arc::new(AtomicBool::new(true));
        let woke = Arc::new(AtomicBool::new(false));
        let woke_clone = Arc::clone(&woke);

        let reporter = BridgeReporter {
            snapshot: Arc::clone(&snapshot),
            notifier,
            wake_ui: Some(Arc::new(move || {
                woke_clone.store(true, Ordering::Release);
            })),
            window_visible,
            cancelled,
            sticky_sigs: Mutex::new(HashSet::new()),
            sticky_names: Mutex::new(HashSet::new()),
        };

        reporter.emit(Event::AuthStatus(
            concat!("GAGAL", " ", "-", "-", " ", "late").to_string(),
        ));
        assert_eq!(snapshot.lock().unwrap().status, EngineStatus::Idle);
        assert!(!woke.load(Ordering::Acquire));

        // Verify EngineController::stop() does not block even if a worker sleeps longer than 120ms.
        let mut ctrl = EngineController::new();
        ctrl.worker = Some(std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(600));
        }));
        let start = std::time::Instant::now();
        ctrl.stop();
        assert!(start.elapsed() < std::time::Duration::from_millis(350));
        assert!(!ctrl.is_running());
        assert_eq!(ctrl.snapshot().status, EngineStatus::Stopped);
    }
}

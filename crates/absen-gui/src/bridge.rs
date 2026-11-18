//! Thread-safe bridge between `absen-core` worker thread and the `egui` UI loop.

use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;

use absen_core::{Color, Config, EngineCommand, LogLevel, UserInfo};

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

/// Controller owning the engine worker thread and command channel.
///
/// Stage 1 provides the stable contract and thread-safe state scaffolding;
/// Stage 2 (`gui-services`) completes event projection, ring buffer, and notifications.
pub struct EngineController {
    snapshot: Arc<Mutex<GuiSnapshot>>,
    cmd_tx: Option<mpsc::Sender<EngineCommand>>,
    worker: Option<JoinHandle<()>>,
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
            cmd_tx: None,
            worker: None,
        }
    }

    pub fn snapshot(&self) -> GuiSnapshot {
        self.snapshot.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn is_running(&self) -> bool {
        self.worker
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
    }

    pub fn start(
        &mut self,
        _cfg: Config,
        _loop_mode: bool,
        _wake_ui: Option<Box<dyn Fn() + Send + Sync>>,
    ) -> Result<(), String> {
        if self.is_running() {
            return Err("engine sudah berjalan".to_string());
        }
        let (tx, _rx) = mpsc::channel();
        self.cmd_tx = Some(tx);
        if let Ok(mut snap) = self.snapshot.lock() {
            snap.status = EngineStatus::Connecting;
            snap.last_error = None;
        }
        Ok(())
    }

    pub fn scan_now(&self) -> bool {
        self.cmd_tx
            .as_ref()
            .is_some_and(|tx| tx.send(EngineCommand::ScanNow).is_ok())
    }

    pub fn stop(&mut self) {
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(EngineCommand::Stop);
        }
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
        if let Ok(mut snap) = self.snapshot.lock() {
            snap.status = EngineStatus::Stopped;
            snap.countdown_secs = 0;
        }
    }
}

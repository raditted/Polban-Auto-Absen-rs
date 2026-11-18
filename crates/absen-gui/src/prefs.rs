//! Non-secret desktop preferences persistence and `.env` import helpers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use absen_core::config::{
    build_env_candidates, Config, DEFAULT_BASE_URL, DEFAULT_DELAY_MAX, DEFAULT_DELAY_MIN,
    DEFAULT_ENGINE, DEFAULT_INTERVAL, DEFAULT_MAX_RETRY, DEFAULT_RETRY_WAIT,
};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// User-selectable color theme mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    Light,
    Dark,
    #[default]
    System,
}

#[derive(Debug, Error)]
pub enum PrefsError {
    #[error("direktori konfigurasi aplikasi tidak tersedia")]
    ProjectDirUnavailable,
    #[error("gagal membaca/menulis preferensi: {0}")]
    Io(#[from] std::io::Error),
    #[error("format JSON preferensi tidak valid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("file .env tidak valid: {0}")]
    InvalidDotenv(String),
}

/// Non-secret user preferences stored in `prefs.json`.
/// Passwords are never serialized here; they belong in `CredentialStore`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppPrefs {
    pub nim: String,
    pub base_url: String,
    pub target_url: String,
    pub interval: u64,
    pub delay_min: f64,
    pub delay_max: f64,
    pub max_retry: u32,
    pub retry_wait: f64,
    pub theme: ThemePreference,
    pub minimize_to_tray: bool,
    pub autostart: bool,
    pub notifications_enabled: bool,
    pub auto_start_monitoring: bool,
    pub state_file: Option<PathBuf>,
}

impl Default for AppPrefs {
    fn default() -> Self {
        Self {
            nim: String::new(),
            base_url: DEFAULT_BASE_URL.to_string(),
            target_url: format!("{DEFAULT_BASE_URL}/ajar/absen"),
            interval: DEFAULT_INTERVAL,
            delay_min: DEFAULT_DELAY_MIN,
            delay_max: DEFAULT_DELAY_MAX,
            max_retry: DEFAULT_MAX_RETRY,
            retry_wait: DEFAULT_RETRY_WAIT,
            theme: ThemePreference::System,
            minimize_to_tray: true,
            autostart: false,
            notifications_enabled: true,
            auto_start_monitoring: false,
            state_file: None,
        }
    }
}

impl AppPrefs {
    /// Convert desktop preferences plus a retrieved secret password into `absen_core::Config`.
    pub fn to_core_config(&self, password: String) -> Config {
        let base_url = self.base_url.trim().trim_end_matches('/').to_string();
        let target_url = if self.target_url.trim().is_empty() {
            format!("{base_url}/ajar/absen")
        } else {
            self.target_url.trim().to_string()
        };
        Config {
            username: self.nim.trim().to_string(),
            password,
            base_url,
            target_url,
            interval: self.interval.max(5),
            delay_min: self.delay_min.max(0.0),
            delay_max: self.delay_max.max(self.delay_min.max(0.0)),
            max_retry: self.max_retry.max(1),
            retry_wait: self.retry_wait.max(0.0),
            engine: DEFAULT_ENGINE.to_string(),
            env_path_used: None,
            state_file: self
                .state_file
                .clone()
                .unwrap_or_else(default_state_file_path),
        }
    }
}

/// Result returned when importing a `.env` file into GUI settings.
#[derive(Debug, Clone)]
pub struct DotenvImportResult {
    pub prefs: AppPrefs,
    pub password: Option<String>,
    pub source_path: PathBuf,
}

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("id", "polban", "PolbanPresensi")
}

/// Canonical path to `prefs.json` in the OS config directory.
pub fn prefs_file_path() -> Result<PathBuf, PrefsError> {
    let dirs = project_dirs().ok_or(PrefsError::ProjectDirUnavailable)?;
    Ok(dirs.config_dir().join("prefs.json"))
}

/// Default path for `.clicked.json` attendance dedup state in OS local data directory.
pub fn default_state_file_path() -> PathBuf {
    project_dirs()
        .map(|d| d.data_local_dir().join("clicked.json"))
        .unwrap_or_else(|| PathBuf::from(".clicked.json"))
}

/// Load preferences from the default OS path, returning defaults if the file does not exist yet.
pub fn load() -> Result<AppPrefs, PrefsError> {
    let path = prefs_file_path()?;
    load_from_path(&path)
}

/// Load preferences from an explicit path (useful for deterministic unit tests).
pub fn load_from_path(path: &Path) -> Result<AppPrefs, PrefsError> {
    if !path.is_file() {
        return Ok(AppPrefs::default());
    }
    let raw = std::fs::read_to_string(path)?;
    let prefs = serde_json::from_str(&raw)?;
    Ok(prefs)
}

/// Save preferences to the default OS path.
pub fn save(prefs: &AppPrefs) -> Result<(), PrefsError> {
    let path = prefs_file_path()?;
    save_to_path(prefs, &path)
}

/// Save preferences atomically to an explicit path.
pub fn save_to_path(prefs: &AppPrefs, path: &Path) -> Result<(), PrefsError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(prefs)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Detect existing `.env` files matching `absen_core` candidate order.
pub fn detect_env_candidates() -> Vec<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let cwd = std::env::current_dir().ok();
    let polban_env = std::env::var("POLBAN_ENV").ok();
    build_env_candidates(polban_env.as_deref(), exe_dir.as_deref(), cwd.as_deref())
        .into_iter()
        .filter(|p| p.is_file())
        .collect()
}

/// Parse an explicit `.env` file into `AppPrefs` + optional secret password.
pub fn import_dotenv_file(path: &Path) -> Result<DotenvImportResult, PrefsError> {
    if !path.is_file() {
        return Err(PrefsError::InvalidDotenv(format!(
            "file tidak ditemukan: {}",
            path.display()
        )));
    }
    let (cfg, _) = Config::load_from_candidates(
        &[path.to_path_buf()],
        HashMap::new(),
        default_state_file_path(),
    )
    .map_err(|e| PrefsError::InvalidDotenv(e.to_string()))?;

    let prefs = AppPrefs {
        nim: cfg.username,
        base_url: cfg.base_url,
        target_url: cfg.target_url,
        interval: cfg.interval,
        delay_min: cfg.delay_min,
        delay_max: cfg.delay_max,
        max_retry: cfg.max_retry,
        retry_wait: cfg.retry_wait,
        ..AppPrefs::default()
    };

    let password = if cfg.password.is_empty() {
        None
    } else {
        Some(cfg.password)
    };

    Ok(DotenvImportResult {
        prefs,
        password,
        source_path: path.to_path_buf(),
    })
}

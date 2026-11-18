//! Non-secret desktop preferences persistence and `.env` import helpers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use absen_core::config::{
    build_env_candidates, Config, DEFAULT_BASE_URL, DEFAULT_DELAY_MAX, DEFAULT_DELAY_MIN,
    DEFAULT_ENGINE, DEFAULT_INTERVAL, DEFAULT_MAX_RETRY, DEFAULT_RETRY_WAIT,
};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use thiserror::Error;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

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
        let base_url = if base_url.is_empty() {
            DEFAULT_BASE_URL.to_string()
        } else {
            base_url
        };
        let target_url = if self.target_url.trim().is_empty() {
            format!("{base_url}/ajar/absen")
        } else {
            self.target_url.trim().to_string()
        };
        let delay_min = self.delay_min.max(0.0);
        let delay_max = self.delay_max.max(delay_min);
        Config {
            username: self.nim.trim().to_string(),
            password,
            base_url,
            target_url,
            interval: self.interval.max(5),
            delay_min,
            delay_max,
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

/// Save preferences atomically to an explicit path via temporary file rename.
pub fn save_to_path(prefs: &AppPrefs, path: &Path) -> Result<(), PrefsError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;

    let json = serde_json::to_string_pretty(prefs)?;
    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("prefs.json");
    let tmp_path = parent.join(format!(".{file_name}.tmp.{}.{seq}", std::process::id()));

    if let Err(err) = std::fs::write(&tmp_path, json) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(PrefsError::Io(err));
    }
    if let Err(err) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(PrefsError::Io(err));
    }
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

/// Open a native file picker for a `.env` file and import it if selected.
pub fn pick_dotenv_and_import() -> Option<Result<DotenvImportResult, PrefsError>> {
    let path = rfd::FileDialog::new()
        .set_title("Pilih file .env Polban Presensi")
        .pick_file()?;
    Some(import_dotenv_file(&path))
}

/// Open a native save dialog for exporting diagnostic attendance HTML.
pub fn pick_diagnostic_html_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Simpan HTML Halaman Absen (Diagnostik)")
        .set_file_name("absen-diagnostik.html")
        .add_filter("HTML", &["html", "htm"])
        .save_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "polban_absen_prefs_{name}_{}_{seq}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn prefs_json_roundtrip_never_serializes_password() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("nested").join("prefs.json");

        assert_eq!(load_from_path(&path).unwrap(), AppPrefs::default());

        let prefs = AppPrefs {
            nim: "251511001".to_string(),
            base_url: "https://akademik.polban.ac.id".to_string(),
            target_url: "https://akademik.polban.ac.id/ajar/absen".to_string(),
            interval: 45,
            delay_min: 1.0,
            delay_max: 2.5,
            max_retry: 4,
            retry_wait: 1.5,
            theme: ThemePreference::Dark,
            minimize_to_tray: false,
            autostart: true,
            notifications_enabled: false,
            auto_start_monitoring: true,
            state_file: Some(dir.join("clicked.json")),
        };

        save_to_path(&prefs, &path).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("password"));

        let loaded = load_from_path(&path).unwrap();
        assert_eq!(loaded, prefs);

        let updated = AppPrefs {
            interval: 90,
            ..prefs
        };
        save_to_path(&updated, &path).unwrap();
        assert_eq!(load_from_path(&path).unwrap().interval, 90);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn to_core_config_sanitizes_and_clamps_values() {
        let prefs = AppPrefs {
            nim: "  251511002  ".to_string(),
            base_url: "  https://portal.example.test/// ".to_string(),
            target_url: "   ".to_string(),
            interval: 1,
            delay_min: -3.0,
            delay_max: -1.0,
            max_retry: 0,
            retry_wait: -5.0,
            state_file: Some(PathBuf::from("custom_state.json")),
            ..AppPrefs::default()
        };

        let cfg = prefs.to_core_config("secret#pass".to_string());
        assert_eq!(cfg.username, "251511002");
        assert_eq!(cfg.password, "secret#pass");
        assert_eq!(cfg.base_url, "https://portal.example.test");
        assert_eq!(cfg.target_url, "https://portal.example.test/ajar/absen");
        assert_eq!(cfg.interval, 5);
        assert!((cfg.delay_min - 0.0).abs() < f64::EPSILON);
        assert!((cfg.delay_max - 0.0).abs() < f64::EPSILON);
        assert_eq!(cfg.max_retry, 1);
        assert!((cfg.retry_wait - 0.0).abs() < f64::EPSILON);
        assert_eq!(cfg.state_file, PathBuf::from("custom_state.json"));
    }

    #[test]
    fn import_dotenv_file_handles_hash_and_quotes_and_missing_file() {
        let dir = temp_dir("dotenv");
        let missing = dir.join("missing.env");
        assert!(matches!(
            import_dotenv_file(&missing),
            Err(PrefsError::InvalidDotenv(_))
        ));

        let env_path = dir.join(".env");
        std::fs::write(
            &env_path,
            "POLBAN_USERNAME=251511003\n\
             POLBAN_PASSWORD=\"p@ss#with#hash!\"\n\
             POLBAN_BASE_URL=https://akademik.example.id/\n\
             POLBAN_INTERVAL=30\n\
             POLBAN_DELAY_MIN=0.5\n\
             POLBAN_DELAY_MAX=1.5\n\
             POLBAN_MAX_RETRY=6\n\
             POLBAN_RETRY_WAIT=2.0\n",
        )
        .unwrap();

        let imported = import_dotenv_file(&env_path).unwrap();
        assert_eq!(imported.prefs.nim, "251511003");
        assert_eq!(imported.password.as_deref(), Some("p@ss#with#hash!"));
        assert_eq!(imported.prefs.base_url, "https://akademik.example.id");
        assert_eq!(
            imported.prefs.target_url,
            "https://akademik.example.id/ajar/absen"
        );
        assert_eq!(imported.prefs.interval, 30);
        assert_eq!(imported.prefs.max_retry, 6);
        assert_eq!(imported.source_path, env_path);

        let single_quoted = dir.join("single.env");
        std::fs::write(
            &single_quoted,
            "POLBAN_USERNAME=251511004\nPOLBAN_PASSWORD='quoted#value'\n",
        )
        .unwrap();
        let res_single = import_dotenv_file(&single_quoted).unwrap();
        assert_eq!(res_single.password.as_deref(), Some("quoted#value"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

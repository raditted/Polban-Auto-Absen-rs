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

/// Minimum supported UI zoom percentage (`70%` -> `0.70x`).
pub const MIN_UI_ZOOM_PCT: u16 = 70;
/// Maximum supported UI zoom percentage (`150%` -> `1.50x`).
pub const MAX_UI_ZOOM_PCT: u16 = 150;
/// Default UI zoom percentage (`100%` -> `1.00x`).
pub const DEFAULT_UI_ZOOM_PCT: u16 = 100;
/// Minimum notification sound volume percentage (`0%`, muted).
pub const MIN_NOTIFICATION_VOLUME: u8 = 0;
/// Maximum notification sound volume percentage (`100%`).
pub const MAX_NOTIFICATION_VOLUME: u8 = 100;
/// Default notification sound volume percentage (`100%`).
pub const DEFAULT_NOTIFICATION_VOLUME: u8 = 100;

/// Clamp a UI zoom percentage into `MIN_UI_ZOOM_PCT..=MAX_UI_ZOOM_PCT`.
pub fn clamp_ui_zoom_pct(pct: u16) -> u16 {
    pct.clamp(MIN_UI_ZOOM_PCT, MAX_UI_ZOOM_PCT)
}

/// Convert a UI zoom percentage (`70..=150`) into an egui zoom factor (`0.70..=1.50`).
pub fn zoom_pct_to_factor(pct: u16) -> f32 {
    f32::from(clamp_ui_zoom_pct(pct)) / 100.0
}

/// Clamp a notification volume percentage into `0..=100`.
pub fn clamp_notification_volume(volume_pct: u8) -> u8 {
    volume_pct.min(MAX_NOTIFICATION_VOLUME)
}

/// User-selectable color theme mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    Light,
    Dark,
    #[default]
    System,
}

/// Curated Antigravity Light theme presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightThemePreset {
    #[default]
    DefaultLight,
    LegacyLight,
    Catppuccin,
    Everforest,
    GruvboxMaterial,
    OneLight,
    SolarizedLight,
}

impl LightThemePreset {
    pub const ALL: [Self; 7] = [
        Self::DefaultLight,
        Self::LegacyLight,
        Self::Catppuccin,
        Self::Everforest,
        Self::GruvboxMaterial,
        Self::OneLight,
        Self::SolarizedLight,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::DefaultLight => "Default Light",
            Self::LegacyLight => "Legacy Light",
            Self::Catppuccin => "Catppuccin",
            Self::Everforest => "Everforest",
            Self::GruvboxMaterial => "Gruvbox Material",
            Self::OneLight => "One Light",
            Self::SolarizedLight => "Solarized Light",
        }
    }
}

/// Curated Antigravity Dark theme presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DarkThemePreset {
    #[default]
    DefaultDark,
    LegacyDark,
    Catppuccin,
    Dracula,
    Everforest,
    GruvboxMaterial,
    Monokai,
    Nord,
    OneDarkPro,
    TokyoNight,
    SolarizedDark,
    Vesper,
}

impl DarkThemePreset {
    pub const ALL: [Self; 12] = [
        Self::DefaultDark,
        Self::LegacyDark,
        Self::Catppuccin,
        Self::Dracula,
        Self::Everforest,
        Self::GruvboxMaterial,
        Self::Monokai,
        Self::Nord,
        Self::OneDarkPro,
        Self::TokyoNight,
        Self::SolarizedDark,
        Self::Vesper,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::DefaultDark => "Default Dark",
            Self::LegacyDark => "Legacy Dark",
            Self::Catppuccin => "Catppuccin",
            Self::Dracula => "Dracula",
            Self::Everforest => "Everforest",
            Self::GruvboxMaterial => "Gruvbox Material",
            Self::Monokai => "Monokai",
            Self::Nord => "Nord",
            Self::OneDarkPro => "One Dark Pro",
            Self::TokyoNight => "Tokyo Night",
            Self::SolarizedDark => "Solarized Dark",
            Self::Vesper => "Vesper",
        }
    }
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

fn default_true() -> bool {
    true
}

fn default_ui_zoom_pct() -> u16 {
    DEFAULT_UI_ZOOM_PCT
}

fn default_notification_volume() -> u8 {
    DEFAULT_NOTIFICATION_VOLUME
}

fn deserialize_ui_zoom_pct<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = u16::deserialize(deserializer)?;
    Ok(clamp_ui_zoom_pct(raw))
}

fn deserialize_notification_volume<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = u8::deserialize(deserializer)?;
    Ok(clamp_notification_volume(raw))
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
    pub light_preset: LightThemePreset,
    pub dark_preset: DarkThemePreset,
    #[serde(
        default = "default_ui_zoom_pct",
        deserialize_with = "deserialize_ui_zoom_pct"
    )]
    pub ui_zoom_pct: u16,
    pub minimize_to_tray: bool,
    pub autostart: bool,
    pub notifications_enabled: bool,
    #[serde(default = "default_true")]
    pub notification_sound: bool,
    #[serde(
        default = "default_notification_volume",
        deserialize_with = "deserialize_notification_volume"
    )]
    pub notification_volume: u8,
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
            light_preset: LightThemePreset::DefaultLight,
            dark_preset: DarkThemePreset::DefaultDark,
            ui_zoom_pct: DEFAULT_UI_ZOOM_PCT,
            minimize_to_tray: true,
            autostart: false,
            notifications_enabled: true,
            notification_sound: true,
            notification_volume: DEFAULT_NOTIFICATION_VOLUME,
            auto_start_monitoring: false,
            state_file: None,
        }
    }
}

impl AppPrefs {
    /// Clamp zoom and volume fields into their safe supported ranges.
    pub fn sanitize(&mut self) {
        self.ui_zoom_pct = clamp_ui_zoom_pct(self.ui_zoom_pct);
        self.notification_volume = clamp_notification_volume(self.notification_volume);
    }

    /// Return the clamped UI zoom factor (`0.70..=1.50`).
    pub fn normalized_ui_zoom(&self) -> f32 {
        zoom_pct_to_factor(self.ui_zoom_pct)
    }

    /// Return the clamped notification volume percentage (`0..=100`).
    pub fn normalized_notification_volume(&self) -> u8 {
        clamp_notification_volume(self.notification_volume)
    }

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
    let mut prefs: AppPrefs = serde_json::from_str(&raw)?;
    prefs.sanitize();
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

/// Open a native save dialog for exporting an attendance proof or portal preview PNG.
pub fn pick_proof_image_save_path(default_filename: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Simpan Gambar Pratinjau Absen")
        .set_file_name(default_filename)
        .add_filter("PNG", &["png"])
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
            light_preset: LightThemePreset::Catppuccin,
            dark_preset: DarkThemePreset::Everforest,
            ui_zoom_pct: 125,
            minimize_to_tray: false,
            autostart: true,
            notifications_enabled: false,
            notification_sound: false,
            notification_volume: 60,
            auto_start_monitoring: true,
            state_file: Some(dir.join("clicked.json")),
        };

        save_to_path(&prefs, &path).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("password"));
        assert!(raw.contains("\"catppuccin\""));
        assert!(raw.contains("\"everforest\""));

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
    fn notification_sound_defaults_true_and_roundtrips_false() {
        let legacy_json = r#"{
            "nim": "251511099",
            "notifications_enabled": true
        }"#;
        let legacy_prefs: AppPrefs = serde_json::from_str(legacy_json).unwrap();
        assert!(legacy_prefs.notification_sound);

        let mut muted = AppPrefs::default();
        assert!(muted.notification_sound);
        muted.notification_sound = false;
        let json = serde_json::to_string(&muted).unwrap();
        let decoded: AppPrefs = serde_json::from_str(&json).unwrap();
        assert!(!decoded.notification_sound);
    }

    #[test]
    fn theme_presets_roundtrip_and_backward_compatible_json_defaults() {
        let legacy_json = r#"{
            "nim": "251511099",
            "base_url": "https://akademik.polban.ac.id",
            "target_url": "https://akademik.polban.ac.id/ajar/absen",
            "interval": 60,
            "theme": "dark"
        }"#;
        let legacy_prefs: AppPrefs = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(legacy_prefs.theme, ThemePreference::Dark);
        assert_eq!(legacy_prefs.light_preset, LightThemePreset::DefaultLight);
        assert_eq!(legacy_prefs.dark_preset, DarkThemePreset::DefaultDark);

        assert_eq!(LightThemePreset::ALL.len(), 7);
        for preset in LightThemePreset::ALL {
            assert!(!preset.label().is_empty());
            let json = serde_json::to_string(&preset).unwrap();
            let decoded: LightThemePreset = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, preset);
        }

        assert_eq!(DarkThemePreset::ALL.len(), 12);
        for preset in DarkThemePreset::ALL {
            assert!(!preset.label().is_empty());
            let json = serde_json::to_string(&preset).unwrap();
            let decoded: DarkThemePreset = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, preset);
        }
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

    #[test]
    fn ui_zoom_and_notification_volume_default_roundtrip_and_clamp() {
        let legacy_json = r#"{
            "nim": "251511099",
            "theme": "light"
        }"#;
        let legacy: AppPrefs = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(legacy.ui_zoom_pct, DEFAULT_UI_ZOOM_PCT);
        assert_eq!(legacy.notification_volume, DEFAULT_NOTIFICATION_VOLUME);
        assert!((legacy.normalized_ui_zoom() - 1.0).abs() < f32::EPSILON);
        assert_eq!(legacy.normalized_notification_volume(), 100);

        let out_of_range_low = r#"{
            "ui_zoom_pct": 15,
            "notification_volume": 0
        }"#;
        let low: AppPrefs = serde_json::from_str(out_of_range_low).unwrap();
        assert_eq!(low.ui_zoom_pct, MIN_UI_ZOOM_PCT);
        assert_eq!(low.notification_volume, 0);
        assert!((low.normalized_ui_zoom() - 0.70).abs() < 1e-6);

        let out_of_range_high = r#"{
            "ui_zoom_pct": 420,
            "notification_volume": 250
        }"#;
        let high: AppPrefs = serde_json::from_str(out_of_range_high).unwrap();
        assert_eq!(high.ui_zoom_pct, MAX_UI_ZOOM_PCT);
        assert_eq!(high.notification_volume, 100);
        assert!((high.normalized_ui_zoom() - 1.50).abs() < 1e-6);

        let dir = temp_dir("zoom_vol");
        let path = dir.join("prefs.json");
        std::fs::write(&path, out_of_range_high).unwrap();
        let loaded_clamped = load_from_path(&path).unwrap();
        assert_eq!(loaded_clamped.ui_zoom_pct, MAX_UI_ZOOM_PCT);
        assert_eq!(loaded_clamped.notification_volume, 100);

        let manual = AppPrefs {
            ui_zoom_pct: 135,
            notification_volume: 42,
            ..AppPrefs::default()
        };
        save_to_path(&manual, &path).unwrap();
        let roundtrip = load_from_path(&path).unwrap();
        assert_eq!(roundtrip.ui_zoom_pct, 135);
        assert_eq!(roundtrip.notification_volume, 42);
        assert!((roundtrip.normalized_ui_zoom() - 1.35).abs() < 1e-6);
        assert_eq!(roundtrip.normalized_notification_volume(), 42);

        let unclamped_struct = AppPrefs {
            ui_zoom_pct: 10,
            notification_volume: 180,
            ..AppPrefs::default()
        };
        assert!((unclamped_struct.normalized_ui_zoom() - 0.70).abs() < 1e-6);
        assert_eq!(unclamped_struct.normalized_notification_volume(), 100);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

//! `.env` loader and runtime configuration with secret redaction.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use thiserror::Error;

pub const DEFAULT_BASE_URL: &str = "https://akademik.polban.ac.id";
pub const DEFAULT_INTERVAL: u64 = 60;
pub const DEFAULT_DELAY_MIN: f64 = 1.2;
pub const DEFAULT_DELAY_MAX: f64 = 2.6;
pub const DEFAULT_MAX_RETRY: u32 = 5;
pub const DEFAULT_RETRY_WAIT: f64 = 2.5;
pub const DEFAULT_ENGINE: &str = "http";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("nilai konfigurasi tidak valid untuk {key}: '{value}'")]
    InvalidValue { key: &'static str, value: String },
}

/// Runtime configuration loaded from `.env` + environment variables.
///
/// `Debug` intentionally redacts `password` so credentials never leak into logs.
#[derive(Clone, PartialEq)]
pub struct Config {
    pub username: String,
    pub password: String,
    pub base_url: String,
    pub target_url: String,
    pub interval: u64,
    pub delay_min: f64,
    pub delay_max: f64,
    pub max_retry: u32,
    pub retry_wait: f64,
    pub engine: String,
    pub env_path_used: Option<PathBuf>,
    pub state_file: PathBuf,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("base_url", &self.base_url)
            .field("target_url", &self.target_url)
            .field("interval", &self.interval)
            .field("delay_min", &self.delay_min)
            .field("delay_max", &self.delay_max)
            .field("max_retry", &self.max_retry)
            .field("retry_wait", &self.retry_wait)
            .field("engine", &self.engine)
            .field("env_path_used", &self.env_path_used)
            .field("state_file", &self.state_file)
            .finish()
    }
}

impl Config {
    /// Load configuration using standard `.env` candidate paths and process env.
    pub fn load() -> Result<(Self, Option<PathBuf>), ConfigError> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        let cwd = std::env::current_dir().ok();
        let env_map: HashMap<String, String> = std::env::vars().collect();

        let candidates = build_env_candidates(
            env_map.get("POLBAN_ENV").map(String::as_str),
            exe_dir.as_deref(),
            cwd.as_deref(),
        );
        let default_state_dir = exe_dir.or(cwd).unwrap_or_else(|| PathBuf::from("."));
        let state_file = env_map
            .get("POLBAN_STATE_FILE")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| default_state_dir.join(".clicked.json"));

        Self::load_from_candidates(&candidates, env_map, state_file)
    }

    /// Pure, testable loader accepting explicit candidate paths, initial env map, and state path.
    pub fn load_from_candidates(
        candidates: &[PathBuf],
        mut env_map: HashMap<String, String>,
        state_file: PathBuf,
    ) -> Result<(Self, Option<PathBuf>), ConfigError> {
        let mut env_path_used = None;
        for cand in candidates {
            if cand.is_file() {
                if let Ok(contents) = std::fs::read_to_string(cand) {
                    for (k, v) in parse_dotenv(&contents) {
                        env_map.insert(k, v);
                    }
                    env_path_used = Some(cand.clone());
                    break;
                }
            }
        }

        let username = env_map.get("POLBAN_USERNAME").cloned().unwrap_or_default();
        let password = env_map.get("POLBAN_PASSWORD").cloned().unwrap_or_default();

        let explicit_base = env_map
            .get("POLBAN_BASE_URL")
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty());

        let explicit_target = env_map
            .get("POLBAN_TARGET_URL")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let base_url = explicit_base
            .or_else(|| explicit_target.as_deref().and_then(extract_origin))
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        let target_url = explicit_target.unwrap_or_else(|| format!("{base_url}/ajar/absen"));

        let interval = parse_or_default(&env_map, "POLBAN_INTERVAL", DEFAULT_INTERVAL)?;
        let delay_min = parse_or_default(&env_map, "POLBAN_DELAY_MIN", DEFAULT_DELAY_MIN)?;
        let delay_max = parse_or_default(&env_map, "POLBAN_DELAY_MAX", DEFAULT_DELAY_MAX)?;
        let max_retry = parse_or_default(&env_map, "POLBAN_MAX_RETRY", DEFAULT_MAX_RETRY)?;
        let retry_wait = parse_or_default(&env_map, "POLBAN_RETRY_WAIT", DEFAULT_RETRY_WAIT)?;
        let engine = env_map
            .get("POLBAN_ENGINE")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_ENGINE.to_string());

        let cfg = Self {
            username,
            password,
            base_url,
            target_url,
            interval,
            delay_min,
            delay_max,
            max_retry,
            retry_wait,
            engine,
            env_path_used: env_path_used.clone(),
            state_file,
        };
        Ok((cfg, env_path_used))
    }

    /// Override base URL (and derived default target URL if it matched the old base).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        let old_default_target = format!("{}/ajar/absen", self.base_url);
        let base = base_url.into().trim_end_matches('/').to_string();
        if self.target_url == old_default_target {
            self.target_url = format!("{base}/ajar/absen");
        }
        self.base_url = base;
        self
    }

    pub fn login_url(&self) -> String {
        format!("{}/laman/login", self.base_url)
    }

    pub fn absen_endpoint(&self) -> String {
        format!("{}/ajar/absen/absensi_awal", self.base_url)
    }

    pub fn referer_root(&self) -> String {
        format!("{}/", self.base_url)
    }
}

/// Build `.env` candidate paths in exact priority order:
/// `$POLBAN_ENV` -> `<exe_dir>/.env` -> `<cwd>/.env` -> `<exe_parent_dir>/.env`.
pub fn build_env_candidates(
    polban_env: Option<&str>,
    exe_dir: Option<&Path>,
    cwd: Option<&Path>,
) -> Vec<PathBuf> {
    let mut out = Vec::with_capacity(4);
    if let Some(p) = polban_env.map(str::trim).filter(|s| !s.is_empty()) {
        out.push(PathBuf::from(p));
    }
    if let Some(dir) = exe_dir {
        out.push(dir.join(".env"));
    }
    if let Some(dir) = cwd {
        out.push(dir.join(".env"));
    }
    if let Some(parent) = exe_dir.and_then(Path::parent) {
        out.push(parent.join(".env"));
    }
    out
}

/// Parse `.env` text matching `windows/auto_absen.py`:
/// - Blank lines and lines whose trimmed start is `#` are skipped.
/// - Lines without `=` are skipped.
/// - `#` inside a value is preserved literally.
/// - One matching pair of outer `'...'` or `"..."` quotes is stripped.
pub fn parse_dotenv(contents: &str) -> Vec<(String, String)> {
    let contents = contents.strip_prefix('\u{feff}').unwrap_or(contents);
    let mut pairs = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        if key.is_empty() {
            continue;
        }
        let mut val = v.trim();
        if val.len() >= 2
            && ((val.starts_with('\'') && val.ends_with('\''))
                || (val.starts_with('"') && val.ends_with('"')))
        {
            val = &val[1..val.len() - 1];
        }
        pairs.push((key.to_string(), val.to_string()));
    }
    pairs
}

fn parse_or_default<T: std::str::FromStr>(
    env_map: &HashMap<String, String>,
    key: &'static str,
    default: T,
) -> Result<T, ConfigError> {
    match env_map.get(key).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        None => Ok(default),
        Some(raw) => raw.parse::<T>().map_err(|_| ConfigError::InvalidValue {
            key,
            value: raw.to_string(),
        }),
    }
}

fn extract_origin(url: &str) -> Option<String> {
    let scheme_end = url.find("://")?;
    let after_scheme = &url[scheme_end + 3..];
    let authority_len = after_scheme.find('/').unwrap_or(after_scheme.len());
    if authority_len == 0 {
        return None;
    }
    Some(url[..scheme_end + 3 + authority_len].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dotenv_preserves_hash_and_strips_outer_quotes() {
        let raw = "\
# Komentar penuh
   # Komentar berindentasi

POLBAN_USERNAME=251511001
POLBAN_PASSWORD=p@ss#w0rd!2024#
POLBAN_SPACES = ab cd#ef
POLBAN_SINGLE='sekret#1'
POLBAN_DOUBLE=\"sekret\\\"2\"
INVALID_LINE_NO_EQ
";
        let parsed = parse_dotenv(raw);
        assert_eq!(
            parsed,
            vec![
                ("POLBAN_USERNAME".into(), "251511001".into()),
                ("POLBAN_PASSWORD".into(), "p@ss#w0rd!2024#".into()),
                ("POLBAN_SPACES".into(), "ab cd#ef".into()),
                ("POLBAN_SINGLE".into(), "sekret#1".into()),
                ("POLBAN_DOUBLE".into(), "sekret\\\"2".into()),
            ]
        );
    }

    #[test]
    fn debug_redacts_password() {
        let cfg = Config {
            username: "251511001".into(),
            password: "super-secret-password#123".into(),
            base_url: DEFAULT_BASE_URL.into(),
            target_url: format!("{DEFAULT_BASE_URL}/ajar/absen"),
            interval: DEFAULT_INTERVAL,
            delay_min: DEFAULT_DELAY_MIN,
            delay_max: DEFAULT_DELAY_MAX,
            max_retry: DEFAULT_MAX_RETRY,
            retry_wait: DEFAULT_RETRY_WAIT,
            engine: DEFAULT_ENGINE.into(),
            env_path_used: None,
            state_file: PathBuf::from(".clicked.json"),
        };
        let dbg = format!("{cfg:?}");
        assert!(dbg.contains("[REDACTED]"));
        assert!(!dbg.contains("super-secret-password#123"));
        assert!(dbg.contains("251511001"));
    }

    #[test]
    fn candidate_priority_and_override_behavior() {
        let dir =
            std::env::temp_dir().join(format!("polban_absen_cfg_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let exe_dir = dir.join("exe");
        let cwd_dir = dir.join("cwd");
        std::fs::create_dir_all(&exe_dir).unwrap();
        std::fs::create_dir_all(&cwd_dir).unwrap();

        let first_env = dir.join("custom.env");
        let exe_env = exe_dir.join(".env");
        std::fs::write(
            &first_env,
            "POLBAN_USERNAME=111\nPOLBAN_PASSWORD='a#b'\nPOLBAN_INTERVAL=30\n",
        )
        .unwrap();
        std::fs::write(&exe_env, "POLBAN_USERNAME=222\nPOLBAN_PASSWORD=xyz\n").unwrap();

        let candidates = build_env_candidates(
            Some(first_env.to_str().unwrap()),
            Some(&exe_dir),
            Some(&cwd_dir),
        );
        assert_eq!(candidates.len(), 4);
        assert_eq!(candidates[0], first_env);
        assert_eq!(candidates[1], exe_env);
        assert_eq!(candidates[2], cwd_dir.join(".env"));
        assert_eq!(candidates[3], dir.join(".env"));

        let mut base_env = HashMap::new();
        base_env.insert("POLBAN_USERNAME".into(), "from-process".into());
        let (cfg, used) =
            Config::load_from_candidates(&candidates, base_env, dir.join(".clicked.json")).unwrap();
        assert_eq!(used.as_deref(), Some(first_env.as_path()));
        assert_eq!(cfg.username, "111");
        assert_eq!(cfg.password, "a#b");
        assert_eq!(cfg.interval, 30);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn target_url_derives_base_url_for_mock() {
        let mut env_map = HashMap::new();
        env_map.insert(
            "POLBAN_TARGET_URL".into(),
            "http://127.0.0.1:4567/ajar/absen".into(),
        );
        let (cfg, _) =
            Config::load_from_candidates(&[], env_map, PathBuf::from(".clicked.json")).unwrap();
        assert_eq!(cfg.base_url, "http://127.0.0.1:4567");
        assert_eq!(cfg.login_url(), "http://127.0.0.1:4567/laman/login");
        assert_eq!(
            cfg.absen_endpoint(),
            "http://127.0.0.1:4567/ajar/absen/absensi_awal"
        );
    }
}

//! Non-blocking GitHub Release auto-updater with SHA256 verification and Windows self-replace.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Current application version embedded at compile time.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub Releases latest API endpoint for this repository.
pub const DEFAULT_RELEASE_API_URL: &str =
    "https://api.github.com/repos/raditted/Polban-Auto-Absen-rs/releases/latest";

/// GitHub Releases latest web URL (rate-limit-free fallback when REST API returns 403/429).
pub const DEFAULT_RELEASE_WEB_URL: &str =
    "https://github.com/raditted/Polban-Auto-Absen-rs/releases/latest";

const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);
const WINDOWS_PORTABLE_ZIP: &str = "Polban-Presensi-Portable-Windows-x64.zip";
const WINDOWS_INSTALLER_EXE: &str = "Polban-Presensi-Setup-Windows-x64.exe";
const SHA256SUMS_NAME: &str = "SHA256SUMS.txt";

/// Parsed Semantic Version (`major.minor.patch[-prerelease]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Option<String>,
}

/// Parse a version tag such as `"v3.0.2"` or `"3.0.1-rc.1"` into `SemVer`.
pub fn parse_semver(raw: &str) -> Option<SemVer> {
    let trimmed = raw.trim();
    let stripped = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    let without_build = stripped.split('+').next()?;
    let (core, pre) = match without_build.split_once('-') {
        Some((c, p)) if !p.is_empty() => (c, Some(p.to_string())),
        Some((c, _)) => (c, None),
        None => (without_build, None),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(SemVer {
        major,
        minor,
        patch,
        pre,
    })
}

fn compare_pre_identifiers(a: &str, b: &str) -> Ordering {
    let mut a_parts = a.split('.');
    let mut b_parts = b.split('.');
    loop {
        match (a_parts.next(), b_parts.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ai), Some(bi)) => {
                let ord = match (ai.parse::<u64>(), bi.parse::<u64>()) {
                    (Ok(an), Ok(bn)) => an.cmp(&bn),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => ai.cmp(bi),
                };
                if ord != Ordering::Equal {
                    return ord;
                }
            }
        }
    }
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> Ordering {
        let core_ord =
            (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch));
        if core_ord != Ordering::Equal {
            return core_ord;
        }
        match (&self.pre, &other.pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => compare_pre_identifiers(a, b),
        }
    }
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Normalize a release tag to bare `"major.minor.patch..."` without leading `'v'`.
pub fn normalize_version_str(raw: &str) -> String {
    let trimmed = raw.trim();
    trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed)
        .to_string()
}

/// Return `true` when `latest_tag` parses cleanly and is strictly newer than `current_version`.
pub fn is_newer_version(latest_tag: &str, current_version: &str) -> bool {
    match (parse_semver(latest_tag), parse_semver(current_version)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

/// GitHub Release asset metadata returned by the Releases API.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

/// Subset of the GitHub Releases API response required by the auto-updater.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GitHubRelease {
    pub tag_name: String,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub assets: Vec<GitHubAsset>,
}

/// Delivery strategy chosen for an available release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetStrategy {
    WindowsPortableZip,
    WindowsInstallerExe,
    OpenReleasePage,
}

/// Selected release asset and delivery strategy for the host environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedAsset {
    pub strategy: AssetStrategy,
    pub asset: Option<GitHubAsset>,
}

/// Select the best release artifact for `(target_os, target_arch, exe_dir_writable)`.
pub fn select_release_asset(
    assets: &[GitHubAsset],
    target_os: &str,
    _target_arch: &str,
    exe_dir_writable: bool,
) -> SelectedAsset {
    if target_os == "windows" {
        if exe_dir_writable {
            if let Some(zip_asset) = assets.iter().find(|a| a.name == WINDOWS_PORTABLE_ZIP) {
                return SelectedAsset {
                    strategy: AssetStrategy::WindowsPortableZip,
                    asset: Some(zip_asset.clone()),
                };
            }
        }
        if let Some(setup_asset) = assets.iter().find(|a| a.name == WINDOWS_INSTALLER_EXE) {
            return SelectedAsset {
                strategy: AssetStrategy::WindowsInstallerExe,
                asset: Some(setup_asset.clone()),
            };
        }
    }
    SelectedAsset {
        strategy: AssetStrategy::OpenReleasePage,
        asset: None,
    }
}

/// Metadata describing a newer release available for download or browser open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCandidate {
    pub version: String,
    pub release_url: String,
    pub strategy: AssetStrategy,
    pub asset: Option<GitHubAsset>,
    pub sha256sums_url: Option<String>,
}

/// Prepared update action executed when the user triggers `Restart untuk Update`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartPlan {
    ReplaceExecutable {
        current_exe: PathBuf,
        staged_new: PathBuf,
    },
    RunInstaller {
        installer_path: PathBuf,
    },
}

/// Live UI-facing state of the in-app updater.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(UpdateCandidate),
    Downloading {
        version: String,
        progress_pct: Option<u8>,
        verifying: bool,
    },
    ReadyToRestart {
        version: String,
        plan: RestartPlan,
    },
    Failed {
        message: String,
        retry_candidate: Option<UpdateCandidate>,
    },
}

/// Result of querying GitHub Releases for a newer version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    UpToDate,
    Available(UpdateCandidate),
}

enum DownloadEvent {
    Progress { pct: Option<u8>, verifying: bool },
    Ready(RestartPlan),
    Failed(String),
}

struct CheckMessage {
    manual: bool,
    result: Result<CheckOutcome, String>,
}

/// Parse a `SHA256SUMS.txt` file into a map of `basename -> lowercase hex digest`.
pub fn parse_sha256sums(contents: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next() else {
            continue;
        };
        let Some(file_part) = parts.next() else {
            continue;
        };
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let cleaned = file_part
            .trim_start_matches('*')
            .trim_start_matches("./")
            .trim_start_matches(".\\");
        let basename = cleaned.rsplit(['/', '\\']).next().unwrap_or(cleaned);
        if !basename.is_empty() {
            map.insert(basename.to_string(), hash.to_ascii_lowercase());
        }
    }
    map
}

/// Compute lowercase hexadecimal SHA-256 digest of `bytes`.
pub fn compute_sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Verify `bytes` against an expected 64-character hex SHA-256 digest.
pub fn verify_sha256(bytes: &[u8], expected_hex: &str) -> Result<(), String> {
    let expected = expected_hex.trim().to_ascii_lowercase();
    if expected.len() != 64 || !expected.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("Format hash SHA256 tidak valid.".to_string());
    }
    let actual = compute_sha256_hex(bytes);
    if actual != expected {
        return Err(format!(
            "Verifikasi SHA256 gagal: diharapkan {expected}, diperoleh {actual}."
        ));
    }
    Ok(())
}

/// Extract the first `.exe` file entry from `zip_bytes` into `dest_path`.
pub fn extract_first_exe_from_zip(zip_bytes: &[u8], dest_path: &Path) -> Result<(), String> {
    let cursor = Cursor::new(zip_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("Gagal membuka arsip ZIP: {e}"))?;

    for idx in 0..archive.len() {
        let mut entry = archive
            .by_index(idx)
            .map_err(|e| format!("Gagal membaca entri ZIP: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_ascii_lowercase();
        if !name.ends_with(".exe") {
            continue;
        }

        if let Some(parent) = dest_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Gagal membuat direktori update: {e}"))?;
            }
        }

        let mut out_file = File::create(dest_path)
            .map_err(|e| format!("Gagal menulis file update {}: {e}", dest_path.display()))?;
        std::io::copy(&mut entry, &mut out_file)
            .map_err(|e| format!("Gagal mengekstrak biner dari ZIP: {e}"))?;
        out_file
            .flush()
            .map_err(|e| format!("Gagal menyelesaikan penulisan file: {e}"))?;
        let _ = out_file.sync_all();
        return Ok(());
    }

    Err("Arsip ZIP tidak berisi file executable (.exe).".to_string())
}

/// Compute `<exe_dir>/.<exe_name>.new` for staging a Windows self-update.
pub fn staged_new_path_for(exe_path: &Path) -> PathBuf {
    let file_name = exe_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "polban-absen-gui.exe".to_string());
    let staged_name = format!(".{file_name}.new");
    match exe_path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(staged_name),
        _ => PathBuf::from(staged_name),
    }
}

/// Compute `<exe_path>.old` backup path for the currently running executable.
pub fn old_backup_path_for(exe_path: &Path) -> PathBuf {
    let mut os = exe_path.as_os_str().to_os_string();
    os.push(".old");
    PathBuf::from(os)
}

/// Remove any leftover `<exe_path>.old` from a previous self-update.
pub fn cleanup_old_executable_for(exe_path: &Path) {
    let old_path = old_backup_path_for(exe_path);
    if old_path.exists() {
        let _ = fs::remove_file(old_path);
    }
}

/// Best-effort startup cleanup for the running executable's `.old` backup file.
pub fn cleanup_old_executables() {
    if let Ok(exe_path) = std::env::current_exe() {
        cleanup_old_executable_for(&exe_path);
    }
}

/// Probe whether `dir` allows creating new files (distinguishes Program Files from user-writable dirs).
pub fn is_dir_writable(dir: &Path) -> bool {
    let probe_name = format!(
        ".polban-write-probe-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let probe_path = dir.join(probe_name);
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe_path)
    {
        Ok(f) => {
            drop(f);
            let _ = fs::remove_file(&probe_path);
            true
        }
        Err(_) => false,
    }
}

/// Rename `current_exe -> <current_exe>.old` and `staged_new -> current_exe`, rolling back on error.
pub fn stage_replace_and_backup(current_exe: &Path, staged_new: &Path) -> Result<PathBuf, String> {
    if !staged_new.exists() {
        return Err(format!(
            "File pembaruan tidak ditemukan: {}",
            staged_new.display()
        ));
    }
    let old_path = old_backup_path_for(current_exe);
    if old_path.exists() {
        fs::remove_file(&old_path).map_err(|e| {
            format!(
                "Gagal menghapus cadangan versi lama {}: {e}",
                old_path.display()
            )
        })?;
    }
    fs::rename(current_exe, &old_path).map_err(|e| {
        format!(
            "Gagal memindahkan aplikasi aktif ke {}: {e}",
            old_path.display()
        )
    })?;
    if let Err(err) = fs::rename(staged_new, current_exe) {
        let _ = fs::rename(&old_path, current_exe);
        return Err(format!(
            "Gagal mengganti executable dengan versi baru: {err}"
        ));
    }
    Ok(old_path)
}

fn spawn_detached(program: &Path) -> Result<(), String> {
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Gagal menjalankan {}: {e}", program.display()))
}

/// Release the single-instance lock first, replace/launch the updated binary or installer, and return.
pub fn apply_update_and_restart(plan: &RestartPlan, release_lock: &dyn Fn()) -> Result<(), String> {
    release_lock();
    match plan {
        RestartPlan::ReplaceExecutable {
            current_exe,
            staged_new,
        } => {
            stage_replace_and_backup(current_exe, staged_new)?;
            spawn_detached(current_exe)?;
        }
        RestartPlan::RunInstaller { installer_path } => {
            spawn_detached(installer_path)?;
        }
    }
    Ok(())
}

/// Return `true` only when `url` is an `http://` or `https://` URL with a non-empty host
/// and zero whitespace, control characters, or shell metacharacters.
pub fn is_safe_http_url(url: &str) -> bool {
    if url.is_empty()
        || url.chars().any(|c| {
            c.is_whitespace()
                || c.is_control()
                || matches!(
                    c,
                    '&' | '|'
                        | '^'
                        | '<'
                        | '>'
                        | '"'
                        | '\''
                        | '`'
                        | '$'
                        | '('
                        | ')'
                        | ';'
                        | '%'
                        | '!'
                        | '\\'
                )
        })
    {
        return false;
    }
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    matches!(parsed.scheme(), "http" | "https") && parsed.has_host()
}

/// Open a validated `http://` or `https://` URL in the user's default browser cleanly.
pub fn open_url_in_browser(url: &str) -> Result<(), String> {
    if !is_safe_http_url(url) {
        return Err("URL tidak valid atau skema tidak diizinkan.".to_string());
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Gagal membuka browser: {e}"))
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Gagal membuka browser: {e}"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Gagal membuka browser: {e}"))
    }
}

fn build_http_client(timeout: Duration) -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .user_agent(format!("polban-absen-gui/{CURRENT_VERSION}"))
        .build()
        .map_err(|e| format!("Gagal inisialisasi klien HTTP: {e}"))
}

fn build_no_redirect_http_client(timeout: Duration) -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(format!("polban-absen-gui/{CURRENT_VERSION}"))
        .build()
        .map_err(|e| format!("Gagal inisialisasi klien HTTP: {e}"))
}

fn url_origin(url: &reqwest::Url) -> Option<String> {
    let host = url.host_str()?;
    match url.port() {
        Some(port) => Some(format!("{}://{host}:{port}", url.scheme())),
        None => Some(format!("{}://{host}", url.scheme())),
    }
}

fn is_web_releases_latest_url(raw_url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(raw_url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or("");
    let path = parsed.path().trim_end_matches('/');
    (host.eq_ignore_ascii_case("github.com") || host.eq_ignore_ascii_case("www.github.com"))
        && path.ends_with("/releases/latest")
}

/// Derive the rate-limit-free web `.../releases/latest` URL from a GitHub Releases API URL.
pub fn derive_web_releases_url(api_url: &str) -> Option<String> {
    if is_web_releases_latest_url(api_url) {
        return Some(api_url.to_string());
    }
    let parsed = reqwest::Url::parse(api_url).ok()?;
    let trimmed_path = parsed.path().trim_end_matches('/');
    let rest = trimmed_path.strip_prefix("/repos/")?;
    let repo_prefix = rest.strip_suffix("/releases/latest")?;
    let mut parts = repo_prefix.split('/');
    let owner = parts.next().filter(|s| !s.is_empty())?;
    let repo = parts.next().filter(|s| !s.is_empty())?;
    if parts.next().is_some() {
        return None;
    }

    if parsed
        .host_str()
        .is_some_and(|h| h.eq_ignore_ascii_case("api.github.com"))
    {
        Some(format!("https://github.com/{owner}/{repo}/releases/latest"))
    } else {
        let origin = url_origin(&parsed)?;
        Some(format!("{origin}/{owner}/{repo}/releases/latest"))
    }
}

/// Extract `(tag_name, release_url, download_base_url)` from a `.../releases/tag/<tag>` URL.
fn parse_release_tag_url(url: &reqwest::Url) -> Option<(String, String, String)> {
    let path = url.path();
    let idx = path.find("/releases/tag/")?;
    let repo_prefix = &path[..idx];
    let after = &path[idx + "/releases/tag/".len()..];
    let tag = after.split('/').next()?.trim();
    if tag.is_empty() {
        return None;
    }
    let origin = url_origin(url)?;
    let release_url = format!("{origin}{repo_prefix}/releases/tag/{tag}");
    let download_base = format!("{origin}{repo_prefix}/releases/download/{tag}");
    Some((tag.to_string(), release_url, download_base))
}

fn resolve_latest_tag_via_web_redirect(
    web_latest_url: &str,
) -> Result<(String, String, String), String> {
    let client = build_no_redirect_http_client(CHECK_TIMEOUT)?;
    let mut current = reqwest::Url::parse(web_latest_url)
        .map_err(|e| format!("URL fallback rilis tidak valid: {e}"))?;

    for _hop in 0..5 {
        if let Some(parsed) = parse_release_tag_url(&current) {
            return Ok(parsed);
        }

        let resp = client
            .get(current.clone())
            .send()
            .map_err(|e| format!("Gagal menghubungi halaman rilis GitHub: {e}"))?;
        let status = resp.status();

        if status.is_redirection() {
            let loc = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| "Halaman rilis mengalihkan tanpa header Location.".to_string())?;
            let next_url = current
                .join(loc)
                .map_err(|e| format!("Header Location rilis tidak valid: {e}"))?;
            if let Some(parsed) = parse_release_tag_url(&next_url) {
                return Ok(parsed);
            }
            current = next_url;
            continue;
        }

        if status.is_success() {
            if let Some(parsed) = parse_release_tag_url(resp.url()) {
                return Ok(parsed);
            }
            return Err("Halaman rilis tidak mengarahkan ke tag versi terbaru.".to_string());
        }

        return Err(format!("Halaman rilis mengembalikan status HTTP {status}"));
    }

    Err("Terlalu banyak pengalihan saat memeriksa halaman rilis.".to_string())
}

fn discover_assets_from_web_release(
    client: &reqwest::blocking::Client,
    download_base: &str,
    exe_dir_writable: bool,
) -> (Vec<GitHubAsset>, Option<String>) {
    let sums_url = format!("{download_base}/{SHA256SUMS_NAME}");
    if let Ok(sums_resp) = client.get(&sums_url).send() {
        if sums_resp.status().is_success() {
            if let Ok(sums_text) = sums_resp.text() {
                let map = parse_sha256sums(&sums_text);
                if !map.is_empty() {
                    let mut assets: Vec<GitHubAsset> = map
                        .into_keys()
                        .map(|name| GitHubAsset {
                            browser_download_url: format!("{download_base}/{name}"),
                            name,
                            size: 0,
                        })
                        .collect();
                    assets.sort_by(|a, b| a.name.cmp(&b.name));
                    return (assets, Some(sums_url));
                }
            }
        }
    }

    // Fallback when SHA256SUMS.txt is absent: probe known Windows assets via HTTP HEAD.
    let mut probe_names = Vec::new();
    if std::env::consts::OS == "windows" {
        if exe_dir_writable {
            probe_names.push(WINDOWS_PORTABLE_ZIP);
        }
        probe_names.push(WINDOWS_INSTALLER_EXE);
    }

    let mut probed = Vec::new();
    for name in probe_names {
        let asset_url = format!("{download_base}/{name}");
        if let Ok(head_resp) = client.head(&asset_url).send() {
            if head_resp.status().is_success() {
                let size = head_resp.content_length().unwrap_or(0);
                probed.push(GitHubAsset {
                    name: name.to_string(),
                    browser_download_url: asset_url,
                    size,
                });
            }
        }
    }

    (probed, None)
}

fn check_latest_release_via_web(
    client: &reqwest::blocking::Client,
    web_latest_url: &str,
    current_version: &str,
    exe_dir_writable: bool,
) -> Result<CheckOutcome, String> {
    let (tag_name, release_url, download_base) =
        resolve_latest_tag_via_web_redirect(web_latest_url)?;
    if !is_newer_version(&tag_name, current_version) {
        return Ok(CheckOutcome::UpToDate);
    }

    let version = normalize_version_str(&tag_name);
    let (assets, sha256sums_url) =
        discover_assets_from_web_release(client, &download_base, exe_dir_writable);
    let selected = select_release_asset(
        &assets,
        std::env::consts::OS,
        std::env::consts::ARCH,
        exe_dir_writable,
    );

    Ok(CheckOutcome::Available(UpdateCandidate {
        version,
        release_url,
        strategy: selected.strategy,
        asset: selected.asset,
        sha256sums_url,
    }))
}

/// Query GitHub Releases JSON at `api_url` (with automatic rate-limit web fallback on HTTP 403/429)
/// and return whether a newer version is available.
pub fn check_latest_release(
    api_url: &str,
    current_version: &str,
    exe_dir_writable: bool,
) -> Result<CheckOutcome, String> {
    check_latest_release_with_fallback(api_url, None, current_version, exe_dir_writable)
}

/// Query GitHub Releases JSON at `api_url`, falling back to `web_fallback_url` (or derived web URL)
/// when the REST API returns HTTP 403 or 429 due to rate limits.
pub fn check_latest_release_with_fallback(
    api_url: &str,
    web_fallback_url: Option<&str>,
    current_version: &str,
    exe_dir_writable: bool,
) -> Result<CheckOutcome, String> {
    let client = build_http_client(CHECK_TIMEOUT)?;

    if is_web_releases_latest_url(api_url) {
        return check_latest_release_via_web(&client, api_url, current_version, exe_dir_writable);
    }

    let resp = client
        .get(api_url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .map_err(|e| format!("Gagal menghubungi GitHub Releases: {e}"))?;

    let status = resp.status();
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        let fallback_url = web_fallback_url
            .map(str::to_string)
            .or_else(|| derive_web_releases_url(api_url));

        if let Some(web_url) = fallback_url {
            return check_latest_release_via_web(
                &client,
                &web_url,
                current_version,
                exe_dir_writable,
            )
            .map_err(|fallback_err| {
                format!(
                    "Batas akses GitHub API tercapai (HTTP {status}) dan pemeriksaan cadangan gagal: {fallback_err}"
                )
            });
        }

        return Err(format!(
            "Batas akses GitHub API tercapai (HTTP {status}). Silakan coba lagi beberapa saat lagi."
        ));
    }

    if !status.is_success() {
        return Err(format!("Server mengembalikan status HTTP {status}"));
    }

    let body = resp
        .text()
        .map_err(|e| format!("Gagal membaca respons rilis: {e}"))?;
    let release: GitHubRelease =
        serde_json::from_str(&body).map_err(|e| format!("Format JSON rilis tidak valid: {e}"))?;

    if release.draft || !is_newer_version(&release.tag_name, current_version) {
        return Ok(CheckOutcome::UpToDate);
    }

    let version = normalize_version_str(&release.tag_name);
    let selected = select_release_asset(
        &release.assets,
        std::env::consts::OS,
        std::env::consts::ARCH,
        exe_dir_writable,
    );
    let sha256sums_url = release
        .assets
        .iter()
        .find(|a| a.name == SHA256SUMS_NAME)
        .map(|a| a.browser_download_url.clone());

    Ok(CheckOutcome::Available(UpdateCandidate {
        version,
        release_url: release.html_url,
        strategy: selected.strategy,
        asset: selected.asset,
        sha256sums_url,
    }))
}

/// Download `candidate`, verify against `SHA256SUMS.txt` when present, and stage the `RestartPlan`.
pub fn download_and_stage_candidate(
    candidate: &UpdateCandidate,
    current_exe: &Path,
    installer_dir: &Path,
    mut on_progress: impl FnMut(Option<u8>, bool),
) -> Result<RestartPlan, String> {
    let asset = candidate
        .asset
        .as_ref()
        .ok_or_else(|| "Aset pembaruan untuk platform ini tidak tersedia.".to_string())?;

    let client = build_http_client(DOWNLOAD_TIMEOUT)?;

    let expected_sha256 = if let Some(sums_url) = &candidate.sha256sums_url {
        let sums_resp = client
            .get(sums_url)
            .send()
            .map_err(|e| format!("Gagal mengunduh {SHA256SUMS_NAME}: {e}"))?;
        if !sums_resp.status().is_success() {
            return Err(format!(
                "Gagal mengunduh {SHA256SUMS_NAME}: HTTP {}",
                sums_resp.status()
            ));
        }
        let sums_text = sums_resp
            .text()
            .map_err(|e| format!("Gagal membaca {SHA256SUMS_NAME}: {e}"))?;
        let map = parse_sha256sums(&sums_text);
        let digest = map.get(&asset.name).cloned().ok_or_else(|| {
            format!(
                "Checksum SHA256 untuk {} tidak ditemukan di {SHA256SUMS_NAME}.",
                asset.name
            )
        })?;
        Some(digest)
    } else {
        None
    };

    let mut resp = client
        .get(&asset.browser_download_url)
        .send()
        .map_err(|e| format!("Gagal mengunduh pembaruan: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Gagal mengunduh pembaruan: HTTP {}", resp.status()));
    }

    let total_len = resp
        .content_length()
        .filter(|&n| n > 0)
        .or_else(|| (asset.size > 0).then_some(asset.size));

    let mut bytes =
        Vec::with_capacity(total_len.unwrap_or(1024 * 1024).min(64 * 1024 * 1024) as usize);
    let mut buf = [0u8; 32 * 1024];
    let mut last_pct: Option<u8> = None;
    let mut last_emit = Instant::now();
    on_progress(total_len.map(|_| 0), false);

    loop {
        let n = resp
            .read(&mut buf)
            .map_err(|e| format!("Koneksi terputus saat mengunduh pembaruan: {e}"))?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        let pct = total_len.map(|tot| ((bytes.len() as u64 * 100) / tot).min(100) as u8);
        if pct != last_pct && (last_emit.elapsed() >= Duration::from_millis(50) || pct == Some(100))
        {
            last_pct = pct;
            last_emit = Instant::now();
            on_progress(pct, false);
        }
    }

    on_progress(Some(100), true);
    if let Some(expected) = expected_sha256 {
        verify_sha256(&bytes, &expected)?;
    }

    match candidate.strategy {
        AssetStrategy::WindowsPortableZip => {
            let staged_new = staged_new_path_for(current_exe);
            extract_first_exe_from_zip(&bytes, &staged_new)?;
            Ok(RestartPlan::ReplaceExecutable {
                current_exe: current_exe.to_path_buf(),
                staged_new,
            })
        }
        AssetStrategy::WindowsInstallerExe => {
            fs::create_dir_all(installer_dir)
                .map_err(|e| format!("Gagal membuat folder unduhan installer: {e}"))?;
            let installer_path = installer_dir.join(&asset.name);
            let mut out = File::create(&installer_path)
                .map_err(|e| format!("Gagal menyimpan installer: {e}"))?;
            out.write_all(&bytes)
                .map_err(|e| format!("Gagal menulis file installer: {e}"))?;
            out.flush()
                .map_err(|e| format!("Gagal menyelesaikan file installer: {e}"))?;
            let _ = out.sync_all();
            Ok(RestartPlan::RunInstaller { installer_path })
        }
        AssetStrategy::OpenReleasePage => {
            Err("Platform ini memperbarui lewat halaman rilis.".to_string())
        }
    }
}

/// Non-blocking coordinator between the egui UI and background update worker threads.
pub struct UpdateManager {
    pub status: UpdateStatus,
    pub checking_manual: bool,
    pub manual_note: Option<(String, bool)>,
    api_url: String,
    last_candidate: Option<UpdateCandidate>,
    check_rx: Option<mpsc::Receiver<CheckMessage>>,
    download_rx: Option<mpsc::Receiver<DownloadEvent>>,
}

impl Default for UpdateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdateManager {
    pub fn new() -> Self {
        Self {
            status: UpdateStatus::Idle,
            checking_manual: false,
            manual_note: None,
            api_url: DEFAULT_RELEASE_API_URL.to_string(),
            last_candidate: None,
            check_rx: None,
            download_rx: None,
        }
    }

    pub fn current_version(&self) -> &'static str {
        CURRENT_VERSION
    }

    /// Trigger a silent one-shot background check after startup (no-op if already busy).
    pub fn start_silent_check(&mut self, ctx: &egui::Context) {
        if self.check_rx.is_some()
            || matches!(
                self.status,
                UpdateStatus::Downloading { .. } | UpdateStatus::ReadyToRestart { .. }
            )
        {
            return;
        }
        self.status = UpdateStatus::Checking;
        self.spawn_check(ctx, false);
    }

    /// Trigger an explicit check from Settings with inline status feedback.
    pub fn start_manual_check(&mut self, ctx: &egui::Context) {
        if self.check_rx.is_some() || matches!(self.status, UpdateStatus::Downloading { .. }) {
            return;
        }
        self.checking_manual = true;
        self.manual_note = Some(("Memeriksa pembaruan...".to_string(), false));
        self.spawn_check(ctx, true);
    }

    fn spawn_check(&mut self, ctx: &egui::Context, manual: bool) {
        let (tx, rx) = mpsc::channel();
        self.check_rx = Some(rx);
        let api_url = self.api_url.clone();
        let wake_ctx = ctx.clone();
        let writable = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(is_dir_writable))
            .unwrap_or(false);

        if std::thread::Builder::new()
            .name("polban-update-check".to_string())
            .spawn(move || {
                let result = check_latest_release(&api_url, CURRENT_VERSION, writable);
                let _ = tx.send(CheckMessage { manual, result });
                wake_ctx.request_repaint();
            })
            .is_err()
        {
            self.check_rx = None;
            self.checking_manual = false;
            if !manual {
                self.status = UpdateStatus::Idle;
            } else {
                self.manual_note = Some((
                    "Gagal membuat thread pemeriksa pembaruan.".to_string(),
                    true,
                ));
            }
        }
    }

    /// Start downloading (or open release page on non-Windows) for the available candidate.
    pub fn start_download(&mut self, ctx: &egui::Context) {
        if self.download_rx.is_some() {
            return;
        }
        let candidate = match &self.status {
            UpdateStatus::Available(c) => c.clone(),
            UpdateStatus::Failed {
                retry_candidate: Some(c),
                ..
            } => c.clone(),
            _ => return,
        };

        if matches!(candidate.strategy, AssetStrategy::OpenReleasePage) || candidate.asset.is_none()
        {
            if let Err(err) = open_url_in_browser(&candidate.release_url) {
                self.manual_note = Some((err, true));
            }
            return;
        }

        let current_exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(err) => {
                self.status = UpdateStatus::Failed {
                    message: format!("Lokasi aplikasi tidak terbaca: {err}"),
                    retry_candidate: Some(candidate),
                };
                return;
            }
        };
        let installer_dir = std::env::temp_dir().join("polban-presensi-update");
        let version = candidate.version.clone();
        let retry_copy = candidate.clone();
        self.last_candidate = Some(candidate.clone());
        self.status = UpdateStatus::Downloading {
            version,
            progress_pct: Some(0),
            verifying: false,
        };

        let (tx, rx) = mpsc::channel();
        self.download_rx = Some(rx);
        let wake_ctx = ctx.clone();

        if std::thread::Builder::new()
            .name("polban-update-download".to_string())
            .spawn(move || {
                let res = download_and_stage_candidate(
                    &candidate,
                    &current_exe,
                    &installer_dir,
                    |pct, verifying| {
                        let _ = tx.send(DownloadEvent::Progress { pct, verifying });
                        wake_ctx.request_repaint();
                    },
                );
                match res {
                    Ok(plan) => {
                        let _ = tx.send(DownloadEvent::Ready(plan));
                    }
                    Err(err) => {
                        let _ = tx.send(DownloadEvent::Failed(err));
                    }
                }
                wake_ctx.request_repaint();
            })
            .is_err()
        {
            self.download_rx = None;
            self.status = UpdateStatus::Failed {
                message: "Gagal menjalankan thread pengunduhan.".to_string(),
                retry_candidate: Some(retry_copy),
            };
        }
    }

    /// Poll background update channels without blocking the UI thread.
    pub fn poll(&mut self) {
        if let Some(rx) = &self.check_rx {
            match rx.try_recv() {
                Ok(msg) => {
                    self.check_rx = None;
                    if msg.manual {
                        self.checking_manual = false;
                    }
                    match msg.result {
                        Ok(CheckOutcome::UpToDate) => {
                            if msg.manual {
                                self.manual_note = Some((
                                    format!("Sudah versi terbaru (v{CURRENT_VERSION})."),
                                    false,
                                ));
                            }
                            if !matches!(
                                self.status,
                                UpdateStatus::Downloading { .. }
                                    | UpdateStatus::ReadyToRestart { .. }
                            ) {
                                self.status = UpdateStatus::UpToDate;
                            }
                        }
                        Ok(CheckOutcome::Available(candidate)) => {
                            if msg.manual {
                                self.manual_note = Some((
                                    format!("Tersedia versi v{}.", candidate.version),
                                    false,
                                ));
                            }
                            if !matches!(
                                self.status,
                                UpdateStatus::Downloading { .. }
                                    | UpdateStatus::ReadyToRestart { .. }
                            ) {
                                self.status = UpdateStatus::Available(candidate);
                            }
                        }
                        Err(err) => {
                            if msg.manual {
                                self.manual_note =
                                    Some((format!("Gagal memeriksa pembaruan: {err}"), true));
                            } else if matches!(self.status, UpdateStatus::Checking) {
                                // Startup checks must remain completely silent on offline/rate-limit errors.
                                self.status = UpdateStatus::Idle;
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.check_rx = None;
                    self.checking_manual = false;
                    if matches!(self.status, UpdateStatus::Checking) {
                        self.status = UpdateStatus::Idle;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }

        let mut finish_download = false;
        if let Some(rx) = &self.download_rx {
            loop {
                match rx.try_recv() {
                    Ok(DownloadEvent::Progress { pct, verifying }) => {
                        if let UpdateStatus::Downloading {
                            progress_pct,
                            verifying: ver_flag,
                            ..
                        } = &mut self.status
                        {
                            *progress_pct = pct;
                            *ver_flag = verifying;
                        }
                    }
                    Ok(DownloadEvent::Ready(plan)) => {
                        let version = match &self.status {
                            UpdateStatus::Downloading { version, .. } => version.clone(),
                            _ => String::new(),
                        };
                        self.status = UpdateStatus::ReadyToRestart { version, plan };
                        finish_download = true;
                        break;
                    }
                    Ok(DownloadEvent::Failed(message)) => {
                        self.manual_note = Some((message.clone(), true));
                        self.status = UpdateStatus::Failed {
                            message,
                            retry_candidate: self.last_candidate.clone(),
                        };
                        finish_download = true;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.status = UpdateStatus::Failed {
                            message: "Proses pengunduhan berhenti sebelum selesai.".to_string(),
                            retry_candidate: self.last_candidate.clone(),
                        };
                        finish_download = true;
                        break;
                    }
                }
            }
        }
        if finish_download {
            self.download_rx = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn make_temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "polban-updater-test-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp test dir");
        dir
    }

    fn build_zip_with_exe(exe_name: &str, payload: &[u8]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            writer
                .start_file("README.txt", options)
                .expect("start readme");
            writer.write_all(b"Polban Presensi").expect("write readme");
            writer.start_file(exe_name, options).expect("start exe");
            writer.write_all(payload).expect("write exe");
            writer.finish().expect("finish zip");
        }
        cursor.into_inner()
    }

    #[test]
    fn semver_parsing_and_comparison_handle_v_prefix_and_prerelease() {
        assert!(is_newer_version("v3.0.2", "3.0.1"));
        assert!(is_newer_version("V3.1.0", "3.0.9"));
        assert!(is_newer_version("4.0.0", "3.9.9"));
        assert!(!is_newer_version("v3.0.1", "3.0.1"));
        assert!(!is_newer_version("v3.0.0", "3.0.1"));
        assert!(!is_newer_version("v3.0.1-rc.1", "3.0.1"));
        assert!(is_newer_version("v3.0.1", "3.0.1-rc.1"));
        assert!(is_newer_version("v3.0.1-rc.2", "3.0.1-rc.1"));
        assert!(!is_newer_version("invalid-tag", "3.0.1"));
    }

    #[test]
    fn sha256sums_parser_and_verification_detect_valid_and_tampered_bytes() {
        let good_bytes = b"polban-presensi-binary-v3.0.2";
        let good_hex = compute_sha256_hex(good_bytes);
        let sums = format!(
            "# release checksums\n\
             {good_hex}  ./{WINDOWS_PORTABLE_ZIP}\n\
             aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa *{WINDOWS_INSTALLER_EXE}\n"
        );
        let map = parse_sha256sums(&sums);
        assert_eq!(map.get(WINDOWS_PORTABLE_ZIP), Some(&good_hex));
        assert_eq!(
            map.get(WINDOWS_INSTALLER_EXE).map(String::as_str),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );

        assert!(verify_sha256(good_bytes, &good_hex).is_ok());
        let err = verify_sha256(b"tampered-binary", &good_hex).expect_err("must reject mismatch");
        assert!(err.contains("SHA256"));
    }

    #[test]
    fn platform_asset_selection_prefers_portable_zip_when_writable_and_falls_back_to_installer() {
        let assets = vec![
            GitHubAsset {
                name: WINDOWS_INSTALLER_EXE.to_string(),
                browser_download_url: "https://example.test/setup.exe".to_string(),
                size: 200,
            },
            GitHubAsset {
                name: WINDOWS_PORTABLE_ZIP.to_string(),
                browser_download_url: "https://example.test/portable.zip".to_string(),
                size: 100,
            },
            GitHubAsset {
                name: "Polban-Presensi-Linux-x64.tar.gz".to_string(),
                browser_download_url: "https://example.test/linux.tar.gz".to_string(),
                size: 150,
            },
        ];

        let writable_win = select_release_asset(&assets, "windows", "x86_64", true);
        assert_eq!(writable_win.strategy, AssetStrategy::WindowsPortableZip);
        assert_eq!(
            writable_win.asset.as_ref().map(|a| a.name.as_str()),
            Some(WINDOWS_PORTABLE_ZIP)
        );

        let readonly_win = select_release_asset(&assets, "windows", "x86_64", false);
        assert_eq!(readonly_win.strategy, AssetStrategy::WindowsInstallerExe);
        assert_eq!(
            readonly_win.asset.as_ref().map(|a| a.name.as_str()),
            Some(WINDOWS_INSTALLER_EXE)
        );

        let linux_fallback = select_release_asset(&assets, "linux", "x86_64", true);
        assert_eq!(linux_fallback.strategy, AssetStrategy::OpenReleasePage);
        assert!(linux_fallback.asset.is_none());
    }

    #[test]
    fn zip_extraction_and_backup_replace_flow_work_cleanly() {
        let dir = make_temp_dir("zip-replace");
        let current_exe = dir.join("Polban Presensi.exe");
        fs::write(&current_exe, b"old-exe-v3.0.1").expect("write current exe");

        // Stale .old from an earlier update should be cleaned up cleanly.
        let stale_old = old_backup_path_for(&current_exe);
        fs::write(&stale_old, b"ancient-exe-v3.0.0").expect("write stale old");
        cleanup_old_executable_for(&current_exe);
        assert!(!stale_old.exists());

        let zip_bytes = build_zip_with_exe("Polban Presensi.exe", b"new-exe-v3.0.2");
        let staged_new = staged_new_path_for(&current_exe);
        extract_first_exe_from_zip(&zip_bytes, &staged_new).expect("extract exe");
        assert_eq!(fs::read(&staged_new).unwrap(), b"new-exe-v3.0.2");

        let backup = stage_replace_and_backup(&current_exe, &staged_new).expect("replace exe");
        assert_eq!(backup, stale_old);
        assert_eq!(fs::read(&current_exe).unwrap(), b"new-exe-v3.0.2");
        assert_eq!(fs::read(&backup).unwrap(), b"old-exe-v3.0.1");
        assert!(!staged_new.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn httpmock_check_download_and_sha256_verification_end_to_end() {
        let server = MockServer::start();
        let zip_payload = build_zip_with_exe("Polban Presensi.exe", b"verified-new-binary");
        let good_sha = compute_sha256_hex(&zip_payload);
        let sums_body = format!("{good_sha}  {WINDOWS_PORTABLE_ZIP}\n");

        let api_mock = server.mock(|when, then| {
            when.method(GET).path("/releases/latest");
            then.status(200)
                .header("content-type", "application/json")
                .body(format!(
                    r#"{{
                        "tag_name": "v9.9.9",
                        "html_url": "{}/release",
                        "draft": false,
                        "prerelease": false,
                        "assets": [
                            {{
                                "name": "{WINDOWS_PORTABLE_ZIP}",
                                "browser_download_url": "{}/download/portable.zip",
                                "size": {}
                            }},
                            {{
                                "name": "{SHA256SUMS_NAME}",
                                "browser_download_url": "{}/download/SHA256SUMS.txt",
                                "size": {}
                            }}
                        ]
                    }}"#,
                    server.base_url(),
                    server.base_url(),
                    zip_payload.len(),
                    server.base_url(),
                    sums_body.len(),
                ));
        });

        let zip_mock = server.mock(|when, then| {
            when.method(GET).path("/download/portable.zip");
            then.status(200).body(zip_payload.clone());
        });

        let sums_mock = server.mock(|when, then| {
            when.method(GET).path("/download/SHA256SUMS.txt");
            then.status(200).body(sums_body.clone());
        });

        let outcome = check_latest_release(&server.url("/releases/latest"), "3.0.1", true)
            .expect("check release");
        api_mock.assert();

        let candidate = match outcome {
            CheckOutcome::Available(c) => c,
            other => panic!("expected Available, got {other:?}"),
        };
        assert_eq!(candidate.version, "9.9.9");

        // Force WindowsPortableZip plan test regardless of host OS check by supplying candidate asset directly.
        let win_candidate = UpdateCandidate {
            strategy: AssetStrategy::WindowsPortableZip,
            asset: Some(GitHubAsset {
                name: WINDOWS_PORTABLE_ZIP.to_string(),
                browser_download_url: server.url("/download/portable.zip"),
                size: zip_payload.len() as u64,
            }),
            ..candidate
        };

        let dir = make_temp_dir("httpmock-e2e");
        let exe_path = dir.join("Polban Presensi.exe");
        fs::write(&exe_path, b"old-binary").expect("write dummy exe");

        let mut progress_events = Vec::new();
        let plan = download_and_stage_candidate(&win_candidate, &exe_path, &dir, |pct, ver| {
            progress_events.push((pct, ver));
        })
        .expect("verified download should succeed");

        zip_mock.assert();
        sums_mock.assert();
        assert!(progress_events.iter().any(|(_, ver)| *ver));
        assert_eq!(
            plan,
            RestartPlan::ReplaceExecutable {
                current_exe: exe_path.clone(),
                staged_new: staged_new_path_for(&exe_path),
            }
        );

        // Now verify tampered SHA256SUMS.txt rejects the download cleanly.
        let bad_sums_mock = server.mock(|when, then| {
            when.method(GET).path("/download/BADSUMS.txt");
            then.status(200).body(format!(
                "0000000000000000000000000000000000000000000000000000000000000000  {WINDOWS_PORTABLE_ZIP}\n"
            ));
        });
        let tampered_candidate = UpdateCandidate {
            sha256sums_url: Some(server.url("/download/BADSUMS.txt")),
            ..win_candidate
        };
        let err = download_and_stage_candidate(&tampered_candidate, &exe_path, &dir, |_, _| {})
            .expect_err("tampered download must be rejected");
        bad_sums_mock.assert();
        assert!(err.contains("SHA256"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_safe_http_url_and_open_url_reject_unsafe_schemes_and_metacharacters() {
        assert!(is_safe_http_url("https://akademik.polban.ac.id/ajar/absen"));
        assert!(is_safe_http_url("http://127.0.0.1:8080/ajar/absen"));

        let rejected = [
            "",
            "   ",
            "javascript:alert(1)",
            "file:///C:/Windows/System32/calc.exe",
            "ms-msdt:/id PCWDiagnostic",
            "https://akademik.polban.ac.id/ajar/absen & calc.exe",
            "https://akademik.polban.ac.id/ajar/absen|calc",
            "https://akademik.polban.ac.id/ajar/absen^calc",
            "https://akademik.polban.ac.id/ajar/absen>out.txt",
            "https://akademik.polban.ac.id/ajar/\"absen\"",
            "https://akademik.polban.ac.id/ajar/%COMSPEC%",
            "https://",
            " http://akademik.polban.ac.id",
        ];
        for bad in rejected {
            assert!(!is_safe_http_url(bad), "expected rejection for {bad:?}");
            let err = open_url_in_browser(bad).expect_err("unsafe URL must not spawn browser");
            assert!(err.contains("tidak valid"));
        }
    }

    #[test]
    fn derive_web_releases_url_maps_github_and_mock_repo_urls() {
        assert_eq!(
            derive_web_releases_url(DEFAULT_RELEASE_API_URL).as_deref(),
            Some(DEFAULT_RELEASE_WEB_URL)
        );
        assert_eq!(
            derive_web_releases_url(
                "http://127.0.0.1:8080/repos/raditted/Polban-Auto-Absen-rs/releases/latest"
            )
            .as_deref(),
            Some("http://127.0.0.1:8080/raditted/Polban-Auto-Absen-rs/releases/latest")
        );
        assert_eq!(
            derive_web_releases_url(DEFAULT_RELEASE_WEB_URL).as_deref(),
            Some(DEFAULT_RELEASE_WEB_URL)
        );
        assert_eq!(
            derive_web_releases_url("http://127.0.0.1:8080/releases/latest"),
            None
        );
    }

    #[test]
    fn rate_limit_403_falls_back_to_web_redirect_and_sha256sums_discovery() {
        let server = MockServer::start();
        let zip_payload = build_zip_with_exe("Polban Presensi.exe", b"fallback-new-binary");
        let good_sha = compute_sha256_hex(&zip_payload);
        let sums_body = format!(
            "{good_sha}  {WINDOWS_PORTABLE_ZIP}\n\
             bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  {WINDOWS_INSTALLER_EXE}\n"
        );

        let api_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/raditted/Polban-Auto-Absen-rs/releases/latest");
            then.status(403)
                .header("X-RateLimit-Remaining", "0")
                .body(r#"{"message":"API rate limit exceeded for 104.28.250.135"}"#);
        });

        let web_latest_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/raditted/Polban-Auto-Absen-rs/releases/latest");
            then.status(302).header(
                "Location",
                "/raditted/Polban-Auto-Absen-rs/releases/tag/v9.9.9",
            );
        });

        let sums_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/raditted/Polban-Auto-Absen-rs/releases/download/v9.9.9/SHA256SUMS.txt");
            then.status(200).body(sums_body.clone());
        });

        let zip_mock = server.mock(|when, then| {
            when.method(GET).path(format!(
                "/raditted/Polban-Auto-Absen-rs/releases/download/v9.9.9/{WINDOWS_PORTABLE_ZIP}"
            ));
            then.status(200).body(zip_payload.clone());
        });

        let outcome = check_latest_release(
            &server.url("/repos/raditted/Polban-Auto-Absen-rs/releases/latest"),
            "3.2.1",
            true,
        )
        .expect("403 rate limit should recover via web redirect fallback");

        api_mock.assert();
        web_latest_mock.assert();
        sums_mock.assert();

        let candidate = match outcome {
            CheckOutcome::Available(c) => c,
            other => panic!("expected Available, got {other:?}"),
        };
        assert_eq!(candidate.version, "9.9.9");
        assert_eq!(
            candidate.release_url,
            server.url("/raditted/Polban-Auto-Absen-rs/releases/tag/v9.9.9")
        );
        assert_eq!(
            candidate.sha256sums_url,
            Some(
                server
                    .url("/raditted/Polban-Auto-Absen-rs/releases/download/v9.9.9/SHA256SUMS.txt")
            )
        );

        if cfg!(target_os = "windows") {
            assert_eq!(candidate.strategy, AssetStrategy::WindowsPortableZip);
            assert_eq!(
                candidate.asset.as_ref().map(|a| a.browser_download_url.as_str()),
                Some(
                    server
                        .url(format!(
                            "/raditted/Polban-Auto-Absen-rs/releases/download/v9.9.9/{WINDOWS_PORTABLE_ZIP}"
                        ))
                        .as_str()
                )
            );

            let dir = make_temp_dir("fallback-e2e");
            let exe_path = dir.join("Polban Presensi.exe");
            fs::write(&exe_path, b"old-binary").expect("write dummy exe");
            let plan = download_and_stage_candidate(&candidate, &exe_path, &dir, |_, _| {})
                .expect("download from fallback candidate should verify and stage");
            zip_mock.assert();
            assert_eq!(
                plan,
                RestartPlan::ReplaceExecutable {
                    current_exe: exe_path.clone(),
                    staged_new: staged_new_path_for(&exe_path),
                }
            );
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn rate_limit_403_web_redirect_to_current_or_older_tag_returns_up_to_date() {
        let server = MockServer::start();

        let api_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/raditted/Polban-Auto-Absen-rs/releases/latest");
            then.status(403).body("rate limit exceeded");
        });
        let web_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/raditted/Polban-Auto-Absen-rs/releases/latest");
            then.status(302).header(
                "Location",
                server.url("/raditted/Polban-Auto-Absen-rs/releases/tag/v3.2.1"),
            );
        });
        let sums_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/raditted/Polban-Auto-Absen-rs/releases/download/v3.2.1/SHA256SUMS.txt");
            then.status(200).body("");
        });

        let outcome = check_latest_release(
            &server.url("/repos/raditted/Polban-Auto-Absen-rs/releases/latest"),
            "3.2.1",
            true,
        )
        .expect("same tag via web fallback must succeed as UpToDate");

        api_mock.assert();
        web_mock.assert();
        sums_mock.assert_hits(0);
        assert_eq!(outcome, CheckOutcome::UpToDate);
    }

    #[test]
    fn rate_limit_429_without_sha256sums_probes_head_or_falls_back_to_release_page() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET)
                .path("/repos/raditted/Polban-Auto-Absen-rs/releases/latest");
            then.status(429).body("too many requests");
        });
        server.mock(|when, then| {
            when.method(GET)
                .path("/raditted/Polban-Auto-Absen-rs/releases/latest");
            then.status(302).header(
                "Location",
                "/raditted/Polban-Auto-Absen-rs/releases/tag/v9.9.9",
            );
        });
        server.mock(|when, then| {
            when.method(GET)
                .path("/raditted/Polban-Auto-Absen-rs/releases/download/v9.9.9/SHA256SUMS.txt");
            then.status(404);
        });
        let head_zip = server.mock(|when, then| {
            when.method(httpmock::Method::HEAD).path(format!(
                "/raditted/Polban-Auto-Absen-rs/releases/download/v9.9.9/{WINDOWS_PORTABLE_ZIP}"
            ));
            then.status(200).header("content-length", "12345");
        });

        let outcome = check_latest_release(
            &server.url("/repos/raditted/Polban-Auto-Absen-rs/releases/latest"),
            "3.2.1",
            true,
        )
        .expect("429 fallback without SHA256SUMS should succeed");

        let candidate = match outcome {
            CheckOutcome::Available(c) => c,
            other => panic!("expected Available, got {other:?}"),
        };
        assert_eq!(candidate.version, "9.9.9");
        assert!(candidate.sha256sums_url.is_none());
        if cfg!(target_os = "windows") {
            head_zip.assert();
            assert_eq!(candidate.strategy, AssetStrategy::WindowsPortableZip);
            assert_eq!(
                candidate
                    .asset
                    .as_ref()
                    .map(|a| a.browser_download_url.as_str()),
                Some(
                    server
                        .url(format!(
                            "/raditted/Polban-Auto-Absen-rs/releases/download/v9.9.9/{WINDOWS_PORTABLE_ZIP}"
                        ))
                        .as_str()
                )
            );
        }

        // Now test when no assets are confirmed via HEAD -> safely returns OpenReleasePage.
        let server2 = MockServer::start();
        server2.mock(|when, then| {
            when.method(GET).path("/api/latest");
            then.status(403);
        });
        server2.mock(|when, then| {
            when.method(GET).path("/web/latest");
            then.status(302).header(
                "Location",
                "/raditted/Polban-Auto-Absen-rs/releases/tag/v9.9.9",
            );
        });
        let outcome2 = check_latest_release_with_fallback(
            &server2.url("/api/latest"),
            Some(&server2.url("/web/latest")),
            "3.2.1",
            true,
        )
        .expect("missing assets must safely fall back to OpenReleasePage");
        let candidate2 = match outcome2 {
            CheckOutcome::Available(c) => c,
            other => panic!("expected Available, got {other:?}"),
        };
        assert_eq!(candidate2.strategy, AssetStrategy::OpenReleasePage);
        assert!(candidate2.asset.is_none());
    }

    #[test]
    fn rate_limit_403_and_fallback_failure_returns_clear_indonesian_error() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET)
                .path("/repos/raditted/Polban-Auto-Absen-rs/releases/latest");
            then.status(403).body("rate limit exceeded");
        });
        server.mock(|when, then| {
            when.method(GET)
                .path("/raditted/Polban-Auto-Absen-rs/releases/latest");
            then.status(502).body("bad gateway");
        });

        let err = check_latest_release(
            &server.url("/repos/raditted/Polban-Auto-Absen-rs/releases/latest"),
            "3.2.1",
            true,
        )
        .expect_err("expected error when both API 403 and web fallback fail");

        assert!(
            err.contains("Batas akses GitHub API tercapai") && err.contains("403"),
            "unexpected error message: {err}"
        );
        assert!(
            err.contains("502"),
            "expected fallback status in error: {err}"
        );
    }
}

//! Non-blocking GitHub Release auto-updater with SHA256 verification and cross-OS self-update.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{BufReader, Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use flate2::read::GzDecoder;
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
const MAX_DOWNLOAD_BYTES: usize = 256 * 1024 * 1024;
const MAX_EXTRACTED_BINARY_BYTES: u64 = 128 * 1024 * 1024;
const MAX_TAR_META_BYTES: u64 = 64 * 1024;

const WINDOWS_PORTABLE_ZIP: &str = "Polban-Presensi-Portable-Windows-x64.zip";
const WINDOWS_INSTALLER_EXE: &str = "Polban-Presensi-Setup-Windows-x64.exe";
const LINUX_FEDORA_RPM: &str = "Polban-Presensi-Fedora-x64.rpm";
const LINUX_PORTABLE_TAR_GZ: &str = "Polban-Presensi-Linux-x64.tar.gz";
const MACOS_APPLE_SILICON_ZIP: &str = "Polban-Presensi-macOS-AppleSilicon.zip";
const MACOS_INTEL_ZIP: &str = "Polban-Presensi-macOS-Intel.zip";
const LINUX_BINARY_NAME: &str = "polban-absen-gui";
const MACOS_BINARY_NAME: &str = "polban-absen-gui";
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
    LinuxFedoraRpm,
    LinuxTarGz,
    MacOsAppZip,
    MacOsBinaryZip,
    OpenReleasePage,
}

/// Host environment capabilities used to pick the safest release update strategy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostUpdateCapabilities {
    pub target_os: String,
    pub target_arch: String,
    pub exe_dir_writable: bool,
    pub linux_rpm_upgradeable: bool,
    pub macos_app_bundle: Option<PathBuf>,
    pub macos_app_parent_writable: bool,
}

impl HostUpdateCapabilities {
    /// Construct capabilities with explicit OS, architecture, and directory writability.
    pub fn new(target_os: &str, target_arch: &str, exe_dir_writable: bool) -> Self {
        Self {
            target_os: target_os.to_string(),
            target_arch: target_arch.to_string(),
            exe_dir_writable,
            linux_rpm_upgradeable: false,
            macos_app_bundle: None,
            macos_app_parent_writable: false,
        }
    }
}

/// Detect `<...>/<Name>.app/Contents/MacOS/<bin>` and return the `.app` bundle directory.
pub fn detect_macos_app_bundle(exe: &Path) -> Option<PathBuf> {
    let bin_name = exe.file_name()?;
    if bin_name.is_empty() {
        return None;
    }
    let macos_dir = exe.parent()?;
    if macos_dir.file_name() != Some(OsStr::new("MacOS")) {
        return None;
    }
    let contents_dir = macos_dir.parent()?;
    if contents_dir.file_name() != Some(OsStr::new("Contents")) {
        return None;
    }
    let app_dir = contents_dir.parent()?;
    let app_name = app_dir.file_name()?.to_str()?;
    if app_name.ends_with(".app") && app_name.len() > ".app".len() {
        Some(app_dir.to_path_buf())
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn find_executable_in_path(bin: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(bin);
        if let Ok(meta) = fs::metadata(&candidate) {
            if meta.is_file() && (meta.permissions().mode() & 0o111) != 0 {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn detect_linux_rpm_upgradeable(exe_path: &Path) -> bool {
    let has_pkexec = find_executable_in_path("pkexec").is_some();
    let rpm_path = find_executable_in_path("rpm");
    let has_pkg_mgr = find_executable_in_path("dnf").is_some() || rpm_path.is_some();
    if !has_pkexec || !has_pkg_mgr {
        return false;
    }
    let Some(rpm_bin) = rpm_path else {
        return false;
    };
    Command::new(rpm_bin)
        .arg("-qf")
        .arg(exe_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Probe current host OS, architecture, writability, RPM ownership, and macOS bundle context.
///
/// Intended to run on a background worker thread because `rpm -qf` and filesystem probes perform I/O.
pub fn detect_host_update_capabilities() -> HostUpdateCapabilities {
    let exe = std::env::current_exe().ok();
    let exe_dir_writable = exe
        .as_deref()
        .and_then(|p| p.parent())
        .map(is_dir_writable)
        .unwrap_or(false);

    #[cfg(target_os = "linux")]
    let linux_rpm_upgradeable = exe.as_deref().is_some_and(detect_linux_rpm_upgradeable);
    #[cfg(not(target_os = "linux"))]
    let linux_rpm_upgradeable = false;

    let macos_app_bundle = if std::env::consts::OS == "macos" {
        exe.as_deref().and_then(detect_macos_app_bundle)
    } else {
        None
    };
    let macos_app_parent_writable = macos_app_bundle
        .as_deref()
        .and_then(|app| app.parent())
        .map(is_dir_writable)
        .unwrap_or(false);

    HostUpdateCapabilities {
        target_os: std::env::consts::OS.to_string(),
        target_arch: std::env::consts::ARCH.to_string(),
        exe_dir_writable,
        linux_rpm_upgradeable,
        macos_app_bundle,
        macos_app_parent_writable,
    }
}

/// Selected release asset and delivery strategy for the host environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedAsset {
    pub strategy: AssetStrategy,
    pub asset: Option<GitHubAsset>,
}

fn macos_asset_name_for_arch(arch: &str) -> Option<&'static str> {
    match arch {
        "aarch64" => Some(MACOS_APPLE_SILICON_ZIP),
        "x86_64" => Some(MACOS_INTEL_ZIP),
        _ => None,
    }
}

/// Select the best release artifact using full host capabilities.
pub fn select_release_asset_for_host(
    assets: &[GitHubAsset],
    caps: &HostUpdateCapabilities,
) -> SelectedAsset {
    match caps.target_os.as_str() {
        "windows" => {
            if caps.exe_dir_writable {
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
        "linux" if caps.target_arch == "x86_64" => {
            if caps.linux_rpm_upgradeable {
                if let Some(rpm_asset) = assets.iter().find(|a| a.name == LINUX_FEDORA_RPM) {
                    return SelectedAsset {
                        strategy: AssetStrategy::LinuxFedoraRpm,
                        asset: Some(rpm_asset.clone()),
                    };
                }
            } else if caps.exe_dir_writable {
                if let Some(tar_asset) = assets.iter().find(|a| a.name == LINUX_PORTABLE_TAR_GZ) {
                    return SelectedAsset {
                        strategy: AssetStrategy::LinuxTarGz,
                        asset: Some(tar_asset.clone()),
                    };
                }
            }
        }
        "macos" => {
            if let Some(zip_name) = macos_asset_name_for_arch(&caps.target_arch) {
                if let Some(zip_asset) = assets.iter().find(|a| a.name == zip_name) {
                    if caps.macos_app_bundle.is_some() {
                        if caps.macos_app_parent_writable {
                            return SelectedAsset {
                                strategy: AssetStrategy::MacOsAppZip,
                                asset: Some(zip_asset.clone()),
                            };
                        }
                    } else if caps.exe_dir_writable {
                        return SelectedAsset {
                            strategy: AssetStrategy::MacOsBinaryZip,
                            asset: Some(zip_asset.clone()),
                        };
                    }
                }
            }
        }
        _ => {}
    }
    SelectedAsset {
        strategy: AssetStrategy::OpenReleasePage,
        asset: None,
    }
}

/// Select the best release artifact for `(target_os, target_arch, exe_dir_writable)`.
pub fn select_release_asset(
    assets: &[GitHubAsset],
    target_os: &str,
    target_arch: &str,
    exe_dir_writable: bool,
) -> SelectedAsset {
    select_release_asset_for_host(
        assets,
        &HostUpdateCapabilities::new(target_os, target_arch, exe_dir_writable),
    )
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
    RunLinuxRpmUpgrade {
        rpm_path: PathBuf,
        current_exe: PathBuf,
    },
    ReplaceMacOsApp {
        current_app: PathBuf,
        staged_app: PathBuf,
        relaunch_exe: PathBuf,
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
    Installing {
        version: String,
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

fn set_file_mode(path: &Path, mode: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|e| format!("Gagal mengatur izin file {}: {e}", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
    Ok(())
}

fn write_atomic_file_with_mode(dest_path: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    if let Some(parent) = dest_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Gagal membuat direktori update: {e}"))?;
        }
    }
    let file_name = dest_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "update.bin".to_string());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_name = format!(".{file_name}.tmp-{}-{nanos}", std::process::id());
    let tmp_path = match dest_path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(tmp_name),
        _ => PathBuf::from(tmp_name),
    };

    let write_res = (|| -> Result<(), String> {
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .map_err(|e| format!("Gagal menulis file update {}: {e}", dest_path.display()))?;
        out.write_all(bytes)
            .map_err(|e| format!("Gagal menulis file update {}: {e}", dest_path.display()))?;
        out.flush()
            .map_err(|e| format!("Gagal menyelesaikan penulisan file: {e}"))?;
        let _ = out.sync_all();
        drop(out);
        set_file_mode(&tmp_path, mode)?;
        fs::rename(&tmp_path, dest_path)
            .map_err(|e| format!("Gagal menyimpan file update {}: {e}", dest_path.display()))?;
        Ok(())
    })();

    if write_res.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    write_res
}

fn validate_archive_rel_path(raw: &str) -> Result<PathBuf, String> {
    if raw.is_empty()
        || raw.contains('\0')
        || raw.contains('\\')
        || raw.starts_with('/')
        || raw.as_bytes().get(1) == Some(&b':')
    {
        return Err(format!("Path arsip tidak aman: {raw}"));
    }

    let mut clean = PathBuf::new();
    for comp in Path::new(raw).components() {
        match comp {
            Component::Normal(seg) => {
                if seg == OsStr::new("..") {
                    return Err(format!("Path arsip tidak aman: {raw}"));
                }
                clean.push(seg);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("Path arsip tidak aman: {raw}"));
            }
        }
    }
    Ok(clean)
}

fn read_bounded_zip_entry<R: Read>(
    entry: &mut R,
    max_bytes: u64,
    label: &str,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut limited = entry.take(max_bytes.saturating_add(1));
    limited
        .read_to_end(&mut out)
        .map_err(|e| format!("Gagal mengekstrak {label} dari ZIP: {e}"))?;
    if (out.len() as u64) > max_bytes {
        return Err(format!("Ukuran {label} di arsip ZIP melebihi batas aman."));
    }
    Ok(out)
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

        let bytes = read_bounded_zip_entry(&mut entry, MAX_EXTRACTED_BINARY_BYTES, "executable")?;
        return write_atomic_file_with_mode(dest_path, &bytes, 0o755);
    }

    Err("Arsip ZIP tidak berisi file executable (.exe).".to_string())
}

fn validate_safe_zip_entry<R: Read>(entry: &zip::read::ZipFile<'_, R>) -> Result<PathBuf, String> {
    let raw_name = entry.name();
    let is_symlink = entry.is_symlink()
        || entry
            .unix_mode()
            .is_some_and(|m| (m & 0o170_000) == 0o120_000);
    if is_symlink {
        return Err(format!(
            "Arsip ZIP mengandung symlink yang tidak diizinkan: {raw_name}"
        ));
    }
    if entry.enclosed_name().is_none() {
        return Err(format!("Path arsip ZIP tidak aman: {raw_name}"));
    }
    validate_archive_rel_path(raw_name)
}

/// Extract a macOS `<Name>.app` bundle from `zip_bytes` directly into `staged_app_dir`.
///
/// Verifies path traversal safety, rejects symlinks, sets executable permissions on
/// `Contents/MacOS/*`, and checks that `Contents/MacOS/polban-absen-gui` exists.
pub fn extract_macos_app_from_zip(
    zip_bytes: &[u8],
    staged_app_dir: &Path,
) -> Result<PathBuf, String> {
    let cursor = Cursor::new(zip_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("Gagal membuka arsip ZIP: {e}"))?;

    if staged_app_dir.exists() {
        let _ = fs::remove_dir_all(staged_app_dir);
    }
    fs::create_dir_all(staged_app_dir)
        .map_err(|e| format!("Gagal membuat direktori staging .app: {e}"))?;

    let mut bundle_root: Option<String> = None;
    let mut total_written: u64 = 0;

    let extract_res = (|| -> Result<PathBuf, String> {
        for idx in 0..archive.len() {
            let mut entry = archive
                .by_index(idx)
                .map_err(|e| format!("Gagal membaca entri ZIP: {e}"))?;
            let clean_rel = validate_safe_zip_entry(&entry)?;
            let mut comps = clean_rel.components();
            let Some(Component::Normal(first)) = comps.next() else {
                continue;
            };
            let first_str = first.to_string_lossy();
            if first_str == "__MACOSX" {
                continue;
            }
            if !first_str.ends_with(".app") || first_str.len() <= ".app".len() {
                continue;
            }

            if let Some(expected_root) = &bundle_root {
                if expected_root != first_str.as_ref() {
                    return Err(format!(
                        "Arsip ZIP berisi lebih dari satu bundle .app ({expected_root} dan {first_str})."
                    ));
                }
            } else {
                bundle_root = Some(first_str.into_owned());
            }

            let inner_rel: PathBuf = comps.collect();
            if inner_rel.as_os_str().is_empty() {
                continue;
            }
            let out_path = staged_app_dir.join(&inner_rel);
            if !out_path.starts_with(staged_app_dir) {
                return Err("Path keluaran berada di luar direktori staging .app.".to_string());
            }

            if entry.is_dir() {
                fs::create_dir_all(&out_path)
                    .map_err(|e| format!("Gagal membuat direktori bundle: {e}"))?;
                continue;
            }

            let file_bytes =
                read_bounded_zip_entry(&mut entry, MAX_EXTRACTED_BINARY_BYTES, "file bundle")?;
            total_written = total_written
                .checked_add(file_bytes.len() as u64)
                .ok_or_else(|| "Ukuran total arsip ZIP melebihi batas aman.".to_string())?;
            if total_written > MAX_DOWNLOAD_BYTES as u64 {
                return Err("Ukuran total konten arsip ZIP melebihi batas aman.".to_string());
            }

            let is_macos_bin = inner_rel.starts_with(Path::new("Contents").join("MacOS"));
            let has_exec_bit = entry.unix_mode().is_some_and(|m| (m & 0o111) != 0);
            let mode = if is_macos_bin || has_exec_bit {
                0o755
            } else {
                0o644
            };
            write_atomic_file_with_mode(&out_path, &file_bytes, mode)?;
        }

        if bundle_root.is_none() {
            return Err("Arsip ZIP tidak berisi bundle .app macOS.".to_string());
        }
        let relaunch_bin = staged_app_dir
            .join("Contents")
            .join("MacOS")
            .join(MACOS_BINARY_NAME);
        let meta = fs::metadata(&relaunch_bin).map_err(|_| {
            format!("Bundle .app tidak berisi executable Contents/MacOS/{MACOS_BINARY_NAME}.")
        })?;
        if !meta.is_file() || meta.len() == 0 {
            return Err(format!(
                "Executable Contents/MacOS/{MACOS_BINARY_NAME} di dalam bundle .app tidak valid."
            ));
        }
        set_file_mode(&relaunch_bin, 0o755)?;
        Ok(relaunch_bin)
    })();

    if extract_res.is_err() {
        let _ = fs::remove_dir_all(staged_app_dir);
    }
    extract_res
}

/// Extract the standalone macOS `polban-absen-gui` binary from `zip_bytes` into `dest_path`.
pub fn extract_macos_binary_from_zip(zip_bytes: &[u8], dest_path: &Path) -> Result<(), String> {
    let cursor = Cursor::new(zip_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("Gagal membuka arsip ZIP: {e}"))?;

    let mut matched_bytes: Option<Vec<u8>> = None;
    for idx in 0..archive.len() {
        let mut entry = archive
            .by_index(idx)
            .map_err(|e| format!("Gagal membaca entri ZIP: {e}"))?;
        let clean_rel = validate_safe_zip_entry(&entry)?;
        if entry.is_dir() {
            continue;
        }
        if clean_rel
            .components()
            .next()
            .is_some_and(|c| c.as_os_str() == "__MACOSX")
        {
            continue;
        }
        let is_exact_bundle_bin =
            clean_rel.ends_with(Path::new("Contents").join("MacOS").join(MACOS_BINARY_NAME));
        let is_basename = clean_rel.file_name() == Some(OsStr::new(MACOS_BINARY_NAME));
        if (is_exact_bundle_bin || is_basename) && (matched_bytes.is_none() || is_exact_bundle_bin)
        {
            matched_bytes = Some(read_bounded_zip_entry(
                &mut entry,
                MAX_EXTRACTED_BINARY_BYTES,
                MACOS_BINARY_NAME,
            )?);
        }
    }

    let bytes = matched_bytes
        .ok_or_else(|| format!("Arsip ZIP macOS tidak berisi executable {MACOS_BINARY_NAME}."))?;
    write_atomic_file_with_mode(dest_path, &bytes, 0o755)
}

#[derive(Default)]
struct PaxAttrs {
    path: Option<String>,
    size: Option<u64>,
}

fn read_tar_block_or_eof(reader: &mut impl Read) -> Result<Option<[u8; 512]>, String> {
    let mut block = [0u8; 512];
    let mut filled = 0;
    while filled < block.len() {
        match reader.read(&mut block[filled..]) {
            Ok(0) => {
                if filled == 0 {
                    return Ok(None);
                }
                return Err("Arsip .tar.gz terpotong atau rusak.".to_string());
            }
            Ok(n) => filled += n,
            Err(e) => return Err(format!("Gagal mendekompresi arsip .tar.gz: {e}")),
        }
    }
    Ok(Some(block))
}

fn read_tar_exact(reader: &mut impl Read, len: usize) -> Result<Vec<u8>, String> {
    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .map_err(|e| format!("Gagal membaca entri arsip .tar.gz: {e}"))?;
    Ok(buf)
}

fn skip_tar_exact(reader: &mut impl Read, len: u64) -> Result<(), String> {
    if len == 0 {
        return Ok(());
    }
    let copied = std::io::copy(&mut reader.take(len), &mut std::io::sink())
        .map_err(|e| format!("Gagal melompati data arsip .tar.gz: {e}"))?;
    if copied != len {
        return Err("Arsip .tar.gz terpotong di tengah entri.".to_string());
    }
    Ok(())
}

fn parse_tar_cstr(field: &[u8]) -> String {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

fn parse_tar_octal(field: &[u8]) -> Result<u64, String> {
    let text = parse_tar_cstr(field);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(trimmed, 8)
        .map_err(|_| format!("Nilai oktal header tar tidak valid: {trimmed}"))
}

fn parse_tar_size(field: &[u8]) -> Result<u64, String> {
    if field.first().is_some_and(|&b| (b & 0x80) != 0) {
        if field[0] == 0xff {
            return Err("Ukuran entri tar negatif tidak didukung.".to_string());
        }
        let mut value: u64 = u64::from(field[0] & 0x7f);
        for &byte in &field[1..] {
            value = value
                .checked_shl(8)
                .and_then(|v| v.checked_add(u64::from(byte)))
                .ok_or_else(|| "Ukuran entri tar melampaui batas.".to_string())?;
        }
        return Ok(value);
    }
    parse_tar_octal(field)
}

fn verify_tar_checksum(block: &[u8; 512]) -> Result<(), String> {
    let stored = parse_tar_octal(&block[148..156])?;
    let mut unsigned_sum: u64 = 0;
    let mut signed_sum: i64 = 0;
    for (idx, &raw) in block.iter().enumerate() {
        let byte = if (148..156).contains(&idx) { b' ' } else { raw };
        unsigned_sum += u64::from(byte);
        signed_sum += i64::from(byte as i8);
    }
    if unsigned_sum == stored || signed_sum == stored as i64 {
        Ok(())
    } else {
        Err("Checksum header arsip tar tidak valid.".to_string())
    }
}

fn parse_pax_records(data: &[u8], attrs: &mut PaxAttrs) -> Result<(), String> {
    let mut pos = 0;
    while pos < data.len() {
        let Some(rel_space) = data[pos..].iter().position(|&b| b == b' ') else {
            return Err("Format header PAX tar tidak valid.".to_string());
        };
        let len_str = std::str::from_utf8(&data[pos..pos + rel_space])
            .map_err(|_| "Panjang record PAX tar tidak valid.".to_string())?;
        let record_len: usize = len_str
            .parse()
            .map_err(|_| "Panjang record PAX tar bukan angka.".to_string())?;
        if record_len <= rel_space + 1
            || pos
                .checked_add(record_len)
                .is_none_or(|end| end > data.len())
        {
            return Err("Batas record PAX tar tidak konsisten.".to_string());
        }
        let body = &data[pos + rel_space + 1..pos + record_len];
        let body = body
            .strip_suffix(b"\n")
            .ok_or_else(|| "Record PAX tar tidak diakhiri newline.".to_string())?;
        if let Some(eq_idx) = body.iter().position(|&b| b == b'=') {
            let key = &body[..eq_idx];
            let val = &body[eq_idx + 1..];
            if key == b"path" {
                let path = String::from_utf8(val.to_vec())
                    .map_err(|_| "Path PAX tar bukan UTF-8 valid.".to_string())?;
                attrs.path = Some(path);
            } else if key == b"size" {
                let size_txt = std::str::from_utf8(val)
                    .map_err(|_| "Ukuran PAX tar bukan UTF-8 valid.".to_string())?;
                let parsed = size_txt
                    .parse::<u64>()
                    .map_err(|_| "Nilai ukuran PAX tar tidak valid.".to_string())?;
                attrs.size = Some(parsed);
            }
        }
        pos += record_len;
    }
    Ok(())
}

/// Extract a named executable (`binary_name`) from a `.tar.gz` byte slice into `dest_path`.
///
/// Supports POSIX ustar headers, PAX (`x`/`g`) extended headers, and GNU LongName (`L`),
/// rejects path traversal (`..`, absolute paths, Windows separators), and sets `0o755` on Unix.
pub fn extract_binary_from_tar_gz(
    tar_gz_bytes: &[u8],
    binary_name: &str,
    dest_path: &Path,
) -> Result<(), String> {
    let mut reader = BufReader::new(GzDecoder::new(Cursor::new(tar_gz_bytes)));
    let mut global_pax = PaxAttrs::default();
    let mut per_file_pax = PaxAttrs::default();
    let mut gnu_longname: Option<String> = None;
    let mut found_bytes: Option<Vec<u8>> = None;

    while let Some(header) = read_tar_block_or_eof(&mut reader)? {
        if header.iter().all(|&b| b == 0) {
            break;
        }
        verify_tar_checksum(&header)?;

        let typeflag = header[156];
        let header_size = parse_tar_size(&header[124..136])?;
        let pad_len = (512 - (header_size % 512)) % 512;

        if matches!(typeflag, b'x' | b'g' | b'L' | b'K') {
            if header_size > MAX_TAR_META_BYTES {
                return Err("Metadata header arsip tar terlalu besar.".to_string());
            }
            let meta_bytes = read_tar_exact(&mut reader, header_size as usize)?;
            skip_tar_exact(&mut reader, pad_len)?;
            match typeflag {
                b'x' => parse_pax_records(&meta_bytes, &mut per_file_pax)?,
                b'g' => parse_pax_records(&meta_bytes, &mut global_pax)?,
                b'L' => {
                    let trimmed = match meta_bytes.iter().position(|&b| b == 0) {
                        Some(pos) => &meta_bytes[..pos],
                        None => &meta_bytes[..],
                    };
                    let long_str = String::from_utf8(trimmed.to_vec())
                        .map_err(|_| "Nama file panjang tar bukan UTF-8 valid.".to_string())?;
                    gnu_longname = Some(long_str);
                }
                b'K' => {}
                _ => {}
            }
            continue;
        }

        let pax = std::mem::take(&mut per_file_pax);
        let longname = gnu_longname.take();

        let name = parse_tar_cstr(&header[0..100]);
        let prefix = parse_tar_cstr(&header[345..500]);
        let ustar_path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let raw_path = pax
            .path
            .or(longname)
            .or_else(|| global_pax.path.clone())
            .unwrap_or(ustar_path);
        let clean_rel = validate_archive_rel_path(&raw_path)?;

        let entry_size = pax.size.or(global_pax.size).unwrap_or(header_size);
        let entry_pad = (512 - (entry_size % 512)) % 512;
        let is_regular_file = matches!(typeflag, b'0' | b'\0');
        let matches_binary =
            is_regular_file && clean_rel.file_name() == Some(OsStr::new(binary_name));

        if matches_binary {
            if entry_size > MAX_EXTRACTED_BINARY_BYTES {
                return Err(format!(
                    "Ukuran biner {binary_name} di arsip .tar.gz melebihi batas aman."
                ));
            }
            let bytes = read_tar_exact(&mut reader, entry_size as usize)?;
            skip_tar_exact(&mut reader, entry_pad)?;
            if found_bytes.is_none() {
                found_bytes = Some(bytes);
            }
        } else {
            let total_skip = entry_size
                .checked_add(entry_pad)
                .ok_or_else(|| "Ukuran entri arsip tar melampaui batas.".to_string())?;
            skip_tar_exact(&mut reader, total_skip)?;
        }
    }

    let bytes = found_bytes
        .ok_or_else(|| format!("Arsip .tar.gz tidak berisi file biner {binary_name}."))?;
    write_atomic_file_with_mode(dest_path, &bytes, 0o755)
}

/// Compute `<exe_dir>/.<exe_name>.new` for staging a self-update executable.
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

/// Compute `<parent>/.<AppName>.app.new` for staging a macOS `.app` bundle replacement.
pub fn staged_new_app_path_for(app_bundle: &Path) -> PathBuf {
    let dir_name = app_bundle
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Polban Presensi.app".to_string());
    let staged_name = format!(".{dir_name}.new");
    match app_bundle.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(staged_name),
        _ => PathBuf::from(staged_name),
    }
}

/// Compute `<path>.old` backup path for the currently running executable or `.app` bundle.
pub fn old_backup_path_for(exe_path: &Path) -> PathBuf {
    let mut os = exe_path.as_os_str().to_os_string();
    os.push(".old");
    PathBuf::from(os)
}

fn remove_backup_path(old_path: &Path) {
    if let Ok(meta) = fs::symlink_metadata(old_path) {
        if meta.is_dir() {
            let _ = fs::remove_dir_all(old_path);
        } else {
            let _ = fs::remove_file(old_path);
        }
    }
}

/// Remove any leftover `<exe_path>.old` or `<AppName>.app.old` directory from a previous self-update.
pub fn cleanup_old_executable_for(exe_path: &Path) {
    remove_backup_path(&old_backup_path_for(exe_path));
    if let Some(app_bundle) = detect_macos_app_bundle(exe_path) {
        remove_backup_path(&old_backup_path_for(&app_bundle));
    }
}

/// Best-effort startup cleanup for the running executable's `.old` backup file or `.app.old` bundle.
pub fn cleanup_old_executables() {
    if let Ok(exe_path) = std::env::current_exe() {
        cleanup_old_executable_for(&exe_path);
    }
}

/// Probe whether `dir` allows creating new files (distinguishes Program Files / `/usr/bin` from user-writable dirs).
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

/// Rename `current_app -> <current_app>.old` and `staged_app -> current_app`, rolling back on error.
pub fn stage_replace_app_bundle(current_app: &Path, staged_app: &Path) -> Result<PathBuf, String> {
    if !staged_app.is_dir() {
        return Err(format!(
            "Direktori bundle pembaruan tidak ditemukan: {}",
            staged_app.display()
        ));
    }
    let old_app = old_backup_path_for(current_app);
    if let Ok(meta) = fs::symlink_metadata(&old_app) {
        if meta.is_dir() {
            fs::remove_dir_all(&old_app).map_err(|e| {
                format!(
                    "Gagal menghapus cadangan bundle lama {}: {e}",
                    old_app.display()
                )
            })?;
        } else {
            fs::remove_file(&old_app).map_err(|e| {
                format!(
                    "Gagal menghapus cadangan bundle lama {}: {e}",
                    old_app.display()
                )
            })?;
        }
    }
    fs::rename(current_app, &old_app).map_err(|e| {
        format!(
            "Gagal memindahkan bundle aplikasi aktif ke {}: {e}",
            old_app.display()
        )
    })?;
    if let Err(err) = fs::rename(staged_app, current_app) {
        let _ = fs::rename(&old_app, current_app);
        return Err(format!(
            "Gagal mengganti bundle aplikasi dengan versi baru: {err}"
        ));
    }
    Ok(old_app)
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
    #[cfg(unix)]
    {
        use std::process::Stdio;
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Gagal menjalankan {}: {e}", program.display()))
}

#[cfg(target_os = "macos")]
fn launch_macos_app_bundle(current_app: &Path, relaunch_exe: &Path) -> Result<(), String> {
    use std::process::Stdio;
    if Command::new("open")
        .arg("-n")
        .arg(current_app)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
    {
        return Ok(());
    }
    spawn_detached(relaunch_exe)
}

#[cfg(not(target_os = "macos"))]
fn launch_macos_app_bundle(_current_app: &Path, relaunch_exe: &Path) -> Result<(), String> {
    spawn_detached(relaunch_exe)
}

/// Release the single-instance lock strictly before launching `program` after an install.
pub fn relaunch_after_install(program: &Path, release_lock: &dyn Fn()) -> Result<(), String> {
    release_lock();
    spawn_detached(program)
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
        RestartPlan::ReplaceMacOsApp {
            current_app,
            staged_app,
            relaunch_exe,
        } => {
            stage_replace_app_bundle(current_app, staged_app)?;
            launch_macos_app_bundle(current_app, relaunch_exe)?;
        }
        RestartPlan::RunLinuxRpmUpgrade { .. } => {
            return Err(
                "Pembaruan paket RPM harus dijalankan melalui instalasi latar belakang."
                    .to_string(),
            );
        }
    }
    Ok(())
}

#[cfg(any(target_os = "linux", test))]
fn truncate_utf8(input: &str, max_bytes: usize) -> &str {
    if input.len() <= max_bytes {
        return input;
    }
    let mut end = max_bytes;
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    &input[..end]
}

#[cfg(any(target_os = "linux", test))]
fn format_rpm_upgrade_error(exit_code: Option<i32>, stderr: &[u8]) -> String {
    if matches!(exit_code, Some(126 | 127)) {
        return "Autentikasi dibatalkan. Klik untuk mencoba lagi.".to_string();
    }
    let raw = String::from_utf8_lossy(stderr);
    let compact = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" | ");
    if compact.is_empty() {
        match exit_code {
            Some(code) => format!("Gagal memasang paket RPM (kode keluar {code})."),
            None => "Proses instalasi paket RPM dihentikan sebelum selesai.".to_string(),
        }
    } else {
        let clipped = truncate_utf8(&compact, 220);
        format!("Gagal memasang paket RPM: {clipped}")
    }
}

#[cfg(target_os = "linux")]
fn run_linux_rpm_upgrade(rpm_path: &Path) -> Result<(), String> {
    if !rpm_path.is_file() {
        return Err(format!(
            "File paket RPM tidak ditemukan: {}",
            rpm_path.display()
        ));
    }
    let pkexec = find_executable_in_path("pkexec")
        .ok_or_else(|| "Perintah pkexec tidak tersedia untuk memasang paket RPM.".to_string())?;

    let mut cmd = Command::new(pkexec);
    if let Some(dnf) = find_executable_in_path("dnf") {
        cmd.arg(dnf).arg("upgrade").arg("-y").arg(rpm_path);
    } else if let Some(rpm) = find_executable_in_path("rpm") {
        cmd.arg(rpm).arg("-U").arg(rpm_path);
    } else {
        return Err("Manajer paket dnf maupun rpm tidak ditemukan di sistem.".to_string());
    }

    let output = cmd
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("Gagal menjalankan pkexec untuk pembaruan RPM: {e}"))?;

    if output.status.success() {
        let _ = fs::remove_file(rpm_path);
        return Ok(());
    }

    Err(format_rpm_upgrade_error(
        output.status.code(),
        &output.stderr,
    ))
}

#[cfg(not(target_os = "linux"))]
fn run_linux_rpm_upgrade(_rpm_path: &Path) -> Result<(), String> {
    Err("Instalasi paket RPM hanya didukung di Linux.".to_string())
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

#[cfg(all(unix, not(target_os = "macos")))]
fn launch_unix_browser_command(program: &str, args: &[&str]) -> Result<(), String> {
    use std::process::Stdio;
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Gagal menjalankan {program}: {e}"))?;

    let deadline = Instant::now() + Duration::from_millis(350);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(());
                }
                return Err(format!(
                    "{program} keluar dengan kode {}",
                    status
                        .code()
                        .map_or_else(|| "?".to_string(), |c| c.to_string())
                ));
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = std::thread::Builder::new()
                        .name("polban-browser-reap".to_string())
                        .spawn(move || {
                            let _ = child.wait();
                        });
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => return Err(format!("Gagal memeriksa status {program}: {e}")),
        }
    }
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
        match launch_unix_browser_command("xdg-open", &[url]) {
            Ok(()) => Ok(()),
            Err(xdg_err) => launch_unix_browser_command("gio", &["open", url])
                .map_err(|gio_err| format!("Gagal membuka browser ({xdg_err}; {gio_err}).")),
        }
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

fn web_probe_asset_names(caps: &HostUpdateCapabilities) -> Vec<&'static str> {
    let mut probe_names = Vec::new();
    match caps.target_os.as_str() {
        "windows" => {
            if caps.exe_dir_writable {
                probe_names.push(WINDOWS_PORTABLE_ZIP);
            }
            probe_names.push(WINDOWS_INSTALLER_EXE);
        }
        "linux" if caps.target_arch == "x86_64" => {
            if caps.linux_rpm_upgradeable {
                probe_names.push(LINUX_FEDORA_RPM);
            } else if caps.exe_dir_writable {
                probe_names.push(LINUX_PORTABLE_TAR_GZ);
            }
        }
        "macos" => {
            let can_update = if caps.macos_app_bundle.is_some() {
                caps.macos_app_parent_writable
            } else {
                caps.exe_dir_writable
            };
            if can_update {
                if let Some(zip_name) = macos_asset_name_for_arch(&caps.target_arch) {
                    probe_names.push(zip_name);
                }
            }
        }
        _ => {}
    }
    probe_names
}

fn discover_assets_from_web_release(
    client: &reqwest::blocking::Client,
    download_base: &str,
    caps: &HostUpdateCapabilities,
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

    // Fallback when SHA256SUMS.txt is absent: probe platform release assets via HTTP HEAD.
    let mut probed = Vec::new();
    for name in web_probe_asset_names(caps) {
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
    caps: &HostUpdateCapabilities,
) -> Result<CheckOutcome, String> {
    let (tag_name, release_url, download_base) =
        resolve_latest_tag_via_web_redirect(web_latest_url)?;
    if !is_newer_version(&tag_name, current_version) {
        return Ok(CheckOutcome::UpToDate);
    }

    let version = normalize_version_str(&tag_name);
    let (assets, sha256sums_url) = discover_assets_from_web_release(client, &download_base, caps);
    let selected = select_release_asset_for_host(&assets, caps);

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
    let caps = HostUpdateCapabilities::new(
        std::env::consts::OS,
        std::env::consts::ARCH,
        exe_dir_writable,
    );
    check_latest_release_with_capabilities(api_url, web_fallback_url, current_version, &caps)
}

/// Query GitHub Releases JSON at `api_url` using explicit `HostUpdateCapabilities`
/// and automatic rate-limit web fallback on HTTP 403/429.
pub fn check_latest_release_with_capabilities(
    api_url: &str,
    web_fallback_url: Option<&str>,
    current_version: &str,
    caps: &HostUpdateCapabilities,
) -> Result<CheckOutcome, String> {
    let client = build_http_client(CHECK_TIMEOUT)?;

    if is_web_releases_latest_url(api_url) {
        return check_latest_release_via_web(&client, api_url, current_version, caps);
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
            return check_latest_release_via_web(&client, &web_url, current_version, caps)
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
    let selected = select_release_asset_for_host(&release.assets, caps);
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
    if matches!(candidate.strategy, AssetStrategy::OpenReleasePage) {
        return Err("Platform ini memperbarui lewat halaman rilis.".to_string());
    }

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
    if total_len.is_some_and(|n| n > MAX_DOWNLOAD_BYTES as u64) {
        return Err("Ukuran file pembaruan melebihi batas aman.".to_string());
    }

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
        if bytes.len().saturating_add(n) > MAX_DOWNLOAD_BYTES {
            return Err("Ukuran unduhan pembaruan melebihi batas aman.".to_string());
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
            let installer_path = installer_dir.join(&asset.name);
            write_atomic_file_with_mode(&installer_path, &bytes, 0o755)?;
            Ok(RestartPlan::RunInstaller { installer_path })
        }
        AssetStrategy::LinuxFedoraRpm => {
            let rpm_path = installer_dir.join(&asset.name);
            write_atomic_file_with_mode(&rpm_path, &bytes, 0o644)?;
            Ok(RestartPlan::RunLinuxRpmUpgrade {
                rpm_path,
                current_exe: current_exe.to_path_buf(),
            })
        }
        AssetStrategy::LinuxTarGz => {
            let staged_new = staged_new_path_for(current_exe);
            extract_binary_from_tar_gz(&bytes, LINUX_BINARY_NAME, &staged_new)?;
            Ok(RestartPlan::ReplaceExecutable {
                current_exe: current_exe.to_path_buf(),
                staged_new,
            })
        }
        AssetStrategy::MacOsAppZip => {
            let current_app = detect_macos_app_bundle(current_exe)
                .ok_or_else(|| "Bundle aplikasi .app macOS aktif tidak ditemukan.".to_string())?;
            let staged_app = staged_new_app_path_for(&current_app);
            extract_macos_app_from_zip(&bytes, &staged_app)?;
            let relaunch_exe = current_app
                .join("Contents")
                .join("MacOS")
                .join(MACOS_BINARY_NAME);
            Ok(RestartPlan::ReplaceMacOsApp {
                current_app,
                staged_app,
                relaunch_exe,
            })
        }
        AssetStrategy::MacOsBinaryZip => {
            let staged_new = staged_new_path_for(current_exe);
            extract_macos_binary_from_zip(&bytes, &staged_new)?;
            Ok(RestartPlan::ReplaceExecutable {
                current_exe: current_exe.to_path_buf(),
                staged_new,
            })
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
    install_rx: Option<mpsc::Receiver<Result<PathBuf, String>>>,
    open_rx: Option<mpsc::Receiver<Result<String, String>>>,
    ready_relaunch: Option<PathBuf>,
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
            install_rx: None,
            open_rx: None,
            ready_relaunch: None,
        }
    }

    pub fn current_version(&self) -> &'static str {
        CURRENT_VERSION
    }

    /// Consume the executable path ready to relaunch after a background install succeeds.
    pub fn take_ready_relaunch(&mut self) -> Option<PathBuf> {
        self.ready_relaunch.take()
    }

    /// Trigger a silent one-shot background check after startup (no-op if already busy).
    pub fn start_silent_check(&mut self, ctx: &egui::Context) {
        if self.check_rx.is_some()
            || matches!(
                self.status,
                UpdateStatus::Downloading { .. }
                    | UpdateStatus::ReadyToRestart { .. }
                    | UpdateStatus::Installing { .. }
            )
        {
            return;
        }
        self.status = UpdateStatus::Checking;
        self.spawn_check(ctx, false);
    }

    /// Trigger an explicit check from Settings with inline status feedback.
    pub fn start_manual_check(&mut self, ctx: &egui::Context) {
        if self.check_rx.is_some()
            || matches!(
                self.status,
                UpdateStatus::Downloading { .. } | UpdateStatus::Installing { .. }
            )
        {
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

        if std::thread::Builder::new()
            .name("polban-update-check".to_string())
            .spawn(move || {
                let caps = detect_host_update_capabilities();
                let result =
                    check_latest_release_with_capabilities(&api_url, None, CURRENT_VERSION, &caps);
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

    /// Start downloading (or opening the release page in a background thread) for the available candidate.
    pub fn start_download(&mut self, ctx: &egui::Context) {
        if self.download_rx.is_some() || self.install_rx.is_some() || self.open_rx.is_some() {
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
            let release_url = if candidate.release_url.trim().is_empty() {
                DEFAULT_RELEASE_WEB_URL.to_string()
            } else {
                candidate.release_url.clone()
            };
            self.last_candidate = Some(candidate.clone());
            self.manual_note = Some(("Membuka halaman rilis di browser...".to_string(), false));
            let (tx, rx) = mpsc::channel();
            self.open_rx = Some(rx);
            let wake_ctx = ctx.clone();

            if std::thread::Builder::new()
                .name("polban-open-release".to_string())
                .spawn(move || {
                    let res = open_url_in_browser(&release_url).map(|()| release_url);
                    let _ = tx.send(res);
                    wake_ctx.request_repaint();
                })
                .is_err()
            {
                self.open_rx = None;
                let msg = "Gagal menjalankan thread pembuka browser.".to_string();
                self.manual_note = Some((msg.clone(), true));
                self.status = UpdateStatus::Failed {
                    message: msg,
                    retry_candidate: Some(candidate),
                };
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

    /// Start a non-blocking privileged Linux RPM upgrade via `pkexec` (`dnf upgrade -y` or `rpm -U`).
    pub fn start_rpm_install(
        &mut self,
        ctx: &egui::Context,
        version: String,
        rpm_path: PathBuf,
        current_exe: PathBuf,
    ) {
        if self.install_rx.is_some() {
            return;
        }
        self.ready_relaunch = None;
        self.manual_note = Some((
            format!("Menunggu otorisasi sistem untuk memasang v{version}..."),
            false,
        ));
        self.status = UpdateStatus::Installing { version };

        let (tx, rx) = mpsc::channel();
        self.install_rx = Some(rx);
        let wake_ctx = ctx.clone();

        if std::thread::Builder::new()
            .name("polban-update-install".to_string())
            .spawn(move || {
                let res = run_linux_rpm_upgrade(&rpm_path).map(|()| current_exe);
                let _ = tx.send(res);
                wake_ctx.request_repaint();
            })
            .is_err()
        {
            self.install_rx = None;
            let msg = "Gagal menjalankan proses instalasi RPM.".to_string();
            self.manual_note = Some((msg.clone(), true));
            self.status = UpdateStatus::Failed {
                message: msg,
                retry_candidate: self.last_candidate.clone(),
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
                                    | UpdateStatus::Installing { .. }
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
                                    | UpdateStatus::Installing { .. }
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
                        let msg = "Proses pengunduhan berhenti sebelum selesai.".to_string();
                        self.manual_note = Some((msg.clone(), true));
                        self.status = UpdateStatus::Failed {
                            message: msg,
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

        if let Some(rx) = &self.install_rx {
            match rx.try_recv() {
                Ok(Ok(relaunch_exe)) => {
                    self.install_rx = None;
                    self.ready_relaunch = Some(relaunch_exe);
                }
                Ok(Err(message)) => {
                    self.install_rx = None;
                    self.manual_note = Some((message.clone(), true));
                    self.status = UpdateStatus::Failed {
                        message,
                        retry_candidate: self.last_candidate.clone(),
                    };
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.install_rx = None;
                    let message =
                        "Proses instalasi pembaruan terhenti sebelum selesai.".to_string();
                    self.manual_note = Some((message.clone(), true));
                    self.status = UpdateStatus::Failed {
                        message,
                        retry_candidate: self.last_candidate.clone(),
                    };
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }

        if let Some(rx) = &self.open_rx {
            match rx.try_recv() {
                Ok(Ok(opened_url)) => {
                    self.open_rx = None;
                    self.manual_note = Some((
                        format!("Halaman rilis dibuka di browser ({opened_url})."),
                        false,
                    ));
                    if matches!(self.status, UpdateStatus::Failed { .. }) {
                        if let Some(candidate) = self.last_candidate.clone() {
                            self.status = UpdateStatus::Available(candidate);
                        }
                    }
                }
                Ok(Err(message)) => {
                    self.open_rx = None;
                    self.manual_note = Some((message.clone(), true));
                    self.status = UpdateStatus::Failed {
                        message,
                        retry_candidate: self.last_candidate.clone(),
                    };
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.open_rx = None;
                    let message = "Gagal membuka halaman rilis di browser.".to_string();
                    self.manual_note = Some((message.clone(), true));
                    self.status = UpdateStatus::Failed {
                        message,
                        retry_candidate: self.last_candidate.clone(),
                    };
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
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

        let linux_portable = select_release_asset(&assets, "linux", "x86_64", true);
        assert_eq!(linux_portable.strategy, AssetStrategy::LinuxTarGz);
        assert_eq!(
            linux_portable.asset.as_ref().map(|a| a.name.as_str()),
            Some(LINUX_PORTABLE_TAR_GZ)
        );

        let unsupported_arch = select_release_asset(&assets, "linux", "aarch64", true);
        assert_eq!(unsupported_arch.strategy, AssetStrategy::OpenReleasePage);
        assert!(unsupported_arch.asset.is_none());
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

    fn build_tar_header(name: &str, typeflag: u8, size: u64) -> [u8; 512] {
        let mut hdr = [0u8; 512];
        let name_bytes = name.as_bytes();
        let copy_len = name_bytes.len().min(100);
        hdr[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
        hdr[100..108].copy_from_slice(b"0000755\0");
        hdr[108..116].copy_from_slice(b"0001000\0");
        hdr[116..124].copy_from_slice(b"0001000\0");
        let size_str = format!("{size:011o}\0");
        hdr[124..136].copy_from_slice(size_str.as_bytes());
        hdr[136..148].copy_from_slice(b"14000000000\0");
        hdr[148..156].fill(b' ');
        hdr[156] = typeflag;
        hdr[257..263].copy_from_slice(b"ustar\0");
        hdr[263..265].copy_from_slice(b"00");

        let sum: u32 = hdr.iter().map(|&b| u32::from(b)).sum();
        let sum_str = format!("{sum:06o}\0 ");
        hdr[148..156].copy_from_slice(sum_str.as_bytes());
        hdr
    }

    fn append_tar_entry(tar: &mut Vec<u8>, name: &str, typeflag: u8, data: &[u8]) {
        let hdr = build_tar_header(name, typeflag, data.len() as u64);
        tar.extend_from_slice(&hdr);
        tar.extend_from_slice(data);
        let rem = data.len() % 512;
        if rem != 0 {
            tar.resize(tar.len() + (512 - rem), 0);
        }
    }

    fn gzip_bytes(raw: &[u8]) -> Vec<u8> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(raw).expect("gzip write");
        encoder.finish().expect("gzip finish")
    }

    fn build_tar_gz(entries: &[(&str, u8, &[u8])]) -> Vec<u8> {
        let mut tar = Vec::new();
        for &(name, typeflag, data) in entries {
            append_tar_entry(&mut tar, name, typeflag, data);
        }
        tar.resize(tar.len() + 1024, 0);
        gzip_bytes(&tar)
    }

    fn make_pax_record(key: &str, value: &str) -> Vec<u8> {
        let body = format!("{key}={value}\n");
        let mut total = body.len() + 2;
        loop {
            let line = format!("{total} {body}");
            if line.len() == total {
                return line.into_bytes();
            }
            total = line.len();
        }
    }

    fn build_macos_app_zip(
        app_name: &str,
        bin_payload: &[u8],
        extra_entries: &[(&str, &[u8], Option<u32>)],
    ) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let exec_opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o755);
            let file_opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o644);

            writer
                .add_directory(format!("{app_name}/Contents/MacOS/"), exec_opts)
                .expect("add macos dir");
            writer
                .start_file(format!("{app_name}/Contents/Info.plist"), file_opts)
                .expect("start plist");
            writer
                .write_all(b"<plist>Polban</plist>")
                .expect("write plist");
            writer
                .start_file(
                    format!("{app_name}/Contents/MacOS/{MACOS_BINARY_NAME}"),
                    exec_opts,
                )
                .expect("start bin");
            writer.write_all(bin_payload).expect("write bin");

            for &(name, data, mode) in extra_entries {
                if mode.is_some_and(|m| (m & 0o170_000) == 0o120_000) {
                    let target = std::str::from_utf8(data).expect("utf8 symlink target");
                    writer
                        .add_symlink(name, target, file_opts)
                        .expect("add symlink");
                    continue;
                }
                let mut opts = file_opts;
                if let Some(m) = mode {
                    opts = opts.unix_permissions(m);
                }
                writer.start_file(name, opts).expect("start extra");
                writer.write_all(data).expect("write extra");
            }
            writer.finish().expect("finish zip");
        }
        cursor.into_inner()
    }

    #[test]
    fn detect_macos_app_bundle_identifies_bundle_root_purely() {
        assert_eq!(
            detect_macos_app_bundle(Path::new(
                "/Applications/Polban Presensi.app/Contents/MacOS/polban-absen-gui"
            )),
            Some(PathBuf::from("/Applications/Polban Presensi.app"))
        );
        assert_eq!(
            detect_macos_app_bundle(Path::new(
                "/Users/dev/Applications/Custom.app/Contents/MacOS/polban-absen-gui"
            )),
            Some(PathBuf::from("/Users/dev/Applications/Custom.app"))
        );
        assert_eq!(
            detect_macos_app_bundle(Path::new("/usr/local/bin/polban-absen-gui")),
            None
        );
        assert_eq!(
            detect_macos_app_bundle(Path::new(
                "/Applications/.app/Contents/MacOS/polban-absen-gui"
            )),
            None
        );
        assert_eq!(
            detect_macos_app_bundle(Path::new(
                "/Applications/Polban Presensi.app/Contents/MacOS"
            )),
            None
        );
    }

    #[test]
    fn host_capabilities_asset_selection_matrix_across_windows_linux_macos() {
        let all_assets = vec![
            GitHubAsset {
                name: WINDOWS_PORTABLE_ZIP.to_string(),
                browser_download_url: "https://example.test/win-zip".to_string(),
                size: 10,
            },
            GitHubAsset {
                name: WINDOWS_INSTALLER_EXE.to_string(),
                browser_download_url: "https://example.test/win-setup".to_string(),
                size: 20,
            },
            GitHubAsset {
                name: LINUX_FEDORA_RPM.to_string(),
                browser_download_url: "https://example.test/linux-rpm".to_string(),
                size: 30,
            },
            GitHubAsset {
                name: LINUX_PORTABLE_TAR_GZ.to_string(),
                browser_download_url: "https://example.test/linux-tgz".to_string(),
                size: 40,
            },
            GitHubAsset {
                name: MACOS_APPLE_SILICON_ZIP.to_string(),
                browser_download_url: "https://example.test/mac-arm64".to_string(),
                size: 50,
            },
            GitHubAsset {
                name: MACOS_INTEL_ZIP.to_string(),
                browser_download_url: "https://example.test/mac-x64".to_string(),
                size: 60,
            },
        ];

        // Windows writable -> Portable ZIP; readonly -> Installer EXE
        assert_eq!(
            select_release_asset_for_host(
                &all_assets,
                &HostUpdateCapabilities::new("windows", "x86_64", true)
            )
            .strategy,
            AssetStrategy::WindowsPortableZip
        );
        assert_eq!(
            select_release_asset_for_host(
                &all_assets,
                &HostUpdateCapabilities::new("windows", "x86_64", false)
            )
            .strategy,
            AssetStrategy::WindowsInstallerExe
        );

        // Linux x86_64: RPM-managed prefers RPM even if dir happens to be writable
        let rpm_caps = HostUpdateCapabilities {
            target_os: "linux".to_string(),
            target_arch: "x86_64".to_string(),
            exe_dir_writable: true,
            linux_rpm_upgradeable: true,
            macos_app_bundle: None,
            macos_app_parent_writable: false,
        };
        let rpm_sel = select_release_asset_for_host(&all_assets, &rpm_caps);
        assert_eq!(rpm_sel.strategy, AssetStrategy::LinuxFedoraRpm);
        assert_eq!(
            rpm_sel.asset.as_ref().map(|a| a.name.as_str()),
            Some(LINUX_FEDORA_RPM)
        );

        // Linux RPM-managed with missing RPM asset must NOT fall back to tar.gz
        let no_rpm_assets: Vec<_> = all_assets
            .iter()
            .filter(|a| a.name != LINUX_FEDORA_RPM)
            .cloned()
            .collect();
        assert_eq!(
            select_release_asset_for_host(&no_rpm_assets, &rpm_caps).strategy,
            AssetStrategy::OpenReleasePage
        );

        // Linux x86_64 portable writable -> LinuxTarGz; unwritable -> OpenReleasePage; aarch64 -> OpenReleasePage
        assert_eq!(
            select_release_asset_for_host(
                &all_assets,
                &HostUpdateCapabilities::new("linux", "x86_64", true)
            )
            .strategy,
            AssetStrategy::LinuxTarGz
        );
        assert_eq!(
            select_release_asset_for_host(
                &all_assets,
                &HostUpdateCapabilities::new("linux", "x86_64", false)
            )
            .strategy,
            AssetStrategy::OpenReleasePage
        );
        assert_eq!(
            select_release_asset_for_host(
                &all_assets,
                &HostUpdateCapabilities::new("linux", "aarch64", true)
            )
            .strategy,
            AssetStrategy::OpenReleasePage
        );

        // macOS .app writable (aarch64 & x86_64) -> MacOsAppZip with matching arch asset
        let mac_arm_app = HostUpdateCapabilities {
            target_os: "macos".to_string(),
            target_arch: "aarch64".to_string(),
            exe_dir_writable: false,
            linux_rpm_upgradeable: false,
            macos_app_bundle: Some(PathBuf::from("/Applications/Polban Presensi.app")),
            macos_app_parent_writable: true,
        };
        let mac_arm_sel = select_release_asset_for_host(&all_assets, &mac_arm_app);
        assert_eq!(mac_arm_sel.strategy, AssetStrategy::MacOsAppZip);
        assert_eq!(
            mac_arm_sel.asset.as_ref().map(|a| a.name.as_str()),
            Some(MACOS_APPLE_SILICON_ZIP)
        );

        let mac_x64_app = HostUpdateCapabilities {
            target_arch: "x86_64".to_string(),
            ..mac_arm_app.clone()
        };
        let mac_x64_sel = select_release_asset_for_host(&all_assets, &mac_x64_app);
        assert_eq!(mac_x64_sel.strategy, AssetStrategy::MacOsAppZip);
        assert_eq!(
            mac_x64_sel.asset.as_ref().map(|a| a.name.as_str()),
            Some(MACOS_INTEL_ZIP)
        );

        // macOS .app unwritable parent -> OpenReleasePage (not MacOsBinaryZip)
        let mac_ro_app = HostUpdateCapabilities {
            exe_dir_writable: true,
            macos_app_parent_writable: false,
            ..mac_arm_app
        };
        assert_eq!(
            select_release_asset_for_host(&all_assets, &mac_ro_app).strategy,
            AssetStrategy::OpenReleasePage
        );

        // macOS standalone binary writable -> MacOsBinaryZip; unwritable -> OpenReleasePage
        let mac_standalone_ok = HostUpdateCapabilities::new("macos", "aarch64", true);
        let mac_standalone_sel = select_release_asset_for_host(&all_assets, &mac_standalone_ok);
        assert_eq!(mac_standalone_sel.strategy, AssetStrategy::MacOsBinaryZip);
        assert_eq!(
            mac_standalone_sel.asset.as_ref().map(|a| a.name.as_str()),
            Some(MACOS_APPLE_SILICON_ZIP)
        );
        assert_eq!(
            select_release_asset_for_host(
                &all_assets,
                &HostUpdateCapabilities::new("macos", "x86_64", false)
            )
            .strategy,
            AssetStrategy::OpenReleasePage
        );
    }

    #[test]
    fn extract_binary_from_tar_gz_supports_ustar_pax_longname_and_rejects_traversal() {
        let dir = make_temp_dir("tar-gz");
        let dest = dir.join(".polban-absen-gui.new");

        // 1. Standard ustar archive with directory, extra files, and regular binary.
        let payload = b"\x7fELF-polban-linux-v3.4.0";
        let tar_gz = build_tar_gz(&[
            ("Polban-Presensi/", b'5', b""),
            (
                "Polban-Presensi/polban-presensi.desktop",
                b'0',
                b"[Desktop Entry]",
            ),
            ("Polban-Presensi/polban-absen-gui", b'0', payload),
        ]);
        extract_binary_from_tar_gz(&tar_gz, LINUX_BINARY_NAME, &dest).expect("extract tar.gz");
        assert_eq!(fs::read(&dest).unwrap(), payload);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&dest).unwrap().permissions().mode();
            assert_ne!(mode & 0o111, 0, "extracted binary must be executable");
        }
        let _ = fs::remove_file(&dest);

        // 2. PAX 'x' extended header + GNU LongName 'L' + '\0' old-style regular file typeflag.
        let pax_payload = make_pax_record("path", "sub/dir/polban-absen-gui");
        let pax_tar = build_tar_gz(&[
            ("PaxHeaders/polban", b'x', &pax_payload),
            ("ignored-short-name", b'\0', b"pax-binary-bytes"),
        ]);
        extract_binary_from_tar_gz(&pax_tar, LINUX_BINARY_NAME, &dest).expect("extract PAX tar.gz");
        assert_eq!(fs::read(&dest).unwrap(), b"pax-binary-bytes");
        let _ = fs::remove_file(&dest);

        let longname_tar = build_tar_gz(&[
            (
                "././@LongLink",
                b'L',
                b"very/deep/folder/polban-absen-gui\0",
            ),
            ("truncated", b'0', b"gnu-longname-bytes"),
        ]);
        extract_binary_from_tar_gz(&longname_tar, LINUX_BINARY_NAME, &dest)
            .expect("extract GNU LongName tar.gz");
        assert_eq!(fs::read(&dest).unwrap(), b"gnu-longname-bytes");
        let _ = fs::remove_file(&dest);

        // 3. Reject path traversal & absolute paths (both ustar and PAX).
        for unsafe_path in [
            "../polban-absen-gui",
            "pkg/../../polban-absen-gui",
            "/usr/bin/polban-absen-gui",
        ] {
            let bad_tar = build_tar_gz(&[(unsafe_path, b'0', b"evil")]);
            let err = extract_binary_from_tar_gz(&bad_tar, LINUX_BINARY_NAME, &dest)
                .expect_err("path traversal must fail");
            assert!(err.contains("tidak aman"), "unexpected error: {err}");
            assert!(!dest.exists());
        }
        let bad_pax = make_pax_record("path", "../../etc/polban-absen-gui");
        let bad_pax_tar = build_tar_gz(&[
            ("PaxHeaders/x", b'x', &bad_pax),
            ("safe-fallback", b'0', b"evil"),
        ]);
        assert!(extract_binary_from_tar_gz(&bad_pax_tar, LINUX_BINARY_NAME, &dest).is_err());

        // 4. Symlink entry with target name does not satisfy binary lookup, and missing binary errors.
        let symlink_only = build_tar_gz(&[("polban-absen-gui", b'2', b"")]);
        let missing_err = extract_binary_from_tar_gz(&symlink_only, LINUX_BINARY_NAME, &dest)
            .expect_err("symlink must not count as regular binary");
        assert!(missing_err.contains("tidak berisi"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn macos_app_and_binary_zip_extraction_replace_and_old_cleanup_work_safely() {
        let dir = make_temp_dir("macos-app");
        let current_app = dir.join("Polban Presensi.app");
        let current_bin = current_app
            .join("Contents")
            .join("MacOS")
            .join(MACOS_BINARY_NAME);
        fs::create_dir_all(current_bin.parent().unwrap()).unwrap();
        fs::write(&current_bin, b"old-macos-app-bin").unwrap();

        // Startup cleanup removes leftover <Name>.app.old directory.
        let stale_old_app = old_backup_path_for(&current_app);
        fs::create_dir_all(stale_old_app.join("Contents/MacOS")).unwrap();
        fs::write(
            stale_old_app.join("Contents/MacOS/polban-absen-gui"),
            b"ancient-app",
        )
        .unwrap();
        cleanup_old_executable_for(&current_bin);
        assert!(!stale_old_app.exists());

        // Extract valid .app bundle ZIP (including harmless __MACOSX metadata).
        let zip_bytes = build_macos_app_zip(
            "Polban Presensi.app",
            b"new-macos-app-bin-v3.4",
            &[(
                "__MACOSX/Polban Presensi.app/._Info.plist",
                b"appledouble",
                None,
            )],
        );
        let staged_app = staged_new_app_path_for(&current_app);
        let staged_bin =
            extract_macos_app_from_zip(&zip_bytes, &staged_app).expect("extract macos .app");
        assert_eq!(
            staged_bin,
            staged_app.join("Contents/MacOS/polban-absen-gui")
        );
        assert_eq!(fs::read(&staged_bin).unwrap(), b"new-macos-app-bin-v3.4");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&staged_bin).unwrap().permissions().mode();
            assert_ne!(mode & 0o111, 0, "macOS app executable must have +x bit");
        }

        // Atomic swap + backup.
        let backup = stage_replace_app_bundle(&current_app, &staged_app).expect("swap .app");
        assert_eq!(backup, stale_old_app);
        assert_eq!(fs::read(&current_bin).unwrap(), b"new-macos-app-bin-v3.4");
        assert_eq!(
            fs::read(backup.join("Contents/MacOS/polban-absen-gui")).unwrap(),
            b"old-macos-app-bin"
        );
        assert!(!staged_app.exists());

        // Reject traversal & symlinks inside macOS ZIP without leaving partial staging dir.
        let traversal_zip = build_macos_app_zip(
            "Polban Presensi.app",
            b"ok",
            &[("Polban Presensi.app/../../evil.txt", b"boom", None)],
        );
        let bad_stage = dir.join(".bad.app.new");
        assert!(extract_macos_app_from_zip(&traversal_zip, &bad_stage).is_err());
        assert!(!bad_stage.exists());

        let symlink_zip = build_macos_app_zip(
            "Polban Presensi.app",
            b"ok",
            &[(
                "Polban Presensi.app/Contents/Resources/link",
                b"/etc/passwd",
                Some(0o120_777),
            )],
        );
        assert!(extract_macos_app_from_zip(&symlink_zip, &bad_stage).is_err());
        assert!(!bad_stage.exists());
        let bad_standalone = dir.join(".bad-bin.new");
        assert!(extract_macos_binary_from_zip(&symlink_zip, &bad_standalone).is_err());
        assert!(!bad_standalone.exists());

        // Standalone macOS binary extraction from the same .app ZIP.
        let standalone_dest = dir.join(".polban-absen-gui.new");
        extract_macos_binary_from_zip(&zip_bytes, &standalone_dest)
            .expect("extract standalone macos binary");
        assert_eq!(
            fs::read(&standalone_dest).unwrap(),
            b"new-macos-app-bin-v3.4"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn httpmock_linux_rpm_targz_macos_app_and_429_head_fallback_e2e() {
        let server = MockServer::start();

        let rpm_payload = b"RPM-PKG-BYTES-v9.9.9";
        let tgz_payload = build_tar_gz(&[("polban-absen-gui", b'0', b"LINUX-ELF-v9.9.9")]);
        let mac_payload = build_macos_app_zip("Polban Presensi.app", b"MACHO-BIN-v9.9.9", &[]);

        let sums_body = format!(
            "{}  {LINUX_FEDORA_RPM}\n\
             {}  {LINUX_PORTABLE_TAR_GZ}\n\
             {}  {MACOS_APPLE_SILICON_ZIP}\n",
            compute_sha256_hex(rpm_payload),
            compute_sha256_hex(&tgz_payload),
            compute_sha256_hex(&mac_payload),
        );

        server.mock(|when, then| {
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
                            {{"name": "{LINUX_FEDORA_RPM}", "browser_download_url": "{}/dl/{LINUX_FEDORA_RPM}", "size": {}}},
                            {{"name": "{LINUX_PORTABLE_TAR_GZ}", "browser_download_url": "{}/dl/{LINUX_PORTABLE_TAR_GZ}", "size": {}}},
                            {{"name": "{MACOS_APPLE_SILICON_ZIP}", "browser_download_url": "{}/dl/{MACOS_APPLE_SILICON_ZIP}", "size": {}}},
                            {{"name": "{SHA256SUMS_NAME}", "browser_download_url": "{}/dl/{SHA256SUMS_NAME}", "size": {}}}
                        ]
                    }}"#,
                    server.base_url(),
                    server.base_url(),
                    rpm_payload.len(),
                    server.base_url(),
                    tgz_payload.len(),
                    server.base_url(),
                    mac_payload.len(),
                    server.base_url(),
                    sums_body.len(),
                ));
        });
        server.mock(|when, then| {
            when.method(GET).path(format!("/dl/{SHA256SUMS_NAME}"));
            then.status(200).body(sums_body.clone());
        });
        server.mock(|when, then| {
            when.method(GET).path(format!("/dl/{LINUX_FEDORA_RPM}"));
            then.status(200).body(rpm_payload.as_slice());
        });
        server.mock(|when, then| {
            when.method(GET)
                .path(format!("/dl/{LINUX_PORTABLE_TAR_GZ}"));
            then.status(200).body(tgz_payload.clone());
        });
        server.mock(|when, then| {
            when.method(GET)
                .path(format!("/dl/{MACOS_APPLE_SILICON_ZIP}"));
            then.status(200).body(mac_payload.clone());
        });

        let dir = make_temp_dir("cross-os-e2e");

        // 1. Linux Fedora RPM
        let rpm_caps = HostUpdateCapabilities {
            target_os: "linux".to_string(),
            target_arch: "x86_64".to_string(),
            exe_dir_writable: false,
            linux_rpm_upgradeable: true,
            macos_app_bundle: None,
            macos_app_parent_writable: false,
        };
        let CheckOutcome::Available(rpm_cand) = check_latest_release_with_capabilities(
            &server.url("/releases/latest"),
            None,
            "3.4.0",
            &rpm_caps,
        )
        .unwrap() else {
            panic!("expected Available");
        };
        assert_eq!(rpm_cand.strategy, AssetStrategy::LinuxFedoraRpm);
        let linux_exe = dir.join("polban-absen-gui");
        fs::write(&linux_exe, b"old-linux").unwrap();
        let rpm_plan =
            download_and_stage_candidate(&rpm_cand, &linux_exe, &dir, |_, _| {}).unwrap();
        assert_eq!(
            rpm_plan,
            RestartPlan::RunLinuxRpmUpgrade {
                rpm_path: dir.join(LINUX_FEDORA_RPM),
                current_exe: linux_exe.clone(),
            }
        );
        assert_eq!(fs::read(dir.join(LINUX_FEDORA_RPM)).unwrap(), rpm_payload);

        // 2. Linux .tar.gz portable
        let tgz_caps = HostUpdateCapabilities::new("linux", "x86_64", true);
        let CheckOutcome::Available(tgz_cand) = check_latest_release_with_capabilities(
            &server.url("/releases/latest"),
            None,
            "3.4.0",
            &tgz_caps,
        )
        .unwrap() else {
            panic!("expected Available");
        };
        assert_eq!(tgz_cand.strategy, AssetStrategy::LinuxTarGz);
        let tgz_plan =
            download_and_stage_candidate(&tgz_cand, &linux_exe, &dir, |_, _| {}).unwrap();
        assert_eq!(
            tgz_plan,
            RestartPlan::ReplaceExecutable {
                current_exe: linux_exe.clone(),
                staged_new: staged_new_path_for(&linux_exe),
            }
        );
        assert_eq!(
            fs::read(staged_new_path_for(&linux_exe)).unwrap(),
            b"LINUX-ELF-v9.9.9"
        );

        // 3. macOS .app ZIP
        let mac_app = dir.join("Polban Presensi.app");
        let mac_exe = mac_app.join("Contents/MacOS/polban-absen-gui");
        fs::create_dir_all(mac_exe.parent().unwrap()).unwrap();
        fs::write(&mac_exe, b"old-mac").unwrap();
        let mac_caps = HostUpdateCapabilities {
            target_os: "macos".to_string(),
            target_arch: "aarch64".to_string(),
            exe_dir_writable: false,
            linux_rpm_upgradeable: false,
            macos_app_bundle: Some(mac_app.clone()),
            macos_app_parent_writable: true,
        };
        let CheckOutcome::Available(mac_cand) = check_latest_release_with_capabilities(
            &server.url("/releases/latest"),
            None,
            "3.4.0",
            &mac_caps,
        )
        .unwrap() else {
            panic!("expected Available");
        };
        assert_eq!(mac_cand.strategy, AssetStrategy::MacOsAppZip);
        let mac_plan = download_and_stage_candidate(&mac_cand, &mac_exe, &dir, |_, _| {}).unwrap();
        assert_eq!(
            mac_plan,
            RestartPlan::ReplaceMacOsApp {
                current_app: mac_app.clone(),
                staged_app: staged_new_app_path_for(&mac_app),
                relaunch_exe: mac_exe,
            }
        );
        assert_eq!(
            fs::read(staged_new_app_path_for(&mac_app).join("Contents/MacOS/polban-absen-gui"))
                .unwrap(),
            b"MACHO-BIN-v9.9.9"
        );

        // 4. 429 rate-limit web fallback without SHA256SUMS.txt probes Linux & macOS via HEAD.
        let fb_server = MockServer::start();
        fb_server.mock(|when, then| {
            when.method(GET).path("/api/latest");
            then.status(429);
        });
        fb_server.mock(|when, then| {
            when.method(GET).path("/web/latest");
            then.status(302).header(
                "Location",
                "/raditted/Polban-Auto-Absen-rs/releases/tag/v9.9.9",
            );
        });
        fb_server.mock(|when, then| {
            when.method(httpmock::Method::HEAD).path(format!(
                "/raditted/Polban-Auto-Absen-rs/releases/download/v9.9.9/{LINUX_FEDORA_RPM}"
            ));
            then.status(200).header("content-length", "999");
        });
        fb_server.mock(|when, then| {
            when.method(httpmock::Method::HEAD).path(format!(
                "/raditted/Polban-Auto-Absen-rs/releases/download/v9.9.9/{MACOS_INTEL_ZIP}"
            ));
            then.status(200).header("content-length", "888");
        });

        let CheckOutcome::Available(fb_rpm) = check_latest_release_with_capabilities(
            &fb_server.url("/api/latest"),
            Some(&fb_server.url("/web/latest")),
            "3.4.0",
            &rpm_caps,
        )
        .unwrap() else {
            panic!("expected Available");
        };
        assert_eq!(fb_rpm.strategy, AssetStrategy::LinuxFedoraRpm);
        assert_eq!(
            fb_rpm.asset.as_ref().map(|a| a.name.as_str()),
            Some(LINUX_FEDORA_RPM)
        );

        let mac_intel_caps = HostUpdateCapabilities {
            target_arch: "x86_64".to_string(),
            ..mac_caps
        };
        let CheckOutcome::Available(fb_mac) = check_latest_release_with_capabilities(
            &fb_server.url("/api/latest"),
            Some(&fb_server.url("/web/latest")),
            "3.4.0",
            &mac_intel_caps,
        )
        .unwrap() else {
            panic!("expected Available");
        };
        assert_eq!(fb_mac.strategy, AssetStrategy::MacOsAppZip);
        assert_eq!(
            fb_mac.asset.as_ref().map(|a| a.name.as_str()),
            Some(MACOS_INTEL_ZIP)
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rpm_error_formatter_and_open_release_page_report_failures_cleanly() {
        assert_eq!(
            format_rpm_upgrade_error(Some(126), b"dismissed"),
            "Autentikasi dibatalkan. Klik untuk mencoba lagi."
        );
        assert_eq!(
            format_rpm_upgrade_error(Some(127), b"not authorized"),
            "Autentikasi dibatalkan. Klik untuk mencoba lagi."
        );
        let long_utf8 = format!("konflik {}", "é".repeat(140));
        let generic_err = format_rpm_upgrade_error(Some(1), long_utf8.as_bytes());
        assert!(generic_err.starts_with("Gagal memasang paket RPM: konflik "));
        assert!(generic_err.len() < 260);

        // apply_update_and_restart must reject RunLinuxRpmUpgrade (must go via start_rpm_install).
        let rpm_err = apply_update_and_restart(
            &RestartPlan::RunLinuxRpmUpgrade {
                rpm_path: PathBuf::from("/tmp/fake.rpm"),
                current_exe: PathBuf::from("/usr/bin/polban-absen-gui"),
            },
            &|| {},
        )
        .expect_err("RPM upgrade cannot run synchronously in apply_update_and_restart");
        assert!(rpm_err.contains("RPM"));

        // stage_replace_app_bundle errors cleanly without moving current_app when staged_app is missing.
        let stage_dir = make_temp_dir("app-rollback");
        let cur_app = stage_dir.join("Polban Presensi.app");
        fs::create_dir_all(cur_app.join("Contents")).unwrap();
        let missing_stage = stage_dir.join(".Polban Presensi.app.new");
        assert!(stage_replace_app_bundle(&cur_app, &missing_stage).is_err());
        assert!(cur_app.join("Contents").is_dir());
        let _ = fs::remove_dir_all(&stage_dir);

        // Non-blocking OpenReleasePage failure reports both UpdateStatus::Failed and manual_note.
        let mut manager = UpdateManager::new();
        let bad_candidate = UpdateCandidate {
            version: "9.9.9".to_string(),
            release_url: "javascript:alert(1)".to_string(),
            strategy: AssetStrategy::OpenReleasePage,
            asset: None,
            sha256sums_url: None,
        };
        manager.status = UpdateStatus::Available(bad_candidate.clone());
        let ctx = egui::Context::default();
        manager.start_download(&ctx);

        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            manager.poll();
            if matches!(manager.status, UpdateStatus::Failed { .. }) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        match &manager.status {
            UpdateStatus::Failed {
                message,
                retry_candidate,
            } => {
                assert!(message.contains("tidak valid"));
                assert_eq!(retry_candidate.as_ref(), Some(&bad_candidate));
            }
            other => panic!("expected Failed status, got {other:?}"),
        }
        assert!(
            matches!(&manager.manual_note, Some((note, true)) if note.contains("tidak valid")),
            "expected error manual_note, got {:?}",
            manager.manual_note
        );
    }
}

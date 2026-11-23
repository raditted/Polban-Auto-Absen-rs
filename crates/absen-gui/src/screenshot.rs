//! Local attendance-section HTML sanitizer, headless Chromium screenshot capture,
//! and pure-Rust fallback renderer using bundled OFL fonts.

use std::io::{Cursor, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use absen_core::parser::{find_rows_with_meta, parse_user_header};
use absen_core::{AttendanceProof, Color, PortalPageSnapshot, ProofVerification, RowMeta};
use image::codecs::png::PngEncoder;
use image::{ColorType, ImageEncoder, ImageFormat, Rgba, RgbaImage};

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);
static CHROMIUM_BINARY: OnceLock<Option<PathBuf>> = OnceLock::new();
static SAMPLE_PNG_CACHE: OnceLock<Vec<u8>> = OnceLock::new();

const FONT_REGULAR_BYTES: &[u8] = include_bytes!("../assets/fonts/PlusJakartaSans-Regular.ttf");
const FONT_SEMIBOLD_BYTES: &[u8] = include_bytes!("../assets/fonts/PlusJakartaSans-SemiBold.ttf");
const FONT_MONO_BYTES: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");

const CHROMIUM_TIMEOUT: Duration = Duration::from_secs(10);
const CROP_PADDING_PX: u32 = 28;
const CROP_MIN_HEIGHT_PX: u32 = 260;

/// Origin of a rendered attendance screenshot PNG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenshotSource {
    HeadlessBrowser,
    NativeFallback,
}

impl ScreenshotSource {
    /// Human-readable Indonesian badge label shown inside the proof modal.
    pub const fn badge_label(self) -> &'static str {
        match self {
            Self::HeadlessBrowser => "Tampilan Web Portal",
            Self::NativeFallback => "Pratinjau Tabel Portal",
        }
    }
}

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(prefix: &str) -> Result<Self, String> {
        let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("{prefix}_{}_{seq}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("gagal membuat direktori sementara: {e}"))?;
        Ok(Self(dir))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if std::fs::remove_dir_all(&self.0).is_err() {
            std::thread::sleep(Duration::from_millis(40));
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn html_escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\u{2014}' => out.push('-'),
            c => out.push(c),
        }
    }
    out.replace(concat!(" ", "-", "-", " "), " - ")
}

fn sanitize_base_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return None;
    }
    if trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    if trimmed.contains(['"', '\'', '<', '>']) {
        return None;
    }
    Some(html_escape(trimmed))
}

fn is_tag_boundary(byte: Option<u8>) -> bool {
    match byte {
        None => true,
        Some(b) => b.is_ascii_whitespace() || b == b'>' || b == b'/',
    }
}

/// Remove `<tag ...>...</tag>` blocks case-insensitively on valid UTF-8 boundaries.
fn strip_tag_block_ci(html: &str, tag: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let open_needle = format!("<{tag}");
    let close_needle = format!("</{tag}");
    let mut out = String::with_capacity(html.len());
    let mut cursor = 0;

    while cursor < html.len() {
        let Some(rel_start) = lower[cursor..].find(&open_needle) else {
            out.push_str(&html[cursor..]);
            break;
        };
        let start = cursor + rel_start;
        let after_name = start + open_needle.len();
        if !is_tag_boundary(lower.as_bytes().get(after_name).copied()) {
            out.push_str(&html[cursor..after_name]);
            cursor = after_name;
            continue;
        }

        out.push_str(&html[cursor..start]);

        let Some(rel_open_end) = lower[after_name..].find('>') else {
            // Unclosed opening tag: drop remainder so trailing script/iframe cannot execute.
            break;
        };
        let content_start = after_name + rel_open_end + 1;

        let mut search_from = content_start;
        let mut closed_at = None;
        while search_from < lower.len() {
            let Some(rel_close) = lower[search_from..].find(&close_needle) else {
                break;
            };
            let close_pos = search_from + rel_close;
            let after_close_name = close_pos + close_needle.len();
            if is_tag_boundary(lower.as_bytes().get(after_close_name).copied()) {
                if let Some(rel_gt) = lower[after_close_name..].find('>') {
                    closed_at = Some(after_close_name + rel_gt + 1);
                    break;
                }
                closed_at = Some(html.len());
                break;
            }
            search_from = after_close_name;
        }

        match closed_at {
            Some(end) => cursor = end,
            None => break,
        }
    }

    out
}

/// Remove `<meta ... http-equiv="refresh" ...>` tags case-insensitively.
fn strip_meta_refresh_ci(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut cursor = 0;

    while cursor < html.len() {
        let Some(rel_start) = lower[cursor..].find("<meta") else {
            out.push_str(&html[cursor..]);
            break;
        };
        let start = cursor + rel_start;
        let after_name = start + 5;
        let next_b = lower.as_bytes().get(after_name).copied();
        if !next_b.is_some_and(|b| b.is_ascii_whitespace() || b == b'/') {
            out.push_str(&html[cursor..after_name]);
            cursor = after_name;
            continue;
        }

        out.push_str(&html[cursor..start]);
        let Some(rel_gt) = lower[after_name..].find('>') else {
            out.push_str(&html[start..]);
            break;
        };
        let end = after_name + rel_gt + 1;
        let tag_lower = &lower[start..end];
        if !(tag_lower.contains("http-equiv") && tag_lower.contains("refresh")) {
            out.push_str(&html[start..end]);
        }
        cursor = end;
    }

    out
}

/// Remove `<link ...>` tags referencing external font providers (e.g., `//fonts.googleapis.com`).
fn strip_external_font_links_ci(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut cursor = 0;

    while cursor < html.len() {
        let Some(rel_start) = lower[cursor..].find("<link") else {
            out.push_str(&html[cursor..]);
            break;
        };
        let start = cursor + rel_start;
        let after_name = start + 5;
        if !is_tag_boundary(lower.as_bytes().get(after_name).copied()) {
            out.push_str(&html[cursor..after_name]);
            cursor = after_name;
            continue;
        }

        out.push_str(&html[cursor..start]);
        let Some(rel_gt) = lower[after_name..].find('>') else {
            out.push_str(&html[start..]);
            break;
        };
        let end = after_name + rel_gt + 1;
        let tag_lower = &lower[start..end];
        let is_font_link = tag_lower.contains("//fonts.")
            || tag_lower.contains("fonts.googleapis.com")
            || tag_lower.contains("fonts.gstatic.com");
        if !is_font_link {
            out.push_str(&html[start..end]);
        }
        cursor = end;
    }

    out
}

fn find_at_import_end(css_bytes: &[u8], after_keyword: usize) -> usize {
    let mut i = after_keyword;
    let mut in_quote: Option<u8> = None;
    let mut paren_depth: usize = 0;

    while i < css_bytes.len() {
        let b = css_bytes[i];
        if let Some(q) = in_quote {
            if b == b'\\' {
                i = (i + 2).min(css_bytes.len());
                continue;
            }
            if b == q {
                in_quote = None;
            }
            i += 1;
            continue;
        }

        match b {
            b'\'' | b'"' => in_quote = Some(b),
            b'(' => paren_depth += 1,
            b')' => paren_depth = paren_depth.saturating_sub(1),
            b';' if paren_depth == 0 => return i + 1,
            b'\n' | b'\r' if paren_depth == 0 => return i,
            _ => {}
        }
        i += 1;
    }
    css_bytes.len()
}

fn strip_external_font_imports_in_css(css: &str) -> String {
    let lower = css.to_ascii_lowercase();
    let bytes = css.as_bytes();
    let mut out = String::with_capacity(css.len());
    let mut cursor = 0;

    while cursor < css.len() {
        let Some(rel_start) = lower[cursor..].find("@import") else {
            out.push_str(&css[cursor..]);
            break;
        };
        let start = cursor + rel_start;
        let after_kw = start + 7;
        let valid_sep = bytes
            .get(after_kw)
            .is_some_and(|b| b.is_ascii_whitespace() || matches!(b, b'(' | b'\'' | b'"'));
        if !valid_sep {
            out.push_str(&css[cursor..after_kw]);
            cursor = after_kw;
            continue;
        }

        out.push_str(&css[cursor..start]);
        let end = find_at_import_end(bytes, after_kw);
        let stmt_lower = &lower[start..end];
        let is_external_font = stmt_lower.contains("//fonts.")
            || stmt_lower.contains("fonts.googleapis.com")
            || stmt_lower.contains("fonts.gstatic.com")
            || (stmt_lower.contains("//") && stmt_lower.contains("font"));
        if !is_external_font {
            out.push_str(&css[start..end]);
        }
        cursor = end;
    }

    out
}

/// Remove protocol-relative / external font `@import` rules inside `<style>` blocks.
fn strip_external_font_imports_in_styles_ci(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut cursor = 0;

    while cursor < html.len() {
        let Some(rel_start) = lower[cursor..].find("<style") else {
            out.push_str(&html[cursor..]);
            break;
        };
        let start = cursor + rel_start;
        let after_name = start + 6;
        if !is_tag_boundary(lower.as_bytes().get(after_name).copied()) {
            out.push_str(&html[cursor..after_name]);
            cursor = after_name;
            continue;
        }

        let Some(rel_open_end) = lower[after_name..].find('>') else {
            out.push_str(&html[cursor..]);
            break;
        };
        let content_start = after_name + rel_open_end + 1;
        out.push_str(&html[cursor..content_start]);

        let mut search_from = content_start;
        let mut close_start = None;
        while search_from < lower.len() {
            let Some(rel_close) = lower[search_from..].find("</style") else {
                break;
            };
            let pos = search_from + rel_close;
            let after_close = pos + 7;
            if is_tag_boundary(lower.as_bytes().get(after_close).copied()) {
                close_start = Some(pos);
                break;
            }
            search_from = after_close;
        }

        match close_start {
            Some(end) => {
                out.push_str(&strip_external_font_imports_in_css(
                    &html[content_start..end],
                ));
                cursor = end;
            }
            None => {
                out.push_str(&strip_external_font_imports_in_css(&html[content_start..]));
                cursor = html.len();
            }
        }
    }

    out
}

fn find_tag_open_end_ci(html: &str, tag: &str) -> Option<usize> {
    let lower = html.to_ascii_lowercase();
    let needle = format!("<{tag}");
    let mut from = 0;
    while from < lower.len() {
        let rel = lower[from..].find(&needle)?;
        let start = from + rel;
        let after_name = start + needle.len();
        if is_tag_boundary(lower.as_bytes().get(after_name).copied()) {
            let rel_gt = lower[after_name..].find('>')?;
            return Some(after_name + rel_gt + 1);
        }
        from = after_name;
    }
    None
}

fn find_tag_start_ci(html: &str, needle: &str) -> Option<usize> {
    html.to_ascii_lowercase().find(&needle.to_ascii_lowercase())
}

const PORTAL_VIEW_STYLE: &str = r#"<style id="absen-gui-shot-style">
* {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif !important;
}
:where(body) {
  margin: 0;
  padding: 14px 18px;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
  background: #f8fafc;
  color: #0f172a;
}
:where(.main-header, .user-header) {
  margin-bottom: 12px;
  padding: 10px 14px;
  background: #ffffff;
  border: 1px solid #cbd5e1;
  border-radius: 6px;
  font-size: 13px;
  font-weight: 600;
  color: #0f172a;
}
:where(.user-header span) {
  margin-right: 10px;
}
:where(table, table#jadwal, .table) {
  width: 100%;
  border-collapse: collapse;
  background: #ffffff;
  font-size: 13px;
  border: 1px solid #cbd5e1;
}
:where(table th, table td, .table th, .table td) {
  border: 1px solid #cbd5e1;
  padding: 8px 10px;
  text-align: left;
  vertical-align: middle;
}
:where(table thead th, .table thead th) {
  background: #f1f5f9;
  font-weight: 600;
  color: #0f172a;
}
:where(.btn) {
  display: inline-block;
  padding: 5px 10px;
  font-size: 12px;
  font-weight: 600;
  line-height: 1.3;
  border-radius: 4px;
  border: 1px solid transparent;
  text-decoration: none;
  white-space: nowrap;
}
:where(.btn-success) { background-color: #16a34a; border-color: #15803d; color: #ffffff; }
:where(.btn-primary) { background-color: #2563eb; border-color: #1d4ed8; color: #ffffff; }
:where(.btn-info)    { background-color: #0284c7; border-color: #0369a1; color: #ffffff; }
:where(.btn-warning) { background-color: #d97706; border-color: #b45309; color: #ffffff; }
:where(.btn-danger)  { background-color: #dc2626; border-color: #b91c1c; color: #ffffff; }
:where(.btn-default) { background-color: #e2e8f0; border-color: #cbd5e1; color: #334155; }
#absen-gui-stamp {
  margin-top: 12px;
  padding: 8px 12px;
  background: #f1f5f9;
  border: 1px solid #cbd5e1;
  border-radius: 6px;
  color: #334155;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 11.5px;
  line-height: 1.4;
}
</style>"#;

/// Sanitize and prepare portal HTML for offline local `file://` rendering without scripts or redirects.
pub fn prepare_attendance_section_html(
    snapshot: &PortalPageSnapshot,
    proof: Option<&AttendanceProof>,
) -> String {
    let mut cleaned = strip_tag_block_ci(&snapshot.html, "script");
    cleaned = strip_tag_block_ci(&cleaned, "iframe");
    cleaned = strip_meta_refresh_ci(&cleaned);
    cleaned = strip_external_font_links_ci(&cleaned);
    cleaned = strip_external_font_imports_in_styles_ci(&cleaned);

    let mut head_injection = String::new();
    if let Some(safe_base) = sanitize_base_url(&snapshot.base_url) {
        head_injection.push_str(&format!("<base href=\"{safe_base}/\">"));
    }
    head_injection.push_str(PORTAL_VIEW_STYLE);

    if let Some(head_end) = find_tag_open_end_ci(&cleaned, "head") {
        cleaned.insert_str(head_end, &head_injection);
    } else if let Some(html_end) = find_tag_open_end_ci(&cleaned, "html") {
        cleaned.insert_str(html_end, &format!("<head>{head_injection}</head>"));
    } else {
        cleaned = format!(
            "<!DOCTYPE html><html><head>{head_injection}</head><body>{cleaned}</body></html>"
        );
    }

    let mut stamp = format!(
        "Sumber: {} | Diambil: {}",
        html_escape(snapshot.target_url.trim()),
        html_escape(snapshot.captured_at.trim())
    );
    if let Some(p) = proof {
        let pid = p.proof_id.trim();
        if !pid.is_empty() {
            stamp.push_str(&format!(" | ID Bukti: {}", html_escape(pid)));
        }
    }
    let stamp_html = format!("<div id=\"absen-gui-stamp\">{stamp}</div>");

    if let Some(body_close) = find_tag_start_ci(&cleaned, "</body>") {
        cleaned.insert_str(body_close, &stamp_html);
    } else {
        cleaned.push_str(&stamp_html);
    }

    cleaned
}

fn find_in_path(names: &[&str]) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Discover an installed Chromium-based browser binary for local headless HTML rendering (cached).
pub fn find_chromium_binary() -> Option<PathBuf> {
    CHROMIUM_BINARY
        .get_or_init(discover_chromium_binary_uncached)
        .clone()
}

fn discover_chromium_binary_uncached() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let rel_paths = [
            r"Microsoft\Edge\Application\msedge.exe",
            r"Google\Chrome\Application\chrome.exe",
            r"BraveSoftware\Brave-Browser\Application\brave.exe",
            r"Vivaldi\Application\vivaldi.exe",
            r"Chromium\Application\chrome.exe",
        ];
        let roots = ["ProgramFiles", "ProgramFiles(x86)", "LocalAppData"];
        for rel in rel_paths {
            for env_key in roots {
                if let Some(base) = std::env::var_os(env_key) {
                    let candidate = PathBuf::from(base).join(rel);
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
        find_in_path(&[
            "msedge.exe",
            "chrome.exe",
            "brave.exe",
            "vivaldi.exe",
            "chromium.exe",
        ])
    }

    #[cfg(target_os = "macos")]
    {
        let candidates = [
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
            "/Applications/Vivaldi.app/Contents/MacOS/Vivaldi",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ];
        for p in candidates {
            let pb = PathBuf::from(p);
            if pb.is_file() {
                return Some(pb);
            }
        }
        find_in_path(&[
            "microsoft-edge",
            "google-chrome",
            "brave-browser",
            "vivaldi",
            "chromium",
        ])
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let names = [
            "microsoft-edge",
            "google-chrome",
            "chromium",
            "chromium-browser",
            "brave-browser",
            "vivaldi",
        ];
        if let Some(found) = find_in_path(&names) {
            return Some(found);
        }
        for dir in ["/usr/bin", "/usr/local/bin", "/snap/bin"] {
            for name in names {
                let pb = Path::new(dir).join(name);
                if pb.is_file() {
                    return Some(pb);
                }
            }
        }
        None
    }
}

fn path_to_file_url(path: &Path) -> String {
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let raw = abs.to_string_lossy();
    let trimmed = raw.strip_prefix(r"\\?\").unwrap_or(&raw);
    let slashed = trimmed.replace('\\', "/");
    let mut encoded = String::with_capacity(slashed.len() + 8);
    for ch in slashed.chars() {
        match ch {
            ' ' => encoded.push_str("%20"),
            '%' => encoded.push_str("%25"),
            '#' => encoded.push_str("%23"),
            '?' => encoded.push_str("%3F"),
            c => encoded.push(c),
        }
    }
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

fn apply_no_window(cmd: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = cmd;
}

fn kill_child_tree(child: &mut Child) {
    #[cfg(target_os = "windows")]
    {
        let pid = child.id();
        let mut tk = Command::new("taskkill");
        tk.args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        apply_no_window(&mut tk);
        let _ = tk.status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

struct ChildTreeGuard(Option<Child>);

impl ChildTreeGuard {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }

    fn as_mut(&mut self) -> Option<&mut Child> {
        self.0.as_mut()
    }
}

impl Drop for ChildTreeGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            kill_child_tree(&mut child);
        }
    }
}

fn is_brave_binary(bin: &Path) -> bool {
    bin.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.to_ascii_lowercase().contains("brave"))
}

fn decode_base64_standard(input: &str) -> Result<Vec<u8>, String> {
    fn val(b: u8) -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let mut clean = Vec::with_capacity(input.len());
    for &b in input.as_bytes() {
        if !b.is_ascii_whitespace() {
            clean.push(b);
        }
    }
    if clean.len() % 4 != 0 {
        return Err("panjang base64 tidak valid".to_string());
    }
    let mut out = Vec::with_capacity((clean.len() / 4) * 3);
    for chunk in clean.as_chunks::<4>().0 {
        let a = val(chunk[0]).ok_or_else(|| "karakter base64 tidak valid".to_string())?;
        let b = val(chunk[1]).ok_or_else(|| "karakter base64 tidak valid".to_string())?;
        let pad2 = chunk[2] == b'=';
        let pad3 = chunk[3] == b'=';
        let c = if pad2 {
            0
        } else {
            val(chunk[2]).ok_or_else(|| "karakter base64 tidak valid".to_string())?
        };
        let d = if pad3 {
            0
        } else {
            val(chunk[3]).ok_or_else(|| "karakter base64 tidak valid".to_string())?
        };
        out.push((a << 2) | (b >> 4));
        if !pad2 {
            out.push((b << 4) | (c >> 2));
        }
        if !pad3 {
            out.push((c << 6) | d);
        }
    }
    Ok(out)
}

fn http_get_localhost(port: u16, path: &str, timeout: Duration) -> Result<Vec<u8>, String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, timeout)
        .map_err(|e| format!("gagal terhubung ke DevTools localhost: {e}"))?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));

    let req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("gagal mengirim request DevTools: {e}"))?;

    let mut raw = Vec::with_capacity(4096);
    let mut buf = [0u8; 4096];
    let mut header_end: Option<usize> = None;
    let mut content_len: Option<usize> = None;

    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if header_end.is_none() {
                    if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        let end = pos + 4;
                        header_end = Some(end);
                        let hdr_str = String::from_utf8_lossy(&raw[..pos]);
                        for line in hdr_str.lines() {
                            if let Some((k, v)) = line.split_once(':') {
                                if k.trim().eq_ignore_ascii_case("content-length") {
                                    content_len = v.trim().parse::<usize>().ok();
                                }
                            }
                        }
                    }
                }
                if let (Some(end), Some(len)) = (header_end, content_len) {
                    if raw.len() >= end + len {
                        return Ok(raw[end..end + len].to_vec());
                    }
                }
            }
            Err(e) => {
                if let Some(end) = header_end {
                    if content_len.is_none() && raw.len() > end {
                        return Ok(raw[end..].to_vec());
                    }
                }
                return Err(format!("gagal membaca respon DevTools: {e}"));
            }
        }
    }
    let sep = header_end.ok_or_else(|| "respon HTTP DevTools tidak valid".to_string())?;
    Ok(raw[sep..].to_vec())
}

fn ws_connect_localhost(port: u16, ws_path: &str, timeout: Duration) -> Result<TcpStream, String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, timeout)
        .map_err(|e| format!("gagal membuka WebSocket DevTools: {e}"))?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));

    let req = format!(
        "GET {ws_path} HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n"
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("gagal handshake WebSocket DevTools: {e}"))?;

    let mut hdr = Vec::with_capacity(512);
    let mut byte = [0u8; 1];
    while hdr.len() < 8192 {
        stream
            .read_exact(&mut byte)
            .map_err(|e| format!("gagal membaca header handshake WebSocket: {e}"))?;
        hdr.push(byte[0]);
        if hdr.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let hdr_str = String::from_utf8_lossy(&hdr);
    if !hdr_str.contains("101") {
        return Err("handshake WebSocket DevTools ditolak".to_string());
    }
    Ok(stream)
}

fn ws_send_text(stream: &mut TcpStream, text: &str) -> Result<(), String> {
    let payload = text.as_bytes();
    let len = payload.len();
    let mask = [0x37u8, 0xfa, 0x21, 0x3d];
    let mut frame = Vec::with_capacity(len + 14);
    frame.push(0x81); // FIN + Text
    if len < 126 {
        frame.push(0x80 | (len as u8));
    } else if len <= 65535 {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(0x80 | 127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }
    frame.extend_from_slice(&mask);
    for (i, &b) in payload.iter().enumerate() {
        frame.push(b ^ mask[i % 4]);
    }
    stream
        .write_all(&frame)
        .map_err(|e| format!("gagal mengirim perintah CDP: {e}"))
}

fn ws_read_text_message(stream: &mut TcpStream) -> Result<String, String> {
    let mut msg = Vec::new();
    loop {
        let mut h = [0u8; 2];
        stream
            .read_exact(&mut h)
            .map_err(|e| format!("gagal membaca frame CDP: {e}"))?;
        let fin = (h[0] & 0x80) != 0;
        let opcode = h[0] & 0x0f;
        let masked = (h[1] & 0x80) != 0;
        let mut len = (h[1] & 0x7f) as usize;
        if len == 126 {
            let mut ext = [0u8; 2];
            stream
                .read_exact(&mut ext)
                .map_err(|e| format!("gagal membaca panjang frame CDP: {e}"))?;
            len = u16::from_be_bytes(ext) as usize;
        } else if len == 127 {
            let mut ext = [0u8; 8];
            stream
                .read_exact(&mut ext)
                .map_err(|e| format!("gagal membaca panjang frame CDP 64-bit: {e}"))?;
            len = u64::from_be_bytes(ext) as usize;
        }
        let mut mask_key = [0u8; 4];
        if masked {
            stream
                .read_exact(&mut mask_key)
                .map_err(|e| format!("gagal membaca mask frame CDP: {e}"))?;
        }
        let mut payload = vec![0u8; len];
        if len > 0 {
            stream
                .read_exact(&mut payload)
                .map_err(|e| format!("gagal membaca payload CDP: {e}"))?;
        }
        if masked {
            for (i, b) in payload.iter_mut().enumerate() {
                *b ^= mask_key[i % 4];
            }
        }
        match opcode {
            0x8 => return Err("koneksi CDP ditutup oleh browser".to_string()),
            0x9 => {
                // Reply to ping with pong
                let mut pong = Vec::with_capacity(payload.len() + 6);
                pong.push(0x8a);
                if payload.len() < 126 {
                    pong.push(0x80 | payload.len() as u8);
                    pong.extend_from_slice(&[0u8; 4]);
                    pong.extend_from_slice(&payload);
                    let _ = stream.write_all(&pong);
                }
                continue;
            }
            0xa => continue,
            _ => msg.extend_from_slice(&payload),
        }
        if fin {
            return String::from_utf8(msg).map_err(|e| format!("pesan CDP bukan UTF-8 valid: {e}"));
        }
    }
}

fn cdp_call(
    stream: &mut TcpStream,
    id: u64,
    method: &str,
    params: serde_json::Value,
    deadline: Instant,
) -> Result<serde_json::Value, String> {
    let req = serde_json::json!({
        "id": id,
        "method": method,
        "params": params,
    });
    ws_send_text(stream, &req.to_string())?;
    loop {
        if Instant::now() >= deadline {
            return Err("batas waktu perintah CDP terlampaui".to_string());
        }
        let txt = ws_read_text_message(stream)?;
        let val: serde_json::Value =
            serde_json::from_str(&txt).map_err(|e| format!("JSON CDP tidak valid: {e}"))?;
        if val.get("id").and_then(|v| v.as_u64()) == Some(id) {
            if let Some(err) = val.get("error") {
                return Err(format!("CDP error pada {method}: {err}"));
            }
            return Ok(val
                .get("result")
                .cloned()
                .unwrap_or(serde_json::Value::Null));
        }
    }
}

fn capture_with_chromium_cdp(
    bin: &Path,
    profile_dir: &Path,
    file_url: &str,
) -> Result<Vec<u8>, String> {
    let mut cmd = Command::new(bin);
    cmd.arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-extensions")
        .arg("--disable-background-networking")
        .arg("--disable-sync")
        .arg("--metrics-recording-only")
        .arg("--hide-scrollbars")
        .arg("--window-size=1120,760")
        .arg("--remote-debugging-port=0")
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg(file_url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    apply_no_window(&mut cmd);

    let child = cmd
        .spawn()
        .map_err(|e| format!("gagal menjalankan browser headless CDP: {e}"))?;
    let mut child_guard = ChildTreeGuard::new(child);
    let deadline = Instant::now() + CHROMIUM_TIMEOUT;

    let port_file = profile_dir.join("DevToolsActivePort");
    let port: u16 = loop {
        if let Some(c) = child_guard.as_mut() {
            if let Ok(Some(_)) = c.try_wait() {
                return Err("browser headless berhenti sebelum DevTools siap".to_string());
            }
        }
        if let Ok(raw) = std::fs::read_to_string(&port_file) {
            if let Some(first) = raw.lines().next() {
                if let Ok(p) = first.trim().parse::<u16>() {
                    if p > 0 {
                        break p;
                    }
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(
                "batas waktu menunggu DevToolsActivePort (10 detik) terlampaui".to_string(),
            );
        }
        std::thread::sleep(Duration::from_millis(40));
    };

    let ws_path = loop {
        let rem = deadline.saturating_duration_since(Instant::now());
        if rem.is_zero() {
            return Err("batas waktu membaca target DevTools terlampaui".to_string());
        }
        if let Ok(body) = http_get_localhost(port, "/json/list", rem.min(Duration::from_secs(2))) {
            if let Ok(targets) = serde_json::from_slice::<serde_json::Value>(&body) {
                if let Some(arr) = targets.as_array() {
                    if let Some(ws_url) = arr.iter().find_map(|item| {
                        let is_page = item.get("type").and_then(|v| v.as_str()) == Some("page");
                        if is_page {
                            item.get("webSocketDebuggerUrl").and_then(|v| v.as_str())
                        } else {
                            None
                        }
                    }) {
                        let prefix = format!("ws://127.0.0.1:{port}");
                        if let Some(path) = ws_url.strip_prefix(&prefix) {
                            break path.to_string();
                        }
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    };

    let rem = deadline.saturating_duration_since(Instant::now());
    let mut ws = ws_connect_localhost(port, &ws_path, rem.max(Duration::from_millis(500)))?;

    // Wait briefly (up to 2.5s) for local page + external CSS load to complete.
    let load_budget = Instant::now() + Duration::from_millis(2500);
    let mut req_id = 1u64;
    while Instant::now() < load_budget && Instant::now() < deadline {
        let res = cdp_call(
            &mut ws,
            req_id,
            "Runtime.evaluate",
            serde_json::json!({ "expression": "document.readyState" }),
            deadline,
        )?;
        req_id += 1;
        let state = res
            .get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.as_str());
        if state == Some("complete") {
            break;
        }
        std::thread::sleep(Duration::from_millis(60));
    }

    let _ = cdp_call(
        &mut ws,
        req_id,
        "Emulation.setDeviceMetricsOverride",
        serde_json::json!({
            "width": 1120,
            "height": 760,
            "deviceScaleFactor": 1,
            "mobile": false
        }),
        deadline,
    );
    req_id += 1;

    // Wait for document.fonts.ready (bounded by a short timeout) so text never captures mid-FOIT.
    let fonts_rem_ms = deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_millis(1500))
        .as_millis() as u64;
    if fonts_rem_ms > 50 {
        let wait_ms = fonts_rem_ms.saturating_sub(20);
        let expr = format!(
            "Promise.race([(document.fonts && document.fonts.ready) ? document.fonts.ready.catch(function(){{}}) : Promise.resolve(), new Promise(function(r){{ setTimeout(r, {wait_ms}); }})])"
        );
        let _ = cdp_call(
            &mut ws,
            req_id,
            "Runtime.evaluate",
            serde_json::json!({
                "expression": expr,
                "awaitPromise": true
            }),
            deadline,
        );
        req_id += 1;
    }

    let shot_res = cdp_call(
        &mut ws,
        req_id,
        "Page.captureScreenshot",
        serde_json::json!({ "format": "png" }),
        deadline,
    )?;
    let b64 = shot_res
        .get("data")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "CDP Page.captureScreenshot tidak mengembalikan data PNG".to_string())?;
    let raw_png = decode_base64_standard(b64)?;
    drop(ws);
    drop(child_guard);
    crop_png_bottom_whitespace(&raw_png)
}

fn capture_with_chromium(bin: &Path, prepared_html: &str) -> Result<Vec<u8>, String> {
    let guard = TempDirGuard::new("polban_absen_shot")?;
    let profile_dir = guard.path().join("profile");
    let html_path = guard.path().join("page.html");
    let png_path = guard.path().join("shot.png");

    std::fs::create_dir_all(&profile_dir)
        .map_err(|e| format!("gagal membuat profil sementara browser: {e}"))?;
    std::fs::write(&html_path, prepared_html.as_bytes())
        .map_err(|e| format!("gagal menulis HTML sementara: {e}"))?;

    let file_url = path_to_file_url(&html_path);

    if is_brave_binary(bin) {
        return capture_with_chromium_cdp(bin, &profile_dir, &file_url);
    }

    let mut cmd = Command::new(bin);
    cmd.arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-extensions")
        .arg("--disable-background-networking")
        .arg("--disable-sync")
        .arg("--metrics-recording-only")
        .arg("--hide-scrollbars")
        .arg("--virtual-time-budget=2500")
        .arg("--window-size=1120,760")
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg(format!("--screenshot={}", png_path.display()))
        .arg(&file_url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    apply_no_window(&mut cmd);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("gagal menjalankan browser headless: {e}"))?;

    let deadline = Instant::now() + CHROMIUM_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) => {
                if Instant::now() >= deadline {
                    kill_child_tree(&mut child);
                    return Err(
                        "batas waktu render browser headless (10 detik) terlampaui".to_string()
                    );
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => {
                kill_child_tree(&mut child);
                return Err(format!("gagal menunggu browser headless: {e}"));
            }
        }
    };

    if !status.success() || !png_path.is_file() {
        return Err("browser headless tidak menghasilkan file PNG".to_string());
    }

    let raw_png = std::fs::read(&png_path).map_err(|e| format!("gagal membaca hasil PNG: {e}"))?;
    crop_png_bottom_whitespace(&raw_png)
}

fn pixel_close(a: Rgba<u8>, b: Rgba<u8>) -> bool {
    a[0].abs_diff(b[0]) <= 6
        && a[1].abs_diff(b[1]) <= 6
        && a[2].abs_diff(b[2]) <= 6
        && a[3].abs_diff(b[3]) <= 6
}

/// Trim excess blank background whitespace from the bottom of a screenshot PNG.
pub fn crop_png_bottom_whitespace(png_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory_with_format(png_bytes, ImageFormat::Png)
        .map_err(|e| format!("PNG tidak valid: {e}"))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    if w == 0 || h <= CROP_MIN_HEIGHT_PX {
        return Ok(png_bytes.to_vec());
    }

    let bg = *img.get_pixel(w.saturating_sub(2), h.saturating_sub(2));
    let mut last_content_y = 0u32;
    let mut found_content = false;

    'outer: for y in (0..h).rev() {
        for x in 0..w {
            if !pixel_close(*img.get_pixel(x, y), bg) {
                last_content_y = y;
                found_content = true;
                break 'outer;
            }
        }
    }

    if !found_content {
        return Ok(png_bytes.to_vec());
    }

    let target_h = last_content_y
        .saturating_add(1 + CROP_PADDING_PX)
        .max(CROP_MIN_HEIGHT_PX)
        .min(h);
    if target_h >= h {
        return Ok(png_bytes.to_vec());
    }

    let cropped = image::imageops::crop_imm(&img, 0, 0, w, target_h).to_image();
    encode_rgba_png(&cropped)
}

fn encode_rgba_png(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    let encoder = PngEncoder::new(Cursor::new(&mut buf));
    encoder
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            ColorType::Rgba8.into(),
        )
        .map_err(|e| format!("gagal encode PNG: {e}"))?;
    Ok(buf)
}

fn blend_pixel(img: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>, coverage: f32) {
    if x < 0 || y < 0 || x >= img.width() as i32 || y >= img.height() as i32 {
        return;
    }
    let alpha = (coverage.clamp(0.0, 1.0) * (color[3] as f32 / 255.0)).clamp(0.0, 1.0);
    if alpha <= 0.001 {
        return;
    }
    let dst = img.get_pixel_mut(x as u32, y as u32);
    let inv = 1.0 - alpha;
    dst[0] = ((color[0] as f32 * alpha) + (dst[0] as f32 * inv)).round() as u8;
    dst[1] = ((color[1] as f32 * alpha) + (dst[1] as f32 * inv)).round() as u8;
    dst[2] = ((color[2] as f32 * alpha) + (dst[2] as f32 * inv)).round() as u8;
    dst[3] = 255;
}

fn fill_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, color: Rgba<u8>) {
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + w).min(img.width() as i32);
    let y1 = (y + h).min(img.height() as i32);
    for py in y0..y1 {
        for px in x0..x1 {
            img.put_pixel(px as u32, py as u32, color);
        }
    }
}

fn stroke_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, color: Rgba<u8>) {
    if w <= 0 || h <= 0 {
        return;
    }
    fill_rect(img, x, y, w, 1, color);
    fill_rect(img, x, y + h - 1, w, 1, color);
    fill_rect(img, x, y, 1, h, color);
    fill_rect(img, x + w - 1, y, 1, h, color);
}

fn measure_text_width(font: &FontRef<'_>, px: f32, text: &str) -> f32 {
    let scaled = font.as_scaled(PxScale::from(px));
    let mut width = 0.0f32;
    let mut prev = None;
    for ch in text.chars() {
        let gid = scaled.glyph_id(ch);
        if let Some(p) = prev {
            width += scaled.kern(p, gid);
        }
        width += scaled.h_advance(gid);
        prev = Some(gid);
    }
    width
}

fn truncate_text_to_width(font: &FontRef<'_>, px: f32, text: &str, max_w: f32) -> String {
    if measure_text_width(font, px, text) <= max_w {
        return text.to_string();
    }
    let ellipsis = "...";
    let ell_w = measure_text_width(font, px, ellipsis);
    let budget = (max_w - ell_w).max(0.0);
    let scaled = font.as_scaled(PxScale::from(px));
    let mut out = String::new();
    let mut width = 0.0f32;
    let mut prev = None;
    for ch in text.chars() {
        let gid = scaled.glyph_id(ch);
        let kern = prev.map_or(0.0, |p| scaled.kern(p, gid));
        let adv = scaled.h_advance(gid);
        if width + kern + adv > budget {
            break;
        }
        out.push(ch);
        width += kern + adv;
        prev = Some(gid);
    }
    out.push_str(ellipsis);
    out
}

struct TextBox {
    x: f32,
    y: f32,
    max_w: f32,
    px: f32,
    color: Rgba<u8>,
}

fn draw_text(img: &mut RgbaImage, font: &FontRef<'_>, style: TextBox, text: &str) {
    let clean = text
        .replace(" \u{2014} ", ": ")
        .replace('\u{2014}', ", ")
        .replace(concat!(" ", "-", "-", " "), ": ");
    let clipped = truncate_text_to_width(font, style.px, clean.trim(), style.max_w);
    let scaled = font.as_scaled(PxScale::from(style.px));
    let baseline = style.y + scaled.ascent();
    let mut pen_x = style.x;
    let mut prev = None;

    for ch in clipped.chars() {
        let gid = scaled.glyph_id(ch);
        if let Some(p) = prev {
            pen_x += scaled.kern(p, gid);
        }
        let glyph =
            gid.with_scale_and_position(PxScale::from(style.px), ab_glyph::point(pen_x, baseline));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, cov| {
                let px_x = bounds.min.x as i32 + gx as i32;
                let px_y = bounds.min.y as i32 + gy as i32;
                blend_pixel(img, px_x, px_y, style.color, cov);
            });
        }
        pen_x += scaled.h_advance(gid);
        prev = Some(gid);
    }
}

fn proof_matches_row(proof: &AttendanceProof, row: &RowMeta) -> bool {
    if !proof.sig.is_empty() && row.sig == proof.sig {
        return true;
    }
    if !proof.key.is_empty() && row.key == proof.key {
        return true;
    }
    !proof.nama_mk.is_empty() && row.nama_mk == proof.nama_mk
}

fn pill_colors(color: Color) -> (Rgba<u8>, Rgba<u8>) {
    match color {
        Color::Green => (Rgba([22, 163, 74, 255]), Rgba([255, 255, 255, 255])),
        Color::Blue => (Rgba([37, 99, 235, 255]), Rgba([255, 255, 255, 255])),
        Color::Orange => (Rgba([217, 119, 6, 255]), Rgba([255, 255, 255, 255])),
        Color::Red => (Rgba([220, 38, 38, 255]), Rgba([255, 255, 255, 255])),
        Color::Unknown => (Rgba([100, 116, 139, 255]), Rgba([255, 255, 255, 255])),
    }
}

/// Pure-Rust PNG fallback renderer producing a crisp 960px portal schedule table image.
pub fn render_fallback_attendance_png(
    snapshot: &PortalPageSnapshot,
    proof: Option<&AttendanceProof>,
) -> Result<Vec<u8>, String> {
    let font_reg = FontRef::try_from_slice(FONT_REGULAR_BYTES)
        .map_err(|e| format!("gagal memuat font reguler: {e}"))?;
    let font_semi = FontRef::try_from_slice(FONT_SEMIBOLD_BYTES)
        .map_err(|e| format!("gagal memuat font semibold: {e}"))?;
    let font_mono = FontRef::try_from_slice(FONT_MONO_BYTES)
        .map_err(|e| format!("gagal memuat font mono: {e}"))?;

    let fallback_nim = proof.map(|p| p.nim.as_str()).unwrap_or("");
    let user = parse_user_header(&snapshot.html, fallback_nim);
    let (_, _, rows) = find_rows_with_meta(&snapshot.html, &snapshot.target_url);

    let width: u32 = 960;
    let margin: i32 = 20;
    let card_w = (width as i32) - margin * 2;
    let header_h: i32 = 74;
    let tbl_header_h: i32 = 34;
    let row_h: i32 = 40;
    let body_rows = rows.len().max(1) as i32;
    let footer_h: i32 = 42;
    let total_h = (margin * 2 + header_h + 12 + tbl_header_h + body_rows * row_h + 12 + footer_h)
        .max(CROP_MIN_HEIGHT_PX as i32) as u32;

    let bg_canvas = Rgba([248, 250, 252, 255]);
    let bg_surface = Rgba([255, 255, 255, 255]);
    let bg_subtle = Rgba([241, 245, 249, 255]);
    let border = Rgba([203, 213, 225, 255]);
    let text_primary = Rgba([15, 23, 42, 255]);
    let text_secondary = Rgba([71, 85, 105, 255]);
    let green_row_bg = Rgba([236, 253, 245, 255]);

    let mut img = RgbaImage::from_pixel(width, total_h, bg_canvas);

    // Header Card
    fill_rect(&mut img, margin, margin, card_w, header_h, bg_surface);
    stroke_rect(&mut img, margin, margin, card_w, header_h, border);
    draw_text(
        &mut img,
        &font_semi,
        TextBox {
            x: (margin + 16) as f32,
            y: (margin + 13) as f32,
            max_w: (card_w - 32) as f32,
            px: 17.0,
            color: text_primary,
        },
        "Portal Akademik Polban · Jadwal & Presensi",
    );

    let student_nim = if !user.nim.is_empty() {
        user.nim.as_str()
    } else {
        proof.map(|p| p.nim.as_str()).unwrap_or("")
    };
    let student_nama = if !user.nama.is_empty() {
        user.nama.as_str()
    } else {
        proof.map(|p| p.student_nama.as_str()).unwrap_or("")
    };
    let student_jur = if !user.jurusan.is_empty() {
        user.jurusan.as_str()
    } else {
        proof.map(|p| p.jurusan.as_str()).unwrap_or("")
    };
    let mut student_parts = Vec::new();
    if !student_nim.is_empty() {
        student_parts.push(student_nim);
    }
    if !student_nama.is_empty() {
        student_parts.push(student_nama);
    }
    if !student_jur.is_empty() {
        student_parts.push(student_jur);
    }
    let student_line = if student_parts.is_empty() {
        "Data Jadwal Kuliah Portal".to_string()
    } else {
        student_parts.join(" · ")
    };
    draw_text(
        &mut img,
        &font_reg,
        TextBox {
            x: (margin + 16) as f32,
            y: (margin + 42) as f32,
            max_w: (card_w - 32) as f32,
            px: 12.5,
            color: text_secondary,
        },
        &student_line,
    );

    // Table Container
    let tbl_y = margin + header_h + 12;
    let tbl_h = tbl_header_h + body_rows * row_h;
    fill_rect(&mut img, margin, tbl_y, card_w, tbl_h, bg_surface);
    stroke_rect(&mut img, margin, tbl_y, card_w, tbl_h, border);

    // Table Header
    fill_rect(
        &mut img,
        margin + 1,
        tbl_y + 1,
        card_w - 2,
        tbl_header_h - 1,
        bg_subtle,
    );
    fill_rect(
        &mut img,
        margin,
        tbl_y + tbl_header_h - 1,
        card_w,
        1,
        border,
    );

    // Column offsets relative to margin (total = 920px):
    // No(42) | Kode(84) | Mata Kuliah(246) | Dosen(238) | TP(38) | Jam(146) | Status(126)
    let col_x = [0, 42, 126, 372, 610, 648, 794, 920];
    let headers = ["No", "Kode", "Mata Kuliah", "Dosen", "TP", "Jam", "Status"];
    for (idx, title) in headers.iter().enumerate() {
        let cx = margin + col_x[idx] + 10;
        let cw = (col_x[idx + 1] - col_x[idx] - 16) as f32;
        draw_text(
            &mut img,
            &font_semi,
            TextBox {
                x: cx as f32,
                y: (tbl_y + 9) as f32,
                max_w: cw,
                px: 12.0,
                color: text_primary,
            },
            title,
        );
    }

    if rows.is_empty() {
        let ry = tbl_y + tbl_header_h;
        let fallback_msg = if let Some(p) = proof {
            format!(
                "{} ({}) · Status: {}",
                p.nama_mk,
                p.jam,
                p.verification_label()
            )
        } else {
            "Tidak ada baris jadwal ditemukan pada snapshot HTML portal.".to_string()
        };
        draw_text(
            &mut img,
            &font_reg,
            TextBox {
                x: (margin + 16) as f32,
                y: (ry + 12) as f32,
                max_w: (card_w - 32) as f32,
                px: 12.5,
                color: text_secondary,
            },
            &fallback_msg,
        );
    } else {
        for (i, row) in rows.iter().enumerate() {
            let ry = tbl_y + tbl_header_h + (i as i32) * row_h;
            let highlight =
                row.color == Color::Green || proof.is_some_and(|p| proof_matches_row(p, row));
            if highlight && row.color == Color::Green {
                fill_rect(&mut img, margin + 1, ry, card_w - 2, row_h, green_row_bg);
            }
            if i > 0 {
                fill_rect(&mut img, margin, ry, card_w, 1, border);
            }

            let no_str = format!("{}", i + 1);
            let jam_short = row.jam.replace(" s.d. ", "-");
            let text_y = (ry + 12) as f32;

            draw_text(
                &mut img,
                &font_mono,
                TextBox {
                    x: (margin + col_x[0] + 10) as f32,
                    y: text_y,
                    max_w: (col_x[1] - col_x[0] - 14) as f32,
                    px: 11.5,
                    color: text_secondary,
                },
                &no_str,
            );
            draw_text(
                &mut img,
                &font_mono,
                TextBox {
                    x: (margin + col_x[1] + 8) as f32,
                    y: text_y,
                    max_w: (col_x[2] - col_x[1] - 14) as f32,
                    px: 11.5,
                    color: text_secondary,
                },
                &row.mk_code,
            );
            draw_text(
                &mut img,
                &font_semi,
                TextBox {
                    x: (margin + col_x[2] + 8) as f32,
                    y: text_y,
                    max_w: (col_x[3] - col_x[2] - 14) as f32,
                    px: 12.0,
                    color: text_primary,
                },
                &row.nama_mk,
            );
            draw_text(
                &mut img,
                &font_reg,
                TextBox {
                    x: (margin + col_x[3] + 8) as f32,
                    y: text_y,
                    max_w: (col_x[4] - col_x[3] - 14) as f32,
                    px: 11.5,
                    color: text_secondary,
                },
                &row.dosen,
            );
            draw_text(
                &mut img,
                &font_mono,
                TextBox {
                    x: (margin + col_x[4] + 8) as f32,
                    y: text_y,
                    max_w: (col_x[5] - col_x[4] - 12) as f32,
                    px: 11.5,
                    color: text_secondary,
                },
                &row.tp,
            );
            draw_text(
                &mut img,
                &font_mono,
                TextBox {
                    x: (margin + col_x[5] + 8) as f32,
                    y: text_y,
                    max_w: (col_x[6] - col_x[5] - 14) as f32,
                    px: 11.0,
                    color: text_secondary,
                },
                &jam_short,
            );

            // Status pill
            let (pill_bg, pill_fg) = pill_colors(row.color);
            let pill_x = margin + col_x[6] + 8;
            let pill_y = ry + 8;
            let pill_w = col_x[7] - col_x[6] - 16;
            let pill_h = row_h - 16;
            fill_rect(&mut img, pill_x, pill_y, pill_w, pill_h, pill_bg);
            let btn_label = if !row.btn_txt.trim().is_empty() {
                row.btn_txt.trim()
            } else {
                match row.color {
                    Color::Green => "Sudah Absen",
                    Color::Blue => "Absen Awal",
                    Color::Orange => "Belum Buka",
                    Color::Red => "Belum Waktunya",
                    Color::Unknown => "Info",
                }
            };
            draw_text(
                &mut img,
                &font_semi,
                TextBox {
                    x: (pill_x + 8) as f32,
                    y: (pill_y + 5) as f32,
                    max_w: (pill_w - 16) as f32,
                    px: 11.0,
                    color: pill_fg,
                },
                btn_label,
            );
        }
    }

    // Vertical column separators inside table
    for &cx in &col_x[1..col_x.len() - 1] {
        fill_rect(&mut img, margin + cx, tbl_y, 1, tbl_h, border);
    }

    // Footer stamp bar
    let footer_y = tbl_y + tbl_h + 12;
    fill_rect(&mut img, margin, footer_y, card_w, footer_h, bg_subtle);
    stroke_rect(&mut img, margin, footer_y, card_w, footer_h, border);
    let mut stamp = format!(
        "Sumber: {} | Diambil: {}",
        snapshot.target_url.trim(),
        snapshot.captured_at.trim()
    );
    if let Some(p) = proof {
        if !p.proof_id.trim().is_empty() {
            stamp.push_str(&format!(" | ID Bukti: {}", p.proof_id.trim()));
        }
    }
    draw_text(
        &mut img,
        &font_mono,
        TextBox {
            x: (margin + 14) as f32,
            y: (footer_y + 13) as f32,
            max_w: (card_w - 28) as f32,
            px: 11.0,
            color: text_secondary,
        },
        &stamp,
    );

    encode_rgba_png(&img)
}

/// Capture an attendance screenshot PNG from an already-fetched `PortalPageSnapshot`.
/// Uses headless Chromium first and automatically falls back to pure-Rust table rendering.
pub fn capture_attendance_screenshot_png(
    snapshot: &PortalPageSnapshot,
    proof: Option<&AttendanceProof>,
) -> Result<(Vec<u8>, ScreenshotSource), String> {
    let prepared_html = prepare_attendance_section_html(snapshot, proof);
    if let Some(bin) = find_chromium_binary() {
        if let Ok(png) = capture_with_chromium(&bin, &prepared_html) {
            return Ok((png, ScreenshotSource::HeadlessBrowser));
        }
    }
    let fallback_png = render_fallback_attendance_png(snapshot, proof)?;
    Ok((fallback_png, ScreenshotSource::NativeFallback))
}

/// Cached pure-Rust sample attendance table PNG for Sample Preview mode (zero browser cost).
pub fn render_sample_attendance_png() -> &'static [u8] {
    SAMPLE_PNG_CACHE
        .get_or_init(|| {
            let sample_html = r#"<!DOCTYPE html>
<html lang="id">
<body>
  <div class="user-header">251511000 Mahasiswa Contoh D3-Teknik Informatika</div>
  <table id="jadwal">
    <tbody>
      <tr>
        <td>1</td><td>KO078N-Dosen Pengampu Contoh</td><td>25IF201</td>
        <td>Algoritma &amp; Struktur Data</td><td>T</td><td>1</td><td>3</td>
        <td>07:00:00 s.d. 09:30:00</td>
        <td><button class="btn btn-success">Sudah Absen</button></td>
      </tr>
      <tr>
        <td>2</td><td>BD011N-Dosen Basis Data</td><td>25IF202</td>
        <td>Basis Data</td><td>P</td><td>4</td><td>6</td>
        <td>09:30:00 s.d. 12:00:00</td>
        <td><button class="btn btn-primary">Absen Awal</button></td>
      </tr>
      <tr>
        <td>3</td><td>KO015N-Dosen Matematika</td><td>25IF203</td>
        <td>Matematika Diskrit</td><td>T</td><td>7</td><td>8</td>
        <td>13:00:00 s.d. 14:40:00</td>
        <td><button class="btn btn-warning">Belum Buka</button></td>
      </tr>
      <tr>
        <td>4</td><td>KO020N-Dosen Jaringan</td><td>25IF204</td>
        <td>Jaringan Komputer</td><td>P</td><td>9</td><td>10</td>
        <td>14:40:00 s.d. 16:20:00</td>
        <td><button class="btn btn-danger">Belum Waktunya</button></td>
      </tr>
    </tbody>
  </table>
</body>
</html>"#;
            let snap = PortalPageSnapshot {
                html: sample_html.to_string(),
                base_url: "https://akademik.polban.ac.id".to_string(),
                target_url: "https://akademik.polban.ac.id/ajar/absen".to_string(),
                captured_at: "2026-11-21T07:06:00+07:00".to_string(),
                has_green_row: true,
            };
            let proof = AttendanceProof {
                proof_id: "CONTOH-PREVIEW".to_string(),
                sig:
                    "Algoritma & Struktur Data|KO078N-Dosen Pengampu Contoh|07:00:00 s.d. 09:30:00"
                        .to_string(),
                key: "Algoritma & Struktur Data|KO078N-Dosen Pengampu Contoh".to_string(),
                date: "2026-11-21".to_string(),
                recorded_at: "2026-11-21T07:05:12+07:00".to_string(),
                verified_at: Some("2026-11-21T07:06:00+07:00".to_string()),
                server_date_header: None,
                verification: ProofVerification::PortalVerified,
                nim: "251511000".to_string(),
                student_nama: "Mahasiswa Contoh".to_string(),
                jurusan: "D3-Teknik Informatika".to_string(),
                nama_mk: "Algoritma & Struktur Data".to_string(),
                mk_code: "25IF201".to_string(),
                dosen: "KO078N-Dosen Pengampu Contoh".to_string(),
                jam: "07:00:00 s.d. 09:30:00".to_string(),
                tp: "T".to_string(),
                ja: "1".to_string(),
                jb: "3".to_string(),
                kls: "1A".to_string(),
                target_url: "https://akademik.polban.ac.id/ajar/absen".to_string(),
                endpoint_url: None,
                http_status: Some(200),
                server_response: None,
                server_button_label: Some("Sudah Absen".to_string()),
                attempt: Some(1),
            };
            render_fallback_attendance_png(&snap, Some(&proof)).unwrap_or_default()
        })
        .as_slice()
}

/// Atomically write PNG bytes to `path` via a sibling temporary file rename.
pub fn write_png_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| format!("gagal membuat direktori tujuan: {e}"))?;

    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("screenshot.png");
    let tmp_path = parent.join(format!(".{file_name}.tmp.{}.{seq}", std::process::id()));

    if let Err(err) = std::fs::write(&tmp_path, bytes) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("gagal menulis file PNG sementara: {err}"));
    }
    if let Err(err) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!(
            "gagal menyimpan file PNG ke {}: {err}",
            path.display()
        ));
    }
    Ok(())
}

/// Build a safe ASCII default PNG filename (`Bukti_Absen_<MK>_<date>.png` or `Halaman_Absen_<date>.png`).
pub fn default_screenshot_filename(mk: Option<&str>, date: &str) -> String {
    let safe_date: String = date
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(20)
        .collect();
    let date_part = if safe_date.is_empty() {
        "hari-ini".to_string()
    } else {
        safe_date
    };

    if let Some(raw_mk) = mk {
        let mut slug = String::new();
        let mut prev_underscore = false;
        for ch in raw_mk.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                slug.push(ch);
                prev_underscore = false;
            } else if !slug.is_empty() && !prev_underscore {
                slug.push('_');
                prev_underscore = true;
            }
            if slug.len() >= 40 {
                break;
            }
        }
        let trimmed_slug = slug.trim_matches('_');
        if !trimmed_slug.is_empty() {
            return format!("Bukti_Absen_{trimmed_slug}_{date_part}.png");
        }
    }

    format!("Halaman_Absen_{date_part}.png")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_snapshot(html: &str) -> PortalPageSnapshot {
        PortalPageSnapshot {
            html: html.to_string(),
            base_url: "https://akademik.polban.ac.id/".to_string(),
            target_url: "https://akademik.polban.ac.id/ajar/absen".to_string(),
            captured_at: "2026-11-21T07:06:00+07:00".to_string(),
            has_green_row: true,
        }
    }

    #[test]
    fn prepare_html_strips_scripts_iframes_meta_refresh_and_injects_base_and_stamp() {
        let raw = r#"<!DOCTYPE html>
<html>
<head>
  <title>Absen</title>
  <META HTTP-EQUIV="Refresh" CONTENT="0;url=/laman/login">
  <meta http-equiv='refresh' content='5;url=/login'>
  <meta charset="utf-8">
  <link rel="stylesheet" href="//fonts.googleapis.com/css?family=Source+Sans+Pro:300,400,600">
  <link rel="stylesheet" href="https://fonts.gstatic.com/s/sourcesanspro/font.css">
  <link rel="stylesheet" href="assets/dist/css/AdminLTE.min.css">
  <style>
    @import url(//fonts.googleapis.com/css?family=Source+Sans+Pro:300,400,600,700);
    @IMPORT "https://fonts.googleapis.com/css?family=Roboto";
    .box { border-radius: 3px; }
  </style>
  <SCRIPT type="text/javascript">window.location = '/laman/login';</SCRIPT>
</head>
<body>
  <aside class="main-sidebar">Menu</aside>
  <div class="content-wrapper">
    <table id="jadwal"><tr><td>Struktur Data</td></tr></table>
  </div>
  <iframe src="https://evil.example/frame"></iframe>
  <script>alert(1)</script>
</body>
</html>"#;

        let snap = sample_snapshot(raw);
        let proof = AttendanceProof {
            proof_id: "PRF-20261121-1234ABCD".to_string(),
            sig: "s".to_string(),
            key: "k".to_string(),
            date: "2026-11-21".to_string(),
            recorded_at: "2026-11-21T07:05:00+07:00".to_string(),
            verified_at: None,
            server_date_header: None,
            verification: ProofVerification::PortalVerified,
            nim: "251511001".to_string(),
            student_nama: "Budi".to_string(),
            jurusan: "TI".to_string(),
            nama_mk: "Struktur Data".to_string(),
            mk_code: "25IF201".to_string(),
            dosen: "Dosen".to_string(),
            jam: "07:00:00 s.d. 09:30:00".to_string(),
            tp: "T".to_string(),
            ja: "1".to_string(),
            jb: "3".to_string(),
            kls: "1A".to_string(),
            target_url: "https://akademik.polban.ac.id/ajar/absen".to_string(),
            endpoint_url: None,
            http_status: Some(200),
            server_response: None,
            server_button_label: Some("Sudah Absen".to_string()),
            attempt: Some(1),
        };

        let out = prepare_attendance_section_html(&snap, Some(&proof));
        let lower = out.to_ascii_lowercase();
        assert!(!lower.contains("<script"));
        assert!(!lower.contains("alert(1)"));
        assert!(!lower.contains("<iframe"));
        assert!(!lower.contains("http-equiv=\"refresh\""));
        assert!(!lower.contains("http-equiv='refresh'"));
        assert!(!lower.contains("fonts.googleapis.com"));
        assert!(!lower.contains("fonts.gstatic.com"));
        assert!(out.contains(r#"<link rel="stylesheet" href="assets/dist/css/AdminLTE.min.css">"#));
        assert!(out.contains(".box { border-radius: 3px; }"));
        assert!(out.contains("<meta charset=\"utf-8\">"));
        assert!(out.contains(r#"<base href="https://akademik.polban.ac.id/">"#));
        assert!(out.contains("absen-gui-shot-style"));
        assert!(out.contains(r#"font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif !important;"#));
        assert!(!out.contains("display: none !important"));
        assert!(!out.contains("margin-left: 0 !important"));
        assert!(out.contains("PRF-20261121-1234ABCD"));
        assert!(out.contains("https://akademik.polban.ac.id/ajar/absen"));
        assert!(!out.contains('\u{2014}'));
        assert!(!out.contains(concat!(" ", "-", "-", " ")));
    }

    #[test]
    fn fallback_renderer_and_sample_png_produce_valid_png_images() {
        let html = r#"<!DOCTYPE html>
<html><body>
  <div class="user-header">251511001 Budi Santoso D3-Teknik Informatika</div>
  <table id="jadwal">
    <tbody>
      <tr>
        <td>1</td><td>KO078N-Trisna Gelar</td><td>25IF201</td>
        <td>Struktur Data</td><td>T</td><td>1</td><td>3</td>
        <td>07:00:00 s.d. 09:30:00</td>
        <td><button class="btn btn-success">Sudah Absen</button></td>
      </tr>
      <tr>
        <td>2</td><td>BD011N-Siti Aminah</td><td>25IF202</td>
        <td>Basis Data</td><td>P</td><td>4</td><td>6</td>
        <td>09:30:00 s.d. 12:00:00</td>
        <td><button class="btn btn-primary">Absen Awal</button></td>
      </tr>
    </tbody>
  </table>
</body></html>"#;

        let png = render_fallback_attendance_png(&sample_snapshot(html), None).unwrap();
        let decoded = image::load_from_memory_with_format(&png, ImageFormat::Png).unwrap();
        assert_eq!(decoded.width(), 960);
        assert!(decoded.height() >= CROP_MIN_HEIGHT_PX);

        let sample_png = render_sample_attendance_png();
        let sample_decoded =
            image::load_from_memory_with_format(sample_png, ImageFormat::Png).unwrap();
        assert_eq!(sample_decoded.width(), 960);
        assert!(sample_decoded.height() >= CROP_MIN_HEIGHT_PX);
    }

    #[test]
    fn crop_png_bottom_whitespace_trims_blank_tail_respecting_min_height() {
        let mut img = RgbaImage::from_pixel(400, 760, Rgba([255, 255, 255, 255]));
        fill_rect(&mut img, 20, 20, 360, 300, Rgba([30, 64, 175, 255]));
        let raw = encode_rgba_png(&img).unwrap();

        let cropped_bytes = crop_png_bottom_whitespace(&raw).unwrap();
        let cropped =
            image::load_from_memory_with_format(&cropped_bytes, ImageFormat::Png).unwrap();
        assert_eq!(cropped.width(), 400);
        assert_eq!(cropped.height(), 320 + CROP_PADDING_PX);
    }

    #[test]
    fn write_png_atomic_and_default_filename_work_cleanly() {
        let guard = TempDirGuard::new("polban_shot_write_test").unwrap();
        let out_path = guard.path().join("nested").join("proof.png");
        let sample = render_sample_attendance_png();
        write_png_atomic(&out_path, sample).unwrap();
        assert_eq!(std::fs::read(&out_path).unwrap(), sample);

        assert_eq!(
            default_screenshot_filename(Some("Algoritma & Struktur Data"), "2026-11-21"),
            "Bukti_Absen_Algoritma_Struktur_Data_2026-11-21.png"
        );
        assert_eq!(
            default_screenshot_filename(None, "2026-11-21"),
            "Halaman_Absen_2026-11-21.png"
        );
        assert_eq!(
            default_screenshot_filename(Some("../../../etc/passwd"), "2026-11-21"),
            "Bukti_Absen_etc_passwd_2026-11-21.png"
        );
    }

    #[test]
    fn decode_base64_standard_handles_padding_and_whitespace() {
        assert_eq!(decode_base64_standard("SGVsbG8=").unwrap(), b"Hello");
        assert_eq!(decode_base64_standard("SGVs bG8h\n").unwrap(), b"Hello!");
        assert_eq!(decode_base64_standard("YWI=").unwrap(), b"ab");
        assert!(decode_base64_standard("abc").is_err());
    }

    #[test]
    #[ignore = "requires local Chromium/Brave binary; run explicitly with --ignored"]
    fn smoke_chromium_fixture_captures_cropped_png_without_leftovers() {
        let bin = find_chromium_binary().expect("expected installed Chromium/Brave binary");
        println!("Discovered Chromium binary: {}", bin.display());

        let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../absen-core/tests/fixtures/jadwal_mixed.html");
        let html = std::fs::read_to_string(&fixture_path).expect("read jadwal_mixed.html");
        let snap = sample_snapshot(&html);

        let before_leftovers: Vec<_> = std::fs::read_dir(std::env::temp_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("polban_absen_shot_")
            })
            .collect();

        let prepared = prepare_attendance_section_html(&snap, None);
        let direct_png = capture_with_chromium(&bin, &prepared)
            .expect("direct capture_with_chromium must succeed");
        assert!(!direct_png.is_empty());

        let (png, source) = capture_attendance_screenshot_png(&snap, None).unwrap();
        assert_eq!(source, ScreenshotSource::HeadlessBrowser);

        let decoded = image::load_from_memory_with_format(&png, ImageFormat::Png).unwrap();
        println!(
            "Captured PNG: {} bytes, {}x{}, source={:?}",
            png.len(),
            decoded.width(),
            decoded.height(),
            source
        );
        assert_eq!(decoded.width(), 1120);
        assert!(decoded.height() >= CROP_MIN_HEIGHT_PX && decoded.height() < 760);

        let after_leftovers: Vec<_> = std::fs::read_dir(std::env::temp_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("polban_absen_shot_")
            })
            .collect();
        assert_eq!(
            after_leftovers.len(),
            before_leftovers.len(),
            "temp dir cleanup left behind folders: {after_leftovers:?}"
        );
    }
}

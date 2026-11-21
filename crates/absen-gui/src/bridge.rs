//! Thread-safe bridge between `absen-core` worker thread and the `egui` UI loop.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;

use absen_core::engine::{run_with_commands, EngineError};
use absen_core::state::{
    proof_path_for, proof_screenshots_dir, prune_screenshot_files, ProofStore,
};
use absen_core::text::{nama_dosen, split_sig};
use absen_core::{
    AbsenClient, AttendanceProof, Color, Config, EngineCommand, Event, LogLevel,
    PortalPageSnapshot, ProofVerification, Reporter, UserInfo,
};
use chrono::Local;

use crate::platform::Notifier;
use crate::prefs::default_state_file_path;
use crate::screenshot::{capture_attendance_screenshot_png, write_png_atomic, ScreenshotSource};

fn local_today_str() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

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
    pub proof_key: Option<String>,
}

/// Pre-formatted attendance proof ("bukti kehadiran absen") ready for zero-cost frame rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofPreview {
    pub proof_id: String,
    pub sig: String,
    pub key: String,
    pub date: String,
    pub recorded_at: String,
    pub verification: ProofVerification,
    pub verification_label: &'static str,
    pub banner_explanation: String,
    pub inline_summary: String,
    pub nama_mk: String,
    pub course_meta_line: String,
    pub dosen: String,
    pub jam_display: String,
    pub student_nama: String,
    pub nim: String,
    pub jurusan: String,
    pub student_line: String,
    pub student_sub_line: String,
    pub recorded_time_hms: String,
    pub recorded_date_time: String,
    pub verified_time_hms: Option<String>,
    pub server_date_header: Option<String>,
    pub http_summary: String,
    pub target_url: String,
    pub endpoint_url: Option<String>,
    pub server_response: Option<String>,
    pub server_button_label: Option<String>,
    pub receipt_text: String,
}

fn format_clean_jam(raw: &str) -> String {
    fn trim_secs(t: &str) -> &str {
        let bytes = t.as_bytes();
        if bytes.len() == 8 && bytes[2] == b':' && bytes[5] == b':' {
            &t[..5]
        } else {
            t
        }
    }

    let trimmed = sanitize_text(raw.trim());
    if trimmed.is_empty() {
        return String::new();
    }
    let normalized = trimmed
        .replace(" s.d. ", " - ")
        .replace(" s/d ", " - ")
        .replace(" - ", "|");
    if let Some((start, end)) = normalized.split_once('|') {
        let s = trim_secs(start.trim());
        let e = trim_secs(end.trim());
        return format!("{s} \u{2013} {e}");
    }
    trim_secs(&trimmed).to_string()
}

impl ProofPreview {
    /// Pre-format every UI display string and plain-text receipt once on event arrival.
    pub fn from_proof(proof: AttendanceProof) -> Self {
        let verification = proof.verification;
        let verification_label = proof.verification_label();
        let recorded_time_hms = sanitize_text(&proof.short_time_hms());
        let verified_time_hms = proof.verified_time_hms().map(|s| sanitize_text(&s));

        let inline_status = match verification {
            ProofVerification::PortalVerified => "Terverifikasi portal",
            ProofVerification::PostAccepted => "Diterima server",
            ProofVerification::ObservedPresent => "Tercatat di portal",
        };
        let inline_summary = if recorded_time_hms.is_empty() {
            inline_status.to_string()
        } else {
            format!("{recorded_time_hms} · {inline_status}")
        };

        let server_button_label = proof
            .server_button_label
            .as_deref()
            .map(|s| sanitize_text(s.trim()))
            .filter(|s| !s.is_empty());

        let banner_explanation = match verification {
            ProofVerification::PortalVerified => match &server_button_label {
                Some(btn) => {
                    format!("Status tombol di portal akademik terkonfirmasi: {btn}")
                }
                None => "Kehadiran telah terkonfirmasi pada jadwal portal akademik.".to_string(),
            },
            ProofVerification::PostAccepted => {
                "Permintaan presensi diterima server. Status portal akan diverifikasi otomatis pada pemindaian berikutnya, atau klik Verifikasi Ulang.".to_string()
            }
            ProofVerification::ObservedPresent => {
                "Mata kuliah ini sudah berstatus hadir di portal akademik saat jadwal dipindai.".to_string()
            }
        };

        let nama_mk = sanitize_text(proof.nama_mk.trim());
        let mk_code = sanitize_text(proof.mk_code.trim());
        let tp = sanitize_text(proof.tp.trim());
        let ja = sanitize_text(proof.ja.trim());
        let jb = sanitize_text(proof.jb.trim());
        let kls = sanitize_text(proof.kls.trim());

        let mut meta_parts = Vec::new();
        if !mk_code.is_empty() && mk_code != nama_mk {
            meta_parts.push(format!("Kode {mk_code}"));
        }
        if !kls.is_empty() {
            meta_parts.push(format!("Kelas {kls}"));
        }
        if !tp.is_empty() {
            meta_parts.push(tp);
        }
        if !ja.is_empty() && !jb.is_empty() {
            meta_parts.push(format!("Sesi {ja}-{jb}"));
        }
        let course_meta_line = meta_parts.join(" · ");

        let dosen = sanitize_text(&nama_dosen(&proof.dosen));
        let jam_display = format_clean_jam(&proof.jam);

        let student_nama = sanitize_text(proof.student_nama.trim());
        let nim = sanitize_text(proof.nim.trim());
        let jurusan = sanitize_text(proof.jurusan.trim());
        let mut student_parts = Vec::new();
        if !student_nama.is_empty() {
            student_parts.push(student_nama.clone());
        }
        if !nim.is_empty() {
            student_parts.push(nim.clone());
        }
        if !jurusan.is_empty() {
            student_parts.push(jurusan.clone());
        }
        let student_line = student_parts.join(" · ");
        let student_sub_line = match (nim.is_empty(), jurusan.is_empty()) {
            (false, false) => format!("{nim} · {jurusan}"),
            (false, true) => nim.clone(),
            (true, false) => jurusan.clone(),
            (true, true) => String::new(),
        };

        let date = sanitize_text(proof.date.trim());
        let recorded_date_time = match (date.is_empty(), recorded_time_hms.is_empty()) {
            (false, false) => format!("{date} · {recorded_time_hms}"),
            (true, false) => recorded_time_hms.clone(),
            (false, true) => date.clone(),
            (true, true) => String::new(),
        };

        let mut http_parts = Vec::new();
        if let Some(status) = proof.http_status {
            http_parts.push(format!("HTTP {status}"));
        }
        if let Some(attempt) = proof.attempt {
            http_parts.push(format!("Percobaan {attempt}"));
        }
        if let Some(btn) = &server_button_label {
            http_parts.push(format!("Tombol: {btn}"));
        }
        let http_summary = http_parts.join(" · ");

        let server_date_header = proof
            .server_date_header
            .as_deref()
            .map(|s| sanitize_text(s.trim()))
            .filter(|s| !s.is_empty());
        let target_url = sanitize_text(proof.target_url.trim());
        let endpoint_url = proof
            .endpoint_url
            .as_deref()
            .map(|s| sanitize_text(s.trim()))
            .filter(|s| !s.is_empty());
        let server_response = proof
            .server_response
            .as_deref()
            .map(|s| sanitize_text(s.trim()))
            .filter(|s| !s.is_empty());

        let receipt_text = sanitize_text(&proof.format_receipt_text());

        Self {
            proof_id: sanitize_text(proof.proof_id.trim()),
            sig: proof.sig,
            key: proof.key,
            date,
            recorded_at: proof.recorded_at,
            verification,
            verification_label,
            banner_explanation,
            inline_summary,
            nama_mk,
            course_meta_line,
            dosen,
            jam_display,
            student_nama,
            nim,
            jurusan,
            student_line,
            student_sub_line,
            recorded_time_hms,
            recorded_date_time,
            verified_time_hms,
            server_date_header,
            http_summary,
            target_url,
            endpoint_url,
            server_response,
            server_button_label,
            receipt_text,
        }
    }
}

impl From<AttendanceProof> for ProofPreview {
    fn from(value: AttendanceProof) -> Self {
        Self::from_proof(value)
    }
}

/// Immutable snapshot read by the UI each frame without blocking the engine thread.
#[derive(Debug, Clone)]
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
    pub proofs: Vec<ProofPreview>,
    pub today_date: String,
    pub last_error: Option<String>,
    pub has_portal_snapshot: bool,
    pub portal_snapshot_captured_at: Option<String>,
    pub screenshot_revision: u64,
    pub capturing_proof_id: Option<String>,
}

impl Default for GuiSnapshot {
    fn default() -> Self {
        Self {
            status: EngineStatus::default(),
            auth_status: String::new(),
            user: None,
            cycle: 0,
            countdown_secs: 0,
            interval_secs: 0,
            last_scan: String::new(),
            courses: Vec::new(),
            activities: Vec::new(),
            proofs: Vec::new(),
            today_date: local_today_str(),
            last_error: None,
            has_portal_snapshot: false,
            portal_snapshot_captured_at: None,
            screenshot_revision: 0,
            capturing_proof_id: None,
        }
    }
}

impl GuiSnapshot {
    /// Insert or upgrade a proof in-place without downgrading verification strength.
    pub fn upsert_proof(&mut self, mut incoming: ProofPreview) {
        let existing_idx = self.proofs.iter().position(|p| {
            p.date == incoming.date
                && (p.sig == incoming.sig || (!p.key.is_empty() && p.key == incoming.key))
        });

        let Some(idx) = existing_idx else {
            self.proofs.insert(0, incoming);
            return;
        };

        let existing = &self.proofs[idx];
        if incoming.verification < existing.verification {
            return;
        }
        if incoming.verification == existing.verification
            && incoming.recorded_at < existing.recorded_at
        {
            return;
        }

        // Preserve any HTTP request evidence from the earlier stage if the upgrade omits it.
        if incoming.endpoint_url.is_none() {
            incoming.endpoint_url = existing.endpoint_url.clone();
        }
        if incoming.server_response.is_none() {
            incoming.server_response = existing.server_response.clone();
        }
        if incoming.server_date_header.is_none() {
            incoming.server_date_header = existing.server_date_header.clone();
        }
        if incoming.http_summary.is_empty() && !existing.http_summary.is_empty() {
            incoming.http_summary = existing.http_summary.clone();
        }

        self.proofs[idx] = incoming;
    }

    /// Find today's matching attendance proof for a schedule course card in O(N) without allocations.
    pub fn proof_for_course(&self, course: &CourseItem) -> Option<&ProofPreview> {
        if self.proofs.is_empty() || self.today_date.is_empty() {
            return None;
        }
        let today = self.today_date.as_str();
        let (mk, raw_dosen, sig_jam) = split_sig(&course.sig);
        let course_jam = if !sig_jam.is_empty() {
            sig_jam
        } else {
            course.jam.trim()
        };
        let jam_compatible = |p_jam: &str| {
            let pj = p_jam.trim();
            course_jam.is_empty() || pj.is_empty() || pj == course_jam
        };

        // 1. Exact signature or normalized (nama_mk, raw_dosen, jam) match for today.
        if let Some(found) = self.proofs.iter().find(|p| {
            p.date == today
                && (p.sig == course.sig
                    || (!mk.is_empty() && split_sig(&p.sig) == (mk, raw_dosen, sig_jam)))
        }) {
            return Some(found);
        }

        // 2. Key match: (nama_mk, raw_dosen) ONLY when jam does not conflict, for today.
        if !mk.is_empty() && !raw_dosen.is_empty() {
            if let Some(found) = self.proofs.iter().find(|p| {
                if p.date != today {
                    return false;
                }
                let (p_mk, p_dsn, p_jam) = split_sig(&p.sig);
                p_mk == mk && p_dsn == raw_dosen && jam_compatible(p_jam)
            }) {
                return Some(found);
            }
        }

        // 3. Fallback when course.sig lacks raw_dosen: require compatible dosen AND compatible jam for today.
        let target_mk = course.nama_mk.trim();
        let target_dosen = course.dosen.trim();
        if target_mk.is_empty() {
            return None;
        }
        self.proofs.iter().find(|p| {
            if p.date != today || p.nama_mk != target_mk {
                return false;
            }
            let (_, _, p_jam) = split_sig(&p.sig);
            let dosen_compatible = if !raw_dosen.is_empty() {
                split_sig(&p.sig).1 == raw_dosen
            } else {
                target_dosen.is_empty() || p.dosen.is_empty() || p.dosen == target_dosen
            };
            dosen_compatible && jam_compatible(p_jam)
        })
    }

    /// Find a proof by `proof_id`, `sig`, `key`, or normalized skip signature.
    pub fn proof_by_key(&self, key: &str) -> Option<&ProofPreview> {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Some(found) = self
            .proofs
            .iter()
            .find(|p| p.proof_id == trimmed || p.sig == trimmed || p.key == trimmed)
        {
            return Some(found);
        }
        let (mk, raw_dosen, jam) = split_sig(trimmed);
        if mk.is_empty() {
            return None;
        }
        self.proofs
            .iter()
            .find(|p| split_sig(&p.sig) == (mk, raw_dosen, jam))
    }
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

fn sanitize_proof_id_for_filename(proof_id: &str) -> String {
    let cleaned: String = proof_id
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(64)
        .collect();
    if cleaned.is_empty() {
        "proof".to_string()
    } else {
        cleaned
    }
}

/// Result payload delivered on-demand for live portal screenshot preview.
pub type LivePreviewResult = Result<(Vec<u8>, ScreenshotSource, String), String>;

enum CaptureJob {
    Proof {
        proof_id: String,
        snapshot: PortalPageSnapshot,
        proof: Option<Box<AttendanceProof>>,
        out_dir: PathBuf,
        valid_proof_ids: HashSet<String>,
    },
    LivePreview {
        snapshot: PortalPageSnapshot,
        reply_tx: mpsc::Sender<LivePreviewResult>,
    },
    Shutdown,
}

struct CaptureHub {
    snapshot: Arc<Mutex<GuiSnapshot>>,
    window_visible: Arc<AtomicBool>,
    latest_portal_html: Mutex<Option<PortalPageSnapshot>>,
    proof_records: Mutex<HashMap<String, AttendanceProof>>,
    screenshot_dir: Mutex<Option<PathBuf>>,
    pending_ids: Mutex<HashSet<String>>,
    wake_ui: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    tx: Mutex<Option<mpsc::Sender<CaptureJob>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl CaptureHub {
    fn new(snapshot: Arc<Mutex<GuiSnapshot>>, window_visible: Arc<AtomicBool>) -> Self {
        Self {
            snapshot,
            window_visible,
            latest_portal_html: Mutex::new(None),
            proof_records: Mutex::new(HashMap::new()),
            screenshot_dir: Mutex::new(None),
            pending_ids: Mutex::new(HashSet::new()),
            wake_ui: Mutex::new(None),
            tx: Mutex::new(None),
            worker: Mutex::new(None),
        }
    }

    fn set_wake_ui(&self, wake: Option<Arc<dyn Fn() + Send + Sync>>) {
        if let Ok(mut guard) = self.wake_ui.lock() {
            *guard = wake;
        }
    }

    fn wake(&self) {
        if !self.window_visible.load(Ordering::Relaxed) {
            return;
        }
        let cb = self.wake_ui.lock().ok().and_then(|g| g.clone());
        if let Some(wake) = cb {
            wake();
        }
    }

    fn set_state_file(&self, state_file: &Path) {
        if let Ok(mut guard) = self.screenshot_dir.lock() {
            *guard = Some(proof_screenshots_dir(state_file));
        }
    }

    fn current_screenshot_dir(&self) -> PathBuf {
        self.screenshot_dir
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_else(|| proof_screenshots_dir(&default_state_file_path()))
    }

    fn screenshot_path_for(&self, proof_id: &str) -> PathBuf {
        let safe_id = sanitize_proof_id_for_filename(proof_id);
        self.current_screenshot_dir().join(format!("{safe_id}.png"))
    }

    fn ensure_worker(self: &Arc<Self>) -> Option<mpsc::Sender<CaptureJob>> {
        let mut tx_guard = self.tx.lock().ok()?;
        let mut worker_guard = self.worker.lock().ok()?;
        let needs_spawn = worker_guard
            .as_ref()
            .is_none_or(|handle| handle.is_finished());
        if needs_spawn {
            if let Some(old) = worker_guard.take() {
                let _ = old.join();
            }
            let (tx, rx) = mpsc::channel::<CaptureJob>();
            let hub = Arc::clone(self);
            let handle = std::thread::Builder::new()
                .name("absen-proof-shot".to_string())
                .spawn(move || {
                    while let Ok(job) = rx.recv() {
                        match job {
                            CaptureJob::Shutdown => break,
                            CaptureJob::Proof {
                                proof_id,
                                snapshot,
                                proof,
                                out_dir,
                                valid_proof_ids,
                            } => {
                                if let Ok(mut snap) = hub.snapshot.lock() {
                                    snap.capturing_proof_id = Some(proof_id.clone());
                                }
                                hub.wake();

                                let result =
                                    capture_attendance_screenshot_png(&snapshot, proof.as_deref());
                                if let Ok((png_bytes, _source)) = result {
                                    let safe_id = sanitize_proof_id_for_filename(&proof_id);
                                    let out_path = out_dir.join(format!("{safe_id}.png"));
                                    if write_png_atomic(&out_path, &png_bytes).is_ok() {
                                        let _ = prune_screenshot_files(&out_dir, &valid_proof_ids);
                                    }
                                }

                                if let Ok(mut pending) = hub.pending_ids.lock() {
                                    pending.remove(&proof_id);
                                }
                                if let Ok(mut snap) = hub.snapshot.lock() {
                                    if snap.capturing_proof_id.as_deref() == Some(proof_id.as_str())
                                    {
                                        snap.capturing_proof_id = None;
                                    }
                                    snap.screenshot_revision =
                                        snap.screenshot_revision.wrapping_add(1);
                                }
                                hub.wake();
                            }
                            CaptureJob::LivePreview { snapshot, reply_tx } => {
                                let captured_at = snapshot.captured_at.clone();
                                let res = capture_attendance_screenshot_png(&snapshot, None)
                                    .map(|(bytes, src)| (bytes, src, captured_at));
                                let _ = reply_tx.send(res);
                                hub.wake();
                            }
                        }
                    }
                })
                .ok()?;
            *tx_guard = Some(tx.clone());
            *worker_guard = Some(handle);
            Some(tx)
        } else {
            tx_guard.clone()
        }
    }

    fn shutdown(&self) {
        if let Ok(mut tx_guard) = self.tx.lock() {
            if let Some(tx) = tx_guard.take() {
                let _ = tx.send(CaptureJob::Shutdown);
            }
        }
        if let Ok(mut worker_guard) = self.worker.lock() {
            if let Some(handle) = worker_guard.take() {
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(150);
                while !handle.is_finished() && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                if handle.is_finished() {
                    let _ = handle.join();
                } else {
                    let _ = std::thread::Builder::new()
                        .name("absen-shot-reaper".to_string())
                        .spawn(move || {
                            let _ = handle.join();
                        });
                }
            }
        }
    }

    fn record_proof(&self, proof: AttendanceProof) {
        let id = proof.proof_id.trim().to_string();
        if id.is_empty() {
            return;
        }
        if let Ok(mut map) = self.proof_records.lock() {
            map.insert(id, proof);
        }
    }

    fn clear_for_new_state_file(&self) {
        if let Ok(mut map) = self.proof_records.lock() {
            map.clear();
        }
        if let Ok(mut html) = self.latest_portal_html.lock() {
            *html = None;
        }
        if let Ok(mut snap) = self.snapshot.lock() {
            snap.has_portal_snapshot = false;
            snap.portal_snapshot_captured_at = None;
        }
    }

    fn valid_proof_ids(&self) -> HashSet<String> {
        self.snapshot
            .lock()
            .map(|s| s.proofs.iter().map(|p| p.proof_id.clone()).collect())
            .unwrap_or_default()
    }

    fn enqueue_proof(self: &Arc<Self>, proof_id: &str, only_if_missing: bool) {
        let trimmed = proof_id.trim();
        if trimmed.is_empty() {
            return;
        }
        let Some(portal_snap) = self.latest_portal_html.lock().ok().and_then(|g| g.clone()) else {
            return;
        };
        if only_if_missing && !portal_snap.has_green_row {
            return;
        }

        let out_dir = self.current_screenshot_dir();
        let target_path = out_dir.join(format!("{}.png", sanitize_proof_id_for_filename(trimmed)));
        if only_if_missing && target_path.is_file() {
            return;
        }

        {
            let Ok(mut pending) = self.pending_ids.lock() else {
                return;
            };
            if !pending.insert(trimmed.to_string()) {
                return;
            }
        }

        let proof = self
            .proof_records
            .lock()
            .ok()
            .and_then(|m| m.get(trimmed).cloned())
            .map(Box::new);
        let valid_proof_ids = self.valid_proof_ids();

        if let Some(tx) = self.ensure_worker() {
            let job = CaptureJob::Proof {
                proof_id: trimmed.to_string(),
                snapshot: portal_snap,
                proof,
                out_dir,
                valid_proof_ids,
            };
            if tx.send(job).is_err() {
                if let Ok(mut pending) = self.pending_ids.lock() {
                    pending.remove(trimmed);
                }
            }
        } else if let Ok(mut pending) = self.pending_ids.lock() {
            pending.remove(trimmed);
        }
    }

    fn request_live_preview(self: &Arc<Self>) -> Option<mpsc::Receiver<LivePreviewResult>> {
        let portal_snap = self
            .latest_portal_html
            .lock()
            .ok()
            .and_then(|g| g.clone())?;
        let tx = self.ensure_worker()?;
        let (reply_tx, reply_rx) = mpsc::channel();
        if tx
            .send(CaptureJob::LivePreview {
                snapshot: portal_snap,
                reply_tx,
            })
            .is_err()
        {
            return None;
        }
        Some(reply_rx)
    }
}

struct BridgeReporter {
    snapshot: Arc<Mutex<GuiSnapshot>>,
    capture: Arc<CaptureHub>,
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
                    snap.today_date = local_today_str();
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

                    let proof_key = if success && !sig.trim().is_empty() {
                        Some(sig.clone())
                    } else {
                        None
                    };
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
                            proof_key,
                        },
                    );
                }
                if success {
                    self.notify_attendance(&nama);
                }
                self.wake();
            }
            Event::PortalSnapshot(portal_snap) => {
                let captured_at = sanitize_text(&portal_snap.captured_at);
                let has_green = portal_snap.has_green_row;
                if let Ok(mut guard) = self.capture.latest_portal_html.lock() {
                    *guard = Some(portal_snap);
                }
                let mut candidate_ids = Vec::new();
                if let Ok(mut snap) = self.snapshot.lock() {
                    snap.has_portal_snapshot = true;
                    snap.portal_snapshot_captured_at = Some(captured_at);
                    if has_green {
                        let today = snap.today_date.clone();
                        for p in &snap.proofs {
                            let eligible = p.verification == ProofVerification::PortalVerified
                                || (p.verification == ProofVerification::ObservedPresent
                                    && p.date == today);
                            if eligible && !p.proof_id.is_empty() {
                                candidate_ids.push(p.proof_id.clone());
                            }
                        }
                    }
                }
                for pid in candidate_ids {
                    self.capture.enqueue_proof(&pid, true);
                }
                self.wake();
            }
            Event::AttendanceProof(proof) => {
                let proof_id = proof.proof_id.trim().to_string();
                let today = local_today_str();
                let eligible_for_shot = proof.verification == ProofVerification::PortalVerified
                    || (proof.verification == ProofVerification::ObservedPresent
                        && proof.date == today);
                self.capture.record_proof(proof.clone());

                let preview = ProofPreview::from_proof(proof);
                let proof_sig = preview.sig.clone();
                let proof_nama = preview.nama_mk.clone();
                if let Ok(mut snap) = self.snapshot.lock() {
                    snap.today_date = today;
                    snap.upsert_proof(preview);
                    // Back-link any recent ABSEN Ok row missing a proof_key for this course.
                    for act in snap.activities.iter_mut().rev() {
                        if act.code == "ABSEN"
                            && act.kind == ActivityKind::Ok
                            && act.proof_key.is_none()
                            && !proof_nama.is_empty()
                            && act.message.contains(&proof_nama)
                        {
                            act.proof_key = Some(proof_sig.clone());
                            break;
                        }
                    }
                }
                if eligible_for_shot {
                    self.capture.enqueue_proof(&proof_id, true);
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
                            proof_key: None,
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
    capture: Arc<CaptureHub>,
    notifier: Arc<Mutex<Notifier>>,
    window_visible: Arc<AtomicBool>,
    loaded_state_file: Option<PathBuf>,
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
        let snapshot = Arc::new(Mutex::new(GuiSnapshot::default()));
        let window_visible = Arc::new(AtomicBool::new(true));
        let capture = Arc::new(CaptureHub::new(
            Arc::clone(&snapshot),
            Arc::clone(&window_visible),
        ));
        Self {
            snapshot,
            capture,
            notifier: Arc::new(Mutex::new(Notifier::new(true))),
            window_visible,
            loaded_state_file: None,
            cmd_tx: None,
            worker: None,
            run_cancelled: None,
        }
    }

    /// Preload persisted 14-day attendance proofs once at startup before the engine runs.
    pub fn preload_proofs(&mut self, state_file: &Path) {
        self.capture.set_state_file(state_file);
        let proof_path = proof_path_for(state_file);
        let store = ProofStore::load(&proof_path);
        let mut snap = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        snap.today_date = local_today_str();
        // ProofStore::all() is ordered oldest-to-newest; upsert_proof prepends newest to index 0.
        for proof in store.all() {
            self.capture.record_proof(proof.clone());
            snap.upsert_proof(ProofPreview::from_proof(proof.clone()));
        }
        self.loaded_state_file = Some(state_file.to_path_buf());
    }

    /// Configure the state file used to resolve `.proofs_img/` without preloading proofs.
    pub fn set_screenshot_state_file(&mut self, state_file: &Path) {
        self.capture.set_state_file(state_file);
        self.loaded_state_file = Some(state_file.to_path_buf());
    }

    /// Register a UI repaint callback for background screenshot capture completion.
    pub fn set_wake_ui(&self, wake_ui: Option<Arc<dyn Fn() + Send + Sync>>) {
        self.capture.set_wake_ui(wake_ui);
    }

    /// Return the local cache PNG path for `proof_id`.
    pub fn screenshot_path_for(&self, proof_id: &str) -> PathBuf {
        self.capture.screenshot_path_for(proof_id)
    }

    /// Enqueue an on-demand screenshot capture/refresh for `proof_id` using the latest cached HTML.
    pub fn request_proof_screenshot(&self, proof_id: String) {
        self.capture.enqueue_proof(&proof_id, false);
    }

    /// Request an on-demand live attendance preview PNG from the latest cached `/ajar/absen` HTML.
    pub fn request_live_portal_preview(&self) -> Option<mpsc::Receiver<LivePreviewResult>> {
        self.capture.request_live_preview()
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

        let same_state_file = self.loaded_state_file.as_ref() == Some(&cfg.state_file);
        self.loaded_state_file = Some(cfg.state_file.clone());
        self.capture.set_state_file(&cfg.state_file);
        if !same_state_file {
            self.capture.clear_for_new_state_file();
        }

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
            if !same_state_file {
                snap.proofs.clear();
            }
            snap.today_date = local_today_str();
            snap.last_error = None;
        }

        let (tx, rx) = mpsc::channel();
        let snapshot = Arc::clone(&self.snapshot);
        let notifier = Arc::clone(&self.notifier);
        let window_visible = Arc::clone(&self.window_visible);
        let cancelled = Arc::new(AtomicBool::new(false));
        let run_cancelled = Arc::clone(&cancelled);
        let wake_arc: Option<Arc<dyn Fn() + Send + Sync>> = wake_ui.map(Arc::from);
        if wake_arc.is_some() {
            self.capture.set_wake_ui(wake_arc.clone());
        }

        let reporter = BridgeReporter {
            snapshot: Arc::clone(&snapshot),
            capture: Arc::clone(&self.capture),
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
        self.capture.shutdown();
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
                    proof_key: None,
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
            capture: Arc::new(CaptureHub::new(
                Arc::clone(&snapshot),
                Arc::clone(&window_visible),
            )),
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

    fn make_test_proof_for_date(
        date: &str,
        mk: &str,
        dosen: &str,
        jam: &str,
        verification: ProofVerification,
        recorded_at: &str,
    ) -> AttendanceProof {
        AttendanceProof {
            proof_id: format!("PRF-{}-{mk}", date.replace('-', "")),
            sig: format!("{mk}|{dosen}|{jam}"),
            key: format!("{mk}|{dosen}"),
            date: date.to_string(),
            recorded_at: recorded_at.to_string(),
            verified_at: if verification == ProofVerification::PortalVerified {
                Some(format!("{date}T07:06:00+07:00"))
            } else {
                None
            },
            server_date_header: Some("Sat, 21 Nov 2026 00:05:12 GMT".to_string()),
            verification,
            nim: "251511001".to_string(),
            student_nama: "Budi Santoso".to_string(),
            jurusan: "D3-Teknik Informatika".to_string(),
            nama_mk: mk.to_string(),
            mk_code: "25IF201".to_string(),
            dosen: dosen.to_string(),
            jam: jam.to_string(),
            tp: "T".to_string(),
            ja: "1".to_string(),
            jb: "3".to_string(),
            kls: "1A".to_string(),
            target_url: "https://akademik.polban.ac.id/ajar/absen".to_string(),
            endpoint_url: Some("https://akademik.polban.ac.id/ajar/absen/absensi_awal".to_string()),
            http_status: Some(200),
            server_response: Some("ok".to_string()),
            server_button_label: if verification == ProofVerification::PortalVerified {
                Some("Sudah Absen".to_string())
            } else {
                None
            },
            attempt: Some(1),
        }
    }

    fn make_test_proof(
        mk: &str,
        dosen: &str,
        jam: &str,
        verification: ProofVerification,
        recorded_at: &str,
    ) -> AttendanceProof {
        let today = local_today_str();
        make_test_proof_for_date(&today, mk, dosen, jam, verification, recorded_at)
    }

    #[test]
    fn proof_preview_preformats_and_upgrades_in_place_without_downgrade() {
        let snapshot = Arc::new(Mutex::new(GuiSnapshot::default()));
        let reporter = BridgeReporter {
            snapshot: Arc::clone(&snapshot),
            capture: Arc::new(CaptureHub::new(
                Arc::clone(&snapshot),
                Arc::new(AtomicBool::new(true)),
            )),
            notifier: Arc::new(Mutex::new(Notifier::new(false))),
            wake_ui: None,
            window_visible: Arc::new(AtomicBool::new(true)),
            cancelled: Arc::new(AtomicBool::new(false)),
            sticky_sigs: Mutex::new(HashSet::new()),
            sticky_names: Mutex::new(HashSet::new()),
        };

        let sig = "Struktur Data|KO078N-Trisna Gelar|07:00:00 s.d. 09:30:00".to_string();
        reporter.emit(Event::MarkResult {
            sig: sig.clone(),
            success: true,
            nama: "Struktur Data".to_string(),
        });
        reporter.emit(Event::AttendanceProof(make_test_proof(
            "Struktur Data",
            "KO078N-Trisna Gelar",
            "07:00:00 s.d. 09:30:00",
            ProofVerification::PostAccepted,
            "2026-11-21T07:05:12+07:00",
        )));

        {
            let snap = snapshot.lock().unwrap();
            assert_eq!(snap.proofs.len(), 1);
            assert_eq!(snap.proofs[0].inline_summary, "07:05:12 · Diterima server");
            assert_eq!(snap.proofs[0].dosen, "Trisna Gelar");
            assert_eq!(snap.proofs[0].jam_display, "07:00 \u{2013} 09:30");
            assert!(snap.proofs[0].receipt_text.contains("Diterima Server"));
            assert_eq!(snap.activities[0].proof_key.as_deref(), Some(sig.as_str()));
            assert!(snap.proof_by_key(&sig).is_some());
        }

        // Newer proof for a second course is inserted first (newest first).
        reporter.emit(Event::AttendanceProof(make_test_proof(
            "Basis Data",
            "BD011N-Budi",
            "10:00:00 s.d. 12:00:00",
            ProofVerification::ObservedPresent,
            "2026-11-21T10:01:00+07:00",
        )));
        assert_eq!(snapshot.lock().unwrap().proofs.len(), 2);
        assert_eq!(snapshot.lock().unwrap().proofs[0].nama_mk, "Basis Data");

        // Upgrade first course PostAccepted -> PortalVerified replaces in-place without duplicating.
        let mut upgraded = make_test_proof(
            "Struktur Data",
            "KO078N-Trisna Gelar",
            "07:00:00 s.d. 09:30:00",
            ProofVerification::PortalVerified,
            "2026-11-21T07:05:12+07:00",
        );
        upgraded.endpoint_url = None;
        upgraded.http_status = None;
        reporter.emit(Event::AttendanceProof(upgraded));

        // Attempted downgrade to ObservedPresent must be ignored.
        reporter.emit(Event::AttendanceProof(make_test_proof(
            "Struktur Data",
            "KO078N-Trisna Gelar",
            "07:00:00 s.d. 09:30:00",
            ProofVerification::ObservedPresent,
            "2026-11-21T07:07:00+07:00",
        )));

        let snap = snapshot.lock().unwrap();
        assert_eq!(snap.proofs.len(), 2);
        let sd = snap.proof_by_key(&sig).unwrap();
        assert_eq!(sd.verification, ProofVerification::PortalVerified);
        assert_eq!(sd.inline_summary, "07:05:12 · Terverifikasi portal");
        assert!(sd.endpoint_url.is_some());

        // Course lookup matches both Action sig and Skip sig ("Sudah Absen|...").
        let skip_course = CourseItem {
            sig: format!("Sudah Absen|{sig}"),
            nama_mk: "Struktur Data".to_string(),
            dosen: "Trisna Gelar".to_string(),
            jam: "07:00:00 s.d. 09:30:00".to_string(),
            color: Color::Green,
            status_label: "Sudah Absen".to_string(),
            reason: "Sudah absen".to_string(),
            action_ready: false,
        };
        assert_eq!(
            snap.proof_for_course(&skip_course)
                .map(|p| p.proof_id.as_str()),
            Some(sd.proof_id.as_str())
        );

        // Same MK with a different dosen or different jam must NOT falsely match sd's proof.
        let diff_dosen_course = CourseItem {
            sig: "Sudah Absen|Struktur Data|KO999N-Dosen Lain|07:00:00 s.d. 09:30:00".to_string(),
            nama_mk: "Struktur Data".to_string(),
            dosen: "Dosen Lain".to_string(),
            jam: "07:00:00 s.d. 09:30:00".to_string(),
            color: Color::Green,
            status_label: "Sudah Absen".to_string(),
            reason: "Sudah absen".to_string(),
            action_ready: false,
        };
        assert!(
            snap.proof_for_course(&diff_dosen_course).is_none(),
            "same MK with different dosen must not reuse sibling proof"
        );

        let diff_jam_course = CourseItem {
            sig: "Sudah Absen|Struktur Data|KO078N-Trisna Gelar|13:00:00 s.d. 15:30:00".to_string(),
            nama_mk: "Struktur Data".to_string(),
            dosen: "Trisna Gelar".to_string(),
            jam: "13:00:00 s.d. 15:30:00".to_string(),
            color: Color::Green,
            status_label: "Sudah Absen".to_string(),
            reason: "Sudah absen".to_string(),
            action_ready: false,
        };
        assert!(
            snap.proof_for_course(&diff_jam_course).is_none(),
            "same MK and dosen at different jam must not reuse sibling proof"
        );
    }

    #[test]
    fn proof_for_course_ignores_historical_dates_while_proof_by_key_keeps_history() {
        let mut snap = GuiSnapshot {
            today_date: "2026-11-21".to_string(),
            ..Default::default()
        };

        let sig = "Basis Data|BD011N-Budi|10:00:00 s.d. 12:00:00".to_string();
        let yesterday_proof = make_test_proof_for_date(
            "2026-11-20",
            "Basis Data",
            "BD011N-Budi",
            "10:00:00 s.d. 12:00:00",
            ProofVerification::PortalVerified,
            "2026-11-20T10:05:00+07:00",
        );
        let yesterday_id = yesterday_proof.proof_id.clone();
        snap.upsert_proof(ProofPreview::from_proof(yesterday_proof));

        let course = CourseItem {
            sig: format!("Sudah Absen|{sig}"),
            nama_mk: "Basis Data".to_string(),
            dosen: "Budi".to_string(),
            jam: "10:00:00 s.d. 12:00:00".to_string(),
            color: Color::Green,
            status_label: "Sudah Absen".to_string(),
            reason: "Sudah absen".to_string(),
            action_ready: false,
        };

        assert!(
            snap.proof_for_course(&course).is_none(),
            "historical proof from 2026-11-20 must not attach to today's schedule card"
        );
        assert_eq!(
            snap.proof_by_key(&yesterday_id).map(|p| p.date.as_str()),
            Some("2026-11-20"),
            "historical proof must still be accessible in the proof modal"
        );

        let today_proof = make_test_proof_for_date(
            "2026-11-21",
            "Basis Data",
            "BD011N-Budi",
            "10:00:00 s.d. 12:00:00",
            ProofVerification::PortalVerified,
            "2026-11-21T10:02:00+07:00",
        );
        let today_id = today_proof.proof_id.clone();
        snap.upsert_proof(ProofPreview::from_proof(today_proof));

        assert_eq!(
            snap.proof_for_course(&course).map(|p| p.proof_id.as_str()),
            Some(today_id.as_str())
        );
        assert_eq!(snap.proofs.len(), 2);
    }

    #[test]
    fn preload_proofs_loads_oldest_to_newest_from_proof_store() {
        let dir =
            std::env::temp_dir().join(format!("polban_absen_gui_preload_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let state_file = dir.join("clicked.json");
        let proof_path = proof_path_for(&state_file);

        let today = Local::now().date_naive();
        let yesterday = today - chrono::Duration::days(1);
        let today_s = today.format("%Y-%m-%d").to_string();
        let yesterday_s = yesterday.format("%Y-%m-%d").to_string();

        let mut store = ProofStore::load(&proof_path);
        store
            .upsert_and_save(make_test_proof_for_date(
                &yesterday_s,
                "Matematika Diskrit",
                "KO003N-Rina",
                "08:00:00 s.d. 09:40:00",
                ProofVerification::PortalVerified,
                &format!("{yesterday_s}T08:05:00+07:00"),
            ))
            .unwrap();
        store
            .upsert_and_save(make_test_proof_for_date(
                &today_s,
                "Struktur Data",
                "KO078N-Trisna",
                "07:00:00 s.d. 09:30:00",
                ProofVerification::PostAccepted,
                &format!("{today_s}T07:05:00+07:00"),
            ))
            .unwrap();

        let mut ctrl = EngineController::new();
        ctrl.preload_proofs(&state_file);
        let snap = ctrl.snapshot();
        assert_eq!(snap.today_date, today_s);
        assert_eq!(snap.proofs.len(), 2);
        // Upserting oldest-to-newest yields newest at index 0.
        assert_eq!(snap.proofs[0].date, today_s);
        assert_eq!(snap.proofs[0].nama_mk, "Struktur Data");
        assert_eq!(snap.proofs[1].date, yesterday_s);
        assert_eq!(snap.proofs[1].nama_mk, "Matematika Diskrit");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

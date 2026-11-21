//! Domain models for Polban Auto Absen.

use serde::{Deserialize, Serialize};

/// Button color classification matching `akademik.polban.ac.id/ajar/absen`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Color {
    Blue,
    Green,
    Orange,
    Red,
    Unknown,
}

impl Color {
    /// Human-readable dashboard status label in Indonesian.
    pub const fn status_label(self) -> &'static str {
        match self {
            Self::Blue => "SIAP ABSEN",
            Self::Green => "SUDAH ABSEN",
            Self::Orange => "BELUM BUKA",
            Self::Red => "BELUM WAKTUNYA",
            Self::Unknown => "TIDAK DIKENALI",
        }
    }
}

/// Activity log severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogLevel {
    Ok,
    Fail,
    Warn,
    Info,
    Skip,
}

impl LogLevel {
    /// Four-character bracketed tag used in plain mode and TUI log entries.
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Ok => "[OK]",
            Self::Fail => "[!!]",
            Self::Warn => "[??]",
            Self::Info => "[--]",
            Self::Skip => "[>>]",
        }
    }
}

/// Student identity extracted from `.user-header`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UserInfo {
    pub nim: String,
    pub nama: String,
    pub jurusan: String,
}

/// Form payload posted to `/ajar/absen/absensi_awal`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbsenFormData {
    pub ja: String,
    pub jb: String,
    pub mk: String,
    pub dsn: String,
    pub tp: String,
    pub kls: String,
}

impl AbsenFormData {
    /// Borrow fields as form key-value pairs without heap allocation.
    pub fn as_form_pairs(&self) -> [(&'static str, &str); 6] {
        [
            ("ja", &self.ja),
            ("jb", &self.jb),
            ("mk", &self.mk),
            ("dsn", &self.dsn),
            ("tp", &self.tp),
            ("kls", &self.kls),
        ]
    }
}

/// Clickable (blue, enabled) attendance row extracted from `#jadwal`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub sig: String,
    pub key: String,
    pub url: String,
    pub data: AbsenFormData,
    pub nama_mk: String,
    pub dosen: String,
    pub jam: String,
    pub label: String,
}

/// Non-clickable row skipped during scan (green/orange/red/unknown/disabled).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skip {
    pub color: Color,
    pub sig: String,
    pub reason: &'static str,
}

/// Structured metadata for any `#jadwal` row, preserving course/class fields without
/// breaking `Skip` literal constructions in downstream crates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowMeta {
    pub color: Color,
    pub disabled: bool,
    pub sig: String,
    pub key: String,
    pub btn_txt: String,
    pub nama_mk: String,
    pub mk_code: String,
    pub dosen: String,
    pub jam: String,
    pub tp: String,
    pub ja: String,
    pub jb: String,
    pub kls: String,
}

/// Verification strength level for an attendance proof, ordered from weakest to strongest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofVerification {
    /// Row was already `Color::Green` on `#jadwal` without a local submit record.
    ObservedPresent,
    /// 2xx POST accepted by server, awaiting portal reload confirmation.
    PostAccepted,
    /// `#jadwal` row confirmed `Color::Green` by server.
    PortalVerified,
}

/// Structured attendance proof ("bukti kehadiran absen") produced from server evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttendanceProof {
    pub proof_id: String,
    pub sig: String,
    pub key: String,
    pub date: String,
    pub recorded_at: String,
    pub verified_at: Option<String>,
    pub server_date_header: Option<String>,
    pub verification: ProofVerification,
    pub nim: String,
    pub student_nama: String,
    pub jurusan: String,
    pub nama_mk: String,
    pub mk_code: String,
    pub dosen: String,
    pub jam: String,
    pub tp: String,
    pub ja: String,
    pub jb: String,
    pub kls: String,
    pub target_url: String,
    pub endpoint_url: Option<String>,
    pub http_status: Option<u16>,
    pub server_response: Option<String>,
    pub server_button_label: Option<String>,
    pub attempt: Option<u32>,
}

/// Snapshot of the already-fetched portal schedule page emitted for screenshot capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortalPageSnapshot {
    pub html: String,
    pub base_url: String,
    pub target_url: String,
    pub captured_at: String,
    pub has_green_row: bool,
}

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;
const MAX_SERVER_RESPONSE_CHARS: usize = 240;

fn fnv1a_64_parts(parts: &[&str]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for (idx, part) in parts.iter().enumerate() {
        if idx > 0 {
            hash ^= u64::from(b'|');
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        for &b in part.as_bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

/// Generate deterministic readable proof ID `PRF-YYYYMMDD-XXXXXXXX` via FNV-1a 64-bit hash.
pub fn generate_proof_id(date: &str, key: &str, recorded_at: &str) -> String {
    let mut ymd = String::with_capacity(8);
    for ch in date.chars() {
        if ch.is_ascii_digit() {
            ymd.push(ch);
            if ymd.len() == 8 {
                break;
            }
        }
    }
    while ymd.len() < 8 {
        ymd.push('0');
    }
    let hash = fnv1a_64_parts(&[date, key, recorded_at]);
    let folded = ((hash >> 32) ^ hash) as u32;
    format!("PRF-{ymd}-{folded:08X}")
}

fn extract_hms(rfc3339: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(rfc3339) {
        return dt.format("%H:%M:%S").to_string();
    }
    if let Some((_, time_part)) = rfc3339.split_once('T') {
        let bytes = time_part.as_bytes();
        if bytes.len() >= 8 && bytes[2] == b':' && bytes[5] == b':' {
            if let Some(s) = time_part.get(..8) {
                return s.to_string();
            }
        }
    }
    rfc3339.to_string()
}

fn clean_receipt_value(raw: &str) -> String {
    // Ensure plain-text receipt never emits em dashes or " -- " separators.
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\u{2014}' => out.push('-'),
            '\r' | '\n' | '\t' => out.push(' '),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    let collapsed = crate::text::collapse_whitespace(&out);
    collapsed.replace(" -- ", " - ")
}

impl AttendanceProof {
    /// Extract `HH:MM:SS` from `recorded_at`.
    pub fn short_time_hms(&self) -> String {
        extract_hms(&self.recorded_at)
    }

    /// Extract `HH:MM:SS` from `verified_at` when present.
    pub fn verified_time_hms(&self) -> Option<String> {
        self.verified_at.as_deref().map(extract_hms)
    }

    /// Human-readable Indonesian verification label for UI/TUI/receipts.
    pub const fn verification_label(&self) -> &'static str {
        match self.verification {
            ProofVerification::PortalVerified => "Terverifikasi Portal",
            ProofVerification::PostAccepted => "Diterima Server",
            ProofVerification::ObservedPresent => "Tercatat di Portal",
        }
    }

    /// Format a clean plain-text attendance receipt suitable for clipboard and logs.
    pub fn format_receipt_text(&self) -> String {
        let mut lines = Vec::with_capacity(12);
        lines.push("BUKTI KEHADIRAN PRESENSI POLBAN".to_string());
        lines.push(format!(
            "ID Bukti      : {}",
            clean_receipt_value(&self.proof_id)
        ));
        lines.push(format!("Status        : {}", self.verification_label()));
        lines.push(format!(
            "Tanggal       : {}",
            clean_receipt_value(&self.date)
        ));

        let mut waktu = self.short_time_hms();
        if let Some(vhms) = self.verified_time_hms() {
            if vhms != waktu {
                waktu = format!("{waktu} (Verifikasi: {vhms})");
            }
        }
        lines.push(format!("Waktu Catat   : {waktu}"));

        let mk_name = clean_receipt_value(&self.nama_mk);
        let mk_code = clean_receipt_value(&self.mk_code);
        let mk_line = if mk_code.is_empty() || mk_code == mk_name {
            mk_name
        } else {
            format!("{mk_name} ({mk_code})")
        };
        lines.push(format!("Mata Kuliah   : {mk_line}"));

        let dosen_clean = clean_receipt_value(&self.dosen);
        if !dosen_clean.is_empty() {
            lines.push(format!("Dosen         : {dosen_clean}"));
        }

        let jam_clean = clean_receipt_value(&self.jam);
        let mut meta_parts = Vec::new();
        let tp_clean = clean_receipt_value(&self.tp);
        let ja_clean = clean_receipt_value(&self.ja);
        let jb_clean = clean_receipt_value(&self.jb);
        let kls_clean = clean_receipt_value(&self.kls);
        if !tp_clean.is_empty() {
            meta_parts.push(format!("TP {tp_clean}"));
        }
        if !ja_clean.is_empty() && !jb_clean.is_empty() {
            meta_parts.push(format!("JA {ja_clean}-JB {jb_clean}"));
        }
        if !kls_clean.is_empty() {
            meta_parts.push(format!("Kelas {kls_clean}"));
        }
        if !jam_clean.is_empty() || !meta_parts.is_empty() {
            let jadwal_str = if meta_parts.is_empty() {
                jam_clean
            } else if jam_clean.is_empty() {
                meta_parts.join(", ")
            } else {
                format!("{jam_clean} ({})", meta_parts.join(", "))
            };
            lines.push(format!("Jadwal        : {jadwal_str}"));
        }

        let student_nama = clean_receipt_value(&self.student_nama);
        let nim = clean_receipt_value(&self.nim);
        let jurusan = clean_receipt_value(&self.jurusan);
        if !student_nama.is_empty() || !nim.is_empty() {
            let mut mhs = match (student_nama.is_empty(), nim.is_empty()) {
                (false, false) => format!("{student_nama} ({nim})"),
                (false, true) => student_nama,
                (true, false) => nim,
                (true, true) => String::new(),
            };
            if !jurusan.is_empty() {
                mhs.push_str(&format!(" - {jurusan}"));
            }
            lines.push(format!("Mahasiswa     : {mhs}"));
        }

        let target_clean = clean_receipt_value(&self.target_url);
        if !target_clean.is_empty() {
            lines.push(format!("URL Portal    : {target_clean}"));
        }

        if let Some(status) = self.http_status {
            let http_line = match self.attempt {
                Some(att) => format!("HTTP {status} (percobaan {att})"),
                None => format!("HTTP {status}"),
            };
            lines.push(format!("Respon HTTP   : {http_line}"));
        }
        if let Some(btn) = &self.server_button_label {
            let btn_clean = clean_receipt_value(btn);
            if !btn_clean.is_empty() {
                lines.push(format!("Tombol Portal : {btn_clean}"));
            }
        }
        if let Some(resp) = &self.server_response {
            let resp_clean = clean_receipt_value(resp);
            if !resp_clean.is_empty() {
                lines.push(format!("Pesan Server  : {resp_clean}"));
            }
        }
        if let Some(sdate) = &self.server_date_header {
            let sdate_clean = clean_receipt_value(sdate);
            if !sdate_clean.is_empty() {
                lines.push(format!("Waktu Server  : {sdate_clean}"));
            }
        }

        lines.join("\n")
    }
}

/// Strip HTML tags and control characters, collapse whitespace, and truncate safely
/// on a UTF-8 character boundary to `<= 240` characters.
pub fn sanitize_server_response(raw: &str) -> Option<String> {
    let mut stripped = String::with_capacity(raw.len().min(512));
    let mut rest = raw;
    while !rest.is_empty() {
        if let Some(lt_idx) = rest.find('<') {
            stripped.push_str(&rest[..lt_idx]);
            let after_lt = &rest[lt_idx + 1..];
            if let Some(gt_idx) = after_lt.find('>') {
                stripped.push(' ');
                rest = &after_lt[gt_idx + 1..];
            } else {
                // Unclosed '<': keep literal text rather than dropping trailing data.
                stripped.push('<');
                rest = after_lt;
            }
        } else {
            stripped.push_str(rest);
            break;
        }
    }

    let mut clean_chars = String::with_capacity(stripped.len());
    for ch in stripped.chars() {
        if ch.is_control() {
            clean_chars.push(' ');
        } else {
            clean_chars.push(ch);
        }
    }

    let collapsed = crate::text::collapse_whitespace(&clean_chars);
    if collapsed.is_empty() {
        return None;
    }

    if collapsed.chars().count() <= MAX_SERVER_RESPONSE_CHARS {
        return Some(collapsed);
    }

    let cut_byte = collapsed
        .char_indices()
        .nth(MAX_SERVER_RESPONSE_CHARS - 3)
        .map(|(idx, _)| idx)
        .unwrap_or(collapsed.len());
    let mut truncated = collapsed[..cut_byte].trim_end().to_string();
    truncated.push_str("...");
    Some(truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_proof() -> AttendanceProof {
        AttendanceProof {
            proof_id: "PRF-20261121-A1B2C3D4".to_string(),
            sig: "Struktur Data|KO078N-Trisna Gelar|07:00:00 s.d. 09:30:00".to_string(),
            key: "Struktur Data|KO078N-Trisna Gelar".to_string(),
            date: "2026-11-21".to_string(),
            recorded_at: "2026-11-21T07:05:12+07:00".to_string(),
            verified_at: Some("2026-11-21T07:06:00+07:00".to_string()),
            server_date_header: Some("Sat, 21 Nov 2026 00:05:12 GMT".to_string()),
            verification: ProofVerification::PortalVerified,
            nim: "251511001".to_string(),
            student_nama: "Budi Santoso".to_string(),
            jurusan: "D3-Teknik Informatika".to_string(),
            nama_mk: "Struktur Data — Lanjut".to_string(),
            mk_code: "25IF201".to_string(),
            dosen: "KO078N-Trisna Gelar -- Tim".to_string(),
            jam: "07:00:00 s.d. 09:30:00".to_string(),
            tp: "T".to_string(),
            ja: "1".to_string(),
            jb: "3".to_string(),
            kls: "1A".to_string(),
            target_url: "https://akademik.polban.ac.id/ajar/absen".to_string(),
            endpoint_url: Some("https://akademik.polban.ac.id/ajar/absen/absensi_awal".to_string()),
            http_status: Some(200),
            server_response: Some(r#"{"status":"ok"}"#.to_string()),
            server_button_label: Some("Sudah Absen".to_string()),
            attempt: Some(1),
        }
    }

    #[test]
    fn sanitize_server_response_strips_html_and_truncates_utf8() {
        assert_eq!(sanitize_server_response("   \n\t "), None);
        assert_eq!(sanitize_server_response("<div><span>  </span></div>"), None);
        assert_eq!(
            sanitize_server_response("<div class=\"alert\">Absen\n\t<b>Berhasil</b>!</div>"),
            Some("Absen Berhasil !".to_string())
        );
        assert_eq!(
            sanitize_server_response("Nilai < 100 tetap aman"),
            Some("Nilai < 100 tetap aman".to_string())
        );

        let long_unicode = "é".repeat(260);
        let sanitized = sanitize_server_response(&long_unicode).unwrap();
        assert!(sanitized.chars().count() <= 240);
        assert!(sanitized.ends_with("..."));
    }

    #[test]
    fn proof_id_and_time_helpers_are_deterministic() {
        let id1 = generate_proof_id(
            "2026-11-21",
            "Struktur Data|KO078N",
            "2026-11-21T07:05:12+07:00",
        );
        let id2 = generate_proof_id(
            "2026-11-21",
            "Struktur Data|KO078N",
            "2026-11-21T07:05:12+07:00",
        );
        assert_eq!(id1, id2);
        assert!(id1.starts_with("PRF-20261121-"));
        assert_eq!(id1.len(), "PRF-20261121-XXXXXXXX".len());

        let proof = sample_proof();
        assert_eq!(proof.short_time_hms(), "07:05:12");
        assert_eq!(proof.verified_time_hms().as_deref(), Some("07:06:00"));
        assert_eq!(proof.verification_label(), "Terverifikasi Portal");
    }

    #[test]
    fn format_receipt_text_has_no_em_dash_or_double_dash() {
        let proof = sample_proof();
        let receipt = proof.format_receipt_text();
        assert!(!receipt.contains('\u{2014}'));
        assert!(!receipt.contains(" -- "));
        assert!(receipt.contains("PRF-20261121-A1B2C3D4"));
        assert!(receipt.contains("Terverifikasi Portal"));
        assert!(receipt.contains("Struktur Data - Lanjut (25IF201)"));
        assert!(receipt.contains("Sudah Absen"));
        assert!(receipt.contains("HTTP 200 (percobaan 1)"));
    }

    #[test]
    fn verification_strength_ordering_is_monotonic() {
        assert!(ProofVerification::ObservedPresent < ProofVerification::PostAccepted);
        assert!(ProofVerification::PostAccepted < ProofVerification::PortalVerified);
    }
}

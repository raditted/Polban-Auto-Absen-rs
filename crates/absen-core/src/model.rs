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

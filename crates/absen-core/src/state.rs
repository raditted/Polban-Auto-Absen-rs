//! Daily anti-duplicate state persisted in `.clicked.json`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StateError {
    #[error("gagal menyimpan state ke {path}: {source}")]
    Save {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("gagal serialisasi state: {0}")]
    Serialize(#[from] serde_json::Error),
}

#[derive(Debug, Serialize, Deserialize)]
struct StateFileJson {
    #[serde(rename = "_d")]
    date: String,
    #[serde(rename = "s", default)]
    sigs: Vec<String>,
}

/// Persisted set of signatures already clicked today (`{"_d":"YYYY-MM-DD","s":[...]}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickedState {
    path: PathBuf,
    date: String,
    sigs: BTreeSet<String>,
}

impl ClickedState {
    /// Load `.clicked.json` for today's local date, resetting automatically on day change
    /// or missing/corrupt files.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        Self::load_for_date(path, Local::now().date_naive())
    }

    /// Deterministic loader for a given date (used by unit tests).
    pub fn load_for_date(path: impl Into<PathBuf>, today: NaiveDate) -> Self {
        let path = path.into();
        let today_str = format_date(today);
        let mut sigs = BTreeSet::new();

        if let Ok(raw) = std::fs::read_to_string(&path) {
            let clean = raw.strip_prefix('\u{feff}').unwrap_or(&raw);
            if let Ok(parsed) = serde_json::from_str::<StateFileJson>(clean) {
                if parsed.date == today_str {
                    sigs.extend(parsed.sigs);
                }
            }
        }

        Self {
            path,
            date: today_str,
            sigs,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn date(&self) -> &str {
        &self.date
    }

    pub fn contains(&self, sig: &str) -> bool {
        self.sigs.contains(sig)
    }

    pub fn len(&self) -> usize {
        self.sigs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sigs.is_empty()
    }

    /// Insert a signature and persist `.clicked.json`. Resets the set if local date changed.
    pub fn insert_and_save(&mut self, sig: impl Into<String>) -> Result<(), StateError> {
        self.insert_and_save_for_date(sig, Local::now().date_naive())
    }

    /// Deterministic variant of `insert_and_save` for a given date.
    pub fn insert_and_save_for_date(
        &mut self,
        sig: impl Into<String>,
        today: NaiveDate,
    ) -> Result<(), StateError> {
        let today_str = format_date(today);
        if self.date != today_str {
            self.sigs.clear();
            self.date = today_str;
        }
        self.sigs.insert(sig.into());
        self.save()
    }

    /// Write current state to disk atomically in Python-compatible sorted JSON format.
    pub fn save(&self) -> Result<(), StateError> {
        let payload = StateFileJson {
            date: self.date.clone(),
            sigs: self.sigs.iter().cloned().collect(),
        };
        let json = serde_json::to_string(&payload)?;
        if let Some(parent) = self.path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|source| StateError::Save {
                path: self.path.clone(),
                source,
            })?;
        }

        let tmp_name = format!(
            ".{}.tmp.{}",
            self.path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("clicked"),
            std::process::id()
        );
        let tmp_path = self.path.with_file_name(tmp_name);
        if std::fs::write(&tmp_path, &json).is_ok() {
            if std::fs::rename(&tmp_path, &self.path).is_ok() {
                return Ok(());
            }
            let _ = std::fs::remove_file(&tmp_path);
        }

        std::fs::write(&self.path, json).map_err(|source| StateError::Save {
            path: self.path.clone(),
            source,
        })
    }
}

fn format_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_persists_sorted_and_resets_on_new_day() {
        let dir =
            std::env::temp_dir().join(format!("polban_absen_state_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".clicked.json");

        let day1 = NaiveDate::from_ymd_opt(2026, 11, 17).unwrap();
        let day2 = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();

        let mut st = ClickedState::load_for_date(&path, day1);
        assert!(st.is_empty());
        st.insert_and_save_for_date("Matkul B|Dosen|09:00", day1)
            .unwrap();
        st.insert_and_save_for_date("Matkul A|Dosen|07:00", day1)
            .unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            raw,
            r#"{"_d":"2026-11-17","s":["Matkul A|Dosen|07:00","Matkul B|Dosen|09:00"]}"#
        );

        let reloaded_same_day = ClickedState::load_for_date(&path, day1);
        assert!(reloaded_same_day.contains("Matkul A|Dosen|07:00"));
        assert!(reloaded_same_day.contains("Matkul B|Dosen|09:00"));
        assert_eq!(reloaded_same_day.len(), 2);

        let reloaded_next_day = ClickedState::load_for_date(&path, day2);
        assert!(reloaded_next_day.is_empty());
        assert_eq!(reloaded_next_day.date(), "2026-11-18");

        // Also test rollover during insert_and_save_for_date
        st.insert_and_save_for_date("Matkul C|Dosen|10:00", day2)
            .unwrap();
        assert_eq!(st.len(), 1);
        assert!(st.contains("Matkul C|Dosen|10:00"));
        assert!(!st.contains("Matkul A|Dosen|07:00"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_state_file_falls_back_to_empty() {
        let dir =
            std::env::temp_dir().join(format!("polban_absen_corrupt_state_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".clicked.json");
        std::fs::write(&path, "{not valid json").unwrap();

        let day = NaiveDate::from_ymd_opt(2026, 11, 17).unwrap();
        let st = ClickedState::load_for_date(&path, day);
        assert!(st.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn loads_utf8_bom_and_creates_missing_parent_dir_on_save() {
        let dir =
            std::env::temp_dir().join(format!("polban_absen_bom_state_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let nested_path = dir.join("nested").join("dir").join(".clicked.json");
        let day = NaiveDate::from_ymd_opt(2026, 11, 17).unwrap();

        let mut st = ClickedState::load_for_date(&nested_path, day);
        st.insert_and_save_for_date("Pemrograman — Lanjut|Dr. Éka|07:00-09:30", day)
            .unwrap();
        assert!(nested_path.is_file());

        // Prepend UTF-8 BOM and verify reload succeeds
        let content = std::fs::read_to_string(&nested_path).unwrap();
        std::fs::write(&nested_path, format!("\u{feff}{content}")).unwrap();
        let reloaded = ClickedState::load_for_date(&nested_path, day);
        assert!(reloaded.contains("Pemrograman — Lanjut|Dr. Éka|07:00-09:30"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

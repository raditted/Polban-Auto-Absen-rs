//! Daily anti-duplicate state persisted in `.clicked.json`.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::{Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::{AttendanceProof, ProofVerification};

pub const MAX_PROOF_DAYS: i64 = 14;
pub const MAX_PROOF_RECORDS: usize = 100;
const PROOF_SCHEMA_VERSION: u32 = 1;

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
        atomic_write_json(&self.path, &json)
    }
}

fn atomic_write_json(path: &Path, json: &str) -> Result<(), StateError> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|source| StateError::Save {
            path: path.to_path_buf(),
            source,
        })?;
    }

    let tmp_name = format!(
        ".{}.tmp.{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("state"),
        std::process::id()
    );
    let tmp_path = path.with_file_name(tmp_name);
    if std::fs::write(&tmp_path, json).is_ok() {
        if std::fs::rename(&tmp_path, path).is_ok() {
            return Ok(());
        }
        let _ = std::fs::remove_file(&tmp_path);
    }

    std::fs::write(path, json).map_err(|source| StateError::Save {
        path: path.to_path_buf(),
        source,
    })
}

/// Derive sibling `.proofs.json` path from the configured `.clicked.json` `state_file`.
pub fn proof_path_for(state_file: &Path) -> PathBuf {
    if state_file.file_name().is_some() {
        state_file.with_file_name(".proofs.json")
    } else {
        PathBuf::from(".proofs.json")
    }
}

/// Derive sibling `.proofs_img` directory path from the configured `.clicked.json` `state_file`.
pub fn proof_screenshots_dir(state_file: &Path) -> PathBuf {
    if state_file.file_name().is_some() {
        state_file.with_file_name(".proofs_img")
    } else {
        PathBuf::from(".proofs_img")
    }
}

/// Delete stale `.png` files in `dir` whose file stem is not present in `valid_proof_ids`.
///
/// Missing directories, subdirectories, and non-PNG files are safely ignored.
pub fn prune_screenshot_files(dir: &Path, valid_proof_ids: &HashSet<String>) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut deleted = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        let is_file = entry
            .file_type()
            .map(|ft| ft.is_file())
            .unwrap_or_else(|_| path.is_file());
        if !is_file {
            continue;
        }
        let is_png = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("png"));
        if !is_png {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if !valid_proof_ids.contains(stem) && std::fs::remove_file(&path).is_ok() {
            deleted += 1;
        }
    }
    deleted
}

fn default_proof_schema_version() -> u32 {
    PROOF_SCHEMA_VERSION
}

#[derive(Debug, Serialize, Deserialize)]
struct ProofFileJson {
    #[serde(default = "default_proof_schema_version")]
    version: u32,
    #[serde(default)]
    proofs: Vec<AttendanceProof>,
}

/// Bounded, atomic attendance proof store persisted in `.proofs.json`.
///
/// Retains at most `MAX_PROOF_DAYS` (14) days and `MAX_PROOF_RECORDS` (100) records.
/// Only writes to disk when a proof is inserted or upgraded (`PostAccepted -> PortalVerified`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofStore {
    path: PathBuf,
    proofs: Vec<AttendanceProof>,
    by_sig: HashMap<(String, String), usize>,
    by_key: HashMap<(String, String), usize>,
}

impl ProofStore {
    /// Load `.proofs.json`, pruning records older than `MAX_PROOF_DAYS` relative to today.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        Self::load_for_date(path, Local::now().date_naive())
    }

    /// Deterministic loader for a given date (used by unit tests).
    pub fn load_for_date(path: impl Into<PathBuf>, today: NaiveDate) -> Self {
        let path = path.into();
        let mut proofs = Vec::new();

        if let Ok(raw) = std::fs::read_to_string(&path) {
            let clean = raw.strip_prefix('\u{feff}').unwrap_or(&raw);
            if let Ok(parsed) = serde_json::from_str::<ProofFileJson>(clean) {
                proofs = parsed.proofs;
            }
        }

        let mut store = Self {
            path,
            proofs,
            by_sig: HashMap::new(),
            by_key: HashMap::new(),
        };
        store.prune_and_reindex(today);
        store
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn len(&self) -> usize {
        self.proofs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.proofs.is_empty()
    }

    /// All retained proofs ordered oldest-to-newest (`date`, `recorded_at`, `proof_id`).
    pub fn all(&self) -> &[AttendanceProof] {
        &self.proofs
    }

    pub fn today_proofs(&self) -> Vec<&AttendanceProof> {
        let today_str = format_date(Local::now().date_naive());
        self.proofs_for_date(&today_str)
    }

    pub fn proofs_for_date(&self, date: &str) -> Vec<&AttendanceProof> {
        self.proofs.iter().filter(|p| p.date == date).collect()
    }

    pub fn get_by_sig(&self, date: &str, sig: &str) -> Option<&AttendanceProof> {
        let idx = *self.by_sig.get(&(date.to_string(), sig.to_string()))?;
        self.proofs.get(idx)
    }

    pub fn get_by_key(&self, date: &str, key: &str) -> Option<&AttendanceProof> {
        let idx = *self.by_key.get(&(date.to_string(), key.to_string()))?;
        self.proofs.get(idx)
    }

    /// Look up by `(date, sig)` first, falling back to `(date, key)` when needed.
    pub fn find(&self, date: &str, sig: &str, key: &str) -> Option<&AttendanceProof> {
        if !sig.is_empty() {
            if let Some(p) = self.get_by_sig(date, sig) {
                return Some(p);
            }
        }
        if !key.is_empty() {
            return self.get_by_key(date, key);
        }
        None
    }

    /// Insert or upgrade a proof (`PostAccepted -> PortalVerified`) and persist atomically.
    ///
    /// Returns `Ok(true)` if disk was updated, or `Ok(false)` when skipped (e.g. `ObservedPresent`,
    /// existing stronger proof, or duplicate at the same verification level).
    pub fn upsert_and_save(&mut self, proof: AttendanceProof) -> Result<bool, StateError> {
        self.upsert_and_save_for_date(proof, Local::now().date_naive())
    }

    /// Deterministic variant of `upsert_and_save` for a given reference date.
    pub fn upsert_and_save_for_date(
        &mut self,
        proof: AttendanceProof,
        today: NaiveDate,
    ) -> Result<bool, StateError> {
        if proof.verification == ProofVerification::ObservedPresent {
            return Ok(false);
        }

        if let Some(&idx) = self.by_sig.get(&(proof.date.clone(), proof.sig.clone())) {
            let existing = &mut self.proofs[idx];
            if existing.verification >= proof.verification {
                return Ok(false);
            }
            merge_upgraded_proof(existing, proof);
        } else {
            self.proofs.push(proof);
        }

        self.prune_and_reindex(today);
        self.save()?;
        Ok(true)
    }

    fn save(&self) -> Result<(), StateError> {
        let payload = ProofFileJson {
            version: PROOF_SCHEMA_VERSION,
            proofs: self.proofs.clone(),
        };
        let json = serde_json::to_string(&payload)?;
        atomic_write_json(&self.path, &json)
    }

    fn prune_and_reindex(&mut self, today: NaiveDate) {
        let oldest_allowed = today - Duration::days(MAX_PROOF_DAYS - 1);
        let newest_allowed = today + Duration::days(1);

        // Deduplicate by (date, sig), keeping strongest verification (and latest recorded_at on tie).
        let mut dedup: HashMap<(String, String), AttendanceProof> = HashMap::new();
        for item in self.proofs.drain(..) {
            if item.verification == ProofVerification::ObservedPresent {
                continue;
            }
            let Ok(parsed_date) = NaiveDate::parse_from_str(&item.date, "%Y-%m-%d") else {
                continue;
            };
            if parsed_date < oldest_allowed || parsed_date > newest_allowed {
                continue;
            }
            let k = (item.date.clone(), item.sig.clone());
            match dedup.get_mut(&k) {
                None => {
                    dedup.insert(k, item);
                }
                Some(existing) => {
                    if item.verification > existing.verification {
                        merge_upgraded_proof(existing, item);
                    } else if item.verification == existing.verification
                        && item.recorded_at > existing.recorded_at
                    {
                        *existing = item;
                    }
                }
            }
        }

        let mut kept: Vec<AttendanceProof> = dedup.into_values().collect();
        kept.sort_by(|a, b| {
            a.date
                .cmp(&b.date)
                .then_with(|| a.recorded_at.cmp(&b.recorded_at))
                .then_with(|| a.proof_id.cmp(&b.proof_id))
        });
        if kept.len() > MAX_PROOF_RECORDS {
            let excess = kept.len() - MAX_PROOF_RECORDS;
            kept.drain(..excess);
        }
        self.proofs = kept;

        self.by_sig.clear();
        self.by_key.clear();
        for (idx, p) in self.proofs.iter().enumerate() {
            self.by_sig.insert((p.date.clone(), p.sig.clone()), idx);
            self.by_key
                .entry((p.date.clone(), p.key.clone()))
                .and_modify(|prev_idx| {
                    if self.proofs[*prev_idx].verification <= p.verification {
                        *prev_idx = idx;
                    }
                })
                .or_insert(idx);
        }
    }
}

fn fill_if_empty(target: &mut String, fallback: String) {
    if target.is_empty() && !fallback.is_empty() {
        *target = fallback;
    }
}

fn merge_upgraded_proof(existing: &mut AttendanceProof, incoming: AttendanceProof) {
    existing.verification = incoming.verification;
    if incoming.verified_at.is_some() {
        existing.verified_at = incoming.verified_at;
    }
    if incoming.server_button_label.is_some() {
        existing.server_button_label = incoming.server_button_label;
    }
    if existing.server_date_header.is_none() {
        existing.server_date_header = incoming.server_date_header;
    }
    if existing.http_status.is_none() {
        existing.http_status = incoming.http_status;
    }
    if existing.server_response.is_none() {
        existing.server_response = incoming.server_response;
    }
    if existing.endpoint_url.is_none() {
        existing.endpoint_url = incoming.endpoint_url;
    }
    if existing.attempt.is_none() {
        existing.attempt = incoming.attempt;
    }
    fill_if_empty(&mut existing.nim, incoming.nim);
    fill_if_empty(&mut existing.student_nama, incoming.student_nama);
    fill_if_empty(&mut existing.jurusan, incoming.jurusan);
    fill_if_empty(&mut existing.mk_code, incoming.mk_code);
    fill_if_empty(&mut existing.tp, incoming.tp);
    fill_if_empty(&mut existing.ja, incoming.ja);
    fill_if_empty(&mut existing.jb, incoming.jb);
    fill_if_empty(&mut existing.kls, incoming.kls);
    fill_if_empty(&mut existing.target_url, incoming.target_url);
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

    fn mk_proof(date: NaiveDate, idx: usize, verification: ProofVerification) -> AttendanceProof {
        let date_str = format_date(date);
        let recorded_at = format!("{date_str}T07:{:02}:00+07:00", idx % 60);
        let key = format!("Matkul {idx}|KO0{idx}N");
        let sig = format!("{key}|07:00-09:30");
        AttendanceProof {
            proof_id: crate::model::generate_proof_id(&date_str, &key, &recorded_at),
            sig,
            key,
            date: date_str,
            recorded_at: recorded_at.clone(),
            verified_at: (verification == ProofVerification::PortalVerified).then_some(recorded_at),
            server_date_header: Some("Sat, 21 Nov 2026 00:00:00 GMT".to_string()),
            verification,
            nim: "251511001".to_string(),
            student_nama: "Budi".to_string(),
            jurusan: "D3-TI".to_string(),
            nama_mk: format!("Matkul {idx}"),
            mk_code: format!("MK{idx}"),
            dosen: format!("KO0{idx}N"),
            jam: "07:00-09:30".to_string(),
            tp: "T".to_string(),
            ja: "1".to_string(),
            jb: "3".to_string(),
            kls: "1A".to_string(),
            target_url: "http://127.0.0.1/ajar/absen".to_string(),
            endpoint_url: Some("http://127.0.0.1/ajar/absen/absensi_awal".to_string()),
            http_status: Some(200),
            server_response: Some("ok".to_string()),
            server_button_label: (verification == ProofVerification::PortalVerified)
                .then(|| "Sudah Absen".to_string()),
            attempt: Some(1),
        }
    }

    #[test]
    fn proof_path_for_derives_sibling_file() {
        assert_eq!(
            proof_path_for(Path::new("a/b/.clicked.json")),
            PathBuf::from("a/b/.proofs.json")
        );
        assert_eq!(
            proof_path_for(Path::new(".clicked.json")),
            PathBuf::from(".proofs.json")
        );
    }

    #[test]
    fn proof_screenshots_dir_derives_sibling_dir() {
        assert_eq!(
            proof_screenshots_dir(Path::new("a/b/.clicked.json")),
            PathBuf::from("a/b/.proofs_img")
        );
        assert_eq!(
            proof_screenshots_dir(Path::new(".clicked.json")),
            PathBuf::from(".proofs_img")
        );
        assert_eq!(
            proof_screenshots_dir(Path::new("")),
            PathBuf::from(".proofs_img")
        );
    }

    #[test]
    fn prune_screenshot_files_deletes_only_stale_pngs_and_ignores_missing_dir() {
        let dir = std::env::temp_dir().join(format!(
            "polban_absen_prune_screenshots_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);

        let valid: HashSet<String> = ["PRF-20261121-KEEP0001".to_string()].into_iter().collect();

        // Missing directory safely returns 0 without error.
        assert_eq!(prune_screenshot_files(&dir, &valid), 0);

        std::fs::create_dir_all(dir.join("nested.png")).unwrap();
        std::fs::write(dir.join("PRF-20261121-KEEP0001.png"), b"png-keep").unwrap();
        std::fs::write(dir.join("PRF-20261120-STALE001.png"), b"png-stale-1").unwrap();
        std::fs::write(dir.join("PRF-20261119-STALE002.PNG"), b"png-stale-2").unwrap();
        std::fs::write(dir.join("notes.txt"), b"ignore-me").unwrap();
        std::fs::write(dir.join(".keep.json"), b"{}").unwrap();

        let deleted = prune_screenshot_files(&dir, &valid);
        assert_eq!(deleted, 2);
        assert!(dir.join("PRF-20261121-KEEP0001.png").is_file());
        assert!(!dir.join("PRF-20261120-STALE001.png").exists());
        assert!(!dir.join("PRF-20261119-STALE002.PNG").exists());
        assert!(dir.join("notes.txt").is_file());
        assert!(dir.join(".keep.json").is_file());
        assert!(dir.join("nested.png").is_dir());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn proof_store_upgrade_no_downgrade_and_skips_observed_present() {
        let dir =
            std::env::temp_dir().join(format!("polban_absen_proof_upgrade_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join(".proofs.json");
        let day = NaiveDate::from_ymd_opt(2026, 11, 21).unwrap();
        let day_str = format_date(day);

        let mut store = ProofStore::load_for_date(&path, day);
        assert!(store.is_empty());

        // ObservedPresent must NEVER write to disk.
        let obs = mk_proof(day, 9, ProofVerification::ObservedPresent);
        assert!(!store.upsert_and_save_for_date(obs, day).unwrap());
        assert!(!path.exists());

        // Insert PostAccepted -> writes disk.
        let p1 = mk_proof(day, 1, ProofVerification::PostAccepted);
        let original_id = p1.proof_id.clone();
        assert!(store.upsert_and_save_for_date(p1.clone(), day).unwrap());
        assert!(path.is_file());
        assert_eq!(store.len(), 1);

        // Duplicate PostAccepted -> no-op (no disk write).
        assert!(!store.upsert_and_save_for_date(p1, day).unwrap());

        // Upgrade PostAccepted -> PortalVerified preserves proof_id/http_status/response.
        let mut p1_verified = mk_proof(day, 1, ProofVerification::PortalVerified);
        p1_verified.proof_id = "PRF-SHOULD-NOT-REPLACE".to_string();
        p1_verified.http_status = None;
        p1_verified.server_response = None;
        assert!(store.upsert_and_save_for_date(p1_verified, day).unwrap());

        let stored = store
            .find(&day_str, "Matkul 1|KO01N|07:00-09:30", "Matkul 1|KO01N")
            .unwrap();
        assert_eq!(stored.verification, ProofVerification::PortalVerified);
        assert_eq!(stored.proof_id, original_id);
        assert_eq!(stored.http_status, Some(200));
        assert_eq!(stored.server_response.as_deref(), Some("ok"));
        assert_eq!(stored.server_button_label.as_deref(), Some("Sudah Absen"));

        // Attempt downgrade PortalVerified -> PostAccepted is ignored.
        let p1_downgrade = mk_proof(day, 1, ProofVerification::PostAccepted);
        assert!(!store.upsert_and_save_for_date(p1_downgrade, day).unwrap());

        // UTF-8 BOM tolerance on reload.
        let content = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, format!("\u{feff}{content}")).unwrap();
        let reloaded = ProofStore::load_for_date(&path, day);
        assert_eq!(reloaded.len(), 1);
        assert_eq!(
            reloaded
                .get_by_key(&day_str, "Matkul 1|KO01N")
                .unwrap()
                .verification,
            ProofVerification::PortalVerified
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn proof_store_prunes_by_age_and_max_records() {
        let dir =
            std::env::temp_dir().join(format!("polban_absen_proof_prune_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join(".proofs.json");

        let today = NaiveDate::from_ymd_opt(2026, 11, 21).unwrap();
        let expired_day = today - Duration::days(MAX_PROOF_DAYS);
        let oldest_kept_day = today - Duration::days(MAX_PROOF_DAYS - 1);

        let mut store = ProofStore::load_for_date(&path, today);
        // Seed an expired proof + 105 recent proofs -> pruned to 100 newest, expired dropped.
        store
            .upsert_and_save_for_date(
                mk_proof(expired_day, 0, ProofVerification::PostAccepted),
                today,
            )
            .unwrap();
        assert!(store.is_empty());

        store
            .upsert_and_save_for_date(
                mk_proof(oldest_kept_day, 0, ProofVerification::PostAccepted),
                today,
            )
            .unwrap();
        for i in 1..=105 {
            let mut p = mk_proof(today, i, ProofVerification::PostAccepted);
            p.recorded_at = format!("2026-11-21T08:{:02}:{:02}+07:00", i / 60, i % 60);
            store.upsert_and_save_for_date(p, today).unwrap();
        }
        assert_eq!(store.len(), MAX_PROOF_RECORDS);
        // Oldest day entry was pushed out by 105 newer records.
        assert!(store
            .get_by_key(&format_date(oldest_kept_day), "Matkul 0|KO00N")
            .is_none());

        let reloaded = ProofStore::load_for_date(&path, today);
        assert_eq!(reloaded.len(), MAX_PROOF_RECORDS);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

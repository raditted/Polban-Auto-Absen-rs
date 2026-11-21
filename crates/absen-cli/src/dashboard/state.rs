//! Shared dashboard state used by both `PlainReporter` (F3) and ratatui TUI (F4).

use std::collections::{HashMap, HashSet, VecDeque};

use absen_core::text::{nama_dosen, split_sig};
use absen_core::{Action, AttendanceProof, Color, Event, LogLevel, Skip, UserInfo};
use chrono::Local;

pub const MAX_LOG: usize = 15;
const MAX_MK_NAME_CHARS: usize = 48;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MataKuliah {
    pub nama: String,
    pub jam: String,
    pub dosen: String,
    pub status: Color,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub timestamp: String,
    pub code: String,
    pub message: String,
    pub level: LogLevel,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApplyOutcome {
    pub jadwal_updated: bool,
    pub jadwal_changed: bool,
    pub user_info_updated: bool,
    pub new_log: Option<LogEntry>,
    pub proof_updated: Option<AttendanceProof>,
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

/// Thread-agnostic state mirror of `Dashboard` in `windows/tui.py`.
#[derive(Debug, Clone)]
pub struct DashboardState {
    pub user_info: UserInfo,
    pub auth_status: String,
    pub last_scan_info: String,
    pub engine: String,
    pub interval: u64,
    pub countdown: u32,
    pub jadwal: Vec<MataKuliah>,
    pub log: VecDeque<LogEntry>,
    pub cycle_count: u64,
    pub stat_ok: u64,
    pub stat_fail: u64,
    pub proofs: Vec<AttendanceProof>,
    pub proof_modal_index: Option<usize>,
    pub today_date: String,
    absen_sukses: HashSet<String>,
}

impl Default for DashboardState {
    fn default() -> Self {
        Self {
            user_info: UserInfo::default(),
            auth_status: "...".to_string(),
            last_scan_info: "Menunggu scan awal...".to_string(),
            engine: "http".to_string(),
            interval: 60,
            countdown: 0,
            jadwal: Vec::new(),
            log: VecDeque::with_capacity(MAX_LOG),
            cycle_count: 0,
            stat_ok: 0,
            stat_fail: 0,
            proofs: Vec::new(),
            proof_modal_index: None,
            today_date: Self::current_date_str(),
            absen_sukses: HashSet::new(),
        }
    }
}

impl DashboardState {
    fn current_date_str() -> String {
        Local::now().format("%Y-%m-%d").to_string()
    }

    /// Refresh `today_date` from local wall clock, clearing `absen_sukses` if the day rolled over.
    pub fn refresh_today_date(&mut self) {
        let now_date = Self::current_date_str();
        if self.today_date != now_date {
            self.today_date = now_date;
            self.absen_sukses.clear();
        }
    }

    /// Rebuild today's course table from `actions` and `skips`.
    /// Returns `true` if any status changed compared to the previous scan.
    pub fn update_jadwal(&mut self, actions: &[Action], skips: &[Skip]) -> bool {
        self.refresh_today_date();
        let old_states: HashMap<String, Color> = self
            .jadwal
            .iter()
            .map(|mk| (mk.nama.clone(), mk.status))
            .collect();

        let mut jadwal = Vec::with_capacity(skips.len() + actions.len());

        for skip in skips {
            let (nama_raw, dosen_raw, jam_raw) = split_sig(&skip.sig);
            jadwal.push(MataKuliah {
                nama: truncate_chars(nama_raw, MAX_MK_NAME_CHARS),
                jam: jam_raw.to_string(),
                dosen: nama_dosen(dosen_raw),
                status: skip.color,
            });
        }

        for act in actions {
            let (sig_nama, sig_dosen, sig_jam) = split_sig(&act.sig);
            let nama = if act.nama_mk.is_empty() {
                if sig_nama.is_empty() {
                    "?"
                } else {
                    sig_nama
                }
            } else {
                &act.nama_mk
            };
            let jam = if act.jam.is_empty() {
                sig_jam
            } else {
                &act.jam
            };
            let dosen = if act.dosen.is_empty() {
                sig_dosen
            } else {
                &act.dosen
            };
            jadwal.push(MataKuliah {
                nama: truncate_chars(nama, MAX_MK_NAME_CHARS),
                jam: jam.to_string(),
                dosen: nama_dosen(dosen),
                status: Color::Blue,
            });
        }

        let mut changed = false;
        for mk in &mut jadwal {
            if self.absen_sukses.contains(&mk.nama) {
                mk.status = Color::Green;
            }
            if let Some(&old_status) = old_states.get(&mk.nama) {
                if old_status != mk.status {
                    changed = true;
                }
            }
        }
        if old_states.len() != jadwal.len() && !old_states.is_empty() {
            changed = true;
        }

        self.jadwal = jadwal;
        changed
    }

    /// Update row status and stats after attempting attendance on one course.
    pub fn mark_result(&mut self, sig: &str, success: bool, nama: &str) {
        let trimmed = nama.trim();
        let target = if !trimmed.is_empty() {
            trimmed
        } else {
            let s0 = split_sig(sig).0;
            if !s0.is_empty() {
                s0
            } else {
                sig.split('|').next().unwrap_or("").trim()
            }
        };

        if success {
            self.stat_ok += 1;
            if !target.is_empty() {
                self.absen_sukses
                    .insert(truncate_chars(target, MAX_MK_NAME_CHARS));
            }
        } else {
            self.stat_fail += 1;
        }

        if !target.is_empty() {
            for mk in &mut self.jadwal {
                if mk.nama.contains(target) || target.contains(&mk.nama) {
                    mk.status = if success { Color::Green } else { Color::Blue };
                    break;
                }
            }
        }
    }

    pub fn push_log(&mut self, code: String, message: String, level: LogLevel) -> LogEntry {
        let entry = LogEntry {
            timestamp: Local::now().format("%H:%M:%S").to_string(),
            code,
            message,
            level,
        };
        if self.log.len() >= MAX_LOG {
            self.log.pop_front();
        }
        self.log.push_back(entry.clone());
        entry
    }

    pub fn has_proofs(&self) -> bool {
        !self.proofs.is_empty()
    }

    pub fn active_proof(&self) -> Option<&AttendanceProof> {
        self.proof_modal_index.and_then(|idx| self.proofs.get(idx))
    }

    pub fn toggle_proof_modal(&mut self) {
        if self.proof_modal_index.is_some() {
            self.proof_modal_index = None;
        } else if !self.proofs.is_empty() {
            self.proof_modal_index = Some(0);
        }
    }

    pub fn close_proof_modal(&mut self) {
        self.proof_modal_index = None;
    }

    pub fn next_proof(&mut self) {
        if let Some(idx) = self.proof_modal_index {
            if !self.proofs.is_empty() {
                self.proof_modal_index = Some((idx + 1) % self.proofs.len());
            } else {
                self.proof_modal_index = None;
            }
        }
    }

    pub fn prev_proof(&mut self) {
        if let Some(idx) = self.proof_modal_index {
            let len = self.proofs.len();
            if len > 0 {
                self.proof_modal_index = Some((idx + len - 1) % len);
            } else {
                self.proof_modal_index = None;
            }
        }
    }

    pub fn proof_for_course(&self, mk: &MataKuliah) -> Option<&AttendanceProof> {
        let target = mk.nama.trim();
        if target.is_empty() {
            return None;
        }
        let target_dosen = mk.dosen.trim();
        let target_jam = mk.jam.trim();

        self.proofs
            .iter()
            .rev()
            .filter(|p| {
                let proof_date = p.date.trim();
                if proof_date.is_empty() || proof_date != self.today_date {
                    return false;
                }
                let (sig_nama, sig_dosen, sig_jam) = split_sig(&p.sig);
                let raw_name = if !p.nama_mk.trim().is_empty() {
                    p.nama_mk.trim()
                } else {
                    sig_nama.trim()
                };
                if raw_name.is_empty() {
                    return false;
                }
                let norm_name = truncate_chars(raw_name, MAX_MK_NAME_CHARS);
                if !norm_name.eq_ignore_ascii_case(target) {
                    return false;
                }

                let raw_dosen = if !p.dosen.trim().is_empty() {
                    p.dosen.trim()
                } else {
                    sig_dosen.trim()
                };
                let norm_dosen = nama_dosen(raw_dosen);
                if !target_dosen.is_empty()
                    && !norm_dosen.is_empty()
                    && !norm_dosen.eq_ignore_ascii_case(target_dosen)
                {
                    return false;
                }

                let proof_jam = if !p.jam.trim().is_empty() {
                    p.jam.trim()
                } else {
                    sig_jam.trim()
                };
                target_jam.is_empty() || proof_jam.is_empty() || proof_jam == target_jam
            })
            .max_by_key(|p| p.verification)
    }

    /// Insert or upgrade an attendance proof without ever downgrading verification strength.
    ///
    /// Keeps newest/most recently updated proofs first (`index 0`) in deterministic order.
    pub fn upsert_proof(&mut self, proof: AttendanceProof) -> Option<AttendanceProof> {
        self.refresh_today_date();
        if let Some(idx) = self.proofs.iter().position(|p| proof_matches(p, &proof)) {
            let existing_ver = self.proofs[idx].verification;
            if proof.verification < existing_ver {
                return None;
            }
            if proof.verification > existing_ver {
                merge_upgraded_proof(&mut self.proofs[idx], proof);
                let out_idx = if self.proof_modal_index.is_none() && idx != 0 {
                    let updated = self.proofs.remove(idx);
                    self.proofs.insert(0, updated);
                    0
                } else {
                    idx
                };
                return Some(self.proofs[out_idx].clone());
            }
            if proof.recorded_at > self.proofs[idx].recorded_at {
                self.proofs[idx] = proof;
                let out_idx = if self.proof_modal_index.is_none() && idx != 0 {
                    let updated = self.proofs.remove(idx);
                    self.proofs.insert(0, updated);
                    0
                } else {
                    idx
                };
                return Some(self.proofs[out_idx].clone());
            }
            return None;
        }

        self.proofs.insert(0, proof.clone());
        if let Some(open_idx) = self.proof_modal_index {
            self.proof_modal_index = Some((open_idx + 1).min(self.proofs.len().saturating_sub(1)));
        }
        Some(proof)
    }

    pub fn apply(&mut self, event: Event) -> ApplyOutcome {
        let mut outcome = ApplyOutcome::default();
        match event {
            Event::Log {
                code,
                message,
                level,
            } => {
                outcome.new_log = Some(self.push_log(code, message, level));
            }
            Event::AuthStatus(status) => {
                self.auth_status = status;
            }
            Event::UserInfo(info) => {
                self.user_info = info;
                outcome.user_info_updated = true;
            }
            Event::Engine { name, interval } => {
                self.engine = name;
                self.interval = interval;
            }
            Event::CycleIncrement => {
                self.cycle_count += 1;
            }
            Event::Jadwal { actions, skips } => {
                outcome.jadwal_updated = true;
                outcome.jadwal_changed = self.update_jadwal(&actions, &skips);
            }
            Event::LastScan(info) => {
                self.last_scan_info = info;
            }
            Event::MarkResult { sig, success, nama } => {
                self.mark_result(&sig, success, &nama);
            }
            Event::AttendanceProof(proof) => {
                outcome.proof_updated = self.upsert_proof(proof);
            }
            Event::PortalSnapshot(_) => {}
            Event::Countdown(secs) => {
                self.countdown = secs;
            }
        }
        outcome
    }
}

fn proof_matches(a: &AttendanceProof, b: &AttendanceProof) -> bool {
    if a.date != b.date {
        return false;
    }
    if !a.sig.is_empty() && !b.sig.is_empty() {
        return a.sig == b.sig;
    }
    if !a.key.is_empty() && !b.key.is_empty() {
        return a.key == b.key;
    }
    !a.proof_id.is_empty() && a.proof_id == b.proof_id
}

fn fill_if_empty(target: &mut String, fallback: &str) {
    if target.is_empty() && !fallback.is_empty() {
        *target = fallback.to_string();
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
    fill_if_empty(&mut existing.sig, &incoming.sig);
    fill_if_empty(&mut existing.key, &incoming.key);
    fill_if_empty(&mut existing.nim, &incoming.nim);
    fill_if_empty(&mut existing.student_nama, &incoming.student_nama);
    fill_if_empty(&mut existing.jurusan, &incoming.jurusan);
    fill_if_empty(&mut existing.nama_mk, &incoming.nama_mk);
    fill_if_empty(&mut existing.mk_code, &incoming.mk_code);
    fill_if_empty(&mut existing.dosen, &incoming.dosen);
    fill_if_empty(&mut existing.jam, &incoming.jam);
    fill_if_empty(&mut existing.tp, &incoming.tp);
    fill_if_empty(&mut existing.ja, &incoming.ja);
    fill_if_empty(&mut existing.jb, &incoming.jb);
    fill_if_empty(&mut existing.kls, &incoming.kls);
    fill_if_empty(&mut existing.target_url, &incoming.target_url);
}

#[cfg(test)]
mod tests {
    use absen_core::{AbsenFormData, ProofVerification};

    use super::*;

    fn sample_action(nama: &str, dosen: &str, jam: &str) -> Action {
        Action {
            sig: format!("{nama}|{dosen}|{jam}"),
            key: format!("{nama}|{dosen}"),
            url: "http://127.0.0.1/ajar/absen/absensi_awal".into(),
            data: AbsenFormData {
                ja: "1".into(),
                jb: "2".into(),
                mk: "MK1".into(),
                dsn: "KO01N".into(),
                tp: "T".into(),
                kls: "1A".into(),
            },
            nama_mk: nama.into(),
            dosen: dosen.into(),
            jam: jam.into(),
            label: format!("{nama} ({jam})"),
        }
    }

    #[test]
    fn update_jadwal_detects_changes_and_normalizes_dosen() {
        let mut st = DashboardState::default();
        let a1 = sample_action("Struktur Data", "KO078N-Trisna Gelar", "07:00-09:30");
        let s1 = Skip {
            color: Color::Orange,
            sig: "Belum Buka|Basis Data|BD011N-Siti Aminah|09:30-12:00".into(),
            reason: "belum buka",
        };

        // First scan: old_states empty -> changed = false
        assert!(!st.update_jadwal(std::slice::from_ref(&a1), std::slice::from_ref(&s1)));
        assert_eq!(st.jadwal.len(), 2);
        assert_eq!(st.jadwal[0].dosen, "Siti Aminah");
        assert_eq!(st.jadwal[1].dosen, "Trisna Gelar");

        // Same status on next scan -> changed = false
        assert!(!st.update_jadwal(&[a1], &[s1]));

        // Status flips from Orange -> Blue -> changed = true
        let a2 = sample_action("Basis Data", "BD011N-Siti Aminah", "09:30-12:00");
        let a1 = sample_action("Struktur Data", "KO078N-Trisna Gelar", "07:00-09:30");
        assert!(st.update_jadwal(&[a1, a2], &[]));
    }

    #[test]
    fn mark_result_and_sticky_absen_sukses() {
        let mut st = DashboardState::default();
        let a1 = sample_action("Struktur Data", "KO078N-Trisna", "07:00-09:30");
        st.update_jadwal(std::slice::from_ref(&a1), &[]);
        assert_eq!(st.jadwal[0].status, Color::Blue);

        // Failure keeps row Blue and increments stat_fail
        st.mark_result(&a1.sig, false, "Struktur Data");
        assert_eq!(st.stat_fail, 1);
        assert_eq!(st.stat_ok, 0);
        assert_eq!(st.jadwal[0].status, Color::Blue);

        // Success sets row Green and adds to sticky absen_sukses
        st.mark_result(&a1.sig, true, "Struktur Data");
        assert_eq!(st.stat_ok, 1);
        assert_eq!(st.jadwal[0].status, Color::Green);

        // Even if server still reports Blue on next scan, sticky keeps it Green
        assert!(!st.update_jadwal(&[a1], &[]));
        assert_eq!(st.jadwal[0].status, Color::Green);
    }

    #[test]
    fn log_ring_buffer_caps_at_max_log_15() {
        let mut st = DashboardState::default();
        for i in 0..20 {
            st.push_log("TEST".into(), format!("msg {i}"), LogLevel::Info);
        }
        assert_eq!(st.log.len(), MAX_LOG);
        assert_eq!(st.log.front().unwrap().message, "msg 5");
        assert_eq!(st.log.back().unwrap().message, "msg 19");
    }

    fn sample_proof(
        id: &str,
        nama_mk: &str,
        jam: &str,
        recorded_at: &str,
        verification: ProofVerification,
    ) -> AttendanceProof {
        AttendanceProof {
            proof_id: id.into(),
            sig: format!("{nama_mk}|KO078N-Trisna|{jam}"),
            key: format!("{nama_mk}|KO078N-Trisna"),
            date: "2026-11-17".into(),
            recorded_at: recorded_at.into(),
            verified_at: if verification == ProofVerification::PortalVerified {
                Some("2026-11-17T07:05:15+07:00".into())
            } else {
                None
            },
            server_date_header: None,
            verification,
            nim: "251511001".into(),
            student_nama: "Budi Santoso".into(),
            jurusan: "D3-Teknik Informatika".into(),
            nama_mk: nama_mk.into(),
            mk_code: "25IF201".into(),
            dosen: "KO078N-Trisna".into(),
            jam: jam.into(),
            tp: "T".into(),
            ja: "1".into(),
            jb: "3".into(),
            kls: "1A".into(),
            target_url: "https://akademik.polban.ac.id/ajar/absen".into(),
            endpoint_url: Some("https://akademik.polban.ac.id/ajar/absen/absensi_awal".into()),
            http_status: Some(200),
            server_response: Some("ok".into()),
            server_button_label: if verification == ProofVerification::PortalVerified {
                Some("Sudah Absen".into())
            } else {
                None
            },
            attempt: Some(1),
        }
    }

    #[test]
    fn proof_upsert_upgrades_without_downgrade_and_preserves_order() {
        let mut st = DashboardState::default();
        assert!(!st.has_proofs());
        assert_eq!(st.proof_modal_index, None);

        // MarkResult never auto-opens modal
        st.apply(Event::MarkResult {
            sig: "Struktur Data|KO078N-Trisna|07:00-09:30".into(),
            success: true,
            nama: "Struktur Data".into(),
        });
        assert_eq!(st.proof_modal_index, None);

        // Insert PostAccepted for Struktur Data
        let p1 = sample_proof(
            "PRF-20261117-00000001",
            "Struktur Data",
            "07:00-09:30",
            "2026-11-17T07:05:10+07:00",
            ProofVerification::PostAccepted,
        );
        let out1 = st.apply(Event::AttendanceProof(p1));
        assert!(out1.proof_updated.is_some());
        assert_eq!(st.proofs.len(), 1);
        assert_eq!(st.proof_modal_index, None);

        // Insert ObservedPresent for Basis Data (newest goes to index 0)
        let p2 = sample_proof(
            "PRF-20261117-00000002",
            "Basis Data",
            "09:30-12:00",
            "2026-11-17T09:35:10+07:00",
            ProofVerification::ObservedPresent,
        );
        st.apply(Event::AttendanceProof(p2));
        assert_eq!(st.proofs.len(), 2);
        assert_eq!(st.proofs[0].nama_mk, "Basis Data");
        assert_eq!(st.proofs[1].nama_mk, "Struktur Data");

        // Upgrade Struktur Data to PortalVerified (without HTTP/server_response on incoming)
        let mut p1_upgrade = sample_proof(
            "PRF-20261117-00000099",
            "Struktur Data",
            "07:00-09:30",
            "2026-11-17T07:05:15+07:00",
            ProofVerification::PortalVerified,
        );
        p1_upgrade.http_status = None;
        p1_upgrade.server_response = None;
        let out_up = st.apply(Event::AttendanceProof(p1_upgrade));
        assert!(out_up.proof_updated.is_some());
        assert_eq!(st.proofs.len(), 2);
        // Upgraded proof is moved to front when modal is closed
        assert_eq!(st.proofs[0].nama_mk, "Struktur Data");
        assert_eq!(st.proofs[0].verification, ProofVerification::PortalVerified);
        // Preserved original proof_id & http_status
        assert_eq!(st.proofs[0].proof_id, "PRF-20261117-00000001");
        assert_eq!(st.proofs[0].http_status, Some(200));
        assert_eq!(
            st.proofs[0].server_button_label.as_deref(),
            Some("Sudah Absen")
        );

        // Attempt downgrade to PostAccepted or ObservedPresent -> ignored
        let p1_downgrade = sample_proof(
            "PRF-20261117-00000001",
            "Struktur Data",
            "07:00-09:30",
            "2026-11-17T07:06:00+07:00",
            ProofVerification::PostAccepted,
        );
        let out_down = st.apply(Event::AttendanceProof(p1_downgrade));
        assert!(out_down.proof_updated.is_none());
        assert_eq!(st.proofs[0].verification, ProofVerification::PortalVerified);

        // Course lookup matches MataKuliah for today's date
        st.today_date = "2026-11-17".into();
        let mk = MataKuliah {
            nama: "Struktur Data".into(),
            jam: "07:00-09:30".into(),
            dosen: "Trisna".into(),
            status: Color::Green,
        };
        assert_eq!(
            st.proof_for_course(&mk).map(|p| p.proof_id.as_str()),
            Some("PRF-20261117-00000001")
        );
    }

    #[test]
    fn proof_modal_open_close_and_cycle() {
        let mut st = DashboardState::default();
        // Toggling with no proofs keeps modal closed
        st.toggle_proof_modal();
        assert_eq!(st.proof_modal_index, None);
        assert!(st.active_proof().is_none());

        st.apply(Event::AttendanceProof(sample_proof(
            "PRF-20261117-00000001",
            "Struktur Data",
            "07:00-09:30",
            "2026-11-17T07:05:10+07:00",
            ProofVerification::PostAccepted,
        )));
        st.apply(Event::AttendanceProof(sample_proof(
            "PRF-20261117-00000002",
            "Basis Data",
            "09:30-12:00",
            "2026-11-17T09:35:10+07:00",
            ProofVerification::PortalVerified,
        )));

        st.toggle_proof_modal();
        assert_eq!(st.proof_modal_index, Some(0));
        assert_eq!(st.active_proof().unwrap().proof_id, "PRF-20261117-00000002");

        st.next_proof();
        assert_eq!(st.proof_modal_index, Some(1));
        assert_eq!(st.active_proof().unwrap().proof_id, "PRF-20261117-00000001");

        // Wrap forward
        st.next_proof();
        assert_eq!(st.proof_modal_index, Some(0));

        // Wrap backward
        st.prev_proof();
        assert_eq!(st.proof_modal_index, Some(1));

        st.close_proof_modal();
        assert_eq!(st.proof_modal_index, None);
    }

    #[test]
    fn proof_for_course_distinguishes_same_mk_with_different_dosen_or_jam() {
        let mut st = DashboardState::default();
        let mut p1 = sample_proof(
            "PRF-20261117-00000011",
            "Pemrograman Web",
            "07:00:00 s.d. 09:30:00",
            "2026-11-17T07:05:10+07:00",
            ProofVerification::PortalVerified,
        );
        p1.dosen = "KO010N-Dosen Satu".into();
        p1.sig = "Pemrograman Web|KO010N-Dosen Satu|07:00:00 s.d. 09:30:00".into();
        p1.key = "Pemrograman Web|KO010N-Dosen Satu".into();
        st.apply(Event::AttendanceProof(p1));
        st.today_date = "2026-11-17".into();

        let row1 = MataKuliah {
            nama: "Pemrograman Web".into(),
            jam: "07:00:00 s.d. 09:30:00".into(),
            dosen: "Dosen Satu".into(),
            status: Color::Green,
        };
        let row2_diff_dosen = MataKuliah {
            nama: "Pemrograman Web".into(),
            jam: "10:20:00 s.d. 12:00:00".into(),
            dosen: "Dosen Dua".into(),
            status: Color::Blue,
        };
        let row3_diff_jam_same_dosen = MataKuliah {
            nama: "Pemrograman Web".into(),
            jam: "13:00:00 s.d. 15:30:00".into(),
            dosen: "Dosen Satu".into(),
            status: Color::Blue,
        };

        assert_eq!(
            st.proof_for_course(&row1).map(|p| p.proof_id.as_str()),
            Some("PRF-20261117-00000011")
        );
        assert!(st.proof_for_course(&row2_diff_dosen).is_none());
        assert!(st.proof_for_course(&row3_diff_jam_same_dosen).is_none());
    }

    #[test]
    fn proof_for_course_ignores_earlier_days_while_retaining_history_for_modal() {
        let mut st = DashboardState::default();
        let mut yesterday = sample_proof(
            "PRF-20261119-00000001",
            "Struktur Data",
            "07:00-09:30",
            "2026-11-19T07:05:10+07:00",
            ProofVerification::PortalVerified,
        );
        yesterday.date = "2026-11-19".into();
        yesterday.verified_at = Some("2026-11-19T07:05:12+07:00".into());

        st.apply(Event::AttendanceProof(yesterday));
        st.today_date = "2026-11-20".into();

        let mk_today = MataKuliah {
            nama: "Struktur Data".into(),
            jam: "07:00-09:30".into(),
            dosen: "Trisna".into(),
            status: Color::Green,
        };

        // Yesterday's proof must not cross-match today's summary row.
        assert!(st.proof_for_course(&mk_today).is_none());
        // History remains available for modal browsing.
        assert_eq!(st.proofs.len(), 1);
        st.toggle_proof_modal();
        assert_eq!(
            st.active_proof().map(|p| p.proof_id.as_str()),
            Some("PRF-20261119-00000001")
        );

        // Today's proof matches today's course while yesterday remains accessible via next_proof().
        let mut today = sample_proof(
            "PRF-20261120-00000002",
            "Struktur Data",
            "07:00-09:30",
            "2026-11-20T07:05:12+07:00",
            ProofVerification::PostAccepted,
        );
        today.date = "2026-11-20".into();
        today.verified_at = None;
        st.apply(Event::AttendanceProof(today));
        st.today_date = "2026-11-20".into();

        assert_eq!(
            st.proof_for_course(&mk_today).map(|p| p.proof_id.as_str()),
            Some("PRF-20261120-00000002")
        );
        assert_eq!(st.proofs.len(), 2);
        assert_eq!(
            st.proofs
                .iter()
                .map(|p| p.proof_id.as_str())
                .collect::<Vec<_>>(),
            vec!["PRF-20261120-00000002", "PRF-20261119-00000001"]
        );
    }
}

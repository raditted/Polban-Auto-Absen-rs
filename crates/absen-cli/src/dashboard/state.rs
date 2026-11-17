//! Shared dashboard state used by both `PlainReporter` (F3) and ratatui TUI (F4).

use std::collections::{HashMap, HashSet, VecDeque};

use absen_core::text::{nama_dosen, split_sig};
use absen_core::{Action, Color, Event, LogLevel, Skip, UserInfo};
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
            absen_sukses: HashSet::new(),
        }
    }
}

impl DashboardState {
    /// Rebuild today's course table from `actions` and `skips`.
    /// Returns `true` if any status changed compared to the previous scan.
    pub fn update_jadwal(&mut self, actions: &[Action], skips: &[Skip]) -> bool {
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
            Event::Countdown(secs) => {
                self.countdown = secs;
            }
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use absen_core::AbsenFormData;

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
}

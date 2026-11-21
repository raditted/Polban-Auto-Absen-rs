//! Plain-text reporter matching `PlainDashboard` in `windows/tui.py`.

use std::collections::HashSet;
use std::io::Write;
use std::sync::Mutex;

use absen_core::{AttendanceProof, Event, ProofVerification, Reporter};
use chrono::Local;

use crate::dashboard::state::DashboardState;
use crate::platform;

struct PlainInner {
    state: DashboardState,
    jadwal_printed: bool,
    emitted_proofs: HashSet<(String, String, ProofVerification)>,
}

impl PlainInner {
    fn record_proof_line(&mut self, proof: &AttendanceProof) -> Option<String> {
        let proof_date = proof.date.trim();
        if proof_date.is_empty() || proof_date != self.state.today_date {
            return None;
        }
        if !matches!(
            proof.verification,
            ProofVerification::PostAccepted | ProofVerification::PortalVerified
        ) {
            return None;
        }
        let dedup_key = if !proof.sig.is_empty() {
            proof.sig.clone()
        } else if !proof.key.is_empty() {
            proof.key.clone()
        } else {
            proof.proof_id.clone()
        };
        if !self
            .emitted_proofs
            .insert((proof_date.to_string(), dedup_key, proof.verification))
        {
            return None;
        }
        Some(format_plain_proof_line(proof))
    }
}

pub(crate) fn format_plain_proof_line(proof: &AttendanceProof) -> String {
    let status_part = match proof
        .server_button_label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(btn) => format!("{} ({btn})", proof.verification_label()),
        None => proof.verification_label().to_string(),
    };

    let mut parts = vec![
        format!("       Bukti [{}]", proof.proof_id),
        proof.short_time_hms(),
        status_part,
    ];
    if let Some(status) = proof.http_status {
        parts.push(format!("HTTP {status}"));
    }
    parts.join(" | ")
}

/// Line-oriented stdout reporter for `--plain`, non-TTY, or `--once` fallback.
pub struct PlainReporter {
    inner: Mutex<PlainInner>,
}

impl PlainReporter {
    pub fn new(engine: impl Into<String>, interval: u64) -> Self {
        let mut state = DashboardState::default();
        state.engine = engine.into();
        state.interval = interval;
        Self {
            inner: Mutex::new(PlainInner {
                state,
                jadwal_printed: false,
                emitted_proofs: HashSet::new(),
            }),
        }
    }

    pub fn start(&self) {
        platform::print_ascii_banner();
    }

    pub fn render_summary(&self) {
        let Ok(guard) = self.inner.lock() else {
            return;
        };
        let st = &guard.state;
        println!(
            "\nRingkasan: {} siklus | {} berhasil | {} gagal",
            st.cycle_count, st.stat_ok, st.stat_fail
        );
        let _ = std::io::stdout().flush();
    }
}

impl Reporter for PlainReporter {
    fn emit(&self, event: Event) {
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        let outcome = guard.state.apply(event);

        if outcome.user_info_updated {
            let st = &guard.state;
            let info = &st.user_info;
            let garis = "=".repeat(56);
            println!("{garis}");
            if !info.nama.is_empty() {
                println!("  Nama    : {}", info.nama);
            }
            let nim = if info.nim.is_empty() { "-" } else { &info.nim };
            println!("  NIM     : {nim}");
            if !info.jurusan.is_empty() {
                println!("  Jurusan : {}", info.jurusan);
            }
            println!("  Waktu   : {}", Local::now().format("%Y-%m-%d %H:%M:%S"));
            println!("  Engine  : {}   Interval : {}s", st.engine, st.interval);
            println!("{garis}");
            let _ = std::io::stdout().flush();
        }

        if let Some(entry) = outcome.new_log {
            println!(
                "[{}] {} [{}] {}",
                entry.timestamp,
                entry.level.tag(),
                entry.code,
                entry.message
            );
            let _ = std::io::stdout().flush();
        }

        if let Some(proof) = outcome.proof_updated {
            if let Some(line) = guard.record_proof_line(&proof) {
                println!("{line}");
                let _ = std::io::stdout().flush();
            }
        }

        if outcome.jadwal_updated && (outcome.jadwal_changed || !guard.jadwal_printed) {
            guard.jadwal_printed = true;
            print_jadwal_list(&guard.state);
        }
    }
}

fn print_jadwal_list(st: &DashboardState) {
    println!("[{}] Jadwal hari ini:", Local::now().format("%H:%M:%S"));
    if st.jadwal.is_empty() {
        println!("     (tidak ada baris jadwal terbaca)");
    } else {
        for (idx, mk) in st.jadwal.iter().enumerate() {
            let label = mk.status.status_label();
            let jam = if mk.jam.is_empty() {
                String::new()
            } else {
                format!("  {}", mk.jam)
            };
            let dosen = if mk.dosen.is_empty() {
                String::new()
            } else {
                format!("  [{}]", mk.dosen)
            };
            println!(
                "     {}. {}{}  -> {}{}",
                idx + 1,
                mk.nama,
                jam,
                label,
                dosen
            );
        }
    }
    println!();
    let _ = std::io::stdout().flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_proof(verification: ProofVerification) -> AttendanceProof {
        AttendanceProof {
            proof_id: "PRF-20261117-AABBCCDD".into(),
            sig: "Struktur Data|KO078N-Trisna|07:00-09:30".into(),
            key: "Struktur Data|KO078N-Trisna".into(),
            date: "2026-11-17".into(),
            recorded_at: "2026-11-17T07:05:12+07:00".into(),
            verified_at: if verification == ProofVerification::PortalVerified {
                Some("2026-11-17T07:05:14+07:00".into())
            } else {
                None
            },
            server_date_header: None,
            verification,
            nim: "251511001".into(),
            student_nama: "Budi Santoso".into(),
            jurusan: "D3-Teknik Informatika".into(),
            nama_mk: "Struktur Data".into(),
            mk_code: "25IF201".into(),
            dosen: "KO078N-Trisna".into(),
            jam: "07:00-09:30".into(),
            tp: "T".into(),
            ja: "1".into(),
            jb: "3".into(),
            kls: "1A".into(),
            target_url: "https://akademik.polban.ac.id/ajar/absen".into(),
            endpoint_url: None,
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
    fn plain_proof_emission_filters_observed_and_deduplicates() {
        let mut state = DashboardState::default();
        state.today_date = "2026-11-17".into();
        let mut inner = PlainInner {
            state,
            jadwal_printed: false,
            emitted_proofs: HashSet::new(),
        };

        // ObservedPresent should not emit a line in plain mode
        let obs = sample_proof(ProofVerification::ObservedPresent);
        assert!(inner.record_proof_line(&obs).is_none());

        // PostAccepted emits once, second PostAccepted for same sig is deduplicated
        let post = sample_proof(ProofVerification::PostAccepted);
        let line1 = inner.record_proof_line(&post).expect("PostAccepted emits");
        assert_eq!(
            line1,
            "       Bukti [PRF-20261117-AABBCCDD] | 07:05:12 | Diterima Server | HTTP 200"
        );
        assert!(inner.record_proof_line(&post).is_none());

        // Upgrade to PortalVerified emits once, second PortalVerified is deduplicated
        let verified = sample_proof(ProofVerification::PortalVerified);
        let line2 = inner
            .record_proof_line(&verified)
            .expect("PortalVerified emits");
        assert_eq!(
            line2,
            "       Bukti [PRF-20261117-AABBCCDD] | 07:05:12 | Terverifikasi Portal (Sudah Absen) | HTTP 200"
        );
        assert!(inner.record_proof_line(&verified).is_none());
    }

    #[test]
    fn plain_proof_emission_suppresses_previous_day_proofs() {
        let mut inner = PlainInner {
            state: DashboardState::default(),
            jadwal_printed: false,
            emitted_proofs: HashSet::new(),
        };

        let today = inner.state.today_date.clone();
        let yesterday = (Local::now().date_naive() - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();

        // Previous-day startup proofs (PostAccepted / PortalVerified) are suppressed in plain output.
        let mut old_post = sample_proof(ProofVerification::PostAccepted);
        old_post.proof_id = "PRF-OLD-0001".into();
        old_post.date = yesterday.clone();
        old_post.recorded_at = format!("{yesterday}T07:05:12+07:00");
        assert!(inner.record_proof_line(&old_post).is_none());

        let mut old_verified = sample_proof(ProofVerification::PortalVerified);
        old_verified.proof_id = "PRF-OLD-0001".into();
        old_verified.date = yesterday.clone();
        old_verified.recorded_at = format!("{yesterday}T07:05:12+07:00");
        old_verified.verified_at = Some(format!("{yesterday}T07:05:14+07:00"));
        assert!(inner.record_proof_line(&old_verified).is_none());

        // Same course on today's date still emits cleanly without colliding with yesterday's proof.
        let mut today_post = sample_proof(ProofVerification::PostAccepted);
        today_post.proof_id = "PRF-TODAY-0002".into();
        today_post.date = today.clone();
        today_post.recorded_at = format!("{today}T07:05:12+07:00");
        let line = inner
            .record_proof_line(&today_post)
            .expect("today's live proof emits");
        assert!(line.contains("Bukti [PRF-TODAY-0002]"));
        assert!(line.contains("07:05:12"));
        assert!(line.contains("Diterima Server"));
    }
}

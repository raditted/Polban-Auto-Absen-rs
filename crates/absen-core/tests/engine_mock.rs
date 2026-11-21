use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use absen_core::config::{
    Config, DEFAULT_DELAY_MAX, DEFAULT_DELAY_MIN, DEFAULT_ENGINE, DEFAULT_INTERVAL,
    DEFAULT_MAX_RETRY, DEFAULT_RETRY_WAIT,
};
use absen_core::engine::{run, run_with_commands, EngineCommand, EngineError};
use absen_core::event::{Event, Reporter, VecReporter};
use absen_core::state::{proof_path_for, ClickedState, ProofStore};
use absen_core::{
    generate_proof_id, AbsenClient, Action, AttendanceProof, Color, LogLevel, PortalPageSnapshot,
    ProofVerification,
};
use chrono::{Duration as ChronoDuration, Local};
use httpmock::prelude::*;

const LOGIN_HTML: &str = include_str!("fixtures/login.html");

const JADWAL_ONE_BLUE_HTML: &str = r#"<!DOCTYPE html>
<html><body>
  <div class="user-header">251511001 Budi Santoso D3-Teknik Informatika</div>
  <table id="jadwal">
    <tbody>
      <tr>
        <td>1</td>
        <td>KO078N-Trisna Gelar Abdillah, S.T., M.Kom</td>
        <td>25IF201</td>
        <td>Struktur Data</td>
        <td>T</td>
        <td>1</td>
        <td>3</td>
        <td>07:00:00 s.d. 09:30:00</td>
        <td>
          <input type="hidden" id="kls" value="1A">
          <button class="btn btn-primary">Absen Awal</button>
        </td>
      </tr>
    </tbody>
  </table>
</body></html>"#;

const JADWAL_GREEN_HTML: &str = r#"<!DOCTYPE html>
<html><body>
  <div class="user-header">251511001 Budi Santoso D3-Teknik Informatika</div>
  <table id="jadwal">
    <tbody>
      <tr>
        <td>1</td>
        <td>KO078N-Trisna Gelar Abdillah, S.T., M.Kom</td>
        <td>25IF201</td>
        <td>Struktur Data</td>
        <td>T</td>
        <td>1</td>
        <td>3</td>
        <td>07:00:00 s.d. 09:30:00</td>
        <td>
          <button class="btn btn-success">Sudah Absen</button>
        </td>
      </tr>
    </tbody>
  </table>
</body></html>"#;

fn test_config(base_url: &str, state_file: std::path::PathBuf) -> Config {
    Config {
        username: "251511001".to_string(),
        password: "secret#pass".to_string(),
        base_url: base_url.to_string(),
        target_url: format!("{base_url}/ajar/absen"),
        interval: DEFAULT_INTERVAL,
        delay_min: 0.0,
        delay_max: 0.0,
        max_retry: 3,
        retry_wait: 0.0,
        engine: DEFAULT_ENGINE.to_string(),
        env_path_used: None,
        state_file,
    }
}

fn temp_state_path(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("polban_absen_engine_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(".clicked.json")
}

#[test]
fn login_success_reuses_initial_html_without_extra_get() {
    let server = MockServer::start();
    let state_path = temp_state_path("login_reuse");
    let cfg = test_config(&server.base_url(), state_path.clone());
    let client = AbsenClient::new(&cfg.base_url).unwrap();

    let m_login = server.mock(|when, then| {
        when.method(POST)
            .path("/laman/login")
            .x_www_form_urlencoded_tuple("username", "251511001")
            .x_www_form_urlencoded_tuple("password", "secret#pass")
            .x_www_form_urlencoded_tuple("submit", "");
        then.status(200)
            .header("Set-Cookie", "ci_session=abc; Path=/")
            .body("ok");
    });

    let m_target = server.mock(|when, then| {
        when.method(GET)
            .path("/ajar/absen")
            .header("Cookie", "ci_session=abc");
        then.status(200).body(JADWAL_GREEN_HTML);
    });

    let reporter = VecReporter::new();
    let (_tx, rx) = mpsc::channel();
    run(&cfg, &client, false, &reporter, &rx).unwrap();

    m_login.assert_hits(1);
    // Crucial parity & performance check: login GET is reused for UserInfo + first scan.
    m_target.assert_hits(1);

    let events = reporter.snapshot();
    assert!(events.iter().any(
        |e| matches!(e, Event::UserInfo(u) if u.nim == "251511001" && u.nama == "Budi Santoso")
    ));
    assert!(events.iter().any(|e| matches!(e, Event::CycleIncrement)));

    let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
}

#[test]
fn login_failure_returns_login_failed_error() {
    let server = MockServer::start();
    let state_path = temp_state_path("login_fail");
    let cfg = test_config(&server.base_url(), state_path.clone());
    let client = AbsenClient::new(&cfg.base_url).unwrap();

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200).body("invalid");
    });
    server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(LOGIN_HTML);
    });

    let reporter = VecReporter::new();
    let (_tx, rx) = mpsc::channel();
    let err = run(&cfg, &client, false, &reporter, &rx).unwrap_err();
    assert!(matches!(err, EngineError::LoginFailed));

    let events = reporter.snapshot();
    assert!(events.iter().any(|e| {
        matches!(
            e,
            Event::Log { code, level: LogLevel::Fail, .. } if code == "LOGIN"
        )
    }));

    let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
}

#[test]
fn absen_direct_success_saves_state_and_sends_ajax_headers() {
    let server = MockServer::start();
    let state_path = temp_state_path("direct_ok");
    let cfg = test_config(&server.base_url(), state_path.clone());
    let client = AbsenClient::new(&cfg.base_url).unwrap();

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200)
            .header("Set-Cookie", "ci_session=ok1; Path=/")
            .body("ok");
    });
    server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(JADWAL_ONE_BLUE_HTML);
    });
    let m_absen = server.mock(|when, then| {
        when.method(POST)
            .path("/ajar/absen/absensi_awal")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", format!("{}/ajar/absen", server.base_url()))
            .header("Origin", server.base_url())
            .x_www_form_urlencoded_tuple("ja", "1")
            .x_www_form_urlencoded_tuple("jb", "3")
            .x_www_form_urlencoded_tuple("mk", "25IF201")
            .x_www_form_urlencoded_tuple("dsn", "KO078N")
            .x_www_form_urlencoded_tuple("tp", "T")
            .x_www_form_urlencoded_tuple("kls", "1A");
        then.status(200).body(r#"{"status":"ok"}"#);
    });

    let reporter = VecReporter::new();
    let (_tx, rx) = mpsc::channel();
    run(&cfg, &client, false, &reporter, &rx).unwrap();

    m_absen.assert_hits(1);

    let events = reporter.snapshot();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::MarkResult {
            success: true,
            nama,
            ..
        } if nama == "Struktur Data"
    )));

    let st = ClickedState::load(&state_path);
    assert!(st.contains(
        "Struktur Data|KO078N-Trisna Gelar Abdillah, S.T., M.Kom|07:00:00 s.d. 09:30:00"
    ));

    let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
}

#[test]
fn ambiguous_post_triggers_get_confirmation_and_succeeds_when_green() {
    let server = MockServer::start();
    let state_path = temp_state_path("ambiguous_confirm");
    let cfg = test_config(&server.base_url(), state_path.clone());
    let client = AbsenClient::new(&cfg.base_url).unwrap();

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200).body("ok");
    });

    let mut m_blue = Some(server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(JADWAL_ONE_BLUE_HTML);
    }));

    // Body contains "false" -> ambiguous/failure marker, triggers 1x GET confirmation.
    // Swap GET mock to green inside the POST mock's existence via reporter?
    // Better: use a custom Reporter that deletes `m_blue` and installs green on `ABSEN target:` log!
    let m_post = server.mock(|when, then| {
        when.method(POST).path("/ajar/absen/absensi_awal");
        then.status(200).body(r#"{"status":false}"#);
    });

    struct SwapOnTarget<'a> {
        server: &'a MockServer,
        m_blue: Mutex<Option<httpmock::Mock<'a>>>,
        events: Mutex<Vec<Event>>,
    }

    impl<'a> Reporter for SwapOnTarget<'a> {
        fn emit(&self, event: Event) {
            if let Event::Log { code, message, .. } = &event {
                if code == "ABSEN" && message.starts_with("target:") {
                    if let Some(mut m) = self.m_blue.lock().unwrap().take() {
                        m.delete();
                        self.server.mock(|when, then| {
                            when.method(GET).path("/ajar/absen");
                            then.status(200).body(JADWAL_GREEN_HTML);
                        });
                    }
                }
            }
            self.events.lock().unwrap().push(event);
        }
    }

    let reporter = SwapOnTarget {
        server: &server,
        m_blue: Mutex::new(m_blue.take()),
        events: Mutex::new(Vec::new()),
    };
    let (_tx, rx) = mpsc::channel();
    run(&cfg, &client, false, &reporter, &rx).unwrap();

    m_post.assert_hits(1);
    let events = reporter.events.lock().unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Log { code, message, level: LogLevel::Ok }
        if code == "ABSEN" && message.contains("dikonfirmasi")
    )));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::MarkResult { success: true, .. })));

    let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
}

#[test]
fn fails_after_max_retry_when_button_stays_blue_and_does_not_save_state() {
    let server = MockServer::start();
    let state_path = temp_state_path("max_retry_fail");
    let mut cfg = test_config(&server.base_url(), state_path.clone());
    cfg.max_retry = 3;
    let client = AbsenClient::new(&cfg.base_url).unwrap();

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200).body("ok");
    });
    let m_get = server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(JADWAL_ONE_BLUE_HTML);
    });
    let m_post = server.mock(|when, then| {
        when.method(POST).path("/ajar/absen/absensi_awal");
        then.status(200).body("gagal diproses");
    });

    let reporter = VecReporter::new();
    let (_tx, rx) = mpsc::channel();
    run(&cfg, &client, false, &reporter, &rx).unwrap();

    m_post.assert_hits(3);
    // 1 initial GET + 3 confirmation GETs
    m_get.assert_hits(4);

    let events = reporter.snapshot();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::MarkResult { success: false, nama, .. } if nama == "Struktur Data"
    )));
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Log { code, level: LogLevel::Fail, message }
        if code == "ABSEN" && message.contains("GAGAL setelah 3 percobaan")
    )));

    let st = ClickedState::load(&state_path);
    assert!(st.is_empty());

    let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
}

#[test]
fn already_clicked_signature_in_state_is_not_posted_again() {
    let server = MockServer::start();
    let state_path = temp_state_path("already_clicked");
    let mut st = ClickedState::load(&state_path);
    st.insert_and_save(
        "Struktur Data|KO078N-Trisna Gelar Abdillah, S.T., M.Kom|07:00:00 s.d. 09:30:00",
    )
    .unwrap();

    let cfg = test_config(&server.base_url(), state_path.clone());
    let client = AbsenClient::new(&cfg.base_url).unwrap();

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200).body("ok");
    });
    server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(JADWAL_ONE_BLUE_HTML);
    });
    let m_post = server.mock(|when, then| {
        when.method(POST).path("/ajar/absen/absensi_awal");
        then.status(200).body("ok");
    });

    let reporter = VecReporter::new();
    let (_tx, rx) = mpsc::channel();
    run(&cfg, &client, false, &reporter, &rx).unwrap();

    m_post.assert_hits(0);

    let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
}

#[test]
fn expired_session_mid_loop_triggers_automatic_relogin() {
    let server = MockServer::start();
    let state_path = temp_state_path("relogin");
    let mut cfg = test_config(&server.base_url(), state_path.clone());
    cfg.interval = 0; // loop immediately without waiting
    let client = AbsenClient::new(&cfg.base_url).unwrap();

    let login_calls = Arc::new(AtomicUsize::new(0));
    let m_login_1 = Mutex::new(Some(server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200)
            .header("Set-Cookie", "ci_session=v1; Path=/")
            .body("ok");
    })));
    let m_get_1 = Mutex::new(Some(server.mock(|when, then| {
        when.method(GET)
            .path("/ajar/absen")
            .header("Cookie", "ci_session=v1");
        then.status(200).body(JADWAL_GREEN_HTML);
    })));

    let (tx, rx) = mpsc::channel();
    let cycle_count = AtomicUsize::new(0);
    let swapped = AtomicBool::new(false);

    struct LoopReloginReporter<'a> {
        server: &'a MockServer,
        m_login_1: Mutex<Option<httpmock::Mock<'a>>>,
        m_get_1: Mutex<Option<httpmock::Mock<'a>>>,
        cycle_count: AtomicUsize,
        swapped: AtomicBool,
        login_calls: Arc<AtomicUsize>,
        shutdown_tx: mpsc::Sender<()>,
        events: Mutex<Vec<Event>>,
    }

    impl<'a> Reporter for LoopReloginReporter<'a> {
        fn emit(&self, event: Event) {
            match &event {
                Event::CycleIncrement => {
                    let c = self.cycle_count.fetch_add(1, Ordering::SeqCst) + 1;
                    if c >= 2 && self.swapped.load(Ordering::SeqCst) {
                        // Cycle 2 finished or started; wait until LastScan of cycle 2 to stop
                    }
                }
                Event::LastScan(_) => {
                    let c = self.cycle_count.load(Ordering::SeqCst);
                    if c == 1 && !self.swapped.swap(true, Ordering::SeqCst) {
                        // Swap mocks before cycle 2: v1 session now expired -> returns LOGIN_HTML,
                        // re-login issues v2 session -> returns JADWAL_GREEN_HTML.
                        if let Some(mut m) = self.m_get_1.lock().unwrap().take() {
                            m.delete();
                        }
                        if let Some(mut m) = self.m_login_1.lock().unwrap().take() {
                            m.delete();
                        }
                        self.server.mock(|when, then| {
                            when.method(GET)
                                .path("/ajar/absen")
                                .header("Cookie", "ci_session=v1");
                            then.status(200).body(LOGIN_HTML);
                        });
                        self.server.mock(|when, then| {
                            when.method(POST).path("/laman/login");
                            then.status(200)
                                .header("Set-Cookie", "ci_session=v2; Path=/")
                                .body("ok");
                        });
                        self.server.mock(|when, then| {
                            when.method(GET)
                                .path("/ajar/absen")
                                .header("Cookie", "ci_session=v2");
                            then.status(200).body(JADWAL_GREEN_HTML);
                        });
                    } else if c >= 2 {
                        let _ = self.shutdown_tx.send(());
                    }
                }
                Event::Log {
                    code,
                    level: LogLevel::Ok,
                    ..
                } if code == "LOGIN" => {
                    self.login_calls.fetch_add(1, Ordering::SeqCst);
                }
                _ => {}
            }
            self.events.lock().unwrap().push(event);
        }
    }

    let reporter = LoopReloginReporter {
        server: &server,
        m_login_1,
        m_get_1,
        cycle_count,
        swapped,
        login_calls: Arc::clone(&login_calls),
        shutdown_tx: tx,
        events: Mutex::new(Vec::new()),
    };

    run(&cfg, &client, true, &reporter, &rx).unwrap();

    assert_eq!(login_calls.load(Ordering::SeqCst), 2);
    let events = reporter.events.lock().unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Log { code, level: LogLevel::Warn, .. } if code == "SESSION"
    )));

    let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
}

#[test]
fn shutdown_signal_interrupts_countdown_sleep_quickly() {
    let server = MockServer::start();
    let state_path = temp_state_path("shutdown");
    let mut cfg = test_config(&server.base_url(), state_path.clone());
    cfg.interval = 60;
    let client = AbsenClient::new(&cfg.base_url).unwrap();

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200).body("ok");
    });
    server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(JADWAL_GREEN_HTML);
    });

    let (tx, rx) = mpsc::channel();
    let reporter = VecReporter::new();

    let start = Instant::now();
    std::thread::scope(|s| {
        let cfg_ref = &cfg;
        let client_ref = &client;
        let reporter_ref = &reporter;
        let handle = s.spawn(move || run(cfg_ref, client_ref, true, reporter_ref, &rx));
        std::thread::sleep(Duration::from_millis(60));
        tx.send(()).unwrap();
        handle.join().unwrap().unwrap();
    });
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(300),
        "shutdown took too long: {elapsed:?}"
    );

    let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
}

#[test]
fn dump_absen_html_writes_file_on_success_and_errors_on_login_fail() {
    use absen_core::DumpError;

    let server = MockServer::start();
    let state_path = temp_state_path("dump");
    let dump_path = state_path.parent().unwrap().join("absen-dump.html");
    let cfg = test_config(&server.base_url(), state_path.clone());
    let client = AbsenClient::new(&cfg.base_url).unwrap();

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200).body("ok");
    });
    let mut m_get = server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(JADWAL_ONE_BLUE_HTML);
    });

    let written = client.dump_absen_html(&cfg, &dump_path).unwrap();
    assert_eq!(written, dump_path);
    assert_eq!(
        std::fs::read_to_string(&dump_path).unwrap(),
        JADWAL_ONE_BLUE_HTML
    );

    m_get.delete();
    server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(LOGIN_HTML);
    });
    let err = client.dump_absen_html(&cfg, &dump_path).unwrap_err();
    assert!(matches!(err, DumpError::LoginFailed));

    let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
}

#[test]
fn default_constants_sanity() {
    assert_eq!(DEFAULT_INTERVAL, 60);
    assert_eq!(DEFAULT_MAX_RETRY, 5);
    assert!((DEFAULT_DELAY_MIN - 1.2).abs() < f64::EPSILON);
    assert!((DEFAULT_DELAY_MAX - 2.6).abs() < f64::EPSILON);
    assert!((DEFAULT_RETRY_WAIT - 2.5).abs() < f64::EPSILON);
    let _ = (Color::Blue, Action::clone, Local::now());
}

#[test]
fn run_with_commands_scan_now_triggers_immediate_cycle_and_stop_exits() {
    let server = MockServer::start();
    let state_path = temp_state_path("cmd_scan_now");
    let mut cfg = test_config(&server.base_url(), state_path.clone());
    cfg.interval = 60;
    let client = AbsenClient::new(&cfg.base_url).unwrap();

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200).body("ok");
    });
    let m_target = server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(JADWAL_GREEN_HTML);
    });

    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (ev_tx, ev_rx) = mpsc::channel();
    let reporter = absen_core::ChannelReporter::new(ev_tx);

    let start = Instant::now();
    std::thread::scope(|s| {
        let cfg_ref = &cfg;
        let client_ref = &client;
        let reporter_ref = &reporter;
        let handle =
            s.spawn(move || run_with_commands(cfg_ref, client_ref, true, reporter_ref, &cmd_rx));

        // Wait until first countdown begins (Cycle 1 completed).
        let mut saw_countdown = false;
        while let Ok(ev) = ev_rx.recv_timeout(Duration::from_secs(2)) {
            if matches!(ev, Event::Countdown(n) if n > 0) {
                saw_countdown = true;
                break;
            }
        }
        assert!(saw_countdown, "expected initial countdown event");

        // Trigger immediate scan.
        cmd_tx.send(EngineCommand::ScanNow).unwrap();

        // Expect Countdown(0) followed by a second CycleIncrement.
        let mut saw_zero = false;
        let mut cycles_after = 0usize;
        while let Ok(ev) = ev_rx.recv_timeout(Duration::from_secs(2)) {
            match ev {
                Event::Countdown(0) => saw_zero = true,
                Event::CycleIncrement => {
                    cycles_after += 1;
                    break;
                }
                _ => {}
            }
        }
        assert!(saw_zero, "ScanNow should emit Countdown(0)");
        assert_eq!(cycles_after, 1);

        // Wait briefly for second countdown, then stop cleanly.
        while let Ok(ev) = ev_rx.recv_timeout(Duration::from_secs(2)) {
            if matches!(ev, Event::Countdown(n) if n > 0) {
                break;
            }
        }
        cmd_tx.send(EngineCommand::Stop).unwrap();
        handle.join().unwrap().unwrap();
    });

    assert!(
        start.elapsed() < Duration::from_secs(2),
        "ScanNow + Stop took too long: {:?}",
        start.elapsed()
    );
    assert_eq!(m_target.hits(), 2);

    let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
}

#[test]
fn direct_post_success_emits_post_accepted_proof_without_extra_get() {
    let server = MockServer::start();
    let state_path = temp_state_path("proof_direct");
    let cfg = test_config(&server.base_url(), state_path.clone());
    let client = AbsenClient::new(&cfg.base_url).unwrap();

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200).body("ok");
    });
    let m_get = server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(JADWAL_ONE_BLUE_HTML);
    });
    let m_post = server.mock(|when, then| {
        when.method(POST).path("/ajar/absen/absensi_awal");
        then.status(200)
            .header("Date", "Sat, 21 Nov 2026 00:10:00 GMT")
            .body(r#"<div class="alert">{"status":"ok"}</div>"#);
    });

    let reporter = VecReporter::new();
    let (_tx, rx) = mpsc::channel();
    run(&cfg, &client, false, &reporter, &rx).unwrap();

    m_get.assert_hits(1);
    m_post.assert_hits(1);

    let proofs: Vec<_> = reporter
        .snapshot()
        .into_iter()
        .filter_map(|e| match e {
            Event::AttendanceProof(p) => Some(p),
            _ => None,
        })
        .collect();
    assert_eq!(proofs.len(), 1);
    let p = &proofs[0];
    assert_eq!(p.verification, ProofVerification::PostAccepted);
    assert_eq!(p.http_status, Some(200));
    assert_eq!(
        p.server_date_header.as_deref(),
        Some("Sat, 21 Nov 2026 00:10:00 GMT")
    );
    assert_eq!(p.server_response.as_deref(), Some(r#"{"status":"ok"}"#));
    assert_eq!(p.nim, "251511001");
    assert_eq!(p.student_nama, "Budi Santoso");
    assert_eq!(p.mk_code, "25IF201");
    assert_eq!(p.kls, "1A");
    assert_eq!(p.tp, "T");
    assert_eq!(p.ja, "1");
    assert_eq!(p.jb, "3");
    assert_eq!(p.attempt, Some(1));
    assert!(p.verified_at.is_none());

    let store = ProofStore::load(proof_path_for(&state_path));
    assert_eq!(store.len(), 1);

    let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
}

#[test]
fn ambiguous_post_confirmed_by_still_blue_emits_portal_verified_and_updated_jadwal() {
    let server = MockServer::start();
    let state_path = temp_state_path("proof_still_blue");
    let cfg = test_config(&server.base_url(), state_path.clone());
    let client = AbsenClient::new(&cfg.base_url).unwrap();

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200).body("ok");
    });
    let m_blue = server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(JADWAL_ONE_BLUE_HTML);
    });
    server.mock(|when, then| {
        when.method(POST).path("/ajar/absen/absensi_awal");
        then.status(200).body(r#"{"status":false}"#);
    });

    struct SwapToGreen<'a> {
        server: &'a MockServer,
        m_blue: Mutex<Option<httpmock::Mock<'a>>>,
        events: Mutex<Vec<Event>>,
    }
    impl<'a> Reporter for SwapToGreen<'a> {
        fn emit(&self, event: Event) {
            if let Event::Log { code, message, .. } = &event {
                if code == "ABSEN" && message.starts_with("target:") {
                    if let Some(mut m) = self.m_blue.lock().unwrap().take() {
                        m.delete();
                        self.server.mock(|when, then| {
                            when.method(GET).path("/ajar/absen");
                            then.status(200).body(JADWAL_GREEN_HTML);
                        });
                    }
                }
            }
            self.events.lock().unwrap().push(event);
        }
    }

    let reporter = SwapToGreen {
        server: &server,
        m_blue: Mutex::new(Some(m_blue)),
        events: Mutex::new(Vec::new()),
    };
    let (_tx, rx) = mpsc::channel();
    run(&cfg, &client, false, &reporter, &rx).unwrap();

    let events = reporter.events.lock().unwrap();
    let jadwal_events: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Jadwal { actions, skips } => Some((actions, skips)),
            _ => None,
        })
        .collect();
    assert_eq!(jadwal_events.len(), 2);
    assert!(jadwal_events[1].0.is_empty());
    assert_eq!(jadwal_events[1].1[0].color, Color::Green);

    let proofs: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::AttendanceProof(p) => Some(p),
            _ => None,
        })
        .collect();
    assert_eq!(proofs.len(), 1);
    assert_eq!(proofs[0].verification, ProofVerification::PortalVerified);
    assert!(proofs[0].verified_at.is_some());
    assert_eq!(
        proofs[0].server_button_label.as_deref(),
        Some("Sudah Absen")
    );

    let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
}

#[test]
fn next_scan_seeing_green_upgrades_post_accepted_to_portal_verified_once() {
    let server = MockServer::start();
    let state_path = temp_state_path("proof_upgrade_scan");
    let mut cfg = test_config(&server.base_url(), state_path.clone());
    cfg.interval = 0;
    let client = AbsenClient::new(&cfg.base_url).unwrap();

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200).body("ok");
    });
    let m_blue = server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(JADWAL_ONE_BLUE_HTML);
    });
    server.mock(|when, then| {
        when.method(POST).path("/ajar/absen/absensi_awal");
        then.status(200).body("ok");
    });

    struct MultiCycleUpgrade<'a> {
        server: &'a MockServer,
        m_blue: Mutex<Option<httpmock::Mock<'a>>>,
        cycles: AtomicUsize,
        shutdown_tx: mpsc::Sender<()>,
        events: Mutex<Vec<Event>>,
    }
    impl<'a> Reporter for MultiCycleUpgrade<'a> {
        fn emit(&self, event: Event) {
            if matches!(event, Event::LastScan(_)) {
                let c = self.cycles.fetch_add(1, Ordering::SeqCst) + 1;
                if c == 1 {
                    if let Some(mut m) = self.m_blue.lock().unwrap().take() {
                        m.delete();
                        self.server.mock(|when, then| {
                            when.method(GET).path("/ajar/absen");
                            then.status(200).body(JADWAL_GREEN_HTML);
                        });
                    }
                } else if c >= 3 {
                    let _ = self.shutdown_tx.send(());
                }
            }
            self.events.lock().unwrap().push(event);
        }
    }

    let (tx, rx) = mpsc::channel();
    let reporter = MultiCycleUpgrade {
        server: &server,
        m_blue: Mutex::new(Some(m_blue)),
        cycles: AtomicUsize::new(0),
        shutdown_tx: tx,
        events: Mutex::new(Vec::new()),
    };
    run(&cfg, &client, true, &reporter, &rx).unwrap();

    let events = reporter.events.lock().unwrap();
    let proofs: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::AttendanceProof(p) => Some(p),
            _ => None,
        })
        .collect();
    // Emitted once as PostAccepted on cycle 1, then once as PortalVerified on cycle 2; cycle 3 emits nothing.
    assert_eq!(proofs.len(), 2);
    assert_eq!(proofs[0].verification, ProofVerification::PostAccepted);
    assert_eq!(proofs[1].verification, ProofVerification::PortalVerified);
    assert_eq!(proofs[0].proof_id, proofs[1].proof_id);
    assert_eq!(
        proofs[1].server_button_label.as_deref(),
        Some("Sudah Absen")
    );

    let store = ProofStore::load(proof_path_for(&state_path));
    assert_eq!(store.len(), 1);
    assert_eq!(
        store.all()[0].verification,
        ProofVerification::PortalVerified
    );

    let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
}

#[test]
fn already_green_row_emits_observed_present_once_without_writing_proofs_file() {
    let server = MockServer::start();
    let state_path = temp_state_path("proof_observed_green");
    let mut cfg = test_config(&server.base_url(), state_path.clone());
    cfg.interval = 0;
    let client = AbsenClient::new(&cfg.base_url).unwrap();

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200).body("ok");
    });
    server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(JADWAL_GREEN_HTML);
    });

    struct StopAfterTwoScans {
        scans: AtomicUsize,
        tx: mpsc::Sender<()>,
        events: Mutex<Vec<Event>>,
    }
    impl Reporter for StopAfterTwoScans {
        fn emit(&self, event: Event) {
            if matches!(event, Event::LastScan(_))
                && self.scans.fetch_add(1, Ordering::SeqCst) + 1 >= 2
            {
                let _ = self.tx.send(());
            }
            self.events.lock().unwrap().push(event);
        }
    }

    let (tx, rx) = mpsc::channel();
    let reporter = StopAfterTwoScans {
        scans: AtomicUsize::new(0),
        tx,
        events: Mutex::new(Vec::new()),
    };
    run(&cfg, &client, true, &reporter, &rx).unwrap();

    let events = reporter.events.lock().unwrap();
    let proofs: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::AttendanceProof(p) => Some(p),
            _ => None,
        })
        .collect();
    assert_eq!(proofs.len(), 1);
    assert_eq!(proofs[0].verification, ProofVerification::ObservedPresent);
    assert_eq!(
        proofs[0].server_button_label.as_deref(),
        Some("Sudah Absen")
    );
    assert!(!proof_path_for(&state_path).exists());

    let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
}

#[test]
fn restart_reloads_persisted_todays_proofs() {
    let server = MockServer::start();
    let state_path = temp_state_path("proof_restart");
    let cfg = test_config(&server.base_url(), state_path.clone());
    let client = AbsenClient::new(&cfg.base_url).unwrap();

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200).body("ok");
    });
    let mut m_get = server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(JADWAL_ONE_BLUE_HTML);
    });
    server.mock(|when, then| {
        when.method(POST).path("/ajar/absen/absensi_awal");
        then.status(200).body("ok");
    });

    // Run 1: click and persist PostAccepted.
    let rep1 = VecReporter::new();
    let (_tx1, rx1) = mpsc::channel();
    run(&cfg, &client, false, &rep1, &rx1).unwrap();

    // Run 2 (restart): page still blue -> skipped via ClickedState, startup still emits persisted proof.
    m_get.delete();
    server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(JADWAL_ONE_BLUE_HTML);
    });
    let rep2 = VecReporter::new();
    let (_tx2, rx2) = mpsc::channel();
    run(&cfg, &client, false, &rep2, &rx2).unwrap();

    let proofs2: Vec<_> = rep2
        .snapshot()
        .into_iter()
        .filter_map(|e| match e {
            Event::AttendanceProof(p) => Some(p),
            _ => None,
        })
        .collect();
    assert_eq!(proofs2.len(), 1);
    assert_eq!(proofs2[0].verification, ProofVerification::PostAccepted);
    assert_eq!(proofs2[0].nama_mk, "Struktur Data");

    let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
}

#[test]
fn startup_emits_retained_history_oldest_first_and_scopes_green_upgrade_to_today() {
    let server = MockServer::start();
    let state_path = temp_state_path("proof_history_scoped");
    let cfg = test_config(&server.base_url(), state_path.clone());
    let client = AbsenClient::new(&cfg.base_url).unwrap();

    let today = Local::now().date_naive();
    let two_days_ago = today - ChronoDuration::days(2);
    let yesterday = today - ChronoDuration::days(1);
    let today_str = today.format("%Y-%m-%d").to_string();
    let two_days_ago_str = two_days_ago.format("%Y-%m-%d").to_string();
    let yesterday_str = yesterday.format("%Y-%m-%d").to_string();

    let shared_key = "Struktur Data|KO078N-Trisna Gelar Abdillah, S.T., M.Kom";
    let shared_sig = format!("{shared_key}|07:00:00 s.d. 09:30:00");

    let mk_seeded =
        |date_str: &str, nama_mk: &str, key: &str, sig: &str, verification: ProofVerification| {
            AttendanceProof {
                proof_id: generate_proof_id(date_str, key, &format!("{date_str}T07:05:00+07:00")),
                sig: sig.to_string(),
                key: key.to_string(),
                date: date_str.to_string(),
                recorded_at: format!("{date_str}T07:05:00+07:00"),
                verified_at: (verification == ProofVerification::PortalVerified)
                    .then(|| format!("{date_str}T07:06:00+07:00")),
                server_date_header: Some("Thu, 19 Nov 2026 00:05:00 GMT".to_string()),
                verification,
                nim: "251511001".to_string(),
                student_nama: "Budi Santoso".to_string(),
                jurusan: "D3-Teknik Informatika".to_string(),
                nama_mk: nama_mk.to_string(),
                mk_code: "25IF201".to_string(),
                dosen: "KO078N-Trisna Gelar Abdillah, S.T., M.Kom".to_string(),
                jam: "07:00:00 s.d. 09:30:00".to_string(),
                tp: "T".to_string(),
                ja: "1".to_string(),
                jb: "3".to_string(),
                kls: "1A".to_string(),
                target_url: cfg.target_url.clone(),
                endpoint_url: Some(format!("{}/ajar/absen/absensi_awal", server.base_url())),
                http_status: Some(200),
                server_response: Some("ok".to_string()),
                server_button_label: (verification == ProofVerification::PortalVerified)
                    .then(|| "Sudah Absen".to_string()),
                attempt: Some(1),
            }
        };

    // Seed yesterday's PostAccepted (same course/sig as today's JADWAL_GREEN_HTML)
    // before an older PortalVerified record to verify oldest-to-newest sorting too.
    let mut store = ProofStore::load(proof_path_for(&state_path));
    assert!(store
        .upsert_and_save(mk_seeded(
            &yesterday_str,
            "Struktur Data",
            shared_key,
            &shared_sig,
            ProofVerification::PostAccepted,
        ))
        .unwrap());
    assert!(store
        .upsert_and_save(mk_seeded(
            &two_days_ago_str,
            "Basis Data",
            "Basis Data|KO099N",
            "Basis Data|KO099N|09:30:00 s.d. 11:30:00",
            ProofVerification::PortalVerified,
        ))
        .unwrap());

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200).body("ok");
    });
    server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(JADWAL_GREEN_HTML);
    });

    let reporter = VecReporter::new();
    let (_tx, rx) = mpsc::channel();
    run(&cfg, &client, false, &reporter, &rx).unwrap();

    let emitted_proofs: Vec<_> = reporter
        .snapshot()
        .into_iter()
        .filter_map(|e| match e {
            Event::AttendanceProof(p) => Some(p),
            _ => None,
        })
        .collect();

    assert_eq!(emitted_proofs.len(), 3);
    // Startup emits retained history oldest-to-newest:
    assert_eq!(emitted_proofs[0].date, two_days_ago_str);
    assert_eq!(emitted_proofs[0].nama_mk, "Basis Data");
    assert_eq!(
        emitted_proofs[0].verification,
        ProofVerification::PortalVerified
    );

    assert_eq!(emitted_proofs[1].date, yesterday_str);
    assert_eq!(emitted_proofs[1].nama_mk, "Struktur Data");
    assert_eq!(
        emitted_proofs[1].verification,
        ProofVerification::PostAccepted
    );

    // Today's green row for the same course must NOT upgrade yesterday's PostAccepted;
    // it emits an ObservedPresent proof scoped to today.
    assert_eq!(emitted_proofs[2].date, today_str);
    assert_eq!(emitted_proofs[2].nama_mk, "Struktur Data");
    assert_eq!(
        emitted_proofs[2].verification,
        ProofVerification::ObservedPresent
    );

    let reloaded = ProofStore::load(proof_path_for(&state_path));
    assert_eq!(reloaded.len(), 2);
    let yesterday_stored = reloaded.get_by_sig(&yesterday_str, &shared_sig).unwrap();
    assert_eq!(
        yesterday_stored.verification,
        ProofVerification::PostAccepted
    );
    assert!(reloaded.get_by_sig(&today_str, &shared_sig).is_none());

    let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
}

#[test]
fn portal_snapshot_emitted_in_run_once_and_verify_after_post_with_zero_extra_gets() {
    // Part 1: Green page during initial login/run_once emits 1 snapshot with has_green_row = true and 1 GET hit.
    {
        let server = MockServer::start();
        let state_path = temp_state_path("snapshot_green");
        let cfg = test_config(&server.base_url(), state_path.clone());
        let client = AbsenClient::new(&cfg.base_url).unwrap();

        server.mock(|when, then| {
            when.method(POST).path("/laman/login");
            then.status(200).body("ok");
        });
        let m_get = server.mock(|when, then| {
            when.method(GET).path("/ajar/absen");
            then.status(200).body(JADWAL_GREEN_HTML);
        });

        let reporter = VecReporter::new();
        let (_tx, rx) = mpsc::channel();
        run(&cfg, &client, false, &reporter, &rx).unwrap();

        m_get.assert_hits(1);
        let snapshots: Vec<PortalPageSnapshot> = reporter
            .snapshot()
            .into_iter()
            .filter_map(|e| match e {
                Event::PortalSnapshot(s) => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].html, JADWAL_GREEN_HTML);
        assert_eq!(snapshots[0].base_url, cfg.base_url);
        assert_eq!(snapshots[0].target_url, cfg.target_url);
        assert!(snapshots[0].has_green_row);
        assert!(!snapshots[0].captured_at.is_empty());

        let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
    }

    // Part 2: Ambiguous POST triggers 1 verify_after_post GET (total 2 GETs) and emits 2 snapshots:
    // first with has_green_row = false (blue HTML), second with has_green_row = true (green HTML).
    {
        let server = MockServer::start();
        let state_path = temp_state_path("snapshot_verify");
        let cfg = test_config(&server.base_url(), state_path.clone());
        let client = AbsenClient::new(&cfg.base_url).unwrap();

        server.mock(|when, then| {
            when.method(POST).path("/laman/login");
            then.status(200).body("ok");
        });
        let m_blue = server.mock(|when, then| {
            when.method(GET).path("/ajar/absen");
            then.status(200).body(JADWAL_ONE_BLUE_HTML);
        });
        let m_post = server.mock(|when, then| {
            when.method(POST).path("/ajar/absen/absensi_awal");
            then.status(200).body(r#"{"status":false}"#);
        });

        struct SwapToGreenSnapshot<'a> {
            server: &'a MockServer,
            m_blue: Mutex<Option<httpmock::Mock<'a>>>,
            m_green: Mutex<Option<httpmock::Mock<'a>>>,
            events: Mutex<Vec<Event>>,
        }
        impl<'a> Reporter for SwapToGreenSnapshot<'a> {
            fn emit(&self, event: Event) {
                if let Event::Log { code, message, .. } = &event {
                    if code == "ABSEN" && message.starts_with("target:") {
                        if let Some(mut m) = self.m_blue.lock().unwrap().take() {
                            m.assert_hits(1);
                            m.delete();
                            let green = self.server.mock(|when, then| {
                                when.method(GET).path("/ajar/absen");
                                then.status(200).body(JADWAL_GREEN_HTML);
                            });
                            *self.m_green.lock().unwrap() = Some(green);
                        }
                    }
                }
                self.events.lock().unwrap().push(event);
            }
        }

        let reporter = SwapToGreenSnapshot {
            server: &server,
            m_blue: Mutex::new(Some(m_blue)),
            m_green: Mutex::new(None),
            events: Mutex::new(Vec::new()),
        };
        let (_tx, rx) = mpsc::channel();
        run(&cfg, &client, false, &reporter, &rx).unwrap();

        m_post.assert_hits(1);
        reporter
            .m_green
            .lock()
            .unwrap()
            .as_ref()
            .expect("green mock installed")
            .assert_hits(1);

        let snapshots: Vec<PortalPageSnapshot> = reporter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                Event::PortalSnapshot(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(snapshots.len(), 2);

        assert_eq!(snapshots[0].html, JADWAL_ONE_BLUE_HTML);
        assert_eq!(snapshots[0].base_url, cfg.base_url);
        assert_eq!(snapshots[0].target_url, cfg.target_url);
        assert!(!snapshots[0].has_green_row);

        assert_eq!(snapshots[1].html, JADWAL_GREEN_HTML);
        assert_eq!(snapshots[1].base_url, cfg.base_url);
        assert_eq!(snapshots[1].target_url, cfg.target_url);
        assert!(snapshots[1].has_green_row);

        let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
    }
}

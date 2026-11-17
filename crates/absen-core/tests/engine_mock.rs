use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use absen_core::config::{
    Config, DEFAULT_DELAY_MAX, DEFAULT_DELAY_MIN, DEFAULT_ENGINE, DEFAULT_INTERVAL,
    DEFAULT_MAX_RETRY, DEFAULT_RETRY_WAIT,
};
use absen_core::engine::{run, EngineError};
use absen_core::event::{Event, Reporter, VecReporter};
use absen_core::state::ClickedState;
use absen_core::{AbsenClient, Action, Color, LogLevel};
use chrono::Local;
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

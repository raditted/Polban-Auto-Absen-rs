use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use absen_core::config::{Config, DEFAULT_ENGINE};
use absen_core::{Color, UserInfo};
use absen_gui::bridge::{EngineController, EngineStatus, GuiSnapshot};
use absen_gui::platform::Notifier;
use httpmock::prelude::*;

static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

const LOGIN_HTML: &str = r#"<!DOCTYPE html>
<html lang="id">
<head><title>Login Akademik</title></head>
<body>
  <form id="fm" action="/laman/login" method="post">
    <input type="text" name="username" placeholder="NIM">
    <input type="password" name="password" placeholder="Password">
    <button type="submit" name="submit" value="">Masuk</button>
  </form>
</body>
</html>"#;

const JADWAL_MIXED_HTML: &str = r#"<!DOCTYPE html>
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
      <tr>
        <td>2</td>
        <td>BD011N-Budi Santoso</td>
        <td>25IF202</td>
        <td>Basis Data</td>
        <td>P</td>
        <td>2</td>
        <td>4</td>
        <td>10:00:00 s.d. 12:00:00</td>
        <td>
          <button class="btn btn-warning">Belum Buka</button>
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

fn temp_dir(name: &str) -> PathBuf {
    let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "polban_absen_gui_svc_{name}_{}_{seq}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn test_config(base_url: &str, state_file: PathBuf, interval: u64) -> Config {
    Config {
        username: "251511001".to_string(),
        password: "secret#pass".to_string(),
        base_url: base_url.to_string(),
        target_url: format!("{base_url}/ajar/absen"),
        interval,
        delay_min: 0.0,
        delay_max: 0.0,
        max_retry: 2,
        retry_wait: 0.0,
        engine: DEFAULT_ENGINE.to_string(),
        env_path_used: None,
        state_file,
    }
}

fn wait_until(
    timeout: Duration,
    mut pred: impl FnMut(&GuiSnapshot) -> bool,
    ctrl: &EngineController,
) -> GuiSnapshot {
    let start = Instant::now();
    loop {
        let snap = ctrl.snapshot();
        if pred(&snap) {
            return snap;
        }
        assert!(
            start.elapsed() < timeout,
            "timed out waiting for snapshot condition; last snapshot: {snap:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn engine_controller_login_marks_blue_green_and_scan_now_wakes_fast() {
    let server = MockServer::start();
    let dir = temp_dir("e2e_scan_now");
    let cfg = test_config(&server.base_url(), dir.join("clicked.json"), 30);

    let m_login = server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200)
            .header("Set-Cookie", "ci_session=gui1; Path=/")
            .body("ok");
    });
    let m_target = server.mock(|when, then| {
        when.method(GET)
            .path("/ajar/absen")
            .header("Cookie", "ci_session=gui1");
        then.status(200).body(JADWAL_MIXED_HTML);
    });
    let m_absen = server.mock(|when, then| {
        when.method(POST)
            .path("/ajar/absen/absensi_awal")
            .header("X-Requested-With", "XMLHttpRequest");
        then.status(200).body("ok");
    });

    let wake_count = Arc::new(AtomicUsize::new(0));
    let wake_flag = Arc::clone(&wake_count);

    let mut controller = EngineController::new();
    controller.set_notifier(Notifier::new(false));
    controller
        .start(
            cfg,
            true,
            Some(Box::new(move || {
                wake_flag.fetch_add(1, Ordering::SeqCst);
            })),
        )
        .unwrap();

    let snap = wait_until(
        Duration::from_secs(5),
        |s| s.cycle >= 1 && s.countdown_secs > 0,
        &controller,
    );

    assert_eq!(
        snap.user,
        Some(UserInfo {
            nim: "251511001".to_string(),
            nama: "Budi Santoso".to_string(),
            jurusan: "D3-Teknik Informatika".to_string(),
        })
    );
    assert_eq!(snap.courses.len(), 2);

    let first = &snap.courses[0];
    assert_eq!(first.nama_mk, "Struktur Data");
    assert_eq!(first.dosen, "Trisna Gelar Abdillah, S.T., M.Kom");
    assert_eq!(first.color, Color::Green);
    assert_eq!(first.status_label, "Sudah Absen");
    assert!(!first.action_ready);

    let second = &snap.courses[1];
    assert_eq!(second.nama_mk, "Basis Data");
    assert_eq!(second.dosen, "Budi Santoso");
    assert_eq!(second.color, Color::Orange);
    assert_eq!(second.status_label, "Belum Buka");
    assert!(!second.action_ready);
    assert!(!second.reason.contains(concat!(" ", "-", "-", " ")));
    assert!(!second.reason.contains('\u{2014}'));

    assert!(!snap.activities.is_empty());
    for item in &snap.activities {
        assert!(!item.message.contains(concat!(" ", "-", "-", " ")));
        assert!(!item.message.contains('\u{2014}'));
    }

    m_login.assert_hits(1);
    m_absen.assert_hits(1);
    assert!(wake_count.load(Ordering::SeqCst) > 0);

    // Trigger ScanNow during countdown and verify it wakes quickly (< 300ms).
    let scan_start = Instant::now();
    assert!(controller.scan_now());
    let snap2 = wait_until(Duration::from_secs(2), |s| s.cycle >= 2, &controller);
    let wake_elapsed = scan_start.elapsed();
    assert!(
        wake_elapsed < Duration::from_millis(300),
        "ScanNow took too long to wake next cycle: {wake_elapsed:?}"
    );

    // Wait for cycle 2 scan to finish and verify sticky green persists even when HTML still has blue button.
    let snap2_done = wait_until(
        Duration::from_secs(2),
        |s| s.cycle >= 2 && s.countdown_secs > 0,
        &controller,
    );
    assert_eq!(snap2_done.courses[0].color, Color::Green);
    assert!(!snap2_done.courses[0].action_ready);
    m_absen.assert_hits(1);
    assert!(m_target.hits() >= 2);
    assert!(snap2.cycle >= 2);

    controller.stop();
    assert!(!controller.is_running());
    let stopped_snap = controller.snapshot();
    assert_eq!(stopped_snap.status, EngineStatus::Stopped);
    assert_eq!(stopped_snap.countdown_secs, 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn engine_controller_login_failure_sets_auth_failed() {
    let server = MockServer::start();
    let dir = temp_dir("auth_failed");
    let cfg = test_config(&server.base_url(), dir.join("clicked.json"), 30);

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200).body("invalid");
    });
    server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(LOGIN_HTML);
    });

    let mut controller = EngineController::new();
    controller.set_notifier(Notifier::new(false));
    controller.start(cfg, true, None).unwrap();

    let start = Instant::now();
    while controller.is_running() && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!controller.is_running());

    let snap = controller.snapshot();
    assert_eq!(snap.status, EngineStatus::AuthFailed);
    let err = snap.last_error.as_deref().unwrap_or("");
    assert!(err.contains("Login gagal"), "unexpected last_error: {err}");
    assert!(!err.contains(concat!(" ", "-", "-", " ")));
    assert!(!err.contains('\u{2014}'));
    assert!(!snap.auth_status.contains(concat!(" ", "-", "-", " ")));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hidden_window_suppresses_countdown_wake_ui() {
    let server = MockServer::start();
    let dir = temp_dir("hidden_tray");
    let cfg = test_config(&server.base_url(), dir.join("clicked.json"), 10);

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200)
            .header("Set-Cookie", "ci_session=hidden1; Path=/")
            .body("ok");
    });
    server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(JADWAL_GREEN_HTML);
    });

    let wake_count = Arc::new(AtomicUsize::new(0));
    let wake_flag = Arc::clone(&wake_count);

    let mut controller = EngineController::new();
    controller.set_notifier(Notifier::new(false));
    assert!(controller.is_window_visible());

    controller
        .start(
            cfg,
            true,
            Some(Box::new(move || {
                wake_flag.fetch_add(1, Ordering::SeqCst);
            })),
        )
        .unwrap();

    let initial = wait_until(
        Duration::from_secs(5),
        |s| s.cycle >= 1 && s.countdown_secs >= 8,
        &controller,
    );
    controller.set_window_visible(false);
    assert!(!controller.is_window_visible());

    // Allow any event already in flight to settle before sampling wake_count.
    std::thread::sleep(Duration::from_millis(80));
    let count_before = wake_count.load(Ordering::SeqCst);
    let countdown_before = controller.snapshot().countdown_secs;

    // Sleep long enough for multiple 1-second Countdown ticks while hidden.
    std::thread::sleep(Duration::from_millis(2300));
    let count_after = wake_count.load(Ordering::SeqCst);
    let countdown_after = controller.snapshot().countdown_secs;

    assert!(
        countdown_after < countdown_before && countdown_before <= initial.countdown_secs,
        "expected countdown to tick in snapshot: before={countdown_before}, after={countdown_after}"
    );
    assert_eq!(
        count_after, count_before,
        "wake_ui must not be called on Countdown ticks when window_visible is false"
    );

    controller.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dump_diagnostic_html_writes_portal_html_and_handles_login_failure() {
    let server = MockServer::start();
    let dir = temp_dir("dump_html");
    let cfg = test_config(&server.base_url(), dir.join("clicked.json"), 30);

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200)
            .header("Set-Cookie", "ci_session=dump1; Path=/")
            .body("ok");
    });
    server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(JADWAL_GREEN_HTML);
    });

    let out_path = dir.join("diagnostik.html");
    let written = EngineController::dump_diagnostic_html(&cfg, &out_path).unwrap();
    assert_eq!(written, out_path);
    assert_eq!(
        std::fs::read_to_string(&out_path).unwrap(),
        JADWAL_GREEN_HTML
    );

    // Verify failure path cleans error text (no double-dash or em dash).
    let fail_server = MockServer::start();
    fail_server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200).body("bad");
    });
    fail_server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(LOGIN_HTML);
    });
    let fail_cfg = test_config(&fail_server.base_url(), dir.join("clicked2.json"), 30);
    let err =
        EngineController::dump_diagnostic_html(&fail_cfg, &dir.join("fail.html")).unwrap_err();
    assert!(!err.contains(concat!(" ", "-", "-", " ")));
    assert!(!err.contains('\u{2014}'));

    let _ = std::fs::remove_dir_all(&dir);
}

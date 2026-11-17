use std::path::PathBuf;
use std::process::Command;

use httpmock::prelude::*;

const BIN: &str = env!("CARGO_BIN_EXE_polban-absen");

const LOGIN_HTML: &str = r#"<html><body>
  <form id="fm"><input name="username"><input name="password"></form>
</body></html>"#;

const JADWAL_HTML: &str = r#"<!DOCTYPE html>
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

fn make_temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("polban_absen_cli_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn help_lists_all_expected_flags() {
    let out = Command::new(BIN).arg("--help").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for flag in [
        "--engine",
        "--once",
        "--dump",
        "--plain",
        "--no-alt-screen",
        "--ascii",
    ] {
        assert!(
            stdout.contains(flag),
            "missing flag {flag} in --help:\n{stdout}"
        );
    }
}

#[test]
fn engine_pw_exits_1_with_clear_message() {
    let out = Command::new(BIN).args(["--engine", "pw"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("pw"));
    assert!(stderr.contains("tidak didukung"));
}

#[test]
fn empty_credentials_exit_1_with_indonesian_error() {
    let dir = make_temp_dir("empty_creds");
    let env_file = dir.join(".env");
    std::fs::write(&env_file, "POLBAN_USERNAME=\nPOLBAN_PASSWORD=\n").unwrap();

    let out = Command::new(BIN)
        .arg("--plain")
        .arg("--once")
        .env("POLBAN_ENV", &env_file)
        .env_remove("POLBAN_USERNAME")
        .env_remove("POLBAN_PASSWORD")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("POLBAN_USERNAME / POLBAN_PASSWORD belum terisi"));
    assert!(stderr.contains("tapi nilai kosong"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn plain_once_runs_against_mock_server_and_exits_0() {
    let server = MockServer::start();
    let dir = make_temp_dir("plain_once");
    let env_file = dir.join(".env");
    let state_file = dir.join(".clicked.json");

    std::fs::write(
        &env_file,
        format!(
            "POLBAN_USERNAME=251511001\n\
             POLBAN_PASSWORD=secret#pass\n\
             POLBAN_TARGET_URL={}/ajar/absen\n\
             POLBAN_DELAY_MIN=0\n\
             POLBAN_DELAY_MAX=0\n\
             POLBAN_RETRY_WAIT=0\n",
            server.base_url()
        ),
    )
    .unwrap();

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200).body("ok");
    });
    server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(JADWAL_HTML);
    });
    server.mock(|when, then| {
        when.method(POST).path("/ajar/absen/absensi_awal");
        then.status(200).body("ok");
    });

    let out = Command::new(BIN)
        .args(["--plain", "--once"])
        .env("POLBAN_ENV", &env_file)
        .env("POLBAN_STATE_FILE", &state_file)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stderr: {stderr}\nstdout: {stdout}");
    assert!(stdout.contains("Polban Auto Absen"));
    assert!(stdout.contains("Budi Santoso"));
    assert!(stdout.contains("Jadwal hari ini:"));
    assert!(stdout.contains("Struktur Data"));
    assert!(stdout.contains("[OK] [ABSEN] BERHASIL"));
    assert!(stdout.contains("Ringkasan: 1 siklus | 1 berhasil | 0 gagal"));
    assert!(stdout.contains("[AutoAbsen] Selesai (sekali jalan)."));
    assert!(state_file.is_file());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn login_failure_exits_2() {
    let server = MockServer::start();
    let dir = make_temp_dir("login_fail_cli");
    let env_file = dir.join(".env");
    let state_file = dir.join(".clicked.json");

    std::fs::write(
        &env_file,
        format!(
            "POLBAN_USERNAME=251511001\n\
             POLBAN_PASSWORD=wrong\n\
             POLBAN_TARGET_URL={}/ajar/absen\n",
            server.base_url()
        ),
    )
    .unwrap();

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200).body("bad");
    });
    server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(LOGIN_HTML);
    });

    let out = Command::new(BIN)
        .args(["--plain", "--once"])
        .env("POLBAN_ENV", &env_file)
        .env("POLBAN_STATE_FILE", &state_file)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[!!] [LOGIN] GAGAL"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dump_writes_custom_html_file() {
    let server = MockServer::start();
    let dir = make_temp_dir("dump_cli");
    let env_file = dir.join(".env");
    let dump_file = dir.join("custom-dump.html");

    std::fs::write(
        &env_file,
        format!(
            "POLBAN_USERNAME=251511001\n\
             POLBAN_PASSWORD=secret\n\
             POLBAN_TARGET_URL={}/ajar/absen\n",
            server.base_url()
        ),
    )
    .unwrap();

    server.mock(|when, then| {
        when.method(POST).path("/laman/login");
        then.status(200).body("ok");
    });
    server.mock(|when, then| {
        when.method(GET).path("/ajar/absen");
        then.status(200).body(JADWAL_HTML);
    });

    let out = Command::new(BIN)
        .arg("--dump")
        .arg(&dump_file)
        .env("POLBAN_ENV", &env_file)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(std::fs::read_to_string(&dump_file).unwrap(), JADWAL_HTML);

    let _ = std::fs::remove_dir_all(&dir);
}

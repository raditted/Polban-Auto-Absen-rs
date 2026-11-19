use absen_core::parser::{find_rows, is_login_page, parse_user_header};
use absen_core::text::{nama_dosen, split_sig};
use absen_core::Color;

const LOGIN_HTML: &str = include_str!("fixtures/login.html");
const MIXED_HTML: &str = include_str!("fixtures/jadwal_mixed.html");
const SAME_MK_HTML: &str = include_str!("fixtures/jadwal_same_mk_diff_dosen.html");
const ENDPOINT: &str = "http://127.0.0.1:9999/ajar/absen/absensi_awal";

#[test]
fn detects_login_and_non_login_pages() {
    assert!(is_login_page(LOGIN_HTML));
    assert!(!is_login_page(MIXED_HTML));
    assert!(!is_login_page(SAME_MK_HTML));
}

#[test]
fn parses_user_header_variants() {
    let info = parse_user_header(MIXED_HTML, "000");
    assert_eq!(info.nim, "251511001");
    assert_eq!(info.nama, "Budi Santoso");
    assert_eq!(info.jurusan, "D3-Teknik Informatika");

    let info2 = parse_user_header(SAME_MK_HTML, "000");
    assert_eq!(info2.nim, "251524099");
    assert_eq!(info2.nama, "Ani Lestari");
    assert_eq!(info2.jurusan, "Teknik Komputer dan Informatika");

    let d4_html = r#"<div class="user-header">251524001 Citra Dewi D4 Teknik Informatika</div>"#;
    let info3 = parse_user_header(d4_html, "fallback");
    assert_eq!(info3.nim, "251524001");
    assert_eq!(info3.nama, "Citra Dewi");
    assert_eq!(info3.jurusan, "D4 Teknik Informatika");

    let no_nim_html = r#"<div class="user-header">Dosen Tamu</div>"#;
    let info4 = parse_user_header(no_nim_html, "251511001");
    assert_eq!(info4.nim, "251511001");
    assert_eq!(info4.nama, "Dosen Tamu");
    assert_eq!(info4.jurusan, "");
}

#[test]
fn parses_mixed_schedule_colors_disabled_nav_and_form_fields() {
    let (actions, skips) = find_rows(MIXED_HTML, ENDPOINT);

    assert_eq!(actions.len(), 2);
    assert_eq!(skips.len(), 5);

    // Row 1: btn-primary -> Action with exact form fields
    let a1 = &actions[0];
    assert_eq!(a1.url, ENDPOINT);
    assert_eq!(a1.nama_mk, "Struktur Data");
    assert_eq!(a1.dosen, "KO078N-Trisna Gelar Abdillah, S.T., M.Kom");
    assert_eq!(a1.jam, "07:00:00 s.d. 09:30:00");
    assert_eq!(a1.data.ja, "1");
    assert_eq!(a1.data.jb, "3");
    assert_eq!(a1.data.mk, "25IF201");
    assert_eq!(a1.data.dsn, "KO078N");
    assert_eq!(a1.data.tp, "T");
    assert_eq!(a1.data.kls, "1A");
    assert_eq!(
        a1.sig,
        "Struktur Data|KO078N-Trisna Gelar Abdillah, S.T., M.Kom|07:00:00 s.d. 09:30:00"
    );
    assert_eq!(
        a1.key,
        "Struktur Data|KO078N-Trisna Gelar Abdillah, S.T., M.Kom"
    );
    assert_eq!(a1.label, "Struktur Data (07:00:00 s.d. 09:30:00)");

    // Row 2: btn-info -> also Blue
    let a2 = &actions[1];
    assert_eq!(a2.nama_mk, "Basis Data");
    assert_eq!(a2.data.dsn, "BD011N");
    assert_eq!(a2.data.kls, "1B");

    // Skips: Green, Orange, Red, Disabled Blue, Unknown (nav/logout row excluded)
    assert_eq!(skips[0].color, Color::Green);
    assert_eq!(skips[0].reason, "sudah absen (dihitung berhasil)");
    assert_eq!(
        split_sig(&skips[0].sig),
        (
            "Matematika Diskrit",
            "KO012N-Andi Wijaya",
            "13:00:00 s.d. 14:40:00"
        )
    );

    assert_eq!(skips[1].color, Color::Orange);
    assert_eq!(
        skips[1].reason,
        "belum buka -- dosen belum membuka sesi absen"
    );

    assert_eq!(skips[2].color, Color::Red);
    assert_eq!(skips[2].reason, "belum waktunya -- di luar jam absensi");

    assert_eq!(skips[3].color, Color::Blue);
    assert_eq!(skips[3].reason, "disabled");

    assert_eq!(skips[4].color, Color::Unknown);
    assert_eq!(
        skips[4].reason,
        "warna tidak dikenali -- dilewati demi keamanan"
    );
}

#[test]
fn differentiates_same_course_by_lecturer_and_uses_fallback_table() {
    let (actions, skips) = find_rows(SAME_MK_HTML, ENDPOINT);
    assert!(skips.is_empty());
    assert_eq!(actions.len(), 2);
    assert_ne!(actions[0].sig, actions[1].sig);
    assert_ne!(actions[0].key, actions[1].key);
    assert_eq!(actions[0].data.dsn, "KO078N");
    assert_eq!(actions[1].data.dsn, "BD011N");
    assert_eq!(nama_dosen(&actions[1].dosen), "Ana-Maria");
}

#[test]
fn parses_unicode_and_short_rows_safely_without_panicking() {
    let html = r#"<!DOCTYPE html>
<html><body>
  <div class="user-header">251511002 Éka Pratama — Kelas D4-Teknik Informatika</div>
  <table id="jadwal">
    <tbody>
      <tr>
        <td>1</td>
        <td><button class="btn btn-primary">Absen</button></td>
      </tr>
      <tr>
        <td>2</td>
        <td>KO099N-Dr. Ångström — Tim Pengajar, M.Kom</td>
        <td>25IF999</td>
        <td>Pemrograman Berorientasi Objek — Lanjut 🚀</td>
        <td>P</td>
        <td>4</td>
        <td>6</td>
        <td>09:30:00 s.d. 12:00:00</td>
        <td><button class="btn btn-info">Absen</button></td>
      </tr>
    </tbody>
  </table>
</body></html>"#;

    let info = parse_user_header(html, "fallback");
    assert_eq!(info.nim, "251511002");
    assert_eq!(info.nama, "Éka Pratama — Kelas");
    assert_eq!(info.jurusan, "D4-Teknik Informatika");

    let (actions, skips) = find_rows(html, ENDPOINT);
    assert!(skips.is_empty());
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].nama_mk, "");
    assert_eq!(
        actions[1].nama_mk,
        "Pemrograman Berorientasi Objek — Lanjut 🚀"
    );
    assert_eq!(
        nama_dosen(&actions[1].dosen),
        "Dr. Ångström — Tim Pengajar, M.Kom"
    );

    assert_eq!(find_rows("", ENDPOINT), (vec![], vec![]));
    let empty_hdr = parse_user_header(r#"<div class="user-header">   </div>"#, "251511001");
    assert_eq!(empty_hdr.nim, "251511001");
    assert_eq!(empty_hdr.nama, "");
}

//! Headless UI rendering and interaction tests using `egui::Context::default()`.

use absen_core::{Color, UserInfo};
use absen_gui::bridge::{ActivityItem, ActivityKind, CourseItem, EngineStatus, GuiSnapshot};
use absen_gui::credentials::MemoryFallbackStore;
use absen_gui::prefs::{AppPrefs, ThemePreference};
use absen_gui::ui::CompactTab;
use absen_gui::DesktopApp;
use egui::{Event, Key, Modifiers, Pos2, RawInput, Rect, Shape, Vec2};

fn collect_texts_from_shape(shape: &Shape, out: &mut Vec<String>) {
    match shape {
        Shape::Text(text_shape) => {
            out.push(text_shape.galley.text().to_string());
        }
        Shape::Vec(children) => {
            for child in children {
                collect_texts_from_shape(child, out);
            }
        }
        _ => {}
    }
}

fn collect_rendered_text(output: &egui::FullOutput) -> String {
    let mut parts = Vec::new();
    for clipped in &output.shapes {
        collect_texts_from_shape(&clipped.shape, &mut parts);
    }
    parts.join("\n")
}

fn assert_no_horizontal_overflow(output: &egui::FullOutput, max_width: f32, label: &str) {
    for clipped in &output.shapes {
        let rect = clipped.shape.visual_bounding_rect();
        if rect.is_positive() {
            assert!(
                rect.right() <= max_width + 2.0,
                "{label}: shape right edge {} exceeded viewport width {max_width}",
                rect.right()
            );
            assert!(
                rect.left() >= -2.0,
                "{label}: shape left edge {} is negative",
                rect.left()
            );
        }
    }
}

fn run_frames(
    ctx: &egui::Context,
    app: &mut DesktopApp,
    size: Vec2,
    events: Vec<Event>,
) -> egui::FullOutput {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        events,
        ..Default::default()
    };
    let _ = ctx.run(input, |c| app.update_ui(c));
    let settle_input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        ..Default::default()
    };
    ctx.run(settle_input, |c| app.update_ui(c))
}

fn sample_populated_snapshot() -> GuiSnapshot {
    GuiSnapshot {
        status: EngineStatus::Monitoring,
        auth_status: "Terautentikasi".to_string(),
        user: Some(UserInfo {
            nim: "251511001".to_string(),
            nama: "Siti Rahmawati".to_string(),
            jurusan: "D3 Teknik Informatika".to_string(),
        }),
        cycle: 3,
        countdown_secs: 42,
        interval_secs: 60,
        last_scan: "08:15:10".to_string(),
        courses: vec![
            CourseItem {
                sig: "Sudah|Basis Data|KO001N-Dosen A|07:00:00 s.d. 08:40:00".to_string(),
                nama_mk: "Basis Data".to_string(),
                dosen: "KO001N-Dr. Budi Santoso".to_string(),
                jam: "07:00:00 s.d. 08:40:00".to_string(),
                color: Color::Green,
                status_label: "SUDAH ABSEN".to_string(),
                reason: "sudah absen (dihitung berhasil)".to_string(),
                action_ready: false,
            },
            CourseItem {
                sig: "Pemrograman Web|KO002N-Dosen B|08:40:00 s.d. 10:20:00".to_string(),
                nama_mk: "Pemrograman Web".to_string(),
                dosen: "KO002N-Trisna Gelar Abdillah, M.Kom".to_string(),
                jam: "08:40:00 s.d. 10:20:00".to_string(),
                color: Color::Blue,
                status_label: "SIAP ABSEN".to_string(),
                reason: "sesi presensi terbuka".to_string(),
                action_ready: true,
            },
            CourseItem {
                sig: "Belum|Matematika Diskrit|KO003N-Dosen C|10:20:00 s.d. 12:00:00".to_string(),
                nama_mk: "Matematika Diskrit".to_string(),
                dosen: "KO003N-Rina Sari, M.Si".to_string(),
                jam: "10:20:00 s.d. 12:00:00".to_string(),
                color: Color::Orange,
                status_label: "BELUM BUKA".to_string(),
                reason: "dosen belum membuka sesi absen".to_string(),
                action_ready: false,
            },
            CourseItem {
                sig: "Luar|Sistem Operasi|KO004N-Dosen D|13:00:00 s.d. 14:40:00".to_string(),
                nama_mk: "Sistem Operasi".to_string(),
                dosen: "KO004N-Andi Wijaya, M.T.".to_string(),
                jam: "13:00:00 s.d. 14:40:00".to_string(),
                color: Color::Red,
                status_label: "BELUM WAKTUNYA".to_string(),
                reason: "di luar jam absensi".to_string(),
                action_ready: false,
            },
        ],
        activities: vec![
            ActivityItem {
                timestamp: "08:14:55".to_string(),
                code: "LOGIN".to_string(),
                message: "Berhasil masuk ke portal akademik.".to_string(),
                kind: ActivityKind::Info,
            },
            ActivityItem {
                timestamp: "08:15:10".to_string(),
                code: "ABSEN".to_string(),
                message: "Presensi Basis Data tercatat berhasil.".to_string(),
                kind: ActivityKind::Ok,
            },
        ],
        last_error: None,
    }
}

fn configured_prefs(theme: ThemePreference) -> AppPrefs {
    AppPrefs {
        nim: "251511001".to_string(),
        theme,
        ..AppPrefs::default()
    }
}

#[test]
fn first_run_onboarding_renders_inline_without_modal_in_light_and_dark() {
    for theme in [ThemePreference::Light, ThemePreference::Dark] {
        let ctx = egui::Context::default();
        let prefs = AppPrefs {
            nim: String::new(),
            theme,
            ..AppPrefs::default()
        };
        let mut app = DesktopApp::new_headless(&ctx, prefs, Box::new(MemoryFallbackStore::new()));
        assert!(!app.is_settings_open(), "first-run must not pop modal");

        let out = run_frames(&ctx, &mut app, Vec2::new(960.0, 680.0), Vec::new());
        let text = collect_rendered_text(&out);
        assert!(text.contains("Masuk ke Portal Akademik Polban"));
        assert!(text.contains("NIM Mahasiswa"));
        assert!(text.contains("Kata Sandi Akademik"));
        assert!(text.contains("Simpan & Mulai Pantau"));
        assert!(text.contains("Impor dari .env"));
        assert!(text.contains("Memori Sesi"));
    }
}

#[test]
fn populated_schedule_renders_across_three_breakpoints_in_light_and_dark() {
    let viewports = [
        ("wide", Vec2::new(1040.0, 720.0), true, false),
        ("medium_640", Vec2::new(720.0, 640.0), false, false),
        ("medium_tall", Vec2::new(720.0, 920.0), true, false),
        ("compact_400", Vec2::new(400.0, 880.0), false, true),
        ("compact_380", Vec2::new(380.0, 880.0), false, true),
    ];

    for theme in [ThemePreference::Light, ThemePreference::Dark] {
        for (name, size, expects_activity_visible, is_compact) in viewports {
            let ctx = egui::Context::default();
            let creds = MemoryFallbackStore::new();
            let mut app = DesktopApp::new_headless(&ctx, configured_prefs(theme), Box::new(creds));
            app.set_test_snapshot(Some(sample_populated_snapshot()));

            let out = run_frames(&ctx, &mut app, size, Vec::new());
            assert_no_horizontal_overflow(&out, size.x, name);
            let text = collect_rendered_text(&out);

            assert!(
                text.contains("Jadwal Kuliah Hari Ini"),
                "{name} ({theme:?}) missing Jadwal heading"
            );
            assert!(
                text.contains("Basis Data") && text.contains("Pemrograman Web"),
                "{name} ({theme:?}) missing course titles"
            );
            assert!(
                text.contains("SUDAH ABSEN")
                    && text.contains("SIAP ABSEN")
                    && text.contains("BELUM BUKA")
                    && text.contains("BELUM WAKTUNYA"),
                "{name} ({theme:?}) missing status badges"
            );
            assert!(
                text.contains("Dr. Budi Santoso"),
                "{name} ({theme:?}) should strip lecturer code prefix"
            );

            if expects_activity_visible {
                assert!(
                    text.contains("Riwayat Aktivitas"),
                    "{name} ({theme:?}) should display Riwayat Aktivitas alongside/below Jadwal"
                );
            }

            if !is_compact {
                assert!(text.contains("07:00:00 s.d. 08:40:00"));
            } else {
                assert_eq!(app.compact_tab(), CompactTab::Jadwal);
                assert!(
                    text.contains("07:00-08:40"),
                    "{name} ({theme:?}) should compact time range with jam_ringkas"
                );
                assert!(text.contains("Aktivitas"));
            }
        }
    }
}

#[test]
fn engine_loading_auth_failed_network_error_and_empty_states_render_cleanly() {
    let ctx = egui::Context::default();
    let mut app = DesktopApp::new_headless(
        &ctx,
        configured_prefs(ThemePreference::Light),
        Box::new(MemoryFallbackStore::new()),
    );

    // 1. Connecting / Loading
    app.set_test_snapshot(Some(GuiSnapshot {
        status: EngineStatus::Connecting,
        ..Default::default()
    }));
    let out = run_frames(&ctx, &mut app, Vec2::new(900.0, 640.0), Vec::new());
    let text = collect_rendered_text(&out);
    assert!(text.contains("Menghubungkan ke akademik.polban.ac.id"));

    // 2. AuthFailed
    app.set_test_snapshot(Some(GuiSnapshot {
        status: EngineStatus::AuthFailed,
        last_error: Some("NIM atau kata sandi salah pada portal.".to_string()),
        ..Default::default()
    }));
    let out = run_frames(&ctx, &mut app, Vec2::new(900.0, 640.0), Vec::new());
    let text = collect_rendered_text(&out);
    assert!(text.contains("Autentikasi Portal Gagal"));
    assert!(text.contains("NIM atau kata sandi salah pada portal."));
    assert!(text.contains("Periksa Kredensial"));

    // 3. NetworkError
    app.set_test_snapshot(Some(GuiSnapshot {
        status: EngineStatus::NetworkError,
        last_error: Some("Batas waktu koneksi habis (timeout).".to_string()),
        ..Default::default()
    }));
    let out = run_frames(&ctx, &mut app, Vec2::new(900.0, 640.0), Vec::new());
    let text = collect_rendered_text(&out);
    assert!(text.contains("Gangguan Jaringan ke Portal"));
    assert!(text.contains("Batas waktu koneksi habis (timeout)."));
    assert!(text.contains("Coba Lagi Sekarang"));

    // 4. Empty Schedule after successful scan
    app.set_test_snapshot(Some(GuiSnapshot {
        status: EngineStatus::Monitoring,
        last_scan: "08:00:00".to_string(),
        courses: Vec::new(),
        ..Default::default()
    }));
    let out = run_frames(&ctx, &mut app, Vec2::new(900.0, 640.0), Vec::new());
    let text = collect_rendered_text(&out);
    assert!(
        text.contains("Tidak ada jadwal perkuliahan terdaftar untuk hari ini di portal akademik.")
    );
    assert!(text.contains("Cek Sekarang"));
}

#[test]
fn settings_modal_opens_with_ctrl_comma_and_closes_with_escape() {
    let ctx = egui::Context::default();
    let mut app = DesktopApp::new_headless(
        &ctx,
        configured_prefs(ThemePreference::Dark),
        Box::new(MemoryFallbackStore::new()),
    );
    app.set_test_snapshot(Some(sample_populated_snapshot()));
    assert!(!app.is_settings_open());

    // Open with Ctrl+, on standard 1000x720 window
    let open_events = vec![Event::Key {
        key: Key::Comma,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }];
    let out = run_frames(&ctx, &mut app, Vec2::new(1000.0, 720.0), open_events);
    assert!(app.is_settings_open());
    let text = collect_rendered_text(&out);
    assert!(text.contains("Akun Portal Akademik"));
    assert!(text.contains("Integrasi Desktop"));

    // Close with Escape
    let close_events = vec![Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }];
    let _ = run_frames(&ctx, &mut app, Vec2::new(1000.0, 720.0), close_events);
    assert!(!app.is_settings_open());

    // Open in tall context to verify lower settings section & diagnostic export button
    let tall_ctx = egui::Context::default();
    let mut tall_app = DesktopApp::new_headless(
        &tall_ctx,
        configured_prefs(ThemePreference::Light),
        Box::new(MemoryFallbackStore::new()),
    );
    let open_tall = vec![Event::Key {
        key: Key::Comma,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }];
    let out_tall = run_frames(
        &tall_ctx,
        &mut tall_app,
        Vec2::new(1000.0, 1200.0),
        open_tall,
    );
    assert!(tall_app.is_settings_open());
    let tall_text = collect_rendered_text(&out_tall);
    assert!(tall_text.contains("Simpan HTML Halaman Absen (Diagnostik)"));
    assert!(tall_text.contains("Simpan Pengaturan"));
}

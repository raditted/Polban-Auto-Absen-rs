//! Headless UI rendering, responsive geometry, and interaction tests using `egui::Context::default()`.

use absen_core::{AttendanceProof, Color, ProofVerification, UserInfo};
use absen_gui::bridge::{
    ActivityItem, ActivityKind, CourseItem, EngineStatus, GuiSnapshot, ProofPreview,
};
use absen_gui::credentials::MemoryFallbackStore;
use absen_gui::prefs::{AppPrefs, DarkThemePreset, LightThemePreset, ThemePreference};
use absen_gui::theme::Palette;
use absen_gui::updater::{
    AssetStrategy, RestartPlan, UpdateCandidate, UpdateStatus, CURRENT_VERSION,
};
use absen_gui::DesktopApp;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};

fn collect_texts_from_shape(shape: &Shape, out: &mut Vec<(String, Rect)>) {
    match shape {
        Shape::Text(text_shape) => {
            out.push((
                text_shape.galley.text().to_string(),
                text_shape.visual_bounding_rect(),
            ));
        }
        Shape::Vec(children) => {
            for child in children {
                collect_texts_from_shape(child, out);
            }
        }
        _ => {}
    }
}

fn collect_text_rects(output: &egui::FullOutput) -> Vec<(String, Rect)> {
    let mut parts = Vec::new();
    for clipped in &output.shapes {
        collect_texts_from_shape(&clipped.shape, &mut parts);
    }
    parts
}

fn collect_rendered_text(output: &egui::FullOutput) -> String {
    collect_text_rects(output)
        .into_iter()
        .map(|(t, _)| t)
        .collect::<Vec<_>>()
        .join("\n")
}

fn collect_rect_shapes(shape: &Shape, out: &mut Vec<(Rect, egui::Color32)>) {
    match shape {
        Shape::Rect(rect_shape) => {
            out.push((rect_shape.rect, rect_shape.fill));
        }
        Shape::Vec(children) => {
            for child in children {
                collect_rect_shapes(child, out);
            }
        }
        _ => {}
    }
}

fn find_text_rect(rects: &[(String, Rect)], needle: &str) -> Option<Rect> {
    rects
        .iter()
        .find(|(text, _)| text.contains(needle))
        .map(|(_, rect)| *rect)
}

fn find_exact_text_rect(rects: &[(String, Rect)], exact: &str) -> Option<Rect> {
    rects
        .iter()
        .find(|(text, _)| text == exact)
        .map(|(_, rect)| *rect)
}

fn assert_no_horizontal_overflow(output: &egui::FullOutput, max_width: f32, label: &str) {
    for clipped in &output.shapes {
        let rect = clipped
            .shape
            .visual_bounding_rect()
            .intersect(clipped.clip_rect);
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
        auth_status: "OK: Authenticated".to_string(),
        user: Some(UserInfo {
            nim: "251511025".to_string(),
            nama: "RADITHYA MAHESA SYABIL".to_string(),
            jurusan: "D3-Teknik Informatika".to_string(),
        }),
        cycle: 4,
        countdown_secs: 16,
        interval_secs: 60,
        last_scan: "09:55:19".to_string(),
        courses: vec![
            CourseItem {
                sig: "Sudah|Basis Data|KO001N-Dosen A|07:00:00 s.d. 08:40:00".to_string(),
                nama_mk: "Basis Data".to_string(),
                dosen: "KO001N-Dr. Ade Chandra Nugraha, S.Si., MT".to_string(),
                jam: "07:00:00 s.d. 08:40:00".to_string(),
                color: Color::Green,
                status_label: "Sudah Absen".to_string(),
                reason: "Sudah absen (dihitung berhasil)".to_string(),
                action_ready: false,
            },
            CourseItem {
                sig: "Pengantar Rekayasa Perangkat Lunak|KO002N-Dosen B|08:40:00 s.d. 12:20:00"
                    .to_string(),
                nama_mk: "Pengantar Rekayasa Perangkat Lunak".to_string(),
                dosen: "KO002N-Yadhi Aditya Permana, ST, M.Kom".to_string(),
                jam: "08:40:00 s.d. 12:20:00".to_string(),
                color: Color::Blue,
                status_label: "Siap Absen".to_string(),
                reason: "Sesi absensi terbuka, siap dicatat otomatis".to_string(),
                action_ready: true,
            },
            CourseItem {
                sig: "Belum|Matematika Diskrit|KO003N-Dosen C|10:20:00 s.d. 12:00:00".to_string(),
                nama_mk: "Matematika Diskrit".to_string(),
                dosen: "KO003N-Rina Sari, M.Si".to_string(),
                jam: "10:20:00 s.d. 12:00:00".to_string(),
                color: Color::Orange,
                status_label: "Belum Buka".to_string(),
                reason: "Belum buka: dosen belum membuka sesi absen".to_string(),
                action_ready: false,
            },
            CourseItem {
                sig: "Luar|Pengantar Rekayasa Perangkat Lunak Lanjutan|KO004N-Dosen D|13:00:00 s.d. 14:40:00".to_string(),
                nama_mk: "Pengantar Rekayasa Perangkat Lunak Lanjutan".to_string(),
                dosen: "KO004N-Santi Sundari, S.Si., MT".to_string(),
                jam: "13:00:00 s.d. 14:40:00".to_string(),
                color: Color::Red,
                status_label: "Belum Waktunya".to_string(),
                reason: "Belum waktunya: di luar jam absensi".to_string(),
                action_ready: false,
            },
        ],
        activities: vec![
            ActivityItem {
                timestamp: "09:52:15".to_string(),
                code: "LOGIN".to_string(),
                message: "Authenticated".to_string(),
                kind: ActivityKind::Ok,
                proof_key: None,
            },
            ActivityItem {
                timestamp: "09:52:15".to_string(),
                code: "ENGINE".to_string(),
                message: "HTTP aktif. interval=60s loop=True".to_string(),
                kind: ActivityKind::Info,
                proof_key: None,
            },
            ActivityItem {
                timestamp: "09:55:19".to_string(),
                code: "SCAN".to_string(),
                message: "Scan awal: 1 siap absen | 1 sudah absen | 1 belum buka | 1 belum waktunya"
                    .to_string(),
                kind: ActivityKind::Info,
                proof_key: None,
            },
        ],
        proofs: Vec::new(),
        today_date: "2026-11-21".to_string(),
        last_error: None,
        ..Default::default()
    }
}

fn configured_prefs(theme: ThemePreference) -> AppPrefs {
    AppPrefs {
        nim: "251511025".to_string(),
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
        assert!(text.contains("Simpan & Jalankan Presensi Otomatis"));
        assert!(text.contains("Impor dari .env"));
        assert!(text.contains("Memori Sesi"));
    }
}

#[test]
fn populated_schedule_renders_without_overlap_across_responsive_viewports() {
    let viewports = [
        ("narrow_680", Vec2::new(680.0, 540.0), false),
        ("narrow_tall", Vec2::new(680.0, 1080.0), true),
        ("medium_960", Vec2::new(960.0, 640.0), true),
        ("desktop_1280", Vec2::new(1280.0, 800.0), true),
        ("ultrawide_1920", Vec2::new(1920.0, 1080.0), true),
    ];

    for theme in [ThemePreference::Light, ThemePreference::Dark] {
        let expected_palette = match theme {
            ThemePreference::Dark => &Palette::DARK,
            _ => &Palette::LIGHT,
        };
        for (name, size, expects_activity_visible) in viewports {
            let ctx = egui::Context::default();
            let creds = MemoryFallbackStore::new();
            let mut app = DesktopApp::new_headless(&ctx, configured_prefs(theme), Box::new(creds));
            app.set_test_snapshot(Some(sample_populated_snapshot()));

            let out = run_frames(&ctx, &mut app, size, Vec::new());
            assert_no_horizontal_overflow(&out, size.x, name);

            let text_rects = collect_text_rects(&out);
            let text = collect_rendered_text(&out);

            assert!(
                text.contains("Jadwal Kuliah Hari Ini"),
                "{name} missing Jadwal"
            );
            if expects_activity_visible {
                assert!(
                    text.contains("Riwayat Aktivitas"),
                    "{name} missing Activity"
                );
            }
            assert!(
                text.contains("Portal Terhubung"),
                "{name} missing clean portal status"
            );
            assert!(
                text.contains("Cek berikutnya"),
                "{name} missing next-check metric"
            );
            assert!(text.contains("16 dtk"), "{name} missing countdown unit dtk");
            assert!(
                !text.contains("OK: Authenticated"),
                "{name} leaked raw auth string"
            );
            assert!(!text.contains("60d"), "{name} leaked inconsistent 60d unit");
            assert!(
                text.contains("07:00 \u{2013} 08:40") && text.contains("08:40 \u{2013} 12:20"),
                "{name} missing clean time ranges"
            );

            // Verify zero overlap between long course title, status pill, and time range.
            let long_title_rect = find_text_rect(&text_rects, "Pengantar Rekayasa Perangkat Lunak")
                .expect("expected long course title rect");
            let siap_rect =
                find_text_rect(&text_rects, "Siap Absen").expect("expected Siap Absen rect");
            let time_rect = find_text_rect(&text_rects, "08:40 \u{2013} 12:20")
                .expect("expected time range rect");

            assert!(
                !long_title_rect.intersects(siap_rect),
                "{name} ({theme:?}): long title {long_title_rect:?} overlapped badge {siap_rect:?}"
            );
            assert!(
                !long_title_rect.intersects(time_rect),
                "{name} ({theme:?}): long title {long_title_rect:?} overlapped time {time_rect:?}"
            );

            // Verify Belum Waktunya is calm neutral (not red error text) when all 4 cards fit in view.
            if size.y >= 760.0 {
                let belum_waktunya_shape = out.shapes.iter().find_map(|c| {
                    let mut found = None;
                    let mut stack = vec![&c.shape];
                    while let Some(s) = stack.pop() {
                        match s {
                            Shape::Text(ts) if ts.galley.text() == "Belum Waktunya" => {
                                found = Some(ts);
                                break;
                            }
                            Shape::Vec(children) => stack.extend(children.iter()),
                            _ => {}
                        }
                    }
                    found
                });
                let bw = belum_waktunya_shape.expect("expected Belum Waktunya badge");
                let job_col = bw.galley.job.sections[0].format.color;
                assert_ne!(
                    job_col, expected_palette.red_text,
                    "{name}: Belum Waktunya must use calm neutral color, not red"
                );
            }

            // Verify ultra-wide 1920x1080 centers the content within <= 1280px container width.
            if size.x >= 1900.0 {
                let header_rect =
                    find_text_rect(&text_rects, "Polban Presensi").expect("expected header title");
                assert!(
                    header_rect.left() >= 250.0,
                    "{name}: expected centered container on ultrawide, got left={}",
                    header_rect.left()
                );
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
    let out = run_frames(&ctx, &mut app, Vec2::new(960.0, 640.0), Vec::new());
    let text = collect_rendered_text(&out);
    assert!(text.contains("Menghubungkan ke akademik.polban.ac.id"));

    // 2. AuthFailed
    app.set_test_snapshot(Some(GuiSnapshot {
        status: EngineStatus::AuthFailed,
        last_error: Some("NIM atau kata sandi salah pada portal.".to_string()),
        ..Default::default()
    }));
    let out = run_frames(&ctx, &mut app, Vec2::new(960.0, 640.0), Vec::new());
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
    let out = run_frames(&ctx, &mut app, Vec2::new(960.0, 640.0), Vec::new());
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
    let out = run_frames(&ctx, &mut app, Vec2::new(960.0, 640.0), Vec::new());
    let text = collect_rendered_text(&out);
    assert!(
        text.contains("Tidak ada jadwal perkuliahan terdaftar untuk hari ini di portal akademik.")
    );
    assert!(text.contains("Cek Sekarang"));
}

#[test]
fn settings_modal_layout_alignment_backdrop_and_dismiss_across_viewports() {
    let viewports = [
        Vec2::new(680.0, 540.0),
        Vec2::new(960.0, 640.0),
        Vec2::new(1280.0, 800.0),
        Vec2::new(1920.0, 1080.0),
        Vec2::new(1000.0, 1100.0),
    ];

    for theme in [ThemePreference::Light, ThemePreference::Dark] {
        for size in viewports {
            let ctx = egui::Context::default();
            let mut app = DesktopApp::new_headless(
                &ctx,
                configured_prefs(theme),
                Box::new(MemoryFallbackStore::new()),
            );
            app.set_test_snapshot(Some(sample_populated_snapshot()));
            assert!(!app.is_settings_open());

            // Open with Ctrl+,
            let open_events = vec![Event::Key {
                key: Key::Comma,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::COMMAND,
            }];
            let out = run_frames(&ctx, &mut app, size, open_events);
            assert!(app.is_settings_open());

            let text = collect_rendered_text(&out);
            assert!(text.contains("Pengaturan"));
            assert!(text.contains("Akun Portal Akademik"));
            assert!(text.contains("Simpan Pengaturan"));
            assert!(text.contains("Batal"));
            if size.y >= 800.0 {
                assert!(text.contains("Light Theme"));
                assert!(text.contains("Preset"));
                assert!(text.contains("Background"));
            }

            // Verify backdrop scrim exists covering full viewport.
            let mut rect_shapes = Vec::new();
            for c in &out.shapes {
                collect_rect_shapes(&c.shape, &mut rect_shapes);
            }
            let has_scrim = rect_shapes.iter().any(|(r, fill)| {
                (r.width() - size.x).abs() < 2.0
                    && (r.height() - size.y).abs() < 2.0
                    && fill.a() > 100
            });
            assert!(
                has_scrim,
                "{size:?} ({theme:?}): expected dimmed backdrop scrim"
            );

            // Verify modal container is inset from viewport edges (never touching top/bottom).
            let expected_fill = match theme {
                ThemePreference::Dark => Palette::DARK.bg_surface,
                _ => Palette::LIGHT.bg_surface,
            };
            let screen_center = Pos2::new(size.x * 0.5, size.y * 0.5);
            let modal_rect = rect_shapes
                .iter()
                .filter(|(r, fill)| {
                    *fill == expected_fill
                        && r.width() >= 300.0
                        && r.width() <= 600.0
                        && r.height() >= 200.0
                        && (r.center() - screen_center).length() < 8.0
                })
                .map(|(r, _)| *r)
                .max_by(|a, b| a.area().total_cmp(&b.area()))
                .unwrap_or_else(|| {
                    let dump: Vec<_> = rect_shapes
                        .iter()
                        .filter(|(r, _)| r.width() >= 200.0 && r.height() >= 100.0)
                        .collect();
                    panic!("expected floating settings modal rect at {size:?}, expected_fill={expected_fill:?}, candidates={dump:?}");
                });

            assert!(
                modal_rect.left() >= 12.0
                    && modal_rect.right() <= size.x - 12.0
                    && modal_rect.top() >= 16.0
                    && modal_rect.bottom() <= size.y - 16.0,
                "{size:?}: modal {modal_rect:?} touched viewport edges"
            );

            // Scroll modal body down to verify 2-column advanced fields align horizontally.
            if size.x >= 960.0 && size.y >= 800.0 {
                let scrolled_out = run_frames(
                    &ctx,
                    &mut app,
                    size,
                    vec![
                        Event::PointerMoved(screen_center),
                        Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: Vec2::new(0.0, -1600.0),
                            modifiers: Modifiers::NONE,
                        },
                    ],
                );
                let scrolled_rects = collect_text_rects(&scrolled_out);

                let r_interval = find_text_rect(&scrolled_rects, "Interval pemantauan")
                    .expect("expected interval label");
                let r_retry = find_text_rect(&scrolled_rects, "Maks percobaan ulang")
                    .expect("expected max retry label");
                assert!(
                    (r_interval.top() - r_retry.top()).abs() < 2.0,
                    "{size:?}: 2-col row 1 misaligned: interval={r_interval:?}, retry={r_retry:?}"
                );

                let scrolled_more = run_frames(
                    &ctx,
                    &mut app,
                    size,
                    vec![
                        Event::PointerMoved(screen_center),
                        Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: Vec2::new(0.0, -400.0),
                            modifiers: Modifiers::NONE,
                        },
                    ],
                );
                let scrolled_more_rects = collect_text_rects(&scrolled_more);

                let r_dmin = find_text_rect(&scrolled_more_rects, "Jeda klik minimum")
                    .expect("expected delay min label");
                let r_dmax = find_text_rect(&scrolled_more_rects, "Jeda klik maksimum")
                    .expect("expected delay max label");
                assert!(
                    (r_dmin.top() - r_dmax.top()).abs() < 2.0,
                    "{size:?}: 2-col row 2 misaligned: min={r_dmin:?}, max={r_dmax:?}"
                );
            }

            // Close by clicking backdrop outside modal (top-left corner).
            let click_backdrop = vec![
                Event::PointerMoved(Pos2::new(6.0, 6.0)),
                Event::PointerButton {
                    pos: Pos2::new(6.0, 6.0),
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
                Event::PointerButton {
                    pos: Pos2::new(6.0, 6.0),
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ];
            let _ = run_frames(&ctx, &mut app, size, click_backdrop);
            assert!(!app.is_settings_open(), "backdrop click should close modal");

            // Reopen and close with Escape.
            let reopen = vec![Event::Key {
                key: Key::Comma,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::COMMAND,
            }];
            let _ = run_frames(&ctx, &mut app, size, reopen);
            assert!(app.is_settings_open());

            let esc = vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }];
            let _ = run_frames(&ctx, &mut app, size, esc);
            assert!(!app.is_settings_open(), "Escape should close modal");
        }
    }
}

fn click_at(pos: Pos2) -> Vec<Event> {
    vec![
        Event::PointerMoved(pos),
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]
}

#[test]
fn antigravity_appearance_cards_render_and_switch_light_and_dark_presets_live() {
    let size = Vec2::new(1280.0, 900.0);
    let screen_center = Pos2::new(size.x * 0.5, size.y * 0.5);
    let ctx = egui::Context::default();
    let mut app = DesktopApp::new_headless(
        &ctx,
        configured_prefs(ThemePreference::Dark),
        Box::new(MemoryFallbackStore::new()),
    );
    app.set_test_snapshot(Some(sample_populated_snapshot()));

    // Open Settings via Ctrl+,
    let _ = run_frames(
        &ctx,
        &mut app,
        size,
        vec![Event::Key {
            key: Key::Comma,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }],
    );
    assert!(app.is_settings_open());

    // Scroll slightly so both Light Theme and Dark Theme cards are in full view.
    let out = run_frames(
        &ctx,
        &mut app,
        size,
        vec![
            Event::PointerMoved(screen_center),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, -320.0),
                modifiers: Modifiers::NONE,
            },
        ],
    );
    assert_no_horizontal_overflow(&out, size.x, "appearance_cards_initial");

    let text = collect_rendered_text(&out);
    assert!(text.contains("Tema Tampilan"));
    assert!(text.contains("Light Theme"));
    assert!(text.contains("Dark Theme"));
    assert!(text.contains("Preset"));
    assert!(text.contains("Background"));
    assert!(text.contains("Foreground"));
    assert!(text.contains("Accent"));
    assert!(text.contains("Default Light"));
    assert!(text.contains("Default Dark"));
    assert!(text.contains("#FFFFFF"));
    assert!(text.contains("#0F172A"));
    assert!(text.contains("#111827"));
    assert!(text.contains("#F8FAFC"));

    // 1. Open Dark Theme preset dropdown by clicking "Default Dark".
    let rects = collect_text_rects(&out);
    let dark_trigger_rect =
        find_text_rect(&rects, "Default Dark").expect("expected Default Dark dropdown pill");
    let popup_out = run_frames(&ctx, &mut app, size, click_at(dark_trigger_rect.center()));
    assert_no_horizontal_overflow(&popup_out, size.x, "dark_preset_popup_open");

    let popup_text = collect_rendered_text(&popup_out);
    for preset in DarkThemePreset::ALL {
        assert!(
            popup_text.contains(preset.label()),
            "Dark preset popup missing {}",
            preset.label()
        );
    }

    // Select "Everforest" from Dark Theme popup and verify `#2D353B`, `#D3C6AA`, `#A7C080` pills and live surface color.
    let popup_rects = collect_text_rects(&popup_out);
    let everforest_rect =
        find_text_rect(&popup_rects, "Everforest").expect("expected Everforest menu item");
    let after_dark_out = run_frames(&ctx, &mut app, size, click_at(everforest_rect.center()));
    assert_no_horizontal_overflow(&after_dark_out, size.x, "after_everforest_selected");

    let after_dark_text = collect_rendered_text(&after_dark_out);
    assert!(after_dark_text.contains("Everforest"));
    assert!(after_dark_text.contains("#2D353B"));
    assert!(after_dark_text.contains("#D3C6AA"));
    assert!(after_dark_text.contains("#A7C080"));
    assert_eq!(
        ctx.style().visuals.window_fill,
        Palette::EVERFOREST_DARK.bg_surface
    );

    // 2. Open Light Theme preset dropdown by clicking "Default Light".
    let after_dark_rects = collect_text_rects(&after_dark_out);
    let light_trigger_rect = find_text_rect(&after_dark_rects, "Default Light")
        .expect("expected Default Light dropdown pill");
    let light_popup_out = run_frames(&ctx, &mut app, size, click_at(light_trigger_rect.center()));
    assert_no_horizontal_overflow(&light_popup_out, size.x, "light_preset_popup_open");

    let light_popup_text = collect_rendered_text(&light_popup_out);
    for preset in LightThemePreset::ALL {
        assert!(
            light_popup_text.contains(preset.label()),
            "Light preset popup missing {}",
            preset.label()
        );
    }

    // Select "Solarized Light" from Light Theme popup and verify its token hex pills appear immediately.
    let light_popup_rects = collect_text_rects(&light_popup_out);
    let solarized_rect = find_text_rect(&light_popup_rects, "Solarized Light")
        .expect("expected Solarized Light menu item");
    let after_light_out = run_frames(&ctx, &mut app, size, click_at(solarized_rect.center()));
    assert_no_horizontal_overflow(&after_light_out, size.x, "after_solarized_light_selected");

    let after_light_text = collect_rendered_text(&after_light_out);
    assert!(after_light_text.contains("Solarized Light"));
    assert!(
        after_light_text.contains(&Palette::hex_rgb(Palette::SOLARIZED_LIGHT.bg_surface)),
        "expected Solarized Light background hex"
    );
    assert!(
        after_light_text.contains(&Palette::hex_rgb(Palette::SOLARIZED_LIGHT.text_primary)),
        "expected Solarized Light foreground hex"
    );
    assert!(
        after_light_text.contains(&Palette::hex_rgb(Palette::SOLARIZED_LIGHT.accent)),
        "expected Solarized Light accent hex"
    );

    // Save Settings by clicking "Simpan Pengaturan" and verify prefs persisted both presets.
    let save_rects = collect_text_rects(&after_light_out);
    let save_rect =
        find_text_rect(&save_rects, "Simpan Pengaturan").expect("expected Simpan Pengaturan");
    let _ = run_frames(&ctx, &mut app, size, click_at(save_rect.center()));
    assert!(!app.is_settings_open());
    assert_eq!(app.prefs().dark_preset, DarkThemePreset::Everforest);
    assert_eq!(app.prefs().light_preset, LightThemePreset::SolarizedLight);
}

#[test]
fn all_19_light_and_dark_theme_presets_render_without_overflow_across_viewports() {
    let viewports = [
        Vec2::new(680.0, 540.0),
        Vec2::new(960.0, 640.0),
        Vec2::new(1280.0, 800.0),
    ];

    for light_preset in LightThemePreset::ALL {
        let expected = Palette::light_for_preset(light_preset);
        for size in viewports {
            let ctx = egui::Context::default();
            let prefs = AppPrefs {
                nim: "251511025".to_string(),
                theme: ThemePreference::Light,
                light_preset,
                ..AppPrefs::default()
            };
            let mut app =
                DesktopApp::new_headless(&ctx, prefs, Box::new(MemoryFallbackStore::new()));
            app.set_test_snapshot(Some(sample_populated_snapshot()));
            let out = run_frames(&ctx, &mut app, size, Vec::new());
            assert_no_horizontal_overflow(&out, size.x, light_preset.label());
            assert_eq!(ctx.style().visuals.window_fill, expected.bg_surface);
        }
    }

    for dark_preset in DarkThemePreset::ALL {
        let expected = Palette::dark_for_preset(dark_preset);
        for size in viewports {
            let ctx = egui::Context::default();
            let prefs = AppPrefs {
                nim: "251511025".to_string(),
                theme: ThemePreference::Dark,
                dark_preset,
                ..AppPrefs::default()
            };
            let mut app =
                DesktopApp::new_headless(&ctx, prefs, Box::new(MemoryFallbackStore::new()));
            app.set_test_snapshot(Some(sample_populated_snapshot()));
            let out = run_frames(&ctx, &mut app, size, Vec::new());
            assert_no_horizontal_overflow(&out, size.x, dark_preset.label());
            assert_eq!(ctx.style().visuals.window_fill, expected.bg_surface);
        }
    }
}

#[test]
fn header_update_pill_renders_all_states_without_overflow_or_overlap() {
    let viewports = [
        ("narrow_680", Vec2::new(680.0, 540.0)),
        ("medium_960", Vec2::new(960.0, 640.0)),
        ("desktop_1280", Vec2::new(1280.0, 800.0)),
    ];

    let sample_candidate = UpdateCandidate {
        version: "3.0.2".to_string(),
        release_url: "https://github.com/raditted/Polban-Auto-Absen-rs/releases/tag/v3.0.2"
            .to_string(),
        strategy: AssetStrategy::WindowsPortableZip,
        asset: None,
        sha256sums_url: None,
    };

    for (vp_name, size) in viewports {
        let ctx = egui::Context::default();
        let mut app = DesktopApp::new_headless(
            &ctx,
            configured_prefs(ThemePreference::Light),
            Box::new(MemoryFallbackStore::new()),
        );
        app.set_test_snapshot(Some(sample_populated_snapshot()));

        // Idle, Checking, UpToDate must render zero header update clutter.
        for hidden_status in [
            UpdateStatus::Idle,
            UpdateStatus::Checking,
            UpdateStatus::UpToDate,
        ] {
            app.set_update_status_for_test(hidden_status);
            let out = run_frames(&ctx, &mut app, size, Vec::new());
            assert_no_horizontal_overflow(&out, size.x, vp_name);
            let text = collect_rendered_text(&out);
            assert!(!text.contains("Update v"));
            assert!(!text.contains("Mengunduh"));
            assert!(!text.contains("Restart untuk Update"));
            assert!(!text.contains("Gagal Update"));
        }

        // Available -> Update v3.0.2
        app.set_update_status_for_test(UpdateStatus::Available(sample_candidate.clone()));
        let avail_out = run_frames(&ctx, &mut app, size, Vec::new());
        assert_no_horizontal_overflow(&avail_out, size.x, vp_name);
        let avail_rects = collect_text_rects(&avail_out);
        let pill_rect = find_text_rect(&avail_rects, "Update v3.0.2")
            .unwrap_or_else(|| panic!("{vp_name} missing Update v3.0.2 pill"));
        let stop_rect = find_text_rect(&avail_rects, "Hentikan")
            .unwrap_or_else(|| panic!("{vp_name} missing Hentikan button"));
        assert!(
            !pill_rect.intersects(stop_rect),
            "{vp_name}: update pill {pill_rect:?} overlaps Hentikan {stop_rect:?}"
        );

        // Downloading 42%
        app.set_update_status_for_test(UpdateStatus::Downloading {
            version: "3.0.2".to_string(),
            progress_pct: Some(42),
            verifying: false,
        });
        let dl_out = run_frames(&ctx, &mut app, size, Vec::new());
        assert_no_horizontal_overflow(&dl_out, size.x, vp_name);
        assert!(collect_rendered_text(&dl_out).contains("Mengunduh 42%"));

        // Verifying...
        app.set_update_status_for_test(UpdateStatus::Downloading {
            version: "3.0.2".to_string(),
            progress_pct: Some(100),
            verifying: true,
        });
        let ver_out = run_frames(&ctx, &mut app, size, Vec::new());
        assert_no_horizontal_overflow(&ver_out, size.x, vp_name);
        assert!(collect_rendered_text(&ver_out).contains("Memverifikasi..."));

        // ReadyToRestart -> Restart untuk Update
        app.set_update_status_for_test(UpdateStatus::ReadyToRestart {
            version: "3.0.2".to_string(),
            plan: RestartPlan::RunInstaller {
                installer_path: std::path::PathBuf::from("Polban-Presensi-Setup-Windows-x64.exe"),
            },
        });
        let ready_out = run_frames(&ctx, &mut app, size, Vec::new());
        assert_no_horizontal_overflow(&ready_out, size.x, vp_name);
        let ready_rects = collect_text_rects(&ready_out);
        let restart_rect = find_text_rect(&ready_rects, "Restart untuk Update")
            .unwrap_or_else(|| panic!("{vp_name} missing Restart untuk Update pill"));
        let ready_stop_rect = find_text_rect(&ready_rects, "Hentikan")
            .unwrap_or_else(|| panic!("{vp_name} missing Hentikan button"));
        assert!(
            !restart_rect.intersects(ready_stop_rect),
            "{vp_name}: restart pill {restart_rect:?} overlaps Hentikan {ready_stop_rect:?}"
        );

        // Failed -> Gagal Update - Coba Lagi
        app.set_update_status_for_test(UpdateStatus::Failed {
            message: "Checksum tidak cocok".to_string(),
            retry_candidate: Some(sample_candidate.clone()),
        });
        let fail_out = run_frames(&ctx, &mut app, size, Vec::new());
        assert_no_horizontal_overflow(&fail_out, size.x, vp_name);
        assert!(collect_rendered_text(&fail_out).contains("Gagal Update - Coba Lagi"));
    }
}

#[test]
fn settings_modal_displays_current_version_and_manual_update_check() {
    let ctx = egui::Context::default();
    let mut app = DesktopApp::new_headless(
        &ctx,
        configured_prefs(ThemePreference::Light),
        Box::new(MemoryFallbackStore::new()),
    );
    app.set_test_snapshot(Some(sample_populated_snapshot()));
    let size = Vec2::new(960.0, 680.0);
    let screen_center = Pos2::new(size.x * 0.5, size.y * 0.5);

    let main_out = run_frames(&ctx, &mut app, size, Vec::new());
    let settings_btn = find_text_rect(&collect_text_rects(&main_out), "Pengaturan")
        .expect("expected Pengaturan button");
    let _ = run_frames(&ctx, &mut app, size, click_at(settings_btn.center()));
    assert!(app.is_settings_open());

    // Scroll to the bottom of the settings ScrollArea where version and update check live.
    let mut scrolled_out = run_frames(&ctx, &mut app, size, Vec::new());
    for _ in 0..4 {
        scrolled_out = run_frames(
            &ctx,
            &mut app,
            size,
            vec![
                Event::PointerMoved(screen_center),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0.0, -2000.0),
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    assert_no_horizontal_overflow(&scrolled_out, size.x, "settings_update_section");

    let text = collect_rendered_text(&scrolled_out);
    assert!(text.contains(&format!("v{CURRENT_VERSION}")));
    assert!(text.contains("Cek Pembaruan"));
}

#[test]
fn header_jalankan_presensi_otomatis_fits_without_overlap_across_viewports() {
    let viewports = [
        ("narrow_680", Vec2::new(680.0, 540.0)),
        ("medium_860", Vec2::new(860.0, 640.0)),
        ("medium_960", Vec2::new(960.0, 640.0)),
        ("desktop_1280", Vec2::new(1280.0, 800.0)),
        ("ultrawide_1920", Vec2::new(1920.0, 1080.0)),
    ];

    for (vp_name, size) in viewports {
        let ctx = egui::Context::default();
        let mut app = DesktopApp::new_headless(
            &ctx,
            configured_prefs(ThemePreference::Light),
            Box::new(MemoryFallbackStore::new()),
        );
        let mut idle_snap = sample_populated_snapshot();
        idle_snap.status = EngineStatus::Idle;
        app.set_test_snapshot(Some(idle_snap));

        let out = run_frames(&ctx, &mut app, size, Vec::new());
        assert_no_horizontal_overflow(&out, size.x, vp_name);
        let rects = collect_text_rects(&out);
        let start_rect = find_text_rect(&rects, "Jalankan Presensi Otomatis")
            .unwrap_or_else(|| panic!("{vp_name} missing Jalankan Presensi Otomatis"));
        let scan_rect = find_text_rect(&rects, "Cek Sekarang")
            .unwrap_or_else(|| panic!("{vp_name} missing Cek Sekarang"));
        let settings_rect = find_text_rect(&rects, "Pengaturan")
            .unwrap_or_else(|| panic!("{vp_name} missing Pengaturan"));
        assert!(
            !start_rect.intersects(scan_rect) && !start_rect.intersects(settings_rect),
            "{vp_name}: Jalankan Presensi Otomatis {start_rect:?} overlaps scan={scan_rect:?} or settings={settings_rect:?}"
        );
    }
}

fn sample_proof_snapshot() -> GuiSnapshot {
    let mut snap = sample_populated_snapshot();

    let sig1 = "Basis Data|KO001N-Dosen A|07:00:00 s.d. 08:40:00";
    let proof1 = ProofPreview::from_proof(AttendanceProof {
        proof_id: "PRF-20261121-11223344".to_string(),
        sig: sig1.to_string(),
        key: "Basis Data|KO001N-Dosen A".to_string(),
        date: "2026-11-21".to_string(),
        recorded_at: "2026-11-21T07:05:12+07:00".to_string(),
        verified_at: Some("2026-11-21T07:06:00+07:00".to_string()),
        server_date_header: Some("Sat, 21 Nov 2026 00:05:12 GMT".to_string()),
        verification: ProofVerification::PortalVerified,
        nim: "251511025".to_string(),
        student_nama: "RADITHYA MAHESA SYABIL".to_string(),
        jurusan: "D3-Teknik Informatika".to_string(),
        nama_mk: "Basis Data".to_string(),
        mk_code: "25IF201".to_string(),
        dosen: "KO001N-Dosen A".to_string(),
        jam: "07:00:00 s.d. 08:40:00".to_string(),
        tp: "T".to_string(),
        ja: "1".to_string(),
        jb: "2".to_string(),
        kls: "1A".to_string(),
        target_url: "https://akademik.polban.ac.id/ajar/absen".to_string(),
        endpoint_url: Some("https://akademik.polban.ac.id/ajar/absen/absensi_awal".to_string()),
        http_status: Some(200),
        server_response: Some(r#"{"status":"ok"}"#.to_string()),
        server_button_label: Some("Sudah Absen".to_string()),
        attempt: Some(1),
    });

    let sig2 = "Aljabar Linear|KO009N-Dosen E|08:40:00 s.d. 10:20:00";
    let proof2 = ProofPreview::from_proof(AttendanceProof {
        proof_id: "PRF-20261121-AABBCCDD".to_string(),
        sig: sig2.to_string(),
        key: "Aljabar Linear|KO009N-Dosen E".to_string(),
        date: "2026-11-21".to_string(),
        recorded_at: "2026-11-21T08:42:05+07:00".to_string(),
        verified_at: None,
        server_date_header: None,
        verification: ProofVerification::PostAccepted,
        nim: "251511025".to_string(),
        student_nama: "RADITHYA MAHESA SYABIL".to_string(),
        jurusan: "D3-Teknik Informatika".to_string(),
        nama_mk: "Aljabar Linear".to_string(),
        mk_code: "25IF205".to_string(),
        dosen: "KO009N-Dosen E".to_string(),
        jam: "08:40:00 s.d. 10:20:00".to_string(),
        tp: "T".to_string(),
        ja: "3".to_string(),
        jb: "4".to_string(),
        kls: "1A".to_string(),
        target_url: "https://akademik.polban.ac.id/ajar/absen".to_string(),
        endpoint_url: Some("https://akademik.polban.ac.id/ajar/absen/absensi_awal".to_string()),
        http_status: Some(200),
        server_response: Some("ok".to_string()),
        server_button_label: None,
        attempt: Some(1),
    });

    snap.today_date = "2026-11-21".to_string();
    snap.proofs = vec![proof1, proof2];
    snap.courses.push(CourseItem {
        sig: "Sudah|Basis Data|KO099N-Dosen Lain|07:00:00 s.d. 08:40:00".to_string(),
        nama_mk: "Basis Data".to_string(),
        dosen: "Dosen Lain".to_string(),
        jam: "07:00:00 s.d. 08:40:00".to_string(),
        color: Color::Green,
        status_label: "Sudah Absen".to_string(),
        reason: "Sudah absen (dihitung berhasil)".to_string(),
        action_ready: false,
    });
    snap.activities.push(ActivityItem {
        timestamp: "07:05:12".to_string(),
        code: "ABSEN".to_string(),
        message: "Presensi berhasil dicatat untuk Basis Data".to_string(),
        kind: ActivityKind::Ok,
        proof_key: Some(sig1.to_string()),
    });
    snap
}

#[test]
fn attendance_proof_inline_micro_card_activity_link_and_modal_interactions() {
    let ctx = egui::Context::default();
    let mut app = DesktopApp::new_headless(
        &ctx,
        configured_prefs(ThemePreference::Light),
        Box::new(MemoryFallbackStore::new()),
    );
    app.set_test_snapshot(Some(sample_proof_snapshot()));
    assert!(!app.is_proof_modal_open(), "proof modal must not auto-open");

    let size = Vec2::new(1120.0, 760.0);
    let out = run_frames(&ctx, &mut app, size, Vec::new());
    assert_no_horizontal_overflow(&out, size.x, "proof_inline_initial");
    let text = collect_rendered_text(&out);
    assert!(text.contains("07:05:12 · Terverifikasi portal"));
    assert!(text.contains("Lihat Bukti"));
    assert!(text.contains("Bukti"));

    // Only 1 green card in sample_proof_snapshot, so Lihat Bukti appears exactly once.
    let rects = collect_text_rects(&out);
    let lihat_count = rects.iter().filter(|(t, _)| t == "Lihat Bukti").count();
    assert_eq!(
        lihat_count, 1,
        "only green card with proof shows Lihat Bukti"
    );

    // 1. Click "Lihat Bukti" on green card to open modal.
    let lihat_rect = find_text_rect(&rects, "Lihat Bukti").expect("expected Lihat Bukti button");
    let modal_out = run_frames(&ctx, &mut app, size, click_at(lihat_rect.center()));
    assert!(app.is_proof_modal_open());
    assert_no_horizontal_overflow(&modal_out, size.x, "proof_modal_open");

    let modal_text = collect_rendered_text(&modal_out);
    assert!(modal_text.contains("Bukti Kehadiran"));
    assert!(modal_text.contains("Hari ini"));
    assert!(modal_text.contains("PRF-20261121-11223344"));
    assert!(modal_text.contains("Terverifikasi Portal"));
    assert!(modal_text.contains("Status tombol di portal akademik terkonfirmasi: Sudah Absen"));
    assert!(modal_text.contains("1/2"));
    assert!(modal_text.contains("Detail teknis server"));
    assert!(
        !modal_text.contains("https://akademik.polban.ac.id/ajar/absen/absensi_awal"),
        "technical details should be collapsed by default"
    );
    assert!(
        !modal_text.contains("Verifikasi Ulang"),
        "Verifikasi Ulang is shown only for PostAccepted"
    );

    // 2. Toggle "Detail teknis server" open, scroll to reveal the mono box, and verify contents.
    assert!(!app.is_proof_tech_expanded());
    let modal_rects = collect_text_rects(&modal_out);
    let tech_rect =
        find_text_rect(&modal_rects, "Detail teknis server").expect("expected technical toggle");
    let _ = run_frames(&ctx, &mut app, size, click_at(tech_rect.center()));
    assert!(app.is_proof_tech_expanded());
    let screen_center = Pos2::new(size.x * 0.5, size.y * 0.5);
    let tech_out = run_frames(
        &ctx,
        &mut app,
        size,
        vec![
            Event::PointerMoved(screen_center),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, -800.0),
                modifiers: Modifiers::NONE,
            },
        ],
    );
    assert_no_horizontal_overflow(&tech_out, size.x, "proof_modal_tech_expanded");
    let tech_text = collect_rendered_text(&tech_out);
    assert!(tech_text.contains("https://akademik.polban.ac.id/ajar/absen/absensi_awal"));
    assert!(tech_text.contains(r#"{"status":"ok"}"#));

    // 3. Click "Salin Bukti" and verify inline "Tersalin" feedback.
    let copy_rect = find_text_rect(&collect_text_rects(&tech_out), "Salin Bukti")
        .expect("expected Salin Bukti button");
    let copied_out = run_frames(&ctx, &mut app, size, click_at(copy_rect.center()));
    assert!(collect_rendered_text(&copied_out).contains("Tersalin"));

    // 4. Switch to next proof via '>' button (1/2 -> 2/2, PostAccepted).
    let next_rect = find_text_rect(&collect_text_rects(&copied_out), ">")
        .expect("expected next proof '>' button");
    let second_out = run_frames(&ctx, &mut app, size, click_at(next_rect.center()));
    let second_text = collect_rendered_text(&second_out);
    assert!(second_text.contains("2/2"));
    assert!(second_text.contains("PRF-20261121-AABBCCDD"));
    assert!(second_text.contains("Diterima Server"));
    assert!(second_text.contains("Verifikasi Ulang"));
    assert!(second_text.contains("Buka Portal"));

    // Switch back via '<' button (2/2 -> 1/2).
    let prev_rect = find_text_rect(&collect_text_rects(&second_out), "<")
        .expect("expected prev proof '<' button");
    let back_out = run_frames(&ctx, &mut app, size, click_at(prev_rect.center()));
    assert!(collect_rendered_text(&back_out).contains("1/2"));

    // 5. Dismiss with Escape.
    let _ = run_frames(
        &ctx,
        &mut app,
        size,
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert!(
        !app.is_proof_modal_open(),
        "Escape should dismiss proof modal"
    );

    // 6. Open from Activity row click ("Bukti" tag / ABSEN row), then dismiss by backdrop click.
    let main_out = run_frames(&ctx, &mut app, size, Vec::new());
    let bukti_rect = find_exact_text_rect(&collect_text_rects(&main_out), "Bukti")
        .expect("expected Bukti tag on ABSEN activity row");
    let _ = run_frames(&ctx, &mut app, size, click_at(bukti_rect.center()));
    assert!(
        app.is_proof_modal_open(),
        "Activity row click should open proof modal"
    );

    let _ = run_frames(&ctx, &mut app, size, click_at(Pos2::new(6.0, 6.0)));
    assert!(
        !app.is_proof_modal_open(),
        "Backdrop click should dismiss proof modal"
    );

    // 7. Verify unsafe URL scheme hides "Buka Portal" button in proof modal.
    let mut unsafe_snap = sample_proof_snapshot();
    unsafe_snap.proofs[0].target_url = "javascript:alert(1)".to_string();
    app.set_test_snapshot(Some(unsafe_snap));
    let unsafe_main = run_frames(&ctx, &mut app, size, Vec::new());
    let unsafe_lihat = find_text_rect(&collect_text_rects(&unsafe_main), "Lihat Bukti")
        .expect("expected Lihat Bukti button");
    let unsafe_modal = run_frames(&ctx, &mut app, size, click_at(unsafe_lihat.center()));
    assert!(!collect_rendered_text(&unsafe_modal).contains("Buka Portal"));
}

#[test]
fn proof_modal_renders_without_overflow_across_viewports_and_themes() {
    let viewports = [
        Vec2::new(680.0, 540.0),
        Vec2::new(960.0, 640.0),
        Vec2::new(1280.0, 800.0),
        Vec2::new(1920.0, 1080.0),
    ];

    for theme in [ThemePreference::Light, ThemePreference::Dark] {
        let expected_fill = match theme {
            ThemePreference::Dark => Palette::DARK.bg_surface,
            _ => Palette::LIGHT.bg_surface,
        };
        for size in viewports {
            let ctx = egui::Context::default();
            let mut app = DesktopApp::new_headless(
                &ctx,
                configured_prefs(theme),
                Box::new(MemoryFallbackStore::new()),
            );
            app.set_test_snapshot(Some(sample_proof_snapshot()));

            let main_out = run_frames(&ctx, &mut app, size, Vec::new());
            assert_no_horizontal_overflow(&main_out, size.x, &format!("proof_main_{size:?}"));

            let lihat_rect = find_text_rect(&collect_text_rects(&main_out), "Lihat Bukti")
                .expect("expected Lihat Bukti button");
            let modal_out = run_frames(&ctx, &mut app, size, click_at(lihat_rect.center()));
            assert!(app.is_proof_modal_open());
            assert_no_horizontal_overflow(&modal_out, size.x, &format!("proof_modal_{size:?}"));

            let mut rect_shapes = Vec::new();
            for c in &modal_out.shapes {
                collect_rect_shapes(&c.shape, &mut rect_shapes);
            }
            let has_scrim = rect_shapes.iter().any(|(r, fill)| {
                (r.width() - size.x).abs() < 2.0
                    && (r.height() - size.y).abs() < 2.0
                    && fill.a() > 100
            });
            assert!(has_scrim, "{size:?} ({theme:?}): expected dimmed scrim");

            let screen_center = Pos2::new(size.x * 0.5, size.y * 0.5);
            let modal_rect = rect_shapes
                .iter()
                .filter(|(r, fill)| {
                    *fill == expected_fill
                        && r.width() >= 300.0
                        && r.width() <= 780.0
                        && r.height() >= 200.0
                        && (r.center() - screen_center).length() < 8.0
                })
                .map(|(r, _)| *r)
                .max_by(|a, b| a.area().total_cmp(&b.area()))
                .expect("expected centered proof modal rect");

            assert!(
                modal_rect.left() >= 12.0
                    && modal_rect.right() <= size.x - 12.0
                    && modal_rect.top() >= 16.0
                    && modal_rect.bottom() <= size.y - 16.0,
                "{size:?} ({theme:?}): proof modal {modal_rect:?} touched viewport edges"
            );
        }
    }
}

#[test]
fn header_bukti_button_renders_without_overlap_or_overflow_with_zero_classes_in_light_and_dark() {
    let viewports = [
        ("narrow_680", Vec2::new(680.0, 540.0)),
        ("medium_860", Vec2::new(860.0, 640.0)),
        ("medium_960", Vec2::new(960.0, 640.0)),
        ("desktop_1280", Vec2::new(1280.0, 800.0)),
        ("ultrawide_1920", Vec2::new(1920.0, 1080.0)),
    ];

    for theme in [ThemePreference::Light, ThemePreference::Dark] {
        for status in [EngineStatus::Idle, EngineStatus::Monitoring] {
            for (vp_name, size) in viewports {
                let ctx = egui::Context::default();
                let mut app = DesktopApp::new_headless(
                    &ctx,
                    configured_prefs(theme),
                    Box::new(MemoryFallbackStore::new()),
                );
                app.set_test_snapshot(Some(GuiSnapshot {
                    status,
                    last_scan: "08:00:00".to_string(),
                    courses: Vec::new(),
                    proofs: Vec::new(),
                    ..Default::default()
                }));

                let out = run_frames(&ctx, &mut app, size, Vec::new());
                assert_no_horizontal_overflow(&out, size.x, vp_name);
                let rects = collect_text_rects(&out);

                let bukti_rect = find_exact_text_rect(&rects, "Bukti").unwrap_or_else(|| {
                    panic!("{vp_name} ({theme:?}, {status:?}) missing Bukti button")
                });
                let scan_rect = find_text_rect(&rects, "Cek Sekarang")
                    .unwrap_or_else(|| panic!("{vp_name} missing Cek Sekarang"));
                let settings_rect = find_text_rect(&rects, "Pengaturan")
                    .unwrap_or_else(|| panic!("{vp_name} missing Pengaturan"));
                let primary_label = if status.is_active() {
                    "Hentikan"
                } else {
                    "Jalankan Presensi Otomatis"
                };
                let primary_rect = find_text_rect(&rects, primary_label)
                    .unwrap_or_else(|| panic!("{vp_name} missing {primary_label}"));
                let title_rect = find_text_rect(&rects, "Polban Presensi")
                    .unwrap_or_else(|| panic!("{vp_name} missing Polban Presensi"));

                assert!(
                    !bukti_rect.intersects(scan_rect)
                        && !bukti_rect.intersects(settings_rect)
                        && !bukti_rect.intersects(primary_rect)
                        && !bukti_rect.intersects(title_rect),
                    "{vp_name} ({theme:?}, {status:?}): Bukti {bukti_rect:?} overlapped another header element"
                );
            }
        }
    }
}

#[test]
fn header_bukti_empty_state_sample_preview_and_close_resets_sample_mode() {
    let size = Vec2::new(960.0, 680.0);
    let ctx = egui::Context::default();
    let mut app = DesktopApp::new_headless(
        &ctx,
        configured_prefs(ThemePreference::Light),
        Box::new(MemoryFallbackStore::new()),
    );
    app.set_test_snapshot(Some(GuiSnapshot {
        status: EngineStatus::Monitoring,
        last_scan: "08:00:00".to_string(),
        courses: Vec::new(),
        proofs: Vec::new(),
        ..Default::default()
    }));
    assert!(!app.is_proof_modal_open());
    assert!(!app.is_proof_sample_mode());

    // 1. Click header "Bukti" with 0 proofs -> opens calm empty state.
    let main_out = run_frames(&ctx, &mut app, size, Vec::new());
    let bukti_rect = find_exact_text_rect(&collect_text_rects(&main_out), "Bukti")
        .expect("expected header Bukti button");
    let empty_out = run_frames(&ctx, &mut app, size, click_at(bukti_rect.center()));
    assert!(app.is_proof_modal_open());
    assert!(!app.is_proof_sample_mode());
    assert_no_horizontal_overflow(&empty_out, size.x, "proof_empty_state");

    let empty_text = collect_rendered_text(&empty_out);
    assert!(empty_text.contains("Bukti Kehadiran"));
    assert!(empty_text.contains("Belum Ada Bukti Kehadiran Tersimpan"));
    assert!(empty_text.contains("14 hari"));
    assert!(empty_text.contains("Lihat Contoh Bukti"));
    assert!(empty_text.contains("Tutup"));

    // 2. Click "Lihat Contoh Bukti" -> shows unmistakable sample preview banner without real actions.
    let sample_btn = find_text_rect(&collect_text_rects(&empty_out), "Lihat Contoh Bukti")
        .expect("expected Lihat Contoh Bukti button");
    let _ = run_frames(&ctx, &mut app, size, click_at(sample_btn.center()));
    assert!(app.is_proof_modal_open());
    assert!(app.is_proof_sample_mode());
    let sample_out = run_frames_until(
        &ctx,
        &mut app,
        size,
        std::time::Duration::from_secs(3),
        |a, _| a.has_loaded_proof_texture(),
    );
    assert_no_horizontal_overflow(&sample_out, size.x, "proof_sample_mode");

    let sample_text = collect_rendered_text(&sample_out);
    assert!(sample_text.contains("PRATINJAU CONTOH · Bukan bukti kehadiran asli"));
    assert!(sample_text.contains("Pratinjau Halaman Absen"));
    assert!(sample_text.contains("Pratinjau Tabel Portal"));
    assert!(sample_text.contains("Kembali"));
    assert!(sample_text.contains("Tutup"));
    assert!(!sample_text.contains("Verifikasi Ulang"));
    assert!(!sample_text.contains("Buka Portal"));
    assert!(!sample_text.contains("Salin Bukti"));
    assert!(!sample_text.contains("Simpan Gambar"));

    // Scroll down in sample mode to verify receipt details below the sample image.
    let sample_scrolled = run_frames(
        &ctx,
        &mut app,
        size,
        vec![
            Event::PointerMoved(Pos2::new(size.x * 0.5, size.y * 0.5)),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, -400.0),
                modifiers: Modifiers::NONE,
            },
        ],
    );
    assert!(collect_rendered_text(&sample_scrolled).contains("Algoritma & Struktur Data"));

    // 3. Click "Kembali" -> returns to empty state.
    let back_btn = find_text_rect(&collect_text_rects(&sample_out), "Kembali")
        .expect("expected Kembali button");
    let back_out = run_frames(&ctx, &mut app, size, click_at(back_btn.center()));
    assert!(app.is_proof_modal_open());
    assert!(!app.is_proof_sample_mode());
    assert!(collect_rendered_text(&back_out).contains("Belum Ada Bukti Kehadiran Tersimpan"));

    // 4. Re-enter sample preview, close with Escape -> resets sample_mode and drops GPU texture.
    let sample_btn2 = find_text_rect(&collect_text_rects(&back_out), "Lihat Contoh Bukti")
        .expect("expected Lihat Contoh Bukti button");
    let _ = run_frames(&ctx, &mut app, size, click_at(sample_btn2.center()));
    assert!(app.is_proof_sample_mode());
    let _ = run_frames_until(
        &ctx,
        &mut app,
        size,
        std::time::Duration::from_secs(3),
        |a, _| a.has_loaded_proof_texture(),
    );

    let _ = run_frames(
        &ctx,
        &mut app,
        size,
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert!(!app.is_proof_modal_open());
    assert!(!app.is_proof_sample_mode());
    assert!(
        !app.has_loaded_proof_texture(),
        "closing modal must drop GPU texture immediately"
    );

    // 5. Reopen via header "Bukti" -> opens empty state (not sample mode).
    let reopened_out = run_frames(&ctx, &mut app, size, click_at(bukti_rect.center()));
    assert!(app.is_proof_modal_open());
    assert!(!app.is_proof_sample_mode());
    assert!(collect_rendered_text(&reopened_out).contains("Belum Ada Bukti Kehadiran Tersimpan"));
}

#[test]
fn historical_proof_in_modal_shows_date_pill_hides_reverify_and_never_attaches_to_today_card() {
    let size = Vec2::new(1120.0, 760.0);
    let ctx = egui::Context::default();
    let mut app = DesktopApp::new_headless(
        &ctx,
        configured_prefs(ThemePreference::Light),
        Box::new(MemoryFallbackStore::new()),
    );

    let sig_basis = "Basis Data|KO001N-Dosen A|07:00:00 s.d. 08:40:00".to_string();
    let yesterday_post = ProofPreview::from_proof(AttendanceProof {
        proof_id: "PRF-20261120-YESTERDAY".to_string(),
        sig: sig_basis.clone(),
        key: "Basis Data|KO001N-Dosen A".to_string(),
        date: "2026-11-20".to_string(),
        recorded_at: "2026-11-20T07:05:12+07:00".to_string(),
        verified_at: None,
        server_date_header: None,
        verification: ProofVerification::PostAccepted,
        nim: "251511025".to_string(),
        student_nama: "RADITHYA MAHESA SYABIL".to_string(),
        jurusan: "D3-Teknik Informatika".to_string(),
        nama_mk: "Basis Data".to_string(),
        mk_code: "25IF201".to_string(),
        dosen: "KO001N-Dosen A".to_string(),
        jam: "07:00:00 s.d. 08:40:00".to_string(),
        tp: "T".to_string(),
        ja: "1".to_string(),
        jb: "2".to_string(),
        kls: "1A".to_string(),
        target_url: "https://akademik.polban.ac.id/ajar/absen".to_string(),
        endpoint_url: Some("https://akademik.polban.ac.id/ajar/absen/absensi_awal".to_string()),
        http_status: Some(200),
        server_response: Some("ok".to_string()),
        server_button_label: None,
        attempt: Some(1),
    });

    let older_verified = ProofPreview::from_proof(AttendanceProof {
        proof_id: "PRF-20261118-OLDER".to_string(),
        sig: "Aljabar Linear|KO009N-Dosen E|08:40:00 s.d. 10:20:00".to_string(),
        key: "Aljabar Linear|KO009N-Dosen E".to_string(),
        date: "2026-11-18".to_string(),
        recorded_at: "2026-11-18T08:42:05+07:00".to_string(),
        verified_at: Some("2026-11-18T08:43:00+07:00".to_string()),
        server_date_header: None,
        verification: ProofVerification::PortalVerified,
        nim: "251511025".to_string(),
        student_nama: "RADITHYA MAHESA SYABIL".to_string(),
        jurusan: "D3-Teknik Informatika".to_string(),
        nama_mk: "Aljabar Linear".to_string(),
        mk_code: "25IF205".to_string(),
        dosen: "KO009N-Dosen E".to_string(),
        jam: "08:40:00 s.d. 10:20:00".to_string(),
        tp: "T".to_string(),
        ja: "3".to_string(),
        jb: "4".to_string(),
        kls: "1A".to_string(),
        target_url: "https://akademik.polban.ac.id/ajar/absen".to_string(),
        endpoint_url: None,
        http_status: Some(200),
        server_response: None,
        server_button_label: Some("Sudah Absen".to_string()),
        attempt: Some(1),
    });

    let mut snap = sample_populated_snapshot();
    snap.today_date = "2026-11-21".to_string();
    snap.proofs = vec![yesterday_post, older_verified];
    app.set_test_snapshot(Some(snap));

    let main_out = run_frames(&ctx, &mut app, size, Vec::new());
    assert_no_horizontal_overflow(&main_out, size.x, "historical_main");
    let main_text = collect_rendered_text(&main_out);
    assert!(main_text.contains("Bukti (2)"));
    assert!(
        !main_text.contains("Lihat Bukti"),
        "historical proof must never attach inline Lihat Bukti to today's card"
    );
    assert!(!main_text.contains("Diterima server"));

    // Click "Bukti (2)" in header -> opens modal on newest historical proof (2026-11-20).
    let bukti_btn = find_exact_text_rect(&collect_text_rects(&main_out), "Bukti (2)")
        .expect("expected Bukti (2) header button");
    let modal_out = run_frames(&ctx, &mut app, size, click_at(bukti_btn.center()));
    assert!(app.is_proof_modal_open());
    assert_no_horizontal_overflow(&modal_out, size.x, "historical_modal_1");

    let modal_text = collect_rendered_text(&modal_out);
    assert!(modal_text.contains("2026-11-20"));
    assert!(!modal_text.contains("Hari ini"));
    assert!(modal_text.contains("PRF-20261120-YESTERDAY"));
    assert!(modal_text.contains("1/2"));
    assert!(
        !modal_text.contains("Verifikasi Ulang"),
        "historical PostAccepted proof must NOT offer Verifikasi Ulang"
    );

    // Navigate to 2/2 -> 2026-11-18.
    let next_btn =
        find_text_rect(&collect_text_rects(&modal_out), ">").expect("expected '>' nav button");
    let second_out = run_frames(&ctx, &mut app, size, click_at(next_btn.center()));
    let second_text = collect_rendered_text(&second_out);
    assert!(second_text.contains("2/2"));
    assert!(second_text.contains("2026-11-18"));
    assert!(second_text.contains("PRF-20261118-OLDER"));
}

fn run_frames_until(
    ctx: &egui::Context,
    app: &mut DesktopApp,
    size: Vec2,
    timeout: std::time::Duration,
    mut pred: impl FnMut(&DesktopApp, &egui::FullOutput) -> bool,
) -> egui::FullOutput {
    let start = std::time::Instant::now();
    loop {
        let out = run_frames(ctx, app, size, Vec::new());
        if pred(app, &out) {
            return out;
        }
        assert!(
            start.elapsed() < timeout,
            "timed out waiting for headless UI condition"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn proof_modal_screenshot_preview_zoom_save_and_empty_live_preview() {
    let dir = std::env::temp_dir().join(format!("polban_absen_gui_ui_shot_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let state_file = dir.join("clicked.json");

    let viewports = [
        Vec2::new(680.0, 540.0),
        Vec2::new(960.0, 640.0),
        Vec2::new(1280.0, 800.0),
        Vec2::new(1920.0, 1080.0),
    ];

    for theme in [ThemePreference::Light, ThemePreference::Dark] {
        let expected_fill = match theme {
            ThemePreference::Dark => Palette::DARK.bg_surface,
            _ => Palette::LIGHT.bg_surface,
        };
        for size in viewports {
            let ctx = egui::Context::default();
            let mut prefs = configured_prefs(theme);
            prefs.state_file = Some(state_file.clone());
            let mut app =
                DesktopApp::new_headless(&ctx, prefs, Box::new(MemoryFallbackStore::new()));
            let snap = sample_proof_snapshot();
            let proof1_id = snap.proofs[0].proof_id.clone();
            let png_path = app.controller().screenshot_path_for(&proof1_id);
            absen_gui::screenshot::write_png_atomic(
                &png_path,
                absen_gui::screenshot::render_sample_attendance_png(),
            )
            .unwrap();
            app.set_test_snapshot(Some(snap));

            let main_out = run_frames(&ctx, &mut app, size, Vec::new());
            let lihat = find_text_rect(&collect_text_rects(&main_out), "Lihat Bukti")
                .expect("expected Lihat Bukti button");
            let _ = run_frames(&ctx, &mut app, size, click_at(lihat.center()));
            let modal_out = run_frames_until(
                &ctx,
                &mut app,
                size,
                std::time::Duration::from_secs(3),
                |a, _| a.has_loaded_proof_texture(),
            );
            assert_no_horizontal_overflow(&modal_out, size.x, &format!("shot_modal_{size:?}"));
            let text = collect_rendered_text(&modal_out);
            assert!(text.contains("Pratinjau Halaman Absen"));
            assert!(text.contains("Perbesar"));
            assert!(text.contains("Simpan Gambar"));

            // Toggle zoom -> Sesuaikan, verify no overflow.
            let zoom_btn = find_text_rect(&collect_text_rects(&modal_out), "Perbesar")
                .expect("expected Perbesar button");
            let zoomed_out = run_frames(&ctx, &mut app, size, click_at(zoom_btn.center()));
            assert!(app.is_proof_image_zoomed());
            assert_no_horizontal_overflow(&zoomed_out, size.x, &format!("shot_zoom_{size:?}"));
            assert!(collect_rendered_text(&zoomed_out).contains("Sesuaikan"));
            let mut zoom_rects = Vec::new();
            for c in &zoomed_out.shapes {
                collect_rect_shapes(&c.shape, &mut zoom_rects);
            }
            let screen_center = Pos2::new(size.x * 0.5, size.y * 0.5);
            let modal_w_zoomed = zoom_rects
                .iter()
                .filter(|(r, fill)| {
                    *fill == expected_fill
                        && r.width() >= 300.0
                        && r.height() >= 200.0
                        && (r.center() - screen_center).length() < 8.0
                })
                .map(|(r, _)| r.width())
                .fold(0.0_f32, f32::max);
            assert!(
                modal_w_zoomed > 300.0 && modal_w_zoomed <= 762.0,
                "zoomed modal width {modal_w_zoomed} out of bounds for {size:?}"
            );

            let fit_btn = find_text_rect(&collect_text_rects(&zoomed_out), "Sesuaikan")
                .expect("expected Sesuaikan button");
            let _ = run_frames(&ctx, &mut app, size, click_at(fit_btn.center()));
            assert!(!app.is_proof_image_zoomed());

            // Test non-blocking Simpan Gambar with deterministic destination path.
            let saved_dest = dir.join(format!("saved_{}x{}.png", size.x as u32, size.y as u32));
            let _ = std::fs::remove_file(&saved_dest);
            app.set_test_save_path(Some(saved_dest.clone()));
            let latest_out = run_frames(&ctx, &mut app, size, Vec::new());
            let save_btn = find_text_rect(&collect_text_rects(&latest_out), "Simpan Gambar")
                .expect("expected Simpan Gambar button");
            let _ = run_frames(&ctx, &mut app, size, click_at(save_btn.center()));
            let saved_out = run_frames_until(
                &ctx,
                &mut app,
                size,
                std::time::Duration::from_secs(3),
                |_, o| collect_rendered_text(o).contains("Gambar tersimpan"),
            );
            assert!(collect_rendered_text(&saved_out).contains("Gambar tersimpan"));
            assert!(saved_dest.is_file());
            assert!(std::fs::read(&saved_dest)
                .unwrap()
                .starts_with(b"\x89PNG\r\n\x1a\n"));

            // Switching to proof 2/2 (no PNG cached) must drop proof 1's GPU texture.
            let next_btn =
                find_text_rect(&collect_text_rects(&saved_out), ">").expect("expected '>' button");
            let after_next_out = run_frames(&ctx, &mut app, size, click_at(next_btn.center()));
            assert!(
                !app.has_loaded_proof_texture(),
                "switching proof must drop previous GPU texture"
            );

            // Closing modal must leave zero loaded textures.
            let close_btn = find_text_rect(&collect_text_rects(&after_next_out), "Tutup")
                .expect("expected Tutup button");
            let _ = run_frames(&ctx, &mut app, size, click_at(close_btn.center()));
            assert!(!app.is_proof_modal_open());
            assert!(!app.has_loaded_proof_texture());
        }
    }

    // Verify empty state shows "Preview Halaman Absen" when snapshot.has_portal_snapshot is true.
    let ctx = egui::Context::default();
    let mut app = DesktopApp::new_headless(
        &ctx,
        configured_prefs(ThemePreference::Light),
        Box::new(MemoryFallbackStore::new()),
    );
    app.set_test_snapshot(Some(GuiSnapshot {
        status: EngineStatus::Monitoring,
        has_portal_snapshot: true,
        portal_snapshot_captured_at: Some("2026-11-21T08:00:00+07:00".to_string()),
        proofs: Vec::new(),
        ..Default::default()
    }));
    let size = Vec2::new(960.0, 640.0);
    let main_out = run_frames(&ctx, &mut app, size, Vec::new());
    let bukti_btn = find_exact_text_rect(&collect_text_rects(&main_out), "Bukti")
        .expect("expected Bukti header button");
    let empty_out = run_frames(&ctx, &mut app, size, click_at(bukti_btn.center()));
    assert_no_horizontal_overflow(&empty_out, size.x, "empty_with_portal_snapshot");
    let empty_text = collect_rendered_text(&empty_out);
    assert!(empty_text.contains("Preview Halaman Absen"));
    assert!(empty_text.contains("Lihat Contoh Bukti"));

    let _ = std::fs::remove_dir_all(&dir);
}

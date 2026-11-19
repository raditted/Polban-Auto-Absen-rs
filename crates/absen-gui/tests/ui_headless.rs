//! Headless UI rendering, responsive geometry, and interaction tests using `egui::Context::default()`.

use absen_core::{Color, UserInfo};
use absen_gui::bridge::{ActivityItem, ActivityKind, CourseItem, EngineStatus, GuiSnapshot};
use absen_gui::credentials::MemoryFallbackStore;
use absen_gui::prefs::{AppPrefs, DarkThemePreset, LightThemePreset, ThemePreference};
use absen_gui::theme::Palette;
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
            },
            ActivityItem {
                timestamp: "09:52:15".to_string(),
                code: "ENGINE".to_string(),
                message: "HTTP aktif. interval=60s loop=True".to_string(),
                kind: ActivityKind::Info,
            },
            ActivityItem {
                timestamp: "09:55:19".to_string(),
                code: "SCAN".to_string(),
                message: "Scan awal: 1 siap absen | 1 sudah absen | 1 belum buka | 1 belum waktunya"
                    .to_string(),
                kind: ActivityKind::Info,
            },
        ],
        last_error: None,
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
        assert!(text.contains("Simpan & Mulai Pantau"));
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

            assert!(text.contains("Jadwal Kuliah Hari Ini"), "{name} missing Jadwal");
            if expects_activity_visible {
                assert!(text.contains("Riwayat Aktivitas"), "{name} missing Activity");
            }
            assert!(text.contains("Portal Terhubung"), "{name} missing clean portal status");
            assert!(text.contains("Cek berikutnya"), "{name} missing next-check metric");
            assert!(text.contains("16 dtk"), "{name} missing countdown unit dtk");
            assert!(!text.contains("OK: Authenticated"), "{name} leaked raw auth string");
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
            assert!(has_scrim, "{size:?} ({theme:?}): expected dimmed backdrop scrim");

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

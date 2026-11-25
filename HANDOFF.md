# Cross-OS Handoff Context (Windows <-> Linux)

Last updated: 2026-11-25 (Linux session)

## 1. Current Repository State

- **Base branch:** `main` (`50bcb0c`: `feat(gui,linux): auto XWayland/tray host detection, tiling window controls, DPI fix, and cross-OS zoom/volume sliders`, package version `3.3.1`)
- **Working tree:** Clean after committing `HANDOFF.md` and pushing to `origin/main` (local Linux `--release` binaries built in `target/release/`).
- **Current focus:** Cross-OS verification via GitHub Actions CI (`windows-latest`, `macos-latest`, `ubuntu-latest`) and live Windows smoke-testing of the new UI zoom and notification volume sliders.

### Recent Milestones
- `50bcb0c`: Auto-selected X11/XWayland when `DISPLAY` is reachable (with `WINIT_X11_SCALE_FACTOR=1` default DPI guard), added live `StatusNotifierWatcher` + `_NET_SYSTEM_TRAY` tray host detection, added Linux tiling/borderless in-app window controls (`ui/titlebar.rs`), aligned `StartupWMClass`/`app_id` as `polban-presensi`, and added cross-OS Settings sliders for UI zoom (`70..=150%`, synced with `Ctrl/Cmd +/-/0`) and notification volume (`0..=100%`).
- `63b7e39`: Added tracked English [HANDOFF.md](HANDOFF.md) for cross-OS context sync.
- `f5ee2c1`: Upgraded GitHub Actions (`actions/checkout@v6`, `actions/upload-artifact@v7`, `actions/download-artifact@v8`, `softprops/action-gh-release@v3`) to Node 24 runtimes.
- `0cab29e` (`v3.3.1`): Fixed Linux Wayland close-to-tray, isolated GTK tray thread (`polban-gtk-tray`), dropped `libxdo`, and fixed cross-OS UI test.
- `af717c2`: Allowed deprecated `activateIgnoringOtherApps` on macOS 11+ for CI clippy.
- `a348f83` (`v3.3.0`): Added Windows/macOS custom pixel-art toasts with embedded audio and Fedora RPM packaging (`linux/polban-presensi.spec`).
- `b4fe370`: Added tracked English [AGENTS.md](AGENTS.md).

## 2. Cross-OS Continuation Notes

- **Canonical rules & architecture:** see [AGENTS.md](AGENTS.md) and [DESIGN.md](DESIGN.md) (automatically loaded by Antigravity on all OSes).
- **Filesystem:** clone the repository on a native Linux filesystem (`ext4`/`btrfs`) rather than compiling directly on a mounted Windows NTFS partition.
- **Linux `absen-gui` build dependencies:**
  - Debian/Ubuntu: `libgtk-3-dev libayatana-appindicator3-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libgl1-mesa-dev libdbus-1-dev`
  - Fedora: see `BuildRequires` in `linux/polban-presensi.spec`.
- **Mandatory gates before committing:**
  ```bash
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```
- **Local release build & Windows binary sync:**
  ```bash
  cargo build --release --bin polban-absen --bin polban-absen-gui
  ```
  - On Windows, all batch launchers prefer `windows/*.exe` over `target/release/*.exe`. Always check `Get-Process` (`polban-absen-gui`, `polban-absen`) without killing active sessions, copy `target/release/*.exe` to `windows/*.exe` when unlocked, and verify matching SHA256 hashes (see [AGENTS.md](AGENTS.md) Section 3).
- **Linux runtime invariants:**
  - `main.rs` (`resolve_linux_window_backend_and_xft_dpi`, `configure_linux_x11_scale_factor`) respects explicit `WINIT_UNIX_BACKEND=x11|wayland` (with safe fallback) and otherwise defaults to X11/XWayland (`with_x11()`) when `DISPLAY` is non-empty and `XOpenDisplay` succeeds (via `x11-dl`), falling back to native Wayland (`with_wayland()`) when `DISPLAY` is absent or X11 fails. On the `X11` backend, if `WINIT_X11_SCALE_FACTOR` is unset and neither XSETTINGS `Xft/DPI` nor `RESOURCE_MANAGER`/`.Xresources` `Xft.dpi` is configured, it sets `WINIT_X11_SCALE_FACTOR=1` on the single-threaded main thread before spawning threads or `EventLoop` so `winit` does not fall back to raw XRandR millimeter DPI. Linux viewport sets `with_app_id("polban-presensi")`, setting both Wayland `app_id` and X11/XWayland `WM_CLASS` class to `"polban-presensi"`.
  - `LinuxTrayBackend` runs `gtk::init()` and `gtk::main()` on its dedicated `"polban-gtk-tray"` thread; never initialize GTK on the main `eframe`/`winit` thread.
  - `app.rs` detects the actual runtime backend via `detect_window_backend(cc)` (`RawDisplayHandle`) and combines it with live Linux tray host detection (`org.kde.StatusNotifierWatcher` via `gio::bus_watch_name`, plus X11 `_NET_SYSTEM_TRAY_S<screen>` fallback). On native Wayland or trayless Linux desktops (e.g. GNOME Wayland without an AppIndicator extension), `effective_hide_to_tray_supported()` is `false` so closing exits cleanly and `--minimized` falls back to `Minimized(true)`.
  - `platform/mod.rs` (`needs_window_controls` / `needs_window_controls_from_env`, with `POLBAN_WINDOW_CONTROLS=1|0|auto` override) enables `ui/titlebar.rs` (`TopBottomPanel::top("linux_window_titlebar")`) only on Linux tiling/borderless WMs/compositors (Hyprland, Niri, Sway, i3, River, bspwm, dwm, etc.) and disables it on Windows, macOS, and native-titlebar Linux DEs (GNOME, KDE, XFCE, etc.). Its Close button shares `AppCore::handle_window_close` with native window close.
- **Cross-OS UI zoom & notification volume invariants:**
  - `prefs.rs`, `app.rs`, and `ui/settings.rs`: `ui_zoom_pct` (`70..=150%`, default `100%`) and `notification_volume` (`0..=100%`, default `100%`) use backward-compatible serde defaults and clamping deserializers. Built-in `zoom_with_keyboard` is disabled in `DesktopApp::new` and `new_headless` so `Ctrl/Cmd +` / `=` / `-` / `0` step deterministically (`10%`, ignoring `AltGr`), persist immediately, and stay synced with the Settings slider; `PreviewZoom` reverts on Cancel/Esc/backdrop dismiss.
  - `platform/sound.rs` and `platform/notify.rs`: `sound.rs` decodes the embedded MP3 once (`DECODED_PCM` `OnceLock`) and caches `'static` per-volume WAV slices (`[OnceLock<Option<Vec<u8>>>; 101]`), returning silently at `0%` and preserving bit-exact PCM at `100%` (using `(v / 100.0)^1.5` gain for `1..99%`). Windows `PlaySoundW(SND_MEMORY | SND_ASYNC | SND_NODEFAULT)` safely reads `'static` WAV slices; macOS (`afplay`) and Linux (`paplay`/`pw-play`/`aplay`) write atomic volume-keyed temp WAV files (`polban-presensi-notification-v{clamped}.wav`). Desktop notifications use `toast_win32.rs` (Windows), `toast_macos.rs` (macOS), and `notify-rust` (Linux/BSD D-Bus), with `Uji Notifikasi` reflecting the live Settings volume slider immediately.

## 3. Active Focus / Next TODOs

- **Verify GitHub Actions CI (`ci.yml`):** confirm `windows-latest`, `macos-latest`, and `ubuntu-latest` checks pass for the pushed `main` commits (`50bcb0c` and this `HANDOFF.md` commit).
- **Next Windows session:**
  1. Run `git pull origin main`.
  2. Build `--release` (`cargo build --release --bin polban-absen --bin polban-absen-gui`) and sync `target/release/*.exe` -> `windows/*.exe` with SHA256 verification (checking running processes first).
  3. Live-test the new Settings UI Zoom slider (`Ctrl +`/`-`/`0`, live preview, Save/Cancel) and Notification Volume slider (`0%` mute, `50%`, `100%` via `PlaySoundW` + `Uji Notifikasi`), and confirm no regression in Win32 tray close/restore or custom pixel-art toasts.
- **Optional future release:** bump version (`3.3.2` or `3.4.0`) across `Cargo.toml`, `Cargo.lock`, and `windows/installer.iss`, then push a release tag when ready to publish updated artifacts.
- **Minor pre-existing macOS note from audit:** clicking a custom macOS toast (`toast_macos.rs`) after close-to-tray invokes `wake_callback` to reveal the window without resetting `controller.set_window_visible(true)` (unlike `TrayCommand::ShowWindow`), so 1s `Countdown` ticks stay suppressed until shown via tray or another focus path.

---

### Usage
1. Before switching OSes, ask the agent: *"Update `HANDOFF.md` for switching to Linux/Windows"*, then commit and push.
2. On the other OS, run `git pull` and mention `@HANDOFF.md` in the first message of the new Antigravity session.

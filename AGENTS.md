# AGENTS.md

Working guide for AI coding agents in the **Polban Auto Absen / Polban Presensi** repository (`raditted/Polban-Auto-Absen-rs`).

## 1. Repository Map

Rust workspace (`edition = 2021`, `stable` toolchain in `rust-toolchain.toml`):

- `crates/absen-core/` — core library with no UI dependencies: `config`, `state` (atomic writes), `model`, `text`, `parser`, `client` (HTTP `reqwest` + `rustls`), `engine`, `event`.
- `crates/absen-cli/` — `polban-absen` binary (CLI + `ratatui`/`crossterm` TUI; `build.rs` embeds `assets/app-icon.ico` on Windows).
- `crates/absen-gui/` — `polban-absen-gui` desktop binary (`eframe`/`egui`):
  - `app.rs`, `main.rs`, `bridge.rs`, `credentials.rs`, `prefs.rs`, `theme.rs`, `icon.rs`, `updater.rs`
  - `platform/{win32,tray,notify,autostart}.rs`
  - `ui/{mod,header,jadwal,activity,settings,icons}.rs`
  - `build.rs` embeds `assets/app-icon.ico` on Windows.
- `assets/` — canonical app icons (`app-icon.ico`, `app-icon.png`) shared by GUI, CLI, Inno Setup, macOS `.app`, and Linux `.desktop`.
- `windows/`, `linux/`, `termux/` — platform launchers, install scripts, and `windows/installer.iss` (Inno Setup 6).
- `extension/` — Chrome MV3 extension (plain JavaScript).
- `legacy/python/` — archived v2.0 implementation. **Do not edit or maintain.**
- `DESIGN.md` — color tokens, WCAG 2.x AA contrast checks, typography, and UI breakpoints.

## 2. Commands & Mandatory Gates

Run from the repository root before finishing or committing (matches `.github/workflows/ci.yml`):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Local release build:

```bash
cargo build --release --bin polban-absen --bin polban-absen-gui
```

When modifying `extension/`:

```bash
node --check extension/*.js
```

Cross-OS notes:
- CI runs `clippy -D warnings` on **Windows, Linux, and macOS**. Always guard Windows-only code/imports with `#[cfg(target_os = "windows")]` (or `#[cfg(not(target_os = "windows"))]`) so Linux/macOS builds do not fail on `dead_code` or `unused` warnings.
- On Linux, building/testing `absen-gui` requires: `libgtk-3-dev libayatana-appindicator3-dev libxdo-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libgl1-mesa-dev libdbus-1-dev`.

## 3. Security, Privacy & Offline Testing

- **100% offline tests**: never contact `akademik.polban.ac.id` or live GitHub from the test suite. Use synthetic HTML fixtures (`crates/absen-core/tests/fixtures/`) and `httpmock`.
- **Never commit secrets or local runtime files**: `.env`, `.clicked.json`, `*-dump.html`, `*.lnk`, or personal credentials.
- **Password isolation**: GUI passwords may only be stored via `CredentialStore` (`crates/absen-gui/src/credentials.rs`, OS keyring), never serialized into `prefs.json`, and must always be redacted in `Debug`/log output.
- **Do not kill active attendance processes without permission**: `windows/polban-absen.exe` or `windows/polban-absen-gui.exe` may be actively running for the user's live attendance session. If a target binary is locked when copying build output, report it before running `Stop-Process`.

## 4. Architecture & Platform Invariants

### Windows System Tray (`platform/win32.rs`, `platform/tray.rs`, `app.rs`)
- When an `eframe`/`winit` window is hidden (`SW_HIDE` / `Visible(false)`), Windows stops delivering `WM_PAINT`, so `eframe::App::update()` **does not run**. Do not rely on polling tray events inside `update()`.
- Register `MenuEvent` and `TrayIconEvent` handlers **before** calling `TrayIconBuilder::build()`. Dispatch tray actions on the UI thread via the hidden `HWND_MESSAGE` window (`MessageWindow` + `WM_APP_*`).
- Closing the window (`X`) while `minimize_to_tray == true` hides the window to the tray and sends a non-blocking `Notifier::notify_minimized_to_tray()` notification. Exiting via the tray menu must shut down cleanly without deadlocking.

### Never Block the UI Thread (`absen-gui`)
- Slow operations (HTTP calls, `notify-rust`, `rfd` file dialogs, update checks/downloads, `EngineController::stop`) must be non-blocking or strictly bounded (bounded stop + background reaper in `bridge.rs`) so Windows never marks the app *Not Responding*.

### Non-Blocking Auto-Updater & Single-Instance (`updater.rs`, `main.rs`)
- **Never add a blocking update gate on startup** (unlike Discord). The app must open immediately and remain usable when offline or when an update is ignored.
- The header renders no update button for `Idle`/`Checking`/`UpToDate`; show a pill only for `Available` (`Update vX.Y.Z`), `Downloading`, `ReadyToRestart` (`Restart untuk Update`), or `Failed`.
- Verify downloaded release artifacts against `SHA256SUMS.txt` from the same release before installing.
- Before spawning the new binary on restart, call `SingleInstanceHandle::release()` first so the new process does not exit thinking the old instance still holds the lock. Clean up leftover `*.old` binaries on startup.

### Core & TUI Resilience (`absen-core`, `absen-cli`)
- Validate numeric config values (reject `NaN`, `±Inf`, negatives) before passing to `Duration` or `rand::gen_range`; use `checked_add` for `Instant` arithmetic; persist state atomically (temp file + rename).
- Truncate strings on valid UTF-8 character boundaries.
- TUI must always restore terminal state (raw mode, alternate screen, DECSET 1007) via an RAII guard and a `Once`-guarded panic hook, with a bounded worker shutdown wait.

## 5. UI/UX & Documentation Rules

- Follow the color tokens and WCAG 2.x AA contrast thresholds (`>= 4.5:1` text, `>= 3.0:1` interactive/badge borders) in `DESIGN.md` and `crates/absen-gui/src/theme.rs` across all Light and Dark presets.
- Keep responsive layouts overlap-free (`min 640×520`, centered container `<= 1280px` on wide screens, 2 columns `>= 860px`, single-column stack `< 860px`). Any layout change must pass geometry tests in `crates/absen-gui/tests/ui_headless.rs`.
- Semantic status colors: `Sudah Absen` (emerald/green), `Siap Absen` (blue), `Belum Buka` (amber), `Belum Waktunya` (neutral slate + clock icon — **not error red**), `Gagal` (red).
- The primary start CTA label is **`Jalankan Presensi Otomatis`**.
- Keep user-facing Indonesian copy (`README.md`, platform docs) direct, concrete, and free of AI slop, hype, excessive emoji, or em dashes. Keep `LICENSE` credits clean without tags like `(Rust rewrite ...)`.

## 6. Version & Release Packaging Sync

When bumping the version or changing release asset names, keep all of the following in sync:
1. `Cargo.toml` (`[workspace.package].version`), `Cargo.lock`, and `windows/installer.iss` (`#define AppVersion`).
2. `.github/workflows/release.yml` (generates `SHA256SUMS.txt` and the download table):
   - Windows GUI: `Polban-Presensi-Setup-Windows-x64.exe` (per-user Inno Setup, installs `Polban Presensi.exe`) & `Polban-Presensi-Portable-Windows-x64.zip`
   - macOS GUI: `Polban-Presensi-macOS-AppleSilicon.zip` & `Polban-Presensi-macOS-Intel.zip`
   - Linux GUI: `Polban-Presensi-Linux-x64.tar.gz`
   - CLI/Termux: `polban-absen-<target>`
3. Asset consumers: `crates/absen-gui/src/updater.rs`, `windows/install.bat`, `linux/install.sh`, `README.md`, and `windows/README.md`.

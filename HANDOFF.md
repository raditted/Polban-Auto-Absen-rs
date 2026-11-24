# Cross-OS Handoff Context (Windows <-> Linux)

Last updated: 2026-11-24 (Windows session)

## 1. Current Repository State

- **Branch:** `main` (synced with `origin/main`)
- **Latest code/CI commit:** `f5ee2c1` — `ci: upgrade GitHub Actions to Node 24 runtimes`
- **Working tree:** clean (excluding local untracked artifacts such as `windows/*.lnk`).

### Recent Milestones
- `f5ee2c1`: Upgraded GitHub Actions (`actions/checkout@v6`, `actions/upload-artifact@v7`, `actions/download-artifact@v8`, `softprops/action-gh-release@v3`) to Node 24 runtimes.
- `0cab29e` (`v3.3.1`): Fixed Linux Wayland close-to-tray, isolated GTK tray thread (`polban-gtk-tray`), dropped `libxdo`, and fixed cross-OS UI test.
- `af717c2`: Allowed deprecated `activateIgnoringOtherApps` on macOS 11+ for CI clippy.
- `a348f83` (`v3.3.0`): Added Windows/macOS custom pixel-art toasts with embedded audio and Fedora RPM packaging (`linux/polban-presensi.spec`).
- `b4fe370`: Added tracked English [AGENTS.md](AGENTS.md).

## 2. Linux Continuation Notes

- **Canonical rules & architecture:** see [AGENTS.md](AGENTS.md) and [DESIGN.md](DESIGN.md) (automatically loaded by Antigravity on both OSes).
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
- **Local release build:**
  ```bash
  cargo build --release --bin polban-absen --bin polban-absen-gui
  ```
- **Linux runtime invariants:**
  - `LinuxTrayBackend` runs `gtk::init()` and `gtk::main()` on its dedicated `"polban-gtk-tray"` thread; never initialize GTK on the main `eframe`/`winit` thread.
  - On native Wayland (`WAYLAND_DISPLAY` set and `WINIT_UNIX_BACKEND != "x11"`), `supports_hide_to_tray()` returns `false` so closing the window exits cleanly instead of hanging.
  - Linux desktop notifications use `notify-rust` (D-Bus), while Windows and macOS use custom corner toasts (`toast_win32.rs`, `toast_macos.rs`).

## 3. Active Focus / Next TODOs

- *(No pending tasks. Update this section before switching OSes so the next session can resume immediately.)*

---

### Usage
1. Before switching OSes, ask the agent: *"Update `HANDOFF.md` for switching to Linux/Windows"*, then commit and push.
2. On the other OS, run `git pull` and mention `@HANDOFF.md` in the first message of the new Antigravity session.

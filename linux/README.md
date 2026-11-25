# Polban Auto Absen untuk Linux (Terminal TUI)

Auto-login + auto-klik tombol absen **biru** di `akademik.polban.ac.id/ajar/absen` menggunakan binary native Rust (`polban-absen`) dengan **live terminal dashboard** (`ratatui`).

---

## Tampilan

<p align="center">
  <img src="preview.png" alt="Polban Auto Absen Linux Dashboard" width="100%">
</p>

Dashboard berjalan di **alternate terminal buffer** (seperti `htop`), mendukung scroll (`↑`/`↓`, `PgUp`/`PgDn`, `Home`/`End`, `j`/`k`/`g`/`G`/spasi/`b`), dan responsif terhadap lebar terminal (4 tingkat kolom tabel jadwal).

---

## Quick Start

```bash
cd linux

# 1. Unduh release binary (CLI static musl + Desktop GUI) atau build via Cargo + siapkan .env
bash install.sh
# Opsional: tambahkan --desktop untuk membuat shortcut ~/.local/share/applications/polban-presensi.desktop
# bash install.sh --desktop

# 2A. Jalankan Aplikasi Desktop Polban Presensi (GUI)
bash run-gui.sh

# 2B. Atau mode CLI/TUI: edit .env lalu jalankan
nano .env
bash run.sh --once
bash run.sh
```

---

## Argumen CLI

| Argumen | Keterangan |
| --- | --- |
| `--engine http` | Engine HTTP ringan (default; satu-satunya engine di v3.0) |
| `--once` | Sekali jalan lalu exit (tanpa alternate screen; cocok untuk cron jika digabung `--plain`) |
| `--plain` | Paksa mode plain text (tanpa live TUI) |
| `--no-alt-screen` | Gambar dashboard di layar biasa (tanpa alternate screen) |
| `--ascii` | Pakai box border ASCII (`+ - |`) |
| `--dump [FILE]` | Simpan HTML halaman absen ke file lalu exit (diagnostik) |

---

## File

| File | Fungsi |
| --- | --- |
| `polban-absen` | Binary Rust CLI/TUI (diunduh/dibangun oleh `install.sh`, di-ignore git) |
| `polban-absen-gui` | Binary Rust Desktop GUI **Polban Presensi** (diunduh/dibangun oleh `install.sh`, di-ignore git) |
| `install.sh` | Unduh binary release CLI + GUI atau build via `cargo build --release`, plus opsi `.desktop` |
| `run.sh` | Launcher CLI/TUI (meneruskan `POLBAN_ENV` dan argumen CLI) |
| `run-gui.sh` | Launcher Desktop GUI (`polban-absen-gui`) |
| `.env.example` | Template config |

## Aplikasi Desktop (Linux)

- **Fedora (RPM, disarankan):** unduh `Polban-Presensi-Fedora-x64.rpm` dari [GitHub Releases](https://github.com/raditted/Polban-Auto-Absen-rs/releases/latest) lalu pasang lewat DNF:
  ```bash
  sudo dnf install ./Polban-Presensi-Fedora-x64.rpm
  ```
  Paket RPM memasang binary ke `/usr/bin/polban-absen-gui`, ikon, serta shortcut aplikasi **Polban Presensi** otomatis. Pembaruan berikutnya dapat dilakukan langsung dari tombol **Update** di dalam aplikasi (mengunduh `.rpm`, memverifikasi `SHA256SUMS.txt`, lalu memasang di latar belakang lewat otorisasi Polkit `pkexec` saat **Restart untuk Update**), atau manual lewat `sudo dnf upgrade ./Polban-Presensi-Fedora-x64.rpm`.
- **Pembaruan otomatis di aplikasi (Auto-Update):**
  - **Instalasi RPM Fedora:** bila aplikasi terpasang lewat RPM dan `pkexec` + `dnf`/`rpm` tersedia, pembaruan berjalan non-blocking di latar belakang lalu me-restart aplikasi otomatis setelah otorisasi Polkit berhasil (jika dibatalkan, status kembali ke **Gagal Update** untuk dicoba ulang tanpa menutup aplikasi).
  - **Instalasi portabel (`.tar.gz` / `install.sh`):** pada sistem Linux `x86_64` non-RPM di folder yang dapat ditulis pengguna, aplikasi mengunduh `Polban-Presensi-Linux-x64.tar.gz`, memverifikasi `SHA256SUMS.txt`, mengekstrak `polban-absen-gui`, lalu mengganti binary dan me-restart otomatis.
  - **Fallback browser:** jika lokasi instalasi tidak dikelola RPM dan tidak dapat ditulis (atau pada arsitektur selain `x86_64`), tombol **Update** membuka halaman rilis di browser secara non-blocking.
- Jika membangun `polban-absen-gui` dari source di Fedora, pasang paket development berikut:
  ```bash
  sudo dnf install -y gtk3-devel libayatana-appindicator-gtk3-devel \
    libxkbcommon-devel libxcb-devel mesa-libGL-devel dbus-devel
  ```
- Jika membangun `polban-absen-gui` dari source di Ubuntu/Debian, pasang paket dependensi sistem terlebih dahulu:
  ```bash
  sudo apt-get update && sudo apt-get install -y \
    libgtk-3-dev libayatana-appindicator3-dev \
    libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
    libxkbcommon-dev libgl1-mesa-dev libdbus-1-dev
  ```
- **Arch Linux / EndeavourOS:** pasang dependensi runtime/build berikut agar GUI dan ikon system tray (StatusNotifierItem / Waybar) berjalan:
  ```bash
  sudo pacman -S --needed gtk3 libayatana-appindicator libxkbcommon libxcb mesa dbus
  ```
- **Wayland, XWayland, X11 & System Tray:**
  - **Pemilihan backend otomatis:** Secara default, `polban-absen-gui` memilih X11/XWayland secara otomatis selama `DISPLAY` tersedia dan koneksi X11 berhasil, lalu fallback ke Wayland native apabila `DISPLAY` tidak ada (sesi Wayland murni tanpa XWayland). Di **X11**, **Hyprland**, **Niri**, **Sway**, **KDE Wayland**, dan **GNOME Wayland**, jalankan `polban-absen-gui` atau `bash run-gui.sh` secara langsung tanpa perlu menyetel `WINIT_UNIX_BACKEND=x11`.
  - **Identitas jendela:** jendela memakai `app_id` dan `WM_CLASS` `"polban-presensi"` di Wayland maupun X11/XWayland agar cocok dengan shortcut `polban-presensi.desktop`.
  - **Deteksi layanan system tray:** mode sembunyikan ke tray (*close-to-tray* dan `--minimized`) aktif hanya ketika aplikasi berjalan di X11/XWayland **dan** host system tray terdeteksi (`org.kde.StatusNotifierWatcher` di D-Bus atau `_NET_SYSTEM_TRAY` di X11).
  - **GNOME Wayland:** GNOME tidak menyertakan system tray bawaan. Pasang dan aktifkan ekstensi AppIndicator (misalnya *AppIndicator and KStatusNotifierItem Support*) beserta `libayatana-appindicator` jika ingin memakai *close-to-tray*. Tanpa host tray aktif, menutup jendela (`X`) langsung menghentikan aplikasi agar tidak tersembunyi tanpa ikon, dan `--minimized` fallback ke memperkecil jendela biasa.
  - **Hyprland / Niri / Sway:** pastikan panel atau daemon StatusNotifier (seperti Waybar, Noctalia, atau panel tray sejenis) berjalan agar ikon tray dan *close-to-tray* aktif; jika tidak ada host tray aktif, menutup jendela langsung menghentikan aplikasi.
  - **Wayland murni / override manual:** jika `DISPLAY` kosong atau Anda memaksa Wayland native dengan `WINIT_UNIX_BACKEND=wayland polban-absen-gui`, `winit` tidak mendukung `set_visible(false)`, sehingga menutup jendela keluar normal dan `--minimized` hanya memperkecil jendela. Anda tetap dapat memberi override opsional:
    ```bash
    WINIT_UNIX_BACKEND=wayland polban-absen-gui   # paksa Wayland native
    WINIT_UNIX_BACKEND=x11 polban-absen-gui       # utamakan X11/XWayland (fallback aman ke Wayland jika gagal)
    ```
  - **Skala tampilan (DPI) di X11 / XWayland:** saat berjalan pada backend X11/XWayland tanpa `WINIT_X11_SCALE_FACTOR` dan tanpa nilai `Xft.dpi` / `Xft/DPI` (misalnya default di Hyprland, Niri, Sway, atau i3), aplikasi otomatis memakai skala `1.0` (`WINIT_X11_SCALE_FACTOR=1`) agar ukuran UI seragam dengan aplikasi desktop lain dan tidak membesar berlebih akibat ukuran fisik monitor XRandR. Desktop yang menyetel `Xft.dpi` / `Xft/DPI` (GNOME, KDE Plasma, XFCE, atau pengguna `xrdb`) dan Wayland native tetap mengikuti skala masing-masing. Untuk skala manual (misalnya saat memakai `xwayland:force_zero_scaling = true` di Hyprland), setel:
    ```bash
    WINIT_X11_SCALE_FACTOR=1.5 polban-absen-gui
    ```
  - **Kontrol jendela pada tiling WM / compositor tanpa titlebar:** di **Hyprland**, **Niri**, **Sway**, **i3**, **River**, **bspwm**, **dwm**, **awesome**, **qtile**, **xmonad**, **herbstluftwm**, **leftwm**, dan WM sejenis, `polban-absen-gui` otomatis menampilkan bar kontrol jendela tipis di sisi atas (`Perkecil`, `Perbesar/Kembalikan Ukuran`, `Tutup`) beserta area geser/klik ganda, sedangkan desktop dengan titlebar bawaan (GNOME, KDE/Plasma, XFCE, Cinnamon, MATE, LXQt, Budgie, COSMIC, labwc, wayfire) tetap memakai titlebar native tanpa duplikasi. Gunakan `POLBAN_WINDOW_CONTROLS=1`, `0`, atau `auto` untuk menimpa deteksi otomatis.
  - Gunakan `Ctrl+Q` kapan saja untuk keluar penuh dari aplikasi (termasuk saat *close-to-tray* aktif).
- Kredensial di GUI disimpan lewat Secret Service OS (GNOME Keyring/KWallet) jika tersedia, atau memori sesi sebagai fallback.
- Jalankan `bash install.sh --desktop` untuk menambahkan **Polban Presensi** ke menu aplikasi (`~/.local/share/applications/polban-presensi.desktop`).

---

## Mode Plain / Non-TTY

Jika terminal bukan TTY (misalnya dialihkan ke file log / cron) atau memakai `--plain`:
- Otomatis berjalan dalam mode plain text tanpa overhead TUI.

## Keamanan

- Kredensial disimpan **lokal** di `.env` dan password di-redact di output debug.
- File `.clicked.json`, `polban-absen`, dan `*-dump.html` sudah ada di `.gitignore`.

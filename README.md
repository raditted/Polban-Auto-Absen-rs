# Polban Auto Absen (v3.0 — Rust)

Auto-login + auto-klik tombol absen **biru** di `akademik.polban.ac.id/ajar/absen`, dengan log sukses/gagal yang jelas dan live terminal dashboard (`ratatui`). Berjalan **lokal dengan IP asli** sebagai satu binary mandiri tanpa runtime eksternal.

---

**Rust v3.0 oleh [raditted](https://github.com/raditted)** — berdasarkan proyek asli oleh **Haikal**. Lihat [LICENSE](LICENSE).

## Struktur Repository

```
polban-auto-absen/
├── Cargo.toml                  # Workspace Rust v3.0
├── rust-toolchain.toml         # Channel stable + fmt/clippy
├── crates/
│   ├── absen-core/             # Config, state (.clicked.json), parser HTML, HTTP client & engine
│   ├── absen-cli/              # Binary `polban-absen` + live TUI dashboard (ratatui) + mode plain
│   └── absen-gui/              # Aplikasi Desktop native `polban-absen-gui` (eframe/egui)
│
├── windows/                    # Launcher Windows (.bat -> polban-absen.exe & polban-absen-gui.exe)
│   ├── install.bat             # Build via Cargo / unduh release binary CLI + GUI + buat .env
│   ├── polban_presensi_gui.bat # Launcher Desktop GUI tanpa jendela cmd hitam
│   ├── persiapan.bat           # Cek binary & validasi .env (dipanggil launcher CLI)
│   ├── auto_absen.bat          # Jalankan mode loop CLI (live dashboard)
│   ├── absen_sekali.bat        # Tes sekali jalan CLI (--once)
│   └── .env.example  README.md
│
├── linux/                      # Launcher Linux
│   ├── install.sh              # Unduh release CLI + GUI / build Cargo + opsi shortcut .desktop
│   ├── run.sh                  # Launcher utama CLI/TUI
│   ├── run-gui.sh              # Launcher Desktop GUI (`polban-absen-gui`)
│   └── .env.example  README.md  preview.png
│
├── termux/                     # Launcher Android Termux (24/7)
│   ├── termux-install.sh       # Unduh binary Android / pkg install rust + autostart Termux:Boot
│   ├── termux-run.sh           # Launcher loop + wake-lock + auto-restart
│   └── .env.example  README.md
│
├── extension/                  # Chrome MV3 (JS, berjalan di browser)
├── legacy/python/              # Arsip implementasi v2.0 (tidak lagi dipakai launcher)
└── .github/workflows/          # CI (Linux/Windows/macOS) & Release multi-target
```

## Fitur

- **Satu binary mandiri (`polban-absen`)** — dikompilasi dengan Rust (`reqwest` + `rustls`), tanpa OpenSSL dan tanpa dependensi runtime eksternal.
- **Auto-login & re-login otomatis** — mengisi NIM + password ke `/laman/login`, lalu memantau `/ajar/absen`. Jika sesi habis di tengah jalan, otomatis login ulang.
- **Bypass popup "Kuesioner Layanan"** — karena bekerja di level HTTP, dialog JavaScript browser tidak menghalangi absen.
- **Klik HANYA tombol BIRU** — tombol `btn-primary` / `btn-info` (yang tidak `disabled`) diklik lewat `POST /ajar/absen/absensi_awal`.
- **Retry sampai terverifikasi** — memeriksa response POST, dan jika ambigu melakukan 1× GET konfirmasi (maks `POLBAN_MAX_RETRY`, default 5).
- **Anti-duplikat harian** — menyimpan signature di `.clicked.json` dan reset otomatis saat ganti tanggal lokal.
- **Live TUI Dashboard responsif (`ratatui`)** — panel **Identitas**, tabel **Jadwal Hari Ini** 4 tingkat lebar (`>=100` / `>=56` / `>=44` / `<44` kolom), countdown scan berikutnya, log ring buffer 15 entri, scroll (`↑`/`↓`, `PgUp`/`PgDn`, `Home`/`End`, `j`/`k`/`g`/`G`/spasi/`b`), dan terminal restore guard.
- **Aplikasi Desktop Polban Presensi (`polban-absen-gui`)**: antarmuka grafis native untuk Windows, macOS, dan Linux dengan onboarding inline, penyimpanan kata sandi aman lewat Keychain/Credential Manager OS (`keyring`), system tray dengan 0% CPU saat tersembunyi, notifikasi desktop native, autostart saat login, impor `.env`, tema Terang/Gelap/Sistem, dan ekspor HTML diagnostik.
- **Fallback plain otomatis** — otomatis beralih ke output plain text saat `--plain` dipakai atau stdout bukan TTY.

## Cara Pakai

### 1. Pakai Binary Release (tanpa install Rust)

Unduh binary dari halaman **GitHub Releases** sesuai platform (atau jalankan `install.bat` / `install.sh` / `termux-install.sh` di folder platform masing-masing).

### 2. Build Sendiri dari Source

Butuh [Rust toolchain](https://rustup.rs/) (`stable`):

```bash
# Binary CLI / TUI
cargo build --release --bin polban-absen

# Binary Desktop GUI
cargo build --release --bin polban-absen-gui
```

Binary hasil build ada di `target/release/polban-absen` dan `target/release/polban-absen-gui` (atau `.exe` di Windows).

### Aplikasi Desktop Polban Presensi (`polban-absen-gui`)

Aplikasi desktop tersedia untuk tiga platform:
- **Windows x64**: unduh `polban-absen-gui-x86_64-pc-windows-msvc.exe` (atau jalankan `windows\install.bat` lalu klik 2x `windows\polban_presensi_gui.bat`).
- **macOS (Apple Silicon & Intel)**: unduh `Polban-Presensi-macos-aarch64.zip` atau `Polban-Presensi-macos-x86_64.zip`, ekstrak `Polban Presensi.app` ke `/Applications`.
- **Linux x64**: unduh `polban-absen-gui-x86_64-unknown-linux-gnu.tar.gz` (atau jalankan `bash linux/install.sh --desktop` lalu `bash linux/run-gui.sh`). Pada build dari source di Linux, pastikan paket dev GTK3/AppIndicator/X11/OpenGL terpasang (`libgtk-3-dev libayatana-appindicator3-dev libxdo-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libgl1-mesa-dev libdbus-1-dev`).

Opsi baris perintah `polban-absen-gui`:
- `--minimized`: langsung mulai tersembunyi di system tray (dipakai otomatis oleh fitur autostart).
- `--help`, `--version`: tampilkan bantuan atau versi aplikasi.

Shortcut keyboard di aplikasi desktop: `Ctrl+,` (Pengaturan), `Escape` (tutup Pengaturan), `Ctrl+R` (Mulai/Hentikan pemantauan), `F5` (Cek Sekarang), `Ctrl+Q` (Keluar penuh).

### Argumen CLI

```bash
polban-absen [OPSI]
```

| Argumen | Keterangan |
|---|---|
| `--engine http` | Engine HTTP ringan (satu-satunya engine yang didukung di v3.0) |
| `--once` | Sekali jalan lalu exit (tanpa alternate screen agar dashboard tetap terbaca) |
| `--plain` | Mode plain text tanpa live dashboard |
| `--no-alt-screen` | Gambar dashboard di layar biasa (tanpa alternate screen) |
| `--ascii` | Pakai border ASCII (`+ - |`) untuk terminal lama |
| `--dump [FILE]` | Login lalu simpan HTML `/ajar/absen` ke `FILE` (default `absen-dump.html`) untuk diagnostik |

> **Catatan migrasi dari v2.0:** Engine `--engine pw` (Playwright) sudah dihapus di versi Rust v3.0. Jika `--engine pw` atau `POLBAN_ENGINE=pw` diberikan, program mencetak pesan penolakan dan keluar dengan kode `1`.

## 🖥️ Extension (Chrome)

1. `chrome://extensions` → **Developer mode** ON → **Load unpacked** → pilih folder `extension/`.
2. Klik ikon → **⚙️ Pengaturan** → isi NIM & password → **Simpan**.
3. Klik ikon → **▶ Mulai Absen**.

## 🖥️ Windows `.bat`

Lihat [**`windows/README.md`**](windows/README.md).

```bat
cd windows
install.bat
:: Desktop GUI:
polban_presensi_gui.bat
:: Atau CLI/TUI: edit .env lalu jalankan:
absen_sekali.bat
auto_absen.bat
```

## 🐧 Linux

Lihat [**`linux/README.md`**](linux/README.md).

```bash
cd linux
bash install.sh
# Desktop GUI:
bash run-gui.sh
# Atau CLI/TUI:
nano .env
./polban-absen --once
bash run.sh
```

## 📱 Termux (Android)

Lihat [**`termux/README.md`**](termux/README.md).

```bash
cd termux
bash termux-install.sh
nano .env
./polban-absen --once
bash termux-run.sh
```

## Cara Membaca Log & Status

Format baris log: `HH:MM:SS  [TAG] [KODE] pesan`. Tag: `[OK]` sukses · `[!!]` gagal · `[??]` peringatan · `[--]` info · `[>>]` skip.

| Status Tabel | Warna | Arti |
|---|---|---|
| `SUDAH ABSEN` | Hijau | Absensi sudah tercatat |
| `SIAP ABSEN` | Biru | Bisa diabsen sekarang — akan diklik otomatis |
| `BELUM BUKA` | Oranye/Kuning | Dosen belum membuka sesi absen |
| `BELUM WAKTUNYA` | Merah | Di luar jam absensi |

### Kode Exit

- `0`: selesai normal / dihentikan user (`Ctrl+C` atau `q`).
- `1`: konfigurasi `.env` kosong/tidak valid, argumen salah (termasuk `--engine pw`), atau error runtime.
- `2`: login gagal (NIM/password salah).

## Keamanan

- Kredensial disimpan **lokal** (Keychain/Credential Manager/Secret Service OS untuk Desktop GUI; `chrome.storage` untuk extension; `.env` untuk CLI). Tidak pernah dikirim selain ke host target yang dikonfigurasi.
- `prefs.json` pada Desktop GUI tidak pernah menyimpan kata sandi; hanya NIM dan preferensi tampilan/jadwal yang disimpan di direktori konfigurasi OS.
- `.env`, `.clicked.json`, dan `*-dump.html` sudah ada di `.gitignore`.
- Password pada struct konfigurasi di-redact (`[REDACTED]`) pada output `Debug` dan tidak pernah ditulis ke log.

# Polban Presensi

Aplikasi desktop dan terminal untuk login otomatis dan menekan tombol presensi biru di `akademik.polban.ac.id/ajar/absen`. Program berjalan lokal di perangkat Anda tanpa browser eksternal dan tanpa mengirim kredensial ke pihak ketiga.

Oleh [raditted](https://github.com/raditted), berdasarkan proyek asli oleh Haikal. Lihat [LICENSE](LICENSE).

## Cara Mencari dan Mengunduh

1. Buka halaman rilis terbaru di [GitHub Releases](https://github.com/raditted/Polban-Auto-Absen-rs/releases/latest), atau klik **Releases** di sisi kanan halaman utama repository ini.
2. Gulir ke bagian **Assets** di bagian bawah catatan rilis.
3. Unduh file sesuai sistem operasi Anda:
   - Windows (disarankan): `Polban-Presensi-Setup-Windows-x64.exe` (Installer)
   - Windows (portable): `Polban-Presensi-Portable-Windows-x64.zip` (ekstrak, lalu klik 2x `Polban Presensi.exe`)
   - macOS: `Polban-Presensi-macOS-AppleSilicon.zip` (Mac M1/M2/M3/M4) atau `Polban-Presensi-macOS-Intel.zip` (Mac Intel)
   - Linux (Fedora / RPM): `Polban-Presensi-Fedora-x64.rpm`
   - Linux (umum / tarball): `Polban-Presensi-Linux-x64.tar.gz`

## Cara Menginstall dan Menjalankan (Desktop GUI)

### 1. Pasang aplikasi

- **Windows (Installer):** klik 2x `Polban-Presensi-Setup-Windows-x64.exe` lalu ikuti langkah instalasi hingga selesai. Jika muncul jendela biru Windows SmartScreen (*Windows protected your PC*), klik **More info**, lalu klik **Run anyway**.
- **Windows (Portable):** ekstrak `Polban-Presensi-Portable-Windows-x64.zip` ke folder pilihan Anda, lalu klik 2x `Polban Presensi.exe`.
- **macOS:** ekstrak file `.zip` sesuai arsitektur Mac Anda, lalu pindahkan `Polban Presensi.app` ke folder `Applications`.
- **Linux (Fedora / RPM):** jalankan `sudo dnf install ./Polban-Presensi-Fedora-x64.rpm`, lalu buka **Polban Presensi** dari launcher aplikasi atau jalankan `polban-absen-gui`.
- **Linux (tarball):** ekstrak `Polban-Presensi-Linux-x64.tar.gz`, lalu jalankan binary `polban-absen-gui`.

### 2. Isi akun akademik

1. Buka menu **Pengaturan** (atau ikuti kartu masuk pada layar awal).
2. Masukkan NIM dan kata sandi portal akademik Polban Anda, lalu simpan.
3. Kredensial disimpan lokal di penyimpanan aman sistem operasi (Windows Credential Manager, macOS Keychain, atau Linux Secret Service) dan tidak pernah disimpan di dalam file `.exe`.

### 3. Jalankan dan kelola aplikasi

- Klik **Jalankan Presensi Otomatis** di bagian atas untuk mulai memantau jadwal dan menekan tombol presensi biru secara otomatis.
- Jika Anda menekan tombol tutup (`X`), jendela disembunyikan ke system tray dan notifikasi desktop akan memberi tahu bahwa aplikasi masih berjalan di latar belakang. Klik ikon di system tray untuk membuka jendela kembali, atau klik kanan lalu pilih **Keluar** untuk menutup aplikasi sepenuhnya. Di Linux, aplikasi otomatis memakai X11/XWayland bila tersedia sehingga system tray aktif di sebagian besar desktop; pada sesi Wayland murni atau desktop tanpa layanan system tray (misalnya GNOME tanpa ekstensi AppIndicator), tombol tutup langsung menutup aplikasi (lihat [linux/README.md](linux/README.md)).
- Saat versi baru tersedia, tombol **Update** akan muncul di bilah atas tanpa mengganggu pemakaian. Klik tombol tersebut untuk mengunduh pembaruan di latar belakang, lalu klik **Restart untuk Update** ketika verifikasi selesai.

Shortcut keyboard di aplikasi desktop: `Ctrl+,` (Pengaturan), `Escape` (tutup Pengaturan), `Ctrl+R` (Mulai atau Hentikan pemantauan), `F5` (Cek Sekarang), `Ctrl+Q` (Keluar penuh).

---

## Mode Terminal (CLI / TUI) dan Platform Lain

Bagian di bawah ditujukan untuk pengguna yang ingin menjalankan versi terminal (`polban-absen`), Termux di Android, ekstensi Chrome, atau membangun sendiri dari kode sumber.

### Windows (.bat)

Lihat panduan lengkap di [windows/README.md](windows/README.md).

```bat
cd windows
install.bat
:: Desktop GUI:
polban_presensi_gui.bat
:: Atau CLI/TUI: edit .env lalu jalankan:
absen_sekali.bat
auto_absen.bat
```

### Linux

Lihat panduan lengkap di [linux/README.md](linux/README.md).

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

### Termux (Android)

Lihat panduan lengkap di [termux/README.md](termux/README.md).

```bash
cd termux
bash termux-install.sh
nano .env
./polban-absen --once
bash termux-run.sh
```

### Ekstensi Chrome

1. Buka `chrome://extensions`, aktifkan **Developer mode**, klik **Load unpacked**, lalu pilih folder `extension/`.
2. Klik ikon ekstensi, buka **Pengaturan**, isi NIM dan kata sandi, lalu klik **Simpan**.
3. Klik **Mulai Absen**.

### Argumen CLI (`polban-absen`)

```bash
polban-absen [OPSI]
```

| Argumen | Keterangan |
|---|---|
| `--engine http` | Engine HTTP ringan (satu-satunya engine yang didukung di v3.0) |
| `--once` | Jalan satu siklus lalu keluar |
| `--plain` | Mode teks polos tanpa dashboard interaktif |
| `--no-alt-screen` | Gambar dashboard di layar terminal biasa (tanpa alternate screen) |
| `--ascii` | Pakai border ASCII (`+ - \|`) untuk terminal lama |
| `--dump [FILE]` | Login lalu simpan HTML `/ajar/absen` ke `FILE` (default `absen-dump.html`) untuk diagnostik |

Catatan migrasi dari v2.0: engine `--engine pw` (Playwright) sudah dihapus. Jika `--engine pw` atau `POLBAN_ENGINE=pw` diberikan, program mencetak pesan penolakan dan keluar dengan kode `1`.

### Build dari Source

Membutuhkan [Rust toolchain](https://rustup.rs/) (`stable`):

```bash
# Binary CLI / TUI
cargo build --release --bin polban-absen

# Binary Desktop GUI
cargo build --release --bin polban-absen-gui
```

Hasil build tersimpan di `target/release/polban-absen` dan `target/release/polban-absen-gui` (dengan ekstensi `.exe` di Windows). Untuk build GUI dari source di Linux, pastikan paket development GTK3, AppIndicator, X11, dan OpenGL sudah terpasang (`libgtk-3-dev libayatana-appindicator3-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libgl1-mesa-dev libdbus-1-dev`).

## Cara Kerja dan Status Jadwal

Program melakukan login ke `/laman/login`, memantau halaman `/ajar/absen`, dan hanya menekan tombol biru (`btn-primary` atau `btn-info` yang aktif) lewat `POST /ajar/absen/absensi_awal`. Karena bekerja langsung di level HTTP, popup JavaScript seperti kuesioner layanan tidak menghalangi proses presensi. Jika sesi habis di tengah jalan, program login ulang otomatis dan menyimpan riwayat harian di `.clicked.json` agar tidak mengirim klik ganda.

Format baris log: `HH:MM:SS  [TAG] [KODE] pesan`. Tag: `[OK]` sukses, `[!!]` gagal, `[??]` peringatan, `[--]` info, `[>>]` dilewati.

| Status Tabel | Warna | Arti |
|---|---|---|
| `SUDAH ABSEN` | Hijau | Absensi sudah tercatat |
| `SIAP ABSEN` | Biru | Bisa diabsen sekarang dan akan diklik otomatis |
| `BELUM BUKA` | Oranye/Kuning | Dosen belum membuka sesi absen |
| `BELUM WAKTUNYA` | Merah | Di luar jam absensi |

Kode exit CLI: `0` selesai normal atau dihentikan pengguna (`Ctrl+C` atau `q`), `1` konfigurasi `.env` kosong/tidak valid atau error runtime, `2` login gagal karena NIM atau kata sandi salah.

## Struktur Repository

```text
polban-auto-absen/
├── Cargo.toml                  # Workspace Rust
├── rust-toolchain.toml         # Channel stable + fmt/clippy
├── assets/                     # Ikon aplikasi (.ico dan .png)
├── crates/
│   ├── absen-core/             # Config, state (.clicked.json), parser HTML, HTTP client & engine
│   ├── absen-cli/              # Binary polban-absen + live TUI dashboard (ratatui) + mode plain
│   └── absen-gui/              # Aplikasi Desktop native polban-absen-gui (eframe/egui)
├── windows/                    # Installer Inno Setup (.iss), launcher .bat, dan README Windows
├── linux/                      # Installer & launcher Linux
├── termux/                     # Installer & launcher Android Termux
├── extension/                  # Ekstensi Chrome MV3
├── legacy/python/              # Arsip implementasi v2.0 (tidak lagi dipakai launcher)
└── .github/workflows/          # CI (Linux/Windows/macOS) & GitHub Release multi-platform
```

## Keamanan

- Kredensial disimpan lokal (Credential Manager, Keychain, atau Secret Service untuk Desktop GUI; `chrome.storage` untuk ekstensi; `.env` untuk CLI) dan hanya dikirim ke host target yang dikonfigurasi.
- File `prefs.json` pada Desktop GUI tidak menyimpan kata sandi, hanya NIM dan preferensi tampilan/jadwal.
- File `.env`, `.clicked.json`, dan `*-dump.html` sudah dikecualikan di `.gitignore`.
- Kata sandi pada struct konfigurasi disensor (`[REDACTED]`) pada output `Debug` dan tidak pernah ditulis ke log.

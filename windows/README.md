# Polban Auto Absen untuk Windows

Panduan ini mencakup aplikasi desktop **Polban Presensi** (`Polban-Presensi-Setup-Windows-x64.exe` atau `Polban-Presensi-Portable-Windows-x64.zip`) serta mode terminal (`polban-absen.exe`) dengan launcher `.bat`.

## Aplikasi Desktop (Polban Presensi GUI)

### Instalasi dari GitHub Releases (disarankan)

1. Unduh `Polban-Presensi-Setup-Windows-x64.exe` dari halaman [GitHub Releases](https://github.com/raditted/Polban-Auto-Absen-rs/releases/latest), lalu jalankan installer (atau unduh `Polban-Presensi-Portable-Windows-x64.zip`, ekstrak, lalu klik 2x `Polban Presensi.exe`).
2. Jika Windows SmartScreen muncul, klik **More info**, lalu klik **Run anyway**.
3. Buka **Pengaturan**, isi NIM dan kata sandi portal akademik (disimpan aman di Windows Credential Manager), atau klik **Impor dari .env**.
4. Klik **Jalankan Presensi Otomatis** untuk memulai pemantauan.
5. Menekan tombol tutup (`X`) akan menyembunyikan jendela ke system tray disertai notifikasi desktop. Klik ikon tray untuk membuka kembali, atau klik kanan ikon tray lalu pilih **Keluar** untuk menutup aplikasi sepenuhnya.

### Menyiapkan dari folder repository (`install.bat`)

1. Klik 2x `install.bat` sekali untuk membangun via Cargo (jika Rust terpasang) atau mengunduh binary release ke folder `windows/`.
2. Klik 2x `polban_presensi_gui.bat` (atau langsung klik 2x `polban-absen-gui.exe`).

## Mode Terminal (CLI / TUI)

Auto-absen juga dapat berjalan di terminal Windows sebagai satu binary native (`polban-absen.exe`) dengan live TUI dashboard (`ratatui`) yang menyesuaikan ukuran jendela. Tekan `↑`/`↓`, `PgUp`/`PgDn`, `Home`/`End`, `j`/`k`/`g`/`G`, spasi/`b`, atau roda mouse untuk menggulir, dan `Ctrl+C` atau `q` untuk keluar.

### File di folder `windows/`

| File | Tugas |
| --- | --- |
| `polban-absen.exe` | Binary Rust CLI/TUI (disiapkan oleh `install.bat`, di-ignore git) |
| `polban-absen-gui.exe` | Binary Desktop GUI (disiapkan oleh `install.bat`, di-ignore git) |
| `polban_presensi_gui.bat` | Launcher Desktop GUI tanpa meninggalkan jendela cmd |
| `persiapan.bat` | Dipanggil launcher CLI untuk mengecek `polban-absen.exe` dan validasi `.env` |
| `install.bat` | Menyiapkan binary CLI + GUI dan membuat `.env` |
| `auto_absen.bat` | Menjalankan mode loop CLI (live dashboard) |
| `absen_sekali.bat` | Tes sekali jalan CLI lalu berhenti |
| `.env.example` | Template konfigurasi |

### Cara pakai mode terminal

1. Klik 2x `install.bat` untuk menyiapkan binary dan file `.env`.
2. Edit `.env` (dibuat dari `.env.example`) dan isi kredensial Anda:
   ```env
   POLBAN_USERNAME=25.....
   POLBAN_PASSWORD=password_kamu
   ```
3. Klik 2x `absen_sekali.bat` (`--engine http --once`) untuk tes satu kali.
4. Klik 2x `auto_absen.bat` untuk berjalan terus-menerus. Tekan `Ctrl+C` atau `q` untuk berhenti.
5. Untuk membuat shortcut desktop, klik kanan `auto_absen.bat`, pilih **Send to**, lalu pilih **Desktop**.

## Argumen CLI

Dapat ditambahkan di belakang nama `.bat` atau dipanggil langsung ke `polban-absen.exe`:

| Argumen | Keterangan |
| --- | --- |
| `--engine http` | Engine HTTP ringan (satu-satunya engine di v3.0) |
| `--once` | Sekali jalan lalu exit (tanpa alternate screen) |
| `--plain` | Paksa plain text (log mengalir biasa, tanpa dashboard) |
| `--no-alt-screen` | Gambar dashboard di layar biasa (tanpa alternate screen) |
| `--ascii` | Gunakan box ASCII (`+ - \|`) jika border Unicode pecah di conhost lama |
| `--dump [FILE]` | Simpan HTML halaman absen lalu exit (diagnostik) |

## Troubleshooting

| Masalah | Solusi |
| --- | --- |
| `install.bat` gagal unduh atau build | Cek koneksi internet, atau pasang Rust lalu jalankan `cargo build --release --bin polban-absen` di root repo |
| `Login GAGAL` (exit 2) | Periksa kembali NIM dan kata sandi di `.env` atau Pengaturan |
| Border pecah di `cmd.exe` lama | Jalankan `auto_absen.bat --ascii` atau `--no-alt-screen`, atau gunakan Windows Terminal |
| `Tidak ada tombol biru` | Di luar jam absen atau dosen belum membuka sesi; aplikasi otomatis mengecek ulang di siklus berikutnya |

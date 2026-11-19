# Polban Auto Absen — Windows (.bat double-click)

Auto-absen jalan di **terminal Windows** sebagai satu binary native (`polban-absen.exe`) tanpa browser dan tanpa runtime eksternal, dengan **live TUI dashboard** (`ratatui`): identitas mahasiswa, tabel jadwal hari ini, countdown scan berikutnya, dan log aktivitas ringkas.

## Tampilan

Dashboard digambar di **alternate screen buffer** (seperti `htop`), jadi layar bersih dan tidak meninggalkan scrollback. Panel diperbarui *in-place* dan **otomatis menyesuaikan ukuran jendela**: di jendela sempit kolom Jam lalu Dosen melipat ke bawah nama matkul. Jika isi lebih tinggi dari jendela, scroll dengan `↑`/`↓`, `PgUp`/`PgDn`, `Home`/`End`, `j`/`k`/`g`/`G`/spasi/`b`, atau roda mouse. Tekan `Ctrl+C` atau `q` untuk keluar.

## File

| File | Tugas |
| --- | --- |
| `polban-absen.exe` | Binary Rust CLI/TUI (diunduh/dibangun oleh `install.bat`, di-ignore git) |
| `polban-absen-gui.exe` | Binary Desktop GUI **Polban Presensi** (diunduh/dibangun oleh `install.bat`, di-ignore git) |
| `polban_presensi_gui.bat` | Launcher Desktop GUI tanpa meninggalkan jendela cmd hitam |
| `persiapan.bat` | Dicek otomatis oleh launcher CLI: pastikan `polban-absen.exe` tersedia + `.env` valid |
| `install.bat` | Jalankan SEKALI: build/unduh `polban-absen.exe` dan `polban-absen-gui.exe` + buat `.env` |
| `auto_absen.bat` | Jalankan mode loop CLI (live dashboard) |
| `absen_sekali.bat` | Tes SEKALI jalan CLI lalu berhenti (dashboard tetap terlihat di layar) |
| `.env.example` | Template config |

## Aplikasi Desktop (Polban Presensi GUI)

1. Jalankan `install.bat` sekali untuk menyiapkan `polban-absen-gui.exe` (build lokal via Cargo atau unduh paket portable `Polban-Presensi-Portable-Windows-x64.zip`). Alternatifnya, unduh `Polban-Presensi-Setup-Windows-x64.exe` dari halaman GitHub Releases untuk installer Windows dengan shortcut Start Menu & Desktop.
2. Klik 2x `polban_presensi_gui.bat` (atau langsung klik 2x `polban-absen-gui.exe`).
3. Masukkan NIM dan kata sandi langsung di layar onboarding (disimpan aman di Windows Credential Manager) atau klik **Impor dari .env**.
4. Jendela dapat disembunyikan ke system tray, mendukung notifikasi Windows, autostart saat login (`--minimized`), serta ekspor HTML diagnostik dari menu Pengaturan.

## Cara Pakai (dari awal)

1. **Siapkan binary sekali** — klik 2x `install.bat` (membangun via `cargo build --release` jika Rust terpasang, atau mengunduh release `polban-absen-x86_64-pc-windows-msvc.exe`).
2. **Isi kredensial** — buka `.env` (dibuat dari `.env.example`), isi:
   ```env
   POLBAN_USERNAME=25.....
   POLBAN_PASSWORD=password_kamu
   ```
3. **Tes sekali** — klik 2x `absen_sekali.bat` (`--engine http --once`). Hasil tetap tampil di layar setelah selesai.
4. **Jalankan terus-menerus** — klik 2x `auto_absen.bat`. Berhenti: `Ctrl+C` atau `q`.
5. **Desktop shortcut** — klik kanan `auto_absen.bat` → Send to → Desktop.

## Argumen CLI

Dapat ditambahkan di belakang nama `.bat` (diteruskan lewat `%*`), atau dipanggil langsung ke `polban-absen.exe`:

| Argumen | Keterangan |
| --- | --- |
| `--engine http` | Engine HTTP ringan (satu-satunya engine di Rust v3.0) |
| `--once` | Sekali jalan lalu exit (tanpa alternate screen) |
| `--plain` | Paksa plain text (log mengalir biasa, tanpa dashboard) |
| `--no-alt-screen` | Gambar dashboard di layar biasa (tanpa alternate screen) |
| `--ascii` | Gunakan box ASCII (`+ - |`) jika border Unicode pecah di conhost lama |
| `--dump [FILE]` | Simpan HTML halaman absen lalu exit (diagnostik) |

## Troubleshooting

| Masalah | Solusi |
| --- | --- |
| `install.bat` gagal unduh / build | Cek internet, atau pasang Rust lalu jalankan `cargo build --release --bin polban-absen` di root repo |
| `Login GAGAL` (exit 2) | NIM/password di `.env` salah |
| Border/kotak pecah di `cmd.exe` lama | Jalankan `auto_absen.bat --ascii` atau `--no-alt-screen`, atau pakai Windows Terminal |
| `Tidak ada tombol biru` | Di luar jam absen / dosen belum buka — otomatis dicek ulang di siklus berikutnya |

# Polban Auto Absen — Linux (Terminal TUI)

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

# 1. Unduh release binary (static musl) atau build via Cargo + siapkan .env
bash install.sh

# 2. Edit .env — isi NIM & password
nano .env

# 3. Tes sekali (dashboard tetap terlihat setelah selesai)
bash run.sh --once

# 4. Mode loop (live dashboard)
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
| `polban-absen` | Binary Rust (diunduh/dibangun oleh `install.sh`, di-ignore git) |
| `install.sh` | Unduh binary release (`x86_64`/`aarch64` musl) atau build via `cargo build --release`, plus buat `.env` |
| `run.sh` | Launcher shortcut (meneruskan `POLBAN_ENV` dan argumen CLI) |
| `.env.example` | Template config |

---

## Mode Plain / Non-TTY

Jika terminal bukan TTY (misalnya dialihkan ke file log / cron) atau memakai `--plain`:
- Otomatis berjalan dalam mode plain text tanpa overhead TUI.

## Keamanan

- Kredensial disimpan **lokal** di `.env` dan password di-redact di output debug.
- File `.clicked.json`, `polban-absen`, dan `*-dump.html` sudah ada di `.gitignore`.

# Polban Auto Absen — Termux (Android)

Jalankan auto-absen di HP Android **tanpa browser**, hemat baterai, bisa background 24/7 — memakai binary native Rust (`polban-absen`) dengan **live TUI dashboard** yang responsif hingga layar HP sempit (`< 44` kolom).

## Prasyarat

- **Termux** — install dari **F-Droid** (bukan Play Store).
- *(Opsional)* **Termux:Boot** dari F-Droid — untuk auto-jalan saat HP nyala.
- *(Opsional)* **Termux:API** — untuk `termux-wake-lock` (cegah CPU tidur).

## Instalasi

1. **Siapkan folder kerja** di Termux (atau `git clone` repo ini):

   ```bash
   cd termux
   bash termux-install.sh
   ```

   `termux-install.sh` otomatis mengunduh binary target Android (`aarch64-linux-android` / `armv7-linux-androideabi`), atau fallback memasang `rust` via `pkg install -y rust` dan menjalankan `cargo build --release`, serta membuat `.env` dan autostart `~/.termux/boot/start-auto-absen.sh`.

2. **Isi kredensial:**

   ```bash
   nano .env
   ```

   Isi tanpa kutip (simbol `# ! @` dan spasi aman; hanya baris yang diawali `#` yang dianggap komentar):

   ```env
   POLBAN_USERNAME=NIM_kamu
   POLBAN_PASSWORD=password_kamu
   ```

3. **Tes sekali:**

   ```bash
   ./polban-absen --engine http --once
   ```

4. **Mode loop 24/7** (auto-restart bila error jaringan, berhenti otomatis jika login gagal `exit 2`):

   ```bash
   bash termux-run.sh
   ```

   Berhenti: `Ctrl+C` atau `q`. Di layar sempit, jalankan `bash termux-run.sh --no-alt-screen` atau `bash termux-run.sh --plain`.

## Argumen CLI

| Argumen | Keterangan |
| --- | --- |
| `--once` | Sekali jalan lalu exit (tanpa alternate screen) |
| `--plain` | Paksa plain text |
| `--no-alt-screen` | Dashboard tanpa alternate screen |
| `--ascii` | Border ASCII (`+ - |`) |
| `--dump [FILE]` | Simpan HTML halaman absen (diagnostik) |

## Log Background (Termux:Boot)

Jika memakai autostart Termux:Boot, output otomatis jatuh ke mode plain dan ditulis ke `absen.log`:

```bash
tail -f absen.log
```

## Troubleshooting

| Masalah | Solusi |
| --- | --- |
| Binary gagal diunduh | Pastikan koneksi stabil, atau build lokal dengan `pkg install -y rust` lalu `cargo build --release --bin polban-absen` |
| `ERROR: POLBAN_USERNAME...` | `.env` belum diisi — edit `.env` |
| `[ERROR] .env tidak ditemukan` | Letakkan `.env` di folder yang sama dengan `termux-run.sh`, atau jalankan `POLBAN_ENV=/path/.env bash termux-run.sh` |
| `Login GAGAL` (exit 2) | NIM/password salah di `.env` (loop otomatis berhenti agar tidak spam login) |
| Berhenti saat HP tidur | Install Termux:API lalu jalankan `termux-wake-lock` (otomatis dipanggil oleh `termux-run.sh`) |

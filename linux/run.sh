#!/usr/bin/env bash
# ============================================================
#  Polban Auto Absen - Launcher Linux
#  Jalankan:  bash run.sh
#  Berhenti:  tekan Ctrl+C atau q
#
#  .env dicari di folder YANG SAMA dengan script ini (atau
#  override lewat env var POLBAN_ENV=/path/ke/.env).
# ============================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR" || { echo "Tidak bisa masuk $SCRIPT_DIR"; exit 1; }

export POLBAN_ENV="${POLBAN_ENV:-$SCRIPT_DIR/.env}"

if [ -x "$SCRIPT_DIR/polban-absen" ]; then
  BIN="$SCRIPT_DIR/polban-absen"
elif [ -x "$SCRIPT_DIR/../target/release/polban-absen" ]; then
  BIN="$SCRIPT_DIR/../target/release/polban-absen"
elif command -v polban-absen >/dev/null 2>&1; then
  BIN="$(command -v polban-absen)"
else
  echo "[ERROR] binary polban-absen tidak ditemukan."
  echo "        Jalankan dulu:  bash install.sh"
  exit 1
fi

if [ ! -f "$POLBAN_ENV" ]; then
  echo "[ERROR] .env tidak ditemukan di: $POLBAN_ENV"
  echo "        Buat dulu:  cp .env.example .env   lalu  nano .env  (isi NIM & password)"
  echo "        Atau taruh di path lain dan jalankan:  POLBAN_ENV=/path/.env bash run.sh"
  exit 1
fi

exec "$BIN" --engine http "$@"

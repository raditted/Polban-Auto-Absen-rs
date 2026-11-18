#!/usr/bin/env bash
# ============================================================
#  Polban Presensi Desktop GUI - Launcher Linux
#  Jalankan:  bash run-gui.sh
# ============================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR" || { echo "Tidak bisa masuk $SCRIPT_DIR"; exit 1; }

if [ -z "${POLBAN_ENV:-}" ] && [ -f "$SCRIPT_DIR/.env" ]; then
  export POLBAN_ENV="$SCRIPT_DIR/.env"
fi

if [ -x "$SCRIPT_DIR/polban-absen-gui" ]; then
  GUI_BIN="$SCRIPT_DIR/polban-absen-gui"
elif [ -x "$SCRIPT_DIR/../target/release/polban-absen-gui" ]; then
  GUI_BIN="$SCRIPT_DIR/../target/release/polban-absen-gui"
elif command -v polban-absen-gui >/dev/null 2>&1; then
  GUI_BIN="$(command -v polban-absen-gui)"
else
  echo "[ERROR] binary polban-absen-gui tidak ditemukan."
  echo "        Jalankan dulu:  bash install.sh"
  exit 1
fi

exec "$GUI_BIN" "$@"

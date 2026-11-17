#!/data/data/com.termux/files/usr/bin/bash
# ============================================================
#  Polban Auto Absen - Launcher Termux (loop + auto-restart)
#  Jalankan:  bash termux-run.sh
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
  echo "[ERROR] binary polban-absen tidak ditemukan di $SCRIPT_DIR"
  echo "        Jalankan dulu:  bash termux-install.sh"
  exit 1
fi

if [ ! -f "$POLBAN_ENV" ]; then
  echo "[ERROR] .env tidak ditemukan di: $POLBAN_ENV"
  echo "        Buat dulu:  cp .env.example .env   lalu  nano .env  (isi NIM & password)"
  echo "        Atau taruh di path lain dan jalankan:  POLBAN_ENV=/path/.env bash termux-run.sh"
  exit 1
fi

if ! grep -qE '^POLBAN_USERNAME=.+' "$POLBAN_ENV"; then
  echo "[PERINGATAN] POLBAN_USERNAME di $POLBAN_ENV sepertinya masih kosong."
  echo "             Isi dulu POLBAN_USERNAME dan POLBAN_PASSWORD:  nano \"$POLBAN_ENV\""
  printf "             Tetap lanjut? [y/N] "
  read -r lanjut
  case "$lanjut" in
    [yY]*) ;;
    *) exit 1 ;;
  esac
fi

command -v termux-wake-lock >/dev/null 2>&1 && termux-wake-lock

trap 'exit 0' INT TERM

echo " Folder : $SCRIPT_DIR"
echo " Config : $POLBAN_ENV"
while true; do
  "$BIN" --engine http "$@"
  rc=$?
  if [ "$rc" -eq 0 ]; then
    break
  fi
  if [ "$rc" -eq 2 ]; then
    echo "[ERROR] Login gagal (exit 2) -- hentikan loop agar tidak spam login. Cek .env."
    exit 2
  fi
  echo "[$(date '+%F %T')] berhenti (exit $rc), restart 10 detik..."
  sleep 10
done

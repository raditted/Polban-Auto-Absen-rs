#!/data/data/com.termux/files/usr/bin/bash
# ============================================================
#  Polban Auto Absen - Installer Termux (Android)
#  Jalankan:  bash termux-install.sh
#  Mengunduh binary Android (*-linux-android, bukan musl) atau
#  fallback build lokal via pkg install rust.
# ============================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

BIN_OUT="$SCRIPT_DIR/polban-absen"
REPO="${POLBAN_RELEASE_REPO:-raditted/auto-presensi}"

detect_android_target() {
  case "$(uname -m)" in
    aarch64|arm64)  echo "aarch64-linux-android" ;;
    armv7l|armv8l|arm*) echo "armv7-linux-androideabi" ;;
    *)              echo "" ;;
  esac
}

download_binary() {
  local target="$1"
  local url="https://github.com/${REPO}/releases/latest/download/polban-absen-${target}"
  echo "    Mengunduh $url ..."
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$BIN_OUT"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO "$BIN_OUT" "$url"
  else
    return 1
  fi
}

echo "==> [1/3] Menyiapkan binary polban-absen (Android)"
if [ -x "$BIN_OUT" ]; then
  echo "    Binary sudah ada: $BIN_OUT"
else
  TARGET="$(detect_android_target)"
  INSTALLED=0

  if [ -n "$TARGET" ] && download_binary "$TARGET"; then
    chmod +x "$BIN_OUT"
    echo "    Binary release terpasang: $BIN_OUT"
    INSTALLED=1
  else
    rm -f "$BIN_OUT"
  fi

  if [ "$INSTALLED" -eq 0 ] && [ -f "$SCRIPT_DIR/../Cargo.toml" ]; then
    echo "    Fallback: memasang rust di Termux & build dari source..."
    if ! command -v cargo >/dev/null 2>&1; then
      pkg update -y
      pkg install -y rust nano
    fi
    cargo build --release --manifest-path "$SCRIPT_DIR/../Cargo.toml" --bin polban-absen
    cp "$SCRIPT_DIR/../target/release/polban-absen" "$BIN_OUT"
    chmod +x "$BIN_OUT"
    INSTALLED=1
  fi

  if [ "$INSTALLED" -eq 0 ]; then
    echo "[ERROR] Gagal mengunduh binary Android maupun build dari source."
    exit 1
  fi
fi

echo "==> [2/3] Siapkan config di folder script: $SCRIPT_DIR"
if [ ! -f "$SCRIPT_DIR/.env" ]; then
  if [ -f "$SCRIPT_DIR/.env.example" ]; then
    cp "$SCRIPT_DIR/.env.example" "$SCRIPT_DIR/.env"
    echo "    .env dibuat dari template -> EDIT: isi NIM & password."
  else
    cat > "$SCRIPT_DIR/.env" <<'EOF'
POLBAN_USERNAME=
POLBAN_PASSWORD=
POLBAN_TARGET_URL=https://akademik.polban.ac.id/ajar/absen
POLBAN_INTERVAL=60
POLBAN_ENGINE=http
EOF
    echo "    .env kosong dibuat -> EDIT: isi NIM & password."
  fi
else
  echo "    .env sudah ada, dilewati."
fi

echo "==> [3/3] Siapkan autostart Termux:Boot"
BOOT_DIR="$HOME/.termux/boot"
mkdir -p "$BOOT_DIR"
cat > "$BOOT_DIR/start-auto-absen.sh" <<EOF
#!/data/data/com.termux/files/usr/bin/sh
termux-wake-lock
export POLBAN_ENV="$SCRIPT_DIR/.env"
cd "$SCRIPT_DIR" && "$BIN_OUT" --engine http --plain >> "$SCRIPT_DIR/absen.log" 2>&1 &
EOF
chmod +x "$BOOT_DIR/start-auto-absen.sh"
echo "    Autostart dibuat di $BOOT_DIR/start-auto-absen.sh"

cat <<EOF

============================================================
 SELESAI. Folder kerja: $SCRIPT_DIR
 Langkah selanjutnya:
   1. Edit .env (isi NIM & password):   nano "$SCRIPT_DIR/.env"
   2. Tes sekali:   "$BIN_OUT" --engine http --once
   3. Mode loop 24/7:   bash "$SCRIPT_DIR/termux-run.sh"
   4. (Opsional) install Termux:Boot (F-Droid) agar auto-jalan saat HP nyala
============================================================
EOF

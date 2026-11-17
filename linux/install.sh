#!/usr/bin/env bash
# ============================================================
#  Polban Auto Absen - Installer Linux
#  Jalankan:  bash install.sh
#  Mengunduh binary release (musl static) atau build via Cargo.
# ============================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

BIN_OUT="$SCRIPT_DIR/polban-absen"
REPO="${POLBAN_RELEASE_REPO:-raditted/Polban-Auto-Absen-rs}"

echo "============================================================"
echo " Polban Auto Absen -- Linux Installer (Rust v3.0)"
echo "============================================================"
echo ""

detect_target() {
  case "$(uname -m)" in
    x86_64|amd64)   echo "x86_64-unknown-linux-musl" ;;
    aarch64|arm64)  echo "aarch64-unknown-linux-musl" ;;
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

echo "==> [1/2] Menyiapkan binary polban-absen"
if [ -x "$BIN_OUT" ]; then
  echo "    Binary sudah ada: $BIN_OUT"
else
  TARGET="$(detect_target)"
  INSTALLED=0

  if [ -n "$TARGET" ] && download_binary "$TARGET"; then
    chmod +x "$BIN_OUT"
    echo "    Binary release terpasang: $BIN_OUT"
    INSTALLED=1
  else
    rm -f "$BIN_OUT"
  fi

  if [ "$INSTALLED" -eq 0 ] && [ -f "$SCRIPT_DIR/../Cargo.toml" ]; then
    if command -v cargo >/dev/null 2>&1; then
      echo "    Membangun dari source via cargo build --release..."
      cargo build --release --manifest-path "$SCRIPT_DIR/../Cargo.toml" --bin polban-absen
      cp "$SCRIPT_DIR/../target/release/polban-absen" "$BIN_OUT"
      chmod +x "$BIN_OUT"
      echo "    Build selesai: $BIN_OUT"
      INSTALLED=1
    fi
  fi

  if [ "$INSTALLED" -eq 0 ]; then
    echo "[ERROR] Gagal mengunduh release maupun build dari source."
    echo "        Pasang Rust (https://rustup.rs) lalu jalankan:"
    echo "          cargo build --release --bin polban-absen"
    exit 1
  fi
fi

echo ""
echo "==> [2/2] Siapkan config"
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

echo ""
echo "============================================================"
echo " SELESAI. Folder kerja: $SCRIPT_DIR"
echo ""
echo " Langkah selanjutnya:"
echo "   1. Edit .env (isi NIM & password):"
echo "        nano $SCRIPT_DIR/.env"
echo ""
echo "   2. Tes sekali:"
echo "        $BIN_OUT --engine http --once"
echo ""
echo "   3. Mode loop (live dashboard):"
echo "        bash $SCRIPT_DIR/run.sh"
echo "============================================================"

#!/usr/bin/env bash
# ============================================================
#  Polban Auto Absen - Installer Linux
#  Jalankan:  bash install.sh [--desktop] [--cli-only]
#  Mengunduh binary release (CLI musl static + Desktop GUI)
#  atau build via Cargo, serta opsi shortcut .desktop.
# ============================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

BIN_OUT="$SCRIPT_DIR/polban-absen"
GUI_BIN_OUT="$SCRIPT_DIR/polban-absen-gui"
REPO="${POLBAN_RELEASE_REPO:-raditted/Polban-Auto-Absen-rs}"
INSTALL_DESKTOP="${INSTALL_DESKTOP:-0}"
CLI_ONLY=0

for arg in "$@"; do
  case "$arg" in
    --desktop) INSTALL_DESKTOP=1 ;;
    --cli-only) CLI_ONLY=1 ;;
    *) ;;
  esac
done

echo "============================================================"
echo " Polban Auto Absen - Linux Installer (Rust v3.0)"
echo "============================================================"
echo ""

detect_target() {
  case "$(uname -m)" in
    x86_64|amd64)   echo "x86_64-unknown-linux-musl" ;;
    aarch64|arm64)  echo "aarch64-unknown-linux-musl" ;;
    *)              echo "" ;;
  esac
}

download_file() {
  local url="$1"
  local out="$2"
  echo "    Mengunduh $url ..."
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$out"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO "$out" "$url"
  else
    return 1
  fi
}

echo "==> [1/3] Menyiapkan binary CLI polban-absen"
if [ -x "$BIN_OUT" ]; then
  echo "    Binary CLI sudah ada: $BIN_OUT"
else
  TARGET="$(detect_target)"
  INSTALLED=0

  if [ -n "$TARGET" ] && download_file "https://github.com/${REPO}/releases/latest/download/polban-absen-${TARGET}" "$BIN_OUT"; then
    chmod +x "$BIN_OUT"
    echo "    Binary release terpasang: $BIN_OUT"
    INSTALLED=1
  else
    rm -f "$BIN_OUT"
  fi

  if [ "$INSTALLED" -eq 0 ] && [ -f "$SCRIPT_DIR/../Cargo.toml" ]; then
    if command -v cargo >/dev/null 2>&1; then
      echo "    Membangun CLI dari source via cargo build --release..."
      cargo build --release --manifest-path "$SCRIPT_DIR/../Cargo.toml" --bin polban-absen
      cp "$SCRIPT_DIR/../target/release/polban-absen" "$BIN_OUT"
      chmod +x "$BIN_OUT"
      echo "    Build selesai: $BIN_OUT"
      INSTALLED=1
    fi
  fi

  if [ "$INSTALLED" -eq 0 ]; then
    echo "[ERROR] Gagal mengunduh release maupun build CLI dari source."
    echo "        Pasang Rust (https://rustup.rs) lalu jalankan:"
    echo "          cargo build --release --bin polban-absen"
    exit 1
  fi
fi

echo ""
echo "==> [2/3] Menyiapkan binary Desktop GUI polban-absen-gui"
GUI_READY=0
if [ "$CLI_ONLY" -eq 1 ]; then
  echo "    Dilewati (--cli-only)."
elif [ -x "$GUI_BIN_OUT" ]; then
  echo "    Binary Desktop GUI sudah ada: $GUI_BIN_OUT"
  GUI_READY=1
else
  ARCH="$(uname -m)"
  if [ "$ARCH" = "x86_64" ] || [ "$ARCH" = "amd64" ]; then
    TMP_DIR="$(mktemp -d)"
    ARCHIVE="$TMP_DIR/polban-absen-gui.tar.gz"
    GUI_URL="https://github.com/${REPO}/releases/latest/download/polban-absen-gui-x86_64-unknown-linux-gnu.tar.gz"
    if download_file "$GUI_URL" "$ARCHIVE" && tar -xzf "$ARCHIVE" -C "$TMP_DIR"; then
      EXTRACTED_BIN="$(find "$TMP_DIR" -type f -name "polban-absen-gui" | head -n 1 || true)"
      if [ -n "$EXTRACTED_BIN" ]; then
        cp "$EXTRACTED_BIN" "$GUI_BIN_OUT"
        chmod +x "$GUI_BIN_OUT"
        echo "    Binary GUI release terpasang: $GUI_BIN_OUT"
        GUI_READY=1
      fi
    fi
    rm -rf "$TMP_DIR"
  fi

  if [ "$GUI_READY" -eq 0 ] && [ -f "$SCRIPT_DIR/../Cargo.toml" ] && command -v cargo >/dev/null 2>&1; then
    echo "    Membangun Desktop GUI dari source via cargo build --release..."
    if cargo build --release --manifest-path "$SCRIPT_DIR/../Cargo.toml" --bin polban-absen-gui; then
      cp "$SCRIPT_DIR/../target/release/polban-absen-gui" "$GUI_BIN_OUT"
      chmod +x "$GUI_BIN_OUT"
      echo "    Build GUI selesai: $GUI_BIN_OUT"
      GUI_READY=1
    else
      echo "    [PERINGATAN] Build GUI gagal (pastikan paket dev GTK/X11/OpenGL terpasang). Mode CLI tetap siap."
    fi
  fi

  if [ "$GUI_READY" -eq 0 ]; then
    echo "    [INFO] Binary GUI belum tersedia untuk sistem ini; mode CLI tetap dapat dipakai."
  fi
fi

if [ "$GUI_READY" -eq 1 ]; then
  CREATE_DESKTOP=0
  if [ "$INSTALL_DESKTOP" = "1" ]; then
    CREATE_DESKTOP=1
  elif [ -t 0 ]; then
    printf "    Buat shortcut aplikasi (~/.local/share/applications/polban-presensi.desktop)? [y/N]: "
    read -r REPLY || true
    case "${REPLY:-n}" in
      y|Y|yes|YES) CREATE_DESKTOP=1 ;;
      *) CREATE_DESKTOP=0 ;;
    esac
  fi

  if [ "$CREATE_DESKTOP" -eq 1 ]; then
    APP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
    mkdir -p "$APP_DIR"
    DESKTOP_FILE="$APP_DIR/polban-presensi.desktop"
    ICON_OUT="polban-presensi"
    ICON_SRC="$SCRIPT_DIR/../assets/app-icon.png"
    if [ -f "$ICON_SRC" ]; then
      ICON_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/512x512/apps"
      mkdir -p "$ICON_DIR"
      cp "$ICON_SRC" "$ICON_DIR/polban-presensi.png"
      ICON_OUT="$ICON_DIR/polban-presensi.png"
    fi
    cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Type=Application
Name=Polban Presensi
Comment=Pemantau dan pencatat presensi akademik Polban
Exec=${GUI_BIN_OUT}
Icon=${ICON_OUT}
Terminal=false
Categories=Education;Utility;
StartupNotify=true
EOF
    chmod +x "$DESKTOP_FILE"
    echo "    Shortcut .desktop dibuat: $DESKTOP_FILE"
  fi
fi

echo ""
echo "==> [3/3] Siapkan config"
if [ ! -f "$SCRIPT_DIR/.env" ]; then
  if [ -f "$SCRIPT_DIR/.env.example" ]; then
    cp "$SCRIPT_DIR/.env.example" "$SCRIPT_DIR/.env"
    echo "    .env dibuat dari template -> EDIT: isi NIM & password (opsional untuk GUI)."
  else
    cat > "$SCRIPT_DIR/.env" <<'EOF'
POLBAN_USERNAME=
POLBAN_PASSWORD=
POLBAN_TARGET_URL=https://akademik.polban.ac.id/ajar/absen
POLBAN_INTERVAL=60
POLBAN_ENGINE=http
EOF
    echo "    .env kosong dibuat -> EDIT: isi NIM & password (opsional untuk GUI)."
  fi
else
  echo "    .env sudah ada, dilewati."
fi

echo ""
echo "============================================================"
echo " SELESAI. Folder kerja: $SCRIPT_DIR"
echo ""
echo " Langkah selanjutnya:"
if [ "$GUI_READY" -eq 1 ]; then
  echo "   - Desktop GUI:"
  echo "        bash $SCRIPT_DIR/run-gui.sh"
  echo "     (opsi shortcut: bash $SCRIPT_DIR/install.sh --desktop)"
  echo ""
fi
echo "   - Terminal TUI / CLI:"
echo "        nano $SCRIPT_DIR/.env"
echo "        $BIN_OUT --engine http --once"
echo "        bash $SCRIPT_DIR/run.sh"
echo "============================================================"

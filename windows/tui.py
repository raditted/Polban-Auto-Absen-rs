#!/usr/bin/env python3
# Copyright (c) 2026 Haikal. All rights reserved.
# Polban Auto Absen -- TUI module (rich-based live dashboard)
# Hanya UI layer. Tidak mengandung engine logic.
"""Modern terminal dashboard untuk Polban Auto Absen.

File ini SAMA PERSIS di windows/ dan termux/ (label platform dideteksi
otomatis saat runtime), dan merupakan port dari linux/tui.py.

Dua mode:
  1. Live dashboard (default) -- rich.live.Live, panel di-update in-place.
  2. Plain fallback           -- terminal non-interaktif, --plain, atau
                                rich tidak terpasang.

Prinsip:
  - Tidak ada print() berulang. Semua lewat Live context.
  - Tabel jadwal hanya berubah visual saat status berubah.
  - Countdown timer mengisi jeda antar-scan.
  - Ring buffer log 15 entri, tidak scroll tak terbatas.
  - Tidak ada yang terpotong: dashboard yang lebih tinggi dari jendela
    digambar sebagai viewport yang bisa di-scroll (panah, PgUp/PgDn,
    Home/End, roda mouse / swipe di terminal yang mendukung).
  - Flex: padding, banner, kolom tabel, dan tinggi panel log menyesuaikan
    ukuran jendela saat itu juga.

Catatan Windows:
  - VT/ANSI diaktifkan lewat kernel32.SetConsoleMode supaya warna & alternate
    screen jalan juga di conhost lama (cmd.exe klasik), bukan hanya Windows
    Terminal.
  - stdout dipaksa UTF-8 supaya border rich tidak jadi '?' di code page 437.
"""

from __future__ import annotations  # anotasi jadi lazy -> tui.py tetap bisa
# di-import walau `rich` belum terpasang (nama Panel/Group/Live baru dipakai
# saat dashboard benar-benar digambar, bukan saat class didefinisikan).

import atexit
import os
import re
import shutil
import sys
import time
from collections import deque
from datetime import datetime
from threading import Event, RLock, Thread

try:
    from rich.align import Align
    from rich.box import ASCII as BOX_ASCII, ROUNDED as BOX_ROUNDED, SIMPLE_HEAD
    from rich.console import Console, Group
    from rich.live import Live
    from rich.panel import Panel
    from rich.segment import Segment
    from rich.style import Style
    from rich.table import Table
    from rich.text import Text

    HAS_RICH = True
except ImportError:
    HAS_RICH = False


# ---------------------------------------------------------------
# Kompatibilitas terminal (Windows conhost / Termux)
# ---------------------------------------------------------------
def _enable_windows_ansi() -> bool:
    """Aktifkan VIRTUAL_TERMINAL_PROCESSING di console Windows.

    Return True bila berhasil (atau bukan Windows). Tanpa ini, conhost lama
    mencetak escape code mentah seperti '[32m' alih-alih warna."""
    if os.name != "nt":
        return True
    try:
        import ctypes

        kernel32 = ctypes.windll.kernel32
        ENABLE_VT = 0x0004
        ok = True
        for handle_id in (-11, -12):  # STDOUT, STDERR
            handle = kernel32.GetStdHandle(handle_id)
            mode = ctypes.c_uint32()
            if kernel32.GetConsoleMode(handle, ctypes.byref(mode)):
                kernel32.SetConsoleMode(handle, mode.value | ENABLE_VT)
            else:
                ok = False
        return ok
    except Exception:
        return False


def _force_utf8_stdout():
    """Paksa stdout/stderr ke UTF-8 (Python 3.7+)."""
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")
        except Exception:
            pass
    if os.name == "nt":
        try:
            import ctypes

            ctypes.windll.kernel32.SetConsoleOutputCP(65001)
            ctypes.windll.kernel32.SetConsoleCP(65001)
        except Exception:
            pass


def fit_console_width() -> None:
    """Windows: lebarkan buffer console agar dashboard memakai seluruh jendela.

    cmd.exe/conhost membuka console dengan buffer tetap (umumnya 120 kolom).
    Isi console TIDAK BOLEH melebihi lebar buffer, jadi waktu jendelanya
    di-maximize sisi kanan tinggal area kosong -- bukan karena layout-nya
    kurang lebar, tapi karena rich memang cuma dijatah 120 kolom.

    Yang dilakukan: hitung berapa kolom yang sebenarnya muat di jendela (lebar
    area gambar dibagi lebar sel huruf), lalu kalau buffer lebih sempit dari itu,
    buffer DAN viewport-nya dilebarkan. Hanya membesarkan, tidak pernah
    mengecilkan, jadi jendela yang sengaja dibuat kecil tidak diganggu.
    Semua dibungkus try/except: gagal = diam-diam dilewati.
    """
    if os.name != "nt":
        return
    try:
        import ctypes
        from ctypes import wintypes

        class COORD(ctypes.Structure):
            _fields_ = [("X", wintypes.SHORT), ("Y", wintypes.SHORT)]

        class SMALL_RECT(ctypes.Structure):
            _fields_ = [("Left", wintypes.SHORT), ("Top", wintypes.SHORT),
                        ("Right", wintypes.SHORT), ("Bottom", wintypes.SHORT)]

        class CSBI(ctypes.Structure):
            _fields_ = [("dwSize", COORD), ("dwCursorPosition", COORD),
                        ("wAttributes", wintypes.WORD), ("srWindow", SMALL_RECT),
                        ("dwMaximumWindowSize", COORD)]

        class CONSOLE_FONT_INFO(ctypes.Structure):
            _fields_ = [("nFont", wintypes.DWORD), ("dwFontSize", COORD)]

        k32 = ctypes.windll.kernel32
        handle = k32.GetStdHandle(-11)
        info = CSBI()
        if not k32.GetConsoleScreenBufferInfo(handle, ctypes.byref(info)):
            return

        win_cols = info.srWindow.Right - info.srWindow.Left + 1
        win_rows = info.srWindow.Bottom - info.srWindow.Top + 1

        k32.GetLargestConsoleWindowSize.restype = COORD
        largest = int(k32.GetLargestConsoleWindowSize(handle).X)

        # Berapa kolom yang SEBENARNYA muat di jendela? Diukur dari lebar area
        # gambar jendela (piksel) dibagi lebar satu sel huruf. Ini yang dipakai,
        # bukan status maximize -- jendela yang diperlebar dengan tarikan mouse
        # pun ikut terdeteksi, dan mode --once yang cuma hidup beberapa detik
        # tidak perlu menunggu pengecekan berikutnya.
        target = 0
        hwnd = k32.GetConsoleWindow()
        if hwnd:
            user32 = ctypes.windll.user32
            rect = wintypes.RECT()
            font = CONSOLE_FONT_INFO()
            if (user32.GetClientRect(hwnd, ctypes.byref(rect))
                    and k32.GetCurrentConsoleFont(handle, False, ctypes.byref(font))
                    and font.dwFontSize.X > 0):
                muat = (rect.right - rect.left) // int(font.dwFontSize.X)
                if 20 <= muat <= largest:
                    target = muat
            # Cadangan: jendela di-maximize tapi ukuran selnya tidak terbaca
            # (misal font proporsional aneh) -> pakai ukuran layar penuh.
            if not target and user32.IsZoomed(hwnd):
                target = largest

        if target <= win_cols:
            return  # sudah selebar yang muat -- tidak ada yang perlu dibenahi

        # Buffer harus dibesarkan DULU, baru jendelanya boleh ikut melebar.
        k32.SetConsoleScreenBufferSize(
            handle, COORD(target, max(int(info.dwSize.Y), win_rows)))
        k32.SetConsoleWindowInfo(handle, True, ctypes.byref(
            SMALL_RECT(0, info.srWindow.Top, target - 1, info.srWindow.Bottom)))
    except Exception:
        pass


def _set_console_title(title: str):
    if os.name != "nt":
        return
    try:
        import ctypes

        ctypes.windll.kernel32.SetConsoleTitleW(title)
    except Exception:
        pass


def platform_label() -> str:
    """Label platform untuk subtitle banner."""
    if os.name == "nt":
        return "Windows"
    if "com.termux" in os.environ.get("PREFIX", ""):
        return "Termux"
    return "Linux"


def term_width(default: int = 80) -> int:
    return shutil.get_terminal_size((default, 24)).columns


# ---------------------------------------------------------------
# Input keyboard untuk scroll
# ---------------------------------------------------------------
# Escape sequence terminal (Linux / Termux / Windows Terminal) -> aksi scroll.
# Roda mouse dan swipe Termux di alternate screen juga dikirim terminal
# sebagai panah atas/bawah, jadi ikut tertangkap di sini.
SCROLL_KEYS = {
    "\x1b[A": "up", "\x1bOA": "up", "k": "up",
    "\x1b[B": "down", "\x1bOB": "down", "j": "down",
    "\x1b[5~": "pgup", "b": "pgup",
    "\x1b[6~": "pgdn", " ": "pgdn",
    "\x1b[H": "home", "\x1bOH": "home", "\x1b[1~": "home", "\x1b[7~": "home",
    "g": "home",
    "\x1b[F": "end", "\x1bOF": "end", "\x1b[4~": "end", "\x1b[8~": "end",
    "G": "end",
}
# msvcrt.getwch(): tombol khusus datang sebagai '\x00'/'\xe0' + kode ini.
_WIN_SCAN_KEYS = {"H": "up", "P": "down", "I": "pgup", "Q": "pgdn",
                  "G": "home", "O": "end"}
_ESC_SEQ = re.compile(r"\x1b(?:\[[0-9;?]*[@-~]|O[@-~])")
_ESC_PARTIAL = re.compile(r"\x1b(?:\[[0-9;?]*|O)?")


def parse_keys(buf: str):
    """Pecah input mentah terminal jadi daftar aksi scroll.

    Return (aksi, sisa). `sisa` = escape sequence yang belum lengkap (misal
    '\\x1b[5' tanpa '~'), disimpan sampai potongan berikutnya terbaca."""
    keys = []
    while buf:
        if buf[0] != "\x1b":
            if buf[0] in SCROLL_KEYS:
                keys.append(SCROLL_KEYS[buf[0]])
            buf = buf[1:]
            continue
        m = _ESC_SEQ.match(buf)
        if m:
            if m.group() in SCROLL_KEYS:
                keys.append(SCROLL_KEYS[m.group()])
            buf = buf[m.end():]
        elif _ESC_PARTIAL.fullmatch(buf):
            break
        else:
            buf = buf[1:]  # ESC tunggal / sequence asing: buang ESC-nya
    return keys, buf


class _KeyReader:
    """Baca tombol scroll di thread latar, tanpa perlu menekan Enter.

    POSIX (Linux/Termux): stdin dipindah ke mode cbreak (tanpa echo, tanpa
    buffer baris) lalu dikembalikan saat stop() / exit. Ctrl+C tetap jadi
    SIGINT seperti biasa. Windows: msvcrt, mode console tidak diubah."""

    def __init__(self, on_key):
        self._on_key = on_key
        self._stop = Event()
        self._thread = None
        self._fd = None
        self._saved = None
        self.active = False

    def start(self):
        try:
            if not sys.stdin or not sys.stdin.isatty():
                return
            if os.name == "nt":
                import msvcrt  # noqa: F401
                target = self._run_windows
            else:
                import termios
                import tty

                self._fd = sys.stdin.fileno()
                self._saved = termios.tcgetattr(self._fd)
                tty.setcbreak(self._fd)
                atexit.register(self._restore)
                target = self._run_posix
        except Exception:
            self._restore()
            return
        self.active = True
        self._thread = Thread(target=target, name="tui-keys", daemon=True)
        self._thread.start()

    def stop(self):
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=0.5)
            self._thread = None
        self._restore()
        self.active = False

    def _restore(self):
        if self._saved is None:
            return
        try:
            import termios

            termios.tcsetattr(self._fd, termios.TCSADRAIN, self._saved)
        except Exception:
            pass
        self._saved = None

    def _emit(self, keys):
        for key in keys:
            try:
                self._on_key(key)
            except Exception:
                pass

    def _run_posix(self):
        import select

        buf = ""
        while not self._stop.is_set():
            try:
                ready, _, _ = select.select([self._fd], [], [], 0.1)
                if not ready:
                    buf = ""  # sisa escape yang tidak pernah lengkap
                    continue
                data = os.read(self._fd, 64)
            except Exception:
                return
            if not data:
                return  # EOF
            keys, buf = parse_keys(buf + data.decode("utf-8", "ignore"))
            self._emit(keys)

    def _run_windows(self):
        import msvcrt

        while not self._stop.is_set():
            try:
                if not msvcrt.kbhit():
                    self._stop.wait(0.05)
                    continue
                ch = msvcrt.getwch()
                if ch in ("\x00", "\xe0"):
                    key = _WIN_SCAN_KEYS.get(msvcrt.getwch())
                else:
                    key = SCROLL_KEYS.get(ch)
            except Exception:
                return
            if key:
                self._emit([key])


# ---------------------------------------------------------------
# ASCII Art Header
# ---------------------------------------------------------------
ASCII_BANNER = r"""
    ___         __           ___    __
   /   | __  __/ /_____     /   |  / /_  ________  ____
  / /| |/ / / / __/ __ \   / /| | / __ \/ ___/ _ \/ __ \
 / ___ / /_/ / /_/ /_/ /  / ___ |/ /_/ (__  )  __/ / / /
/_/  |_\__,_/\__/\____/  /_/  |_/_.___/____/\___/_/ /_/
"""

BANNER_WIDTH = max(len(l) for l in ASCII_BANNER.strip("\n").splitlines())
VERSION = "v2.0"
SUBTITLE = f"Polban Auto Absen -- {platform_label()} {VERSION}"
COMPACT_TITLE = ">>> POLBAN AUTO ABSEN <<<"


def print_ascii_banner():
    """Cetak ASCII art ke stdout (dipakai plain mode)."""
    cols = term_width()
    if cols < BANNER_WIDTH + 2:
        print(COMPACT_TITLE.center(cols))
        print(SUBTITLE.center(cols))
        print()
        return
    lines = ASCII_BANNER.strip("\n").splitlines()
    pad = " " * max(0, (cols - BANNER_WIDTH) // 2)
    for line in lines:
        print(pad + line)
    print(SUBTITLE.center(cols))
    print()


# ---------------------------------------------------------------
# Status constants
# ---------------------------------------------------------------
# Hanya EMPAT status, sesuai warna tombol di halaman akademik. Hasil klik
# otomatis tidak memunculkan status baru: baris yang berhasil diabsenkan
# langsung menjadi SUDAH ABSEN (hijau), yang gagal tetap SIAP ABSEN (biru)
# dan akan dicoba lagi -- kegagalannya dilaporkan lewat Log Aktivitas.
STATUS_LABELS = {
    "blue":    "SIAP ABSEN",
    "green":   "SUDAH ABSEN",
    "orange":  "BELUM BUKA",
    "red":     "BELUM WAKTUNYA",
    "unknown": "TIDAK DIKENALI",   # cadangan; tidak seharusnya muncul
}

STATUS_STYLES = {
    "blue":    "bold cyan",
    "green":   "bold green",
    "orange":  "bold yellow",
    "red":     "bold red",
    "unknown": "dim",
}

LEVEL_STYLES = {
    "ok":   "green",
    "fail": "bold red",
    "warn": "yellow",
    "info": "dim white",
    "skip": "dim cyan",
}

LEVEL_TAGS = {
    "ok":   "[OK]",
    "fail": "[!!]",
    "warn": "[??]",
    "info": "[--]",
    "skip": "[>>]",
}


# ---------------------------------------------------------------
# Data models
# ---------------------------------------------------------------
class MataKuliah:
    """Satu baris matkul di tabel jadwal."""
    __slots__ = ("nama", "jam", "dosen", "status")

    def __init__(self, nama: str, jam: str = "", dosen: str = "",
                 status: str = "unknown"):
        self.nama = nama
        self.jam = jam
        self.dosen = nama_dosen(dosen)
        self.status = status


class LogEntry:
    """Satu baris di activity log."""
    __slots__ = ("timestamp", "code", "message", "level")

    def __init__(self, code: str, message: str, level: str = "info"):
        self.timestamp = time.strftime("%H:%M:%S")
        self.code = code
        self.message = message
        self.level = level


def nama_dosen(teks: str) -> str:
    """Buang kode dosen di depan: 'KO078N-Trisna Gelar Abdillah, S.T., M.Kom'
    -> 'Trisna Gelar Abdillah, S.T., M.Kom'.

    Hanya dipotong kalau bagian sebelum '-' memang berbentuk kode (pendek,
    tanpa spasi), supaya nama yang mengandung tanda hubung tidak ikut terpotong.
    """
    t = " ".join(str(teks or "").split())
    if "-" in t:
        kode, sisa = t.split("-", 1)
        kode, sisa = kode.strip(), sisa.strip()
        # Kode dosen selalu memuat angka (KO078N, BD011N, ...) -- syarat ini
        # mencegah nama bertanda hubung seperti "Ana-Maria" ikut terpotong.
        if (sisa and kode and " " not in kode and len(kode) <= 12
                and any(c.isdigit() for c in kode)):
            return sisa
    return t


_JAM_RANGE = re.compile(
    r"(\d{1,2}:\d{2})(?::\d{2})?\s*(?:s\.?\s*d\.?|-)\s*(\d{1,2}:\d{2})(?::\d{2})?")


def jam_ringkas(jam: str) -> str:
    """'07:00:00 s.d. 09:30:00' -> '07:00-09:30' (untuk kolom sempit).
    Format lain dikembalikan apa adanya."""
    m = _JAM_RANGE.search(jam or "")
    return f"{m.group(1)}-{m.group(2)}" if m else jam


def _split_sig(sig: str):
    """Pecah signature engine jadi (nama_mk, dosen, jam).

    Engine memakai DUA bentuk signature:
      - baris yang di-skip : 'teks_tombol|nama_mk|dosen|jam'  (4 bagian)
      - aksi absen         : 'nama_mk|dosen|jam'              (3 bagian)
    Keduanya harus menghasilkan nama matkul yang sama, kalau tidak baris hasil
    klik tidak akan ketemu waktu status diperbarui.
    Playwright memakai 'teks_tombol|teks_baris' (2 bagian)."""
    parts = [p.strip() for p in str(sig).split("|")]
    if len(parts) >= 4:
        return parts[1], parts[2], parts[3]
    if len(parts) == 3:
        return parts[0], parts[1], parts[2]
    if len(parts) == 2:
        return parts[1], "", ""
    return parts[0] if parts else str(sig), "", ""


# ---------------------------------------------------------------
# TUI Dashboard (rich)
# ---------------------------------------------------------------
class _LiveView:
    """Renderable dinamis untuk rich.live.Live.

    Layout dibangun ULANG setiap kali frame digambar (bukan sekali saat Live
    dibuat), jadi dashboard langsung ikut menyesuaikan waktu jendela terminal
    di-maximize / di-restore / diseret lebarnya -- tanpa perlu menunggu event
    berikutnya dari engine.
    """

    __slots__ = ("_dash",)

    def __init__(self, dashboard):
        self._dash = dashboard

    def __rich_console__(self, console, options):
        yield self._dash._build_layout(options)


class _Lines:
    """Baris-baris yang sudah dirender (isi viewport) sebagai renderable."""

    __slots__ = ("_lines",)

    def __init__(self, lines):
        self._lines = lines

    def __rich_console__(self, console, options):
        new_line = Segment.line()
        for i, line in enumerate(self._lines):
            if i:
                yield new_line
            yield from line


class Dashboard:
    """Live terminal dashboard. Thread-safe."""

    MAX_LOG = 15

    def __init__(self, plain: bool = False, alt_screen: bool = True):
        self._plain = plain or not HAS_RICH
        self._alt_screen = alt_screen
        # RLock: layout digambar oleh thread refresh milik rich sementara
        # engine menulis state dari thread utama -- keduanya mengunci.
        self._lock = RLock()

        # State
        self._user_info = {"nim": "", "nama": "", "jurusan": ""}
        self._auth_status = "..."
        self._last_scan_info = "Menunggu scan awal..."
        self._engine = "http"
        self._interval = 60
        self._countdown = 0
        self._jadwal = []          # list[MataKuliah]
        self._log = deque(maxlen=self.MAX_LOG)
        self._cycle_count = 0
        self._stat_ok = 0
        self._stat_fail = 0
        # Nama matkul yang sudah berhasil diabsenkan di sesi ini -- dipakai agar
        # barisnya tetap SUDAH ABSEN walau scan berikutnya belum ikut berubah.
        self._absen_sukses = set()

        # Viewport: kalau dashboard lebih tinggi dari jendela, yang digambar
        # hanya potongan setinggi layar mulai baris ke-_scroll. Sisanya
        # dicapai lewat scroll, bukan dipotong.
        self._scroll = 0
        self._max_scroll = 0
        self._page = 10
        self._follow = False       # menempel di dasar seperti `tail -f`
        self._full_render = False  # frame terakhir tanpa alt screen: utuh

        # Rich objects
        if not self._plain:
            _force_utf8_stdout()
            _enable_windows_ansi()
            fit_console_width()
            _set_console_title("Polban Auto Absen")
            self._last_fit = 0.0
            self._console = Console()
            self._live = None
            self._keys = _KeyReader(self._on_key)
            # conhost lama: border unicode sering pecah -> pakai box ASCII
            self._box = BOX_ASCII if getattr(self._console, "legacy_windows", False) else BOX_ROUNDED

    # -- public API --------------------------------------------------

    def set_user_info(self, info: dict):
        with self._lock:
            self._user_info = info or {}
        self._refresh()

    def set_auth_status(self, status: str):
        with self._lock:
            self._auth_status = status
        self._refresh()

    def set_last_scan(self, info: str):
        with self._lock:
            self._last_scan_info = info
        self._refresh()

    def set_engine(self, engine: str, interval: int):
        with self._lock:
            self._engine = engine
            self._interval = interval
        self._refresh()

    def set_countdown(self, seconds: int):
        with self._lock:
            self._countdown = seconds
        self._refresh()

    def increment_cycle(self):
        with self._lock:
            self._cycle_count += 1
        self._refresh()

    def update_jadwal(self, actions: list, skips: list) -> bool:
        """Rebuild tabel jadwal dari actions (biru) dan skips.
        Return True jika ada perubahan status dibanding scan sebelumnya."""
        with self._lock:
            old_states = {mk.nama: mk.status for mk in self._jadwal}
            jadwal = []

            # Skips dulu (sudah absen / belum buka / dll.)
            for item in (skips or []):
                # `reason` ikut dibawa engine tapi tidak ditampilkan: alasannya
                # sudah terbaca dari kolom Status.
                color, sig = item[0], item[1]
                nama, dosen, jam = _split_sig(sig)
                jadwal.append(MataKuliah(nama=nama[:48], jam=jam, dosen=dosen,
                                         status=color))

            # Actions (biru / siap klik)
            for a in (actions or []):
                nama = a.get("nama_mk") or _split_sig(a.get("sig", "?"))[0]
                jadwal.append(MataKuliah(
                    nama=str(nama)[:48],
                    jam=a.get("jam", "") or _split_sig(a.get("sig", ""))[2],
                    dosen=a.get("dosen", "") or _split_sig(a.get("sig", ""))[1],
                    status="blue",
                ))

            changed = False
            for mk in jadwal:
                # Baris yang sudah berhasil diabsenkan di sesi ini tetap
                # SUDAH ABSEN, walau scan berikutnya sempat melaporkan warna
                # lama (server kadang telat memperbarui tombolnya).
                if mk.nama in self._absen_sukses:
                    mk.status = "green"
                if mk.nama in old_states and old_states[mk.nama] != mk.status:
                    changed = True
            if len(old_states) != len(jadwal) and len(old_states) > 0:
                changed = True

            self._jadwal = jadwal
        self._refresh()
        return changed

    def mark_result(self, sig: str, success: bool, nama: str = ""):
        """Update baris jadwal setelah dicoba diabsenkan.

        Berhasil -> langsung jadi SUDAH ABSEN (hijau), sama seperti matkul yang
        memang sudah hijau dari sananya. Gagal -> TETAP SIAP ABSEN (biru) supaya
        ikut dicoba lagi di siklus berikutnya; sebab gagalnya ada di log.

        `nama` sebaiknya diisi engine (a['nama_mk']) supaya pencocokan barisnya
        pasti; `sig` hanya dipakai sebagai cadangan."""
        target = (nama or "").strip() or _split_sig(sig)[0] \
            or str(sig).split("|")[0].strip()
        with self._lock:
            if success:
                self._stat_ok += 1
                if target:
                    self._absen_sukses.add(target)
            else:
                self._stat_fail += 1
            for mk in self._jadwal:
                if target and (target in mk.nama or mk.nama in target):
                    # Berhasil -> hijau. Gagal -> tetap biru supaya diulang;
                    # sebab gagalnya dilaporkan lewat Log Aktivitas.
                    mk.status = "green" if success else "blue"
                    break
        self._refresh()

    def push_event(self, code: str, message: str, level: str = "info"):
        """Tambah baris ke activity log."""
        entry = LogEntry(code, message, level)
        with self._lock:
            self._log.append(entry)
        if self._plain:
            tag = LEVEL_TAGS.get(level, "[--]")
            print(f"[{entry.timestamp}] {tag} [{code}] {message}", flush=True)
        else:
            self._refresh()

    def start(self):
        """Mulai live dashboard."""
        if self._plain:
            print_ascii_banner()
            return
        if self._alt_screen and not self._console.legacy_windows:
            # Alternate scroll mode (DECSET 1007): roda mouse di alternate
            # screen dikirim terminal sebagai panah atas/bawah, jadi ikut
            # men-scroll dashboard. Terminal yang tidak kenal mengabaikannya.
            # Sengaja tidak dimatikan saat keluar: di banyak terminal (VTE,
            # Konsole) perilaku ini memang bawaan.
            sys.stdout.write("\x1b[?1007h")
            sys.stdout.flush()
        # screen=True mengaktifkan Alternate Screen Buffer (seperti vim/htop):
        # layar bersih, riwayat shell tidak ikut ter-scroll.
        # _LiveView bikin layout dibangun ulang tiap frame -> responsif saat
        # jendela diubah ukurannya; 4 fps supaya resize terasa langsung.
        self._live = Live(
            _LiveView(self),
            console=self._console,
            refresh_per_second=4,
            screen=self._alt_screen,
        )
        self._live.start()
        self._keys.start()

    def stop(self):
        """Hentikan live dashboard."""
        keys = getattr(self, "_keys", None)
        if keys is not None:
            keys.stop()
        live = getattr(self, "_live", None)
        if live:
            # Tanpa alternate screen frame terakhir tertinggal di layar, jadi
            # digambar utuh (bukan potongan viewport) supaya seluruhnya tetap
            # bisa dibaca lewat scroll terminal.
            self._full_render = True
            try:
                live.stop()
            except Exception:
                pass
            self._live = None

    def render_summary(self):
        """Cetak ringkasan singkat SETELAH live dashboard ditutup.

        Hanya perlu kalau dashboard digambar di alternate screen -- layar itu
        dihapus saat keluar, jadi hasil akhirnya ikut hilang. Tanpa alternate
        screen (mis. mode --once) dashboard penuh tetap tertinggal di layar,
        jadi ringkasan ini dilewati supaya tidak dobel."""
        if self._plain or not HAS_RICH or not self._alt_screen:
            return
        try:
            info = self._user_info or {}
            console = self._console
            console.print()
            console.print(f"[bold cyan]{COMPACT_TITLE}[/bold cyan]  "
                          f"[dim]{SUBTITLE}[/dim]")
            console.print(f"  Nama    : [bold]{info.get('nama') or '-'}[/bold]  "
                          f"(NIM {info.get('nim') or '-'})")
            console.print(f"  Siklus  : {self._cycle_count}   "
                          f"[green]berhasil {self._stat_ok}[/green]   "
                          f"[red]gagal {self._stat_fail}[/red]")
            console.print(f"  Scan    : {self._last_scan_info}")
            for mk in self._jadwal:
                label = STATUS_LABELS.get(mk.status, mk.status.upper())
                style = STATUS_STYLES.get(mk.status, "")
                console.print(f"   - {mk.nama} {mk.jam}  [{style}]{label}[/{style}]"
                              + (f"  [dim]{mk.dosen}[/dim]" if mk.dosen else ""))
            console.print()
        except Exception:
            pass

    # -- internal ----------------------------------------------------

    def _refresh(self):
        live = getattr(self, "_live", None)
        if self._plain or live is None:
            return
        try:
            # Renderable-nya dinamis (_LiveView), jadi cukup minta gambar ulang.
            live.refresh()
        except Exception:
            pass  # Jangan crash karena render error

    def _on_key(self, key: str):
        """Geser viewport sesuai tombol dari _KeyReader."""
        with self._lock:
            step = {"up": -1, "down": 1, "pgup": -self._page,
                    "pgdn": self._page}.get(key, 0)
            if key == "home":
                pos = 0
            elif key == "end":
                pos = self._max_scroll
            else:
                pos = self._scroll + step
            self._scroll = max(0, min(self._max_scroll, pos))
            # Berhenti di dasar = ikuti konten, log baru tetap terlihat.
            self._follow = 0 < self._max_scroll == self._scroll
        self._refresh()

    def _build_banner(self, big: bool = True) -> Group:
        """Banner ASCII art besar, atau judul ringkas satu baris."""
        head = (Text(ASCII_BANNER.strip("\n"), style="bold cyan") if big
                else Text(COMPACT_TITLE, style="bold cyan"))
        return Group(
            Align.center(head),
            Align.center(Text(SUBTITLE, style="dim white")),
            Text(""),
        )

    def _build_layout(self, options=None) -> Panel:
        """Render full dashboard sebagai satu Panel setinggi jendela.

        Isi dirender utuh dulu, lalu diambil potongan setinggi layar mulai
        baris _scroll. Kalau isinya lebih tinggi dari jendela, muncul scrollbar
        di kanan dan petunjuk tombol di bingkai bawah -- tidak ada yang
        terpotong."""
        # Kalau user me-maximize jendela di tengah jalan, buffer console perlu
        # ikut dilebarkan. Dicek paling sering sekali per detik supaya tidak
        # membebani loop render.
        now = time.monotonic()
        if now - getattr(self, "_last_fit", 0.0) > 1.0:
            self._last_fit = now
            fit_console_width()

        options = options or self._console.options
        width = options.max_width
        height = options.height or options.size.height
        # Jendela lega: bingkai berjarak. Sempit/pendek: jaraknya dipangkas
        # supaya ruang dipakai untuk isi.
        roomy = width >= 80 and height >= 30
        pad_y, pad_x = (1, 2) if roomy else (0, 1)
        inner_w = max(10, width - 2 - 2 * pad_x)
        view_h = max(1, height - 2 - 2 * pad_y)
        full = self._full_render
        body = self._render_body(options, inner_w, None if full else view_h)
        total = len(body)

        with self._lock:
            if full or total <= view_h:
                self._scroll = self._max_scroll = 0
                visible, start = body, 0
            else:
                self._max_scroll = total - view_h
                self._page = max(1, view_h - 1)
                if self._follow:
                    self._scroll = self._max_scroll
                self._scroll = min(self._scroll, self._max_scroll)
                start = self._scroll
                visible = body[start:start + view_h]
            cycle = self._cycle_count

        scrolling = len(visible) < total
        padding = (pad_y, pad_x, pad_y, pad_x)
        if scrolling:
            # Scrollbar mengisi kolom terakhir jarak kanan, menempel ke
            # bingkai; lebar isi tidak berubah.
            bar = self._scrollbar(total, view_h, start)
            gap = Segment(" " * (pad_x - 1))
            visible = [line + [gap, seg] for line, seg in zip(visible, bar)]
            padding = (pad_y, 0, pad_y, pad_x)
        # Alt screen: bingkai selalu setinggi jendela. Layar biasa: setinggi
        # isinya saja, kecuali isinya tidak muat.
        fixed = scrolling or (self._alt_screen and not full)
        return Panel(
            _Lines(visible),
            title="[bold]POLBAN AUTO ABSEN[/bold]",
            subtitle=self._footer(width, cycle, start, len(visible), total),
            border_style="bright_blue",
            box=self._box,
            padding=padding,
            expand=True,
            height=height if fixed else None,
        )

    def _render_body(self, options, width: int, view_h):
        """Render semua bagian dashboard jadi daftar baris selebar `width`.

        Tinggi jendela ikut menentukan isi (flex):
          - banner ASCII besar hanya dipakai kalau semuanya tetap muat tanpa
            scroll; kalau tidak, cukup judul ringkas;
          - sisa tinggi diberikan ke panel log, jadi bingkai selalu penuh.
        view_h None = render utuh tanpa batas tinggi."""
        opts = options.update(width=width, height=None)

        def render(renderable):
            return self._console.render_lines(renderable, opts, pad=True)

        head = render(self._build_banner(big=False))
        status = render(self._build_status_bar(width))
        jadwal = render(self._build_jadwal_panel(width))
        log = render(self._build_log_panel(width))
        spare = 0
        if view_h is not None:
            spare = view_h - (len(head) + len(status) + len(jadwal)
                              + len(log) + 2)
        if width >= BANNER_WIDTH and (view_h is None or spare > 0):
            big = render(self._build_banner(big=True))
            extra = len(big) - len(head)
            if view_h is None or extra <= spare:
                head, spare = big, spare - extra
        if spare > 0 and self._alt_screen:
            log = render(self._build_log_panel(width, height=len(log) + spare))
        gap = [Segment(" " * width)]
        return head + status + [gap] + jadwal + [gap] + log

    def _scrollbar(self, total: int, view: int, start: int) -> list:
        """Satu Segment per baris viewport: jempol di posisi yang terlihat."""
        thumb = max(1, round(view * view / total))
        top = round(start * (view - thumb) / max(1, total - view))
        ascii_only = self._box is BOX_ASCII
        on = Segment("#" if ascii_only else "█", Style(color="bright_blue"))
        off = Segment("|" if ascii_only else "│", Style(dim=True))
        return [on if top <= i < top + thumb else off for i in range(view)]

    def _footer(self, width: int, cycle: int, start: int, shown: int,
                total: int):
        """Teks bingkai bawah: versi terpanjang yang masih muat."""
        if shown < total:
            pos = f"{start + 1}-{start + shown}/{total}"
            if self._keys.active:
                arrows = "Up/Dn" if self._box is BOX_ASCII else "↑↓"
                pilihan = [
                    f"siklus #{cycle} | {arrows} PgUp PgDn Home End: "
                    f"scroll {pos} | Ctrl+C keluar",
                    f"siklus #{cycle} | {arrows} PgUp PgDn: scroll {pos}",
                    f"{arrows} scroll {pos}",
                    pos,
                ]
            else:
                pilihan = [f"siklus #{cycle} | baris {pos} | Ctrl+C keluar",
                           pos]
        else:
            pilihan = [
                f"{VERSION} {platform_label().lower()} | siklus #{cycle} | "
                f"Ctrl+C untuk keluar",
                f"siklus #{cycle} | Ctrl+C keluar",
                f"siklus #{cycle}",
            ]
        for teks in pilihan:
            if len(teks) + 6 <= width:
                return f"[dim]{teks}[/dim]"
        return None

    def _build_status_bar(self, width: int) -> Panel:
        """Panel identitas + status.

        Disusun sebagai grid label | : | nilai, jadi nilai panjang yang
        terpaksa patah tetap menjorok rapi di kolomnya. Engine, Interval,
        Next scan, dan Absen digabung satu baris selama muat, lalu dipecah
        bertahap kalau jendelanya sempit."""
        with self._lock:
            info = dict(self._user_info or {})
            auth = self._auth_status
            last_scan = self._last_scan_info
            engine, interval = self._engine, self._interval
            countdown = self._countdown
            ok, fail = self._stat_ok, self._stat_fail

        nama = info.get("nama", "") or "-"
        nim = info.get("nim", "") or "-"
        jurusan = info.get("jurusan", "") or ""
        auth_style = "green" if ("OK" in auth or "Authenticated" in auth) else (
            "yellow" if "..." in auth or "re-login" in auth.lower() else "red")

        countdown_str = f"{countdown // 60:02d}:{countdown % 60:02d}"
        tgl = datetime.now().strftime("%A, %d %B %Y")

        pad = 2 if width >= 50 else 1
        # Lebar kolom nilai = isi panel - label terpanjang ('Hasil Scan') - ':'
        # beserta jaraknya.
        value_w = width - 2 - 2 * pad - 13

        grid = Table.grid(padding=(0, 1))
        grid.add_column(style="dim", no_wrap=True)
        grid.add_column(style="dim", no_wrap=True)
        grid.add_column(overflow="fold")

        def row(label, value, style=""):
            if not isinstance(value, Text):
                value = Text(str(value), style=style)
            grid.add_row(label, ":", value)

        row("Nama", nama, "bold")
        row("NIM", nim)
        if jurusan:
            row("Jurusan", jurusan)
        row("Tanggal", tgl)
        grid.add_row("", "", "")
        row("Status", auth, auth_style)
        row("Hasil Scan", last_scan)

        mesin = Text.assemble(engine, "   ", ("Interval : ", "dim"),
                              f"{interval}s", "   ", ("Next scan : ", "dim"),
                              (countdown_str, "bold cyan"))
        absen = Text.assemble((f"{ok} berhasil", "green"), (" / ", "dim"),
                              (f"{fail} gagal", "red"))
        if mesin.cell_len + 11 + absen.cell_len <= value_w:
            row("Engine", Text.assemble(mesin, "   ", ("Absen : ", "dim"), absen))
        elif mesin.cell_len <= value_w:
            row("Engine", mesin)
            row("Absen", absen)
        else:
            row("Engine", engine)
            row("Interval", f"{interval}s")
            row("Next scan", countdown_str, "bold cyan")
            row("Absen", absen)

        return Panel(grid, title="[bold]Identitas[/bold]",
                     border_style="dim", box=self._box, padding=(0, pad))

    def _build_jadwal_panel(self, width: int) -> Panel:
        """Tabel jadwal matkul hari ini.

        Isi tabel (matkul, jam, dosen) memakai warna teks normal -- BUKAN
        'dim' -- supaya sama terbacanya; dim di terminal gelap jadi abu-abu
        yang nyaris tenggelam ke latar.

        Empat tingkat, menyesuaikan lebar isi dashboard saat itu juga:
          >= 100 kolom : No | Mata Kuliah | Jam | Status | Dosen
          >=  56 kolom : Jam (diringkas HH:MM-HH:MM) pindah ke bawah nama
          >=  44 kolom : Dosen ikut pindah ke bawah nama matkul
          <   44 kolom : Status juga pindah ke bawah nama (layar HP)
        Kata yang lebih panjang dari kolomnya dipatah (fold), tidak dipotong.
        """
        narrow = width < 100
        tiny = width < 56
        micro = width < 44
        # Tabel sudah di dalam panel berbingkai: di jendela sempit bingkai
        # kedua dibuang supaya kolom nama tidak tergencet.
        slim = narrow and self._box is not BOX_ASCII

        table = Table(show_header=True, header_style="bold", expand=True,
                      show_lines=False, pad_edge=not tiny,
                      box=SIMPLE_HEAD if slim else self._box, show_edge=not slim)
        table.add_column("No", style="dim", width=3, justify="right")
        table.add_column("Mata Kuliah", ratio=3, min_width=14, overflow="fold")
        if not narrow:
            # 'HH:MM:SS s.d. HH:MM:SS' = 22 karakter: dikunci supaya tidak
            # dipatahkan jadi dua baris padahal ruangnya cukup.
            table.add_column("Jam", width=22)
        if not micro:
            # 'BELUM WAKTUNYA' = 14 karakter. Lebar DIKUNCI (bukan ratio)
            # supaya tidak pernah terpotong jadi 'SUDAH A...'.
            table.add_column("Status", width=14, justify="center", no_wrap=True)
        if not tiny:
            table.add_column("Dosen", ratio=3, min_width=14, overflow="fold")

        n_col = (2 + (0 if narrow else 1) + (0 if micro else 1)
                 + (0 if tiny else 1))

        with self._lock:
            jadwal = list(self._jadwal)

        if not jadwal:
            kosong = ["", "[dim]Belum ada data jadwal[/dim]"]
            table.add_row(*(kosong + [""] * (n_col - 2)))
        else:
            for i, mk in enumerate(jadwal, 1):
                status = Text(STATUS_LABELS.get(mk.status, mk.status.upper()),
                              style=STATUS_STYLES.get(mk.status, ""))
                if narrow:
                    nama = Text(mk.nama)
                    if micro:
                        nama.append("\n")
                        nama.append_text(status)
                    if mk.jam:
                        nama.append(f"\n{jam_ringkas(mk.jam)}")
                    if tiny and mk.dosen:
                        nama.append(f"\n{mk.dosen}")
                    row = [str(i), nama] + ([] if micro else [status])
                    if not tiny:
                        row.append(Text(mk.dosen or "-"))
                    table.add_row(*row)
                else:
                    table.add_row(str(i), Text(mk.nama), Text(mk.jam), status,
                                  Text(mk.dosen or "-"))

        return Panel(table, title="[bold]Jadwal Hari Ini[/bold]",
                     border_style="dim", box=self._box,
                     padding=(0, 0) if tiny else (0, 1))

    def _build_log_panel(self, width: int, height: int = None) -> Panel:
        """Ring buffer activity log -- semua entri (maks MAX_LOG) ditampilkan;
        kalau tidak muat, dashboard yang di-scroll.

        Grid waktu | tag | pesan: pesan panjang yang patah tetap menjorok di
        kolom pesan. `height` diisi _render_body supaya panel ini memenuhi
        sisa tinggi jendela."""
        with self._lock:
            entries = list(self._log)

        pad = 2 if width >= 50 else 1
        if not entries:
            content = Text("Belum ada aktivitas.", style="dim")
        else:
            content = Table.grid(padding=(0, 1))
            content.add_column(style="dim", no_wrap=True)
            content.add_column(no_wrap=True)
            content.add_column(overflow="fold")
            for entry in entries:
                content.add_row(
                    entry.timestamp,
                    Text(LEVEL_TAGS.get(entry.level, "[--]"),
                         style=LEVEL_STYLES.get(entry.level, "")),
                    Text.assemble((f"[{entry.code}] ", "bold"), entry.message),
                )

        return Panel(content,
                     title=f"[bold]Log Aktivitas ({len(entries)} terakhir)[/bold]",
                     border_style="dim", box=self._box, padding=(0, pad),
                     height=height)


# ---------------------------------------------------------------
# Plain-text fallback (no rich)
# ---------------------------------------------------------------
class PlainDashboard(Dashboard):
    """Fallback ketika rich tidak tersedia atau --plain/--once dipakai."""

    def __init__(self):
        super().__init__(plain=True)
        self._jadwal_printed = False

    def start(self):
        _force_utf8_stdout()
        print_ascii_banner()

    def set_user_info(self, info: dict):
        with self._lock:
            self._user_info = info or {}
        info = self._user_info
        garis = "=" * 56
        print(garis)
        if info.get("nama"):
            print(f"  Nama    : {info['nama']}")
        print(f"  NIM     : {info.get('nim', '-')}")
        if info.get("jurusan"):
            print(f"  Jurusan : {info['jurusan']}")
        print(f"  Waktu   : {time.strftime('%Y-%m-%d %H:%M:%S')}")
        print(f"  Engine  : {self._engine}   Interval : {self._interval}s")
        print(garis, flush=True)

    def update_jadwal(self, actions, skips):
        """Plain mode tidak punya tabel, jadi jadwal dicetak sebagai daftar.

        Hanya saat scan pertama atau ketika ada status yang berubah -- bukan
        tiap siklus -- supaya log (dan absen.log di Termux) tidak membengkak."""
        changed = super().update_jadwal(actions, skips)
        if changed or not self._jadwal_printed:
            self._jadwal_printed = True
            with self._lock:
                jadwal = list(self._jadwal)
            print(f"[{time.strftime('%H:%M:%S')}] Jadwal hari ini:")
            if not jadwal:
                print("     (tidak ada baris jadwal terbaca)")
            for i, mk in enumerate(jadwal, 1):
                label = STATUS_LABELS.get(mk.status, mk.status.upper())
                jam = f"  {mk.jam}" if mk.jam else ""
                dosen = f"  [{mk.dosen}]" if mk.dosen else ""
                print(f"     {i}. {mk.nama}{jam}  -> {label}{dosen}")
            print(flush=True)
        return changed

    def set_countdown(self, seconds):
        with self._lock:
            self._countdown = seconds

    def set_auth_status(self, status):
        with self._lock:
            self._auth_status = status

    def set_last_scan(self, info):
        with self._lock:
            self._last_scan_info = info

    def increment_cycle(self):
        with self._lock:
            self._cycle_count += 1

    def mark_result(self, sig: str, success: bool):
        with self._lock:
            if success:
                self._stat_ok += 1
            else:
                self._stat_fail += 1

    def render_summary(self):
        print(f"\nRingkasan: {self._cycle_count} siklus | "
              f"{self._stat_ok} berhasil | {self._stat_fail} gagal", flush=True)


def create_dashboard(plain: bool = False, alt_screen: bool = True) -> Dashboard:
    """Factory: buat Dashboard yang tepat berdasar kemampuan terminal."""
    if plain or not HAS_RICH:
        return PlainDashboard()
    if not sys.stdout.isatty():
        return PlainDashboard()
    return Dashboard(alt_screen=alt_screen)

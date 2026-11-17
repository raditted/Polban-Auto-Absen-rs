@echo off
REM ============================================================
REM  Polban Auto Absen - Persiapan sebelum jalan
REM
REM  SATU file ini dipanggil oleh SEMUA launcher (auto_absen.bat dan
REM  absen_sekali.bat) supaya keduanya tidak pernah beda perilaku.
REM
REM  Isi pemeriksaan:
REM    1. Binary polban-absen.exe tersedia (lokal / target\release / PATH)
REM       -> kalau belum ada, jalankan install.bat
REM    2. .env ada dan POLBAN_USERNAME terisi -> peringatkan kalau belum
REM
REM  Exit code: 0 = siap jalan (POLBAN_BIN ter-set), 1 = batal.
REM ============================================================
setlocal EnableExtensions DisableDelayedExpansion

call :find_bin
if not defined FOUND_BIN (
  echo [Setup] polban-absen.exe belum ditemukan -- menjalankan install.bat...
  call "%~dp0install.bat" || exit /b 1
  call :find_bin
)
if not defined FOUND_BIN (
  echo [Setup] GAGAL: polban-absen.exe tetap tidak ditemukan.
  echo         Jalankan install.bat atau build: cargo build --release --bin polban-absen
  exit /b 1
)

REM ---------- Config .env ----------
if not exist "%~dp0.env" (
  if exist "%~dp0.env.example" (
    copy "%~dp0.env.example" "%~dp0.env" >nul
    echo [Setup] .env dibuat dari template -- EDIT dulu: isi NIM ^& password.
  ) else (
    echo [Setup] GAGAL: .env dan .env.example sama-sama tidak ada.
    exit /b 1
  )
)
findstr /R /C:"^POLBAN_USERNAME=." "%~dp0.env" >nul 2>&1
if errorlevel 1 (
  echo.
  echo [Setup] POLBAN_USERNAME di .env sepertinya masih kosong.
  echo         Buka .env lalu isi POLBAN_USERNAME dan POLBAN_PASSWORD.
  echo.
  choice /C YN /M "Tetap lanjut"
  if errorlevel 2 exit /b 1
)

endlocal & set "POLBAN_BIN=%FOUND_BIN%"
exit /b 0

:find_bin
set "FOUND_BIN="
if exist "%~dp0polban-absen.exe" (
  set "FOUND_BIN=%~dp0polban-absen.exe"
  exit /b 0
)
if exist "%~dp0..\target\release\polban-absen.exe" (
  set "FOUND_BIN=%~dp0..\target\release\polban-absen.exe"
  exit /b 0
)
for /f "delims=" %%p in ('where polban-absen.exe 2^>nul') do (
  if not defined FOUND_BIN set "FOUND_BIN=%%p"
)
exit /b 0

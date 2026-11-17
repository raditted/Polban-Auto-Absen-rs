@echo off
REM ============================================================
REM  Polban Auto Absen - Installer Windows (sekali saja)
REM  Menyiapkan binary polban-absen.exe (build Cargo lokal atau
REM  unduh dari GitHub Releases) dan membuat .env dari template.
REM ============================================================
setlocal EnableExtensions DisableDelayedExpansion
cd /d "%~dp0"
set "BIN_OUT=%~dp0polban-absen.exe"
set "REPO=%POLBAN_RELEASE_REPO%"
if not defined REPO set "REPO=raditted/auto-presensi"
set "DL_URL=https://github.com/%REPO%/releases/latest/download/polban-absen-x86_64-pc-windows-msvc.exe"

echo [1/2] Menyiapkan binary polban-absen.exe...
if exist "%BIN_OUT%" (
  echo       Sudah ada: %BIN_OUT%
  goto check_env
)

if exist "%~dp0..\target\release\polban-absen.exe" (
  copy /Y "%~dp0..\target\release\polban-absen.exe" "%BIN_OUT%" >nul
  echo       Disalin dari ..\target\release\polban-absen.exe
  goto check_env
)

if exist "%~dp0..\Cargo.toml" (
  where cargo >nul 2>&1
  if not errorlevel 1 (
    echo       Membangun dari source via cargo build --release...
    cargo build --release --manifest-path "%~dp0..\Cargo.toml" --bin polban-absen
    if not errorlevel 1 if exist "%~dp0..\target\release\polban-absen.exe" (
      copy /Y "%~dp0..\target\release\polban-absen.exe" "%BIN_OUT%" >nul
      echo       Build selesai: %BIN_OUT%
      goto check_env
    )
    echo       Build cargo gagal -- mencoba unduh release binary...
  )
)

echo       Mengunduh release binary dari %DL_URL% ...
powershell -NoProfile -Command "Invoke-WebRequest -Uri '%DL_URL%' -OutFile '%BIN_OUT%'"
if errorlevel 1 (
  if exist "%BIN_OUT%" del "%BIN_OUT%" >nul 2>&1
  echo.
  echo [ERROR] Gagal menyiapkan polban-absen.exe.
  echo         Pastikan terhubung internet, atau pasang Rust lalu jalankan:
  echo           cargo build --release --bin polban-absen
  pause
  exit /b 1
)

:check_env
echo [2/2] Menyiapkan config .env...
if not exist "%~dp0.env" (
  if exist "%~dp0.env.example" (
    copy "%~dp0.env.example" "%~dp0.env" >nul
    echo       .env dibuat. EDIT dulu: isi NIM ^& password.
  ) else (
    (echo POLBAN_USERNAME=& echo POLBAN_PASSWORD=& echo POLBAN_TARGET_URL=https://akademik.polban.ac.id/ajar/absen& echo POLBAN_INTERVAL=60& echo POLBAN_ENGINE=http) > "%~dp0.env"
    echo       .env dibuat. EDIT dulu: isi NIM ^& password.
  )
) else (
  echo       .env sudah ada, dilewati.
)

echo.
echo ============================================================
echo  SELESAI. Langkah selanjutnya:
echo   1. Edit .env (buka Notepad), isi:
echo        POLBAN_USERNAME=NIM_kamu
echo        POLBAN_PASSWORD=password_kamu
echo   2. Klik 2x  auto_absen.bat   untuk menjalankan.
echo ============================================================
pause
exit /b 0

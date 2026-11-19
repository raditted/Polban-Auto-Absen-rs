@echo off
REM ============================================================
REM  Polban Auto Absen - Installer Windows (sekali saja)
REM  Menyiapkan binary polban-absen.exe (CLI/TUI) dan
REM  polban-absen-gui.exe (Desktop GUI), lalu membuat .env.
REM ============================================================
setlocal EnableExtensions DisableDelayedExpansion
cd /d "%~dp0"
set "CLI_OUT=%~dp0polban-absen.exe"
set "GUI_OUT=%~dp0polban-absen-gui.exe"
set "REPO=%POLBAN_RELEASE_REPO%"
if not defined REPO set "REPO=raditted/Polban-Auto-Absen-rs"
set "CLI_DL_URL=https://github.com/%REPO%/releases/latest/download/polban-absen-x86_64-pc-windows-msvc.exe"
set "GUI_DL_URL=https://github.com/%REPO%/releases/latest/download/Polban-Presensi-Portable-Windows-x64.zip"

echo [1/3] Menyiapkan binary CLI polban-absen.exe...
if exist "%CLI_OUT%" (
  echo       Sudah ada: %CLI_OUT%
  goto check_gui
)

if exist "%~dp0..\target\release\polban-absen.exe" (
  copy /Y "%~dp0..\target\release\polban-absen.exe" "%CLI_OUT%" >nul
  echo       Disalin dari ..\target\release\polban-absen.exe
  goto check_gui
)

if exist "%~dp0..\Cargo.toml" (
  where cargo >nul 2>&1
  if not errorlevel 1 (
    echo       Membangun dari source via cargo build --release...
    cargo build --release --manifest-path "%~dp0..\Cargo.toml" --bin polban-absen
    if not errorlevel 1 if exist "%~dp0..\target\release\polban-absen.exe" (
      copy /Y "%~dp0..\target\release\polban-absen.exe" "%CLI_OUT%" >nul
      echo       Build selesai: %CLI_OUT%
      goto check_gui
    )
    echo       Build cargo gagal -- mencoba unduh release binary...
  )
)

echo       Mengunduh release binary dari %CLI_DL_URL% ...
powershell -NoProfile -Command "Invoke-WebRequest -Uri '%CLI_DL_URL%' -OutFile '%CLI_OUT%'"
if errorlevel 1 (
  if exist "%CLI_OUT%" del "%CLI_OUT%" >nul 2>&1
  echo.
  echo [ERROR] Gagal menyiapkan polban-absen.exe.
  echo         Pastikan terhubung internet, atau pasang Rust lalu jalankan:
  echo           cargo build --release --bin polban-absen
  pause
  exit /b 1
)

:check_gui
echo [2/3] Menyiapkan binary Desktop GUI polban-absen-gui.exe...
if exist "%GUI_OUT%" (
  echo       Sudah ada: %GUI_OUT%
  goto check_env
)

if exist "%~dp0..\target\release\polban-absen-gui.exe" (
  copy /Y "%~dp0..\target\release\polban-absen-gui.exe" "%GUI_OUT%" >nul
  echo       Disalin dari ..\target\release\polban-absen-gui.exe
  goto check_env
)

if exist "%~dp0..\Cargo.toml" (
  where cargo >nul 2>&1
  if not errorlevel 1 (
    echo       Membangun GUI dari source via cargo build --release...
    cargo build --release --manifest-path "%~dp0..\Cargo.toml" --bin polban-absen-gui
    if not errorlevel 1 if exist "%~dp0..\target\release\polban-absen-gui.exe" (
      copy /Y "%~dp0..\target\release\polban-absen-gui.exe" "%GUI_OUT%" >nul
      echo       Build selesai: %GUI_OUT%
      goto check_env
    )
    echo       Build cargo GUI gagal -- mencoba unduh release binary...
  )
)

echo       Mengunduh paket portable GUI dari %GUI_DL_URL% ...
set "GUI_ZIP=%~dp0.polban-gui.zip"
set "GUI_TMP=%~dp0.polban-gui-tmp"
if exist "%GUI_ZIP%" del /Q "%GUI_ZIP%" >nul 2>&1
if exist "%GUI_TMP%" rmdir /S /Q "%GUI_TMP%" >nul 2>&1
powershell -NoProfile -Command "Invoke-WebRequest -Uri '%GUI_DL_URL%' -OutFile '%GUI_ZIP%'; Expand-Archive -Path '%GUI_ZIP%' -DestinationPath '%GUI_TMP%' -Force"
if not errorlevel 1 (
  if exist "%GUI_TMP%\Polban Presensi.exe" (
    copy /Y "%GUI_TMP%\Polban Presensi.exe" "%GUI_OUT%" >nul
  ) else if exist "%GUI_TMP%\polban-absen-gui.exe" (
    copy /Y "%GUI_TMP%\polban-absen-gui.exe" "%GUI_OUT%" >nul
  )
)
if exist "%GUI_ZIP%" del /Q "%GUI_ZIP%" >nul 2>&1
if exist "%GUI_TMP%" rmdir /S /Q "%GUI_TMP%" >nul 2>&1
if not exist "%GUI_OUT%" (
  echo.
  echo [ERROR] Gagal menyiapkan polban-absen-gui.exe.
  echo         Pastikan terhubung internet, atau pasang Rust lalu jalankan:
  echo           cargo build --release --bin polban-absen-gui
  pause
  exit /b 1
)

:check_env
echo [3/3] Menyiapkan config .env (opsional untuk CLI / impor GUI)...
if not exist "%~dp0.env" (
  if exist "%~dp0.env.example" (
    copy "%~dp0.env.example" "%~dp0.env" >nul
    echo       .env dibuat. Isi NIM ^& password jika memakai mode CLI atau impor GUI.
  ) else (
    (echo POLBAN_USERNAME=& echo POLBAN_PASSWORD=& echo POLBAN_TARGET_URL=https://akademik.polban.ac.id/ajar/absen& echo POLBAN_INTERVAL=60& echo POLBAN_ENGINE=http) > "%~dp0.env"
    echo       .env dibuat. Isi NIM ^& password jika memakai mode CLI atau impor GUI.
  )
) else (
  echo       .env sudah ada, dilewati.
)

echo.
echo ============================================================
echo  SELESAI. Langkah selanjutnya:
echo   - Desktop GUI: klik 2x polban_presensi_gui.bat
echo     (atau langsung klik 2x polban-absen-gui.exe)
echo   - Terminal TUI: edit .env lalu klik 2x auto_absen.bat
echo ============================================================
pause
exit /b 0

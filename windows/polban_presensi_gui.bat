@echo off
REM ============================================================
REM  Polban Presensi Desktop GUI - Launcher Windows
REM  Klik 2x file ini untuk membuka aplikasi desktop tanpa
REM  meninggalkan jendela cmd hitam.
REM ============================================================
setlocal EnableExtensions DisableDelayedExpansion

call :find_gui_bin
if not defined GUI_BIN (
  echo [Setup] polban-absen-gui.exe belum ditemukan -- menjalankan install.bat...
  call "%~dp0install.bat" || (pause & exit /b 1)
  call :find_gui_bin
)

if not defined GUI_BIN (
  echo [ERROR] polban-absen-gui.exe tetap tidak ditemukan.
  echo         Jalankan install.bat atau build: cargo build --release --bin polban-absen-gui
  pause
  exit /b 1
)

start "" "%GUI_BIN%" %*
exit /b 0

:find_gui_bin
set "GUI_BIN="
if exist "%~dp0polban-absen-gui.exe" (
  set "GUI_BIN=%~dp0polban-absen-gui.exe"
  exit /b 0
)
if exist "%~dp0..\target\release\polban-absen-gui.exe" (
  set "GUI_BIN=%~dp0..\target\release\polban-absen-gui.exe"
  exit /b 0
)
for /f "delims=" %%p in ('where polban-absen-gui.exe 2^>nul') do (
  if not defined GUI_BIN set "GUI_BIN=%%p"
)
exit /b 0

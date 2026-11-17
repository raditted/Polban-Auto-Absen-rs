@echo off
REM ============================================================
REM  Polban Auto Absen - Launcher Windows (mode LOOP)
REM  Klik 2x file ini untuk menjalankan auto-absen terus-menerus.
REM ============================================================
setlocal EnableExtensions DisableDelayedExpansion
cd /d "%~dp0"
set "ARGS=--engine http"
set "JUDUL=Polban Auto Absen"

call "%~dp0persiapan.bat" || (pause & exit /b 1)

REM UTF-8 agar border & warna live dashboard tampil benar di cmd.exe klasik.
for /f "tokens=2 delims=:." %%c in ('chcp') do set "OLDCP=%%c"
chcp 65001 >nul

title %JUDUL%
"%POLBAN_BIN%" %ARGS% %*

if defined OLDCP chcp %OLDCP% >nul
echo.
echo Selesai / berhenti.
pause

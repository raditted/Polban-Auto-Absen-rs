@echo off
REM ============================================================
REM  Polban Auto Absen - Launcher Windows (SEKALI jalan, untuk tes)
REM  Klik 2x file ini untuk scan+absen satu kali lalu berhenti.
REM ============================================================
setlocal EnableExtensions DisableDelayedExpansion
cd /d "%~dp0"
set "ARGS=--engine http --once"
set "JUDUL=Polban Auto Absen (sekali)"

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

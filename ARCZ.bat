@echo off
setlocal EnableExtensions
cd /d "%~dp0"
title ARCZ - Atualizar, preparar, testar e abrir

where powershell.exe >nul 2>nul
if errorlevel 1 (
  echo [ERRO] Windows PowerShell nao foi encontrado.
  pause
  exit /b 1
)

powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\windows\arcz-launch.ps1" %*
set "ARCZ_RC=%ERRORLEVEL%"
if not "%ARCZ_RC%"=="0" (
  echo.
  echo [ARCZ] Falha real detectada. Codigo: %ARCZ_RC%
  echo Consulte .arcz\logs\launcher-latest.log
  pause
)
exit /b %ARCZ_RC%

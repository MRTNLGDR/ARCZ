@echo off
setlocal EnableExtensions
cd /d "%~dp0"
title ARCZ Earth - Launcher local

where python >nul 2>nul
if errorlevel 1 (
  echo [ERRO] Python nao foi encontrado no PATH.
  pause
  exit /b 1
)

set "ARCZ_NETWORK_MODE=offline_strict"
set "ARCZ_BANCO=%CD%\resources\assets"
set "ARCZ_SEM_NAVEGADOR=1"

if not exist "%ARCZ_BANCO%" (
  echo [ERRO] Biblioteca local ausente: %ARCZ_BANCO%
  pause
  exit /b 1
)

echo [ARCZ] Validando runtime interativo local...
python tools\runtime_preflight.py --profile interactive > "%TEMP%\arcz-runtime-preflight.json"
if errorlevel 1 (
  echo.
  echo [BLOQUEADO] O ARCZ nao abrira uma interface parcialmente quebrada.
  echo Falta pelo menos um runtime local obrigatorio.
  echo.
  type "%TEMP%\arcz-runtime-preflight.json"
  echo.
  echo Execute PREPARAR_ARCZ.cmd e tente novamente.
  pause
  exit /b 2
)

echo [OK] Runtime local validado. Iniciando API/UI em http://127.0.0.1:8123/
start "ARCZ Local Server" /D "%~dp0" cmd /k "set ARCZ_NETWORK_MODE=offline_strict&& set ARCZ_BANCO=%CD%\resources\assets&& set ARCZ_SEM_NAVEGADOR=1&& python servidor.py 8123"

set /a tries=0
:wait_server
set /a tries+=1
powershell -NoProfile -Command "try { $r=Invoke-WebRequest -UseBasicParsing -TimeoutSec 1 http://127.0.0.1:8123/api/v2/health; if($r.StatusCode -eq 200){exit 0}else{exit 1} } catch { exit 1 }" >nul 2>nul
if not errorlevel 1 goto server_ready
if %tries% GEQ 30 goto server_failed
>nul timeout /t 1 /nobreak
goto wait_server

:server_ready
start "" "http://127.0.0.1:8123/"
exit /b 0

:server_failed
echo.
echo [ERRO] O servidor local nao respondeu em /api/v2/health.
echo Veja a janela "ARCZ Local Server" para o erro real.
pause
exit /b 3

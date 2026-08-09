@echo off
setlocal EnableExtensions
cd /d "%~dp0"
title ARCZ Earth - Preparar runtime local

where python >nul 2>nul
if errorlevel 1 (
  echo [ERRO] Python nao foi encontrado no PATH.
  echo Instale/use o Python local do ARCZ antes de continuar.
  pause
  exit /b 1
)

where node >nul 2>nul
if errorlevel 1 (
  echo [ERRO] Node.js 22+ nao foi encontrado no PATH.
  echo O Cesium pinado exige Node 22+ durante a preparacao.
  pause
  exit /b 1
)

where bun >nul 2>nul
if errorlevel 1 (
  echo [ERRO] Bun nao foi encontrado no PATH.
  echo Bun e obrigatorio para construir o fork Aedifex durante o setup.
  pause
  exit /b 1
)

echo ============================================================
echo  ARCZ EARTH - PREPARACAO LOCAL AUDITADA
echo ============================================================
echo Esta etapa pode acessar Git/npm apenas para materializar os SHAs
echo pinados. O resultado sera gravado dentro deste repositorio.
echo O runtime normal continuara offline_strict, sem CDN/fallback remoto.
echo.

set "ARCZ_BANCO=%CD%\resources\assets"
if not exist "%ARCZ_BANCO%" mkdir "%ARCZ_BANCO%"
set "ARCZ_NETWORK_MODE=import_assisted"
python tools\prepare_local_runtime.py --interactive
if errorlevel 1 (
  echo.
  echo [FALHA] O runtime local nao foi preparado integralmente.
  echo Consulte o erro acima. Nenhum mock/fallback sera usado.
  pause
  exit /b 1
)

set "ARCZ_NETWORK_MODE=offline_strict"
echo.
echo [OK] Mapa e modelador foram materializados e revalidados offline.
echo [OK] Biblioteca de assets: %ARCZ_BANCO%
echo Agora use ABRIR_ARCZ.cmd.
pause
exit /b 0

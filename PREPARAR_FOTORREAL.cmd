@echo off
setlocal EnableExtensions
cd /d "%~dp0"
title ARCZ Earth - Preparar Fotorreal local

if "%~1"=="" goto usage
if "%~2"=="" goto usage

where python >nul 2>nul
if errorlevel 1 (
  echo [ERRO] Python nao foi encontrado no PATH.
  pause
  exit /b 1
)

set "BLENDER_SOURCE=%~f1"
set "BLENDER_LICENSE=%~f2"
set "ARCZ_NETWORK_MODE=offline_strict"

echo ============================================================
echo  ARCZ EARTH - FOTORREAL LOCAL / CYCLES
echo ============================================================
echo Origem Blender: %BLENDER_SOURCE%
echo Licenca: %BLENDER_LICENSE%
echo.
echo Nada sera baixado. A distribuicao sera validada, copiada para

echo vendor\blender, hash-eada e revalidada dentro do repositorio.
echo.

python tools\vendor_blender.py --source "%BLENDER_SOURCE%" --license-file "%BLENDER_LICENSE%" --force
if errorlevel 1 (
  echo.
  echo [FALHA] Blender nao foi aceito. Nenhum fallback de PATH sera usado.
  pause
  exit /b 2
)

python tools\photoreal_preflight.py
if errorlevel 1 (
  echo.
  echo [FALHA] O vendor foi criado, mas o gate Cycles/worker nao passou.
  pause
  exit /b 3
)

echo.
echo [OK] Fotorreal base Cycles esta materializado e validado localmente.
echo Enhancement por IA continua opcional e exige modelo local apenas se ativado.
pause
exit /b 0

:usage
echo Uso:
echo   PREPARAR_FOTORREAL.cmd "C:\caminho\blender-portatil-ou.zip" "C:\caminho\LICENSE"
echo.
echo O primeiro argumento deve ser uma distribuicao Blender portatil REAL.
echo O segundo deve ser a licenca correspondente.
echo Nenhum download, CDN ou Blender encontrado no PATH e usado como substituto.
pause
exit /b 64

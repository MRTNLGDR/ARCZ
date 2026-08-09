@echo off
setlocal
cd /d "%~dp0"
echo [ARCZ] Preparacao manual separada foi eliminada.
echo [ARCZ] ARCZ.bat agora atualiza, prepara, testa e abre sozinho.
call "%~dp0ARCZ.bat" -ForceSetup %*
exit /b %ERRORLEVEL%

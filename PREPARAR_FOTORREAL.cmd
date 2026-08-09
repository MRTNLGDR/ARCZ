@echo off
setlocal
cd /d "%~dp0"
echo [ARCZ] O fluxo Fotorreal separado foi incorporado ao launcher unico.
echo [ARCZ] ARCZ.bat instala/localiza Blender real, copia para vendor e valida Cycles.
call "%~dp0ARCZ.bat" -ForceSetup %*
exit /b %ERRORLEVEL%

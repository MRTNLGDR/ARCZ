@echo off
setlocal
cd /d "%~dp0"
echo [ARCZ] Este atalho agora usa o launcher unico ARCZ.bat.
call "%~dp0ARCZ.bat" %*
exit /b %ERRORLEVEL%

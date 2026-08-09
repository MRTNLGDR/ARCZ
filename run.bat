@echo off
REM ===========================================================================
REM  ARCZ Earth V11  -  UM CLIQUE FAZ TUDO
REM
REM  Este e o UNICO arquivo que voce precisa executar. Ele, nesta ordem:
REM     1. puxa do GitHub o que foi corrigido/melhorado (git pull)
REM     2. instala / atualiza todas as dependencias
REM     3. sobe o servidor na porta 8123
REM     4. abre o navegador
REM
REM  Opcoes (arraste nao precisa, e so digitar no cmd):
REM     RUN.bat /sem-navegador   sobe sem abrir o navegador
REM     RUN.bat /sem-pull        nao atualiza pelo git, so instala e sobe
REM     RUN.bat /reinstalar      refaz node_modules / .venv do zero
REM     RUN.bat /parar           derruba o servidor deste projeto
REM
REM  Nunca apaga trabalho local: antes do pull ele guarda no git stash
REM  e devolve depois. Nao usa reset --hard.
REM ===========================================================================
setlocal
cd /d "%~dp0"
title ARCZ Earth V11

set "ARGS="
:parse
if "%~1"=="" goto pronto
if /i "%~1"=="/sem-navegador" set "ARGS=%ARGS% -SemNavegador" & shift & goto parse
if /i "%~1"=="/sem-pull"      set "ARGS=%ARGS% -SemPull"      & shift & goto parse
if /i "%~1"=="/reinstalar"    set "ARGS=%ARGS% -Reinstalar"   & shift & goto parse
if /i "%~1"=="/parar"         set "ARGS=%ARGS% -Parar"        & shift & goto parse
echo [aviso] opcao desconhecida: %~1
shift
goto parse
:pronto

if not exist "D:\AGENT_SYNC\motor-run.ps1" (
  echo [ERRO] motor nao encontrado em D:\AGENT_SYNC\motor-run.ps1
  pause
  exit /b 9
)

powershell -NoProfile -ExecutionPolicy Bypass -File "D:\AGENT_SYNC\motor-run.ps1" -Projeto ARCZ %ARGS%
set "RC=%ERRORLEVEL%"
if not "%RC%"=="0" (
  echo.
  echo Terminou com erro %RC%. A janela fica aberta para voce ler a causa acima.
  pause
)
endlocal & exit /b %RC%
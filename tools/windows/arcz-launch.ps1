param(
  [switch]$ForceSetup,
  [switch]$SkipUpdate,
  [switch]$NoBrowser,
  [switch]$SkipPhotoreal
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$StateDir = Join-Path $Root '.arcz'
$LogDir = Join-Path $StateDir 'logs'
$Toolchains = Join-Path $Root 'vendor\toolchains'
$AssetBank = Join-Path $Root 'resources\assets'
$PreparedHeadFile = Join-Path $StateDir 'prepared-head.txt'
$VerifiedHeadFile = Join-Path $StateDir 'verified-head.txt'
$ServerPidFile = Join-Path $StateDir 'server.pid'
$LatestLog = Join-Path $LogDir 'launcher-latest.log'

New-Item -ItemType Directory -Force -Path $StateDir,$LogDir,$Toolchains,$AssetBank | Out-Null
Set-Content -Path $LatestLog -Value '' -Encoding UTF8
Start-Transcript -Path $LatestLog -Force | Out-Null

function Step([string]$Text) {
  Write-Host ''
  Write-Host ('=== ' + $Text + ' ===') -ForegroundColor Cyan
}

function Refresh-Path {
  $machine = [Environment]::GetEnvironmentVariable('Path','Machine')
  $user = [Environment]::GetEnvironmentVariable('Path','User')
  $env:Path = @($machine,$user,$env:Path) -join ';'
}

function Command-Path([string]$Name, [string[]]$Fallbacks = @()) {
  $cmd = Get-Command $Name -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($cmd -and $cmd.Source -and (Test-Path $cmd.Source)) { return $cmd.Source }
  foreach ($candidate in $Fallbacks) {
    if ($candidate -and (Test-Path $candidate)) { return $candidate }
  }
  return $null
}

function Run([string]$Exe, [string[]]$Args, [switch]$AllowFailure) {
  Write-Host ('+ ' + $Exe + ' ' + ($Args -join ' ')) -ForegroundColor DarkGray
  & $Exe @Args
  $rc = $LASTEXITCODE
  if ($null -eq $rc) { $rc = 0 }
  if (($rc -ne 0) -and (-not $AllowFailure)) {
    throw "Comando falhou ($rc): $Exe $($Args -join ' ')"
  }
  return [int]$rc
}

function Has-Winget {
  return $null -ne (Command-Path 'winget.exe' @("$env:LOCALAPPDATA\Microsoft\WindowsApps\winget.exe"))
}

function Winget-Install([string]$Id) {
  if (-not (Has-Winget)) { return $false }
  $winget = Command-Path 'winget.exe' @("$env:LOCALAPPDATA\Microsoft\WindowsApps\winget.exe")
  Write-Host "[ARCZ] Instalando $Id via Windows Package Manager..."
  $rc = Run $winget @('install','--id',$Id,'-e','--silent','--accept-package-agreements','--accept-source-agreements','--disable-interactivity') -AllowFailure
  Refresh-Path
  return $rc -eq 0
}

function Ensure-Git {
  Step 'Git + atualização do código'
  $git = Command-Path 'git.exe' @(
    "$env:ProgramFiles\Git\cmd\git.exe",
    "$env:ProgramFiles\Git\bin\git.exe"
  )
  if (-not $git) {
    [void](Winget-Install 'Git.Git')
    $git = Command-Path 'git.exe' @("$env:ProgramFiles\Git\cmd\git.exe")
  }
  if (-not $git) { throw 'Git não pôde ser instalado automaticamente. Windows Package Manager/Git indisponível.' }
  Run $git @('--version') | Out-Null
  return $git
}

function Update-Repository([string]$Git) {
  if ($SkipUpdate) {
    Write-Host '[ARCZ] Atualização Git ignorada por -SkipUpdate.'
    return
  }
  if (-not (Test-Path (Join-Path $Root '.git'))) {
    throw 'Esta pasta não possui .git. Use um clone do repositório MRTNLGDR/ARCZ; o launcher não sobrescreve uma pasta ZIP sem histórico.'
  }

  $branch = (& $Git -C $Root symbolic-ref --quiet --short HEAD 2>$null).Trim()
  if (-not $branch) { throw 'Checkout Git está detached; selecione uma branch antes de usar o atualizador automático.' }

  $dirty = (& $Git -C $Root status --porcelain --untracked-files=all)
  if ($dirty) {
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    Write-Host '[ARCZ] Alterações locais detectadas; preservando em git stash antes da atualização.' -ForegroundColor Yellow
    Run $Git @('-C',$Root,'stash','push','-u','-m',"ARCZ launcher autostash $stamp") | Out-Null
    Set-Content -Path (Join-Path $StateDir 'last-autostash.txt') -Value "ARCZ launcher autostash $stamp" -Encoding UTF8
  }

  Run $Git @('-C',$Root,'fetch','--prune','origin') | Out-Null
  $remoteRef = "refs/remotes/origin/$branch"
  $exists = Run $Git @('-C',$Root,'show-ref','--verify','--quiet',$remoteRef) -AllowFailure
  if ($exists -eq 0) {
    Run $Git @('-C',$Root,'merge','--ff-only',"origin/$branch") | Out-Null
    Write-Host "[OK] $branch sincronizada com origin/$branch."
  } else {
    Write-Host "[AVISO] origin/$branch não existe; fetch executado, branch local preservada." -ForegroundColor Yellow
  }
}

function Ensure-SystemPython {
  Step 'Python local do bootstrap'
  $candidates = @(
    "$env:LOCALAPPDATA\Programs\Python\Python312\python.exe",
    "$env:ProgramFiles\Python312\python.exe"
  )
  $python = Command-Path 'python.exe' $candidates
  if ($python) {
    & $python -c 'import sys; raise SystemExit(0 if sys.version_info >= (3,11) else 7)'
    if ($LASTEXITCODE -ne 0) { $python = $null }
  }
  if (-not $python) {
    [void](Winget-Install 'Python.Python.3.12')
    Refresh-Path
    $python = Command-Path 'python.exe' $candidates
  }
  if (-not $python) { throw 'Python 3.11+ não pôde ser instalado automaticamente.' }
  Run $python @('--version') | Out-Null
  return $python
}

function Ensure-Venv([string]$BootstrapPython, [bool]$Changed) {
  $venv = Join-Path $Root '.venv'
  $venvPython = Join-Path $venv 'Scripts\python.exe'
  if (-not (Test-Path $venvPython)) {
    Step 'Criando .venv dentro do repositório'
    Run $BootstrapPython @('-m','venv',$venv) | Out-Null
    $Changed = $true
  }
  if ($Changed -or $ForceSetup) {
    Step 'Dependências Python'
    Run $venvPython @('-m','pip','install','--upgrade','pip') | Out-Null
    Run $venvPython @('-m','pip','install','-r',(Join-Path $Root 'requirements.txt'),'-r',(Join-Path $Root 'requirements-dev.txt')) | Out-Null
  }
  return $venvPython
}

function Node-Major([string]$Node) {
  if (-not $Node) { return 0 }
  $raw = (& $Node --version 2>$null).Trim().TrimStart('v')
  $first = ($raw -split '\.')[0]
  $value = 0
  [void][int]::TryParse($first,[ref]$value)
  return $value
}

function Ensure-Node {
  Step 'Node.js 22+'
  $node = Command-Path 'node.exe' @("$env:ProgramFiles\nodejs\node.exe")
  if ((Node-Major $node) -lt 22) {
    [void](Winget-Install 'OpenJS.NodeJS.LTS')
    Refresh-Path
    $node = Command-Path 'node.exe' @("$env:ProgramFiles\nodejs\node.exe")
  }
  if ((Node-Major $node) -lt 22) { throw 'Node.js 22+ não pôde ser instalado automaticamente.' }
  $nodeDir = Split-Path $node -Parent
  if ($env:Path -notlike "*$nodeDir*") { $env:Path = "$nodeDir;$env:Path" }
  Run $node @('--version') | Out-Null
  $npm = Command-Path 'npm.cmd' @((Join-Path $nodeDir 'npm.cmd'))
  if (-not $npm) { throw 'npm não foi encontrado junto do Node.js.' }
  return @{ Node = $node; Npm = $npm }
}

function Ensure-Bun([string]$Npm) {
  Step 'Bun de build dentro do repositório'
  $prefix = Join-Path $Toolchains 'bun'
  $bin = Join-Path $prefix 'node_modules\.bin\bun.cmd'
  if (-not (Test-Path $bin)) {
    Run $Npm @('install','--prefix',$prefix,'--no-audit','--no-fund','bun@1.3.14') | Out-Null
  }
  if (-not (Test-Path $bin)) { throw 'Bun 1.3.14 não foi materializado no vendor/toolchains.' }
  $binDir = Split-Path $bin -Parent
  $env:Path = "$binDir;$env:Path"
  Run $bin @('--version') | Out-Null
  return $bin
}

function Ensure-Rust {
  Step 'Rust 1.97.1 + workers release'
  $rustup = Command-Path 'rustup.exe' @("$env:USERPROFILE\.cargo\bin\rustup.exe")
  if (-not $rustup) {
    [void](Winget-Install 'Rustlang.Rustup')
    Refresh-Path
    $rustup = Command-Path 'rustup.exe' @("$env:USERPROFILE\.cargo\bin\rustup.exe")
  }
  if (-not $rustup) { throw 'rustup não pôde ser instalado automaticamente.' }
  $cargoDir = Join-Path $env:USERPROFILE '.cargo\bin'
  $env:Path = "$cargoDir;$env:Path"
  Run $rustup @('toolchain','install','1.97.1','--profile','minimal','--component','rustfmt','--component','clippy','--target','wasm32-unknown-unknown') | Out-Null
  $cargo = Command-Path 'cargo.exe' @((Join-Path $cargoDir 'cargo.exe'))
  if (-not $cargo) { throw 'cargo não apareceu após instalar Rust 1.97.1.' }
  return $cargo
}

function Interactive-Preflight([string]$Python, [switch]$Quiet) {
  if (-not $Quiet) { Step 'Preflight interativo real' }
  $oldMode = $env:ARCZ_NETWORK_MODE
  $env:ARCZ_NETWORK_MODE = 'offline_strict'
  & $Python (Join-Path $Root 'tools\runtime_preflight.py') --profile interactive *> (Join-Path $StateDir 'interactive-preflight.json')
  $rc = $LASTEXITCODE
  $env:ARCZ_NETWORK_MODE = $oldMode
  return $rc -eq 0
}

function Prepare-Interactive([string]$Python) {
  Step 'Cesium + Aedifex pinados e self-contained'
  $env:ARCZ_NETWORK_MODE = 'import_assisted'
  Run $Python @((Join-Path $Root 'tools\prepare_local_runtime.py'),'--interactive') | Out-Null
  $env:ARCZ_NETWORK_MODE = 'offline_strict'
  if (-not (Interactive-Preflight $Python -Quiet)) {
    Get-Content (Join-Path $StateDir 'interactive-preflight.json') | Write-Host
    throw 'Perfil interativo continua bloqueado após preparação.'
  }
}

function Blender-VendorReady([string]$Python) {
  $code = @'
from pathlib import Path
from tools.runtime_preflight import _blender_check
import sys
r=_blender_check(Path.cwd())
raise SystemExit(0 if r['status']=='READY' else 1)
'@
  Push-Location $Root
  try { & $Python -c $code *> $null; return $LASTEXITCODE -eq 0 }
  finally { Pop-Location }
}

function Find-BlenderExe {
  $cmd = Command-Path 'blender.exe' @()
  if ($cmd) { return $cmd }
  $roots = @(
    "$env:ProgramFiles\Blender Foundation",
    "${env:ProgramFiles(x86)}\Blender Foundation"
  ) | Where-Object { $_ -and (Test-Path $_) }
  foreach ($base in $roots) {
    $item = Get-ChildItem -Path $base -Filter blender.exe -File -Recurse -ErrorAction SilentlyContinue | Sort-Object FullName -Descending | Select-Object -First 1
    if ($item) { return $item.FullName }
  }
  return $null
}

function Ensure-BlenderVendor([string]$Python) {
  if ($SkipPhotoreal) {
    Write-Host '[ARCZ] Blender/Cycles ignorado por -SkipPhotoreal.' -ForegroundColor Yellow
    return
  }
  Step 'Blender/Cycles fotorreal local'
  if (Blender-VendorReady $Python) {
    Write-Host '[OK] vendor/blender já validado por SHA-256.'
    return
  }

  $blender = Find-BlenderExe
  if (-not $blender) {
    $installed = Winget-Install 'BlenderFoundation.Blender'
    if (-not $installed) { $installed = Winget-Install 'BlenderFoundation.Blender.LTS' }
    Refresh-Path
    $blender = Find-BlenderExe
  }
  if (-not $blender) {
    throw 'Blender não pôde ser instalado/localizado automaticamente; Fotorreal não será fingido como disponível.'
  }

  $blenderDir = Split-Path $blender -Parent
  $licenseNames = @('GPL3-license.txt','LICENSE','LICENSE.txt','COPYING','copyright.txt')
  $license = $null
  foreach ($name in $licenseNames) {
    $candidate = Join-Path $blenderDir $name
    if (Test-Path $candidate) { $license = $candidate; break }
  }
  if (-not $license) {
    $licenseItem = Get-ChildItem -Path $blenderDir -File -Recurse -ErrorAction SilentlyContinue |
      Where-Object { $_.Name -match '(?i)^(license|copying|copyright)' } |
      Select-Object -First 1
    if ($licenseItem) { $license = $licenseItem.FullName }
  }
  if (-not $license) { throw 'Distribuição Blender real encontrada, mas arquivo de licença não foi localizado; vendor recusado.' }

  Run $Python @((Join-Path $Root 'tools\vendor_blender.py'),'--source',$blenderDir,'--license-file',$license,'--force') | Out-Null
  if (-not (Blender-VendorReady $Python)) { throw 'Blender foi copiado, mas o gate de integridade recusou o vendor.' }
  Write-Host '[OK] Blender real copiado para vendor/blender e validado.'
}

function Build-Rust([string]$Cargo) {
  Push-Location $Root
  try {
    Run $Cargo @('+1.97.1','build','--release','--workspace','--locked') | Out-Null
  } finally { Pop-Location }
}

function Run-Validation([string]$Python, [string]$Node, [string]$Cargo) {
  Step 'Testes de regressão como gate de abertura'
  Push-Location $Root
  try {
    Run $Python @('-m','compileall','-q','arcz_server','tools') | Out-Null
    Run $Python @('-m','pytest','-q') | Out-Null

    $tests = @(Get-ChildItem -Path (Join-Path $Root 'tests_js\*.mjs') -File | ForEach-Object { $_.FullName })
    if ($tests.Count -gt 0) {
      $nodeArgs = @('--test','--experimental-default-type=module') + $tests
      Run $Node $nodeArgs | Out-Null
    }

    Run $Cargo @('+1.97.1','fmt','--all','--','--check') | Out-Null
    Run $Cargo @('+1.97.1','check','--locked','--workspace','--all-targets') | Out-Null
    Run $Cargo @('+1.97.1','test','--locked','--workspace') | Out-Null
    Run $Cargo @('+1.97.1','clippy','--locked','--workspace','--all-targets','--','-D','warnings') | Out-Null
  } finally { Pop-Location }
}

function Stop-PreviousServer {
  if (-not (Test-Path $ServerPidFile)) { return }
  $raw = (Get-Content $ServerPidFile -Raw -ErrorAction SilentlyContinue).Trim()
  $pidValue = 0
  if ([int]::TryParse($raw,[ref]$pidValue)) {
    $process = Get-Process -Id $pidValue -ErrorAction SilentlyContinue
    if ($process) {
      Write-Host "[ARCZ] Encerrando servidor anterior PID $pidValue."
      Stop-Process -Id $pidValue -Force -ErrorAction SilentlyContinue
      Start-Sleep -Milliseconds 400
    }
  }
  Remove-Item $ServerPidFile -Force -ErrorAction SilentlyContinue
}

function Start-ARCZ([string]$Python) {
  Step 'Abrindo ARCZ offline_strict'
  Stop-PreviousServer
  $env:ARCZ_NETWORK_MODE = 'offline_strict'
  $env:ARCZ_BANCO = $AssetBank
  $env:ARCZ_SEM_NAVEGADOR = '1'
  $stdout = Join-Path $LogDir 'server-out.log'
  $stderr = Join-Path $LogDir 'server-err.log'
  Remove-Item $stdout,$stderr -Force -ErrorAction SilentlyContinue

  $process = Start-Process -FilePath $Python -ArgumentList @('arcz_local.py','8123') -WorkingDirectory $Root -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru
  Set-Content -Path $ServerPidFile -Value $process.Id -Encoding ASCII

  $url = 'http://127.0.0.1:8123/api/v2/health'
  $ready = $false
  for ($i=0; $i -lt 90; $i++) {
    if ($process.HasExited) { break }
    try {
      $response = Invoke-WebRequest -UseBasicParsing -Uri $url -TimeoutSec 1
      if ($response.StatusCode -eq 200) { $ready = $true; break }
    } catch {}
    Start-Sleep -Milliseconds 500
  }
  if (-not $ready) {
    Write-Host '--- server stderr ---' -ForegroundColor Red
    if (Test-Path $stderr) { Get-Content $stderr -Tail 120 | Write-Host }
    throw 'Servidor ARCZ não ficou saudável em /api/v2/health.'
  }

  Write-Host '[OK] ARCZ saudável em http://127.0.0.1:8123/' -ForegroundColor Green
  if (-not $NoBrowser) { Start-Process 'http://127.0.0.1:8123/' }
}

try {
  Set-Location $Root
  Write-Host '============================================================'
  Write-Host ' ARCZ · ATUALIZAR → PREPARAR → TESTAR → ABRIR'
  Write-Host '============================================================'
  Write-Host "Repo: $Root"

  $git = Ensure-Git
  Update-Repository $git
  $head = (& $git -C $Root rev-parse HEAD).Trim()
  if (-not $head) { throw 'Não foi possível resolver o commit atual.' }
  $preparedHead = if (Test-Path $PreparedHeadFile) { (Get-Content $PreparedHeadFile -Raw).Trim() } else { '' }
  $verifiedHead = if (Test-Path $VerifiedHeadFile) { (Get-Content $VerifiedHeadFile -Raw).Trim() } else { '' }
  $changedForSetup = $ForceSetup -or ($preparedHead -ne $head)
  $changedForTests = $ForceSetup -or ($verifiedHead -ne $head)

  $bootstrapPython = Ensure-SystemPython
  $python = Ensure-Venv $bootstrapPython ($changedForSetup -or $changedForTests)
  $nodeInfo = Ensure-Node
  $node = $nodeInfo.Node
  $npm = $nodeInfo.Npm
  [void](Ensure-Bun $npm)
  $cargo = Ensure-Rust

  $env:ARCZ_BANCO = $AssetBank
  $env:ARCZ_NETWORK_MODE = 'offline_strict'

  $interactiveReady = Interactive-Preflight $python -Quiet
  if ($changedForSetup -or (-not $interactiveReady)) {
    Prepare-Interactive $python
    Set-Content -Path $PreparedHeadFile -Value $head -Encoding ASCII
  } else {
    Write-Host '[OK] Cesium/Aedifex já correspondem ao commit validado; build pesado não é repetido.'
  }

  Build-Rust $cargo
  Ensure-BlenderVendor $python

  if ($changedForTests) {
    Run-Validation $python $node $cargo
    Set-Content -Path $VerifiedHeadFile -Value $head -Encoding ASCII
  } else {
    Write-Host '[OK] Este commit já passou pela bateria local; preflight de abertura foi repetido.'
  }

  if (-not (Interactive-Preflight $python -Quiet)) {
    Get-Content (Join-Path $StateDir 'interactive-preflight.json') | Write-Host
    throw 'Gate interativo ficou vermelho imediatamente antes da abertura.'
  }

  Start-ARCZ $python
  Write-Host ''
  Write-Host "[DONE] Commit $head validado e aberto sem mock/fallback remoto." -ForegroundColor Green
  Stop-Transcript | Out-Null
  exit 0
} catch {
  Write-Host ''
  Write-Host ('[FALHA REAL] ' + $_.Exception.Message) -ForegroundColor Red
  Write-Host "Log: $LatestLog"
  try { Stop-Transcript | Out-Null } catch {}
  exit 1
}

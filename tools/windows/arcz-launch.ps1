param(
  [switch]$ForceSetup,
  [switch]$SkipUpdate,
  [switch]$NoBrowser,
  [switch]$SkipPhotoreal
)

$ErrorActionPreference = 'Stop'
$Root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))

function Find-Python {
  $candidates = @(
    'python.exe',
    "$env:LOCALAPPDATA\Programs\Python\Python312\python.exe",
    "$env:ProgramFiles\Python312\python.exe"
  )
  foreach ($candidate in $candidates) {
    try {
      $cmd = Get-Command $candidate -ErrorAction SilentlyContinue | Select-Object -First 1
      $path = if ($cmd) { $cmd.Source } elseif (Test-Path $candidate) { $candidate } else { $null }
      if (-not $path) { continue }
      & $path -c "import sys; raise SystemExit(0 if sys.version_info >= (3,11) else 7)"
      if ($LASTEXITCODE -eq 0) { return $path }
    } catch {}
  }
  return $null
}

$Python = Find-Python
if (-not $Python) {
  $Winget = Get-Command winget.exe -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $Winget) { throw 'Python 3.11+ is missing and winget is unavailable.' }
  & $Winget.Source install --id Python.Python.3.12 -e --silent --accept-package-agreements --accept-source-agreements --disable-interactivity
  if ($LASTEXITCODE -ne 0) { throw 'Automatic Python 3.12 installation failed.' }
  $env:Path = @(
    [Environment]::GetEnvironmentVariable('Path','Machine'),
    [Environment]::GetEnvironmentVariable('Path','User'),
    $env:Path
  ) -join ';'
  $Python = Find-Python
}
if (-not $Python) { throw 'Python 3.11+ could not be resolved after installation.' }

$ArgsList = @((Join-Path $Root 'tools\windows\arcz_launch.py'))
if ($ForceSetup) { $ArgsList += '--force-setup' }
if ($SkipUpdate) { $ArgsList += '--skip-update' }
if ($NoBrowser) { $ArgsList += '--no-browser' }
if ($SkipPhotoreal) { $ArgsList += '--skip-photoreal' }

& $Python @ArgsList
exit $LASTEXITCODE

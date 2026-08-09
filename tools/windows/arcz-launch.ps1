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

# A user may obtain ARCZ as a GitHub ZIP on the first run. Convert that source
# tree into a real, updatable checkout before the canonical controller runs.
# The Python helper commits the local ZIP snapshot to a backup branch first, so
# this operation never requires deleting the user's original tracked source.
if (-not (Test-Path (Join-Path $Root '.git'))) {
  Write-Host '[ARCZ] Source ZIP detected; adopting it into a real Git checkout...'
  & $Python (Join-Path $Root 'tools\windows\adopt_git_checkout.py')
  if ($LASTEXITCODE -ne 0) { throw 'Automatic Git adoption of the source ZIP failed.' }
}

$ArgsList = @((Join-Path $Root 'tools\windows\arcz_launch.py'))
if ($ForceSetup) { $ArgsList += '--force-setup' }
if ($SkipUpdate) { $ArgsList += '--skip-update' }
if ($NoBrowser) { $ArgsList += '--no-browser' }
if ($SkipPhotoreal) { $ArgsList += '--skip-photoreal' }

& $Python @ArgsList
exit $LASTEXITCODE

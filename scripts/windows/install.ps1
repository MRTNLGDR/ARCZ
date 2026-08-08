param(
    [string]$AedifexSource = "",
    [switch]$CloneAedifex,
    [string]$CesiumSource = "",
    [string]$CesiumLicense = "",
    [string]$BlenderPath = "",
    [switch]$ImportAssisted,
    [switch]$SkipPythonInstall,
    [switch]$SkipAedifexBuild
)
. (Join-Path $PSScriptRoot "common.ps1")
Set-Location $ProjectRoot

$systemPython = Resolve-ArczPython
$venv = Join-Path $ProjectRoot ".venv"
if (-not (Test-Path (Join-Path $venv "Scripts\python.exe"))) {
    & $systemPython -m venv $venv
}
$python = Join-Path $venv "Scripts\python.exe"

if (-not $SkipPythonInstall) {
    $wheelhouse = Join-Path $ProjectRoot "vendor\python\wheelhouse"
    if (Test-Path $wheelhouse) {
        & $python -m pip install --disable-pip-version-check --no-index --find-links $wheelhouse -r requirements.txt
    } elseif ($ImportAssisted) {
        & $python -m pip install --disable-pip-version-check -r requirements.txt
    } else {
        throw "Wheelhouse local ausente. Forneça vendor/python/wheelhouse ou use -ImportAssisted explicitamente."
    }
}

$env:ARCZ_NETWORK_MODE = if ($ImportAssisted) { "import_assisted" } else { "offline_strict" }
if ($CesiumSource) {
    if (-not $CesiumLicense) { throw "-CesiumLicense é obrigatório com -CesiumSource." }
    & $python tools/vendor_cesium.py --source $CesiumSource --license-file $CesiumLicense --version 1.143.0 --force
}
if ($AedifexSource) {
    & $python tools/vendor_aedifex.py --source $AedifexSource
} elseif ($CloneAedifex) {
    if (-not $ImportAssisted) { throw "-CloneAedifex exige -ImportAssisted." }
    & $python tools/vendor_aedifex.py --clone
}
if (-not $SkipAedifexBuild -and (Test-Path (Join-Path $ProjectRoot "opensources\forks\aedifex-arcz"))) {
    $buildArgs = @("tools/build_aedifex_sidecar.py")
    if ($ImportAssisted) { $buildArgs += "--allow-network" }
    & $python @buildArgs
}

$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if ($cargo) {
    & $cargo.Source build --release --workspace
} else {
    Write-Warning "Cargo ausente: geradores Rust permanecem bloqueados."
}

$envLines = @(
    "ARCZ_NETWORK_MODE=offline_strict",
    "ARCZ_PORT=8123",
    "ARCZ_SEM_NAVEGADOR=0"
)
if ($BlenderPath) {
    if (-not (Test-Path $BlenderPath)) { throw "Blender não encontrado: $BlenderPath" }
    $envLines += "ARCZ_BLENDER=$BlenderPath"
}
$envLines | Set-Content -Path (Join-Path $ProjectRoot ".env.local") -Encoding utf8

& $python tools/runtime_preflight.py --profile interactive --output logs/runtime-preflight.json
if ($LASTEXITCODE -ne 0) { throw "Preflight interativo falhou. Veja logs/runtime-preflight.json." }
Write-Host "Instalação local validada. Execute run.bat."

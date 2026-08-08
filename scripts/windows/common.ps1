Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProjectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$Logs = Join-Path $ProjectRoot "logs"
New-Item -ItemType Directory -Path $Logs -Force | Out-Null

function Resolve-ArczPython {
    $venv = Join-Path $ProjectRoot ".venv\Scripts\python.exe"
    if (Test-Path $venv) { return $venv }
    $python = Get-Command python -ErrorAction SilentlyContinue
    if ($python) { return $python.Source }
    $py = Get-Command py -ErrorAction SilentlyContinue
    if ($py) { return $py.Source }
    throw "Python 3.11+ não encontrado. Execute install.ps1."
}

function Import-ArczEnv {
    $envFile = Join-Path $ProjectRoot ".env.local"
    if (-not (Test-Path $envFile)) { return }
    foreach ($line in Get-Content $envFile) {
        $trimmed = $line.Trim()
        if (-not $trimmed -or $trimmed.StartsWith("#") -or -not $trimmed.Contains("=")) { continue }
        $parts = $trimmed.Split("=", 2)
        [Environment]::SetEnvironmentVariable($parts[0].Trim(), $parts[1].Trim(), "Process")
    }
}

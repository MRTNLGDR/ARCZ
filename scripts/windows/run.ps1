param([int]$Port = 8123, [switch]$Foreground)
. (Join-Path $PSScriptRoot "common.ps1")
Import-ArczEnv
Set-Location $ProjectRoot
$python = Resolve-ArczPython
& $python tools/runtime_preflight.py --profile interactive --output logs/runtime-preflight.json
if ($LASTEXITCODE -ne 0) { throw "ARCZ não iniciado: preflight falhou. Veja logs/runtime-preflight.json." }

$pidFile = Join-Path $Logs "arcz.pid"
if (Test-Path $pidFile) {
    $oldPid = [int](Get-Content $pidFile -Raw)
    if (Get-Process -Id $oldPid -ErrorAction SilentlyContinue) {
        Write-Host "ARCZ já está em execução (PID $oldPid)."
        exit 0
    }
    Remove-Item $pidFile -Force
}
$env:ARCZ_PORT = "$Port"
if ($Foreground) {
    & $python servidor.py $Port
    exit $LASTEXITCODE
}
$stdout = Join-Path $Logs "server.stdout.log"
$stderr = Join-Path $Logs "server.stderr.log"
$process = Start-Process -FilePath $python -ArgumentList @("servidor.py", "$Port") -WorkingDirectory $ProjectRoot -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru
$process.Id | Set-Content -Path $pidFile -Encoding ascii
Write-Host "ARCZ iniciado (PID $($process.Id)). Logs: $Logs"

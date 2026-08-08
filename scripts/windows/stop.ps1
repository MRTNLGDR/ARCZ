. (Join-Path $PSScriptRoot "common.ps1")
$pidFile = Join-Path $Logs "arcz.pid"
if (-not (Test-Path $pidFile)) { Write-Host "Nenhum PID ARCZ registrado."; exit 0 }
$pidValue = [int](Get-Content $pidFile -Raw)
$process = Get-Process -Id $pidValue -ErrorAction SilentlyContinue
if ($process) {
    & taskkill.exe /PID $pidValue /T /F | Out-Null
    Write-Host "ARCZ encerrado (PID $pidValue)."
} else {
    Write-Host "Processo $pidValue já não existe."
}
Remove-Item $pidFile -Force -ErrorAction SilentlyContinue

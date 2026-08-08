param([switch]$RemoveLocalData, [switch]$RemoveVendoredDependencies)
. (Join-Path $PSScriptRoot "common.ps1")
& (Join-Path $PSScriptRoot "stop.ps1")
$venv = Join-Path $ProjectRoot ".venv"
if (Test-Path $venv) { Remove-Item $venv -Recurse -Force }
Remove-Item (Join-Path $ProjectRoot ".env.local") -Force -ErrorAction SilentlyContinue
if ($RemoveVendoredDependencies) {
    foreach ($path in @("vendor\cesium", "vendor\aedifex-floorplanner", "opensources\upstream\aedifex", "opensources\forks\aedifex-arcz")) {
        $target = Join-Path $ProjectRoot $path
        if (Test-Path $target) { Remove-Item $target -Recurse -Force }
    }
}
if ($RemoveLocalData) {
    foreach ($path in @("data", "jobs", "logs", "cache", "cache_dem", "cache_entorno", "cache_geo", "cache_glb", "cache_overpass")) {
        $target = Join-Path $ProjectRoot $path
        if (Test-Path $target) { Remove-Item $target -Recurse -Force }
    }
}
Write-Host "Runtime local removido. Código-fonte preservado."

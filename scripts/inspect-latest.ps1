[CmdletBinding()]
param(
    [ValidateSet("current", "baseline", "diff", "all")]
    [string]$Scope = "current",
    [switch]$Open
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$uxRoot = Join-Path $repoRoot "artifacts\ux"

$paths = switch ($Scope) {
    "current" { @(Join-Path $uxRoot "current") }
    "baseline" { @(Join-Path $uxRoot "baseline") }
    "diff" { @(Join-Path $uxRoot "diff") }
    default { @(Join-Path $uxRoot "current"), (Join-Path $uxRoot "baseline"), (Join-Path $uxRoot "diff") }
}

$files = foreach ($path in $paths) {
    if (Test-Path $path) {
        Get-ChildItem -Path $path -File -Recurse
    }
}

if (-not $files) {
    Write-Host "No UX artifacts found under $uxRoot"
    return
}

$ordered = $files | Sort-Object LastWriteTimeUtc -Descending
$latestVisual = $ordered | Where-Object { $_.Extension -in ".png", ".gif" } | Select-Object -First 1

$ordered |
    Select-Object -First 12 Name, DirectoryName, Length, LastWriteTimeUtc |
    Format-Table -AutoSize

if ($latestVisual) {
    Write-Host ""
    Write-Host "Latest visual artifact: $($latestVisual.FullName)"
    $sidecarPath = [System.IO.Path]::ChangeExtension($latestVisual.FullName, ".json")
    if (Test-Path $sidecarPath) {
        Write-Host ""
        Get-Content -LiteralPath $sidecarPath
    }

    if ($Open) {
        Start-Process $latestVisual.FullName
    }
}

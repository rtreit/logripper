[CmdletBinding()]
param(
    [string]$Scenario = "main-window",
    [string]$Target = "MainWindow",
    [string]$Fixture,
    [ValidateSet("Default", "Dark", "Light")]
    [string]$Theme = "Dark",
    [string]$Output
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$currentRoot = Join-Path $repoRoot "artifacts\ux\current"
$baselineRoot = Join-Path $repoRoot "artifacts\ux\baseline"
$diffRoot = Join-Path $repoRoot "artifacts\ux\diff"

New-Item -ItemType Directory -Force -Path $currentRoot, $baselineRoot, $diffRoot | Out-Null

if ([string]::IsNullOrWhiteSpace($Output)) {
    $Output = Join-Path $currentRoot "$Scenario.png"
}

$Output = [System.IO.Path]::GetFullPath($Output)
$outputDirectory = Split-Path -Parent $Output
if (-not [string]::IsNullOrWhiteSpace($outputDirectory)) {
    New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
}

if ([string]::IsNullOrWhiteSpace($Fixture)) {
    $defaultFixture = Join-Path $PSScriptRoot "fixtures\ux-main-window.fixture.json"
    if (Test-Path $defaultFixture) {
        $Fixture = $defaultFixture
    }
}

$arguments = @(
    "run",
    "--project",
    "src\dotnet\QsoRipper.Gui\QsoRipper.Gui.csproj",
    "--",
    "--capture",
    "--capture-scenario", $Scenario,
    "--capture-target", $Target,
    "--capture-output", $Output,
    "--capture-theme", $Theme
)

if (-not [string]::IsNullOrWhiteSpace($Fixture)) {
    $arguments += @("--capture-fixture", (Resolve-Path -LiteralPath $Fixture))
}

Push-Location $repoRoot
try {
    & dotnet @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Avalonia capture failed with exit code $LASTEXITCODE."
    }

    $summaryPath = [System.IO.Path]::ChangeExtension($Output, ".json")
    Write-Host "Saved Avalonia capture to $Output"
    if (Test-Path $summaryPath) {
        Write-Host "Summary: $summaryPath"
    }
}
finally {
    Pop-Location
}

#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Build everything and bring up the full local QsoRipper stack from
    artifacts/publish.

.DESCRIPTION
    1. Builds Rust + .NET + Win32 via build.ps1 (skip with -SkipBuild).
    2. Force-restarts the Rust engine (50051) and .NET engine (50052) using
       start-qsoripper.ps1 so the GUI's engine selector sees both.
    3. Stops any running DebugHost / Avalonia GUI / CW Scope and relaunches
       them from artifacts/publish so they reflect the latest build.
    4. Opens http://localhost:5082 in the default browser for the DebugHost.

    Every binary is launched from the published artifact path (no `dotnet run`
    or `cargo run`) so what you exercise on screen is exactly what was built.

.PARAMETER Configuration
    Release (default) or Debug.

.PARAMETER SkipBuild
    Skip build.ps1 — just relaunch from existing artifacts.

.PARAMETER NoEngines
    Do not start the Rust / .NET engine servers.

.PARAMETER NoDebugHost
    Do not start the DebugHost web app.

.PARAMETER NoGui
    Do not start the main Avalonia GUI.

.PARAMETER NoCwScope
    Do not start the CW Scope GUI.

.EXAMPLE
    .\runall.ps1
    Build and launch every component.

.EXAMPLE
    .\runall.ps1 -SkipBuild
    Re-launch every component without rebuilding.

.EXAMPLE
    .\runall.ps1 -NoCwScope
    Skip the CW Scope GUI but bring up the rest.
#>

param(
    [ValidateSet('Release', 'Debug')]
    [string]$Configuration = 'Release',
    [switch]$SkipBuild,
    [switch]$NoEngines,
    [switch]$NoDebugHost,
    [switch]$NoGui,
    [switch]$NoCwScope
)

$ErrorActionPreference = 'Stop'

function Write-Step([string]$Message) {
    Write-Host "`n=== $Message ===" -ForegroundColor Cyan
}

function Stop-ProcessByName([string]$Name) {
    $procs = Get-Process -Name $Name -ErrorAction SilentlyContinue
    if (-not $procs) { return }
    foreach ($p in $procs) {
        try {
            Write-Host "  Stopping $($p.ProcessName) (PID $($p.Id))" -ForegroundColor Yellow
            Stop-Process -Id $p.Id -Force -ErrorAction Stop
        } catch {
            Write-Host "  Warning: failed to stop PID $($p.Id): $_" -ForegroundColor Yellow
        }
    }
    # Give the OS a moment to release file/socket handles.
    Start-Sleep -Milliseconds 500
}

# Names of every process that can hold a lock on a published artifact under
# artifacts/publish/. Killing these before invoking build.ps1 prevents
# Copy-Item failures of the form "the process cannot access the file ...
# because it is being used by another process" when an earlier runall left
# binaries running.
$ManagedProcessNames = @(
    'qsoripper-server',
    'QsoRipper.Engine.DotNet',
    'QsoRipper.DebugHost',
    'QsoRipper.Gui',
    'QsoRipper.Cli',
    'CwDecoderGui',
    'qsoripper-tui',
    'qsoripper-stress-tui',
    'qsoripper-win32'
)

function Stop-AllManagedProcesses {
    # Try to stop the engines politely first so they release their gRPC
    # listening sockets and SQLite handles, then force-kill anything that
    # is still alive (GUIs, leftover children, etc.).
    try {
        & "$PSScriptRoot\stop-qsoripper.ps1" -All | Out-Null
    } catch {
        Write-Host "  stop-qsoripper.ps1 reported: $_" -ForegroundColor Yellow
    }
    foreach ($name in $ManagedProcessNames) {
        Stop-ProcessByName $name
    }
}

function Start-Detached([string]$Path, [string[]]$Arguments, [string]$WorkingDirectory) {
    if (-not (Test-Path -LiteralPath $Path)) {
        throw "Cannot launch '$Path' — file not found. Did the build step run?"
    }
    $params = @{
        FilePath         = $Path
        WorkingDirectory = $WorkingDirectory
        PassThru         = $true
    }
    if ($Arguments -and $Arguments.Count -gt 0) {
        $params.ArgumentList = $Arguments
    }
    $proc = Start-Process @params
    Write-Host "  Launched $([System.IO.Path]::GetFileName($Path)) (PID $($proc.Id))" -ForegroundColor Green
    return $proc
}

# --- Resolve published artifact paths ----------------------------------------

$publishRoot              = Join-Path $PSScriptRoot 'artifacts' 'publish'
$serverExe                = Join-Path $publishRoot 'qsoripper-server'        | Join-Path -ChildPath $Configuration | Join-Path -ChildPath ($IsWindows ? 'qsoripper-server.exe' : 'qsoripper-server')
$dotnetEngineDir          = Join-Path $publishRoot 'qsoripper-engine-dotnet' | Join-Path -ChildPath $Configuration
$dotnetEngineExe          = Join-Path $dotnetEngineDir ($IsWindows ? 'QsoRipper.Engine.DotNet.exe' : 'QsoRipper.Engine.DotNet')
$debugHostDir             = Join-Path $publishRoot 'qsoripper-debughost'     | Join-Path -ChildPath $Configuration
$debugHostExe             = Join-Path $debugHostDir ($IsWindows ? 'QsoRipper.DebugHost.exe' : 'QsoRipper.DebugHost')
$guiDir                   = Join-Path $publishRoot 'qsoripper-gui'           | Join-Path -ChildPath $Configuration
$guiExe                   = Join-Path $guiDir ($IsWindows ? 'QsoRipper.Gui.exe' : 'QsoRipper.Gui')
$cwScopeDir               = Join-Path $publishRoot 'cw-decoder-gui'          | Join-Path -ChildPath $Configuration
$cwScopeExe               = Join-Path $cwScopeDir ($IsWindows ? 'CwDecoderGui.exe' : 'CwDecoderGui')

# --- Step 0: stop anything that could lock published artifacts ---------------

Write-Step 'Stopping any running QsoRipper processes (release artifact locks)'
Stop-AllManagedProcesses

# --- Step 1: build -----------------------------------------------------------

if (-not $SkipBuild) {
    Write-Step "Building all artifacts ($Configuration)"
    & "$PSScriptRoot\build.ps1" -Configuration $Configuration
    if ($LASTEXITCODE -ne 0) {
        throw "build.ps1 exited with $LASTEXITCODE"
    }
} else {
    Write-Step 'Skipping build (-SkipBuild)'
}

# --- Step 2: engines (force restart) -----------------------------------------

if (-not $NoEngines) {
    # Start the .NET engine first and the Rust engine LAST so the legacy
    # qsoripper-engine.json (used by the GUI as the default endpoint) ends
    # up pointing at the Rust engine, which fully loads the persisted
    # station_profile from config.toml. The .NET engine doesn't yet hydrate
    # station profiles from config.toml, so making it the default would
    # silently hide the F8 azimuthal map (origin lat/lon comes back empty).
    Write-Step 'Restarting .NET engine on 127.0.0.1:50052'
    & "$PSScriptRoot\start-qsoripper.ps1" -Engine local-dotnet -ForceRestart -SkipBuild
    if ($LASTEXITCODE -ne 0) { throw "start-qsoripper.ps1 (dotnet) exited with $LASTEXITCODE" }

    Write-Step 'Restarting Rust engine on 127.0.0.1:50051 (default for GUI)'
    & "$PSScriptRoot\start-qsoripper.ps1" -Engine local-rust -ForceRestart -SkipBuild
    if ($LASTEXITCODE -ne 0) { throw "start-qsoripper.ps1 (rust) exited with $LASTEXITCODE" }
} else {
    Write-Step 'Skipping engine startup (-NoEngines)'
}

# --- Step 3: DebugHost (force restart, launch from artifacts) ----------------

if (-not $NoDebugHost) {
    Write-Step 'Restarting DebugHost on http://localhost:5082'
    Stop-ProcessByName 'QsoRipper.DebugHost'
    $null = Start-Detached -Path $debugHostExe -WorkingDirectory $debugHostDir -Arguments @('--urls', 'http://localhost:5082')
    Start-Sleep -Seconds 2
    try {
        Start-Process 'http://localhost:5082'
    } catch {
        Write-Host "  Could not auto-open browser: $_" -ForegroundColor Yellow
    }
} else {
    Write-Step 'Skipping DebugHost (-NoDebugHost)'
}

# --- Step 4: main GUI --------------------------------------------------------

if (-not $NoGui) {
    Write-Step 'Restarting QsoRipper GUI'
    Stop-ProcessByName 'QsoRipper.Gui'
    $null = Start-Detached -Path $guiExe -WorkingDirectory $guiDir
} else {
    Write-Step 'Skipping main GUI (-NoGui)'
}

# --- Step 5: CW Scope --------------------------------------------------------

if (-not $NoCwScope) {
    Write-Step 'Restarting CW Scope GUI'
    Stop-ProcessByName 'CwDecoderGui'
    if (Test-Path -LiteralPath $cwScopeExe) {
        $null = Start-Detached -Path $cwScopeExe -WorkingDirectory $cwScopeDir
    } else {
        Write-Host "  CW Scope artifact not found at $cwScopeExe — skipping (was the build skipped?)" -ForegroundColor Yellow
    }
} else {
    Write-Step 'Skipping CW Scope (-NoCwScope)'
}

Write-Step 'All requested components launched'
Write-Host @"
Endpoints:
  Rust engine        http://127.0.0.1:50051
  .NET engine        http://127.0.0.1:50052
  DebugHost          http://localhost:5082

Use .\stop-qsoripper.ps1 to stop the engines.
GUI / DebugHost / CW Scope can be closed via their windows or Stop-Process.
"@ -ForegroundColor Cyan

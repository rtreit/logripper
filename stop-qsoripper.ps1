#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Stops background QsoRipper engine(s) started by start-qsoripper.ps1.

.DESCRIPTION
    With -Engine, stops only that engine profile. Without -Engine, stops all
    running engine profiles found under artifacts/run.
#>

param(
    [string]$Engine,
    [int]$TimeoutSeconds = 15
)

$ErrorActionPreference = 'Stop'

$runtimeDirectory = Join-Path $PSScriptRoot 'artifacts' | Join-Path -ChildPath 'run'
$legacyStatePath = Join-Path $runtimeDirectory 'qsoripper-engine.json'

function Get-State([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) {
        return $null
    }

    return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}

function Stop-EngineFromState([string]$StatePath) {
    $state = Get-State -Path $StatePath
    if ($null -eq $state) {
        return
    }

    $process = Get-Process -Id $state.pid -ErrorAction SilentlyContinue
    if ($null -eq $process) {
        Remove-Item -LiteralPath $StatePath -Force -ErrorAction SilentlyContinue
        Write-Host 'Removed stale QsoRipper state file.' -ForegroundColor Yellow
        return
    }

    Stop-Process -Id $process.Id

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 200
        if ($null -eq (Get-Process -Id $process.Id -ErrorAction SilentlyContinue)) {
            Remove-Item -LiteralPath $StatePath -Force -ErrorAction SilentlyContinue
            $engineLabel = if (-not [string]::IsNullOrWhiteSpace($state.displayName)) {
                $state.displayName
            }
            elseif (-not [string]::IsNullOrWhiteSpace($state.engine)) {
                "$($state.engine) engine"
            }
            else {
                'engine'
            }
            Write-Host "Stopped $engineLabel (PID $($process.Id))." -ForegroundColor Green
            return
        }
    }

    throw "Timed out waiting for QsoRipper process $($process.Id) to stop."
}

if (-not [string]::IsNullOrWhiteSpace($Engine)) {
    # Stop a specific engine profile
    $statePath = Join-Path $runtimeDirectory "qsoripper-$Engine.state.json"
    if (-not (Test-Path -LiteralPath $statePath)) {
        Write-Host "No running $Engine engine found." -ForegroundColor Yellow
        exit 0
    }

    Stop-EngineFromState -StatePath $statePath
}
else {
    # Stop all running engines (per-profile state files + legacy single file)
    $stateFiles = @()
    if (Test-Path -LiteralPath $runtimeDirectory) {
        $stateFiles = @(Get-ChildItem -LiteralPath $runtimeDirectory -Filter 'qsoripper-*.state.json' -File)
    }

    # Also check legacy single-engine state file
    if (Test-Path -LiteralPath $legacyStatePath) {
        $stateFiles += @(Get-Item -LiteralPath $legacyStatePath)
    }

    if ($stateFiles.Count -eq 0) {
        Write-Host 'QsoRipper is not running through the helper script.' -ForegroundColor Yellow
        exit 0
    }

    foreach ($stateFile in $stateFiles) {
        Stop-EngineFromState -StatePath $stateFile.FullName
    }
}

Write-Host "Logs retained under $runtimeDirectory." -ForegroundColor Green

#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Stops a background QsoRipper engine started by start-qsoripper.ps1.
#>

param(
    [int]$TimeoutSeconds = 15
)

$ErrorActionPreference = 'Stop'

$runtimeDirectory = Join-Path $PSScriptRoot 'artifacts' | Join-Path -ChildPath 'run'
$statePath = Join-Path $runtimeDirectory 'qsoripper-engine.json'

function Get-State {
    if (-not (Test-Path -LiteralPath $statePath)) {
        return $null
    }

    return Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json
}

$state = Get-State
if ($null -eq $state) {
    Write-Host 'QsoRipper is not running through the helper script.' -ForegroundColor Yellow
    exit 0
}

$process = Get-Process -Id $state.pid -ErrorAction SilentlyContinue
if ($null -eq $process) {
    Remove-Item -LiteralPath $statePath -Force -ErrorAction SilentlyContinue
    Write-Host 'Removed stale QsoRipper state file.' -ForegroundColor Yellow
    exit 0
}

Stop-Process -Id $process.Id

$deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
while ([DateTime]::UtcNow -lt $deadline) {
    Start-Sleep -Milliseconds 200
    if ($null -eq (Get-Process -Id $process.Id -ErrorAction SilentlyContinue)) {
        Remove-Item -LiteralPath $statePath -Force -ErrorAction SilentlyContinue
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
        Write-Host "Logs retained under $runtimeDirectory." -ForegroundColor Green
        exit 0
    }
}

throw "Timed out waiting for QsoRipper process $($process.Id) to stop."

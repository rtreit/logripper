#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Stops background QsoRipper engine processes started by start-qsoripper.ps1.
#>

param(
    [string]$Engine,
    [switch]$All,
    [int]$TimeoutSeconds = 15
)

$ErrorActionPreference = 'Stop'

$runtimeDirectory = Join-Path $PSScriptRoot 'artifacts' | Join-Path -ChildPath 'run'
$legacyStatePath = Join-Path $runtimeDirectory 'qsoripper-engine.json'
$stateGlob = 'qsoripper-engine-*.json'

function Get-StateFromPath([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path -LiteralPath $Path)) {
        return $null
    }

    try {
        return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    }
    catch {
        return $null
    }
}

function Get-StateEntries {
    $entries = [System.Collections.Generic.List[pscustomobject]]::new()

    if (Test-Path -LiteralPath $legacyStatePath) {
        $legacyState = Get-StateFromPath -Path $legacyStatePath
        if ($null -ne $legacyState) {
            $entries.Add([pscustomobject]@{
                    Path = $legacyStatePath
                    IsLegacy = $true
                    State = $legacyState
                })
        }
    }

    if (Test-Path -LiteralPath $runtimeDirectory) {
        foreach ($stateFile in Get-ChildItem -LiteralPath $runtimeDirectory -Filter $stateGlob -File -ErrorAction SilentlyContinue) {
            $state = Get-StateFromPath -Path $stateFile.FullName
            if ($null -eq $state) {
                continue
            }

            $entries.Add([pscustomobject]@{
                    Path = $stateFile.FullName
                    IsLegacy = $false
                    State = $state
                })
        }
    }

    return @(
        $entries |
            Sort-Object Path -Unique
    )
}

function Test-EngineMatch([pscustomobject]$State, [string]$RequestedEngine) {
    if ([string]::IsNullOrWhiteSpace($RequestedEngine) -or $null -eq $State) {
        return $false
    }

    if (
        $RequestedEngine -ieq $State.engine -or
        $RequestedEngine -ieq $State.engineId -or
        $RequestedEngine -ieq $State.displayName
    ) {
        return $true
    }

    switch ($RequestedEngine.ToLowerInvariant()) {
        'rust' {
            return (
                (-not [string]::IsNullOrWhiteSpace($State.engine) -and $State.engine.Contains('rust', [System.StringComparison]::OrdinalIgnoreCase)) -or
                (-not [string]::IsNullOrWhiteSpace($State.engineId) -and $State.engineId.Contains('rust', [System.StringComparison]::OrdinalIgnoreCase))
            )
        }
        'dotnet' {
            return (
                (-not [string]::IsNullOrWhiteSpace($State.engine) -and $State.engine.Contains('dotnet', [System.StringComparison]::OrdinalIgnoreCase)) -or
                (-not [string]::IsNullOrWhiteSpace($State.engineId) -and $State.engineId.Contains('dotnet', [System.StringComparison]::OrdinalIgnoreCase))
            )
        }
        'managed' {
            return (
                (-not [string]::IsNullOrWhiteSpace($State.engine) -and $State.engine.Contains('dotnet', [System.StringComparison]::OrdinalIgnoreCase)) -or
                (-not [string]::IsNullOrWhiteSpace($State.engineId) -and $State.engineId.Contains('dotnet', [System.StringComparison]::OrdinalIgnoreCase))
            )
        }
        default {
            return $false
        }
    }
}

function Wait-ForProcessStop([int]$ProcessId, [int]$TimeoutSeconds) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 200
        if ($null -eq (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)) {
            return $true
        }
    }

    return $false
}

function Resolve-Targets([pscustomobject[]]$Entries) {
    $normalizedEntries = @($Entries)

    if ($normalizedEntries.Count -eq 0) {
        return @()
    }

    if ($All) {
        return $normalizedEntries
    }

    if (-not [string]::IsNullOrWhiteSpace($Engine)) {
        return @(
            $normalizedEntries |
                Where-Object { Test-EngineMatch -State $_.State -RequestedEngine $Engine }
        )
    }

    $legacyEntry = $normalizedEntries | Where-Object { $_.IsLegacy } | Select-Object -First 1
    if ($null -ne $legacyEntry) {
        return @($legacyEntry)
    }

    $latestEntry = $normalizedEntries |
        Sort-Object {
            try {
                [DateTime]::Parse($_.State.startedAtUtc, [System.Globalization.CultureInfo]::InvariantCulture, [System.Globalization.DateTimeStyles]::RoundtripKind)
            }
            catch {
                [DateTime]::MinValue
            }
        } -Descending |
        Select-Object -First 1

    if ($null -ne $latestEntry) {
        return @($latestEntry)
    }

    return @()
}

$stateEntries = @(Get-StateEntries)
if ($stateEntries.Count -eq 0) {
    Write-Host 'QsoRipper is not running through the helper script.' -ForegroundColor Yellow
    exit 0
}

$targetEntries = @(Resolve-Targets -Entries $stateEntries)
if ($targetEntries.Count -eq 0) {
    if (-not [string]::IsNullOrWhiteSpace($Engine)) {
        Write-Host "No tracked engine state matched '$Engine'." -ForegroundColor Yellow
        exit 0
    }

    Write-Host 'No tracked engine state selected.' -ForegroundColor Yellow
    exit 0
}

$targetProcessIds = @(
    $targetEntries |
        Select-Object -ExpandProperty State |
        Select-Object -ExpandProperty pid -Unique
)

foreach ($processId in $targetProcessIds) {
    $process = Get-Process -Id $processId -ErrorAction SilentlyContinue
    $entriesForProcess = @($stateEntries | Where-Object { [int]$_.State.pid -eq [int]$processId })
    $labelSource = $entriesForProcess | Select-Object -First 1
    $engineLabel = if ($null -ne $labelSource -and -not [string]::IsNullOrWhiteSpace($labelSource.State.displayName)) {
        $labelSource.State.displayName
    }
    elseif ($null -ne $labelSource -and -not [string]::IsNullOrWhiteSpace($labelSource.State.engine)) {
        "$($labelSource.State.engine) engine"
    }
    else {
        'engine'
    }

    if ($null -eq $process) {
        foreach ($entry in $entriesForProcess) {
            Remove-Item -LiteralPath $entry.Path -Force -ErrorAction SilentlyContinue
        }
        Write-Host "Removed stale state for $engineLabel (PID $processId)." -ForegroundColor Yellow
        continue
    }

    Stop-Process -Id $process.Id
    if (-not (Wait-ForProcessStop -ProcessId $process.Id -TimeoutSeconds $TimeoutSeconds)) {
        throw "Timed out waiting for QsoRipper process $($process.Id) to stop."
    }

    foreach ($entry in $entriesForProcess) {
        Remove-Item -LiteralPath $entry.Path -Force -ErrorAction SilentlyContinue
    }
    Write-Host "Stopped $engineLabel (PID $($process.Id))." -ForegroundColor Green
}

Write-Host "Logs retained under $runtimeDirectory." -ForegroundColor Green

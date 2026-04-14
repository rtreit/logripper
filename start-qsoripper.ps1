#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Starts the QsoRipper Rust engine in the background.

.DESCRIPTION
    Builds qsoripper-server if needed, launches it as a background process,
    and records its PID and log paths under artifacts/run.
#>

param(
    [string]$ListenAddress = '127.0.0.1:50051',
    [ValidateSet('sqlite', 'memory')]
    [string]$Storage = 'sqlite',
    [string]$SqlitePath,
    [string]$ConfigPath,
    [int]$StartupTimeoutSeconds = 30,
    [switch]$SkipBuild,
    [switch]$ForceRestart
)

$ErrorActionPreference = 'Stop'

$runtimeDirectory = Join-Path $PSScriptRoot 'artifacts' | Join-Path -ChildPath 'run'
$statePath = Join-Path $runtimeDirectory 'qsoripper-server.json'
$stdoutPath = Join-Path $runtimeDirectory 'qsoripper-server.stdout.log'
$stderrPath = Join-Path $runtimeDirectory 'qsoripper-server.stderr.log'
$dotenvPath = Join-Path $PSScriptRoot '.env'
$rustManifestPath = Join-Path $PSScriptRoot 'src' | Join-Path -ChildPath 'rust' | Join-Path -ChildPath 'Cargo.toml'
$binaryName = if ($IsWindows) { 'qsoripper-server.exe' } else { 'qsoripper-server' }
$serverBinaryPath = Join-Path $PSScriptRoot 'src' | Join-Path -ChildPath 'rust' | Join-Path -ChildPath 'target' | Join-Path -ChildPath 'debug' | Join-Path -ChildPath $binaryName

function Write-Info([string]$Message) {
    Write-Host $Message -ForegroundColor Cyan
}

function Import-DotEnv([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) {
        return
    }

    foreach ($line in Get-Content -LiteralPath $Path) {
        $trimmed = $line.Trim()
        if ([string]::IsNullOrWhiteSpace($trimmed) -or $trimmed.StartsWith('#')) {
            continue
        }

        $parts = $line -split '=', 2
        if ($parts.Count -ne 2) {
            continue
        }

        $name = $parts[0].Trim()
        $value = $parts[1].Trim()
        if (
            ($value.StartsWith('"') -and $value.EndsWith('"')) -or
            ($value.StartsWith("'") -and $value.EndsWith("'"))
        ) {
            $value = $value.Substring(1, $value.Length - 2)
        }

        Set-Item -Path "Env:$name" -Value $value
    }
}

function Get-State {
    if (-not (Test-Path -LiteralPath $statePath)) {
        return $null
    }

    return Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json
}

function Get-TrackedProcess {
    $state = Get-State
    if ($null -eq $state) {
        return $null
    }

    $process = Get-Process -Id $state.pid -ErrorAction SilentlyContinue
    if ($null -eq $process) {
        Remove-Item -LiteralPath $statePath -Force -ErrorAction SilentlyContinue
        return $null
    }

    [pscustomobject]@{
        State = $state
        Process = $process
    }
}

function Get-ProbeTarget([string]$Address) {
    if ($Address -match '^\[(?<host>.+)\]:(?<port>\d+)$') {
        $probeHost = $Matches.host
        $probePort = [int]$Matches.port
    }
    elseif ($Address -match '^(?<host>[^:]+):(?<port>\d+)$') {
        $probeHost = $Matches.host
        $probePort = [int]$Matches.port
    }
    else {
        throw "Unsupported listen address format: $Address"
    }

    if ($probeHost -in @('0.0.0.0', '::', '[::]', '*', '+')) {
        $probeHost = '127.0.0.1'
    }

    [pscustomobject]@{
        Host = $probeHost
        Port = $probePort
    }
}

function Test-TcpEndpoint([string]$TargetHost, [int]$Port) {
    $client = [System.Net.Sockets.TcpClient]::new()
    try {
        $connectTask = $client.ConnectAsync($TargetHost, $Port)
        if (-not $connectTask.Wait([TimeSpan]::FromMilliseconds(500))) {
            return $false
        }

        return $client.Connected
    }
    catch {
        return $false
    }
    finally {
        $client.Dispose()
    }
}

function Get-LogTail([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) {
        return @()
    }

    return Get-Content -LiteralPath $Path -Tail 20
}

function Stop-TrackedProcess([int]$ProcessId) {
    $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if ($null -eq $process) {
        return
    }

    Stop-Process -Id $ProcessId

    for ($attempt = 0; $attempt -lt 50; $attempt++) {
        Start-Sleep -Milliseconds 200
        if ($null -eq (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)) {
            return
        }
    }

    throw "Timed out waiting for process $ProcessId to stop."
}

New-Item -ItemType Directory -Path $runtimeDirectory -Force | Out-Null
Import-DotEnv -Path $dotenvPath

$existing = Get-TrackedProcess
if ($null -ne $existing) {
    if (-not $ForceRestart) {
        Write-Host "QsoRipper is already running (PID $($existing.Process.Id)) at $($existing.State.listenAddress)." -ForegroundColor Yellow
        Write-Host "Stop it first with .\stop-qsoripper.ps1 or rerun with -ForceRestart." -ForegroundColor Yellow
        exit 0
    }

    Write-Info "Stopping existing QsoRipper process $($existing.Process.Id)."
    Stop-TrackedProcess -ProcessId $existing.Process.Id
    Remove-Item -LiteralPath $statePath -Force -ErrorAction SilentlyContinue
}

if (-not $SkipBuild) {
    Write-Info 'Building qsoripper-server.'
    cargo build --manifest-path $rustManifestPath -p qsoripper-server
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE."
    }
}

if (-not (Test-Path -LiteralPath $serverBinaryPath)) {
    throw "Server binary not found at $serverBinaryPath."
}

$argumentList = @(
    '--listen', $ListenAddress,
    '--storage', $Storage
)

if ($Storage -eq 'sqlite' -and -not [string]::IsNullOrWhiteSpace($SqlitePath)) {
    $argumentList += @('--sqlite-path', $SqlitePath)
}

if (-not [string]::IsNullOrWhiteSpace($ConfigPath)) {
    $argumentList += @('--config', $ConfigPath)
}

Write-Info "Starting QsoRipper on $ListenAddress using $Storage storage."
$startProcessParameters = @{
    FilePath = $serverBinaryPath
    ArgumentList = $argumentList
    WorkingDirectory = $PSScriptRoot
    RedirectStandardOutput = $stdoutPath
    RedirectStandardError = $stderrPath
    PassThru = $true
}

if ($IsWindows) {
    $startProcessParameters.WindowStyle = 'Hidden'
}

$process = Start-Process @startProcessParameters

$state = [pscustomobject]@{
    pid = $process.Id
    listenAddress = $ListenAddress
    storage = $Storage
    sqlitePath = if ($Storage -eq 'sqlite') { $SqlitePath } else { $null }
    configPath = if ([string]::IsNullOrWhiteSpace($ConfigPath)) { $null } else { $ConfigPath }
    startedAtUtc = [DateTime]::UtcNow.ToString('O')
    stdoutPath = $stdoutPath
    stderrPath = $stderrPath
}
$state | ConvertTo-Json | Set-Content -LiteralPath $statePath

$probeTarget = Get-ProbeTarget -Address $ListenAddress
$deadline = [DateTime]::UtcNow.AddSeconds($StartupTimeoutSeconds)

while ([DateTime]::UtcNow -lt $deadline) {
    $runningProcess = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
    if ($null -eq $runningProcess) {
        $stderrTail = Get-LogTail -Path $stderrPath
        $stdoutTail = Get-LogTail -Path $stdoutPath
        $details = @($stderrTail + $stdoutTail) -join [Environment]::NewLine
        throw "QsoRipper exited during startup.`n$details"
    }

    if (Test-TcpEndpoint -TargetHost $probeTarget.Host -Port $probeTarget.Port) {
        Write-Host "QsoRipper started in the background (PID $($process.Id))." -ForegroundColor Green
        Write-Host "Endpoint: http://$($probeTarget.Host):$($probeTarget.Port)" -ForegroundColor Green
        if ($Storage -eq 'sqlite' -and -not [string]::IsNullOrWhiteSpace($SqlitePath)) {
            Write-Host "SQLite file: $SqlitePath" -ForegroundColor Green
        }
        Write-Host "Logs: $stdoutPath" -ForegroundColor Green
        exit 0
    }

    Start-Sleep -Milliseconds 250
}

Stop-TrackedProcess -Pid $process.Id
Remove-Item -LiteralPath $statePath -Force -ErrorAction SilentlyContinue
throw "QsoRipper did not open $($probeTarget.Host):$($probeTarget.Port) within $StartupTimeoutSeconds seconds."

#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Starts the selected QsoRipper engine profile in the background.

.DESCRIPTION
    Builds the selected engine if needed, launches it as a background process,
    and records its PID and log paths under artifacts/run.
#>

param(
    [string]$Engine,
    [string]$ListenAddress,
    [string]$Storage,
    [Alias('SqlitePath')]
    [string]$PersistenceLocation,
    [string]$ConfigPath,
    [int]$StartupTimeoutSeconds = 30,
    [switch]$SkipBuild,
    [switch]$ForceRestart
)

$ErrorActionPreference = 'Stop'

$runtimeDirectory = Join-Path $PSScriptRoot 'artifacts' | Join-Path -ChildPath 'run'
$statePath = Join-Path $runtimeDirectory 'qsoripper-engine.json'
$dotenvPath = Join-Path $PSScriptRoot '.env'

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

function Resolve-TemplateValue([string]$Template, [hashtable]$Tokens) {
    if ([string]::IsNullOrWhiteSpace($Template)) {
        return ''
    }

    $resolved = $Template
    foreach ($token in $Tokens.GetEnumerator()) {
        $resolved = $resolved.Replace("{$($token.Key)}", [string]$token.Value)
    }

    return $resolved
}

function Resolve-TemplateList([string[]]$Templates, [hashtable]$Tokens) {
    $values = @()
    foreach ($template in $Templates) {
        $resolved = Resolve-TemplateValue -Template $template -Tokens $Tokens
        if (-not [string]::IsNullOrWhiteSpace($resolved)) {
            $values += $resolved
        }
    }

    return $values
}

function Invoke-WithTemporaryEnvironment([hashtable]$EnvironmentOverrides, [scriptblock]$Action) {
    $originalValues = @{}
    try {
        foreach ($entry in $EnvironmentOverrides.GetEnumerator()) {
            $name = $entry.Key
            $existing = [System.Environment]::GetEnvironmentVariable($name)
            $originalValues[$name] = $existing

            if ([string]::IsNullOrWhiteSpace($entry.Value)) {
                Remove-Item -Path "Env:$name" -ErrorAction SilentlyContinue
            }
            else {
                Set-Item -Path "Env:$name" -Value $entry.Value
            }
        }

        & $Action
    }
    finally {
        foreach ($entry in $originalValues.GetEnumerator()) {
            if ($null -eq $entry.Value) {
                Remove-Item -Path "Env:$($entry.Key)" -ErrorAction SilentlyContinue
            }
            else {
                Set-Item -Path "Env:$($entry.Key)" -Value $entry.Value
            }
        }
    }
}

function Get-EngineProfiles {
    $rustManifestPath = Join-Path $PSScriptRoot 'src' | Join-Path -ChildPath 'rust' | Join-Path -ChildPath 'Cargo.toml'
    $dotnetProjectPath = Join-Path $PSScriptRoot 'src' | Join-Path -ChildPath 'dotnet' | Join-Path -ChildPath 'QsoRipper.Engine.DotNet' | Join-Path -ChildPath 'QsoRipper.Engine.DotNet.csproj'
    $dotnetDebugDllPath = Join-Path $PSScriptRoot 'src' | Join-Path -ChildPath 'dotnet' | Join-Path -ChildPath 'QsoRipper.Engine.DotNet' | Join-Path -ChildPath 'bin' | Join-Path -ChildPath 'Debug' | Join-Path -ChildPath 'net10.0' | Join-Path -ChildPath 'QsoRipper.Engine.DotNet.dll'
    $dotnetReleaseDllPath = Join-Path $PSScriptRoot 'src' | Join-Path -ChildPath 'dotnet' | Join-Path -ChildPath 'QsoRipper.Engine.DotNet' | Join-Path -ChildPath 'bin' | Join-Path -ChildPath 'Release' | Join-Path -ChildPath 'net10.0' | Join-Path -ChildPath 'QsoRipper.Engine.DotNet.dll'
    $dotnetDllPath = if (Test-Path -LiteralPath $dotnetDebugDllPath) {
        $dotnetDebugDllPath
    }
    elseif (Test-Path -LiteralPath $dotnetReleaseDllPath) {
        $dotnetReleaseDllPath
    }
    else {
        $dotnetDebugDllPath
    }
    $binaryName = if ($IsWindows) { 'qsoripper-server.exe' } else { 'qsoripper-server' }
    $rustBinaryPath = Join-Path $PSScriptRoot 'src' | Join-Path -ChildPath 'rust' | Join-Path -ChildPath 'target' | Join-Path -ChildPath 'debug' | Join-Path -ChildPath $binaryName

    return @(
        [pscustomobject]@{
            ProfileId = 'local-rust'
            EngineId = 'rust-tonic'
            DisplayName = 'QsoRipper Rust Engine'
            Aliases = @('local-rust', 'rust', 'rust-tonic')
            DefaultListenAddress = '127.0.0.1:50051'
            DefaultStorage = 'sqlite'
            DefaultPersistenceLocation = '.\data\qsoripper.db'
            DefaultConfigPath = Join-Path $runtimeDirectory 'rust-engine.json'
            EnvironmentTemplates = @{
                QSORIPPER_STORAGE_BACKEND = '{storageBackend}'
                QSORIPPER_SQLITE_PATH = '{persistenceLocation}'
            }
            BuildFilePath = 'cargo'
            BuildArguments = @('build', '--manifest-path', $rustManifestPath, '-p', 'qsoripper-server')
            LaunchFilePath = $rustBinaryPath
            LaunchArguments = @('--listen', '{listenAddress}', '--config', '{configPath}')
            SupportsStorageSession = $true
        },
        [pscustomobject]@{
            ProfileId = 'local-dotnet'
            EngineId = 'dotnet-aspnet'
            DisplayName = 'QsoRipper .NET Engine'
            Aliases = @('local-dotnet', 'dotnet', 'dotnet-aspnet', 'managed')
            DefaultListenAddress = '127.0.0.1:50052'
            DefaultStorage = 'memory'
            DefaultPersistenceLocation = '.\data\qsoripper.db'
            DefaultConfigPath = Join-Path $runtimeDirectory 'dotnet-engine.json'
            EnvironmentTemplates = @{}
            BuildFilePath = 'dotnet'
            BuildArguments = @('build', $dotnetProjectPath, '-c', 'Debug')
            LaunchFilePath = 'dotnet'
            LaunchArguments = @(
                $dotnetDllPath,
                '--listen',
                '{listenAddress}',
                '--config',
                '{configPath}'
            )
            SupportsStorageSession = $false
        }
    )
}

function Resolve-EngineProfile([string]$RequestedEngine, [object[]]$Profiles) {
    foreach ($profile in $Profiles) {
        if (
            $RequestedEngine -ieq $profile.ProfileId -or
            $RequestedEngine -ieq $profile.EngineId -or
            ($profile.Aliases | Where-Object { $_ -ieq $RequestedEngine })
        ) {
            return $profile
        }
    }

    $knownProfiles = $Profiles |
        ForEach-Object { @($_.ProfileId) + $_.Aliases } |
        Select-Object -Unique

    throw "Unknown engine profile '$RequestedEngine'. Known values: $($knownProfiles -join ', ')."
}

New-Item -ItemType Directory -Path $runtimeDirectory -Force | Out-Null
Import-DotEnv -Path $dotenvPath

if ([string]::IsNullOrWhiteSpace($Engine)) {
    $Engine = if ([string]::IsNullOrWhiteSpace($env:QSORIPPER_ENGINE)) {
        'rust'
    }
    else {
        $env:QSORIPPER_ENGINE
    }
}

$profiles = Get-EngineProfiles
$profile = Resolve-EngineProfile -RequestedEngine $Engine -Profiles $profiles
$stdoutPath = Join-Path $runtimeDirectory "qsoripper-$($profile.ProfileId).stdout.log"
$stderrPath = Join-Path $runtimeDirectory "qsoripper-$($profile.ProfileId).stderr.log"

if ([string]::IsNullOrWhiteSpace($ListenAddress)) {
    $ListenAddress = $profile.DefaultListenAddress
}

if ([string]::IsNullOrWhiteSpace($Storage)) {
    $Storage = $profile.DefaultStorage
}

if (-not $profile.SupportsStorageSession -and $Storage -ne $profile.DefaultStorage) {
    throw "$($profile.DisplayName) only supports its default storage backend '$($profile.DefaultStorage)' through the launcher helper."
}

if ([string]::IsNullOrWhiteSpace($PersistenceLocation)) {
    $PersistenceLocation = $profile.DefaultPersistenceLocation
}

if ([string]::IsNullOrWhiteSpace($ConfigPath)) {
    $ConfigPath = $profile.DefaultConfigPath
}

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

if ($ForceRestart) {
    # Kill any untracked qsoripper-server processes (e.g. started outside this script)
    $orphans = Get-Process -Name 'qsoripper-server' -ErrorAction SilentlyContinue
    foreach ($orphan in $orphans) {
        Write-Info "Stopping untracked qsoripper-server process $($orphan.Id)."
        Stop-TrackedProcess -ProcessId $orphan.Id
    }

    # On Windows the OS may briefly hold a file lock after process exit; wait for the
    # binary to become writable before starting the build.
    if ((Test-Path -LiteralPath $serverBinaryPath) -and $orphans) {
        for ($lockAttempt = 0; $lockAttempt -lt 20; $lockAttempt++) {
            try {
                [System.IO.File]::Open($serverBinaryPath, 'Open', 'ReadWrite', 'None').Dispose()
                break
            }
            catch {
                Start-Sleep -Milliseconds 250
            }
        }
    }
}

if (-not $SkipBuild) {
    Write-Info "Building $($profile.DisplayName)."
    & $profile.BuildFilePath @($profile.BuildArguments)
    if ($LASTEXITCODE -ne 0) {
        throw "Build failed with exit code $LASTEXITCODE."
    }
}

$tokens = @{
    configPath = $ConfigPath
    exeExtension = if ($IsWindows) { '.exe' } else { '' }
    listenAddress = $ListenAddress
    persistenceLocation = if ($Storage -eq 'memory') { '' } else { $PersistenceLocation }
    sqlitePath = if ($Storage -eq 'memory') { '' } else { $PersistenceLocation }
    storageBackend = $Storage
}

$filePath = Resolve-TemplateValue -Template $profile.LaunchFilePath -Tokens $tokens
$argumentList = Resolve-TemplateList -Templates $profile.LaunchArguments -Tokens $tokens
$environmentOverrides = @{}
foreach ($entry in $profile.EnvironmentTemplates.GetEnumerator()) {
    $resolvedValue = Resolve-TemplateValue -Template $entry.Value -Tokens $tokens
    if (-not [string]::IsNullOrWhiteSpace($resolvedValue)) {
        $environmentOverrides[$entry.Key] = $resolvedValue
    }
}

if (-not (Test-Path -LiteralPath $filePath) -and $filePath -notin @('cargo', 'dotnet')) {
    throw "Launch target not found at $filePath."
}

if ($filePath -eq 'dotnet' -and $argumentList.Count -gt 0 -and -not (Test-Path -LiteralPath $argumentList[0])) {
    throw "Launch target not found at $($argumentList[0])."
}

Write-Info "Starting $($profile.DisplayName) on $ListenAddress."
$startProcessParameters = @{
    FilePath = $filePath
    ArgumentList = $argumentList
    WorkingDirectory = $PSScriptRoot
    RedirectStandardOutput = $stdoutPath
    RedirectStandardError = $stderrPath
    PassThru = $true
}

if ($IsWindows) {
    $startProcessParameters.WindowStyle = 'Hidden'
}

$process = $null
Invoke-WithTemporaryEnvironment -EnvironmentOverrides $environmentOverrides -Action {
    $script:process = Start-Process @startProcessParameters
}

$state = [pscustomobject]@{
    configPath = if ([string]::IsNullOrWhiteSpace($ConfigPath)) { $null } else { $ConfigPath }
    displayName = $profile.DisplayName
    engine = $profile.ProfileId
    engineId = $profile.EngineId
    listenAddress = $ListenAddress
    pid = $process.Id
    persistenceLocation = if ($Storage -eq 'memory' -or [string]::IsNullOrWhiteSpace($PersistenceLocation)) { $null } else { $PersistenceLocation }
    sqlitePath = if ($Storage -eq 'memory' -or [string]::IsNullOrWhiteSpace($PersistenceLocation)) { $null } else { $PersistenceLocation }
    startedAtUtc = [DateTime]::UtcNow.ToString('O')
    stderrPath = $stderrPath
    stdoutPath = $stdoutPath
    storage = $Storage
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
        Write-Host "$($profile.DisplayName) started in the background (PID $($process.Id))." -ForegroundColor Green
        Write-Host "Endpoint: http://$($probeTarget.Host):$($probeTarget.Port)" -ForegroundColor Green
        if ($Storage -ne 'memory' -and -not [string]::IsNullOrWhiteSpace($PersistenceLocation)) {
            Write-Host "Persistence location: $PersistenceLocation" -ForegroundColor Green
        }
        if (-not [string]::IsNullOrWhiteSpace($ConfigPath)) {
            Write-Host "Config: $ConfigPath" -ForegroundColor Green
        }
        Write-Host "Logs: $stdoutPath" -ForegroundColor Green
        exit 0
    }

    Start-Sleep -Milliseconds 250
}

Stop-TrackedProcess -ProcessId $process.Id
Remove-Item -LiteralPath $statePath -Force -ErrorAction SilentlyContinue
throw "QsoRipper did not open $($probeTarget.Host):$($probeTarget.Port) within $StartupTimeoutSeconds seconds."

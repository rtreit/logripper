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
$legacyStatePath = Join-Path $runtimeDirectory 'qsoripper-engine.json'
$dotenvPath = Join-Path $PSScriptRoot '.env'
$defaultPersistenceLocation = Join-Path (Join-Path '.' 'data') 'qsoripper.db'

function Get-ProfileStatePath([string]$ProfileId) {
    return Join-Path $runtimeDirectory "qsoripper-$ProfileId.state.json"
}

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

function Get-State([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) {
        return $null
    }

    return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}

function Get-TrackedProcess([string]$Path) {
    $state = Get-State -Path $Path
    if ($null -eq $state) {
        return $null
    }

    $process = Get-Process -Id $state.pid -ErrorAction SilentlyContinue
    if ($null -eq $process) {
        Remove-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
        return $null
    }

    [pscustomobject]@{
        State = $state
        Process = $process
    }
}

function Get-ProbeTargets([string]$Address) {
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

    $probeHosts = switch ($probeHost) {
        '0.0.0.0' { @('127.0.0.1') }
        '::' { @('::1', '127.0.0.1') }
        '[::]' { @('::1', '127.0.0.1') }
        '*' { @('127.0.0.1') }
        '+' { @('127.0.0.1') }
        default { @($probeHost) }
    }

    return @(
        $probeHosts |
            Select-Object -Unique |
            ForEach-Object {
                [pscustomobject]@{
                    Host = $_
                    Port = $probePort
                }
            }
    )
}

function Format-HttpEndpoint([string]$TargetHost, [int]$Port) {
    if ($TargetHost.Contains(':') -and -not $TargetHost.StartsWith('[')) {
        return "http://[${TargetHost}]:${Port}"
    }

    return "http://${TargetHost}:${Port}"
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

function Get-ProcessCommandLine([int]$ProcessId) {
    if ($IsWindows) {
        return (Get-CimInstance Win32_Process -Filter "ProcessId = $ProcessId" -ErrorAction SilentlyContinue).CommandLine
    }

    $procCommandLinePath = "/proc/$ProcessId/cmdline"
    if (Test-Path -LiteralPath $procCommandLinePath) {
        $raw = [System.IO.File]::ReadAllText($procCommandLinePath)
        return ($raw -replace "`0", ' ').Trim()
    }

    foreach ($psPath in @('/bin/ps', '/usr/bin/ps')) {
        if (Test-Path -LiteralPath $psPath) {
            $commandLine = & $psPath -o command= -p $ProcessId 2>$null | Select-Object -First 1
            if (-not [string]::IsNullOrWhiteSpace($commandLine)) {
                return $commandLine.Trim()
            }
        }
    }

    return $null
}

function Get-UntrackedEngineProcesses {
    param(
        [pscustomobject]$Profile,
        [int]$TrackedProcessId = 0
    )

    $fragments = [System.Collections.Generic.List[string]]::new()
    if (-not [string]::IsNullOrWhiteSpace($Profile.LaunchFilePath) -and $Profile.LaunchFilePath -notin @('cargo', 'dotnet')) {
        $fragments.Add($Profile.LaunchFilePath)
        $fragments.Add([System.IO.Path]::GetFileName($Profile.LaunchFilePath))
    }

    foreach ($argument in $Profile.LaunchArguments) {
        if ([string]::IsNullOrWhiteSpace($argument) -or $argument -match '^\{.+\}$') {
            continue
        }

        $fragments.Add($argument)
        if (Test-Path -LiteralPath $argument) {
            $fragments.Add([System.IO.Path]::GetFileName($argument))
        }
    }

    $matches = [System.Collections.Generic.List[System.Diagnostics.Process]]::new()
    foreach ($candidate in Get-Process -ErrorAction SilentlyContinue) {
        if ($TrackedProcessId -gt 0 -and $candidate.Id -eq $TrackedProcessId) {
            continue
        }

        $commandLine = Get-ProcessCommandLine -ProcessId $candidate.Id
        if ([string]::IsNullOrWhiteSpace($commandLine)) {
            continue
        }

        foreach ($fragment in $fragments | Select-Object -Unique) {
            if ($commandLine.Contains($fragment, [System.StringComparison]::OrdinalIgnoreCase)) {
                $matches.Add($candidate)
                break
            }
        }
    }

    return @($matches | Sort-Object Id -Unique)
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
            DefaultPersistenceLocation = $defaultPersistenceLocation
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
            DefaultPersistenceLocation = $defaultPersistenceLocation
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

$statePath = Get-ProfileStatePath -ProfileId $profile.ProfileId

# Migrate legacy single-engine state file if it exists and matches this profile
if ((Test-Path -LiteralPath $legacyStatePath) -and -not (Test-Path -LiteralPath $statePath)) {
    $legacyState = Get-State -Path $legacyStatePath
    if ($null -ne $legacyState -and $legacyState.engine -eq $profile.ProfileId) {
        Move-Item -LiteralPath $legacyStatePath -Destination $statePath -Force
    }
}

$existing = Get-TrackedProcess -Path $statePath
if ($null -ne $existing) {
    if (-not $ForceRestart) {
        Write-Host "$($profile.DisplayName) is already running (PID $($existing.Process.Id)) at $($existing.State.listenAddress)." -ForegroundColor Yellow
        Write-Host "Stop it first with .\stop-qsoripper.ps1 -Engine $($profile.ProfileId) or rerun with -ForceRestart." -ForegroundColor Yellow
        exit 0
    }

    Write-Info "Stopping existing $($profile.DisplayName) process $($existing.Process.Id)."
    Stop-TrackedProcess -ProcessId $existing.Process.Id
    Remove-Item -LiteralPath $statePath -Force -ErrorAction SilentlyContinue
}

if ($ForceRestart) {
    $trackedProcessId = if ($null -ne $existing) { $existing.Process.Id } else { 0 }
    $orphans = Get-UntrackedEngineProcesses -Profile $profile -TrackedProcessId $trackedProcessId
    foreach ($orphan in $orphans) {
        Write-Info "Stopping untracked $($profile.DisplayName) process $($orphan.Id)."
        Stop-TrackedProcess -ProcessId $orphan.Id
    }

    $launchArtifactPath = if ($profile.LaunchFilePath -eq 'dotnet') {
        $profile.LaunchArguments | Select-Object -First 1
    }
    else {
        $profile.LaunchFilePath
    }

    if ($IsWindows -and $orphans -and -not [string]::IsNullOrWhiteSpace($launchArtifactPath) -and (Test-Path -LiteralPath $launchArtifactPath)) {
        for ($lockAttempt = 0; $lockAttempt -lt 20; $lockAttempt++) {
            try {
                [System.IO.File]::Open($launchArtifactPath, 'Open', 'ReadWrite', 'None').Dispose()
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
    listenAddress = $ListenAddress
    persistenceLocation = if ($Storage -eq 'memory') { '' } else { $PersistenceLocation }
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
    startedAtUtc = [DateTime]::UtcNow.ToString('O')
    stderrPath = $stderrPath
    stdoutPath = $stdoutPath
    storage = $Storage
}
$state | ConvertTo-Json | Set-Content -LiteralPath $statePath

$probeTargets = Get-ProbeTargets -Address $ListenAddress
$deadline = [DateTime]::UtcNow.AddSeconds($StartupTimeoutSeconds)

while ([DateTime]::UtcNow -lt $deadline) {
    $runningProcess = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
    if ($null -eq $runningProcess) {
        $stderrTail = Get-LogTail -Path $stderrPath
        $stdoutTail = Get-LogTail -Path $stdoutPath
        $details = @($stderrTail + $stdoutTail) -join [Environment]::NewLine
        throw "QsoRipper exited during startup.`n$details"
    }

    foreach ($probeTarget in $probeTargets) {
        if (Test-TcpEndpoint -TargetHost $probeTarget.Host -Port $probeTarget.Port) {
            Write-Host "$($profile.DisplayName) started in the background (PID $($process.Id))." -ForegroundColor Green
            Write-Host "Endpoint: $(Format-HttpEndpoint -TargetHost $probeTarget.Host -Port $probeTarget.Port)" -ForegroundColor Green
            if ($Storage -ne 'memory' -and -not [string]::IsNullOrWhiteSpace($PersistenceLocation)) {
                Write-Host "Persistence location: $PersistenceLocation" -ForegroundColor Green
            }
            if (-not [string]::IsNullOrWhiteSpace($ConfigPath)) {
                Write-Host "Config: $ConfigPath" -ForegroundColor Green
            }
            Write-Host "Logs: $stdoutPath" -ForegroundColor Green
            exit 0
        }
    }

    Start-Sleep -Milliseconds 250
}

Stop-TrackedProcess -ProcessId $process.Id
Remove-Item -LiteralPath $statePath -Force -ErrorAction SilentlyContinue
throw "QsoRipper did not open any expected endpoint ($((@($probeTargets | ForEach-Object { Format-HttpEndpoint -TargetHost $_.Host -Port $_.Port })) -join ', ')) within $StartupTimeoutSeconds seconds."

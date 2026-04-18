#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Runs a client-visible conformance scenario against both local engine implementations.

.DESCRIPTION
    Builds the .NET CLI if needed, starts the Rust and managed .NET engines one at a time
    through the standard launcher helper, drives the same CLI workflow against each engine,
    and verifies that the client-observable logbook behavior matches after swapping engines.
#>

[CmdletBinding()]
param(
    [string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$startScript = Join-Path $repoRoot 'start-qsoripper.ps1'
$stopScript = Join-Path $repoRoot 'stop-qsoripper.ps1'
$engineStatePath = Join-Path (Join-Path $repoRoot 'artifacts') 'run'
$engineStatePath = Join-Path $engineStatePath 'qsoripper-engine.json'
$dotnetRoot = Join-Path (Join-Path $repoRoot 'src') 'dotnet'
$cliRoot = Join-Path $dotnetRoot 'QsoRipper.Cli'
$cliProject = Join-Path $cliRoot 'QsoRipper.Cli.csproj'
$cliDll = Join-Path (Join-Path (Join-Path $cliRoot 'bin') 'Release') 'net10.0'
$cliDll = Join-Path $cliDll 'QsoRipper.Cli.dll'

if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $artifactsRoot = Join-Path $repoRoot 'artifacts'
    $OutputDirectory = Join-Path $artifactsRoot 'conformance'
}

$runId = [Guid]::NewGuid().ToString('N')
$runDirectory = Join-Path $OutputDirectory $runId
$null = New-Item -ItemType Directory -Path $runDirectory -Force

$stationCallsign = 'K7TST'
$profileName = 'Conformance'
$grid = 'CN87'
$workedCallsign = 'W1AW'
$qsoComment = 'Engine conformance smoke'
$qsoNotes = 'Shared CLI scenario'
$qsoTime = '2026-04-15T12:00:00Z'

function Write-Step([string]$Message) {
    Write-Host "`n>> $Message" -ForegroundColor Cyan
}

function Stop-TestEngine {
    & $stopScript | Out-Null
}

function Invoke-Cli {
    param(
        [string[]]$Arguments,
        [hashtable]$Environment = @{}
    )

    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = 'dotnet'
    $startInfo.WorkingDirectory = $repoRoot
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.UseShellExecute = $false
    $startInfo.ArgumentList.Add($cliDll)

    foreach ($name in @(
            'QSORIPPER_QRZ_XML_USERNAME',
            'QSORIPPER_QRZ_XML_PASSWORD',
            'QSORIPPER_QRZ_USER_AGENT',
            'QSORIPPER_QRZ_XML_BASE_URL',
            'QSORIPPER_QRZ_LOGBOOK_API_KEY',
            'QSORIPPER_QRZ_LOGBOOK_BASE_URL'
        )) {
        $null = $startInfo.Environment.Remove($name)
    }

    foreach ($argument in $Arguments) {
        $startInfo.ArgumentList.Add($argument)
    }

    foreach ($entry in $Environment.GetEnumerator()) {
        $startInfo.Environment[$entry.Key] = [string]$entry.Value
    }

    $process = [System.Diagnostics.Process]::Start($startInfo)
    if ($null -eq $process) {
        throw "Failed to start CLI process."
    }

    $stdout = $process.StandardOutput.ReadToEnd()
    $stderr = $process.StandardError.ReadToEnd()
    $process.WaitForExit()

    [pscustomobject]@{
        ExitCode = $process.ExitCode
        StdOut = $stdout
        StdErr = $stderr
    }
}

function Assert-CommandSucceeded {
    param(
        [pscustomobject]$Result,
        [string]$Description
    )

    if ($Result.ExitCode -ne 0) {
        throw "$Description failed with exit code $($Result.ExitCode).`nSTDOUT:`n$($Result.StdOut)`nSTDERR:`n$($Result.StdErr)"
    }
}

function Get-StartedEngineState {
    if (-not (Test-Path -LiteralPath $engineStatePath)) {
        throw "Engine state file was not created at $engineStatePath."
    }

    return Get-Content -LiteralPath $engineStatePath -Raw | ConvertFrom-Json
}

function Get-ObjectPropertyValue {
    param(
        $Object,
        [string]$Name,
        [object]$DefaultValue = ''
    )

    if ($null -eq $Object) {
        return $DefaultValue
    }

    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $DefaultValue
    }

    return $property.Value
}

function Get-RstDisplay($Rst) {
    if ($null -eq $Rst) {
        return $null
    }

    $raw = Get-ObjectPropertyValue -Object $Rst -Name 'raw' -DefaultValue $null
    if ($null -ne $raw) {
        return [string]$raw
    }

    $readability = [string](Get-ObjectPropertyValue -Object $Rst -Name 'readability')
    $strength = [string](Get-ObjectPropertyValue -Object $Rst -Name 'strength')
    $tone = [string](Get-ObjectPropertyValue -Object $Rst -Name 'tone')
    return "$readability$strength$tone"
}

function Normalize-QsoRecord($Qso) {
    $stationSnapshot = Get-ObjectPropertyValue -Object $Qso -Name 'stationSnapshot' -DefaultValue $null
    $rstSent = Get-ObjectPropertyValue -Object $Qso -Name 'rstSent' -DefaultValue $null
    $rstReceived = Get-ObjectPropertyValue -Object $Qso -Name 'rstReceived' -DefaultValue $null

    [ordered]@{
        workedCallsign = [string](Get-ObjectPropertyValue -Object $Qso -Name 'workedCallsign')
        stationCallsign = [string](Get-ObjectPropertyValue -Object $Qso -Name 'stationCallsign')
        band = [string](Get-ObjectPropertyValue -Object $Qso -Name 'band')
        mode = [string](Get-ObjectPropertyValue -Object $Qso -Name 'mode')
        utcTimestamp = [string](Get-ObjectPropertyValue -Object $Qso -Name 'utcTimestamp')
        comment = [string](Get-ObjectPropertyValue -Object $Qso -Name 'comment')
        notes = [string](Get-ObjectPropertyValue -Object $Qso -Name 'notes')
        rstSent = Get-RstDisplay $rstSent
        rstReceived = Get-RstDisplay $rstReceived
        stationSnapshotCallsign = [string](Get-ObjectPropertyValue -Object $stationSnapshot -Name 'stationCallsign')
        stationSnapshotGrid = [string](Get-ObjectPropertyValue -Object $stationSnapshot -Name 'grid')
    }
}

function Parse-AdifRecords {
    param([string]$Text)

    $records = [System.Collections.Generic.List[object]]::new()
    $current = [ordered]@{}
    $index = 0

    while ($index -lt $Text.Length) {
        $tagStart = $Text.IndexOf('<', $index)
        if ($tagStart -lt 0) {
            break
        }

        $tagEnd = $Text.IndexOf('>', $tagStart)
        if ($tagEnd -lt 0) {
            break
        }

        $header = $Text.Substring($tagStart + 1, $tagEnd - $tagStart - 1)
        $index = $tagEnd + 1

        if ($header -match '^(?i)eoh$') {
            continue
        }

        if ($header -match '^(?i)eor$') {
            if ($current.Count -gt 0) {
                $records.Add([pscustomobject]$current)
                $current = [ordered]@{}
            }

            continue
        }

        if ($header -notmatch '^(?<name>[^:>]+):(?<length>\d+)(:(?<type>[^:>]+))?$') {
            continue
        }

        $fieldLength = [int]$Matches.length
        if ($index + $fieldLength -gt $Text.Length) {
            throw "ADIF field '$($Matches.name)' length exceeded remaining export payload."
        }

        $fieldName = $Matches.name.ToUpperInvariant()
        $fieldValue = $Text.Substring($index, $fieldLength)
        $index += $fieldLength
        $current[$fieldName] = $fieldValue
    }

    if ($current.Count -gt 0) {
        $records.Add([pscustomobject]$current)
    }

    return @($records)
}

function Normalize-AdifRecord($Record) {
    [ordered]@{
        workedCallsign = [string](Get-ObjectPropertyValue -Object $Record -Name 'CALL')
        stationCallsign = [string](Get-ObjectPropertyValue -Object $Record -Name 'STATION_CALLSIGN')
        band = [string](Get-ObjectPropertyValue -Object $Record -Name 'BAND')
        mode = [string](Get-ObjectPropertyValue -Object $Record -Name 'MODE')
        qsoDate = [string](Get-ObjectPropertyValue -Object $Record -Name 'QSO_DATE')
        timeOn = [string](Get-ObjectPropertyValue -Object $Record -Name 'TIME_ON')
        comment = [string](Get-ObjectPropertyValue -Object $Record -Name 'COMMENT')
        notes = [string](Get-ObjectPropertyValue -Object $Record -Name 'NOTES')
        rstSent = [string](Get-ObjectPropertyValue -Object $Record -Name 'RST_SENT')
        rstReceived = [string](Get-ObjectPropertyValue -Object $Record -Name 'RST_RCVD')
    }
}

function Start-TestEngine {
    param(
        [string]$EngineProfile,
        [string]$ConfigPath,
        [string]$Storage,
        [string]$SqlitePath
    )

    $parameters = @{
        Engine = $EngineProfile
        ConfigPath = $ConfigPath
        ForceRestart = $true
    }

    if (-not [string]::IsNullOrWhiteSpace($Storage)) {
        $parameters.Storage = $Storage
    }

    if (-not [string]::IsNullOrWhiteSpace($SqlitePath)) {
        $parameters.PersistenceLocation = $SqlitePath
    }

    if ($SkipBuild) {
        $parameters.SkipBuild = $true
    }

    & $startScript @parameters | Out-Host
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to start engine profile '$EngineProfile'."
    }

    return Get-StartedEngineState
}

function Invoke-ConformanceScenario {
    param(
        [string]$EngineProfile,
        [string]$ConfigPath,
        [string]$PersistencePath,
        [string]$Storage
    )

    Write-Step "Starting $EngineProfile engine"
    $engineState = Start-TestEngine -EngineProfile $EngineProfile -ConfigPath $ConfigPath -Storage $Storage -SqlitePath $PersistencePath

    $environment = @{
        QSORIPPER_STATION_CALLSIGN = $stationCallsign
        QSORIPPER_OPERATOR_CALLSIGN = $stationCallsign
        QSORIPPER_PROFILE_NAME = $profileName
        QSORIPPER_GRID = $grid
    }

    if ($Storage -ne 'memory') {
        $environment.QSORIPPER_LOG_FILE = $PersistencePath
    }

    Write-Step "Running CLI setup against $EngineProfile"
    $setupFromEnv = Invoke-Cli -Arguments @('--engine', $EngineProfile, 'setup', '--from-env') -Environment $environment
    Assert-CommandSucceeded -Result $setupFromEnv -Description "$EngineProfile setup --from-env"

    $setupStatus = Invoke-Cli -Arguments @('--engine', $EngineProfile, 'setup', '--status', '--json')
    Assert-CommandSucceeded -Result $setupStatus -Description "$EngineProfile setup --status --json"
    $setupStatusJson = $setupStatus.StdOut | ConvertFrom-Json

    if (-not $setupStatusJson.status.setupComplete) {
        throw "$EngineProfile setup did not complete successfully."
    }

    if ($setupStatusJson.status.stationProfile.stationCallsign -ne $stationCallsign) {
        throw "$EngineProfile station profile was not persisted correctly."
    }

    Write-Step "Running CLI status against $EngineProfile"
    $statusText = Invoke-Cli -Arguments @('--engine', $EngineProfile, 'status')
    Assert-CommandSucceeded -Result $statusText -Description "$EngineProfile status"

    $expectedEngineId = [string](Get-ObjectPropertyValue -Object $engineState -Name 'engineId')
    if ([string]::IsNullOrWhiteSpace($expectedEngineId)) {
        throw "$EngineProfile launcher state did not include an engine id."
    }
    if ($statusText.StdOut -notmatch [regex]::Escape("($expectedEngineId)")) {
        throw "$EngineProfile status output did not advertise engine id '$expectedEngineId'.`n$($statusText.StdOut)"
    }

    Write-Step "Logging QSO through CLI against $EngineProfile"
    $logResult = Invoke-Cli -Arguments @(
        '--engine', $EngineProfile,
        'log',
        $workedCallsign,
        '20m',
        'CW',
        '--at', $qsoTime,
        '--comment', $qsoComment,
        '--notes', $qsoNotes,
        '--no-enrich'
    )
    Assert-CommandSucceeded -Result $logResult -Description "$EngineProfile log"

    if ($logResult.StdOut -notmatch 'QSO logged:\s+(?<localId>[^\r\n]+)') {
        throw "Unable to parse local id from $EngineProfile log output.`n$($logResult.StdOut)"
    }

    $localId = $Matches.localId.Trim()

    $statusJsonResult = Invoke-Cli -Arguments @('--engine', $EngineProfile, 'status', '--json')
    Assert-CommandSucceeded -Result $statusJsonResult -Description "$EngineProfile status --json"
    $statusJson = $statusJsonResult.StdOut | ConvertFrom-Json

    $listResult = Invoke-Cli -Arguments @('--engine', $EngineProfile, 'list', '--json', '--limit', '5')
    Assert-CommandSucceeded -Result $listResult -Description "$EngineProfile list --json"
    $listJson = @($listResult.StdOut | ConvertFrom-Json)

    if ($listJson.Count -ne 1) {
        throw "$EngineProfile expected exactly one QSO in list output but saw $($listJson.Count)."
    }

    $getResult = Invoke-Cli -Arguments @('--engine', $EngineProfile, 'get', $localId, '--json')
    Assert-CommandSucceeded -Result $getResult -Description "$EngineProfile get --json"
    $getJson = $getResult.StdOut | ConvertFrom-Json

    $exportPath = Join-Path $runDirectory "$EngineProfile-export.adi"
    $exportResult = Invoke-Cli -Arguments @('--engine', $EngineProfile, 'export', '--file', $exportPath)
    Assert-CommandSucceeded -Result $exportResult -Description "$EngineProfile export"
    $exportText = Get-Content -LiteralPath $exportPath -Raw
    $exportRecords = @(Parse-AdifRecords -Text $exportText)

    if ($exportText -notmatch '<CALL:4>W1AW') {
        throw "$EngineProfile export output did not contain the logged callsign."
    }

    if ($exportRecords.Count -ne 1) {
        throw "$EngineProfile export expected exactly one ADIF record but saw $($exportRecords.Count)."
    }

    $deleteResult = Invoke-Cli -Arguments @('--engine', $EngineProfile, 'delete', $localId)
    Assert-CommandSucceeded -Result $deleteResult -Description "$EngineProfile delete"

    $listAfterDeleteResult = Invoke-Cli -Arguments @('--engine', $EngineProfile, 'list', '--json', '--limit', '5')
    Assert-CommandSucceeded -Result $listAfterDeleteResult -Description "$EngineProfile list after delete --json"
    $listAfterDeleteJson = @($listAfterDeleteResult.StdOut | ConvertFrom-Json)
    if ($listAfterDeleteJson.Count -ne 0) {
        throw "$EngineProfile expected zero QSOs after delete but saw $($listAfterDeleteJson.Count)."
    }

    $getAfterDeleteResult = Invoke-Cli -Arguments @('--engine', $EngineProfile, 'get', $localId, '--json')
    if ($getAfterDeleteResult.ExitCode -eq 0) {
        throw "$EngineProfile get unexpectedly succeeded after delete for local id '$localId'."
    }

    return [pscustomobject]@{
        EngineProfile = $EngineProfile
        EngineId = $expectedEngineId
        SetupStatus = $setupStatusJson.status
        SyncStatus = $statusJson
        Qso = Normalize-QsoRecord $getJson.qso
        ListQso = Normalize-QsoRecord $listJson[0]
        ExportQso = Normalize-AdifRecord $exportRecords[0]
    }
}

function Assert-SharedSetupRoundTrip {
    param(
        [string]$FirstEngineProfile,
        [string]$SecondEngineProfile,
        [string]$ScenarioId
    )

    $scenarioDirectory = Join-Path $runDirectory $ScenarioId
    $null = New-Item -ItemType Directory -Path $scenarioDirectory -Force
    $configPath = Join-Path $scenarioDirectory 'config.toml'
    $persistencePath = Join-Path $scenarioDirectory 'shared-setup.db'
    $environment = @{
        QSORIPPER_LOG_FILE = $persistencePath
        QSORIPPER_STATION_CALLSIGN = $stationCallsign
        QSORIPPER_OPERATOR_CALLSIGN = $stationCallsign
        QSORIPPER_PROFILE_NAME = $profileName
        QSORIPPER_GRID = $grid
    }

    Write-Step "Seeding shared setup with $FirstEngineProfile"
    $null = Start-TestEngine -EngineProfile $FirstEngineProfile -ConfigPath $configPath -Storage 'sqlite' -SqlitePath $persistencePath
    $seedSetup = Invoke-Cli -Arguments @('--engine', $FirstEngineProfile, 'setup', '--from-env') -Environment $environment
    Assert-CommandSucceeded -Result $seedSetup -Description "$FirstEngineProfile setup round-trip seed"
    Stop-TestEngine

    Write-Step "Verifying shared setup on $SecondEngineProfile"
    $null = Start-TestEngine -EngineProfile $SecondEngineProfile -ConfigPath $configPath -Storage '' -SqlitePath ''
    $statusResult = Invoke-Cli -Arguments @('--engine', $SecondEngineProfile, 'setup', '--status', '--json')
    Assert-CommandSucceeded -Result $statusResult -Description "$SecondEngineProfile setup --status round-trip"
    $statusJson = $statusResult.StdOut | ConvertFrom-Json
    $runtimeResult = Invoke-Cli -Arguments @('--engine', $SecondEngineProfile, 'config', '--json')
    Assert-CommandSucceeded -Result $runtimeResult -Description "$SecondEngineProfile config --json round-trip"
    $runtimeJson = $runtimeResult.StdOut | ConvertFrom-Json

    if (-not $statusJson.status.setupComplete) {
        throw "$ScenarioId did not preserve setup completeness when switching from $FirstEngineProfile to $SecondEngineProfile."
    }

    if ($statusJson.status.stationProfile.stationCallsign -ne $stationCallsign) {
        throw "$ScenarioId did not preserve the station profile when switching from $FirstEngineProfile to $SecondEngineProfile."
    }

    if ([string]::IsNullOrWhiteSpace($statusJson.status.activeStationProfileId)) {
        throw "$ScenarioId did not preserve the active station profile id when switching from $FirstEngineProfile to $SecondEngineProfile."
    }

    if ($runtimeJson.activeStorageBackend -ne 'sqlite') {
        throw "$ScenarioId did not reload sqlite storage from shared config when switching from $FirstEngineProfile to $SecondEngineProfile."
    }

    if ([string]::IsNullOrWhiteSpace($runtimeJson.persistenceLocation)) {
        throw "$ScenarioId did not report a persistence path after switching from $FirstEngineProfile to $SecondEngineProfile."
    }

    if ([System.IO.Path]::GetFullPath($runtimeJson.persistenceLocation) -ne [System.IO.Path]::GetFullPath($persistencePath)) {
        throw "$ScenarioId did not preserve the shared persistence path when switching from $FirstEngineProfile to $SecondEngineProfile."
    }

    Stop-TestEngine
}

function Assert-EquivalentRecords {
    param(
        [System.Collections.IDictionary]$Left,
        [System.Collections.IDictionary]$Right,
        [string]$Description
    )

    foreach ($key in $Left.Keys) {
        if ($Left[$key] -ne $Right[$key]) {
            throw "$Description mismatch for '$key'. Left='$($Left[$key])' Right='$($Right[$key])'."
        }
    }
}

function Select-ScenarioResult {
    param(
        [object[]]$Results,
        [string]$EngineProfile
    )

    $candidates = @(
        $Results |
            Where-Object {
                $null -ne $_ -and
                $null -ne $_.PSObject.Properties['Qso'] -and
                $null -ne $_.PSObject.Properties['ListQso'] -and
                $null -ne $_.PSObject.Properties['SyncStatus'] -and
                $null -ne $_.PSObject.Properties['ExportQso']
            }
    )

    if ($candidates.Count -ne 1) {
        $types = $Results | ForEach-Object {
            if ($null -eq $_) {
                '<null>'
            }
            else {
                $_.GetType().FullName
            }
        }

        throw "Expected exactly one structured scenario result for $EngineProfile but found $($candidates.Count). Output types: $($types -join ', ')."
    }

    return $candidates[0]
}

try {
    Write-Step 'Stopping any existing tracked engine'
    Stop-TestEngine

    if (-not $SkipBuild -or -not (Test-Path -LiteralPath $cliDll)) {
        Write-Step 'Building QsoRipper CLI (Release)'
        dotnet build $cliProject -c Release
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to build QsoRipper CLI."
        }
    }

    $sharedConfigPath = Join-Path $runDirectory 'config.toml'
    $persistencePath = Join-Path $runDirectory 'conformance-log.db'

    $rustScenarioOutput = @(Invoke-ConformanceScenario -EngineProfile 'rust' -ConfigPath $sharedConfigPath -PersistencePath $persistencePath -Storage 'sqlite')
    $rustResult = Select-ScenarioResult -Results $rustScenarioOutput -EngineProfile 'rust'
    Stop-TestEngine

    $dotnetScenarioOutput = @(Invoke-ConformanceScenario -EngineProfile 'dotnet' -ConfigPath $sharedConfigPath -PersistencePath $persistencePath -Storage 'sqlite')
    $dotnetResult = Select-ScenarioResult -Results $dotnetScenarioOutput -EngineProfile 'dotnet'
    Stop-TestEngine

    Assert-SharedSetupRoundTrip -FirstEngineProfile 'rust' -SecondEngineProfile 'dotnet' -ScenarioId 'rust-to-dotnet'
    Assert-SharedSetupRoundTrip -FirstEngineProfile 'dotnet' -SecondEngineProfile 'rust' -ScenarioId 'dotnet-to-rust'

    Assert-EquivalentRecords -Left $rustResult.Qso -Right $dotnetResult.Qso -Description 'GetQso'
    Assert-EquivalentRecords -Left $rustResult.ListQso -Right $dotnetResult.ListQso -Description 'ListQsos'
    Assert-EquivalentRecords -Left $rustResult.ExportQso -Right $dotnetResult.ExportQso -Description 'ExportAdif'

    if ($rustResult.SyncStatus.localQsoCount -ne 1 -or $dotnetResult.SyncStatus.localQsoCount -ne 1) {
        throw "Expected both engines to report one local QSO after the shared CLI scenario."
    }

    $summary = [pscustomobject]@{
        rust = [pscustomobject]@{
            engine = $rustResult.EngineProfile
            localQsoCount = $rustResult.SyncStatus.localQsoCount
            qso = $rustResult.Qso
        }
        dotnet = [pscustomobject]@{
            engine = $dotnetResult.EngineProfile
            localQsoCount = $dotnetResult.SyncStatus.localQsoCount
            qso = $dotnetResult.Qso
        }
        exports = [pscustomobject]@{
            rust = Join-Path $runDirectory 'rust-export.adi'
            dotnet = Join-Path $runDirectory 'dotnet-export.adi'
        }
    }

    $summaryPath = Join-Path $runDirectory 'engine-conformance-summary.json'
    $summary | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $summaryPath

    Write-Host "`nEngine conformance scenario passed." -ForegroundColor Green
    Write-Host "Artifacts: $runDirectory" -ForegroundColor Green
    exit 0
}
finally {
    Stop-TestEngine
}

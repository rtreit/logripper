#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Pester tests for Run-StressTest.ps1 quality gates.
.DESCRIPTION
    Validates that the stress test harness properly checks build exit codes
    and includes all error categories in pass/fail decisions.
#>

$scriptPath = Join-Path $PSScriptRoot 'Run-StressTest.ps1'
$scriptContent = Get-Content $scriptPath -Raw

Describe 'Run-StressTest.ps1 build exit-code checks (Bug #203)' {

    It 'checks LASTEXITCODE after cargo build -p qsoripper-server' {
        $scriptContent | Should Match 'cargo build -p qsoripper-server'
        $lines = $scriptContent -split "`n"
        $match = $lines | Select-String -SimpleMatch 'cargo build -p qsoripper-server' | Select-Object -First 1
        $cargoBuildIdx = $match.LineNumber - 1
        $cargoBuildIdx | Should Not BeNullOrEmpty
        # Search the next 5 lines for a LASTEXITCODE check
        $found = $false
        for ($i = $cargoBuildIdx + 1; $i -le [Math]::Min($cargoBuildIdx + 5, $lines.Count - 1); $i++) {
            if ($lines[$i] -match '\$LASTEXITCODE') { $found = $true; break }
        }
        $found | Should Be $true
    }

    It 'checks LASTEXITCODE after cargo test --no-run' {
        $lines = $scriptContent -split "`n"
        $match = $lines | Select-String -SimpleMatch 'cargo test --test stress_test --no-run' | Select-Object -First 1
        $cargoTestIdx = $match.LineNumber - 1
        $cargoTestIdx | Should Not BeNullOrEmpty
        $found = $false
        for ($i = $cargoTestIdx + 1; $i -le [Math]::Min($cargoTestIdx + 5, $lines.Count - 1); $i++) {
            if ($lines[$i] -match '\$LASTEXITCODE') { $found = $true; break }
        }
        $found | Should Be $true
    }

    It 'checks LASTEXITCODE after dotnet build' {
        $lines = $scriptContent -split "`n"
        $dotnetBuildIdx = ($lines | Select-String -SimpleMatch 'dotnet build --nologo').LineNumber - 1
        $dotnetBuildIdx | Should Not BeNullOrEmpty
        $found = $false
        for ($i = $dotnetBuildIdx + 1; $i -le [Math]::Min($dotnetBuildIdx + 5, $lines.Count - 1); $i++) {
            if ($lines[$i] -match '\$LASTEXITCODE') { $found = $true; break }
        }
        $found | Should Be $true
    }
}

Describe 'Run-StressTest.ps1 pass/fail logic (Bug #204)' {

    It 'includes grpcInternalCount in pass/fail decision' {
        # The final pass/fail block must reference grpcInternalCount
        $passFailBlock = ($scriptContent -split 'Report written to')[1]
        $passFailBlock | Should Match 'grpcInternalCount'
    }

    It 'parses Other errors count from client output' {
        # Must have a regex matching "Other errors:" to extract the count
        $scriptContent | Should Match "Other errors:"
    }

    It 'includes transport/client error count in pass/fail decision' {
        $passFailBlock = ($scriptContent -split 'Report written to')[1]
        $passFailBlock | Should Match 'clientErrorCount'
    }

    It 'includes dotnet exit code in pass/fail decision' {
        $passFailBlock = ($scriptContent -split 'Report written to')[1]
        $passFailBlock | Should Match 'dotnetExitCode'
    }
}

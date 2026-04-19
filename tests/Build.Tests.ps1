#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Pester tests for build.ps1 quality gate parity with CI.
.DESCRIPTION
    Validates that local check-rust and check-dotnet functions include the
    same quality gates that CI enforces (coverage thresholds, vuln checks).
#>

$repoRoot = Split-Path -Parent $PSScriptRoot
$scriptPath = Join-Path $repoRoot 'build.ps1'
$scriptContent = Get-Content $scriptPath -Raw
$win32MainPath = Join-Path $repoRoot 'src' 'c' 'qsoripper-win32' 'src' 'main.c'
$win32MainContent = Get-Content $win32MainPath -Raw

# Extract function bodies for targeted checks
function Get-FunctionBody([string]$Content, [string]$FunctionName) {
    $pattern = "(?ms)function\s+$FunctionName\s*\{(.+?)^\}"
    if ($Content -match $pattern) { return $Matches[1] }
    return ''
}

$checkRustBody = Get-FunctionBody $scriptContent 'Check-Rust'
$checkDotnetBody = Get-FunctionBody $scriptContent 'Check-Dotnet'

function Get-CFunctionBody([string]$Content, [string]$FunctionSignature) {
    $escaped = [regex]::Escape($FunctionSignature)
    $pattern = "(?ms)$escaped\s*\{(.+?)^\}"
    if ($Content -match $pattern) { return $Matches[1] }
    return ''
}

$win32MainPath = Join-Path $repoRoot 'src' 'c' 'qsoripper-win32' 'src' 'main.c'
$win32MainContent = Get-Content $win32MainPath -Raw
$logQsoBody = Get-CFunctionBody $win32MainContent 'static void LogQso(void)'

Describe 'build.ps1 Check-Rust CI parity (Bug #202)' {

    It 'runs tests with coverage via cargo-llvm-cov when available' {
        # Check-Rust must reference cargo-llvm-cov for coverage collection
        $checkRustBody | Should Match 'cargo-llvm-cov'
    }

    It 'checks Rust coverage against a threshold' {
        # Must reference a numeric threshold (80) for coverage validation
        $checkRustBody | Should Match '80'
    }

    It 'fails if Rust coverage is below threshold' {
        # Must have exit/throw logic tied to coverage check
        $checkRustBody | Should Match 'coverage.*threshold|threshold.*coverage|below.*threshold'
    }
}

Describe 'build.ps1 Check-Dotnet CI parity (Bug #202)' {

    It 'runs tests with coverage collection' {
        # Check-Dotnet must reference XPlat Code Coverage for coverage collection
        $checkDotnetBody | Should Match 'XPlat Code Coverage|Code Coverage'
    }

    It 'checks .NET coverage against a threshold' {
        # Must reference a numeric threshold (50) for coverage validation
        $checkDotnetBody | Should Match '50'
    }

    It 'fails if .NET coverage is below threshold' {
        $checkDotnetBody | Should Match 'coverage.*threshold|threshold.*coverage|below.*threshold'
    }

    It 'runs vulnerable package check' {
        # Must reference --vulnerable for package vulnerability scanning
        $checkDotnetBody | Should Match '--vulnerable'
    }
}

Describe 'Win32 CLI publish/discovery path contract (WIN32-BUG-2)' {

    It 'publishes CLI to artifacts\publish\qsoripper-cli\<Configuration>' {
        $scriptContent | Should Match "'qsoripper-cli'"
    }

    It 'probes the qsoripper-cli directory from FindCliPath candidates' {
        $win32MainContent | Should Match 'qsoripper-cli'
        $win32MainContent | Should Not Match 'QsoRipper\.Cli\\\\%s\\\\(?:net10\.0\\\\)?QsoRipper\.Cli\.exe'
    }
}

Describe 'Win32 LogQso shadowing regression (WIN32-BUG-1)' {

    It 'declares exactly one cmd buffer in LogQso' {
        [regex]::Matches($logQsoBody, 'char\s+cmd\s*\[\s*4096\s*\]\s*;').Count | Should Be 1
    }
}

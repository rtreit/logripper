#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Cross-platform build and quality script for QsoRipper.

.DESCRIPTION
    Orchestrates Rust, .NET, and proto builds and quality checks.
    Mirrors the CI workflows so issues are caught locally before push.

.PARAMETER Command
    The build command to run. Default: build

.EXAMPLE
    ./build.ps1              # Build all projects
    ./build.ps1 check        # Full CI-equivalent quality check
    ./build.ps1 check-rust   # Rust quality only
    ./build.ps1 check-dotnet # .NET quality only
#>

param(
    [Parameter(Position = 0)]
    [ValidateSet('build', 'check', 'rust', 'dotnet', 'check-rust', 'check-dotnet', 'proto', 'help')]
    [string]$Command = 'build'
)

$ErrorActionPreference = 'Stop'

$RustManifest = Join-Path $PSScriptRoot 'src' 'rust' 'Cargo.toml'
$DotnetSolution = Join-Path $PSScriptRoot 'src' 'dotnet' 'QsoRipper.slnx'
$RustDir = Join-Path $PSScriptRoot 'src' 'rust'

function Write-Step([string]$Message) {
    Write-Host "`n=== $Message ===" -ForegroundColor Cyan
}

function Invoke-Build([string]$Step, [string]$Command, [string[]]$Arguments) {
    Write-Step $Step
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: $Step" -ForegroundColor Red
        exit $LASTEXITCODE
    }
}

function Build-Rust {
    Invoke-Build 'Building Rust' cargo @('build', '--manifest-path', $RustManifest)
}

function Build-Dotnet {
    Invoke-Build 'Building .NET' dotnet @('build', $DotnetSolution)
}

function Build-All {
    Build-Rust
    Build-Dotnet
}

function Check-Proto {
    $bufCmd = Get-Command buf -ErrorAction SilentlyContinue
    if (-not $bufCmd) {
        Write-Step 'buf lint'
        Write-Host 'buf not installed, skipping. Install from: https://buf.build/docs/installation' -ForegroundColor Yellow
        return
    }
    Invoke-Build 'buf lint' buf @('lint')
}

function Check-Rust {
    Invoke-Build 'Rust formatting' cargo @('fmt', '--manifest-path', $RustManifest, '--all', '--', '--check')
    Invoke-Build 'Rust clippy' cargo @('clippy', '--manifest-path', $RustManifest, '--all-targets', '--', '-D', 'warnings')
    Invoke-Build 'Rust tests' cargo @('test', '--manifest-path', $RustManifest)

    Check-Proto

    $denyCmd = Get-Command cargo-deny -ErrorAction SilentlyContinue
    if (-not $denyCmd) {
        Write-Step 'cargo deny'
        Write-Host 'cargo-deny not installed, skipping. Install with: cargo install cargo-deny' -ForegroundColor Yellow
    }
    else {
        Write-Step 'cargo deny'
        Push-Location $RustDir
        try {
            cargo deny check --config deny.toml
            if ($LASTEXITCODE -ne 0) {
                Write-Host "FAILED: cargo deny" -ForegroundColor Red
                exit $LASTEXITCODE
            }
        }
        finally {
            Pop-Location
        }
    }
}

function Check-Dotnet {
    Invoke-Build '.NET formatting' dotnet @('format', $DotnetSolution, '--verify-no-changes')
    Invoke-Build '.NET build' dotnet @('build', $DotnetSolution)
    Invoke-Build '.NET tests' dotnet @('test', $DotnetSolution, '--no-build')
}

function Check-All {
    Check-Rust
    Check-Dotnet
}

function Show-Help {
    Write-Host @"

QsoRipper Build Script

Usage: ./build.ps1 [command]

Commands:
  build         Build all projects (default)
  check         Full CI-equivalent quality check
  rust          Build Rust only
  dotnet        Build .NET only
  check-rust    Rust quality: fmt, clippy, test, buf lint, cargo deny
  check-dotnet  .NET quality: format, build, test
  proto         Run buf lint
  help          Show this help

Examples:
  ./build.ps1              # Build everything
  ./build.ps1 check        # Run all quality checks before pushing
  ./build.ps1 check-dotnet # Quick .NET-only check

"@
}

switch ($Command) {
    'build'        { Build-All }
    'check'        { Check-All }
    'rust'         { Build-Rust }
    'dotnet'       { Build-Dotnet }
    'check-rust'   { Check-Rust }
    'check-dotnet' { Check-Dotnet }
    'proto'        { Check-Proto }
    'help'         { Show-Help }
}

Write-Host "`nDone." -ForegroundColor Green

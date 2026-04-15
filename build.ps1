#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Cross-platform build and quality script for QsoRipper.

.DESCRIPTION
    Orchestrates Rust, .NET, and proto builds and quality checks.
    Mirrors the CI workflows so issues are caught locally before push.

.PARAMETER Command
    The build command to run. Default: build

.PARAMETER Configuration
    Build configuration for build and .NET validation commands. Default: Release

.EXAMPLE
    ./build.ps1              # Build all projects
    ./build.ps1 -Configuration Debug
    ./build.ps1 check        # Full CI-equivalent quality check
    ./build.ps1 check-rust   # Rust quality only
    ./build.ps1 check-dotnet # .NET quality only
#>

param(
    [Parameter(Position = 0)]
    [ValidateSet('build', 'check', 'rust', 'dotnet', 'check-rust', 'check-dotnet', 'proto', 'help')]
    [string]$Command = 'build',

    [ValidateSet('Release', 'Debug')]
    [string]$Configuration = 'Release'
)

$ErrorActionPreference = 'Stop'

$RustManifest = Join-Path $PSScriptRoot 'src' 'rust' 'Cargo.toml'
$DotnetSolution = Join-Path $PSScriptRoot 'src' 'dotnet' 'QsoRipper.slnx'
$DotnetCliProject = Join-Path $PSScriptRoot 'src' 'dotnet' 'QsoRipper.Cli' 'QsoRipper.Cli.csproj'
$DotnetGuiProject = Join-Path $PSScriptRoot 'src' 'dotnet' 'QsoRipper.Gui' 'QsoRipper.Gui.csproj'
$DotnetCliPublishDir = Join-Path $PSScriptRoot 'artifacts' 'publish' | Join-Path -ChildPath 'QsoRipper.Cli' | Join-Path -ChildPath $Configuration
$DotnetGuiPublishDir = Join-Path $PSScriptRoot 'artifacts' 'publish' | Join-Path -ChildPath 'QsoRipper.Gui' | Join-Path -ChildPath $Configuration
$TuiPublishDir = Join-Path $PSScriptRoot 'artifacts' 'publish' | Join-Path -ChildPath 'qsoripper-tui' | Join-Path -ChildPath $Configuration
$RustDir = Join-Path $PSScriptRoot 'src' 'rust'
$IsReleaseBuild = $Configuration -eq 'Release'
$RustTargetProfile = if ($IsReleaseBuild) { 'release' } else { 'debug' }
$TuiBinary = if ($IsWindows) { 'qsoripper-tui.exe' } else { 'qsoripper-tui' }

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
    $arguments = @('build', '--manifest-path', $RustManifest)
    if ($IsReleaseBuild) {
        $arguments += '--release'
    }

    Invoke-Build "Building Rust ($Configuration)" cargo $arguments

    $tuiSrc = Join-Path $PSScriptRoot 'src' 'rust' 'target' $RustTargetProfile $TuiBinary
    if (Test-Path $tuiSrc) {
        Write-Step "Publishing qsoripper-tui ($Configuration)"
        $null = New-Item -ItemType Directory -Force -Path $TuiPublishDir
        Copy-Item -Path $tuiSrc -Destination $TuiPublishDir -Force
        Write-Host "  -> $TuiPublishDir"
    }
}

function Find-VcVarsAll {
    $vswherePath = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio' 'Installer' 'vswhere.exe'
    if (-not (Test-Path $vswherePath)) {
        return $null
    }

    # Try vswhere first (standard detection used by ILCompiler)
    $vsPath = & $vswherePath -latest -prerelease -products * `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
        -property installationPath 2>$null
    if ($vsPath) {
        $vcvars = Join-Path $vsPath 'VC' 'Auxiliary' 'Build' 'vcvarsall.bat'
        if (Test-Path $vcvars) {
            return $vcvars
        }
    }

    # Fallback: scan all VS installations for vcvarsall.bat
    $allPaths = & $vswherePath -all -products * -property installationPath 2>$null
    foreach ($path in $allPaths) {
        $vcvars = Join-Path $path 'VC' 'Auxiliary' 'Build' 'vcvarsall.bat'
        if (Test-Path $vcvars) {
            return $vcvars
        }
    }

    return $null
}

function Build-Dotnet {
    # Native AOT requires the MSVC linker. ILCompiler's findvcvarsall.bat uses
    # vswhere to locate it, but some VS installations (e.g., VS 18 BuildTools)
    # may not register VC.Tools correctly. When that happens, set up the VC
    # environment manually and pass IlcUseEnvironmentalTools=true.
    $vcvarsAll = Find-VcVarsAll
    $needsVcEnv = $false
    $extraPublishArgs = @()

    if ($vcvarsAll) {
        # Test if ILCompiler's own detection works
        $ilcFindScript = Join-Path $env:USERPROFILE '.nuget' 'packages' 'microsoft.dotnet.ilcompiler' '*' 'build' 'findvcvarsall.bat' |
            Resolve-Path -ErrorAction SilentlyContinue |
            Sort-Object -Descending |
            Select-Object -First 1

        if ($ilcFindScript) {
            $testResult = cmd /c "`"$($ilcFindScript.Path)`" x64 >nul 2>&1 && echo OK" 2>$null
            if ($testResult -ne 'OK') {
                Write-Host "  ILCompiler cannot find the platform linker via vswhere." -ForegroundColor Yellow
                Write-Host "  Using vcvarsall.bat workaround: $vcvarsAll" -ForegroundColor Yellow
                $needsVcEnv = $true
                $extraPublishArgs = @('-p:IlcUseEnvironmentalTools=true')
            }
        }
    }

    $publishArgs = @(
        'publish',
        $DotnetCliProject,
        '-c',
        $Configuration,
        '--use-current-runtime',
        '-o',
        $DotnetCliPublishDir
    ) + $extraPublishArgs

    if ($needsVcEnv) {
        $arch = if ([System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture -eq 'Arm64') { 'arm64' } else { 'amd64' }
        Write-Step "Publishing QsoRipper.Cli Native AOT ($Configuration)"
        cmd /c "call `"$vcvarsAll`" $arch >nul 2>&1 && dotnet $($publishArgs -join ' ')"
        if ($LASTEXITCODE -ne 0) {
            Write-Host "FAILED: Publishing QsoRipper.Cli Native AOT ($Configuration)" -ForegroundColor Red
            exit $LASTEXITCODE
        }
    }
    else {
        Invoke-Build "Publishing QsoRipper.Cli Native AOT ($Configuration)" dotnet $publishArgs
    }

    Invoke-Build "Publishing QsoRipper.Gui ($Configuration)" dotnet @(
        'publish',
        $DotnetGuiProject,
        '-c',
        $Configuration,
        '--use-current-runtime',
        '-o',
        $DotnetGuiPublishDir
    )
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
    Invoke-Build ".NET build ($Configuration)" dotnet @('build', $DotnetSolution, '-c', $Configuration)
    Invoke-Build ".NET tests ($Configuration)" dotnet @('test', $DotnetSolution, '-c', $Configuration, '--no-build')
}

function Check-All {
    Check-Rust
    Check-Dotnet
}

function Show-Help {
    Write-Host @"

QsoRipper Build Script

Usage: ./build.ps1 [command] [-Configuration Release|Debug]

Commands:
  build         Build Rust (including qsoripper-tui) and publish the CLI and GUI apps (default: Release)
  check         Full CI-equivalent quality check
  rust          Build Rust only (copies qsoripper-tui binary to artifacts)
  dotnet        Publish the CLI and GUI apps only
  check-rust    Rust quality: fmt, clippy, test, buf lint, cargo deny
  check-dotnet  .NET quality: format, build, test
  proto         Run buf lint
  help          Show this help

Examples:
  ./build.ps1                                 # Build Rust and publish the CLI and GUI apps in Release
  ./build.ps1 -Configuration Debug            # Build Rust and publish the CLI and GUI apps in Debug
  ./build.ps1 check                           # Run all quality checks before pushing
  ./build.ps1 check-dotnet -Configuration Debug

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

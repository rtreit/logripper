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
    [ValidateSet('build', 'check', 'rust', 'dotnet', 'win32', 'check-rust', 'check-dotnet', 'proto', 'help')]
    [string]$Command = 'build',

    [ValidateSet('Release', 'Debug')]
    [string]$Configuration = 'Release'
)

$ErrorActionPreference = 'Stop'

$RustManifest = Join-Path $PSScriptRoot 'src' 'rust' 'Cargo.toml'
$DotnetSolution = Join-Path $PSScriptRoot 'src' 'dotnet' 'QsoRipper.slnx'
$DotnetCliProject = Join-Path $PSScriptRoot 'src' 'dotnet' 'QsoRipper.Cli' 'QsoRipper.Cli.csproj'
$DotnetGuiProject = Join-Path $PSScriptRoot 'src' 'dotnet' 'QsoRipper.Gui' 'QsoRipper.Gui.csproj'
$DotnetCliPublishDir = Join-Path $PSScriptRoot 'artifacts' 'publish' | Join-Path -ChildPath 'qsoripper-cli' | Join-Path -ChildPath $Configuration
$DotnetGuiPublishDir = Join-Path $PSScriptRoot 'artifacts' 'publish' | Join-Path -ChildPath 'qsoripper-gui' | Join-Path -ChildPath $Configuration
$TuiPublishDir = Join-Path $PSScriptRoot 'artifacts' 'publish' | Join-Path -ChildPath 'qsoripper-tui' | Join-Path -ChildPath $Configuration
$StressTuiPublishDir = Join-Path $PSScriptRoot 'artifacts' 'publish' | Join-Path -ChildPath 'qsoripper-stress-tui' | Join-Path -ChildPath $Configuration
$RustDir = Join-Path $PSScriptRoot 'src' 'rust'
$IsReleaseBuild = $Configuration -eq 'Release'
$RustTargetProfile = if ($IsReleaseBuild) { 'release' } else { 'debug' }
$TuiBinary = if ($IsWindows) { 'qsoripper-tui.exe' } else { 'qsoripper-tui' }
$StressTuiBinary = if ($IsWindows) { 'qsoripper-stress-tui.exe' } else { 'qsoripper-stress-tui' }

function Write-Step([string]$Message) {
    Write-Host "`n=== $Message ===" -ForegroundColor Cyan
}

function Get-CppcheckInstallHint {
    if ($IsWindows) {
        return 'Install with: winget install Cppcheck.Cppcheck'
    }

    if ($IsMacOS) {
        return 'Install with: brew install cppcheck'
    }

    if ($IsLinux) {
        return 'Install with: sudo apt install cppcheck'
    }

    return 'Install from https://cppcheck.sourceforge.io/'
}

function Invoke-Build([string]$Step, [string]$Command, [string[]]$Arguments) {
    Write-Step $Step
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: $Step" -ForegroundColor Red
        exit $LASTEXITCODE
    }
}

$Win32SourceDir = Join-Path $PSScriptRoot 'src' 'c' 'qsoripper-win32'
$Win32Source = Join-Path $Win32SourceDir 'src' 'main.c'
$Win32PublishDir = Join-Path $PSScriptRoot 'artifacts' 'publish' | Join-Path -ChildPath 'qsoripper-win32' | Join-Path -ChildPath $Configuration

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

    $stressTuiSrc = Join-Path $PSScriptRoot 'src' 'rust' 'target' $RustTargetProfile $StressTuiBinary
    if (Test-Path $stressTuiSrc) {
        Write-Step "Publishing qsoripper-stress-tui ($Configuration)"
        $null = New-Item -ItemType Directory -Force -Path $StressTuiPublishDir
        Copy-Item -Path $stressTuiSrc -Destination $StressTuiPublishDir -Force
        Write-Host "  -> $StressTuiPublishDir"
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

function Build-Win32 {
    if (-not (Test-Path $Win32Source)) {
        Write-Step 'Win32 GUI'
        Write-Host 'Win32 source not found, skipping.' -ForegroundColor Yellow
        return
    }

    $vcvars = Find-VcVarsAll
    if (-not $vcvars) {
        Write-Step 'Win32 GUI'
        Write-Host 'MSVC toolchain not found, skipping Win32 build. Install the C++ Desktop workload.' -ForegroundColor Yellow
        return
    }

    # cppcheck static analysis — fails the build on error-severity findings
    Write-Step 'Win32 static analysis (cppcheck, optional)'
    $cppcheckExe = Get-Command cppcheck -ErrorAction SilentlyContinue
    if ($cppcheckExe) {
        cppcheck --enable=warning,performance,portability `
                 --error-exitcode=1 `
                 --std=c11 `
                 --suppress=missingIncludeSystem `
                 --suppress=missingInclude `
                 --inline-suppr `
                 $Win32Source
        if ($LASTEXITCODE -ne 0) {
            Write-Host 'FAILED: cppcheck found errors' -ForegroundColor Red
            exit $LASTEXITCODE
        }
    }
    else {
        $installHint = Get-CppcheckInstallHint
        Write-Host "cppcheck not found; continuing without optional Win32 static analysis. $installHint" -ForegroundColor Yellow
    }

    Write-Step "Building qsoripper-win32 ($Configuration)"
    $null = New-Item -ItemType Directory -Force -Path $Win32PublishDir
    $optFlags = if ($IsReleaseBuild) { '/O2' } else { '/Od /Zi' }
    $exe = Join-Path $Win32PublishDir 'qsoripper-win32.exe'

    $arch = if ([System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture -eq 'Arm64') { 'arm64' } else { 'amd64' }
    $buildScript = Join-Path $Win32PublishDir '_build.cmd'
    @"
@echo off
call "$vcvars" $arch >nul 2>&1
cl /W4 /WX /analyze $optFlags /DUNICODE /D_UNICODE "$Win32Source" /Fe:"$exe" /link user32.lib gdi32.lib shell32.lib comctl32.lib
"@ | Set-Content -LiteralPath $buildScript -Encoding ASCII

    Push-Location $Win32PublishDir
    try {
        cmd /c $buildScript
        if ($LASTEXITCODE -ne 0) {
            Write-Host "FAILED: Building qsoripper-win32 ($Configuration)" -ForegroundColor Red
            exit $LASTEXITCODE
        }
    }
    finally {
        Pop-Location
    }

    # Clean intermediate files
    Remove-Item (Join-Path $Win32PublishDir '*.obj') -Force -ErrorAction SilentlyContinue
    Remove-Item (Join-Path $Win32PublishDir '*.pft') -Force -ErrorAction SilentlyContinue
    Remove-Item $buildScript -Force -ErrorAction SilentlyContinue

    Write-Host "  -> $Win32PublishDir"
}

function Build-All {
    Build-Rust
    Build-Dotnet
    Build-Win32
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

    # Coverage threshold check (matches CI's 80% minimum)
    $llvmCovCmd = Get-Command cargo-llvm-cov -ErrorAction SilentlyContinue
    if ($llvmCovCmd) {
        Invoke-Build 'Rust tests (with coverage)' cargo @(
            'llvm-cov', '--manifest-path', $RustManifest,
            '--workspace',
            '--exclude', 'qsoripper-stress',
            '--exclude', 'qsoripper-stress-tui'
        )

        Write-Step 'Rust coverage threshold'
        $summaryPath = Join-Path ([System.IO.Path]::GetTempPath()) "qsoripper-rust-cov-$PID.json"
        try {
            cargo llvm-cov report `
                --manifest-path $RustManifest `
                --ignore-filename-regex 'qsoripper-stress' `
                --json --summary-only `
                --output-path $summaryPath
            if ($LASTEXITCODE -ne 0) {
                Write-Host "FAILED: Generating Rust coverage report" -ForegroundColor Red
                exit $LASTEXITCODE
            }
            $summary = Get-Content $summaryPath -Raw | ConvertFrom-Json
            $lineCoverage = [double]$summary.data[0].totals.lines.percent
            $covered = $summary.data[0].totals.lines.covered
            $total = $summary.data[0].totals.lines.count
            $threshold = 80.0
            Write-Host "Rust line coverage: $([math]::Round($lineCoverage, 2))% ($covered/$total lines)  (threshold: ${threshold}%)"
            if ($lineCoverage -lt $threshold) {
                Write-Host "FAIL: Rust line coverage $([math]::Round($lineCoverage, 2))% is below the minimum threshold of ${threshold}%" -ForegroundColor Red
                exit 1
            }
            Write-Host "PASS: Rust coverage threshold met." -ForegroundColor Green
        }
        finally {
            Remove-Item $summaryPath -ErrorAction SilentlyContinue
        }
    }
    else {
        Write-Step 'Rust tests'
        Write-Host 'cargo-llvm-cov not installed, running tests without coverage. Install with: cargo install cargo-llvm-cov' -ForegroundColor Yellow
        Invoke-Build 'Rust tests (no coverage)' cargo @('test', '--manifest-path', $RustManifest)
    }

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

    # Run tests with coverage collection (matches CI)
    $coverageDir = Join-Path $PSScriptRoot 'coverage'
    if (Test-Path $coverageDir) { Remove-Item $coverageDir -Recurse -Force }

    $runsettings = Join-Path $PSScriptRoot 'src' 'dotnet' 'CodeCoverage.runsettings'
    Invoke-Build ".NET tests with coverage ($Configuration)" dotnet @(
        'test', $DotnetSolution, '-c', $Configuration, '--no-build',
        '--collect:XPlat Code Coverage',
        "--settings:$runsettings",
        "--results-directory:$coverageDir"
    )

    # Coverage threshold check (matches CI's 50% minimum)
    Write-Step '.NET coverage threshold'
    $coberturaFiles = Get-ChildItem $coverageDir -Filter 'coverage.cobertura.xml' -Recurse -ErrorAction SilentlyContinue
    if ($coberturaFiles) {
        $totalCovered = 0
        $totalLines = 0
        foreach ($file in $coberturaFiles) {
            [xml]$cobertura = Get-Content $file.FullName
            $lineRate = [double]$cobertura.coverage.'line-rate'
            $linesValid = [int]$cobertura.coverage.'lines-valid'
            $totalCovered += [math]::Round($lineRate * $linesValid)
            $totalLines += $linesValid
        }
        $coverage = if ($totalLines -gt 0) { [math]::Round(($totalCovered / $totalLines) * 100, 1) } else { 0 }
        $threshold = 50.0
        Write-Host ".NET line coverage: ${coverage}% ($totalCovered/$totalLines lines)  (threshold: ${threshold}%)"
        if ($coverage -lt $threshold) {
            Write-Host "FAIL: .NET line coverage ${coverage}% is below the minimum threshold of ${threshold}%" -ForegroundColor Red
            exit 1
        }
        Write-Host "PASS: .NET coverage threshold met." -ForegroundColor Green
    }
    else {
        Write-Host 'WARNING: No Cobertura coverage files found. Coverage threshold not checked.' -ForegroundColor Yellow
    }

    # Vulnerable package check (matches CI)
    Write-Step 'Vulnerable package check'
    $vulnOutput = dotnet list $DotnetSolution package --vulnerable --include-transitive 2>&1 | Out-String
    Write-Host $vulnOutput
    if ($vulnOutput -match 'has the following vulnerable packages') {
        Write-Host "FAIL: Vulnerable NuGet packages detected. Review output above and update affected packages." -ForegroundColor Red
        exit 1
    }
    Write-Host "PASS: No vulnerable packages found." -ForegroundColor Green
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
  build         Build Rust, .NET, and Win32 apps (default: Release)
  check         Full CI-equivalent quality check
  rust          Build Rust only (copies qsoripper-tui and qsoripper-stress-tui binaries to artifacts)
  dotnet        Publish the CLI and GUI apps only
  win32         Build the Win32 C GUI app only
  check-rust    Rust quality: fmt, clippy, test + coverage threshold, buf lint, cargo deny
  check-dotnet  .NET quality: format, build, test + coverage threshold, vulnerable package check
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
    'win32'        { Build-Win32 }
    'check-rust'   { Check-Rust }
    'check-dotnet' { Check-Dotnet }
    'proto'        { Check-Proto }
    'help'         { Show-Help }
}

Write-Host "`nDone." -ForegroundColor Green

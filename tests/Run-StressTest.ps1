<#
.SYNOPSIS
    Runs the QsoRipper adversarial stress test suite and generates an HTML report.

.DESCRIPTION
    Builds the Rust server and C# stress client, starts the server with in-memory
    storage, runs both the Rust in-process stress test and the C# gRPC stress client,
    captures server stderr for panic backtraces, and produces an HTML report.

.PARAMETER Parallelism
    Number of concurrent gRPC tasks for the C# stress client. Default: 100.

.PARAMETER DurationSeconds
    How long the C# stress client runs. Default: 15.

.PARAMETER ServerPort
    Port for the gRPC server. Default: 50051.

.PARAMETER OutputPath
    Path for the HTML report. Default: stress-report.html in the repo root.

.EXAMPLE
    .\Run-StressTest.ps1
    .\Run-StressTest.ps1 -Parallelism 200 -DurationSeconds 30
#>

[CmdletBinding()]
param(
    [int]$Parallelism = 100,
    [int]$DurationSeconds = 15,
    [int]$ServerPort = 50051,
    [string]$OutputPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $OutputPath) {
    $OutputPath = Join-Path $repoRoot 'stress-report.html'
}

$rustDir = Join-Path $repoRoot 'src' 'rust'
$dotnetDir = Join-Path $repoRoot 'src' 'dotnet' 'stress-client'
$serverStderr = Join-Path ([System.IO.Path]::GetTempPath()) "qsoripper-stress-server-$PID.log"
$rustTestOutput = Join-Path ([System.IO.Path]::GetTempPath()) "qsoripper-stress-rust-$PID.log"
$dotnetOutput = Join-Path ([System.IO.Path]::GetTempPath()) "qsoripper-stress-dotnet-$PID.log"

function Write-Step([string]$message) {
    Write-Host "`n>> $message" -ForegroundColor Cyan
}

try {
    # ---- Build ----
    Write-Step 'Building Rust server and stress test'
    Push-Location $rustDir
    cargo build -p qsoripper-server 2>&1 | Out-Null
    cargo test --test stress_test --no-run 2>&1 | Out-Null
    Pop-Location

    Write-Step 'Building C# stress client'
    Push-Location $dotnetDir
    dotnet build --nologo -v quiet 2>&1 | Out-Null
    Pop-Location

    # ---- Run Rust in-process stress test ----
    Write-Step 'Running Rust in-process stress test'
    Push-Location $rustDir
    $env:RUST_BACKTRACE = '1'
    cargo test --test stress_test -- --nocapture *> $rustTestOutput 2>&1
    $rustExitCode = $LASTEXITCODE
    Pop-Location

    $rustOutput = Get-Content $rustTestOutput -Raw -ErrorAction SilentlyContinue
    $rustPassed = $rustExitCode -eq 0

    # ---- Start server ----
    Write-Step "Starting qsoripper-server on port $ServerPort (in-memory storage)"
    $env:RUST_BACKTRACE = '1'
    $serverExe = Join-Path $rustDir 'target' 'debug' 'qsoripper-server.exe'
    if (-not (Test-Path $serverExe)) {
        $serverExe = Join-Path $rustDir 'target' 'debug' 'qsoripper-server'
    }

    $serverProcess = Start-Process -FilePath $serverExe `
        -ArgumentList '--storage', 'memory', '--listen', "127.0.0.1:$ServerPort" `
        -RedirectStandardError $serverStderr `
        -PassThru -NoNewWindow

    Start-Sleep -Seconds 3
    if ($serverProcess.HasExited) {
        throw "Server exited immediately with code $($serverProcess.ExitCode)"
    }
    Write-Host "  Server PID: $($serverProcess.Id)"

    # ---- Run C# stress client ----
    Write-Step "Running C# stress client ($Parallelism tasks, ${DurationSeconds}s)"
    Push-Location $dotnetDir
    dotnet run --no-build -- "http://localhost:$ServerPort" $Parallelism $DurationSeconds *> $dotnetOutput 2>&1
    $dotnetExitCode = $LASTEXITCODE
    Pop-Location

    $dotnetOutputText = Get-Content $dotnetOutput -Raw -ErrorAction SilentlyContinue

    # ---- Stop server ----
    Write-Step 'Stopping server'
    if (-not $serverProcess.HasExited) {
        Stop-Process -Id $serverProcess.Id -Force -ErrorAction SilentlyContinue
        $serverProcess.WaitForExit(5000) | Out-Null
    }
    $serverCrashed = $serverProcess.ExitCode -ne 0 -and $serverProcess.ExitCode -ne -1

    # ---- Parse server panics ----
    $serverLog = Get-Content $serverStderr -Raw -ErrorAction SilentlyContinue
    $panicMatches = [regex]::Matches($serverLog, "thread '([^']+)' panicked at ([^,\n]+):?\n([^\n]+)")
    $serverPanics = @()
    foreach ($m in $panicMatches) {
        $serverPanics += [PSCustomObject]@{
            Thread   = $m.Groups[1].Value
            Location = $m.Groups[2].Value
            Message  = $m.Groups[3].Value
        }
    }
    $uniquePanics = $serverPanics | Sort-Object Location, Message -Unique

    # ---- Parse C# output ----
    $grpcInternalCount = 0
    if ($dotnetOutputText -match 'INTERNAL errors:\s+(\d+)') {
        $grpcInternalCount = [int]$Matches[1]
    }
    $totalCalls = 0
    if ($dotnetOutputText -match 'Total calls:\s+([\d,]+)') {
        $totalCalls = $Matches[1]
    }

    # ---- Generate HTML ----
    Write-Step "Generating HTML report at $OutputPath"

    $timestamp = Get-Date -Format 'yyyy-MM-dd HH:mm:ss'
    $panicCount = $uniquePanics.Count
    $totalPanicHits = $serverPanics.Count
    $statusColor = if ($panicCount -gt 0) { '#dc3545' } else { '#28a745' }
    $statusText = if ($panicCount -gt 0) { "PANICS FOUND ($panicCount unique)" } else { 'ALL CLEAR' }

    $panicRows = ''
    if ($panicCount -gt 0) {
        $i = 0
        foreach ($p in $uniquePanics) {
            $i++
            $escapedMsg = [System.Web.HttpUtility]::HtmlEncode($p.Message)
            $escapedLoc = [System.Web.HttpUtility]::HtmlEncode($p.Location)
            $escapedThread = [System.Web.HttpUtility]::HtmlEncode($p.Thread)
            $panicRows += @"
            <tr>
                <td>$i</td>
                <td><code>$escapedLoc</code></td>
                <td>$escapedMsg</td>
                <td>$escapedThread</td>
            </tr>
"@
        }
    }

    $rustStatusColor = if ($rustPassed) { '#28a745' } else { '#dc3545' }
    $rustStatusText = if ($rustPassed) { 'PASSED (0 panics)' } else { 'FAILED' }

    $html = @"
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>QsoRipper Stress Test Report</title>
<style>
    body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; margin: 2rem; background: #f8f9fa; color: #212529; }
    h1 { margin-bottom: 0.25rem; }
    .timestamp { color: #6c757d; margin-bottom: 1.5rem; }
    .status-badge { display: inline-block; padding: 0.4rem 1rem; border-radius: 4px; color: white; font-weight: bold; font-size: 1.1rem; }
    .section { background: white; border: 1px solid #dee2e6; border-radius: 6px; padding: 1.5rem; margin-bottom: 1.5rem; }
    .section h2 { margin-top: 0; }
    table { border-collapse: collapse; width: 100%; }
    th, td { text-align: left; padding: 0.5rem 0.75rem; border-bottom: 1px solid #dee2e6; }
    th { background: #e9ecef; }
    .metric { display: inline-block; margin-right: 2rem; }
    .metric-value { font-size: 1.5rem; font-weight: bold; }
    .metric-label { color: #6c757d; font-size: 0.85rem; }
    code { background: #e9ecef; padding: 0.15rem 0.4rem; border-radius: 3px; font-size: 0.9rem; }
    pre { background: #1e1e1e; color: #d4d4d4; padding: 1rem; border-radius: 6px; overflow-x: auto; font-size: 0.8rem; max-height: 400px; }
</style>
</head>
<body>
<h1>QsoRipper Stress Test Report</h1>
<div class="timestamp">$timestamp</div>

<div class="section">
    <h2>Overall Status</h2>
    <span class="status-badge" style="background: $statusColor;">$statusText</span>
    <div style="margin-top: 1rem;">
        <div class="metric"><div class="metric-value">$totalCalls</div><div class="metric-label">gRPC calls</div></div>
        <div class="metric"><div class="metric-value">$grpcInternalCount</div><div class="metric-label">INTERNAL errors</div></div>
        <div class="metric"><div class="metric-value">$totalPanicHits</div><div class="metric-label">server panics (total hits)</div></div>
        <div class="metric"><div class="metric-value">$panicCount</div><div class="metric-label">unique panic sites</div></div>
    </div>
</div>

<div class="section">
    <h2>Configuration</h2>
    <table>
        <tr><td>Parallelism</td><td>$Parallelism concurrent tasks</td></tr>
        <tr><td>Duration</td><td>${DurationSeconds}s</td></tr>
        <tr><td>Server port</td><td>$ServerPort</td></tr>
        <tr><td>Storage backend</td><td>memory</td></tr>
        <tr><td>RUST_BACKTRACE</td><td>1</td></tr>
    </table>
</div>

<div class="section">
    <h2>Layer 1: Rust In-Process Test</h2>
    <span class="status-badge" style="background: $rustStatusColor;">$rustStatusText</span>
    <p style="margin-top: 0.5rem;">Calls engine library functions directly with adversarial inputs (ADIF fuzzing, QSO chaos, concurrent lookups, FFI abuse, band/mode parsing).</p>
</div>

<div class="section">
    <h2>Layer 2: C# gRPC Stress Client</h2>
    <p>Fires $Parallelism parallel tasks for ${DurationSeconds}s against the running server.</p>
$(if ($panicCount -gt 0) {
@"
    <h3>Server Panics Detected</h3>
    <table>
        <thead><tr><th>#</th><th>Location</th><th>Message</th><th>Thread</th></tr></thead>
        <tbody>
$panicRows
        </tbody>
    </table>
"@
} else {
    '<p>No server panics detected.</p>'
})
</div>

$(if ($serverLog -and $panicCount -gt 0) {
    $escapedLog = [System.Web.HttpUtility]::HtmlEncode($serverLog)
    if ($escapedLog.Length -gt 50000) { $escapedLog = $escapedLog.Substring(0, 50000) + "`n... (truncated)" }
@"
<div class="section">
    <h2>Server Panic Backtraces</h2>
    <pre>$escapedLog</pre>
</div>
"@
})

<div class="section">
    <h2>C# Client Output</h2>
    <pre>$([System.Web.HttpUtility]::HtmlEncode($dotnetOutputText))</pre>
</div>

</body>
</html>
"@

    $html | Out-File -FilePath $OutputPath -Encoding utf8

    Write-Host "`nReport written to: $OutputPath" -ForegroundColor Green

    if ($panicCount -gt 0) {
        Write-Host "`nFound $panicCount unique panic site(s) with $totalPanicHits total hits:" -ForegroundColor Red
        foreach ($p in $uniquePanics) {
            Write-Host "  $($p.Location): $($p.Message)" -ForegroundColor Red
        }
        exit 1
    } else {
        Write-Host "`nNo panics found. The engine held up." -ForegroundColor Green
        exit 0
    }
}
finally {
    if ($serverProcess -and -not $serverProcess.HasExited) {
        Stop-Process -Id $serverProcess.Id -Force -ErrorAction SilentlyContinue
    }

    Remove-Item $serverStderr -ErrorAction SilentlyContinue
    Remove-Item $rustTestOutput -ErrorAction SilentlyContinue
    Remove-Item $dotnetOutput -ErrorAction SilentlyContinue
}

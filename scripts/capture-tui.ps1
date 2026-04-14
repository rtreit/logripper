[CmdletBinding()]
param(
    [string]$Scenario = "cli-help",
    [string]$Command,
    [string]$Output,
    [int]$Columns = 100,
    [int]$Rows = 30,
    [string]$Theme = "material-dark"
)

$ErrorActionPreference = "Stop"

if (-not $IsWindows) {
    throw "scripts\capture-tui.ps1 currently supports Windows only."
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$currentRoot = Join-Path $repoRoot "artifacts\ux\current"
$baselineRoot = Join-Path $repoRoot "artifacts\ux\baseline"
$diffRoot = Join-Path $repoRoot "artifacts\ux\diff"

New-Item -ItemType Directory -Force -Path $currentRoot, $baselineRoot, $diffRoot | Out-Null

if ([string]::IsNullOrWhiteSpace($Output)) {
    $Output = Join-Path $currentRoot "$Scenario.gif"
}

$Output = [System.IO.Path]::GetFullPath($Output)
$outputDirectory = [System.IO.Path]::GetDirectoryName($Output)
if (-not [string]::IsNullOrWhiteSpace($outputDirectory)) {
    New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
}
$outputStem = [System.IO.Path]::GetFileNameWithoutExtension($Output)
$yamlBase = [System.IO.Path]::Combine($outputDirectory, $outputStem)
$yamlPath = "$yamlBase.yml"
$transcriptPath = [System.IO.Path]::ChangeExtension($Output, ".txt")
$summaryPath = [System.IO.Path]::ChangeExtension($Output, ".json")
$reportPath = Join-Path $currentRoot "report.json"

$resolvedCommand = if (-not [string]::IsNullOrWhiteSpace($Command)) {
    $Command
}
else {
    switch ($Scenario) {
        "cli-help" { "dotnet run --project src\dotnet\QsoRipper.Cli -- --help" }
        "cli-setup-help" { "dotnet run --project src\dotnet\QsoRipper.Cli -- setup --help" }
        default { throw "Unknown terminal scenario '$Scenario'. Supply -Command for custom capture." }
    }
}

$configPath = Join-Path ([System.IO.Path]::GetTempPath()) ("qsoripper-terminalizer-{0}.yml" -f [Guid]::NewGuid().ToString("N"))
$runnerPath = Join-Path ([System.IO.Path]::GetTempPath()) ("qsoripper-terminalizer-run-{0}.ps1" -f [Guid]::NewGuid().ToString("N"))
$pwshPath = (Get-Command pwsh -ErrorAction SilentlyContinue)?.Source
if ([string]::IsNullOrWhiteSpace($pwshPath)) {
    throw "PowerShell 7 (pwsh) is required for terminal capture."
}

$themeBlock = switch ($Theme.ToLowerInvariant()) {
    "material-dark" {
@"
theme:
  background: "#263238"
  foreground: "#eeffff"
  cursor: "#ffcc00"
  black: "#000000"
  red: "#ff5370"
  green: "#c3e88d"
  yellow: "#ffcb6b"
  blue: "#82aaff"
  magenta: "#c792ea"
  cyan: "#89ddff"
  white: "#ffffff"
  brightBlack: "#546e7a"
  brightRed: "#ff5370"
  brightGreen: "#c3e88d"
  brightYellow: "#ffcb6b"
  brightBlue: "#82aaff"
  brightMagenta: "#c792ea"
  brightCyan: "#89ddff"
  brightWhite: "#ffffff"
"@
    }
    default {
@"
theme:
  background: "#1e1e1e"
  foreground: "#d4d4d4"
  cursor: "#ffffff"
  black: "#1e1e1e"
  red: "#f44747"
  green: "#608b4e"
  yellow: "#dcdcaa"
  blue: "#569cd6"
  magenta: "#c586c0"
  cyan: "#4ec9b0"
  white: "#d4d4d4"
  brightBlack: "#808080"
  brightRed: "#f44747"
  brightGreen: "#608b4e"
  brightYellow: "#dcdcaa"
  brightBlue: "#569cd6"
  brightMagenta: "#c586c0"
  brightCyan: "#4ec9b0"
  brightWhite: "#ffffff"
"@
    }
}

$config = @"
command: null
cwd: null
env:
  recording: true
cols: $Columns
rows: $Rows
repeat: 0
quality: 100
frameDelay: auto
maxIdleTime: 2000
frameBox:
  type: floating
  title: QsoRipper
  style:
    border: 0px black solid
watermark:
  imagePath: null
  style:
    position: absolute
    right: 15px
    bottom: 15px
    width: 100px
    opacity: 0.9
cursorStyle: block
fontFamily: "Cascadia Mono, Consolas, Monaco, Lucida Console, Monospace"
fontSize: 14
lineHeight: 1
letterSpacing: 0
$themeBlock
"@

$runner = @"
`$ErrorActionPreference = 'Stop'
Set-Location '$($repoRoot.Replace("'", "''"))'
`$command = @'
$resolvedCommand
'@
Invoke-Expression `$command 2>&1 | Tee-Object -FilePath '$($transcriptPath.Replace("'", "''"))'
if (`$LASTEXITCODE -and `$LASTEXITCODE -ne 0) {
    exit `$LASTEXITCODE
}
Start-Sleep -Milliseconds 250
"@

Set-Content -LiteralPath $configPath -Value $config -NoNewline
Set-Content -LiteralPath $runnerPath -Value $runner -NoNewline

function Resolve-TerminalizerCommand {
    $runtimeRoot = Join-Path $repoRoot "tools\terminalizer-runtime"
    $bootstrapNode = Join-Path $repoRoot "tools\terminalizer-bootstrap\node_modules\node\bin\node.exe"
    $runtimeApp = Join-Path $runtimeRoot "node_modules\terminalizer\bin\app.js"
    if ((Test-Path $bootstrapNode) -and (Test-Path $runtimeApp)) {
        Repair-TerminalizerRuntime -RuntimeRoot $runtimeRoot
        return [PSCustomObject]@{
            Command = $bootstrapNode
            PrefixArgs = @($runtimeApp)
        }
    }

    return Install-TerminalizerRuntime
}

function Install-TerminalizerRuntime {
    $bootstrapRoot = Join-Path $repoRoot "tools\terminalizer-bootstrap"
    $runtimeRoot = Join-Path $repoRoot "tools\terminalizer-runtime"
    $bootstrapNode = Join-Path $bootstrapRoot "node_modules\node\bin\node.exe"
    $bootstrapNpmCli = Join-Path $bootstrapRoot "node_modules\npm\bin\npm-cli.js"
    $runtimeApp = Join-Path $runtimeRoot "node_modules\terminalizer\bin\app.js"

    if (-not ((Test-Path $bootstrapNode) -and (Test-Path $bootstrapNpmCli))) {
        New-Item -ItemType Directory -Force -Path $bootstrapRoot | Out-Null
        & npm install --prefix $bootstrapRoot node@22 npm@10 | Out-Host
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to bootstrap the local Node 22 runtime for Terminalizer."
        }
    }

    if (-not (Test-Path $runtimeApp)) {
        New-Item -ItemType Directory -Force -Path $runtimeRoot | Out-Null
        $originalPath = $env:Path
        try {
            $env:Path = "{0};{1}" -f (Split-Path -Parent $bootstrapNode), $originalPath
            & $bootstrapNode $bootstrapNpmCli install --prefix $runtimeRoot terminalizer@0.12.0 | Out-Host
            if ($LASTEXITCODE -ne 0) {
                throw "Failed to install Terminalizer into the local runtime."
            }
        }
        finally {
            $env:Path = $originalPath
        }
    }

    if (-not (Test-Path $runtimeApp)) {
        throw "Terminalizer bootstrap completed, but the runtime entry point was not found."
    }

    Repair-TerminalizerRuntime -RuntimeRoot $runtimeRoot

    return [PSCustomObject]@{
        Command = $bootstrapNode
        PrefixArgs = @($runtimeApp)
    }
}

function Repair-TerminalizerRuntime([string]$RuntimeRoot) {
    $recordCommandPath = Join-Path $RuntimeRoot "node_modules\terminalizer\commands\record.js"
    if (-not (Test-Path $recordCommandPath)) {
        throw "Terminalizer runtime was found, but '$recordCommandPath' is missing."
    }

    $content = Get-Content -LiteralPath $recordCommandPath -Raw
    $patched = [System.Text.RegularExpressions.Regex]::Replace(
        $content,
        '(?ms)if\s*\(argv\.skipSharing\)\s*\{.*?^\s*\}',
@'
if (argv.skipSharing) {
    process.exit(0);
    return;
  }
'@)

    if ($patched -ne $content) {
        Set-Content -LiteralPath $recordCommandPath -Value $patched -NoNewline
    }
}

function Normalize-TerminalizerRecording([string]$Path) {
    if (-not (Test-Path $Path)) {
        throw "Expected Terminalizer recording at '$Path', but it was not created."
    }

    $content = Get-Content -LiteralPath $Path -Raw
    $normalized = [System.Text.RegularExpressions.Regex]::Replace(
        $content,
        '(?m)^(\s*)command:.*$',
        '${1}command: null')

    if ($normalized -ne $content) {
        Set-Content -LiteralPath $Path -Value $normalized -NoNewline
    }
}

$terminalizer = Resolve-TerminalizerCommand
$terminalizerCommand = "`"$pwshPath`" -NoLogo -NoProfile -ExecutionPolicy Bypass -File `"$runnerPath`""

Push-Location $repoRoot
try {
    & $terminalizer.Command @($terminalizer.PrefixArgs + @("record", $yamlBase, "--config", $configPath, "--command", $terminalizerCommand, "--skip-sharing"))
    if ($LASTEXITCODE -ne 0) {
        throw "Terminalizer record failed with exit code $LASTEXITCODE."
    }

    Normalize-TerminalizerRecording -Path $yamlPath

    & $terminalizer.Command @($terminalizer.PrefixArgs + @("render", $yamlPath, "--output", $Output))
    if ($LASTEXITCODE -ne 0) {
        throw "Terminalizer render failed with exit code $LASTEXITCODE."
    }

    $summary = @{
        surface = "terminal"
        scenario = $Scenario
        command = $resolvedCommand
        gifPath = $Output
        yamlPath = $yamlPath
        transcriptPath = $transcriptPath
        columns = $Columns
        rows = $Rows
        theme = $Theme
        capturedAtUtc = (Get-Date).ToUniversalTime().ToString("o")
    }

    $summary | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $summaryPath
    $summary | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $reportPath

    Write-Host "Saved terminal capture to $Output"
    Write-Host "Transcript: $transcriptPath"
}
finally {
    Pop-Location
    Remove-Item -Force -ErrorAction SilentlyContinue $configPath, $runnerPath
}

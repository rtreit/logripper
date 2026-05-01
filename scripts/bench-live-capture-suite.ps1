# Live-capture regression harness for the cw-decoder V3 streaming path.
#
# Why this exists: bench-v3-clean.ps1 only scores against a synthetic
# truth file for one clean WAV. That doesn't catch the live-streaming
# failure modes the user hits on real radio audio (spurious lock on
# noise, stitching garbage into the session transcript, char-gap mis-
# classification, etc.). This harness replays a curated set of WAVs
# (clean synthesis + real captured radio) through `stream-live-v3
# --file` at real-time pace, captures the final session transcript,
# and scores it against the rock-solid whole-buffer `file` decode of
# the same WAV (the "gold" reference).
#
# The whole-buffer decoder isn't perfect on real radio — but it IS
# stable, has no streaming-specific failure modes, and gives us a
# fixed reference we can regression against. Live ought to be at least
# as good as gold (and ideally identical for clean inputs).
#
# Outputs a per-fixture table plus a summary so future PRs can detect
# regression in either direction.

[CmdletBinding()]
param(
    [string]$FixturesRoot = 'data\cw-samples\live-captures',
    [string[]]$ExtraWavs  = @('data\cw-samples\cw_30wpm_abbrev_clean.wav'),
    [int]$DecodeEveryMs   = 1000,
    [string]$BaselineFile = 'scripts\baselines\live-capture-suite.json',
    [switch]$UpdateBaseline,
    [switch]$JsonOnly
)

$ErrorActionPreference = 'Stop'
$root = Resolve-Path "$PSScriptRoot\.."
$bin  = Join-Path $root 'experiments\cw-decoder\target\release\cw-decoder.exe'
if (-not (Test-Path $bin)) { throw "Missing $bin (run cargo build --release in experiments\cw-decoder)" }

function Get-FinalTranscript {
    param([string]$NdjsonPath)
    $line = Get-Content $NdjsonPath | Where-Object { $_ -match '"type":"transcript"' } | Select-Object -Last 1
    if (-not $line) {
        $line = Get-Content $NdjsonPath | Where-Object { $_ -match '"type":"end"' } | Select-Object -Last 1
    }
    if (-not $line) { return '' }
    $obj = $line | ConvertFrom-Json
    if ($obj.PSObject.Properties['transcript'] -and $obj.transcript) { return "$($obj.transcript)" }
    if ($obj.PSObject.Properties['text']       -and $obj.text)       { return "$($obj.text)" }
    return ''
}

function Get-GoldDecode {
    param([string]$WavPath)
    $raw = & $bin file $WavPath 2>$null
    if ($LASTEXITCODE -ne 0) { return '' }
    # Output looks like:
    #   == decoded text ==
    #   <transcript line(s)>
    $idx = ($raw | Select-String -Pattern '== decoded text ==').LineNumber
    if (-not $idx) { return '' }
    return ($raw[$idx..($raw.Count - 1)] -join "`n").Trim()
}

function Get-LiveDecode {
    param([string]$WavPath, [int]$DecodeMs)
    $tmp = Join-Path $env:TEMP "live-cap-$([guid]::NewGuid().ToString('N')).ndjson"
    try {
        & $bin stream-live-v3 --json --decode-every-ms $DecodeMs --file $WavPath 1>$tmp 2>$null
        if ($LASTEXITCODE -ne 0) { return '' }
        return Get-FinalTranscript -NdjsonPath $tmp
    } finally {
        Remove-Item $tmp -Force -ErrorAction SilentlyContinue
    }
}

function Score-TokenRecall {
    # % of distinct >=2-char alpha tokens from `gold` that appear as
    # whole tokens (case-insensitive) in `live`. Two-char minimum
    # filters out the "E E E E" noise spam that inflates raw matches.
    param([string]$Gold, [string]$Live)
    $goldTokens = ($Gold -split '\s+') | Where-Object { $_ -match '^[A-Za-z0-9]{2,}$' } | ForEach-Object { $_.ToUpperInvariant() }
    if ($goldTokens.Count -eq 0) { return 0.0 }
    $liveTokens = ($Live -split '\s+') | Where-Object { $_ } | ForEach-Object { $_.ToUpperInvariant() }
    $liveSet = $liveTokens | Sort-Object -Unique
    $goldUnique = $goldTokens | Sort-Object -Unique
    $hits = 0
    foreach ($t in $goldUnique) { if ($liveSet -contains $t) { $hits++ } }
    return [math]::Round(100.0 * $hits / $goldUnique.Count, 1)
}

# Discover fixture WAVs.
$fixtures = New-Object System.Collections.Generic.List[string]
$fixturesAbs = Join-Path $root $FixturesRoot
if (Test-Path $fixturesAbs) {
    Get-ChildItem -Path $fixturesAbs -Filter '*.wav' | ForEach-Object { $fixtures.Add($_.FullName) }
}
foreach ($w in $ExtraWavs) {
    $abs = Join-Path $root $w
    if (Test-Path $abs) { $fixtures.Add($abs) }
}
if ($fixtures.Count -eq 0) { throw "No fixture WAVs found under $fixturesAbs or $ExtraWavs" }

$results = New-Object System.Collections.Generic.List[object]
foreach ($wav in $fixtures) {
    if (-not $JsonOnly) { Write-Host "Benching $([System.IO.Path]::GetFileName($wav))..." -ForegroundColor Cyan }
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $gold = Get-GoldDecode -WavPath $wav
    $goldMs = $sw.ElapsedMilliseconds
    $sw.Restart()
    $live = Get-LiveDecode -WavPath $wav -DecodeMs $DecodeEveryMs
    $liveMs = $sw.ElapsedMilliseconds
    $score = Score-TokenRecall -Gold $gold -Live $live
    $results.Add([pscustomobject]@{
        Fixture    = [System.IO.Path]::GetFileName($wav)
        GoldChars  = $gold.Length
        LiveChars  = $live.Length
        Recall_pct = $score
        GoldMs     = $goldMs
        LiveMs     = $liveMs
        GoldHead   = $gold.Substring(0, [Math]::Min(80, $gold.Length))
        LiveHead   = $live.Substring(0, [Math]::Min(80, $live.Length))
    }) | Out-Null
}

$summary = [pscustomobject]@{
    Commit         = (git rev-parse --short HEAD)
    Subject        = (git log -1 --pretty=%s)
    Fixtures       = $results.Count
    AvgRecall_pct  = [math]::Round((($results | Measure-Object -Property Recall_pct -Average).Average), 1)
    MinRecall_pct  = [math]::Round((($results | Measure-Object -Property Recall_pct -Minimum).Minimum), 1)
    Generated      = (Get-Date -Format 'o')
    Results        = $results
}

if ($UpdateBaseline) {
    $baselineAbs = Join-Path $root $BaselineFile
    New-Item -ItemType Directory -Force -Path (Split-Path $baselineAbs) | Out-Null
    $summary | ConvertTo-Json -Depth 6 | Set-Content -Path $baselineAbs -Encoding UTF8
    if (-not $JsonOnly) { Write-Host "Baseline written: $baselineAbs" -ForegroundColor Green }
}

if ($JsonOnly) {
    $summary | ConvertTo-Json -Depth 6
} else {
    $results | Format-Table -AutoSize | Out-String | Write-Host
    Write-Host "Avg recall vs gold: $($summary.AvgRecall_pct)%  Min: $($summary.MinRecall_pct)%" -ForegroundColor Yellow
}

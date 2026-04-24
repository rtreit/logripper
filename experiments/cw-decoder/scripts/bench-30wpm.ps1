# Run cw-decoder bench-latency across all 30wpm-abbrev variants and emit an aggregate table.
[CmdletBinding()]
param(
    [string]$Label = 'default',
    [double]$Purity = -1,
    [int]$WideBins = -1,
    [switch]$NoAutoThreshold,
    [double]$ForcePitchHz = 0,
    [int]$StableN = 5,
    [int]$ChunkMs = 100,
    [double]$Hysteresis = -1,
    [double]$MinGap = -1,
    [double]$MinPulse = -1
)

$ErrorActionPreference = 'Stop'
$root = Resolve-Path "$PSScriptRoot\..\..\.."
$bin = Join-Path $root 'experiments\cw-decoder\target\release\cw-decoder.exe'
if (-not (Test-Path $bin)) {
    Push-Location (Join-Path $root 'experiments\cw-decoder')
    cargo build --release --bin cw-decoder | Out-Null
    Pop-Location
}

$samples = Join-Path $root 'data\cw-samples'
$truth = (Get-Content (Join-Path $samples 'cw_30wpm_abbrev.truth.txt') -Raw).Trim()

$variants = @(
    'cw_30wpm_abbrev_clean.wav',
    'cw_30wpm_abbrev_weak.wav',
    'cw_30wpm_abbrev_qrn.wav',
    'cw_30wpm_abbrev_qsb.wav',
    'cw_30wpm_abbrev_weak_qsb.wav',
    'cw_30wpm_abbrev_extreme_qrn.wav',
    'cw_30wpm_abbrev_crushed.wav',
    'cw_30wpm_abbrev_deep_qsb.wav',
    'cw_30wpm_abbrev_buried.wav',
    'cw_30wpm_abbrev_harsh_white.wav',
    'cw_30wpm_abbrev_inband_qrm.wav',
    'cw_30wpm_abbrev_chaos.wav'
)

$results = @()
foreach ($v in $variants) {
    $path = Join-Path $samples $v
    $extra = @()
    if ($Purity -ge 0) { $extra += '--purity'; $extra += "$Purity" }
    if ($WideBins -ge 0) { $extra += '--wide-bins'; $extra += "$WideBins" }
    if ($NoAutoThreshold) { $extra += '--no-auto-threshold' }
    if ($ForcePitchHz -gt 0) { $extra += '--force-pitch-hz'; $extra += "$ForcePitchHz" }
    if ($Hysteresis -ge 0) { $extra += '--hysteresis-fraction'; $extra += "$Hysteresis" }
    if ($MinGap -ge 0) { $extra += '--min-gap-dot-fraction'; $extra += "$MinGap" }
    if ($MinPulse -ge 0) { $extra += '--min-pulse-dot-fraction'; $extra += "$MinPulse" }

    $args = @(
        'bench-latency',
        '--from-file', $path,
        '--cw-onset-ms', '0',
        '--truth', $truth,
        '--label', $Label,
        '--stable-n', "$StableN",
        '--chunk-ms', "$ChunkMs",
        '--json'
    ) + $extra

    $output = & $bin @args 2>&1
    foreach ($line in $output) {
        $s = "$line"
        if ($s.StartsWith('{') -and $s.Contains('"type":"bench_result"')) {
            $obj = $s | ConvertFrom-Json
            $results += $obj
            break
        }
    }
}

"`nLABEL: $Label"
"{0,-32} {1,10} {2,8} {3,8} {4,7} {5,7} {6,8} {7,9} {8,7}" -f 'scenario','lat_ms','uptime%','drops','relock','ghost','longest','pitch_hz','status' | Write-Host
('-' * 108) | Write-Host
foreach ($r in $results) {
    $up = [math]::Round($r.lock_uptime_ratio * 100, 1)
    $latRaw = $r.acquisition_latency_ms
    $hasStable = $latRaw -ne $null -and "$latRaw" -ne ''
    $latDisplay = if ($hasStable) { "$latRaw" } else { '(no stable)' }
    $ghost = $r.false_chars_before_stable

    # Status classification:
    #   FAIL  - never reached a stable run (decoder couldn't produce trustworthy output)
    #   WARN  - stable run reached but with ghost chars or relock cycles
    #   PASS  - stable run, no ghosts, no relocks
    if (-not $hasStable) {
        $status = 'FAIL'
        $color = 'Red'
    }
    elseif ($ghost -gt 0 -or $r.n_relock_cycles -gt 0 -or $r.n_pitch_lost_after_lock -gt 0) {
        $status = 'WARN'
        $color = 'Yellow'
    }
    else {
        $status = 'PASS'
        $color = 'Green'
    }

    $line = "{0,-32} {1,10} {2,8} {3,8} {4,7} {5,7} {6,8} {7,9} {8,7}" -f `
        $r.scenario, $latDisplay, $up, $r.n_pitch_lost_after_lock,
        $r.n_relock_cycles, $ghost, $r.longest_unlocked_gap_ms, $r.locked_pitch_hz, $status
    Write-Host $line -ForegroundColor $color
}

$stable = @($results | Where-Object { $_.acquisition_latency_ms -ne $null -and "$($_.acquisition_latency_ms)" -ne '' })
$nFail = $results.Count - $stable.Count
$nWarn = @($stable | Where-Object { $_.false_chars_before_stable -gt 0 -or $_.n_relock_cycles -gt 0 -or $_.n_pitch_lost_after_lock -gt 0 }).Count
$nPass = $stable.Count - $nWarn
$mLat = if ($stable.Count -gt 0) { [math]::Round((($stable | Measure-Object -Property acquisition_latency_ms -Average).Average), 0) } else { 'n/a' }
$mUp  = ($results | Measure-Object -Property lock_uptime_ratio -Average).Average
$totDrops = ($results | Measure-Object -Property n_pitch_lost_after_lock -Sum).Sum
$totRelock = ($results | Measure-Object -Property n_relock_cycles -Sum).Sum
$totGhost = ($results | Measure-Object -Property false_chars_before_stable -Sum).Sum
"`nSUMMARY: PASS=$nPass WARN=$nWarn FAIL=$nFail (of $($results.Count))"
"         MEAN lat_ms=$mLat (stable scenarios only) | MEAN uptime=$([math]::Round($mUp*100,1))% | total drops=$totDrops | total relocks=$totRelock | total ghost=$totGhost"
$results | ConvertTo-Json -Depth 4 -Compress | Out-File -Encoding ASCII (Join-Path $root "experiments\cw-decoder\bench-runs\$Label.json")

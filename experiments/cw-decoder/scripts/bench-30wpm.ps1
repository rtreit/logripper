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
    [double]$Hysteresis = -1
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
"{0,-32} {1,8} {2,8} {3,8} {4,7} {5,7} {6,8} {7,9}" -f 'scenario','lat_ms','uptime%','drops','relock','ghost','longest','pitch_hz' | Write-Host
('-' * 96) | Write-Host
foreach ($r in $results) {
    $up = [math]::Round($r.lock_uptime_ratio * 100, 1)
    $line = "{0,-32} {1,8} {2,8} {3,8} {4,7} {5,7} {6,8} {7,9}" -f `
        $r.scenario, $r.acquisition_latency_ms, $up, $r.n_pitch_lost_after_lock,
        $r.n_relock_cycles, $r.false_chars_before_stable, $r.longest_unlocked_gap_ms, $r.locked_pitch_hz
    Write-Host $line
}
$mLat = ($results | Measure-Object -Property acquisition_latency_ms -Average).Average
$mUp  = ($results | Measure-Object -Property lock_uptime_ratio -Average).Average
$totDrops = ($results | Measure-Object -Property n_pitch_lost_after_lock -Sum).Sum
$totRelock = ($results | Measure-Object -Property n_relock_cycles -Sum).Sum
"`nMEAN lat_ms=$([math]::Round($mLat,0)) | MEAN uptime=$([math]::Round($mUp*100,1))% | total drops=$totDrops | total relocks=$totRelock"
$results | ConvertTo-Json -Depth 4 -Compress | Out-File -Encoding ASCII (Join-Path $root "experiments\cw-decoder\bench-runs\$Label.json")

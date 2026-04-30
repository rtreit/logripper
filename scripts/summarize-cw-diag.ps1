<#
.SYNOPSIS
Summarizes a per-cycle CW decoder diagnostic NDJSON log.

.DESCRIPTION
Reads the NDJSON file produced by cw-decoder.exe when the env var
QSORIPPER_CW_DIAG_LOG is set. Reports cycle counts by status (locked,
stitched, suppressed), distribution of SNR / dyn_range / WPM, and the
per-cycle snap text from cycles that were NOT stitched into the session
transcript (so we can see what fidelity is being lost).

.EXAMPLE
$env:QSORIPPER_CW_DIAG_LOG = "C:\temp\cw-diag.ndjson"
# launch GUI, decode for a while, stop
.\scripts\summarize-cw-diag.ps1 -Path C:\temp\cw-diag.ndjson
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $Path,

    [int] $DroppedSamples = 30
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $Path)) {
    throw "Diag log not found: $Path"
}

$cycles = Get-Content -LiteralPath $Path | ForEach-Object {
    if ($_.Trim().Length -eq 0) { return }
    $_ | ConvertFrom-Json
}

$total      = $cycles.Count
$locked     = ($cycles | Where-Object { $_.viz.locked }).Count
$stitched   = ($cycles | Where-Object { $_.session.stitched }).Count
$suppressed = ($cycles | Where-Object { $_.viz.snr_suppressed }).Count

Write-Host "=== CW Decoder Diagnostic Summary ===" -ForegroundColor Cyan
Write-Host "Source file:        $Path"
Write-Host "Total cycles:       $total"
Write-Host "Locked cycles:      $locked  ($([math]::Round(100*$locked/[math]::Max(1,$total),1))%)"
Write-Host "Stitched cycles:    $stitched  ($([math]::Round(100*$stitched/[math]::Max(1,$total),1))%)"
Write-Host "Suppressed cycles:  $suppressed  ($([math]::Round(100*$suppressed/[math]::Max(1,$total),1))%)"

function Quantiles($values, $name) {
    if ($values.Count -eq 0) { return }
    $sorted = $values | Sort-Object
    $p = @(0.0, 0.1, 0.5, 0.9, 1.0) | ForEach-Object {
        $idx = [int][math]::Floor($_ * ($sorted.Count - 1))
        [math]::Round($sorted[$idx], 3)
    }
    Write-Host ("  {0,-18} min={1}  p10={2}  p50={3}  p90={4}  max={5}" -f $name, $p[0], $p[1], $p[2], $p[3], $p[4])
}

Write-Host ""
Write-Host "=== Distribution (locked cycles only) ===" -ForegroundColor Cyan
$lockedCycles = $cycles | Where-Object { $_.viz.locked }
Quantiles ($lockedCycles | ForEach-Object { [double]$_.viz.snr_db })          'snr_db'
Quantiles ($lockedCycles | ForEach-Object { [double]$_.viz.dyn_range_ratio }) 'dyn_range_ratio'
Quantiles ($lockedCycles | ForEach-Object { [double]$_.viz.wpm })             'wpm'
Quantiles ($lockedCycles | ForEach-Object { [double]$_.viz.pitch_hz })        'pitch_hz'
Quantiles ($lockedCycles | ForEach-Object { [int]$_.viz.n_on_durations })     'n_on_durations'

Write-Host ""
Write-Host "=== Distribution (ALL cycles) ===" -ForegroundColor Cyan
Quantiles ($cycles | ForEach-Object { [double]$_.viz.snr_db })          'snr_db'
Quantiles ($cycles | ForEach-Object { [double]$_.viz.dyn_range_ratio }) 'dyn_range_ratio'

Write-Host ""
Write-Host "=== Cycles with snap.text but NOT stitched ===" -ForegroundColor Yellow
$dropped = $cycles | Where-Object {
    -not $_.session.stitched -and $_.snap.text.Length -gt 0
}
Write-Host "Count: $($dropped.Count)"
if ($dropped.Count -gt 0) {
    Write-Host "Sample (first $DroppedSamples):"
    $dropped | Select-Object -First $DroppedSamples | ForEach-Object {
        $reason = if ($_.viz.snr_suppressed) { 'snr_supp' } `
                  elseif (-not $_.viz.locked) { 'acquiring' } `
                  else { 'no_viz' }
        $snr = [math]::Round([double]$_.viz.snr_db, 1)
        $dr  = [math]::Round([double]$_.viz.dyn_range_ratio, 2)
        $t   = [math]::Round([double]$_.t_s, 2)
        Write-Host ("  t={0,6}s  reason={1,-9}  snr={2,5}dB  dr={3,4}  text={4}" -f $t, $reason, $snr, $dr, $_.snap.text)
    }
}

Write-Host ""
Write-Host "=== Stitched cycles (most recent $DroppedSamples) ===" -ForegroundColor Green
$stitchedCycles = $cycles | Where-Object { $_.session.stitched }
$tail = $stitchedCycles | Select-Object -Last $DroppedSamples
foreach ($c in $tail) {
    $t = [math]::Round([double]$c.t_s, 2)
    Write-Host ("  t={0,6}s  appended={1,-30} | snap.text={2}" -f $t, ($c.session.appended.Trim()), $c.snap.text)
}

Write-Host ""
$lastSession = ($cycles | Where-Object { $_.session.len_chars -gt 0 } | Select-Object -Last 1)
if ($lastSession) {
    Write-Host "Final session_transcript length: $($lastSession.session.len_chars) chars" -ForegroundColor Cyan
}

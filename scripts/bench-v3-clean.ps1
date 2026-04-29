# Bench cw-decoder stream-live-v3 against cw_30wpm_abbrev_clean.wav and
# print a tiny diff vs truth. Used to bisect the V3 regression.
[CmdletBinding()]
param(
    [string]$Wav   = 'C:\Users\randy\Git\qsoripper\data\cw-samples\cw_30wpm_abbrev_clean.wav',
    [string]$Truth = (Get-Content 'C:\Users\randy\Git\qsoripper\data\cw-samples\cw_30wpm_abbrev.truth.txt' -Raw).Trim(),
    [int]$DecodeEveryMs = 1000,
    [string]$Field = 'auto'  # auto -> prefer transcript, fallback text
)

$ErrorActionPreference = 'Stop'
$root = Resolve-Path "$PSScriptRoot\.."
$bin = Join-Path $root 'experiments\cw-decoder\target\release\cw-decoder.exe'
if (-not (Test-Path $bin)) { throw "Missing $bin" }

$tmp = Join-Path $env:TEMP "v3bench-$([guid]::NewGuid().ToString('N')).ndjson"
& $bin stream-live-v3 --json --decode-every-ms $DecodeEveryMs --file $Wav 1>$tmp 2>$null
if ($LASTEXITCODE -ne 0) { Write-Warning "decoder exit $LASTEXITCODE"; return }

$last = Get-Content $tmp | Where-Object { $_ -match '"type":"transcript"' } | Select-Object -Last 1
if (-not $last) { Write-Warning 'no transcript event'; return }
$obj = $last | ConvertFrom-Json
Remove-Item $tmp -Force -ErrorAction SilentlyContinue

$out = ''
if ($Field -eq 'auto') {
    if ($obj.PSObject.Properties['transcript'] -and $obj.transcript) { $out = "$($obj.transcript)" }
    else { $out = "$($obj.text)" }
} elseif ($Field -eq 'transcript') { $out = "$($obj.transcript)" }
else { $out = "$($obj.text)" }

# crude similarity: count of matching tokens (in order) vs truth tokens
$tt = $Truth -split '\s+'
$ot = $out   -split '\s+'
# Token-presence score: % of distinct truth tokens that appear at least
# once as a whole token in the output (case sensitive). Tolerates dup
# spam but rewards getting actual multi-letter words right.
$truthSet = $tt | Sort-Object -Unique
$outSet = ($ot | Sort-Object -Unique) | Where-Object { $_ }
$present = 0
foreach ($w in $truthSet) { if ($outSet -contains $w) { $present++ } }
$pct = if ($truthSet.Count -gt 0) { [math]::Round(100.0 * $present / $truthSet.Count, 1) } else { 0 }
$hit = $present

[pscustomobject]@{
    Commit       = (git rev-parse --short HEAD)
    Subject      = (git log -1 --pretty=%s)
    OutLen          = $out.Length
    OutTokens       = $ot.Count
    UniqueTruth     = $truthSet.Count
    TokenPresentPct = $pct
    TokenHits       = $hit
    First120        = $out.Substring(0, [Math]::Min(120, $out.Length))
}

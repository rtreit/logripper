# Generates a deterministic matrix of "stressed" copies of a clean CW
# baseline WAV. Used to stress-test the cw-decoder against attenuation,
# additive noise (white / pink / brown), QRM, narrow IF, and combined
# weak-signal scenarios.
#
# Usage:
#   .\stress-gen.ps1 -InputWav data\cw-samples\cw_30wpm_youtube_70s_2min_12k.wav `
#                    -OutputDir data\cw-stress\30wpm
#
# Requires ffmpeg on PATH. Output WAVs are 12 kHz mono signed-16 PCM
# (the cw-decoder's native target rate, so no decoder-side resampling
# differences mask the variant-under-test).
[CmdletBinding()]
param(
  [Parameter(Mandatory)] [string] $InputWav,
  [Parameter(Mandatory)] [string] $OutputDir,
  [string] $FfmpegPath = 'ffmpeg',
  [int]    $TargetRate = 12000,
  [switch] $Force
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $InputWav)) {
  throw "Input WAV not found: $InputWav"
}
if (-not (Get-Command $FfmpegPath -ErrorAction SilentlyContinue)) {
  throw "ffmpeg not found on PATH (looked for '$FfmpegPath')."
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$inputAbs = (Resolve-Path -LiteralPath $InputWav).Path
$outDir   = (Resolve-Path -LiteralPath $OutputDir).Path

# Common ffmpeg trailers for every variant: convert to 12 kHz mono s16.
$resamp = @('-ar', $TargetRate, '-ac', '1', '-sample_fmt', 's16')

function Invoke-Ff {
  param([string[]] $FfArgs, [string] $OutPath, [string] $Label)
  if ((Test-Path -LiteralPath $OutPath) -and -not $Force) {
    Write-Host "  [skip] $Label  ->  $(Split-Path -Leaf $OutPath)" -ForegroundColor DarkGray
    return
  }
  Write-Host "  [gen]  $Label  ->  $(Split-Path -Leaf $OutPath)" -ForegroundColor Cyan
  & $FfmpegPath -y -hide_banner -loglevel error @FfArgs $OutPath
  if ($LASTEXITCODE -ne 0) {
    throw "ffmpeg failed for $Label (exit $LASTEXITCODE)"
  }
}

# Internal helper: build a graph that mixes the input with a generated
# noise stream at the requested signal/noise volumes (dB FS, both
# negative). Result is loudness-balanced via amix=normalize=0 so the
# operator-set dB volumes are honoured exactly (no auto level drop).
function New-NoisyVariant {
  param(
    [string] $Color,       # white | pink | brown
    [double] $SignalDb,    # signal volume (dB, e.g. -6)
    [double] $NoiseDb,     # noise   volume (dB, e.g. -20)
    [string] $OutPath,
    [string] $Label
  )
  $signalLin = '{0:F4}' -f [math]::Pow(10.0, $SignalDb / 20.0)
  $noiseLin  = '{0:F4}' -f [math]::Pow(10.0, $NoiseDb  / 20.0)
  $filter = (
    "[0:a]volume=$signalLin[s];" +
    "anoisesrc=color=${Color}:sample_rate=${TargetRate}:amplitude=${noiseLin}[n];" +
    "[s][n]amix=inputs=2:duration=first:normalize=0"
  )
  Invoke-Ff -FfArgs @(
    '-i', $inputAbs,
    '-filter_complex', $filter
  ) + $resamp -OutPath $OutPath -Label $Label
}

# ---------- baseline ----------
Invoke-Ff -FfArgs @('-i', $inputAbs) + $resamp `
  -OutPath (Join-Path $outDir 'clean.wav') -Label 'clean (resampled passthrough)'

# ---------- attenuation ladder (no added noise) ----------
foreach ($db in -6, -12, -18, -24, -30) {
  $lin = '{0:F4}' -f [math]::Pow(10.0, $db / 20.0)
  Invoke-Ff -FfArgs @('-i', $inputAbs, '-af', "volume=$lin") + $resamp `
    -OutPath (Join-Path $outDir "atten_${db}dB.wav") -Label "attenuated $db dB"
}

# ---------- white noise ladder (signal at -6 dBFS, noise varied) ----------
foreach ($snr in 20, 10, 6, 3, 0) {
  New-NoisyVariant -Color 'white' -SignalDb -6 -NoiseDb (-6 - $snr) `
    -OutPath (Join-Path $outDir "white_snr${snr}dB.wav") `
    -Label "white noise SNR ${snr} dB"
}

# ---------- pink noise ladder (more like band hiss) ----------
foreach ($snr in 20, 10, 6, 3, 0) {
  New-NoisyVariant -Color 'pink' -SignalDb -6 -NoiseDb (-6 - $snr) `
    -OutPath (Join-Path $outDir "pink_snr${snr}dB.wav") `
    -Label "pink noise SNR ${snr} dB"
}

# ---------- brown / red noise (atmospheric / QRN-like) ----------
foreach ($snr in 10, 6, 3) {
  New-NoisyVariant -Color 'brown' -SignalDb -6 -NoiseDb (-6 - $snr) `
    -OutPath (Join-Path $outDir "brown_snr${snr}dB.wav") `
    -Label "brown noise SNR ${snr} dB"
}

# ---------- narrow IF (250-1100 Hz bandpass simulating CW filter) ----------
Invoke-Ff -FfArgs @(
  '-i', $inputAbs,
  '-af', 'highpass=f=250,lowpass=f=1100'
) + $resamp `
  -OutPath (Join-Path $outDir 'narrow_if.wav') -Label 'narrow IF (250-1100 Hz)'

# ---------- QRM: steady carrier at +250 Hz from CW pitch -10 dB ----------
# Mixed at -16 dB so it's audible but not dominant; broadband nature is
# left intentional so we exercise the off-band noise estimator.
$qrmFilter = (
  "[0:a]volume=0.5[s];" +
  "sine=frequency=850:sample_rate=${TargetRate}:beep_factor=0[q];" +
  "[q]volume=-16dB[qn];" +
  "[s][qn]amix=inputs=2:duration=first:normalize=0"
)
Invoke-Ff -FfArgs @('-i', $inputAbs, '-filter_complex', $qrmFilter) + $resamp `
  -OutPath (Join-Path $outDir 'qrm_850hz.wav') -Label 'QRM carrier @850 Hz (-16 dB)'

# ---------- combined weak-signal preset (the "real test") ----------
# -18 dB attenuation + pink noise at 6 dB SNR is what an operator hears
# as "I can copy that but barely."
New-NoisyVariant -Color 'pink' -SignalDb -18 -NoiseDb (-18 - 6) `
  -OutPath (Join-Path $outDir 'weak_pink_snr6.wav') `
  -Label 'WEAK: -18 dB signal + pink @6 dB SNR'
New-NoisyVariant -Color 'pink' -SignalDb -24 -NoiseDb (-24 - 3) `
  -OutPath (Join-Path $outDir 'weak_pink_snr3.wav') `
  -Label 'VERY WEAK: -24 dB signal + pink @3 dB SNR'

Write-Host ""
Write-Host "Generated $(((Get-ChildItem -LiteralPath $outDir -Filter *.wav).Count)) variants in $outDir" -ForegroundColor Green

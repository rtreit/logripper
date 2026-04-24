# Regenerate the 30 WPM abbreviation reference clip + 4 noise/QSB
# variants used by bench-30wpm.ps1. WAVs are gitignored (large), so
# anyone working on the decoder runs this once after cloning.
#
# Source: data/cw-samples/cw_30wpm_youtube_12k.wav (also gitignored;
# pulled from YouTube CW practice clip and downsampled to 12 kHz mono).
# We snip 14..197 s (the 100-token QSO-abbreviation block) and
# overlay noise / tremolo to produce the 5 bench scenarios.
$ErrorActionPreference = 'Stop'
$repo = Resolve-Path "$PSScriptRoot\..\..\.."
$samples = Join-Path $repo 'data\cw-samples'
$src = Join-Path $samples 'cw_30wpm_youtube_12k.wav'
if (-not (Test-Path $src)) {
    throw "Source clip not found: $src (download cw_30wpm_youtube_12k.wav and downsample to 12 kHz mono)"
}
$clean = Join-Path $samples 'cw_30wpm_abbrev_clean.wav'
$weak  = Join-Path $samples 'cw_30wpm_abbrev_weak.wav'
$qrn   = Join-Path $samples 'cw_30wpm_abbrev_qrn.wav'
$qsb   = Join-Path $samples 'cw_30wpm_abbrev_qsb.wav'
$wkqsb = Join-Path $samples 'cw_30wpm_abbrev_weak_qsb.wav'
$wn    = Join-Path $samples '_white_noise_8.wav'
$bn    = Join-Path $samples '_brown_noise_12.wav'

ffmpeg -y -i $src -ss 14 -to 197 -ac 1 -ar 12000 $clean
ffmpeg -y -f lavfi -i 'anoisesrc=color=white:amplitude=0.08:sample_rate=12000' -t 183 -ac 1 $wn
ffmpeg -y -f lavfi -i 'anoisesrc=color=brown:amplitude=0.12:sample_rate=12000' -t 183 -ac 1 $bn
ffmpeg -y -i $clean -i $wn -filter_complex '[0]volume=0.35[a];[a][1]amix=inputs=2:duration=shortest:normalize=0' -ac 1 $weak
ffmpeg -y -i $clean -i $bn -filter_complex '[0][1]amix=inputs=2:duration=shortest:normalize=0' -ac 1 $qrn
ffmpeg -y -i $clean -af 'tremolo=f=0.15:d=0.7' -ac 1 $qsb
ffmpeg -y -i $weak  -af 'tremolo=f=0.12:d=0.6' -ac 1 $wkqsb
Remove-Item $wn, $bn -Force
Write-Host "Generated 5 variants in $samples"

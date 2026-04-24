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
$clean    = Join-Path $samples 'cw_30wpm_abbrev_clean.wav'
$weak     = Join-Path $samples 'cw_30wpm_abbrev_weak.wav'
$qrn      = Join-Path $samples 'cw_30wpm_abbrev_qrn.wav'
$qsb      = Join-Path $samples 'cw_30wpm_abbrev_qsb.wav'
$wkqsb    = Join-Path $samples 'cw_30wpm_abbrev_weak_qsb.wav'
# Extreme variants — bench stress targets pushing signal-to-noise and
# fade depth far past the mild "weak / qrn / qsb" presets above.
# Note: brown noise (1/f^2) is largely killed by the decoder's 300 Hz
# high-pass filter, so for the *extreme* tier we lean on white and
# CW-band-bandpassed noise that the front end actually has to confront.
$xqrn     = Join-Path $samples 'cw_30wpm_abbrev_extreme_qrn.wav'
$crushed  = Join-Path $samples 'cw_30wpm_abbrev_crushed.wav'
$deepQsb  = Join-Path $samples 'cw_30wpm_abbrev_deep_qsb.wav'
$buried   = Join-Path $samples 'cw_30wpm_abbrev_buried.wav'
$harshWh  = Join-Path $samples 'cw_30wpm_abbrev_harsh_white.wav'
$inBand   = Join-Path $samples 'cw_30wpm_abbrev_inband_qrm.wav'
$chaos    = Join-Path $samples 'cw_30wpm_abbrev_chaos.wav'

$wn   = Join-Path $samples '_white_noise_8.wav'
$bn   = Join-Path $samples '_brown_noise_12.wav'
# High-amplitude noise beds for the extreme variants. Brown noise
# better mimics atmospheric / band noise (1/f^2); white noise is the
# harsher hiss case. Amplitudes chosen so the CW tone is clearly
# embedded in audible noise rather than sitting on top of a faint hiss.
$wnHi  = Join-Path $samples '_white_noise_hi.wav'    # amplitude 0.30
$bnHi  = Join-Path $samples '_brown_noise_hi.wav'    # amplitude 0.45
$bnXh  = Join-Path $samples '_brown_noise_xhi.wav'   # amplitude 0.60
$wnXh  = Join-Path $samples '_white_noise_xhi.wav'   # amplitude 0.55
# CW-band-bandpassed white noise (300-1500 Hz, the same band the decoder
# keeps after its HP/LP chain). This is the noise the decoder cannot
# filter away — it sits right on top of every plausible CW pitch and
# stresses the tone-purity / Fisher acquisition logic directly.
$inBandN = Join-Path $samples '_inband_noise.wav'    # amplitude 0.55, 300-1500 Hz

ffmpeg -y -i $src -ss 14 -to 197 -ac 1 -ar 12000 $clean
ffmpeg -y -f lavfi -i 'anoisesrc=color=white:amplitude=0.08:sample_rate=12000' -t 183 -ac 1 $wn
ffmpeg -y -f lavfi -i 'anoisesrc=color=brown:amplitude=0.12:sample_rate=12000' -t 183 -ac 1 $bn
ffmpeg -y -f lavfi -i 'anoisesrc=color=white:amplitude=0.30:sample_rate=12000' -t 183 -ac 1 $wnHi
ffmpeg -y -f lavfi -i 'anoisesrc=color=brown:amplitude=0.45:sample_rate=12000' -t 183 -ac 1 $bnHi
ffmpeg -y -f lavfi -i 'anoisesrc=color=brown:amplitude=0.60:sample_rate=12000' -t 183 -ac 1 $bnXh
ffmpeg -y -f lavfi -i 'anoisesrc=color=white:amplitude=0.55:sample_rate=12000' -t 183 -ac 1 $wnXh
ffmpeg -y -f lavfi -i 'anoisesrc=color=white:amplitude=0.55:sample_rate=12000' -af 'highpass=f=300,lowpass=f=1500,volume=2.5' -t 183 -ac 1 $inBandN

ffmpeg -y -i $clean -i $wn -filter_complex '[0]volume=0.35[a];[a][1]amix=inputs=2:duration=shortest:normalize=0' -ac 1 $weak
ffmpeg -y -i $clean -i $bn -filter_complex '[0][1]amix=inputs=2:duration=shortest:normalize=0' -ac 1 $qrn
ffmpeg -y -i $clean -af 'tremolo=f=0.15:d=0.7' -ac 1 $qsb
ffmpeg -y -i $weak  -af 'tremolo=f=0.12:d=0.6' -ac 1 $wkqsb

# extreme_qrn: full-strength CW under heavy brown noise (~3.7x mild qrn amplitude).
# Mostly killed by the HP filter, but kept for completeness — it sounds dramatically worse
# than `qrn` to a human listener even though the decoder barely notices.
ffmpeg -y -i $clean -i $bnHi -filter_complex '[0][1]amix=inputs=2:duration=shortest:normalize=0' -ac 1 $xqrn
# crushed: weak CW (0.20 vs the mild-weak 0.35) drowning in loud white hiss.
ffmpeg -y -i $clean -i $wnHi -filter_complex '[0]volume=0.20[a];[a][1]amix=inputs=2:duration=shortest:normalize=0' -ac 1 $crushed
# deep_qsb: very slow, very deep fade (depth 0.95, period 10s) plus moderate brown noise.
ffmpeg -y -i $qrn -af 'tremolo=f=0.1:d=0.95' -ac 1 $deepQsb
# buried: weak CW + extra-heavy brown noise + slow deep fade. The "is the decoder still
# alive?" stress preset — humans struggle on this one too.
ffmpeg -y -i $clean -i $bnXh -filter_complex '[0]volume=0.18[a];[a][1]amix=inputs=2:duration=shortest:normalize=0,tremolo=f=0.1:d=0.85' -ac 1 $buried
# harsh_white: weak CW under very loud white hiss the HP filter cannot strip away.
ffmpeg -y -i $clean -i $wnXh -filter_complex '[0]volume=0.15[a];[a][1]amix=inputs=2:duration=shortest:normalize=0' -ac 1 $harshWh
# inband_qrm: weak CW under loud noise concentrated in the CW band (300-1500 Hz).
# The hardest noise scenario for the front end — there is nothing to filter away.
ffmpeg -y -i $clean -i $inBandN -filter_complex '[0]volume=0.18[a];[a][1]amix=inputs=2:duration=shortest:normalize=0' -ac 1 $inBand
# chaos: harsh in-band noise + deep slow fade. The full-stack stress test.
ffmpeg -y -i $clean -i $inBandN -filter_complex '[0]volume=0.18[a];[a][1]amix=inputs=2:duration=shortest:normalize=0,tremolo=f=0.1:d=0.9' -ac 1 $chaos

Remove-Item $wn, $bn, $wnHi, $bnHi, $bnXh, $wnXh, $inBandN -Force
Write-Host "Generated 12 variants in $samples (5 baseline + 7 extreme)"

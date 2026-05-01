//! Audio preprocessing for the CW decoder family.
//!
//! Two stages, applied in order to a mono PCM buffer with a known dominant
//! pitch:
//!
//! 1. **Narrow bandpass** at the CW pitch (RBJ "constant 0 dB peak gain"
//!    biquad). Suppresses adjacent QRM, broadcast carriers, hum, and
//!    out-of-band hiss so the envelope detector sees the CW tone itself
//!    instead of whatever has the loudest peak after auto-pitch lock.
//!
//! 2. **Compander** (envelope-follower + piecewise-linear dB transfer +
//!    makeup gain). Mirrors the ffmpeg recipe
//!    `compand=attacks=0.001:decays=0.01:points=-90/-90|-30/-10|-20/-5|0/-3:gain=15`
//!    that experimentally cut CER from 0.380 → 0.130 on the
//!    `live-20260427-111419` clip. Real radio CW has 10–20 dB amplitude
//!    swings per element from key strength + ALC + propagation, plus
//!    key-click transients that dominate the envelope max while the
//!    steady-state CW sits in the bottom half of the dynamic range.
//!    Compander pulls everything closer together so a single hysteresis
//!    threshold can separate key-on from key-off cleanly.
//!
//! The combination acts as a fast, in-process equivalent of running the
//! audio through `ffmpeg -af "bandpass=...,compand=..."` before decoding,
//! without the process-spawn overhead.

use std::f32::consts::TAU;

/// Knobs for [`apply`]. Defaults match the recipe that won the
/// live-clip CER bake-off.
#[derive(Debug, Clone, Copy)]
pub struct PreprocessConfig {
    /// Master switch. When `false`, [`apply`] returns the input
    /// untouched.
    pub enabled: bool,
    /// 3-dB bandwidth (Hz) of the bandpass filter centered on the
    /// detected CW pitch. 250–300 Hz is the empirical sweet spot —
    /// narrow enough to reject QRM, wide enough to preserve key-click
    /// risetime and small drift in operator pitch / receiver passband.
    pub bandpass_width_hz: f32,
    /// Makeup gain (dB) added after the compander transfer function.
    /// Compensates for the average level loss the curve applies to
    /// strong elements.
    pub gain_db: f32,
    /// Skip the bandpass when the detected pitch is non-positive or
    /// outside `[80, 4000]` Hz. Compander still runs.
    pub clamp_pitch: bool,
}

impl Default for PreprocessConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bandpass_width_hz: 300.0,
            gain_db: 15.0,
            clamp_pitch: true,
        }
    }
}

impl PreprocessConfig {
    /// Disabled config: [`apply`] is a no-op.
    pub const fn disabled() -> Self {
        Self {
            enabled: false,
            bandpass_width_hz: 300.0,
            gain_db: 0.0,
            clamp_pitch: true,
        }
    }
}

/// Apply [`PreprocessConfig`] to `samples` and return a new buffer.
///
/// `pitch_hz` is the bandpass center frequency. Pass the same pitch the
/// envelope/region decoder will use downstream (typically the output of
/// [`crate::region_stream::estimate_dominant_pitch`]) so the filter
/// narrows around the actual CW tone and not a noise-locked harmonic.
pub fn apply(samples: &[f32], sample_rate: u32, pitch_hz: f32, cfg: &PreprocessConfig) -> Vec<f32> {
    if !cfg.enabled || samples.is_empty() || sample_rate == 0 {
        return samples.to_vec();
    }
    let mut out = samples.to_vec();
    let pitch_ok = pitch_hz.is_finite() && pitch_hz > 0.0;
    let in_range = !cfg.clamp_pitch || (80.0..=4000.0).contains(&pitch_hz);
    if pitch_ok && in_range && cfg.bandpass_width_hz > 1.0 {
        bandpass_in_place(&mut out, sample_rate, pitch_hz, cfg.bandpass_width_hz);
    }
    compand_in_place(&mut out, sample_rate, cfg.gain_db);
    out
}

/// RBJ "constant 0 dB peak gain" bandpass biquad applied in place.
///
/// Coefficients follow the Audio EQ Cookbook
/// (https://www.w3.org/TR/audio-eq-cookbook/), with `Q = f0 / bw_hz` so
/// the `bw_hz` parameter matches ffmpeg's `bandpass width_type=h` (i.e.
/// 3-dB bandwidth in Hz).
pub(crate) fn bandpass_in_place(samples: &mut [f32], sample_rate: u32, f0: f32, bw_hz: f32) {
    if samples.is_empty() || sample_rate == 0 || f0 <= 0.0 || bw_hz <= 0.0 {
        return;
    }
    let fs = sample_rate as f32;
    let nyquist = fs * 0.5;
    let f0 = f0.min(nyquist - 1.0).max(1.0);
    let bw = bw_hz.min(nyquist).max(1.0);
    let q = (f0 / bw).max(0.1);

    let w0 = TAU * f0 / fs;
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    let alpha = sin_w0 / (2.0 * q);

    // RBJ BPF, constant 0 dB peak gain
    let b0 = alpha;
    let b1 = 0.0;
    let b2 = -alpha;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha;

    let nb0 = b0 / a0;
    let nb1 = b1 / a0;
    let nb2 = b2 / a0;
    let na1 = a1 / a0;
    let na2 = a2 / a0;

    let mut x1 = 0.0f32;
    let mut x2 = 0.0f32;
    let mut y1 = 0.0f32;
    let mut y2 = 0.0f32;
    for s in samples.iter_mut() {
        let x0 = *s;
        let y0 = nb0 * x0 + nb1 * x1 + nb2 * x2 - na1 * y1 - na2 * y2;
        x2 = x1;
        x1 = x0;
        y2 = y1;
        y1 = y0;
        *s = y0;
    }
}

// Compander piecewise-linear dB transfer function, defined as a list of
// (input_db, output_db) anchor points covering the recipe
// `-90/-90|-30/-10|-20/-5|0/-3`. Inputs below `-90` map 1:1 (slope 1.0)
// so the noise floor is preserved. Inputs above `0 dB` extrapolate
// using the slope of the last segment.
const COMPAND_POINTS: &[(f32, f32)] = &[(-90.0, -90.0), (-30.0, -10.0), (-20.0, -5.0), (0.0, -3.0)];

#[inline]
fn compand_curve_db(input_db: f32) -> f32 {
    if input_db <= COMPAND_POINTS[0].0 {
        // Below first point: 1:1 (preserve noise floor relative position).
        return COMPAND_POINTS[0].1 + (input_db - COMPAND_POINTS[0].0);
    }
    for win in COMPAND_POINTS.windows(2) {
        let (x0, y0) = win[0];
        let (x1, y1) = win[1];
        if input_db <= x1 {
            let t = (input_db - x0) / (x1 - x0);
            return y0 + t * (y1 - y0);
        }
    }
    // Above last point: extrapolate using last segment's slope.
    let n = COMPAND_POINTS.len();
    let (x0, y0) = COMPAND_POINTS[n - 2];
    let (x1, y1) = COMPAND_POINTS[n - 1];
    let slope = (y1 - y0) / (x1 - x0);
    y1 + slope * (input_db - x1)
}

/// Companding: side-chain envelope follower → piecewise-linear dB
/// transfer → per-sample gain → makeup. Mirrors ffmpeg's `compand`
/// filter with `attacks=0.001:decays=0.01:points=-90/-90|-30/-10|-20/-5|0/-3`.
pub(crate) fn compand_in_place(samples: &mut [f32], sample_rate: u32, makeup_db: f32) {
    if samples.is_empty() || sample_rate == 0 {
        return;
    }
    let attack_t = 0.001f32;
    let decay_t = 0.010f32;
    let dt = 1.0f32 / sample_rate as f32;
    // One-pole follower coefficients: `1 - exp(-dt/T)`. For typical CW
    // sample rates (8 k+) attack ≈ 0.12, decay ≈ 0.0125.
    let k_attack = 1.0 - (-dt / attack_t).exp();
    let k_decay = 1.0 - (-dt / decay_t).exp();
    let eps = 1e-8f32;

    let mut env: f32 = 0.0;
    for s in samples.iter_mut() {
        let x = *s;
        let a = x.abs();
        let k = if a > env { k_attack } else { k_decay };
        env += (a - env) * k;
        let env_clamped = env.max(eps).min(1.0);
        let in_db = 20.0 * env_clamped.log10();
        let out_db = compand_curve_db(in_db);
        let gain_db = out_db - in_db + makeup_db;
        let gain_lin = 10f32.powf(gain_db / 20.0);
        // Safety clamp: keep output in [-2, 2]. CW decoders downstream
        // care about envelope shape, not absolute level, so a hard
        // ceiling is fine.
        *s = (x * gain_lin).clamp(-2.0, 2.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synth_tone(rate: u32, secs: f32, freq: f32, amp: f32) -> Vec<f32> {
        let n = (secs * rate as f32) as usize;
        (0..n)
            .map(|i| amp * (TAU * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    fn rms(buf: &[f32]) -> f32 {
        if buf.is_empty() {
            return 0.0;
        }
        let sum_sq: f64 = buf.iter().map(|&v| (v as f64) * (v as f64)).sum();
        ((sum_sq / buf.len() as f64).sqrt()) as f32
    }

    #[test]
    fn disabled_config_returns_input_untouched() {
        let s = synth_tone(8000, 0.5, 700.0, 0.5);
        let out = apply(&s, 8000, 700.0, &PreprocessConfig::disabled());
        assert_eq!(s.len(), out.len());
        for (a, b) in s.iter().zip(out.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn empty_input_returns_empty() {
        let out = apply(&[], 8000, 700.0, &PreprocessConfig::default());
        assert!(out.is_empty());
    }

    #[test]
    fn bandpass_attenuates_out_of_band_tone() {
        // 60 Hz hum should be heavily reduced when bandpassing around 700 Hz.
        let rate = 8000u32;
        let s = synth_tone(rate, 1.0, 60.0, 0.5);
        let mut out = s.clone();
        bandpass_in_place(&mut out, rate, 700.0, 200.0);
        // Skip the filter warm-up region (first ~50 ms).
        let warmup = (rate as f32 * 0.05) as usize;
        let r_in = rms(&s[warmup..]);
        let r_out = rms(&out[warmup..]);
        assert!(
            r_out < r_in * 0.10,
            "hum should be heavily attenuated: in {} → out {} (ratio {})",
            r_in,
            r_out,
            r_out / r_in
        );
    }

    #[test]
    fn bandpass_passes_in_band_tone() {
        let rate = 8000u32;
        let s = synth_tone(rate, 1.0, 700.0, 0.5);
        let mut out = s.clone();
        bandpass_in_place(&mut out, rate, 700.0, 200.0);
        let warmup = (rate as f32 * 0.05) as usize;
        let r_in = rms(&s[warmup..]);
        let r_out = rms(&out[warmup..]);
        // RBJ constant-0dB BPF passes the center frequency near unity.
        assert!(
            r_out > r_in * 0.5,
            "in-band tone should pass: in {r_in} -> out {r_out}"
        );
    }

    #[test]
    fn compand_boosts_quiet_signal_more_than_loud() {
        let rate = 8000u32;
        // Quiet tone: -40 dBFS approx.
        let mut quiet = synth_tone(rate, 0.5, 700.0, 0.01);
        // Louder tone: -6 dBFS approx.
        let mut loud = synth_tone(rate, 0.5, 700.0, 0.5);
        let r_quiet_in = rms(&quiet);
        let r_loud_in = rms(&loud);
        compand_in_place(&mut quiet, rate, 15.0);
        compand_in_place(&mut loud, rate, 15.0);
        let r_quiet_out = rms(&quiet);
        let r_loud_out = rms(&loud);
        let ratio_quiet = r_quiet_out / r_quiet_in;
        let ratio_loud = r_loud_out / r_loud_in;
        assert!(
            ratio_quiet > ratio_loud,
            "compand must boost quiet (x{ratio_quiet}) more than loud (x{ratio_loud})"
        );
        // Ratios are dynamic-range compression: quiet should be lifted
        // by an order of magnitude in level.
        assert!(
            ratio_quiet > 5.0,
            "quiet signal lift too small: x{ratio_quiet}"
        );
    }

    #[test]
    fn compand_curve_anchor_points_match_recipe() {
        // Sanity checks on the static curve.
        assert!((compand_curve_db(-90.0) - -90.0).abs() < 1e-3);
        assert!((compand_curve_db(-30.0) - -10.0).abs() < 1e-3);
        assert!((compand_curve_db(-20.0) - -5.0).abs() < 1e-3);
        assert!((compand_curve_db(0.0) - -3.0).abs() < 1e-3);
        // Below -90: 1:1 slope.
        assert!((compand_curve_db(-100.0) - -100.0).abs() < 1e-3);
        // Between -30 and -20 should interpolate to ~-7.5 at -25.
        let mid = compand_curve_db(-25.0);
        assert!((mid - -7.5).abs() < 1e-2);
    }

    #[test]
    fn apply_preserves_clean_tone_envelope_shape() {
        // End-to-end: a clean morse-like burst should still look bursty
        // (peak high, gap low) after preprocessing. We're checking that
        // we haven't accidentally smeared the on/off boundary.
        let rate = 8000u32;
        let mut s = Vec::new();
        // 200 ms silence, 200 ms tone, 200 ms silence.
        s.extend(synth_tone(rate, 0.2, 700.0, 0.0));
        s.extend(synth_tone(rate, 0.2, 700.0, 0.4));
        s.extend(synth_tone(rate, 0.2, 700.0, 0.0));
        let out = apply(&s, rate, 700.0, &PreprocessConfig::default());
        assert_eq!(out.len(), s.len());
        // RMS during the tone burst (skip 30 ms warm-up at boundary).
        let n_seg = (rate as f32 * 0.2) as usize;
        let warmup = (rate as f32 * 0.03) as usize;
        let burst = &out[n_seg + warmup..2 * n_seg - warmup];
        let trail = &out[2 * n_seg + warmup..3 * n_seg - warmup];
        let r_burst = rms(burst);
        let r_trail = rms(trail);
        assert!(
            r_burst > r_trail * 5.0,
            "burst should dominate trailing silence: burst {r_burst} vs trail {r_trail}"
        );
    }
}

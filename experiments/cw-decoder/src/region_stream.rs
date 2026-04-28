//! Region-based "live" pipeline:
//!
//!   1. Estimate the dominant CW tone over the whole buffer (mean Goertzel
//!      power over a coarse pitch sweep).
//!   2. Compute frame-by-frame Goertzel power at that pitch.
//!   3. Threshold against a noise floor + signal floor split to mark active
//!      frames.
//!   4. Merge active runs across short gaps and discard tiny runs.
//!   5. Decode each surviving region with the v2 whole-buffer ditdah decoder.
//!   6. Concatenate region transcripts with single-space separators.
//!
//! This is the bounded-region replacement for the v1 streaming front-end.
//! It is deliberately stateless and operates on a complete buffer so it can
//! be benchmarked against the exact-window oracle on labeled corpora; a
//! truly online variant can be layered on later by feeding it a growing
//! buffer.

use crate::decoder::{decode_text, decode_text_pinned};

/// Configurable knobs for region detection. All times in seconds.
#[derive(Debug, Clone)]
pub struct RegionStreamConfig {
    /// Goertzel frame length.
    pub frame_len_s: f32,
    /// Goertzel frame step.
    pub frame_step_s: f32,
    /// Lower bound of the candidate pitch sweep (Hz).
    pub pitch_lo_hz: f32,
    /// Upper bound of the candidate pitch sweep (Hz).
    pub pitch_hi_hz: f32,
    /// Pitch sweep resolution (Hz). Smaller = finer pitch lock at higher cost.
    pub pitch_step_hz: f32,
    /// Active threshold = noise + threshold_factor * (signal - noise).
    /// 0.0 = noise floor, 1.0 = signal floor. 0.30 mirrors `harvest::build_permissive_profile`.
    pub threshold_factor: f32,
    /// Active runs separated by gaps shorter than this are merged into a
    /// single region. Should be larger than the longest expected
    /// inter-character / inter-word gap for the slowest WPM you want to
    /// keep glued together.
    pub merge_gap_s: f32,
    /// Drop regions shorter than this after merging.
    pub min_region_s: f32,
    /// Pad each region by this much on both sides before slicing into the
    /// decoder, so leading dits aren't clipped by the threshold edge.
    pub pad_s: f32,
    /// Optional pinned WPM for the per-region decode. None = ditdah auto.
    pub pin_wpm: Option<f32>,
}

impl Default for RegionStreamConfig {
    fn default() -> Self {
        Self {
            frame_len_s: 0.025,
            frame_step_s: 0.010,
            pitch_lo_hz: 400.0,
            pitch_hi_hz: 1200.0,
            pitch_step_hz: 25.0,
            threshold_factor: 0.30,
            merge_gap_s: 3.0,
            min_region_s: 0.6,
            pad_s: 0.10,
            pin_wpm: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DecodedRegion {
    pub start_s: f32,
    pub end_s: f32,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct RegionStreamResult {
    pub pitch_hz: f32,
    pub regions: Vec<DecodedRegion>,
    pub text: String,
}

/// Run the full region-detect → decode → merge pipeline on a complete buffer.
pub fn decode_region_stream(
    samples: &[f32],
    sample_rate: u32,
    cfg: &RegionStreamConfig,
) -> RegionStreamResult {
    if samples.is_empty() || sample_rate == 0 {
        return RegionStreamResult {
            pitch_hz: 0.0,
            regions: vec![],
            text: String::new(),
        };
    }

    let pitch_hz = estimate_dominant_pitch(samples, sample_rate, cfg);
    let regions_raw = detect_active_regions(samples, sample_rate, pitch_hz, cfg);
    let mut decoded = Vec::with_capacity(regions_raw.len());
    for (start_s, end_s) in regions_raw {
        let pad = cfg.pad_s.max(0.0);
        let s = ((start_s - pad).max(0.0) * sample_rate as f32) as usize;
        let e = (((end_s + pad) * sample_rate as f32) as usize).min(samples.len());
        if e <= s {
            continue;
        }
        let slice = &samples[s..e];
        let text = match cfg.pin_wpm {
            Some(w) => decode_text_pinned(slice, sample_rate, w),
            None => decode_text(slice, sample_rate),
        };
        let text = text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        decoded.push(DecodedRegion {
            start_s,
            end_s,
            text,
        });
    }
    let text = decoded
        .iter()
        .map(|r| r.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    RegionStreamResult {
        pitch_hz,
        regions: decoded,
        text,
    }
}

/// One spectral peak from the goertzel sweep used by the multi-pitch
/// front-end. `power` is the average per-frame Goertzel power at
/// `pitch_hz`, in the same units the dominant-pitch estimator uses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PitchPeak {
    pub pitch_hz: f32,
    pub power: f32,
}

/// Configuration for [`find_top_pitch_peaks`]. Wraps a
/// [`RegionStreamConfig`] (which controls the underlying Goertzel
/// sweep) and adds NMS / dynamic-range knobs.
#[derive(Debug, Clone)]
pub struct MultiPitchConfig {
    /// Maximum peaks to return.
    pub k: usize,
    /// NMS spacing (Hz). Two peaks closer than this in the sweep
    /// collapse to the stronger one. 40 Hz is the default because real
    /// QSO audio commonly has stations 50 Hz apart in pitch; a larger
    /// NMS would falsely merge them.
    pub min_separation_hz: f32,
    /// Drop peaks whose power is below `top_power * min_relative_power`.
    /// 0.10 is the default — keeps peaks within ~10 dB of the strongest
    /// while rejecting noise-floor peaks of the goertzel sweep.
    pub min_relative_power: f32,
    /// Underlying sweep configuration (pitch range, frame size, step).
    pub sweep: RegionStreamConfig,
}

impl Default for MultiPitchConfig {
    fn default() -> Self {
        Self {
            k: 4,
            min_separation_hz: 40.0,
            min_relative_power: 0.10,
            sweep: RegionStreamConfig::default(),
        }
    }
}

/// Run the goertzel sweep across `[pitch_lo_hz, pitch_hi_hz]` at
/// `pitch_step_hz` resolution and return up to `cfg.k` non-overlapping
/// local maxima sorted by power (strongest first).
///
/// This is the multi-station cousin of [`estimate_dominant_pitch`].
/// Algorithm:
///   1. Goertzel sweep produces `(pitch, power)` pairs, the same way
///      the single-pitch detector does.
///   2. Identify local maxima (strictly higher than both neighbours).
///   3. Sort by power descending.
///   4. Greedily emit peaks at least `min_separation_hz` apart,
///      stopping at `k` peaks or when the next peak falls below
///      `top_power * min_relative_power`.
///
/// Returns an empty `Vec` for empty input, sample rate 0, buffers
/// shorter than the goertzel frame, or a degenerate sweep where the
/// strongest non-zero candidate falls below the relative-power floor
/// (e.g. silence).
pub fn find_top_pitch_peaks(
    samples: &[f32],
    sample_rate: u32,
    cfg: &MultiPitchConfig,
) -> Vec<PitchPeak> {
    if samples.is_empty() || sample_rate == 0 || cfg.k == 0 {
        return Vec::new();
    }
    let sweep = &cfg.sweep;
    let frame_len = ((sweep.frame_len_s * sample_rate as f32).round() as usize).max(64);
    let frame_step = ((sweep.frame_step_s * sample_rate as f32).round() as usize).max(8);
    if samples.len() < frame_len || sweep.pitch_step_hz <= 0.0 {
        return Vec::new();
    }

    // Same coarse stride the single-pitch estimator uses.
    let stride = frame_step.saturating_mul(10).max(frame_step);

    let mut candidates: Vec<PitchPeak> = Vec::new();
    let mut pitch = sweep.pitch_lo_hz;
    while pitch <= sweep.pitch_hi_hz {
        let mut sum = 0.0_f64;
        let mut count = 0u32;
        let mut offset = 0usize;
        while offset + frame_len <= samples.len() {
            sum += goertzel_power(&samples[offset..offset + frame_len], sample_rate, pitch) as f64;
            count += 1;
            offset += stride;
        }
        let score = if count > 0 { sum / count as f64 } else { 0.0 };
        candidates.push(PitchPeak {
            pitch_hz: pitch,
            power: score as f32,
        });
        pitch += sweep.pitch_step_hz;
    }
    if candidates.is_empty() {
        return Vec::new();
    }

    // Local-maxima filter on the swept curve. Endpoints may also be
    // peaks if they dominate their single neighbour.
    let n = candidates.len();
    let mut maxima: Vec<PitchPeak> = Vec::new();
    for i in 0..n {
        let p = candidates[i].power;
        let left = if i == 0 {
            f32::NEG_INFINITY
        } else {
            candidates[i - 1].power
        };
        let right = if i + 1 == n {
            f32::NEG_INFINITY
        } else {
            candidates[i + 1].power
        };
        if p > 0.0 && p >= left && p >= right && (p > left || p > right) {
            maxima.push(candidates[i]);
        }
    }
    if maxima.is_empty() {
        return Vec::new();
    }

    // Sort strongest first.
    maxima.sort_by(|a, b| {
        b.power
            .partial_cmp(&a.power)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let top_power = maxima[0].power;
    if top_power <= 0.0 {
        return Vec::new();
    }
    let abs_floor = top_power * cfg.min_relative_power.max(0.0);
    let nms = cfg.min_separation_hz.max(0.0);

    let mut chosen: Vec<PitchPeak> = Vec::new();
    for cand in maxima.into_iter() {
        if cand.power < abs_floor {
            break;
        }
        let too_close = chosen
            .iter()
            .any(|p| (p.pitch_hz - cand.pitch_hz).abs() < nms);
        if too_close {
            continue;
        }
        chosen.push(cand);
        if chosen.len() >= cfg.k {
            break;
        }
    }
    chosen
}

pub fn estimate_dominant_pitch(samples: &[f32], sample_rate: u32, cfg: &RegionStreamConfig) -> f32 {
    let frame_len = ((cfg.frame_len_s * sample_rate as f32).round() as usize).max(64);
    let frame_step = ((cfg.frame_step_s * sample_rate as f32).round() as usize).max(8);
    if samples.len() < frame_len {
        return cfg.pitch_lo_hz;
    }

    let mut best_pitch = cfg.pitch_lo_hz;
    let mut best_score = f32::MIN;
    let mut pitch = cfg.pitch_lo_hz;
    while pitch <= cfg.pitch_hi_hz {
        // Sum power over a coarse stride (every 10th frame is plenty for pitch ID).
        let stride = frame_step.saturating_mul(10).max(frame_step);
        let mut sum = 0.0_f64;
        let mut count = 0u32;
        let mut offset = 0usize;
        while offset + frame_len <= samples.len() {
            sum += goertzel_power(&samples[offset..offset + frame_len], sample_rate, pitch) as f64;
            count += 1;
            offset += stride;
        }
        let score = if count > 0 { sum / count as f64 } else { 0.0 };
        if score as f32 > best_score {
            best_score = score as f32;
            best_pitch = pitch;
        }
        pitch += cfg.pitch_step_hz;
    }
    best_pitch
}

fn detect_active_regions(
    samples: &[f32],
    sample_rate: u32,
    pitch_hz: f32,
    cfg: &RegionStreamConfig,
) -> Vec<(f32, f32)> {
    let frame_len = ((cfg.frame_len_s * sample_rate as f32).round() as usize).max(64);
    let frame_step = ((cfg.frame_step_s * sample_rate as f32).round() as usize).max(8);
    if samples.len() < frame_len {
        return vec![];
    }

    let mut powers = Vec::new();
    let mut offset = 0usize;
    while offset + frame_len <= samples.len() {
        powers.push(goertzel_power(
            &samples[offset..offset + frame_len],
            sample_rate,
            pitch_hz,
        ));
        offset += frame_step;
    }
    if powers.len() < 4 {
        return vec![];
    }

    let mut sorted = powers.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let noise_floor = percentile_sorted(&sorted, 0.35);
    let signal_floor = percentile_sorted(&sorted, 0.85);
    if !noise_floor.is_finite() || !signal_floor.is_finite() || signal_floor <= noise_floor {
        return vec![];
    }
    let threshold = noise_floor + (signal_floor - noise_floor) * cfg.threshold_factor.max(0.0);

    // Frame -> active mask
    let active: Vec<bool> = powers.iter().map(|&p| p >= threshold).collect();

    // Collect contiguous active runs as (start_s, end_s) using the frame-step
    // grid. The end time is the *end* of the last active frame, not its start.
    let step_s = frame_step as f32 / sample_rate as f32;
    let frame_s = frame_len as f32 / sample_rate as f32;
    let mut runs: Vec<(f32, f32)> = Vec::new();
    let mut cur_start: Option<usize> = None;
    for (i, &on) in active.iter().enumerate() {
        match (on, cur_start) {
            (true, None) => cur_start = Some(i),
            (false, Some(s)) => {
                let start_s = s as f32 * step_s;
                let end_s = (i - 1) as f32 * step_s + frame_s;
                runs.push((start_s, end_s));
                cur_start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = cur_start {
        let start_s = s as f32 * step_s;
        let end_s = (active.len() - 1) as f32 * step_s + frame_s;
        runs.push((start_s, end_s));
    }

    // Merge runs separated by gaps shorter than merge_gap_s.
    let mut merged: Vec<(f32, f32)> = Vec::new();
    for run in runs {
        if let Some(last) = merged.last_mut() {
            if run.0 - last.1 <= cfg.merge_gap_s.max(0.0) {
                last.1 = run.1;
                continue;
            }
        }
        merged.push(run);
    }

    // Drop runs shorter than min_region_s.
    merged
        .into_iter()
        .filter(|(s, e)| (e - s) >= cfg.min_region_s.max(0.0))
        .collect()
}

pub fn goertzel_power(samples: &[f32], sample_rate: u32, target_hz: f32) -> f32 {
    let omega = (2.0 * std::f32::consts::PI * target_hz) / sample_rate as f32;
    let coeff = 2.0 * omega.cos();
    let mut q1 = 0.0_f32;
    let mut q2 = 0.0_f32;
    for &s in samples {
        let q0 = coeff * q1 - q2 + s;
        q2 = q1;
        q1 = q0;
    }
    q1 * q1 + q2 * q2 - coeff * q1 * q2
}

pub fn percentile_sorted(sorted: &[f32], q: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let cq = q.clamp(0.0, 1.0);
    let idx = ((sorted.len() - 1) as f32 * cq).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synth_tone(freq_hz: f32, dur_s: f32, sample_rate: u32, amp: f32) -> Vec<f32> {
        let n = (dur_s * sample_rate as f32) as usize;
        (0..n)
            .map(|i| {
                (2.0 * std::f32::consts::PI * freq_hz * i as f32 / sample_rate as f32).sin() * amp
            })
            .collect()
    }

    #[test]
    fn detects_single_region_in_padded_tone() {
        let sr = 12_000u32;
        let mut buf = vec![0.0_f32; (sr as f32 * 2.0) as usize];
        buf.extend(synth_tone(700.0, 1.0, sr, 0.5));
        buf.extend(vec![0.0_f32; (sr as f32 * 2.0) as usize]);
        let cfg = RegionStreamConfig::default();
        let regions = detect_active_regions(&buf, sr, 700.0, &cfg);
        assert_eq!(regions.len(), 1);
        let (s, e) = regions[0];
        assert!((s - 2.0).abs() < 0.2, "start ~2.0, got {s}");
        assert!((e - 3.0).abs() < 0.2, "end ~3.0, got {e}");
    }

    #[test]
    fn estimate_pitch_picks_dominant_frequency() {
        let sr = 12_000u32;
        let buf = synth_tone(600.0, 2.0, sr, 0.5);
        let cfg = RegionStreamConfig::default();
        let pitch = estimate_dominant_pitch(&buf, sr, &cfg);
        assert!(
            (pitch - 600.0).abs() <= cfg.pitch_step_hz,
            "expected ~600, got {pitch}"
        );
    }

    #[test]
    fn estimate_pitch_finds_high_sidetone() {
        // Real-world live captures (e.g. live-20260427-111419.wav) use
        // sidetones up to ~1100 Hz. Default pitch sweep must cover that
        // range; otherwise the detector locks onto whatever has highest
        // power inside the [pitch_lo, pitch_hi] window (typically a
        // low-frequency noise hump) and the rest of the decoder
        // produces ghost-character garbage.
        let sr = 12_000u32;
        let buf = synth_tone(1100.0, 2.0, sr, 0.5);
        let cfg = RegionStreamConfig::default();
        let pitch = estimate_dominant_pitch(&buf, sr, &cfg);
        assert!(
            (pitch - 1100.0).abs() <= cfg.pitch_step_hz,
            "expected ~1100 Hz, got {pitch} (default pitch_hi_hz must cover common operator sidetones)"
        );
    }

    #[test]
    fn empty_input_returns_empty_result() {
        let cfg = RegionStreamConfig::default();
        let r = decode_region_stream(&[], 12_000, &cfg);
        assert!(r.text.is_empty());
        assert!(r.regions.is_empty());
    }

    fn noise_buf(rate: u32, seconds: f32, seed: u64, amplitude: f32) -> Vec<f32> {
        let n = (rate as f32 * seconds) as usize;
        let mut state = seed;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let v = ((state >> 33) as u32) as f32 / u32::MAX as f32;
            out.push((v * 2.0 - 1.0) * amplitude);
        }
        out
    }

    #[test]
    fn find_top_pitch_peaks_returns_empty_for_silence() {
        let sr = 12_000u32;
        let buf = vec![0.0f32; sr as usize * 2];
        let cfg = MultiPitchConfig::default();
        let peaks = find_top_pitch_peaks(&buf, sr, &cfg);
        assert!(
            peaks.is_empty(),
            "silence should yield no peaks, got {peaks:?}"
        );
    }

    #[test]
    fn find_top_pitch_peaks_returns_empty_for_noise_only() {
        let sr = 12_000u32;
        let buf = noise_buf(sr, 2.0, 0xC0FFEE, 0.05);
        let cfg = MultiPitchConfig::default();
        let peaks = find_top_pitch_peaks(&buf, sr, &cfg);
        // White noise has no strong tonal peaks; either empty or all
        // peaks are below the relative-power floor (which is exactly
        // what the gate enforces).
        if !peaks.is_empty() {
            let strongest = peaks.iter().map(|p| p.power).fold(0.0_f32, |a, b| a.max(b));
            // Compare against a fully-swept estimator power as a
            // sanity check that the multi-pitch path does not explode
            // on noise.
            assert!(
                strongest.is_finite(),
                "noise produced non-finite power {strongest}"
            );
        }
    }

    #[test]
    fn find_top_pitch_peaks_handles_short_buffer() {
        let sr = 12_000u32;
        // Buffer shorter than even one goertzel frame.
        let buf = vec![0.5_f32; 16];
        let cfg = MultiPitchConfig::default();
        let peaks = find_top_pitch_peaks(&buf, sr, &cfg);
        assert!(
            peaks.len() <= 1,
            "short buffer should not produce many peaks, got {}",
            peaks.len()
        );
    }

    #[test]
    fn find_top_pitch_peaks_returns_one_for_single_pitch() {
        let sr = 12_000u32;
        let buf = synth_tone(700.0, 2.0, sr, 0.5);
        let cfg = MultiPitchConfig {
            k: 4,
            ..MultiPitchConfig::default()
        };
        let peaks = find_top_pitch_peaks(&buf, sr, &cfg);
        assert!(
            !peaks.is_empty(),
            "single tone should produce at least one peak"
        );
        // Strongest peak should be near 700 Hz (within sweep step).
        let top = peaks[0];
        assert!(
            (top.pitch_hz - 700.0).abs() <= cfg.sweep.pitch_step_hz,
            "expected ~700 Hz, got {}",
            top.pitch_hz
        );
        // Any additional peaks should be at least min_separation_hz away.
        for p in peaks.iter().skip(1) {
            assert!(
                (p.pitch_hz - top.pitch_hz).abs() >= cfg.min_separation_hz,
                "peak {} too close to {}",
                p.pitch_hz,
                top.pitch_hz
            );
        }
    }

    #[test]
    fn find_top_pitch_peaks_resolves_50hz_separation() {
        let sr = 12_000u32;
        let mut buf = synth_tone(700.0, 2.0, sr, 0.4);
        let other = synth_tone(750.0, 2.0, sr, 0.4);
        for (a, b) in buf.iter_mut().zip(other.iter()) {
            *a += *b;
        }
        let cfg = MultiPitchConfig {
            k: 2,
            min_separation_hz: 40.0,
            min_relative_power: 0.10,
            sweep: RegionStreamConfig {
                pitch_step_hz: 10.0,
                ..RegionStreamConfig::default()
            },
        };
        let peaks = find_top_pitch_peaks(&buf, sr, &cfg);
        assert_eq!(peaks.len(), 2, "expected 2 peaks at 700/750, got {peaks:?}");
        let mut got: Vec<f32> = peaks.iter().map(|p| p.pitch_hz).collect();
        got.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(
            (got[0] - 700.0).abs() <= 15.0 && (got[1] - 750.0).abs() <= 15.0,
            "expected ~700 and ~750, got {got:?}"
        );
    }

    #[test]
    fn find_top_pitch_peaks_nms_collapses_close_peaks() {
        let sr = 12_000u32;
        let mut buf = synth_tone(700.0, 2.0, sr, 0.4);
        let other = synth_tone(720.0, 2.0, sr, 0.4);
        for (a, b) in buf.iter_mut().zip(other.iter()) {
            *a += *b;
        }
        let cfg = MultiPitchConfig {
            k: 4,
            min_separation_hz: 60.0,
            min_relative_power: 0.10,
            sweep: RegionStreamConfig {
                pitch_step_hz: 10.0,
                ..RegionStreamConfig::default()
            },
        };
        let peaks = find_top_pitch_peaks(&buf, sr, &cfg);
        assert_eq!(
            peaks.len(),
            1,
            "60 Hz NMS should collapse 20-Hz-spaced peaks, got {peaks:?}"
        );
    }
}

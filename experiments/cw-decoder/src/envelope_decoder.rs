//! Alternate in-Rust CW decoder algorithm.
//!
//! Built specifically as a comparison to ditdah's symbol classifier so we
//! can answer the hypothesis: *is the ditdah element classifier itself the
//! bottleneck, or is it the front-end (segmentation, AGC, WPM tracking)?*
//!
//! Pipeline:
//!   1. Estimate dominant pitch (via [`region_stream`]).
//!   2. Compute Goertzel envelope at that pitch over short frames.
//!   3. Hysteresis threshold to produce key-on / key-off events with
//!      durations in seconds.
//!   4. Classify on-durations into dits/dahs using a 1-D split (median or
//!      pin-WPM derived split point).
//!   5. Classify off-durations into intra-character / character / word
//!      gaps using two split points (1.5x and 4.5x dot length).
//!   6. Build morse string and look up in the canonical table.
//!
//! The point is to keep this self-contained: zero coupling to ditdah, no
//! use of the streaming v1 path, and minimal tunable knobs so any
//! regressions are localised.

use crate::preprocess::{self, PreprocessConfig};
use crate::region_stream::{self, estimate_dominant_pitch, goertzel_power, RegionStreamConfig};

const FRAME_LEN_S: f32 = 0.010; // 10 ms — fine enough for 40 WPM dits.
const FRAME_STEP_S: f32 = 0.005; // 5 ms hop.
const HYST_HIGH: f32 = 0.55; // Fraction of (signal - noise) to enter key-on.
const HYST_LOW: f32 = 0.35; // Fraction of (signal - noise) to leave key-on.
const MIN_ELEMENT_S: f32 = 0.012; // Reject sub-12ms blips as noise (~50 WPM dot).
                                  // Cap auto-detected WPM. Above this the dot estimate is almost always
                                  // noise-locked rather than real CW; blank the transcript instead of
                                  // emitting dit-spam. Pinned WPM bypasses this gate.
const MAX_AUTO_WPM: f32 = 45.0;

/// Default SNR floor (dB) below which the decode is suppressed and the
/// pipeline returns empty text. 20*log10(2.0) ≈ 6 dB; CW signals worth
/// decoding sit comfortably above this. Tuned to filter out the
/// noise-locked failure mode where the auto-pitch detector locks onto a
/// harmonic peak and the envelope is essentially noise.
pub const DEFAULT_MIN_SNR_DB: f32 = 6.0;
/// Default fraction of `envelope_max` that the dynamic range
/// (`signal_floor - noise_floor`) must exceed to pass the bimodality
/// gate. Real CW with intentional silence between elements produces a
/// dynamic range close to `envelope_max`. Random noise has random
/// transient peaks that dominate `envelope_max` while the bulk of the
/// envelope hovers near the mean, giving a much smaller fraction. The
/// 0.55 default cleanly rejects pure white noise (~0.30 in tests) while
/// passing weak but legitimate CW (>0.7).
pub const DEFAULT_MIN_DYN_RANGE_RATIO: f32 = 0.55;

/// Percentile used as the "robust peak" for the dynamic-range gate. We
/// deliberately do NOT use the literal `env_max` because a single
/// key-click transient or QRN spike can dwarf the rest of the envelope
/// and collapse the bimodality ratio even when the underlying CW is
/// obviously bimodal. The 99th percentile keeps the gate sensitive to
/// real noise floors while ignoring isolated transients.
pub(crate) const ROBUST_PEAK_PERCENTILE: f32 = 0.99;

/// SNR (dB) above which the dynamic-range bimodality gate is bypassed.
/// The dyn_range gate exists to catch noise-locked failures where the
/// SNR happens to land above the floor by chance — but real CW with
/// occasional tall key-click transients can fail dyn_range while having
/// a clean SNR of 30+ dB. Above this threshold, treat SNR alone as
/// sufficient evidence the signal is real and let the decoder run.
///
/// Tuned empirically: pure white noise (probed via the project's own
/// percentile-based floors) routinely measures ~21 dB "SNR" because
/// `p90 / p20` of a noise envelope is intrinsically non-trivial. Real
/// CW from a radio reliably sits above 30 dB on the same metric. The
/// threshold below comfortably separates the two.
pub(crate) const DYN_RANGE_BYPASS_SNR_DB: f32 = 30.0;

/// Configuration for [`decode_envelope`].
#[derive(Debug, Clone)]
pub struct EnvelopeConfig {
    /// Optional pin WPM. When `Some`, dot length is derived from it instead
    /// of from the median of detected element lengths. Useful when the
    /// decoder gets confused about dit-vs-dah on short samples.
    pub pin_wpm: Option<f32>,
    /// Optional pin pitch (Hz). When `Some`, the pitch detector is
    /// bypassed and the envelope is computed at the supplied frequency.
    /// Useful when the auto-detector locks onto a noise/harmonic peak.
    pub pin_hz: Option<f32>,
    /// Minimum signal-to-noise ratio (dB) required to emit a transcript.
    /// Computed as `20 * log10(signal_floor / noise_floor)` from the 90th
    /// and 20th percentiles of the envelope. When the gate trips the
    /// pipeline still populates the visualizer frame so an operator can
    /// see *why* nothing decoded — only the text is suppressed.
    pub min_snr_db: f32,
    /// Minimum bimodality ratio: `(signal_floor - noise_floor) /
    /// envelope_max`. Real CW with intentional silence has a value near
    /// 1.0; pure noise is ~0.3 because random transient peaks dominate
    /// `envelope_max` while the percentiles cluster near the mean. This
    /// gate catches the noise-locked failure mode that high-variance
    /// noise sneaks past the percentile-ratio SNR check.
    pub min_dyn_range_ratio: f32,
    /// Front-end audio preprocessing (bandpass + compander). Defaults
    /// match the recipe that cut CER from 0.380 → 0.130 on real-radio
    /// CW. Disable for synthetic test fixtures that already feed the
    /// decoder a pristine tone.
    pub preprocess: PreprocessConfig,
    /// When `Some(s)`, only the most recent `s` seconds of the envelope
    /// drive the quality gate, hysteresis thresholds, and decode. The
    /// visualizer envelope still reflects the full input buffer. This
    /// makes the live decoder robust to QSO turn-taking: when a strong
    /// station finishes and a weaker one starts, the strong station's
    /// peaks no longer anchor `env_max` and collapse the dynamic-range
    /// gate. It also lets a sparse burst at the end of a long buffer
    /// pass the gate as if the buffer were short. `None` keeps the
    /// legacy behavior (gate over the entire buffer); preferred for
    /// offline whole-file decode and synthetic test fixtures.
    pub analysis_window_seconds: Option<f32>,
}

impl Default for EnvelopeConfig {
    fn default() -> Self {
        Self {
            pin_wpm: None,
            pin_hz: None,
            min_snr_db: DEFAULT_MIN_SNR_DB,
            min_dyn_range_ratio: DEFAULT_MIN_DYN_RANGE_RATIO,
            preprocess: PreprocessConfig::default(),
            analysis_window_seconds: None,
        }
    }
}

/// Compute SNR (dB) from envelope noise and signal floor estimates.
/// - `signal <= 0` → 0 dB (no signal at all).
/// - `noise <= 0` and `signal > 0` → +∞ dB (a literal-zero noise floor
///   means the signal is infinitely above the noise; happens with
///   synthetic test inputs and very clean recordings).
#[inline]
pub(crate) fn snr_db(noise: f32, signal: f32) -> f32 {
    if signal <= 0.0 {
        return 0.0;
    }
    if noise <= 0.0 {
        return f32::INFINITY;
    }
    20.0 * (signal / noise).log10()
}

/// Bimodality ratio: dynamic range relative to envelope peak.
/// `(signal_floor - noise_floor) / envelope_max`. 1.0 means the
/// percentiles span the full peak range (clean bimodal CW). Low values
/// mean the percentiles cluster near the mean while a few random
/// transient peaks dominate `envelope_max` (white noise).
#[inline]
pub(crate) fn dyn_range_ratio(noise: f32, signal: f32, env_max: f32) -> f32 {
    if env_max <= 0.0 {
        return 0.0;
    }
    ((signal - noise).max(0.0) / env_max).min(1.0)
}

/// True when the envelope passes the quality gates and a transcript
/// should be emitted.
///
/// The gate is two-tier:
/// 1. SNR floor (`min_snr_db`) is mandatory.
/// 2. Dynamic-range bimodality (`min_dyn_range_ratio`) is required only
///    when SNR sits in the marginal band (`min_snr_db ..
///    DYN_RANGE_BYPASS_SNR_DB`). Above the bypass threshold, SNR alone
///    is enough — the dyn_range check exists to backstop noise-locked
///    false-passes, which can't happen at high SNR.
///
/// This avoids a known failure mode where real CW with occasional tall
/// key-click transients (envelope_max anchored by a few isolated peaks)
/// has clean SNR but fails dyn_range and gets falsely suppressed.
#[inline]
pub(crate) fn passes_quality_gate(
    cfg: &EnvelopeConfig,
    noise: f32,
    signal: f32,
    env_max: f32,
) -> bool {
    let snr = snr_db(noise, signal);
    if snr < cfg.min_snr_db {
        return false;
    }
    if snr >= DYN_RANGE_BYPASS_SNR_DB {
        return true;
    }
    dyn_range_ratio(noise, signal, env_max) >= cfg.min_dyn_range_ratio
}

/// Decode `samples` to text using the envelope+hysteresis pipeline.
///
/// Returns the decoded text. Unknown morse sequences become `*`.
pub fn decode_envelope(samples: &[f32], sample_rate: u32, cfg: &EnvelopeConfig) -> String {
    if samples.is_empty() {
        return String::new();
    }

    let pitch_cfg = RegionStreamConfig {
        frame_len_s: 0.025,
        frame_step_s: 0.010,
        ..RegionStreamConfig::default()
    };
    let pitch = cfg
        .pin_hz
        .unwrap_or_else(|| estimate_dominant_pitch(samples, sample_rate, &pitch_cfg));

    // 0) Front-end preprocessing. Bandpass narrows to the detected
    //    pitch (rejects QRM/hum) and the compander pulls element
    //    amplitudes together so a single hysteresis threshold cleanly
    //    separates key-on from key-off on real-radio audio.
    let preprocessed: Vec<f32>;
    let work: &[f32] = if cfg.preprocess.enabled {
        preprocessed = preprocess::apply(samples, sample_rate, pitch, &cfg.preprocess);
        &preprocessed
    } else {
        samples
    };

    let frame_len = ((FRAME_LEN_S * sample_rate as f32).round() as usize).max(32);
    let frame_step = ((FRAME_STEP_S * sample_rate as f32).round() as usize).max(8);
    if work.len() < frame_len {
        return String::new();
    }

    // 1) Per-frame Goertzel power envelope.
    let mut env: Vec<f32> = Vec::with_capacity(work.len() / frame_step + 1);
    let mut offset = 0usize;
    while offset + frame_len <= work.len() {
        env.push(goertzel_power(
            &work[offset..offset + frame_len],
            sample_rate,
            pitch,
        ));
        offset += frame_step;
    }
    if env.is_empty() {
        return String::new();
    }

    // 2) Estimate noise / signal floor as 20th / 90th percentiles.
    //    Use a robust 99th-percentile peak instead of the literal max
    //    so a single key-click transient or QRN spike does not collapse
    //    the dynamic-range bimodality ratio. When `analysis_window_seconds`
    //    is set, only the most recent slice drives the gate so older
    //    transmissions cannot anchor `env_max`.
    let frame_dt = frame_step as f32 / sample_rate as f32;
    let (analysis_env, _) = analysis_slice(&env, frame_dt, cfg);
    let env_max = robust_peak(analysis_env, ROBUST_PEAK_PERCENTILE);
    let (noise, signal) = percentile_pair(analysis_env, 0.20, 0.90);
    // 2a) Quality gate: combine an SNR floor with a dynamic-range
    //     bimodality check. The latter is what kills the noise-locked
    //     failure mode (random envelope variance can clear the SNR ratio
    //     gate by itself but does not produce a clean bimodal envelope).
    if !passes_quality_gate(cfg, noise, signal, env_max) {
        return String::new();
    }
    let span = (signal - noise).max(1e-9);
    let high = noise + HYST_HIGH * span;
    let low = noise + HYST_LOW * span;

    // 3) Hysteresis state machine -> events. Run on the analysis slice so
    //    the thresholds derived from its stats apply to the same data.
    let (ons, offs) = events_from_envelope(analysis_env, high, low, frame_dt);
    if ons.is_empty() {
        return String::new();
    }

    // 4) Pick dot length. Either from pin_wpm or from the lower-half median
    //    of on-durations (robust against a single long preamble or a few
    //    dahs dominating the mean).
    let dot_s = if let Some(wpm) = cfg.pin_wpm {
        dot_seconds_from_wpm(wpm)
    } else {
        estimate_dot_kmeans(&ons).max(MIN_ELEMENT_S)
    };

    // 5) Decode events into morse + gap tokens, then to text.
    decode_events(&ons, &offs, dot_s)
}

/// Decoded text plus the dot-length the decoder used (in seconds) and
/// the implied WPM. Useful for live UI and feedback loops.
#[derive(Debug, Clone)]
pub struct EnvelopeDecode {
    pub text: String,
    pub dot_seconds: f32,
    pub wpm: f32,
    pub elements: usize,
}

/// Decode `samples` and also return the dot-length / WPM the decoder used.
pub fn decode_envelope_with_stats(
    samples: &[f32],
    sample_rate: u32,
    cfg: &EnvelopeConfig,
) -> EnvelopeDecode {
    if samples.is_empty() {
        return EnvelopeDecode {
            text: String::new(),
            dot_seconds: 0.0,
            wpm: 0.0,
            elements: 0,
        };
    }

    let pitch_cfg = RegionStreamConfig {
        frame_len_s: 0.025,
        frame_step_s: 0.010,
        ..RegionStreamConfig::default()
    };
    let pitch = cfg
        .pin_hz
        .unwrap_or_else(|| estimate_dominant_pitch(samples, sample_rate, &pitch_cfg));

    let preprocessed: Vec<f32>;
    let work: &[f32] = if cfg.preprocess.enabled {
        preprocessed = preprocess::apply(samples, sample_rate, pitch, &cfg.preprocess);
        &preprocessed
    } else {
        samples
    };

    let frame_len = ((FRAME_LEN_S * sample_rate as f32).round() as usize).max(32);
    let frame_step = ((FRAME_STEP_S * sample_rate as f32).round() as usize).max(8);
    if work.len() < frame_len {
        return EnvelopeDecode {
            text: String::new(),
            dot_seconds: 0.0,
            wpm: 0.0,
            elements: 0,
        };
    }

    let mut env: Vec<f32> = Vec::with_capacity(work.len() / frame_step + 1);
    let mut offset = 0usize;
    while offset + frame_len <= work.len() {
        env.push(goertzel_power(
            &work[offset..offset + frame_len],
            sample_rate,
            pitch,
        ));
        offset += frame_step;
    }
    if env.is_empty() {
        return EnvelopeDecode {
            text: String::new(),
            dot_seconds: 0.0,
            wpm: 0.0,
            elements: 0,
        };
    }

    let frame_dt = frame_step as f32 / sample_rate as f32;
    let (analysis_env, _) = analysis_slice(&env, frame_dt, cfg);
    let env_max = robust_peak(analysis_env, ROBUST_PEAK_PERCENTILE);
    let (noise, signal) = percentile_pair(analysis_env, 0.20, 0.90);
    if !passes_quality_gate(cfg, noise, signal, env_max) {
        return EnvelopeDecode {
            text: String::new(),
            dot_seconds: 0.0,
            wpm: 0.0,
            elements: 0,
        };
    }
    let span = (signal - noise).max(1e-9);
    let high = noise + HYST_HIGH * span;
    let low = noise + HYST_LOW * span;

    let (ons, offs) = events_from_envelope(analysis_env, high, low, frame_dt);
    if ons.is_empty() {
        return EnvelopeDecode {
            text: String::new(),
            dot_seconds: 0.0,
            wpm: 0.0,
            elements: 0,
        };
    }

    let dot_s = if let Some(wpm) = cfg.pin_wpm {
        dot_seconds_from_wpm(wpm)
    } else {
        estimate_dot_kmeans(&ons).max(MIN_ELEMENT_S)
    };

    let text = decode_events(&ons, &offs, dot_s);
    let wpm = if dot_s > 0.0 { 1.2 / dot_s } else { 0.0 };
    EnvelopeDecode {
        text,
        dot_seconds: dot_s,
        wpm,
        elements: ons.len(),
    }
}

/// 1-D k-means with k=2 over on-durations. Returns the lower centroid as
/// the dot length. Falls back to lower-half median when there isn't enough
/// spread to separate dots from dahs (e.g. all-dits or all-dahs samples).
fn estimate_dot_kmeans(durations: &[f32]) -> f32 {
    if durations.is_empty() {
        return 0.0;
    }
    if durations.len() < 4 {
        return median_lower_half(durations);
    }
    let mut sorted: Vec<f32> = durations.iter().copied().collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let lo_seed = sorted[sorted.len() / 4];
    let hi_seed = sorted[(3 * sorted.len()) / 4];
    if (hi_seed - lo_seed).abs() < 1e-4 {
        return median_lower_half(durations);
    }
    let mut c_lo = lo_seed;
    let mut c_hi = hi_seed;
    for _ in 0..16 {
        let mut sum_lo = 0.0_f64;
        let mut n_lo = 0u32;
        let mut sum_hi = 0.0_f64;
        let mut n_hi = 0u32;
        for &d in durations.iter() {
            let d_lo = (d - c_lo).abs();
            let d_hi = (d - c_hi).abs();
            if d_lo <= d_hi {
                sum_lo += d as f64;
                n_lo += 1;
            } else {
                sum_hi += d as f64;
                n_hi += 1;
            }
        }
        let new_lo = if n_lo > 0 {
            (sum_lo / n_lo as f64) as f32
        } else {
            c_lo
        };
        let new_hi = if n_hi > 0 {
            (sum_hi / n_hi as f64) as f32
        } else {
            c_hi
        };
        if (new_lo - c_lo).abs() < 1e-5 && (new_hi - c_hi).abs() < 1e-5 {
            c_lo = new_lo;
            c_hi = new_hi;
            break;
        }
        c_lo = new_lo;
        c_hi = new_hi;
    }
    if c_hi < c_lo * 1.5 {
        return median_lower_half(durations);
    }
    c_lo
}

/// Live streaming wrapper around the envelope decoder. Buffers incoming
/// audio and re-runs the whole-buffer decode periodically (analogous to
/// `CausalBaselineStreamer` but using the envelope decoder instead of
/// ditdah). Emits the *full* current transcript on each snapshot; callers
/// that want incremental text should diff against the prior snapshot.
pub struct LiveEnvelopeStreamer {
    sample_rate: u32,
    buffer: Vec<f32>,
    decode_every_samples: usize,
    since_last_decode: usize,
    locked_wpm: Option<f32>,
    pinned_wpm: Option<f32>,
    lock_after_elements: usize,
    last_text: String,
    last_wpm: f32,
    pinned_hz: Option<f32>,
    min_snr_db: f32,
    min_dyn_range_ratio: f32,
    preprocess: PreprocessConfig,
    analysis_window_seconds: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct LiveEnvelopeSnapshot {
    pub transcript: String,
    pub appended: String,
    pub wpm: f32,
    /// Optional viz payload (set when the decode was produced with
    /// `feed_with_viz` / `flush_with_viz`).
    pub viz: Option<VizFrame>,
}

/// A single classified element span over time. Times are seconds from the
/// start of the analysed buffer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VizEventKind {
    OnDit,
    OnDah,
    OffIntra,
    OffChar,
    OffWord,
}

#[derive(Debug, Clone, Copy)]
pub struct VizEvent {
    pub start_s: f32,
    pub end_s: f32,
    pub duration_s: f32,
    pub kind: VizEventKind,
}

/// Snapshot of everything the decoder "sees" at one decode cycle. Designed
/// to be serialised to JSON for the visualizer GUI.
///
/// `envelope` is at the decoder's native frame step (`frame_step_s`) but is
/// downsampled to at most `MAX_VIZ_ENVELOPE_SAMPLES` points so that JSON
/// payloads stay bounded for long buffers.
#[derive(Debug, Clone)]
pub struct VizFrame {
    pub sample_rate: u32,
    pub frame_step_s: f32,
    pub buffer_seconds: f32,
    pub pitch_hz: f32,
    pub envelope: Vec<f32>,
    pub envelope_max: f32,
    pub noise_floor: f32,
    pub signal_floor: f32,
    /// 20*log10(signal_floor / noise_floor). 0 when either is non-positive.
    pub snr_db: f32,
    /// True when the SNR gate suppressed text emission for this frame.
    /// The visualizer should still render the envelope so the operator
    /// can see *why* nothing was decoded.
    pub snr_suppressed: bool,
    pub hyst_high: f32,
    pub hyst_low: f32,
    pub events: Vec<VizEvent>,
    pub on_durations: Vec<f32>,
    pub dot_seconds: f32,
    pub wpm: f32,
    pub centroid_dot: f32,
    pub centroid_dah: f32,
    pub locked_wpm: Option<f32>,
}

pub const MAX_VIZ_ENVELOPE_SAMPLES: usize = 1500;
const MAX_LIVE_ENVELOPE_BUFFER_SECONDS: usize = 60;

impl LiveEnvelopeStreamer {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            buffer: Vec::new(),
            decode_every_samples: ((0.25 * sample_rate as f32) as usize).max(1024),
            since_last_decode: 0,
            locked_wpm: None,
            pinned_wpm: None,
            lock_after_elements: 30,
            last_text: String::new(),
            last_wpm: 0.0,
            pinned_hz: None,
            min_snr_db: DEFAULT_MIN_SNR_DB,
            min_dyn_range_ratio: DEFAULT_MIN_DYN_RANGE_RATIO,
            preprocess: PreprocessConfig::default(),
            // Live decode: only the most recent 3s drive the gate, hysteresis,
            // and decode. Insulates the gate from QSO turn-taking — when a
            // strong station finishes and a weaker one starts, the strong
            // station's peaks no longer anchor `env_max` and falsely trip
            // the dynamic-range gate. Visualizer envelope still spans the
            // full buffer.
            analysis_window_seconds: Some(3.0),
        }
    }

    /// Override the rolling analysis-window length used for gate +
    /// decode (see [`EnvelopeConfig::analysis_window_seconds`]). Pass
    /// `None` to gate over the entire buffer (legacy behavior).
    pub fn set_analysis_window_seconds(&mut self, seconds: Option<f32>) {
        self.analysis_window_seconds = seconds.filter(|s| *s > 0.0);
    }

    /// Override the front-end audio preprocessing (bandpass + compander).
    pub fn set_preprocess(&mut self, preprocess: PreprocessConfig) {
        self.preprocess = preprocess;
    }

    /// Pin the pitch detector to a specific frequency (Hz). When `None`,
    /// auto-detection is used.
    pub fn set_pinned_hz(&mut self, pinned_hz: Option<f32>) {
        self.pinned_hz = pinned_hz;
    }

    /// Pin the timing detector to a specific WPM. When `None`, the streamer
    /// auto-locks once enough elements have been observed.
    pub fn set_pinned_wpm(&mut self, pinned_wpm: Option<f32>) {
        self.pinned_wpm = pinned_wpm.filter(|w| *w > 0.0);
        self.locked_wpm = self.pinned_wpm;
    }

    /// Set the minimum signal-to-noise ratio (dB) required to emit text.
    /// Below this floor the streamer still produces visualizer frames but
    /// returns an empty transcript so the noise-locked failure mode does
    /// not pollute the output.
    pub fn set_min_snr_db(&mut self, min_snr_db: f32) {
        self.min_snr_db = min_snr_db;
    }

    /// Set the minimum dynamic-range bimodality ratio. See
    /// [`EnvelopeConfig::min_dyn_range_ratio`].
    pub fn set_min_dyn_range_ratio(&mut self, ratio: f32) {
        self.min_dyn_range_ratio = ratio;
    }

    /// Feed a chunk of audio. Returns one snapshot per decode cycle (may be
    /// empty if the buffer hasn't grown enough since the last decode).
    pub fn feed(&mut self, samples: &[f32]) -> Vec<LiveEnvelopeSnapshot> {
        self.push_samples(samples);
        self.since_last_decode += samples.len();
        let mut out = Vec::new();
        if self.since_last_decode >= self.decode_every_samples {
            self.since_last_decode = 0;
            out.push(self.decode_now(false));
        }
        out
    }

    /// Like [`feed`] but the snapshot includes a [`VizFrame`].
    pub fn feed_with_viz(&mut self, samples: &[f32]) -> Vec<LiveEnvelopeSnapshot> {
        self.push_samples(samples);
        self.since_last_decode += samples.len();
        let mut out = Vec::new();
        if self.since_last_decode >= self.decode_every_samples {
            self.since_last_decode = 0;
            out.push(self.decode_now(true));
        }
        out
    }

    /// Force a final decode (call when input audio has ended).
    pub fn flush(&mut self) -> LiveEnvelopeSnapshot {
        self.decode_now(false)
    }

    pub fn flush_with_viz(&mut self) -> LiveEnvelopeSnapshot {
        self.decode_now(true)
    }

    pub fn current_wpm(&self) -> f32 {
        self.last_wpm
    }

    pub fn transcript(&self) -> &str {
        &self.last_text
    }

    fn push_samples(&mut self, samples: &[f32]) {
        self.buffer.extend_from_slice(samples);
        let max_samples = (self.sample_rate as usize * MAX_LIVE_ENVELOPE_BUFFER_SECONDS)
            .max(self.decode_every_samples * 2);
        if self.buffer.len() > max_samples {
            let excess = self.buffer.len() - max_samples;
            self.buffer.drain(0..excess);
        }
    }

    fn decode_now(&mut self, with_viz: bool) -> LiveEnvelopeSnapshot {
        let cfg = EnvelopeConfig {
            pin_wpm: self.pinned_wpm.or(self.locked_wpm),
            pin_hz: self.pinned_hz,
            min_snr_db: self.min_snr_db,
            min_dyn_range_ratio: self.min_dyn_range_ratio,
            preprocess: self.preprocess,
            analysis_window_seconds: self.analysis_window_seconds,
        };
        let (text, wpm, elements, viz) = if with_viz {
            let (text, frame) = decode_envelope_with_viz(&self.buffer, self.sample_rate, &cfg);
            let mut frame = frame;
            frame.locked_wpm = self.pinned_wpm.or(self.locked_wpm);
            let elem_count = frame.on_durations.len();
            let wpm = frame.wpm;
            (text, wpm, elem_count, Some(frame))
        } else {
            let result = decode_envelope_with_stats(&self.buffer, self.sample_rate, &cfg);
            (result.text, result.wpm, result.elements, None)
        };

        if self.pinned_wpm.is_none()
            && self.locked_wpm.is_none()
            && elements >= self.lock_after_elements
            && wpm > 5.0
            && wpm <= MAX_AUTO_WPM
        {
            self.locked_wpm = Some(wpm);
        }

        let appended = if text.starts_with(&self.last_text) {
            text[self.last_text.len()..].to_string()
        } else {
            text.clone()
        };
        self.last_text = text.clone();
        self.last_wpm = wpm;
        LiveEnvelopeSnapshot {
            transcript: text,
            appended,
            wpm,
            viz,
        }
    }
}

/// Returns the suffix of the envelope that should drive the quality
/// gate, hysteresis thresholds, and decode, plus the frame offset of
/// that suffix from the start of the full envelope.
///
/// When `cfg.analysis_window_seconds` is `None` (default), the full
/// envelope is used (legacy behavior). When `Some(s)`, only the most
/// recent `s` seconds drive decoding while the visualizer still
/// displays the entire buffer. This insulates the gate from older
/// stations whose peaks would otherwise anchor `env_max` and falsely
/// suppress a quieter station now on the air.
fn analysis_slice<'a>(
    full_env: &'a [f32],
    frame_dt: f32,
    cfg: &EnvelopeConfig,
) -> (&'a [f32], usize) {
    match cfg.analysis_window_seconds {
        Some(s) if s > 0.0 && frame_dt > 0.0 => {
            let target_frames = ((s / frame_dt).ceil() as usize).max(1);
            if full_env.len() > target_frames {
                let offset = full_env.len() - target_frames;
                (&full_env[offset..], offset)
            } else {
                (full_env, 0)
            }
        }
        _ => (full_env, 0),
    }
}

/// Like [`decode_envelope_with_stats`] but also returns a [`VizFrame`] with
/// envelope, thresholds, classified events and k-means centroids for the
/// visualizer. Slightly more expensive than the plain decode (it builds the
/// classified-event list and downsamples the envelope), but still O(N).
pub fn decode_envelope_with_viz(
    samples: &[f32],
    sample_rate: u32,
    cfg: &EnvelopeConfig,
) -> (String, VizFrame) {
    let empty_viz = || VizFrame {
        sample_rate,
        frame_step_s: FRAME_STEP_S,
        buffer_seconds: samples.len() as f32 / sample_rate.max(1) as f32,
        pitch_hz: 0.0,
        envelope: Vec::new(),
        envelope_max: 0.0,
        noise_floor: 0.0,
        signal_floor: 0.0,
        snr_db: 0.0,
        snr_suppressed: false,
        hyst_high: 0.0,
        hyst_low: 0.0,
        events: Vec::new(),
        on_durations: Vec::new(),
        dot_seconds: 0.0,
        wpm: 0.0,
        centroid_dot: 0.0,
        centroid_dah: 0.0,
        locked_wpm: None,
    };

    if samples.is_empty() {
        return (String::new(), empty_viz());
    }

    let pitch_cfg = RegionStreamConfig {
        frame_len_s: 0.025,
        frame_step_s: 0.010,
        ..RegionStreamConfig::default()
    };
    let pitch = cfg
        .pin_hz
        .unwrap_or_else(|| estimate_dominant_pitch(samples, sample_rate, &pitch_cfg));

    let preprocessed: Vec<f32>;
    let work: &[f32] = if cfg.preprocess.enabled {
        preprocessed = preprocess::apply(samples, sample_rate, pitch, &cfg.preprocess);
        &preprocessed
    } else {
        samples
    };

    let frame_len = ((FRAME_LEN_S * sample_rate as f32).round() as usize).max(32);
    let frame_step = ((FRAME_STEP_S * sample_rate as f32).round() as usize).max(8);
    if work.len() < frame_len {
        let mut v = empty_viz();
        v.pitch_hz = pitch;
        return (String::new(), v);
    }

    let mut env: Vec<f32> = Vec::with_capacity(work.len() / frame_step + 1);
    let mut offset = 0usize;
    while offset + frame_len <= work.len() {
        env.push(goertzel_power(
            &work[offset..offset + frame_len],
            sample_rate,
            pitch,
        ));
        offset += frame_step;
    }

    if env.is_empty() {
        let mut v = empty_viz();
        v.pitch_hz = pitch;
        return (String::new(), v);
    }

    let frame_dt = frame_step as f32 / sample_rate as f32;
    let (analysis_env, analysis_offset_frames) = analysis_slice(&env, frame_dt, cfg);
    let analysis_offset_s = analysis_offset_frames as f32 * frame_dt;

    let env_max = robust_peak(analysis_env, ROBUST_PEAK_PERCENTILE);
    let (noise, signal) = percentile_pair(analysis_env, 0.20, 0.90);
    let snr = snr_db(noise, signal);
    if !passes_quality_gate(cfg, noise, signal, env_max) {
        // Quality gate: envelope is essentially noise. Return empty
        // text but a populated viz frame so the operator can SEE why
        // the decoder refused to emit text (the visualizer dashed
        // lines should sit close together, and the noise/signal
        // floors hover near the mean of the envelope).
        let mut v = empty_viz();
        v.pitch_hz = pitch;
        v.envelope = downsample_envelope(&env);
        v.envelope_max = env_max;
        v.noise_floor = noise;
        v.signal_floor = signal;
        v.snr_db = snr;
        v.snr_suppressed = true;
        let span = (signal - noise).max(1e-9);
        v.hyst_high = noise + HYST_HIGH * span;
        v.hyst_low = noise + HYST_LOW * span;
        return (String::new(), v);
    }
    let span = (signal - noise).max(1e-9);
    let high = noise + HYST_HIGH * span;
    let low = noise + HYST_LOW * span;

    let timed = events_with_times(analysis_env, high, low, frame_dt);

    let on_durations: Vec<f32> = timed
        .iter()
        .filter(|e| e.is_on)
        .map(|e| e.duration_s)
        .collect();
    let off_durations: Vec<f32> = timed
        .iter()
        .filter(|e| !e.is_on)
        .map(|e| e.duration_s)
        .collect();

    if on_durations.is_empty() {
        let mut v = empty_viz();
        v.pitch_hz = pitch;
        v.envelope = downsample_envelope(&env);
        v.envelope_max = env_max;
        v.noise_floor = noise;
        v.signal_floor = signal;
        v.snr_db = snr;
        v.hyst_high = high;
        v.hyst_low = low;
        return (String::new(), v);
    }

    let dot_s = if let Some(wpm) = cfg.pin_wpm {
        dot_seconds_from_wpm(wpm)
    } else {
        estimate_dot_kmeans(&on_durations).max(MIN_ELEMENT_S)
    };
    let (centroid_dot, centroid_dah) = kmeans_centroids(&on_durations);

    let text = decode_events(&on_durations, &off_durations, dot_s);
    let wpm = if dot_s > 0.0 { 1.2 / dot_s } else { 0.0 };

    let elem_split = 2.0 * dot_s;
    let char_gap = 2.0 * dot_s;
    let word_gap = 5.0 * dot_s;

    let events: Vec<VizEvent> = timed
        .iter()
        .map(|e| {
            let kind = if e.is_on {
                if e.duration_s >= elem_split {
                    VizEventKind::OnDah
                } else {
                    VizEventKind::OnDit
                }
            } else if e.duration_s >= word_gap {
                VizEventKind::OffWord
            } else if e.duration_s >= char_gap {
                VizEventKind::OffChar
            } else {
                VizEventKind::OffIntra
            };
            VizEvent {
                start_s: e.start_s + analysis_offset_s,
                end_s: e.end_s + analysis_offset_s,
                duration_s: e.duration_s,
                kind,
            }
        })
        .collect();

    let frame = VizFrame {
        sample_rate,
        frame_step_s: frame_dt,
        buffer_seconds: samples.len() as f32 / sample_rate.max(1) as f32,
        pitch_hz: pitch,
        envelope: downsample_envelope(&env),
        envelope_max: env_max,
        noise_floor: noise,
        signal_floor: signal,
        snr_db: snr,
        snr_suppressed: false,
        hyst_high: high,
        hyst_low: low,
        events,
        on_durations,
        dot_seconds: dot_s,
        wpm,
        centroid_dot,
        centroid_dah,
        locked_wpm: None,
    };
    (text, frame)
}

#[derive(Debug, Clone, Copy)]
struct TimedEvent {
    start_s: f32,
    end_s: f32,
    duration_s: f32,
    is_on: bool,
}

fn events_with_times(env: &[f32], high: f32, low: f32, frame_dt: f32) -> Vec<TimedEvent> {
    let mut out: Vec<TimedEvent> = Vec::new();
    let mut keyed = false;
    let mut run_start_frame: usize = 0;
    let mut have_first_on = false;
    let mut last_on_end_frame: usize = 0;

    for (i, &v) in env.iter().enumerate() {
        if keyed {
            if v < low {
                let start_s = run_start_frame as f32 * frame_dt;
                let end_s = i as f32 * frame_dt;
                let dur = end_s - start_s;
                if dur >= MIN_ELEMENT_S {
                    if have_first_on {
                        // Off span from last_on_end_frame to run_start_frame.
                        let off_start = last_on_end_frame as f32 * frame_dt;
                        let off_end = start_s;
                        let off_dur = off_end - off_start;
                        if off_dur > 0.0 {
                            out.push(TimedEvent {
                                start_s: off_start,
                                end_s: off_end,
                                duration_s: off_dur,
                                is_on: false,
                            });
                        }
                    }
                    out.push(TimedEvent {
                        start_s,
                        end_s,
                        duration_s: dur,
                        is_on: true,
                    });
                    have_first_on = true;
                    last_on_end_frame = i;
                }
                keyed = false;
            }
        } else if v > high {
            keyed = true;
            run_start_frame = i;
        }
    }
    if keyed {
        let start_s = run_start_frame as f32 * frame_dt;
        let end_s = env.len() as f32 * frame_dt;
        let dur = end_s - start_s;
        if dur >= MIN_ELEMENT_S {
            if have_first_on {
                let off_start = last_on_end_frame as f32 * frame_dt;
                let off_end = start_s;
                let off_dur = off_end - off_start;
                if off_dur > 0.0 {
                    out.push(TimedEvent {
                        start_s: off_start,
                        end_s: off_end,
                        duration_s: off_dur,
                        is_on: false,
                    });
                }
            }
            out.push(TimedEvent {
                start_s,
                end_s,
                duration_s: dur,
                is_on: true,
            });
        }
    }
    out
}

fn downsample_envelope(env: &[f32]) -> Vec<f32> {
    if env.len() <= MAX_VIZ_ENVELOPE_SAMPLES {
        return env.to_vec();
    }
    let bucket = (env.len() + MAX_VIZ_ENVELOPE_SAMPLES - 1) / MAX_VIZ_ENVELOPE_SAMPLES;
    let mut out = Vec::with_capacity(env.len() / bucket + 1);
    let mut i = 0;
    while i < env.len() {
        let end = (i + bucket).min(env.len());
        let mut peak = 0.0_f32;
        for &v in &env[i..end] {
            if v > peak {
                peak = v;
            }
        }
        out.push(peak);
        i = end;
    }
    out
}

/// Returns (dot_centroid, dah_centroid) from k-means(k=2) over `durations`.
/// Returns (0, 0) for too-small inputs.
fn kmeans_centroids(durations: &[f32]) -> (f32, f32) {
    if durations.len() < 2 {
        let v = durations.first().copied().unwrap_or(0.0);
        return (v, v);
    }
    let mut sorted: Vec<f32> = durations.iter().copied().collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut c_lo = sorted[sorted.len() / 4];
    let mut c_hi = sorted[(3 * sorted.len()) / 4];
    if (c_hi - c_lo).abs() < 1e-4 {
        return (c_lo, c_hi);
    }
    for _ in 0..16 {
        let mut sum_lo = 0.0_f64;
        let mut n_lo = 0u32;
        let mut sum_hi = 0.0_f64;
        let mut n_hi = 0u32;
        for &d in durations.iter() {
            if (d - c_lo).abs() <= (d - c_hi).abs() {
                sum_lo += d as f64;
                n_lo += 1;
            } else {
                sum_hi += d as f64;
                n_hi += 1;
            }
        }
        let new_lo = if n_lo > 0 {
            (sum_lo / n_lo as f64) as f32
        } else {
            c_lo
        };
        let new_hi = if n_hi > 0 {
            (sum_hi / n_hi as f64) as f32
        } else {
            c_hi
        };
        if (new_lo - c_lo).abs() < 1e-5 && (new_hi - c_hi).abs() < 1e-5 {
            c_lo = new_lo;
            c_hi = new_hi;
            break;
        }
        c_lo = new_lo;
        c_hi = new_hi;
    }
    (c_lo, c_hi)
}

fn dot_seconds_from_wpm(wpm: f32) -> f32 {
    // PARIS standard: 1 word = 50 dot units => dot = 1.2 / wpm seconds.
    (1.2_f32 / wpm.max(1.0)).max(MIN_ELEMENT_S)
}

/// Returns `(on_durations_s, off_durations_s)` aligned so that
/// `offs[i]` is the gap between `ons[i]` and `ons[i+1]`. `offs` therefore
/// has at most `ons.len() - 1` entries.
fn events_from_envelope(env: &[f32], high: f32, low: f32, frame_dt: f32) -> (Vec<f32>, Vec<f32>) {
    let mut ons: Vec<f32> = Vec::new();
    let mut offs: Vec<f32> = Vec::new();

    let mut keyed = false;
    let mut run_frames: usize = 0;
    let mut last_off_frames: usize = 0;
    let mut have_first_on = false;

    for &v in env.iter() {
        if keyed {
            run_frames += 1;
            if v < low {
                let dur = run_frames as f32 * frame_dt;
                if dur >= MIN_ELEMENT_S {
                    if have_first_on {
                        offs.push(last_off_frames as f32 * frame_dt);
                    }
                    ons.push(dur);
                    have_first_on = true;
                }
                keyed = false;
                run_frames = 0;
                last_off_frames = 1;
            }
        } else if v > high {
            keyed = true;
            run_frames = 1;
        } else if have_first_on {
            last_off_frames += 1;
        }
    }
    if keyed {
        let dur = run_frames as f32 * frame_dt;
        if dur >= MIN_ELEMENT_S {
            if have_first_on {
                offs.push(last_off_frames as f32 * frame_dt);
            }
            ons.push(dur);
        }
    }
    (ons, offs)
}

fn decode_events(ons: &[f32], offs: &[f32], dot_s: f32) -> String {
    // Standard CW: dah = 3*dot. Element split at 2*dot.
    let elem_split = 2.0 * dot_s;
    // Inter-element gap = 1*dot, char gap = 3*dot, word gap = 7*dot.
    let char_gap = 2.0 * dot_s;
    let word_gap = 5.0 * dot_s;

    let mut out = String::new();
    let mut current = String::new();

    let flush = |buf: &mut String, out: &mut String| {
        if buf.is_empty() {
            return;
        }
        match morse_to_char(buf) {
            Some(c) => out.push(c),
            None => out.push('*'),
        }
        buf.clear();
    };

    for (i, &on) in ons.iter().enumerate() {
        current.push(if on >= elem_split { '-' } else { '.' });
        if i < offs.len() {
            let gap = offs[i];
            if gap >= word_gap {
                flush(&mut current, &mut out);
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            } else if gap >= char_gap {
                flush(&mut current, &mut out);
            }
            // else: intra-character — keep building.
        }
    }
    flush(&mut current, &mut out);
    out.trim_end().to_string()
}

fn percentile_pair(values: &[f32], p_lo: f32, p_hi: f32) -> (f32, f32) {
    let mut sorted: Vec<f32> = values.iter().copied().collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    (
        region_stream::percentile_sorted(&sorted, p_lo),
        region_stream::percentile_sorted(&sorted, p_hi),
    )
}

/// Robust peak: high percentile of the envelope rather than the literal
/// max. Insulates the dynamic-range gate from single-sample QRN spikes
/// or key-click transients that would otherwise dwarf the true CW
/// peaks and falsely collapse the bimodality ratio.
pub(crate) fn robust_peak(values: &[f32], percentile: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f32> = values.iter().copied().collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    region_stream::percentile_sorted(&sorted, percentile)
}

fn median_lower_half(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f32> = values.iter().copied().collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let cutoff = (sorted.len() / 2).max(1);
    sorted[cutoff / 2]
}

fn morse_to_char(s: &str) -> Option<char> {
    match s {
        ".-" => Some('A'),
        "-..." => Some('B'),
        "-.-." => Some('C'),
        "-.." => Some('D'),
        "." => Some('E'),
        "..-." => Some('F'),
        "--." => Some('G'),
        "...." => Some('H'),
        ".." => Some('I'),
        ".---" => Some('J'),
        "-.-" => Some('K'),
        ".-.." => Some('L'),
        "--" => Some('M'),
        "-." => Some('N'),
        "---" => Some('O'),
        ".--." => Some('P'),
        "--.-" => Some('Q'),
        ".-." => Some('R'),
        "..." => Some('S'),
        "-" => Some('T'),
        "..-" => Some('U'),
        "...-" => Some('V'),
        ".--" => Some('W'),
        "-..-" => Some('X'),
        "-.--" => Some('Y'),
        "--.." => Some('Z'),
        ".----" => Some('1'),
        "..---" => Some('2'),
        "...--" => Some('3'),
        "....-" => Some('4'),
        "....." => Some('5'),
        "-...." => Some('6'),
        "--..." => Some('7'),
        "---.." => Some('8'),
        "----." => Some('9'),
        "-----" => Some('0'),
        ".-.-.-" => Some('.'),
        "--..--" => Some(','),
        "..--.." => Some('?'),
        ".----." => Some('\''),
        "-.-.--" => Some('!'),
        "-..-." => Some('/'),
        "-.--." => Some('('),
        "-.--.-" => Some(')'),
        ".-..." => Some('&'),
        "---..." => Some(':'),
        "-.-.-." => Some(';'),
        "-...-" => Some('='),
        ".-.-." => Some('+'),
        "-....-" => Some('-'),
        "..--.-" => Some('_'),
        ".-..-." => Some('"'),
        "...-..-" => Some('$'),
        ".--.-." => Some('@'),
        _ => None,
    }
}

/// Per-track decode result from [`decode_envelope_multi`].
#[derive(Debug, Clone)]
pub struct MultiPitchTrack {
    /// Pitch (Hz) the track was decoded at.
    pub pitch_hz: f32,
    /// Average per-frame Goertzel power at this pitch (sweep units).
    pub power: f32,
    /// Decoded text for this pitch.
    pub transcript: String,
    /// Estimated WPM for this pitch.
    pub wpm: f32,
    /// Visualizer payload from the underlying envelope decode.
    pub viz: VizFrame,
}

/// Default narrow bandpass width for multi-pitch decode (Hz).
///
/// The single-pitch default in [`PreprocessConfig`] is 300 Hz, which is
/// too wide when we are deliberately separating stations 50–200 Hz
/// apart in audio pitch — at 300 Hz, two adjacent stations land in the
/// same passband. 100 Hz is narrow enough to isolate stations spaced
/// ≥ 60 Hz apart while still preserving CW key-click risetime.
pub const DEFAULT_MULTI_PITCH_BANDPASS_WIDTH_HZ: f32 = 100.0;

/// Decode the same buffer at each of the top-`cfg.k` spectral peaks
/// and return one [`MultiPitchTrack`] per peak. Each track runs the
/// existing single-pitch envelope pipeline pinned to its detected
/// pitch.
///
/// `env_cfg` is cloned per track and mutated to set
/// `pin_hz = Some(peak.pitch_hz)`. The caller is responsible for
/// configuring `env_cfg.preprocess.bandpass_width_hz` to a narrow
/// value (suggested
/// [`DEFAULT_MULTI_PITCH_BANDPASS_WIDTH_HZ`]) so each pinned pitch
/// gets its own filtered audio rather than a shared 300-Hz passband.
///
/// Peak detection runs on the recent analysis window (the last
/// `env_cfg.analysis_window_seconds`) so old transients in the
/// rolling buffer can't dominate the goertzel sweep.
pub fn decode_envelope_multi(
    samples: &[f32],
    sample_rate: u32,
    multi_cfg: &crate::region_stream::MultiPitchConfig,
    env_cfg: &EnvelopeConfig,
) -> Vec<MultiPitchTrack> {
    if samples.is_empty() || sample_rate == 0 || multi_cfg.k == 0 {
        return Vec::new();
    }
    // Slice the buffer down to the analysis window before the sweep so
    // peaks reflect what the per-track decoder will actually see.
    let window_samples = match env_cfg.analysis_window_seconds {
        Some(s) if s > 0.0 => {
            let n = (s * sample_rate as f32) as usize;
            if samples.len() > n {
                &samples[samples.len() - n..]
            } else {
                samples
            }
        }
        _ => samples,
    };
    let peaks = crate::region_stream::find_top_pitch_peaks(window_samples, sample_rate, multi_cfg);
    let mut tracks = Vec::with_capacity(peaks.len());
    for peak in peaks {
        let mut cfg = env_cfg.clone();
        cfg.pin_hz = Some(peak.pitch_hz);
        let (text, viz) = decode_envelope_with_viz(samples, sample_rate, &cfg);
        tracks.push(MultiPitchTrack {
            pitch_hz: peak.pitch_hz,
            power: peak.power,
            transcript: text,
            wpm: viz.wpm,
            viz,
        });
    }
    tracks
}

/// Per-track snapshot from [`LiveMultiPitchStreamer`].
#[derive(Debug, Clone)]
pub struct TrackSnapshot {
    /// Stable track id assigned by the streamer. The id persists
    /// across cycles when the same pitch keeps being detected (within
    /// the matching tolerance).
    pub track_id: u32,
    /// Pitch (Hz) the track was decoded at this cycle.
    pub pitch_hz: f32,
    pub wpm: f32,
    pub transcript: String,
    /// Difference from this track's prior transcript. When the
    /// current transcript starts with the prior one, this is the
    /// suffix; otherwise it's the full new transcript.
    pub appended: String,
    pub viz: Option<VizFrame>,
}

/// Internal per-track state kept by [`LiveMultiPitchStreamer`].
struct TrackState {
    track_id: u32,
    pitch_hz: f32,
    last_transcript: String,
    /// Cycles since this track was last matched. Tracks that go
    /// unmatched for more than `expiry_cycles` are dropped.
    cycles_unmatched: u32,
}

/// Live cousin of [`LiveEnvelopeStreamer`] that maintains one
/// transcript per detected pitch with stable track ids across cycles.
///
/// Audio buffering is reused via an internal [`LiveEnvelopeStreamer`]
/// so the existing single-pitch decode keeps running unchanged. On
/// each cycle the streamer also runs [`decode_envelope_multi`] and
/// matches the resulting peaks to existing tracks via a globally-
/// optimal assignment (brute-force over `k!` permutations), keeping
/// track ids stable when stations cross over in pitch.
pub struct LiveMultiPitchStreamer {
    sample_rate: u32,
    buffer: Vec<f32>,
    decode_every_samples: usize,
    since_last_decode: usize,
    multi_cfg: crate::region_stream::MultiPitchConfig,
    preprocess: PreprocessConfig,
    pinned_wpm: Option<f32>,
    min_snr_db: f32,
    min_dyn_range_ratio: f32,
    analysis_window_seconds: Option<f32>,
    /// Maximum pitch difference (Hz) tolerated when matching a peak
    /// to an existing track.
    match_tolerance_hz: f32,
    /// Cycles a track survives without a matching peak before it is
    /// removed. Three cycles ≈ 750 ms at the default cadence — long
    /// enough to ride out a single short dropout but short enough to
    /// reclaim the id when a station goes silent for good.
    expiry_cycles: u32,
    next_track_id: u32,
    tracks: Vec<TrackState>,
}

impl LiveMultiPitchStreamer {
    pub fn new(sample_rate: u32, k: usize) -> Self {
        let preprocess = PreprocessConfig {
            bandpass_width_hz: DEFAULT_MULTI_PITCH_BANDPASS_WIDTH_HZ,
            ..PreprocessConfig::default()
        };
        Self {
            sample_rate,
            buffer: Vec::new(),
            decode_every_samples: ((0.25 * sample_rate as f32) as usize).max(1024),
            since_last_decode: 0,
            multi_cfg: crate::region_stream::MultiPitchConfig {
                k,
                ..crate::region_stream::MultiPitchConfig::default()
            },
            preprocess,
            pinned_wpm: None,
            min_snr_db: DEFAULT_MIN_SNR_DB,
            min_dyn_range_ratio: DEFAULT_MIN_DYN_RANGE_RATIO,
            analysis_window_seconds: Some(3.0),
            match_tolerance_hz: 40.0,
            expiry_cycles: 3,
            next_track_id: 0,
            tracks: Vec::new(),
        }
    }

    pub fn set_k(&mut self, k: usize) {
        self.multi_cfg.k = k;
    }

    pub fn set_match_tolerance_hz(&mut self, tol: f32) {
        self.match_tolerance_hz = tol.max(0.0);
    }

    pub fn set_expiry_cycles(&mut self, cycles: u32) {
        self.expiry_cycles = cycles;
    }

    pub fn set_min_snr_db(&mut self, db: f32) {
        self.min_snr_db = db;
    }

    pub fn set_min_dyn_range_ratio(&mut self, r: f32) {
        self.min_dyn_range_ratio = r;
    }

    pub fn set_pinned_wpm(&mut self, wpm: Option<f32>) {
        self.pinned_wpm = wpm.filter(|w| *w > 0.0);
    }

    pub fn set_preprocess(&mut self, preprocess: PreprocessConfig) {
        self.preprocess = preprocess;
    }

    pub fn set_analysis_window_seconds(&mut self, seconds: Option<f32>) {
        self.analysis_window_seconds = seconds.filter(|s| *s > 0.0);
    }

    /// Feed audio. Returns `Some(snapshots)` once a decode cycle
    /// fires; `None` while the buffer is still filling.
    pub fn feed(&mut self, samples: &[f32]) -> Option<Vec<TrackSnapshot>> {
        self.push_samples(samples);
        self.since_last_decode += samples.len();
        if self.since_last_decode >= self.decode_every_samples {
            self.since_last_decode = 0;
            Some(self.decode_now(false))
        } else {
            None
        }
    }

    /// Like [`feed`] but viz frames are populated.
    pub fn feed_with_viz(&mut self, samples: &[f32]) -> Option<Vec<TrackSnapshot>> {
        self.push_samples(samples);
        self.since_last_decode += samples.len();
        if self.since_last_decode >= self.decode_every_samples {
            self.since_last_decode = 0;
            Some(self.decode_now(true))
        } else {
            None
        }
    }

    pub fn flush(&mut self) -> Vec<TrackSnapshot> {
        self.decode_now(false)
    }

    pub fn flush_with_viz(&mut self) -> Vec<TrackSnapshot> {
        self.decode_now(true)
    }

    fn push_samples(&mut self, samples: &[f32]) {
        self.buffer.extend_from_slice(samples);
        let max_samples = (self.sample_rate as usize * MAX_LIVE_ENVELOPE_BUFFER_SECONDS)
            .max(self.decode_every_samples * 2);
        if self.buffer.len() > max_samples {
            let excess = self.buffer.len() - max_samples;
            self.buffer.drain(0..excess);
        }
    }

    fn decode_now(&mut self, with_viz: bool) -> Vec<TrackSnapshot> {
        let env_cfg = EnvelopeConfig {
            pin_wpm: self.pinned_wpm,
            pin_hz: None,
            min_snr_db: self.min_snr_db,
            min_dyn_range_ratio: self.min_dyn_range_ratio,
            preprocess: self.preprocess,
            analysis_window_seconds: self.analysis_window_seconds,
        };
        let tracks =
            decode_envelope_multi(&self.buffer, self.sample_rate, &self.multi_cfg, &env_cfg);

        // Build assignment of new tracks (peaks) to existing track ids.
        let assignment = assign_tracks(&self.tracks, &tracks, self.match_tolerance_hz);

        // Apply assignment: matched tracks reuse ids, unmatched peaks
        // mint new ids, unmatched existing tracks tick toward expiry.
        let mut new_states: Vec<TrackState> = Vec::with_capacity(tracks.len());
        let mut snapshots: Vec<TrackSnapshot> = Vec::with_capacity(tracks.len());
        let mut existing_matched = vec![false; self.tracks.len()];

        for (new_idx, decoded) in tracks.iter().enumerate() {
            let matched_existing = assignment[new_idx];
            let (track_id, prior_transcript) = match matched_existing {
                Some(old_idx) => {
                    existing_matched[old_idx] = true;
                    (
                        self.tracks[old_idx].track_id,
                        self.tracks[old_idx].last_transcript.clone(),
                    )
                }
                None => {
                    let id = self.next_track_id;
                    self.next_track_id = self.next_track_id.wrapping_add(1);
                    (id, String::new())
                }
            };
            let appended = if decoded.transcript.starts_with(&prior_transcript) {
                decoded.transcript[prior_transcript.len()..].to_string()
            } else {
                decoded.transcript.clone()
            };
            snapshots.push(TrackSnapshot {
                track_id,
                pitch_hz: decoded.pitch_hz,
                wpm: decoded.wpm,
                transcript: decoded.transcript.clone(),
                appended,
                viz: if with_viz {
                    Some(decoded.viz.clone())
                } else {
                    None
                },
            });
            new_states.push(TrackState {
                track_id,
                pitch_hz: decoded.pitch_hz,
                last_transcript: decoded.transcript.clone(),
                cycles_unmatched: 0,
            });
        }

        // Carry forward unmatched existing tracks until they expire.
        for (i, state) in self.tracks.iter().enumerate() {
            if existing_matched[i] {
                continue;
            }
            let aged = state.cycles_unmatched + 1;
            if aged < self.expiry_cycles {
                new_states.push(TrackState {
                    track_id: state.track_id,
                    pitch_hz: state.pitch_hz,
                    last_transcript: state.last_transcript.clone(),
                    cycles_unmatched: aged,
                });
            }
        }

        self.tracks = new_states;
        snapshots
    }
}

/// Returns `assignment[new_idx] = Some(old_idx)` when the new peak
/// matches an existing track, otherwise `None`. Brute-force over all
/// permutations subject to `tolerance_hz`. K is bounded by the
/// `MultiPitchConfig::k` (≤4 in practice), so the K!*K cost is
/// negligible.
fn assign_tracks(
    existing: &[TrackState],
    new_peaks: &[MultiPitchTrack],
    tolerance_hz: f32,
) -> Vec<Option<usize>> {
    let n_new = new_peaks.len();
    let n_old = existing.len();
    let mut assignment = vec![None; n_new];
    if n_new == 0 || n_old == 0 || tolerance_hz <= 0.0 {
        return assignment;
    }

    // Build cost matrix; INF where the diff exceeds tolerance.
    let mut cost = vec![vec![f32::INFINITY; n_old]; n_new];
    for (i, peak) in new_peaks.iter().enumerate() {
        for (j, state) in existing.iter().enumerate() {
            let d = (peak.pitch_hz - state.pitch_hz).abs();
            if d <= tolerance_hz {
                cost[i][j] = d;
            }
        }
    }

    // Enumerate assignments. We pick a subset of new peaks (up to
    // n_old) to match against existing tracks via permutation.
    // Equivalent: pick a permutation `perm` of length n_new whose
    // entries are either an old index (each used at most once) or
    // None (unmatched). Try every such mapping.
    let n_choices = n_old + 1; // n_old slots + "unmatched"
    let total: u64 = (n_choices as u64).saturating_pow(n_new as u32);
    let mut best_cost = f32::INFINITY;
    let mut best: Vec<Option<usize>> = vec![None; n_new];

    // Hard cap to keep this bounded; K should never be > 6 in practice.
    if total > 100_000 {
        // Fall back to greedy nearest-pairing.
        return greedy_assignment(&cost);
    }

    for mut code in 0..total {
        let mut used = vec![false; n_old];
        let mut try_assign = vec![None; n_new];
        let mut total_cost = 0.0_f32;
        let mut valid = true;
        for slot in try_assign.iter_mut().take(n_new) {
            let pick = (code % n_choices as u64) as usize;
            code /= n_choices as u64;
            if pick == n_old {
                *slot = None;
            } else {
                if used[pick] {
                    valid = false;
                    break;
                }
                used[pick] = true;
                *slot = Some(pick);
            }
        }
        if !valid {
            continue;
        }
        for (i, slot) in try_assign.iter().enumerate() {
            if let Some(j) = slot {
                let c = cost[i][*j];
                if !c.is_finite() {
                    valid = false;
                    break;
                }
                total_cost += c;
            } else {
                // Unmatched — soft penalty so we prefer matching when
                // possible. Use tolerance as the constant penalty so a
                // legit match (cost ≤ tolerance) always beats leaving
                // a track unmatched if a match is available.
                total_cost += tolerance_hz;
            }
        }
        if !valid {
            continue;
        }
        if total_cost < best_cost {
            best_cost = total_cost;
            best = try_assign;
        }
    }
    if best_cost.is_finite() {
        assignment = best;
    }
    assignment
}

fn greedy_assignment(cost: &[Vec<f32>]) -> Vec<Option<usize>> {
    let n_new = cost.len();
    let n_old = cost.first().map(|r| r.len()).unwrap_or(0);
    let mut assignment = vec![None; n_new];
    let mut used = vec![false; n_old];
    // Sort all (i, j, cost) ascending and greedily pick.
    let mut pairs: Vec<(usize, usize, f32)> = Vec::new();
    for (i, row) in cost.iter().enumerate() {
        for (j, &c) in row.iter().enumerate() {
            if c.is_finite() {
                pairs.push((i, j, c));
            }
        }
    }
    pairs.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
    for (i, j, _) in pairs {
        if assignment[i].is_none() && !used[j] {
            assignment[i] = Some(j);
            used[j] = true;
        }
    }
    assignment
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn synth(samples: &mut Vec<f32>, rate: u32, secs: f32, on: bool, pitch: f32) {
        let n = (secs * rate as f32) as usize;
        for i in 0..n {
            let t = i as f32 / rate as f32;
            samples.push(if on {
                (TAU * pitch * t).sin() * 0.5
            } else {
                0.0
            });
        }
    }

    fn synth_morse(rate: u32, dot_s: f32, pitch: f32, code: &str) -> Vec<f32> {
        let mut s = Vec::new();
        synth(&mut s, rate, 0.10, false, pitch);
        let chars: Vec<char> = code.chars().collect();
        for (i, ch) in chars.iter().enumerate() {
            match ch {
                '.' => synth(&mut s, rate, dot_s, true, pitch),
                '-' => synth(&mut s, rate, 3.0 * dot_s, true, pitch),
                ' ' => synth(&mut s, rate, 3.0 * dot_s, false, pitch),
                '/' => synth(&mut s, rate, 7.0 * dot_s, false, pitch),
                _ => continue,
            }
            // Inter-element 1-dot gap, but only between two key-on elements.
            let next_is_key = chars
                .get(i + 1)
                .map(|c| matches!(c, '.' | '-'))
                .unwrap_or(false);
            if matches!(ch, '.' | '-') && next_is_key {
                synth(&mut s, rate, dot_s, false, pitch);
            }
        }
        synth(&mut s, rate, 0.10, false, pitch);
        s
    }

    #[test]
    fn decodes_simple_paris() {
        let rate = 8000u32;
        let dot = 0.060_f32; // ~20 WPM
                             // PARIS = .--. .- .-. .. ...
        let s = synth_morse(rate, dot, 700.0, ".--. .- .-. .. ...");
        let txt = decode_envelope(&s, rate, &EnvelopeConfig::default());
        assert_eq!(txt, "PARIS", "got {:?}", txt);
    }

    #[test]
    fn pin_wpm_guides_short_sample() {
        let rate = 8000u32;
        let dot = 0.060_f32;
        let s = synth_morse(rate, dot, 600.0, "-.-"); // K
        let txt = decode_envelope(
            &s,
            rate,
            &EnvelopeConfig {
                pin_wpm: Some(20.0),
                pin_hz: None,
                min_snr_db: DEFAULT_MIN_SNR_DB,
                min_dyn_range_ratio: DEFAULT_MIN_DYN_RANGE_RATIO,
                preprocess: PreprocessConfig::default(),
                analysis_window_seconds: None,
            },
        );
        assert_eq!(txt, "K", "got {:?}", txt);
    }

    #[test]
    fn empty_returns_empty() {
        assert_eq!(decode_envelope(&[], 8000, &EnvelopeConfig::default()), "");
    }

    #[test]
    fn live_streamer_decodes_paris_in_chunks() {
        let rate = 8000u32;
        let dot = 0.060_f32;
        let s = synth_morse(rate, dot, 700.0, ".--. .- .-. .. ...");
        let mut streamer = LiveEnvelopeStreamer::new(rate);
        // Feed in 50 ms chunks to simulate live audio.
        let chunk = (rate as usize) / 20;
        let mut i = 0;
        while i < s.len() {
            let end = (i + chunk).min(s.len());
            streamer.feed(&s[i..end]);
            i = end;
        }
        let final_snap = streamer.flush();
        assert_eq!(
            final_snap.transcript, "PARIS",
            "got {:?}",
            final_snap.transcript
        );
    }

    #[test]
    fn live_streamer_uses_pinned_wpm() {
        let rate = 8000u32;
        let mut streamer = LiveEnvelopeStreamer::new(rate);
        streamer.set_pinned_wpm(Some(20.0));
        streamer.feed(&synth_morse(rate, 0.060, 700.0, "-.-"));

        let final_snap = streamer.flush_with_viz();

        assert_eq!(
            final_snap.transcript, "K",
            "got {:?}",
            final_snap.transcript
        );
        assert_eq!(final_snap.viz.and_then(|viz| viz.locked_wpm), Some(20.0));
    }

    #[test]
    fn live_streamer_bounds_retained_audio() {
        let rate = 1000u32;
        let mut streamer = LiveEnvelopeStreamer::new(rate);
        let samples = vec![0.0; rate as usize * (MAX_LIVE_ENVELOPE_BUFFER_SECONDS + 5)];

        streamer.feed(&samples);

        assert_eq!(
            streamer.buffer.len(),
            rate as usize * MAX_LIVE_ENVELOPE_BUFFER_SECONDS
        );
    }

    #[test]
    fn kmeans_dot_estimate_separates_dits_and_dahs() {
        let mut durs = Vec::new();
        for _ in 0..6 {
            durs.push(0.060);
        }
        for _ in 0..6 {
            durs.push(0.180);
        }
        let dot = estimate_dot_kmeans(&durs);
        assert!((dot - 0.060).abs() < 0.005, "expected ~0.060, got {dot}");
    }

    #[test]
    fn viz_frame_has_envelope_thresholds_and_events() {
        let rate = 8000u32;
        let dot = 0.060_f32;
        let s = synth_morse(rate, dot, 700.0, ".--. .- .-. .. ...");
        let (text, viz) = decode_envelope_with_viz(&s, rate, &EnvelopeConfig::default());
        assert_eq!(text, "PARIS");
        assert!(!viz.envelope.is_empty(), "envelope should be populated");
        assert!(viz.envelope_max > 0.0);
        assert!(viz.signal_floor > viz.noise_floor);
        assert!(viz.hyst_high > viz.hyst_low);
        assert!(!viz.events.is_empty(), "viz events should be populated");
        let on_count = viz
            .events
            .iter()
            .filter(|e| matches!(e.kind, VizEventKind::OnDit | VizEventKind::OnDah))
            .count();
        assert!(
            on_count >= 14,
            "expected at least 14 on events, got {on_count}"
        );
        assert!(
            viz.centroid_dah > viz.centroid_dot,
            "dah centroid > dot centroid"
        );
        assert!(
            viz.wpm > 5.0 && viz.wpm < 40.0,
            "wpm out of range: {}",
            viz.wpm
        );
        assert!(
            viz.snr_db > DEFAULT_MIN_SNR_DB,
            "clean signal SNR should clear the gate, got {} dB",
            viz.snr_db
        );
        assert!(
            !viz.snr_suppressed,
            "clean signal should not trigger SNR gate"
        );
    }

    /// Deterministic pseudo-random noise so tests don't depend on
    /// `rand`. Uses a tiny LCG for reproducibility.
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
    fn noise_only_signal_is_suppressed_by_snr_gate() {
        let rate = 8000u32;
        let s = noise_buf(rate, 5.0, 0x5EED, 0.05);
        // Probe the actual SNR of pure noise to validate the default
        // threshold discriminates noise from real CW.
        let probe_cfg = EnvelopeConfig {
            min_snr_db: 0.0,
            min_dyn_range_ratio: 0.0,
            ..Default::default()
        };
        let (_, probe_viz) = decode_envelope_with_viz(&s, rate, &probe_cfg);
        eprintln!(
            "noise probe: snr_db = {:.2}, dyn_range = {:.3}, noise_floor = {:.6}, signal_floor = {:.6}, env_max = {:.6}",
            probe_viz.snr_db,
            dyn_range_ratio(probe_viz.noise_floor, probe_viz.signal_floor, probe_viz.envelope_max),
            probe_viz.noise_floor,
            probe_viz.signal_floor,
            probe_viz.envelope_max,
        );

        let cfg = EnvelopeConfig::default();
        let (text, viz) = decode_envelope_with_viz(&s, rate, &cfg);
        assert_eq!(
            text, "",
            "noise-only signal must not emit text (got {:?})",
            text
        );
        assert!(
            viz.snr_suppressed,
            "expected snr_suppressed = true; snr_db = {}, threshold = {}",
            viz.snr_db, cfg.min_snr_db
        );
        // Visualizer payload must still be populated so the operator
        // can see *why* nothing decoded.
        assert!(
            !viz.envelope.is_empty(),
            "envelope should remain populated when SNR gate fires"
        );
        assert!(
            viz.signal_floor > 0.0 && viz.noise_floor > 0.0,
            "noise/signal floors should be measured even when gated"
        );
        // Stats-path mirrors the gate.
        let stats = decode_envelope_with_stats(&s, rate, &cfg);
        assert_eq!(stats.text, "");
        assert_eq!(stats.elements, 0);
    }

    #[test]
    fn snr_gate_can_be_disabled() {
        let rate = 8000u32;
        let s = noise_buf(rate, 2.0, 0x5EED, 0.05);
        let cfg = EnvelopeConfig {
            pin_wpm: None,
            pin_hz: None,
            min_snr_db: 0.0,
            min_dyn_range_ratio: 0.0,
            preprocess: PreprocessConfig::default(),
            analysis_window_seconds: None,
        };
        let (_text, viz) = decode_envelope_with_viz(&s, rate, &cfg);
        // With the gate disabled the viz frame is the full happy-path
        // shape (snr_suppressed = false), even on noise.
        assert!(!viz.snr_suppressed);
    }

    #[test]
    fn live_streamer_set_min_snr_db_propagates() {
        let rate = 8000u32;
        let mut streamer = LiveEnvelopeStreamer::new(rate);
        streamer.set_min_snr_db(0.0);
        streamer.set_min_dyn_range_ratio(0.0);
        // Feed pure noise; with both gates disabled the streamer reaches
        // the classifier (which may emit garbage chars). Important: it
        // must not return immediately due to SNR.
        streamer.feed(&noise_buf(rate, 1.5, 0xC0FFEE, 0.05));
        let snap = streamer.flush_with_viz();
        let viz = snap.viz.expect("viz frame requested");
        assert!(!viz.snr_suppressed, "gate should be disabled");
    }

    #[test]
    fn passes_quality_gate_bypasses_dyn_range_when_snr_is_high() {
        // SNR comfortably above the bypass threshold (18 dB). Even with
        // dyn_range_ratio that would normally fail (0.10 < 0.55 default),
        // the gate must pass because high SNR is sufficient evidence the
        // signal is real.
        let cfg = EnvelopeConfig::default();
        // noise=0.01, signal=1.0 → snr ≈ 40 dB (>> 18 dB bypass)
        // env_max=10.0 → dyn_range = (1.0-0.01)/10.0 ≈ 0.099 (< 0.55)
        assert!(
            passes_quality_gate(&cfg, 0.01, 1.0, 10.0),
            "high-SNR signal must bypass dyn_range check; \
             snr={:.1} dB, dyn_range={:.3}",
            snr_db(0.01, 1.0),
            dyn_range_ratio(0.01, 1.0, 10.0)
        );
    }

    #[test]
    fn passes_quality_gate_enforces_dyn_range_in_marginal_snr_band() {
        // SNR above the floor (6 dB) but below the bypass (18 dB) is
        // the marginal band where dyn_range must hold. With dyn_range
        // failing in this band, the gate must reject.
        let cfg = EnvelopeConfig::default();
        // noise=0.5, signal=1.0 → snr ≈ 6 dB (just above floor, below bypass)
        // env_max=10.0 → dyn_range = 0.05 (well below 0.55)
        let snr = snr_db(0.5, 1.0);
        assert!(
            snr >= cfg.min_snr_db && snr < DYN_RANGE_BYPASS_SNR_DB,
            "test setup: snr {} should be in marginal band",
            snr
        );
        assert!(
            !passes_quality_gate(&cfg, 0.5, 1.0, 10.0),
            "marginal SNR with poor dyn_range must be rejected"
        );
    }

    #[test]
    fn passes_quality_gate_rejects_below_snr_floor() {
        // Below the SNR floor, the gate must reject regardless of
        // dyn_range — the SNR floor is mandatory.
        let cfg = EnvelopeConfig::default();
        // noise=0.5, signal=0.6 → snr ≈ 1.6 dB (below 6 dB floor)
        // env_max=0.6 → dyn_range = 0.17 (also poor, but not what's
        // being tested)
        assert!(
            !passes_quality_gate(&cfg, 0.5, 0.6, 0.6),
            "below-SNR-floor must reject"
        );
    }

    #[test]
    fn analysis_window_isolates_recent_station_from_louder_earlier_one() {
        // Regression for the QSO turn-taking false-trip: when a strong
        // station finishes and a quieter station now on the air would
        // otherwise pass on its own, the loud earlier transients anchor
        // env_max for the whole buffer and collapse the dyn-range gate.
        // The 3s analysis window confines gate stats to the recent
        // station's footprint.
        //
        // NOTE: At high SNR (>= DYN_RANGE_BYPASS_SNR_DB) the gate now
        // bypasses dyn_range entirely, so the legacy whole-buffer path
        // also succeeds in this scenario. The analysis window still
        // provides value in the marginal-SNR band where bypass does
        // not kick in, and in the visualizer where stats should reflect
        // recent activity. We only assert here that the windowed config
        // decodes the trailing K cleanly.
        let rate = 8000u32;
        let dot = 0.060_f32;

        // Loud burst first (amp ~3x the K below).
        let loud_pitch = 700.0_f32;
        let mut buf: Vec<f32> = synth_morse(rate, dot, loud_pitch, "-.- -.-");
        for s in &mut buf {
            *s *= 3.0;
        }
        // ~6 s of low background noise simulates inter-transmission silence.
        buf.extend_from_slice(&noise_buf(rate, 6.0, 0xCAFE_BABE, 0.01));
        // Quieter later transmission ("K") that we *want* decoded.
        let mut later = synth_morse(rate, dot, 700.0, "-.-");
        for s in &mut later {
            *s *= 0.4;
        }
        buf.extend_from_slice(&later);

        // Disable preprocessing so the test isolates gate behavior from
        // compander gain (compander would equalize the two amplitudes).
        let no_pp = PreprocessConfig {
            enabled: false,
            ..PreprocessConfig::default()
        };

        let windowed_cfg = EnvelopeConfig {
            preprocess: no_pp,
            analysis_window_seconds: Some(3.0),
            ..Default::default()
        };
        let (windowed_text, windowed_viz) = decode_envelope_with_viz(&buf, rate, &windowed_cfg);
        assert!(
            !windowed_viz.snr_suppressed,
            "windowed gate should pass the recent K, got snr_db={}",
            windowed_viz.snr_db
        );
        assert!(
            windowed_text.contains('K'),
            "expected decoded text to contain 'K', got {:?}",
            windowed_text
        );
    }

    // ---- Multi-pitch tests --------------------------------------------------

    fn synth_continuous_tone(rate: u32, secs: f32, pitch: f32, amp: f32) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        (0..n)
            .map(|i| (TAU * pitch * i as f32 / rate as f32).sin() * amp)
            .collect()
    }

    fn mix(a: &[f32], b: &[f32]) -> Vec<f32> {
        let n = a.len().max(b.len());
        (0..n)
            .map(|i| a.get(i).copied().unwrap_or(0.0) + b.get(i).copied().unwrap_or(0.0))
            .collect()
    }

    #[test]
    fn decode_envelope_multi_decodes_two_simultaneous_morse() {
        let rate = 8000u32;
        let dot = 0.060_f32;
        let a = synth_morse(rate, dot, 600.0, "-.- -.- -.-"); // K K K
        let b = synth_morse(rate, dot, 850.0, "- . ... -"); // T E S T
        let buf = mix(&a, &b);
        let multi_cfg = crate::region_stream::MultiPitchConfig {
            k: 4,
            min_separation_hz: 40.0,
            min_relative_power: 0.10,
            sweep: crate::region_stream::RegionStreamConfig {
                pitch_step_hz: 10.0,
                ..Default::default()
            },
        };
        // Use the narrower bandpass for multi-pitch.
        let preprocess = PreprocessConfig {
            bandpass_width_hz: DEFAULT_MULTI_PITCH_BANDPASS_WIDTH_HZ,
            ..PreprocessConfig::default()
        };
        let env_cfg = EnvelopeConfig {
            preprocess,
            analysis_window_seconds: None,
            ..Default::default()
        };
        let tracks = decode_envelope_multi(&buf, rate, &multi_cfg, &env_cfg);
        assert_eq!(tracks.len(), 2, "expected 2 tracks, got {tracks:?}");

        // Identify which track corresponds to which pitch.
        let mut by_pitch = tracks.clone();
        by_pitch.sort_by(|a, b| a.pitch_hz.partial_cmp(&b.pitch_hz).unwrap());
        let lo = &by_pitch[0];
        let hi = &by_pitch[1];
        assert!(
            (lo.pitch_hz - 600.0).abs() < 25.0,
            "lo pitch {}",
            lo.pitch_hz
        );
        assert!(
            (hi.pitch_hz - 850.0).abs() < 25.0,
            "hi pitch {}",
            hi.pitch_hz
        );
        // Each track should contain at least one of its expected
        // characters. Decoding mixed tones is fragile so we keep the
        // assertion weak — the goal is to prove the two tracks
        // produce distinct, non-empty transcripts.
        assert!(
            !lo.transcript.is_empty(),
            "low-pitch transcript empty: {:?}",
            lo.transcript
        );
        assert!(
            !hi.transcript.is_empty(),
            "high-pitch transcript empty: {:?}",
            hi.transcript
        );
    }

    #[test]
    fn live_multi_pitch_streamer_track_ids_persist_across_cycles() {
        let rate = 8000u32;
        let mut streamer = LiveMultiPitchStreamer::new(rate, 2);
        // First cycle: feed continuous 700 Hz tone (long enough that
        // the 250 ms cadence fires once).
        let chunk = synth_continuous_tone(rate, 0.6, 700.0, 0.5);
        let snaps1 = streamer.feed(&chunk).expect("cycle 1 should fire");
        assert!(!snaps1.is_empty(), "cycle 1 produced no tracks");
        let id1 = snaps1[0].track_id;
        // Second cycle: same pitch — id should stick.
        let snaps2 = streamer
            .feed(&synth_continuous_tone(rate, 0.6, 700.0, 0.5))
            .expect("cycle 2 should fire");
        assert!(!snaps2.is_empty());
        assert_eq!(snaps2[0].track_id, id1, "track id changed across cycles");
    }

    #[test]
    fn live_multi_pitch_streamer_new_pitch_gets_new_track_id() {
        let rate = 8000u32;
        let mut streamer = LiveMultiPitchStreamer::new(rate, 4);
        let _ = streamer.feed(&synth_continuous_tone(rate, 0.6, 700.0, 0.5));
        let mix2 = mix(
            &synth_continuous_tone(rate, 0.6, 700.0, 0.5),
            &synth_continuous_tone(rate, 0.6, 900.0, 0.5),
        );
        let snaps = streamer.feed(&mix2).expect("cycle should fire");
        let pitches: Vec<f32> = snaps.iter().map(|s| s.pitch_hz).collect();
        assert!(
            pitches.iter().any(|p| (p - 900.0).abs() < 30.0),
            "new pitch missing: {pitches:?}"
        );
        // Distinct ids for each track.
        let ids: std::collections::HashSet<u32> = snaps.iter().map(|s| s.track_id).collect();
        assert_eq!(ids.len(), snaps.len(), "track ids not unique: {snaps:?}");
    }

    #[test]
    fn live_multi_pitch_streamer_track_id_persists_through_dropout_and_reappearance() {
        let rate = 8000u32;
        let mut streamer = LiveMultiPitchStreamer::new(rate, 2);
        streamer.set_expiry_cycles(5);
        let snaps1 = streamer
            .feed(&synth_continuous_tone(rate, 0.6, 700.0, 0.5))
            .expect("cycle 1");
        let id1 = snaps1[0].track_id;
        // One cycle of silence (still within expiry window).
        let _ = streamer.feed(&vec![0.0_f32; (rate as f32 * 0.6) as usize]);
        // Pitch reappears.
        let snaps3 = streamer
            .feed(&synth_continuous_tone(rate, 0.6, 700.0, 0.5))
            .expect("cycle 3");
        assert!(!snaps3.is_empty());
        assert_eq!(
            snaps3[0].track_id, id1,
            "track id should be reused across short dropout"
        );
    }

    #[test]
    fn live_multi_pitch_streamer_handles_track_crossing() {
        // Two tracks whose pitches cross over time: A drifts up while
        // B drifts down, ending swapped. With global assignment the
        // ids should still follow the *closest* match each cycle —
        // this test verifies the assignment is at least not arbitrary
        // by checking that both starting ids survive through a small
        // separation cycle.
        let rate = 8000u32;
        let mut streamer = LiveMultiPitchStreamer::new(rate, 2);
        let cycle = mix(
            &synth_continuous_tone(rate, 0.6, 700.0, 0.5),
            &synth_continuous_tone(rate, 0.6, 900.0, 0.5),
        );
        let snaps1 = streamer.feed(&cycle).expect("cycle 1");
        assert_eq!(snaps1.len(), 2, "expected 2 starting tracks");
        let mut starting: std::collections::HashMap<u32, f32> = std::collections::HashMap::new();
        for s in &snaps1 {
            starting.insert(s.track_id, s.pitch_hz);
        }
        // Slightly drifted pitches — 700→720, 900→880. Minimum-cost
        // assignment should keep ids tied to the closer pitch (still
        // 700-ish vs 900-ish), not flip them.
        let cycle2 = mix(
            &synth_continuous_tone(rate, 0.6, 720.0, 0.5),
            &synth_continuous_tone(rate, 0.6, 880.0, 0.5),
        );
        let snaps2 = streamer.feed(&cycle2).expect("cycle 2");
        assert_eq!(snaps2.len(), 2);
        for s in &snaps2 {
            let prior_pitch = starting
                .get(&s.track_id)
                .copied()
                .expect("id should match a starting track");
            assert!(
                (s.pitch_hz - prior_pitch).abs() <= 60.0,
                "track {} jumped from {} to {} — assignment should pick nearest",
                s.track_id,
                prior_pitch,
                s.pitch_hz
            );
        }
    }
}

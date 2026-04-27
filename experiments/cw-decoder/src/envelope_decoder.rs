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

use crate::region_stream::{
    self, estimate_dominant_pitch, goertzel_power, RegionStreamConfig,
};

const FRAME_LEN_S: f32 = 0.010; // 10 ms — fine enough for 40 WPM dits.
const FRAME_STEP_S: f32 = 0.005; // 5 ms hop.
const HYST_HIGH: f32 = 0.55; // Fraction of (signal - noise) to enter key-on.
const HYST_LOW: f32 = 0.35; // Fraction of (signal - noise) to leave key-on.
const MIN_ELEMENT_S: f32 = 0.012; // Reject sub-12ms blips as noise (~50 WPM dot).

/// Configuration for [`decode_envelope`].
#[derive(Debug, Clone)]
pub struct EnvelopeConfig {
    /// Optional pin WPM. When `Some`, dot length is derived from it instead
    /// of from the median of detected element lengths. Useful when the
    /// decoder gets confused about dit-vs-dah on short samples.
    pub pin_wpm: Option<f32>,
}

impl Default for EnvelopeConfig {
    fn default() -> Self {
        Self { pin_wpm: None }
    }
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
    let pitch = estimate_dominant_pitch(samples, sample_rate, &pitch_cfg);

    let frame_len = ((FRAME_LEN_S * sample_rate as f32).round() as usize).max(32);
    let frame_step = ((FRAME_STEP_S * sample_rate as f32).round() as usize).max(8);
    if samples.len() < frame_len {
        return String::new();
    }

    // 1) Per-frame Goertzel power envelope.
    let mut env: Vec<f32> = Vec::with_capacity(samples.len() / frame_step + 1);
    let mut offset = 0usize;
    while offset + frame_len <= samples.len() {
        env.push(goertzel_power(
            &samples[offset..offset + frame_len],
            sample_rate,
            pitch,
        ));
        offset += frame_step;
    }
    if env.is_empty() {
        return String::new();
    }

    // 2) Estimate noise / signal floor as 20th / 90th percentiles.
    let (noise, signal) = percentile_pair(&env, 0.20, 0.90);
    let span = (signal - noise).max(1e-9);
    let high = noise + HYST_HIGH * span;
    let low = noise + HYST_LOW * span;

    // 3) Hysteresis state machine -> events.
    let frame_dt = frame_step as f32 / sample_rate as f32;
    let (ons, offs) = events_from_envelope(&env, high, low, frame_dt);
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
        return EnvelopeDecode { text: String::new(), dot_seconds: 0.0, wpm: 0.0, elements: 0 };
    }

    let pitch_cfg = RegionStreamConfig {
        frame_len_s: 0.025,
        frame_step_s: 0.010,
        ..RegionStreamConfig::default()
    };
    let pitch = estimate_dominant_pitch(samples, sample_rate, &pitch_cfg);

    let frame_len = ((FRAME_LEN_S * sample_rate as f32).round() as usize).max(32);
    let frame_step = ((FRAME_STEP_S * sample_rate as f32).round() as usize).max(8);
    if samples.len() < frame_len {
        return EnvelopeDecode { text: String::new(), dot_seconds: 0.0, wpm: 0.0, elements: 0 };
    }

    let mut env: Vec<f32> = Vec::with_capacity(samples.len() / frame_step + 1);
    let mut offset = 0usize;
    while offset + frame_len <= samples.len() {
        env.push(goertzel_power(
            &samples[offset..offset + frame_len],
            sample_rate,
            pitch,
        ));
        offset += frame_step;
    }
    if env.is_empty() {
        return EnvelopeDecode { text: String::new(), dot_seconds: 0.0, wpm: 0.0, elements: 0 };
    }

    let (noise, signal) = percentile_pair(&env, 0.20, 0.90);
    let span = (signal - noise).max(1e-9);
    let high = noise + HYST_HIGH * span;
    let low = noise + HYST_LOW * span;

    let frame_dt = frame_step as f32 / sample_rate as f32;
    let (ons, offs) = events_from_envelope(&env, high, low, frame_dt);
    if ons.is_empty() {
        return EnvelopeDecode { text: String::new(), dot_seconds: 0.0, wpm: 0.0, elements: 0 };
    }

    let dot_s = if let Some(wpm) = cfg.pin_wpm {
        dot_seconds_from_wpm(wpm)
    } else {
        estimate_dot_kmeans(&ons).max(MIN_ELEMENT_S)
    };

    let text = decode_events(&ons, &offs, dot_s);
    let wpm = if dot_s > 0.0 { 1.2 / dot_s } else { 0.0 };
    EnvelopeDecode { text, dot_seconds: dot_s, wpm, elements: ons.len() }
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
        let new_lo = if n_lo > 0 { (sum_lo / n_lo as f64) as f32 } else { c_lo };
        let new_hi = if n_hi > 0 { (sum_hi / n_hi as f64) as f32 } else { c_hi };
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
    lock_after_elements: usize,
    last_text: String,
    last_wpm: f32,
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

impl LiveEnvelopeStreamer {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            buffer: Vec::new(),
            decode_every_samples: ((0.25 * sample_rate as f32) as usize).max(1024),
            since_last_decode: 0,
            locked_wpm: None,
            lock_after_elements: 30,
            last_text: String::new(),
            last_wpm: 0.0,
        }
    }

    /// Feed a chunk of audio. Returns one snapshot per decode cycle (may be
    /// empty if the buffer hasn't grown enough since the last decode).
    pub fn feed(&mut self, samples: &[f32]) -> Vec<LiveEnvelopeSnapshot> {
        self.buffer.extend_from_slice(samples);
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
        self.buffer.extend_from_slice(samples);
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

    fn decode_now(&mut self, with_viz: bool) -> LiveEnvelopeSnapshot {
        let cfg = EnvelopeConfig { pin_wpm: self.locked_wpm };
        let (text, wpm, elements, viz) = if with_viz {
            let (text, frame) = decode_envelope_with_viz(&self.buffer, self.sample_rate, &cfg);
            let mut frame = frame;
            frame.locked_wpm = self.locked_wpm;
            let elem_count = frame.on_durations.len();
            let wpm = frame.wpm;
            (text, wpm, elem_count, Some(frame))
        } else {
            let result = decode_envelope_with_stats(&self.buffer, self.sample_rate, &cfg);
            (result.text, result.wpm, result.elements, None)
        };

        if self.locked_wpm.is_none()
            && elements >= self.lock_after_elements
            && wpm > 5.0
            && wpm < 60.0
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
        LiveEnvelopeSnapshot { transcript: text, appended, wpm, viz }
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
    let pitch = estimate_dominant_pitch(samples, sample_rate, &pitch_cfg);

    let frame_len = ((FRAME_LEN_S * sample_rate as f32).round() as usize).max(32);
    let frame_step = ((FRAME_STEP_S * sample_rate as f32).round() as usize).max(8);
    if samples.len() < frame_len {
        let mut v = empty_viz();
        v.pitch_hz = pitch;
        return (String::new(), v);
    }

    let mut env: Vec<f32> = Vec::with_capacity(samples.len() / frame_step + 1);
    let mut offset = 0usize;
    while offset + frame_len <= samples.len() {
        env.push(goertzel_power(
            &samples[offset..offset + frame_len],
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

    let env_max = env.iter().copied().fold(0.0_f32, f32::max);
    let (noise, signal) = percentile_pair(&env, 0.20, 0.90);
    let span = (signal - noise).max(1e-9);
    let high = noise + HYST_HIGH * span;
    let low = noise + HYST_LOW * span;

    let frame_dt = frame_step as f32 / sample_rate as f32;
    let timed = events_with_times(&env, high, low, frame_dt);

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
                if e.duration_s >= elem_split { VizEventKind::OnDah } else { VizEventKind::OnDit }
            } else if e.duration_s >= word_gap {
                VizEventKind::OffWord
            } else if e.duration_s >= char_gap {
                VizEventKind::OffChar
            } else {
                VizEventKind::OffIntra
            };
            VizEvent {
                start_s: e.start_s,
                end_s: e.end_s,
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
                    out.push(TimedEvent { start_s, end_s, duration_s: dur, is_on: true });
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
            out.push(TimedEvent { start_s, end_s, duration_s: dur, is_on: true });
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
        let new_lo = if n_lo > 0 { (sum_lo / n_lo as f64) as f32 } else { c_lo };
        let new_hi = if n_hi > 0 { (sum_hi / n_hi as f64) as f32 } else { c_hi };
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
        ".-" => Some('A'), "-..." => Some('B'), "-.-." => Some('C'),
        "-.." => Some('D'), "." => Some('E'), "..-." => Some('F'),
        "--." => Some('G'), "...." => Some('H'), ".." => Some('I'),
        ".---" => Some('J'), "-.-" => Some('K'), ".-.." => Some('L'),
        "--" => Some('M'), "-." => Some('N'), "---" => Some('O'),
        ".--." => Some('P'), "--.-" => Some('Q'), ".-." => Some('R'),
        "..." => Some('S'), "-" => Some('T'), "..-" => Some('U'),
        "...-" => Some('V'), ".--" => Some('W'), "-..-" => Some('X'),
        "-.--" => Some('Y'), "--.." => Some('Z'),
        ".----" => Some('1'), "..---" => Some('2'), "...--" => Some('3'),
        "....-" => Some('4'), "....." => Some('5'), "-...." => Some('6'),
        "--..." => Some('7'), "---.." => Some('8'), "----." => Some('9'),
        "-----" => Some('0'),
        ".-.-.-" => Some('.'), "--..--" => Some(','), "..--.." => Some('?'),
        ".----." => Some('\''), "-.-.--" => Some('!'), "-..-." => Some('/'),
        "-.--." => Some('('), "-.--.-" => Some(')'), ".-..." => Some('&'),
        "---..." => Some(':'), "-.-.-." => Some(';'), "-...-" => Some('='),
        ".-.-." => Some('+'), "-....-" => Some('-'), "..--.-" => Some('_'),
        ".-..-." => Some('"'), "...-..-" => Some('$'), ".--.-." => Some('@'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn synth(samples: &mut Vec<f32>, rate: u32, secs: f32, on: bool, pitch: f32) {
        let n = (secs * rate as f32) as usize;
        for i in 0..n {
            let t = i as f32 / rate as f32;
            samples.push(if on { (TAU * pitch * t).sin() * 0.5 } else { 0.0 });
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
            &EnvelopeConfig { pin_wpm: Some(20.0) },
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
        assert_eq!(final_snap.transcript, "PARIS", "got {:?}", final_snap.transcript);
    }

    #[test]
    fn kmeans_dot_estimate_separates_dits_and_dahs() {
        let mut durs = Vec::new();
        for _ in 0..6 { durs.push(0.060); }
        for _ in 0..6 { durs.push(0.180); }
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
        let on_count = viz.events.iter().filter(|e|
            matches!(e.kind, VizEventKind::OnDit | VizEventKind::OnDah)).count();
        assert!(on_count >= 14, "expected at least 14 on events, got {on_count}");
        assert!(viz.centroid_dah > viz.centroid_dot, "dah centroid > dot centroid");
        assert!(viz.wpm > 5.0 && viz.wpm < 40.0, "wpm out of range: {}", viz.wpm);
    }
}

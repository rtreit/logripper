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
        median_lower_half(&ons).max(MIN_ELEMENT_S)
    };

    // 5) Decode events into morse + gap tokens, then to text.
    decode_events(&ons, &offs, dot_s)
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
}

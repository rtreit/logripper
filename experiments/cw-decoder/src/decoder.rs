//! Wrapper around ditdah's MorseDecoder that captures WPM and pitch via the
//! installed log subscriber, and exposes a "decode this slice" helper.

use anyhow::Result;
use ditdah::decode_samples;

use crate::log_capture::{DitdahLogCapture, DitdahStats};

pub struct DecodeOutcome {
    pub text: String,
    pub stats: DitdahStats,
}

pub fn decode_text(samples: &[f32], sample_rate: u32) -> String {
    decode_samples(samples, sample_rate).unwrap_or_default()
}

/// Run ditdah on a slice of samples. The log capture is shared, so the most
/// recent WPM/pitch stats are returned alongside the decoded text.
pub fn decode_window(
    samples: &[f32],
    sample_rate: u32,
    capture: &DitdahLogCapture,
) -> Result<DecodeOutcome> {
    let text = decode_text(samples, sample_rate); // ditdah bails on tiny/empty buffers; treat as "no decode"
    let stats = capture.snapshot();
    let text = focused_long_capture_decode(samples, sample_rate, capture, &text).unwrap_or(text);
    Ok(DecodeOutcome { text, stats })
}

fn focused_long_capture_decode(
    samples: &[f32],
    sample_rate: u32,
    capture: &DitdahLogCapture,
    whole_file_text: &str,
) -> Option<String> {
    let duration_s = samples.len() as f32 / sample_rate as f32;
    if duration_s < 18.0 || normalized_len(whole_file_text) < 48 {
        return None;
    }

    let win_samples = (4.0 * sample_rate as f32).round() as usize;
    let hop_samples = (2.0 * sample_rate as f32).round() as usize;
    if win_samples == 0 || hop_samples == 0 || samples.len() < win_samples {
        return None;
    }

    let mut focused = Vec::new();
    let mut start = 0usize;
    while start + win_samples <= samples.len() {
        let text = decode_text(&samples[start..start + win_samples], sample_rate);
        let stats = capture.snapshot();
        if plausible_faded_cw_window(&text, stats) {
            focused.push(text);
        }
        start += hop_samples;
    }

    if focused.is_empty() {
        return None;
    }

    let repaired = repair_common_split_morse(&focused.join(" "));
    let focused_len = normalized_len(&repaired);
    if focused_len == 0 || focused_len >= normalized_len(whole_file_text) {
        return None;
    }

    Some(repaired)
}

fn plausible_faded_cw_window(text: &str, stats: DitdahStats) -> bool {
    let len = normalized_len(text);
    if !(2..=24).contains(&len) {
        return false;
    }

    let Some(wpm) = stats.wpm else {
        return false;
    };
    (8.0..=20.0).contains(&wpm)
}

fn normalized_len(text: &str) -> usize {
    text.chars().filter(|ch| ch.is_ascii_alphanumeric()).count()
}

fn repair_common_split_morse(text: &str) -> String {
    text.split_whitespace()
        .map(|token| token.replace("GT", "Q"))
        .collect::<Vec<_>>()
        .join(" ")
}

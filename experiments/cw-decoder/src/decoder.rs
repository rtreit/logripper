//! Wrapper around ditdah's MorseDecoder that captures WPM and pitch via the
//! installed log subscriber, and exposes a "decode this slice" helper.

use anyhow::Result;
use ditdah::decode_samples;

use crate::log_capture::{DitdahLogCapture, DitdahStats};

pub struct DecodeOutcome {
    pub text: String,
    pub stats: DitdahStats,
}

/// Run ditdah on a slice of samples. The log capture is shared, so the most
/// recent WPM/pitch stats are returned alongside the decoded text.
pub fn decode_window(
    samples: &[f32],
    sample_rate: u32,
    capture: &DitdahLogCapture,
) -> Result<DecodeOutcome> {
    let text = decode_samples(samples, sample_rate)
        .unwrap_or_default(); // ditdah bails on tiny/empty buffers; treat as "no decode"
    let stats = capture.snapshot();
    Ok(DecodeOutcome { text, stats })
}

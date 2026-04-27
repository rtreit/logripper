//! Round-2 streaming CW decoder backend: whole-buffer redecode using
//! pristine upstream `ditdah`.
//!
//! ## Design
//!
//! This module implements the validated approach from
//! `tools/rolling-whole-buffer/`:
//!
//! 1. Maintain an **append-only audio buffer** of all samples seen so far
//!    (capped at `MAX_BUFFER_SECS` to bound memory).
//! 2. Every `DECODE_EVERY_MS` milliseconds, call
//!    [`ditdah::decode_samples_with_params`] on the **entire buffer**.
//! 3. The result is the *current best* full transcript. The caller emits
//!    it as a `transcript` event that REPLACES whatever was shown before
//!    (no incremental commit; ditdah is not prefix-stable across growing
//!    buffers and trying to commit incrementally is what tanked the
//!    rolling-window backend in #343).
//!
//! ### Why this works
//!
//! Empirical results on `data/cw-samples/training-set-a/` (30 WPM, 100
//! abbreviations, 183 s):
//!
//! | Decoder                                    | clean CER | qrn CER |
//! |--------------------------------------------|-----------|---------|
//! | ditdah whole-file (this module's target)   | 0.046     | 0.046   |
//! | Legacy Goertzel `stream-live`              | 0.17      | 0.11    |
//! | Rolling 6-20 s window (#343 default)       | 0.89      | 0.90    |
//!
//! Whole-buffer ditdah delivers **3-4× better accuracy** than the
//! Goertzel path while the per-decode cost stays sub-100 ms even on a
//! 3-minute buffer (~67 ms at t=180 s on a workstation).
//!
//! ### Lock-state semantics
//!
//! ditdah re-detects WPM on every decode. We treat the session as
//! `Locked` once the last [`LOCK_WINDOW`] decoded WPMs all agree within
//! [`LOCK_WPM_TOLERANCE`]. Before that we report `Hunting`. The detected
//! WPM is exposed verbatim per decode so the GUI can show drift even
//! before lock.

use anyhow::Result;
use std::collections::VecDeque;

/// Hard upper bound on retained audio (≈10 minutes at 12 kHz).
pub const MAX_BUFFER_SECS: f32 = 600.0;

/// Default decode cadence for live use. The user-visible transcript
/// updates this often; lower values cost more CPU.
pub const DEFAULT_DECODE_EVERY_MS: u64 = 5_000;

/// Minimum buffered audio before the first decode runs. ditdah needs
/// enough samples to detect a pitch and fit a WPM.
pub const MIN_DECODE_AUDIO_SECS: f32 = 4.0;

/// Number of consecutive decodes whose detected WPM must agree within
/// [`LOCK_WPM_TOLERANCE`] before we report `Locked`.
pub const LOCK_WINDOW: usize = 3;

/// Tolerance (WPM) for considering two consecutive WPM detections "the
/// same".
pub const LOCK_WPM_TOLERANCE: f32 = 1.5;

/// One transcript snapshot from a whole-buffer decode.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptSnapshot {
    /// Full decoded text. Replaces (does not append to) any prior
    /// snapshot.
    pub text: String,
    /// WPM ditdah picked for this decode.
    pub wpm: f32,
    /// Pitch (Hz) ditdah picked for this decode.
    pub pitch_hz: f32,
    /// Wall-clock cost of the ditdah call, in milliseconds.
    pub decode_ms: u128,
    /// Total audio buffered at the time of the decode, in seconds.
    pub audio_secs: f32,
    /// Lock state derived from the WPM stability window.
    pub lock: LockState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockState {
    Hunting,
    Locked,
}

impl LockState {
    pub fn as_str(self) -> &'static str {
        match self {
            LockState::Hunting => "hunting",
            LockState::Locked => "locked",
        }
    }
}

/// Append-only audio buffer + ditdah whole-buffer redecode.
pub struct WholeBufferDecoder {
    sample_rate: u32,
    samples: Vec<f32>,
    max_samples: usize,
    recent_wpm: VecDeque<f32>,
    last_lock: LockState,
}

impl WholeBufferDecoder {
    pub fn new(sample_rate: u32) -> Self {
        let max_samples = (MAX_BUFFER_SECS * sample_rate as f32) as usize;
        Self {
            sample_rate,
            samples: Vec::with_capacity(sample_rate as usize * 60),
            max_samples,
            recent_wpm: VecDeque::with_capacity(LOCK_WINDOW),
            last_lock: LockState::Hunting,
        }
    }

    /// Append new audio samples to the buffer. Drops oldest samples when
    /// the buffer grows beyond [`MAX_BUFFER_SECS`].
    pub fn feed(&mut self, chunk: &[f32]) {
        self.samples.extend_from_slice(chunk);
        if self.samples.len() > self.max_samples {
            let drop = self.samples.len() - self.max_samples;
            self.samples.drain(..drop);
        }
    }

    /// Total audio currently buffered, in seconds.
    pub fn audio_secs(&self) -> f32 {
        self.samples.len() as f32 / self.sample_rate as f32
    }

    /// Reset the buffer (e.g. operator hit Esc / "new QSO"). Lock state
    /// resets to Hunting.
    pub fn reset(&mut self) {
        self.samples.clear();
        self.recent_wpm.clear();
        self.last_lock = LockState::Hunting;
    }

    /// Run ditdah on the entire buffer. Returns `Ok(None)` if the
    /// buffer is below [`MIN_DECODE_AUDIO_SECS`].
    pub fn decode(&mut self) -> Result<Option<TranscriptSnapshot>> {
        if self.audio_secs() < MIN_DECODE_AUDIO_SECS {
            return Ok(None);
        }
        let started = std::time::Instant::now();
        let (text, wpm, _threshold) =
            ditdah::decode_samples_with_params(&self.samples, self.sample_rate, None, None)?;
        let decode_ms = started.elapsed().as_millis();
        let pitch_hz = ditdah_estimated_pitch(&self.samples, self.sample_rate).unwrap_or(0.0);

        // Update lock-state window.
        if self.recent_wpm.len() == LOCK_WINDOW {
            self.recent_wpm.pop_front();
        }
        self.recent_wpm.push_back(wpm);
        let lock = if self.recent_wpm.len() == LOCK_WINDOW {
            let min = self
                .recent_wpm
                .iter()
                .copied()
                .fold(f32::INFINITY, f32::min);
            let max = self
                .recent_wpm
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max);
            if (max - min) <= LOCK_WPM_TOLERANCE {
                LockState::Locked
            } else {
                LockState::Hunting
            }
        } else {
            LockState::Hunting
        };
        self.last_lock = lock;

        Ok(Some(TranscriptSnapshot {
            text: normalize_transcript(&text),
            wpm,
            pitch_hz,
            decode_ms,
            audio_secs: self.audio_secs(),
            lock,
        }))
    }

    pub fn last_lock(&self) -> LockState {
        self.last_lock
    }
}

/// Collapse internal whitespace runs so the GUI gets canonical
/// single-space-separated tokens. Trim leading/trailing whitespace.
fn normalize_transcript(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Cheap pitch estimator used purely for diagnostic display
/// (`pitch_hz` field on each transcript). Currently a thin wrapper
/// around `ditdah`'s public estimator if it exposes one; otherwise
/// returns `None`. We intentionally keep this lightweight rather than
/// re-running an FFT — accuracy is not needed because ditdah has
/// already used the same pitch internally for decoding.
fn ditdah_estimated_pitch(_samples: &[f32], _rate: u32) -> Option<f32> {
    // ditdah does not expose its internal `detect_pitch_stft` publicly.
    // We deliberately return `None` rather than re-running an STFT here:
    // the GUI cares about WPM and the decoded text; pitch is a nice-to-
    // have diagnostic that can be added in a follow-up that exposes the
    // ditdah-side estimate via the params API.
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_buffer_decode_returns_none() {
        let mut d = WholeBufferDecoder::new(12_000);
        assert!(d.decode().unwrap().is_none());
    }

    #[test]
    fn feed_drops_oldest_above_max_seconds() {
        let mut d = WholeBufferDecoder::new(12_000);
        // 605 seconds of dummy samples; should clamp to 600 s.
        let chunk = vec![0.0_f32; 12_000 * 5];
        for _ in 0..121 {
            d.feed(&chunk);
        }
        let secs = d.audio_secs();
        assert!(
            (secs - MAX_BUFFER_SECS).abs() < 1.0,
            "expected ~{} s buffered, got {}",
            MAX_BUFFER_SECS,
            secs
        );
    }

    #[test]
    fn reset_clears_buffer_and_lock_state() {
        let mut d = WholeBufferDecoder::new(12_000);
        d.feed(&vec![0.1_f32; 12_000 * 10]);
        d.reset();
        assert_eq!(d.audio_secs(), 0.0);
        assert_eq!(d.last_lock(), LockState::Hunting);
    }

    #[test]
    fn normalize_collapses_whitespace_runs() {
        assert_eq!(normalize_transcript("  CQ  DE   K1ABC \n"), "CQ DE K1ABC");
    }
}

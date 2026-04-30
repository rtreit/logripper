//! Append-only event-stream decoder used as the stable CW foundation.
//!
//! The input is the same `VizFrame` event stream rendered by the Visualizer:
//! classified on/off bars with absolute sample anchors.  Unlike the rolling
//! text decoders, this module consumes each stable event once in audio order
//! and appends decoded characters.  Future, more complex pipelines should
//! compare against this baseline rather than replacing it silently.

use crate::envelope_decoder::{self, LiveEnvelopeStreamer, VizEventKind, VizFrame};

const EPSILON_SAMPLES: u64 = 16;

#[derive(Debug, Clone, Default)]
pub struct AppendDecodeUpdate {
    pub decoded_text: String,
    pub raw_stream: String,
    pub changed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AppendEventDecoder {
    last_emitted_end_sample: u64,
    raw_stream: String,
    decoded_text: String,
    pending_morse: String,
}

impl AppendEventDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn decoded_text(&self) -> &str {
        &self.decoded_text
    }

    pub fn raw_stream(&self) -> &str {
        &self.raw_stream
    }

    pub fn ingest_viz(&mut self, viz: &VizFrame) -> AppendDecodeUpdate {
        let sr = viz.sample_rate.max(1) as f32;
        let window_start = viz.window_start_sample;
        let window_end = viz.window_end_sample;
        if window_end <= window_start {
            return self.snapshot(false);
        }

        let dot_s = if viz.dot_seconds > 0.0 {
            viz.dot_seconds
        } else if viz.wpm > 0.0 {
            1.2 / viz.wpm
        } else {
            0.04
        };
        let guard_samples = (sr * f32::max(0.10, 8.0 * dot_s)) as u64;
        let stable_end = window_end.saturating_sub(guard_samples);
        let mut changed = false;

        for ev in &viz.events {
            let end_s = ev.end_s.max(0.0);
            let event_end = window_start.saturating_add((end_s * sr).round() as u64);
            if event_end > stable_end {
                continue;
            }
            if event_end <= self.last_emitted_end_sample.saturating_add(EPSILON_SAMPLES) {
                continue;
            }

            match ev.kind {
                VizEventKind::OnDit => {
                    self.pending_morse.push('.');
                    self.raw_stream.push('.');
                }
                VizEventKind::OnDah => {
                    self.pending_morse.push('-');
                    self.raw_stream.push('-');
                }
                VizEventKind::OffIntra => {}
                VizEventKind::OffChar => {
                    self.flush_pending_char();
                    self.raw_stream.push('/');
                    changed = true;
                }
                VizEventKind::OffWord => {
                    self.flush_pending_char();
                    if !self.decoded_text.ends_with(' ') && !self.decoded_text.is_empty() {
                        self.decoded_text.push(' ');
                    }
                    self.raw_stream.push_str("//");
                    changed = true;
                }
            }
            self.last_emitted_end_sample = event_end;
        }

        self.snapshot(changed)
    }

    pub fn flush(&mut self) -> AppendDecodeUpdate {
        let changed = self.flush_pending_char();
        self.snapshot(changed)
    }

    fn flush_pending_char(&mut self) -> bool {
        if self.pending_morse.is_empty() {
            return false;
        }
        let ch = envelope_decoder::morse_to_char(&self.pending_morse).unwrap_or('?');
        self.decoded_text.push(ch);
        self.pending_morse.clear();
        true
    }

    fn snapshot(&self, changed: bool) -> AppendDecodeUpdate {
        AppendDecodeUpdate {
            decoded_text: self.decoded_text.clone(),
            raw_stream: self.raw_stream.clone(),
            changed,
        }
    }
}

pub fn decode_samples_append(
    samples: &[f32],
    sample_rate: u32,
    pin_wpm: Option<f32>,
    pin_hz: Option<f32>,
    min_snr_db: f32,
) -> AppendDecodeUpdate {
    let mut streamer = LiveEnvelopeStreamer::new(sample_rate);
    streamer.set_pinned_wpm(pin_wpm);
    streamer.set_pinned_hz(pin_hz);
    streamer.set_min_snr_db(min_snr_db);

    let chunk = ((sample_rate as usize) / 20).max(1);
    let mut decoder = AppendEventDecoder::new();
    let mut cursor = 0usize;
    while cursor < samples.len() {
        let end = (cursor + chunk).min(samples.len());
        streamer.feed(&samples[cursor..end]);
        if let Some(viz) = streamer.flush_with_viz().viz {
            decoder.ingest_viz(&viz);
        }
        cursor = end;
    }
    if let Some(viz) = streamer.flush_with_viz().viz {
        decoder.ingest_viz(&viz);
    }
    decoder.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope_decoder::{VizEvent, VizEventKind};

    fn frame(events: Vec<VizEvent>, window_end_sample: u64) -> VizFrame {
        VizFrame {
            sample_rate: 1_000,
            buffer_seconds: window_end_sample as f32 / 1_000.0,
            frame_step_s: 0.005,
            pitch_hz: 700.0,
            envelope: vec![],
            envelope_max: 1.0,
            noise_floor: 0.1,
            signal_floor: 1.0,
            snr_db: 20.0,
            snr_suppressed: false,
            hyst_high: 0.8,
            hyst_low: 0.4,
            events,
            on_durations: vec![],
            dot_seconds: 0.04,
            wpm: 30.0,
            centroid_dot: 0.04,
            centroid_dah: 0.12,
            locked_wpm: Some(30.0),
            window_start_sample: 0,
            window_end_sample,
        }
    }

    #[test]
    fn append_decoder_decodes_events_once_with_word_spaces() {
        let events = vec![
            VizEvent {
                start_s: 0.00,
                end_s: 0.04,
                duration_s: 0.04,
                kind: VizEventKind::OnDit,
            },
            VizEvent {
                start_s: 0.08,
                end_s: 0.12,
                duration_s: 0.04,
                kind: VizEventKind::OnDit,
            },
            VizEvent {
                start_s: 0.16,
                end_s: 0.20,
                duration_s: 0.04,
                kind: VizEventKind::OnDit,
            },
            VizEvent {
                start_s: 0.20,
                end_s: 0.35,
                duration_s: 0.15,
                kind: VizEventKind::OffChar,
            },
            VizEvent {
                start_s: 0.35,
                end_s: 0.39,
                duration_s: 0.04,
                kind: VizEventKind::OnDit,
            },
            VizEvent {
                start_s: 0.43,
                end_s: 0.55,
                duration_s: 0.12,
                kind: VizEventKind::OnDah,
            },
            VizEvent {
                start_s: 0.55,
                end_s: 0.90,
                duration_s: 0.35,
                kind: VizEventKind::OffWord,
            },
        ];
        let mut d = AppendEventDecoder::new();
        let update = d.ingest_viz(&frame(events.clone(), 2_000));
        assert_eq!(update.raw_stream, ".../.-//");
        assert_eq!(update.decoded_text, "SA ");

        let update = d.ingest_viz(&frame(events, 2_000));
        assert!(!update.changed);
        assert_eq!(update.raw_stream, ".../.-//");
        assert_eq!(update.decoded_text, "SA ");
    }
}

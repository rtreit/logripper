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

    let chunk = ((sample_rate as usize) / 4).max(1);
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

/// Rich bench-grade output of `decode_samples_append_bench`. Captures
/// not just the final transcript but per-char commit-time (so the
/// BENCH harness can compute stable-N latency that a live operator
/// would actually experience), the V3 quality-gate timeline (so we can
/// report gate drops / uptime / longest-closed independently of the
/// V2 confidence machine), and the final pitch / locked WPM the
/// streamer settled on.
#[derive(Debug, Clone, Default)]
pub struct AppendBenchOutput {
    pub decoded_text: String,
    pub raw_stream: String,
    /// One entry per char in `decoded_text`, in emission order
    /// (including any space emitted on `OffWord` and any final char
    /// emitted by `flush()`). The timestamp is the `window_end_ms`
    /// of the `VizFrame` whose ingest caused the
    /// `AppendEventDecoder` to commit that char — i.e. the commit
    /// time, not the raw event-end time. The append decoder enforces
    /// a stability guard so this is later than `event.end_s`, and
    /// represents when a live operator would actually have seen the
    /// character appear.
    pub char_times_ms: Vec<u32>,
    /// First sample-time (ms) at which a non-trivial CW event
    /// (`OnDit` or `OnDah`) appeared in any VizFrame.
    pub t_first_event_ms: Option<u32>,
    /// First sample-time (ms) at which the V3 quality gate was open
    /// (`!snr_suppressed`) AND `locked_wpm` was set. None when no
    /// foundation lock happened during the run.
    pub t_first_lock_ms: Option<u32>,
    pub final_pitch_hz: Option<f32>,
    pub final_locked_wpm: Option<f32>,
    /// Count of quality-gate open→closed transitions after first
    /// foundation lock. The gate fuses SNR floor and dynamic-range
    /// bimodality so this is "stability lost", not just "pitch lost".
    pub quality_gate_drops: usize,
    /// Count of quality-gate closed→open transitions after first
    /// foundation lock.
    pub quality_gate_recoveries: usize,
    /// Total ms with the gate open after first lock (and before
    /// end-of-audio).
    pub gate_open_ms_after_lock: u32,
    /// Total ms with the gate closed after first lock.
    pub gate_closed_ms_after_lock: u32,
    /// Longest single closed-gate stretch (ms) after first lock.
    pub longest_gate_closed_ms: u32,
}

/// Bench-grade entry point used by `cw-decoder bench-latency
/// --foundation`. Drives the same `LiveEnvelopeStreamer` →
/// `AppendEventDecoder` pipeline as the live Visualizer/DECODE path,
/// but instruments commit-time per char and the quality-gate
/// timeline so the BENCH harness can produce metrics comparable to
/// the legacy V2 streaming-decoder bench rows.
pub fn decode_samples_append_bench(
    samples: &[f32],
    sample_rate: u32,
    pin_wpm: Option<f32>,
    pin_hz: Option<f32>,
    min_snr_db: f32,
) -> AppendBenchOutput {
    let mut streamer = LiveEnvelopeStreamer::new(sample_rate);
    streamer.set_pinned_wpm(pin_wpm);
    streamer.set_pinned_hz(pin_hz);
    streamer.set_min_snr_db(min_snr_db);

    let chunk = ((sample_rate as usize) / 4).max(1);
    let mut decoder = AppendEventDecoder::new();
    let mut out = AppendBenchOutput::default();

    let sr_for_ms = sample_rate.max(1) as u64;
    let frame_end_ms_of = |viz: &VizFrame| -> u32 {
        ((viz.window_end_sample.saturating_mul(1000)) / viz.sample_rate.max(1) as u64) as u32
    };

    let mut prev_text_len = 0usize;
    let mut first_lock_ms: Option<u32> = None;
    let mut prev_gate_open: Option<bool> = None;
    let mut prev_frame_end_ms: u32 = 0;
    let mut current_closed_run_ms: u32 = 0;
    let mut longest_closed_run_ms: u32 = 0;
    let mut gate_open_ms = 0u32;
    let mut gate_closed_ms = 0u32;
    let mut drops = 0usize;
    let mut recoveries = 0usize;
    let mut last_pitch_hz: Option<f32> = None;
    let mut last_locked_wpm: Option<f32> = None;

    let process_viz = |viz: &VizFrame,
                       decoder: &mut AppendEventDecoder,
                       out: &mut AppendBenchOutput,
                       prev_text_len: &mut usize,
                       first_lock_ms: &mut Option<u32>,
                       prev_gate_open: &mut Option<bool>,
                       prev_frame_end_ms: &mut u32,
                       current_closed_run_ms: &mut u32,
                       longest_closed_run_ms: &mut u32,
                       gate_open_ms: &mut u32,
                       gate_closed_ms: &mut u32,
                       drops: &mut usize,
                       recoveries: &mut usize,
                       last_pitch_hz: &mut Option<f32>,
                       last_locked_wpm: &mut Option<f32>| {
        let frame_end_ms = frame_end_ms_of(viz);
        if out.t_first_event_ms.is_none()
            && viz
                .events
                .iter()
                .any(|e| matches!(e.kind, VizEventKind::OnDit | VizEventKind::OnDah))
        {
            out.t_first_event_ms = Some(frame_end_ms);
        }
        let gate_open = !viz.snr_suppressed && viz.locked_wpm.is_some();
        if first_lock_ms.is_none() {
            if gate_open {
                *first_lock_ms = Some(frame_end_ms);
                *prev_gate_open = Some(true);
                *prev_frame_end_ms = frame_end_ms;
            }
        } else {
            let dt = frame_end_ms.saturating_sub(*prev_frame_end_ms);
            let was_open = prev_gate_open.unwrap_or(true);
            if was_open {
                *gate_open_ms = gate_open_ms.saturating_add(dt);
            } else {
                *gate_closed_ms = gate_closed_ms.saturating_add(dt);
                *current_closed_run_ms = current_closed_run_ms.saturating_add(dt);
            }
            if was_open != gate_open {
                if was_open {
                    *drops += 1;
                    *current_closed_run_ms = 0;
                } else {
                    *recoveries += 1;
                    if *current_closed_run_ms > *longest_closed_run_ms {
                        *longest_closed_run_ms = *current_closed_run_ms;
                    }
                    *current_closed_run_ms = 0;
                }
            }
            *prev_gate_open = Some(gate_open);
            *prev_frame_end_ms = frame_end_ms;
        }
        *last_pitch_hz = Some(viz.pitch_hz);
        if viz.locked_wpm.is_some() {
            *last_locked_wpm = viz.locked_wpm;
        }
        decoder.ingest_viz(viz);
        let new_len = decoder.decoded_text().chars().count();
        for _ in *prev_text_len..new_len {
            out.char_times_ms.push(frame_end_ms);
        }
        *prev_text_len = new_len;
    };

    let mut cursor = 0usize;
    while cursor < samples.len() {
        let end = (cursor + chunk).min(samples.len());
        streamer.feed(&samples[cursor..end]);
        if let Some(viz) = streamer.flush_with_viz().viz {
            process_viz(
                &viz,
                &mut decoder,
                &mut out,
                &mut prev_text_len,
                &mut first_lock_ms,
                &mut prev_gate_open,
                &mut prev_frame_end_ms,
                &mut current_closed_run_ms,
                &mut longest_closed_run_ms,
                &mut gate_open_ms,
                &mut gate_closed_ms,
                &mut drops,
                &mut recoveries,
                &mut last_pitch_hz,
                &mut last_locked_wpm,
            );
        }
        cursor = end;
    }
    if let Some(viz) = streamer.flush_with_viz().viz {
        process_viz(
            &viz,
            &mut decoder,
            &mut out,
            &mut prev_text_len,
            &mut first_lock_ms,
            &mut prev_gate_open,
            &mut prev_frame_end_ms,
            &mut current_closed_run_ms,
            &mut longest_closed_run_ms,
            &mut gate_open_ms,
            &mut gate_closed_ms,
            &mut drops,
            &mut recoveries,
            &mut last_pitch_hz,
            &mut last_locked_wpm,
        );
    }

    let end_of_audio_ms = ((samples.len() as u64 * 1000) / sr_for_ms) as u32;
    let _ = decoder.flush();
    let final_text = decoder.decoded_text().to_string();
    let final_len = final_text.chars().count();
    for _ in prev_text_len..final_len {
        out.char_times_ms.push(end_of_audio_ms);
    }

    if first_lock_ms.is_some() {
        let dt = end_of_audio_ms.saturating_sub(prev_frame_end_ms);
        let was_open = prev_gate_open.unwrap_or(true);
        if was_open {
            gate_open_ms = gate_open_ms.saturating_add(dt);
        } else {
            gate_closed_ms = gate_closed_ms.saturating_add(dt);
            current_closed_run_ms = current_closed_run_ms.saturating_add(dt);
        }
        if current_closed_run_ms > longest_closed_run_ms {
            longest_closed_run_ms = current_closed_run_ms;
        }
    }

    out.decoded_text = final_text;
    out.raw_stream = decoder.raw_stream().to_string();
    out.t_first_lock_ms = first_lock_ms;
    out.final_pitch_hz = last_pitch_hz;
    out.final_locked_wpm = last_locked_wpm;
    out.quality_gate_drops = drops;
    out.quality_gate_recoveries = recoveries;
    out.gate_open_ms_after_lock = gate_open_ms;
    out.gate_closed_ms_after_lock = gate_closed_ms;
    out.longest_gate_closed_ms = longest_closed_run_ms;
    out
}

pub fn decode_samples_append_exact_window(
    samples: &[f32],
    sample_rate: u32,
    pin_wpm: Option<f32>,
    pin_hz: Option<f32>,
    min_snr_db: f32,
) -> AppendDecodeUpdate {
    let cfg = envelope_decoder::EnvelopeConfig {
        pin_wpm,
        pin_hz,
        min_snr_db,
        ..envelope_decoder::EnvelopeConfig::default()
    };
    let (_, viz) = envelope_decoder::decode_envelope_with_viz(samples, sample_rate, &cfg);
    let mut decoder = AppendEventDecoder::new();
    decoder.ingest_viz(&viz);
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
            wpm_kmeans: 30.0,
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

    #[test]
    fn bench_output_records_one_timestamp_per_appended_char() {
        // 8s of clean PARIS at 700 Hz, 20 wpm. The append decoder
        // should produce a non-empty transcript and the bench output
        // must record one char_time per char of decoded_text
        // (including word-gap spaces and the final flush char).
        let sr = 16_000u32;
        let cw = crate::bench_latency::synth_paris(sr, 700.0, 20.0, 8.0);
        let mut samples: Vec<f32> = vec![0.0; sr as usize]; // 1 s lead-in
        samples.extend(cw);
        let out = decode_samples_append_bench(&samples, sr, None, None, 6.0);
        assert!(
            !out.decoded_text.is_empty(),
            "expected non-empty transcript on clean CW"
        );
        assert_eq!(
            out.char_times_ms.len(),
            out.decoded_text.chars().count(),
            "char_times_ms must align 1:1 with decoded_text chars (text={:?})",
            out.decoded_text
        );
    }

    #[test]
    fn bench_clean_cw_acquires_lock_with_no_gate_drops() {
        let sr = 16_000u32;
        let cw = crate::bench_latency::synth_paris(sr, 700.0, 20.0, 8.0);
        let mut samples: Vec<f32> = vec![0.0; sr as usize];
        samples.extend(cw);
        let out = decode_samples_append_bench(&samples, sr, None, None, 6.0);
        assert!(
            out.t_first_lock_ms.is_some(),
            "expected V3 quality gate to open on clean CW"
        );
        assert_eq!(
            out.quality_gate_drops, 0,
            "clean CW should not show gate drops (got {} drops)",
            out.quality_gate_drops
        );
    }
}

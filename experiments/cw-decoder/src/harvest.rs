use anyhow::Result;

use crate::decoder;
use crate::log_capture::DitdahLogCapture;
use crate::streaming::{DecoderConfig, StreamEvent, StreamingDecoder};

#[derive(Debug, Clone)]
pub struct HarvestConfig {
    pub window_seconds: f32,
    pub hop_seconds: f32,
    pub chunk_ms: u32,
    pub top: usize,
    pub min_shared_chars: usize,
    pub needles: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct StreamSummary {
    pub transcript: String,
    pub pitch_hz: Option<f32>,
    pub wpm: Option<f32>,
    pub threshold: f32,
}

#[derive(Debug, Clone)]
pub struct WindowCandidate {
    pub start_s: f32,
    pub end_s: f32,
    pub is_fallback: bool,
    pub member_count: usize,
    pub shared_chars: usize,
    pub strongest_copy_len: usize,
    pub matched_needles: Vec<String>,
    pub offline_text: String,
    pub offline_pitch_hz: Option<f32>,
    pub offline_wpm: Option<f32>,
    pub stream_text: String,
    pub stream_pitch_hz: Option<f32>,
    pub stream_wpm: Option<f32>,
    pub stream_threshold: f32,
}

#[derive(Debug, Clone)]
pub struct SignalProfilePoint {
    pub time_s: f32,
    pub power: f32,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct SignalProfile {
    pub display_start_s: f32,
    pub display_end_s: f32,
    pub selection_start_s: f32,
    pub selection_end_s: f32,
    pub suggested_start_s: f32,
    pub suggested_end_s: f32,
    pub pitch_hz: f32,
    pub threshold: f32,
    pub frame_step_s: f32,
    pub frame_len_s: f32,
    pub points: Vec<SignalProfilePoint>,
}

#[allow(dead_code)]
pub fn harvest_candidates(
    samples: &[f32],
    sample_rate: u32,
    log_capture: &DitdahLogCapture,
    stream_cfg: DecoderConfig,
    cfg: &HarvestConfig,
) -> Result<Vec<WindowCandidate>> {
    harvest_candidates_with_progress(
        samples,
        sample_rate,
        log_capture,
        stream_cfg,
        cfg,
        |_completed, _total, _start_s, _end_s| {},
    )
}

pub fn harvest_candidates_with_progress<F>(
    samples: &[f32],
    sample_rate: u32,
    log_capture: &DitdahLogCapture,
    stream_cfg: DecoderConfig,
    cfg: &HarvestConfig,
    mut on_progress: F,
) -> Result<Vec<WindowCandidate>>
where
    F: FnMut(usize, usize, f32, f32),
{
    let win_samples = ((cfg.window_seconds * sample_rate as f32) as usize).max(1);
    let hop_samples = ((cfg.hop_seconds * sample_rate as f32) as usize).max(1);
    let total_windows = if samples.len() >= win_samples {
        ((samples.len() - win_samples) / hop_samples) + 1
    } else {
        0
    };
    let normalized_needles: Vec<String> = cfg
        .needles
        .iter()
        .map(|needle| compact_normalized(needle))
        .filter(|needle| !needle.is_empty())
        .collect();

    let mut out = Vec::new();
    let mut start = 0usize;
    let mut completed_windows = 0usize;
    while start + win_samples <= samples.len() {
        let end = start + win_samples;
        let slice = &samples[start..end];

        let offline = decoder::decode_window(slice, sample_rate, log_capture)?;
        let stream = stream_decode_window(slice, sample_rate, stream_cfg, cfg.chunk_ms)?;

        let offline_compact = compact_normalized(&offline.text);
        let stream_compact = compact_normalized(&stream.transcript);
        let shared_chars = longest_common_substring_len(&offline_compact, &stream_compact);
        let strongest_copy_len = offline_compact.len().max(stream_compact.len());
        let matched_needles =
            matched_needles(&normalized_needles, &offline_compact, &stream_compact);

        let keep = if normalized_needles.is_empty() {
            should_keep_without_needles(
                shared_chars,
                strongest_copy_len,
                cfg.min_shared_chars,
                offline.stats.pitch_hz,
                stream.pitch_hz,
                offline.stats.wpm,
                stream.wpm,
            )
        } else {
            !matched_needles.is_empty()
        };

        if keep {
            out.push(WindowCandidate {
                start_s: start as f32 / sample_rate as f32,
                end_s: end as f32 / sample_rate as f32,
                is_fallback: false,
                member_count: 1,
                shared_chars,
                strongest_copy_len,
                matched_needles,
                offline_text: offline.text.trim().to_string(),
                offline_pitch_hz: offline.stats.pitch_hz,
                offline_wpm: offline.stats.wpm,
                stream_text: stream.transcript.trim().to_string(),
                stream_pitch_hz: stream.pitch_hz,
                stream_wpm: stream.wpm,
                stream_threshold: stream.threshold,
            });
        }

        completed_windows += 1;
        on_progress(
            completed_windows,
            total_windows,
            start as f32 / sample_rate as f32,
            end as f32 / sample_rate as f32,
        );
        start += hop_samples;
    }

    out = merge_overlapping_candidates(out, cfg.hop_seconds);
    snap_candidates_to_pauses(
        &mut out,
        samples,
        sample_rate,
        (cfg.window_seconds * 0.5).max(cfg.hop_seconds),
    );
    out.sort_by(|a, b| {
        b.matched_needles
            .len()
            .cmp(&a.matched_needles.len())
            .then_with(|| b.shared_chars.cmp(&a.shared_chars))
            .then_with(|| b.strongest_copy_len.cmp(&a.strongest_copy_len))
            .then_with(|| {
                b.stream_wpm
                    .unwrap_or_default()
                    .partial_cmp(&a.stream_wpm.unwrap_or_default())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                a.start_s
                    .partial_cmp(&b.start_s)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    out.truncate(cfg.top);
    if out.is_empty() && !samples.is_empty() {
        out.push(build_full_file_fallback_candidate(
            samples,
            sample_rate,
            log_capture,
            stream_cfg,
            cfg.chunk_ms,
        )?);
    }
    Ok(out)
}

pub fn build_signal_profile(
    samples: &[f32],
    sample_rate: u32,
    selection_start_s: f32,
    selection_end_s: f32,
    pitch_hz: Option<f32>,
    wpm: Option<f32>,
) -> Option<SignalProfile> {
    let total_duration_s = samples.len() as f32 / sample_rate as f32;
    let window_s = (selection_end_s - selection_start_s).max(0.1);
    let context_before_s = (window_s * 0.5).clamp(0.5, 2.0);
    let context_after_s = (window_s * 1.0).clamp(1.0, 4.0);
    let display_start_s = (selection_start_s - context_before_s).max(0.0);
    let display_end_s = (selection_end_s + context_after_s).min(total_duration_s);
    let profile = build_display_profile(
        samples,
        sample_rate,
        pitch_hz,
        display_start_s,
        display_end_s,
    )?;

    let min_pause_frames =
        adaptive_pause_frames(&profile.active, estimated_dot_s(wpm), profile.frame_step_s);
    let selection_start_frame = (((selection_start_s - profile.start_s) / profile.frame_step_s)
        .floor()
        .max(0.0)) as usize;
    let selection_end_frame = (((selection_end_s - profile.start_s) / profile.frame_step_s)
        .ceil()
        .max(0.0)) as usize;
    let suggested = best_pause_bounded_span(
        &profile.active,
        selection_start_frame.min(profile.active.len()),
        selection_end_frame.min(profile.active.len()),
        min_pause_frames,
    );

    let suggested_start_s = suggested
        .map(|span| (profile.start_s + span.start as f32 * profile.frame_step_s).max(0.0))
        .unwrap_or(selection_start_s);
    let suggested_end_s = suggested
        .map(|span| {
            (profile.start_s + span.end as f32 * profile.frame_step_s + profile.frame_len_s)
                .min(total_duration_s)
        })
        .unwrap_or(selection_end_s);

    let points = profile
        .powers
        .iter()
        .zip(profile.active.iter())
        .enumerate()
        .map(|(index, (&power, &active))| SignalProfilePoint {
            time_s: profile.start_s + index as f32 * profile.frame_step_s,
            power,
            active,
        })
        .collect();

    Some(SignalProfile {
        display_start_s: profile.start_s,
        display_end_s: profile.end_s,
        selection_start_s,
        selection_end_s,
        suggested_start_s,
        suggested_end_s,
        pitch_hz: pitch_hz.unwrap_or_default(),
        threshold: profile.threshold,
        frame_step_s: profile.frame_step_s,
        frame_len_s: profile.frame_len_s,
        points,
    })
}

fn merge_overlapping_candidates(
    mut candidates: Vec<WindowCandidate>,
    max_gap_s: f32,
) -> Vec<WindowCandidate> {
    if candidates.is_empty() {
        return candidates;
    }

    candidates.sort_by(|a, b| {
        a.start_s
            .partial_cmp(&b.start_s)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                a.end_s
                    .partial_cmp(&b.end_s)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let mut merged: Vec<WindowCandidate> = Vec::new();
    for candidate in candidates {
        if let Some(last) = merged.last_mut() {
            if candidates_belong_to_same_region(last, &candidate, max_gap_s) {
                merge_candidate_into(last, candidate);
            } else {
                merged.push(candidate);
            }
        } else {
            merged.push(candidate);
        }
    }

    merged
}

fn build_full_file_fallback_candidate(
    samples: &[f32],
    sample_rate: u32,
    log_capture: &DitdahLogCapture,
    stream_cfg: DecoderConfig,
    chunk_ms: u32,
) -> Result<WindowCandidate> {
    let offline = decoder::decode_window(samples, sample_rate, log_capture)?;
    let stream = stream_decode_window(samples, sample_rate, stream_cfg, chunk_ms)?;
    let offline_compact = compact_normalized(&offline.text);
    let stream_compact = compact_normalized(&stream.transcript);

    Ok(WindowCandidate {
        start_s: 0.0,
        end_s: samples.len() as f32 / sample_rate as f32,
        is_fallback: true,
        member_count: 1,
        shared_chars: longest_common_substring_len(&offline_compact, &stream_compact),
        strongest_copy_len: offline_compact.len().max(stream_compact.len()),
        matched_needles: Vec::new(),
        offline_text: offline.text.trim().to_string(),
        offline_pitch_hz: offline.stats.pitch_hz,
        offline_wpm: offline.stats.wpm,
        stream_text: stream.transcript.trim().to_string(),
        stream_pitch_hz: stream.pitch_hz,
        stream_wpm: stream.wpm,
        stream_threshold: stream.threshold,
    })
}

fn snap_candidates_to_pauses(
    candidates: &mut [WindowCandidate],
    samples: &[f32],
    sample_rate: u32,
    search_margin_s: f32,
) {
    for candidate in candidates {
        let Some((start_s, end_s)) =
            pause_bounded_span(candidate, samples, sample_rate, search_margin_s)
        else {
            continue;
        };

        if end_s > start_s {
            candidate.start_s = start_s;
            candidate.end_s = end_s;
        }
    }
}

fn pause_bounded_span(
    candidate: &WindowCandidate,
    samples: &[f32],
    sample_rate: u32,
    search_margin_s: f32,
) -> Option<(f32, f32)> {
    let pitch_hz = candidate.stream_pitch_hz.or(candidate.offline_pitch_hz)?;
    let total_duration_s = samples.len() as f32 / sample_rate as f32;
    let base_margin_s = search_margin_s.max(0.25);
    let search_start_s = (candidate.start_s - base_margin_s).max(0.0);
    // Give the tail extra room to find a real pause, but keep the extension
    // bounded so one candidate does not swallow an entire recording.
    let search_end_s = (candidate.end_s + base_margin_s + tail_extension_budget_s(candidate))
        .min(total_duration_s);
    let profile =
        build_activity_profile(samples, sample_rate, pitch_hz, search_start_s, search_end_s)?;
    let min_pause_frames = adaptive_pause_frames(
        &profile.active,
        estimated_dot_s(candidate.stream_wpm.or(candidate.offline_wpm)),
        profile.frame_step_s,
    );
    let candidate_start_frame = (((candidate.start_s - profile.start_s) / profile.frame_step_s)
        .floor()
        .max(0.0)) as usize;
    let candidate_end_frame = (((candidate.end_s - profile.start_s) / profile.frame_step_s)
        .ceil()
        .max(0.0)) as usize;
    let span = best_pause_bounded_span(
        &profile.active,
        candidate_start_frame.min(profile.active.len()),
        candidate_end_frame.min(profile.active.len()),
        min_pause_frames,
    )?;

    let snapped_start_s = (profile.start_s + span.start as f32 * profile.frame_step_s).max(0.0);
    let snapped_end_s =
        (profile.start_s + span.end as f32 * profile.frame_step_s + profile.frame_len_s)
            .min(total_duration_s);
    Some((snapped_start_s, snapped_end_s))
}

fn tail_extension_budget_s(candidate: &WindowCandidate) -> f32 {
    let initial_window_s = (candidate.end_s - candidate.start_s).max(0.0);
    (initial_window_s * 0.75).clamp(0.75, 3.0)
}

#[derive(Debug, Clone)]
struct ActivityProfile {
    start_s: f32,
    end_s: f32,
    frame_len_s: f32,
    frame_step_s: f32,
    powers: Vec<f32>,
    threshold: f32,
    active: Vec<bool>,
}

fn build_activity_profile(
    samples: &[f32],
    sample_rate: u32,
    pitch_hz: f32,
    search_start_s: f32,
    search_end_s: f32,
) -> Option<ActivityProfile> {
    if search_end_s <= search_start_s {
        return None;
    }

    let frame_len_s = 0.025;
    let frame_step_s = 0.010;
    let frame_len = ((frame_len_s * sample_rate as f32).round() as usize).max(16);
    let frame_step = ((frame_step_s * sample_rate as f32).round() as usize).max(8);
    let search_start = ((search_start_s * sample_rate as f32).floor() as usize).min(samples.len());
    let search_end = ((search_end_s * sample_rate as f32).ceil() as usize).min(samples.len());
    if search_end <= search_start + frame_len {
        return None;
    }

    let search_slice = &samples[search_start..search_end];
    let mut powers = Vec::new();
    let mut offset = 0usize;
    while offset + frame_len <= search_slice.len() {
        powers.push(goertzel_power(
            &search_slice[offset..offset + frame_len],
            sample_rate,
            pitch_hz,
        ));
        offset += frame_step;
    }

    if powers.len() < 4 {
        return None;
    }

    let mut sorted_powers = powers.clone();
    sorted_powers.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let noise_floor = percentile_sorted(&sorted_powers, 0.35);
    let signal_floor = percentile_sorted(&sorted_powers, 0.85);
    if !noise_floor.is_finite() || !signal_floor.is_finite() || signal_floor <= noise_floor {
        return None;
    }

    let threshold = noise_floor + (signal_floor - noise_floor) * 0.30;
    let active: Vec<bool> = powers.iter().map(|&power| power >= threshold).collect();
    if active.iter().filter(|&&is_active| is_active).count() < 2 {
        return None;
    }

    Some(ActivityProfile {
        start_s: search_start_s,
        end_s: search_end_s,
        frame_len_s,
        frame_step_s,
        powers,
        threshold,
        active,
    })
}

fn build_display_profile(
    samples: &[f32],
    sample_rate: u32,
    pitch_hz: Option<f32>,
    search_start_s: f32,
    search_end_s: f32,
) -> Option<ActivityProfile> {
    if let Some(pitch_hz) = pitch_hz.filter(|pitch| *pitch > 0.0) {
        if let Some(profile) = build_permissive_profile(
            samples,
            sample_rate,
            search_start_s,
            search_end_s,
            |frame| goertzel_power(frame, sample_rate, pitch_hz),
        ) {
            return Some(profile);
        }
    }

    build_permissive_profile(
        samples,
        sample_rate,
        search_start_s,
        search_end_s,
        broadband_frame_power,
    )
}

fn build_permissive_profile<F>(
    samples: &[f32],
    sample_rate: u32,
    search_start_s: f32,
    search_end_s: f32,
    mut power_fn: F,
) -> Option<ActivityProfile>
where
    F: FnMut(&[f32]) -> f32,
{
    if search_end_s <= search_start_s {
        return None;
    }

    let frame_len_s = 0.025;
    let frame_step_s = 0.010;
    let frame_len = ((frame_len_s * sample_rate as f32).round() as usize).max(16);
    let frame_step = ((frame_step_s * sample_rate as f32).round() as usize).max(8);
    let search_start = ((search_start_s * sample_rate as f32).floor() as usize).min(samples.len());
    let search_end = ((search_end_s * sample_rate as f32).ceil() as usize).min(samples.len());
    if search_end <= search_start + frame_len {
        return None;
    }

    let search_slice = &samples[search_start..search_end];
    let mut powers = Vec::new();
    let mut offset = 0usize;
    while offset + frame_len <= search_slice.len() {
        powers.push(power_fn(&search_slice[offset..offset + frame_len]));
        offset += frame_step;
    }

    if powers.len() < 4 {
        return None;
    }

    let mut sorted_powers = powers.clone();
    sorted_powers.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let noise_floor = percentile_sorted(&sorted_powers, 0.35);
    let signal_floor = percentile_sorted(&sorted_powers, 0.85);
    let threshold = if !noise_floor.is_finite() {
        0.0
    } else if !signal_floor.is_finite() || signal_floor <= noise_floor {
        noise_floor
    } else {
        noise_floor + (signal_floor - noise_floor) * 0.30
    };
    let active: Vec<bool> = powers.iter().map(|&power| power >= threshold).collect();

    Some(ActivityProfile {
        start_s: search_start_s,
        end_s: search_end_s,
        frame_len_s,
        frame_step_s,
        powers,
        threshold,
        active,
    })
}

fn broadband_frame_power(frame: &[f32]) -> f32 {
    frame.iter().map(|sample| sample * sample).sum::<f32>() / frame.len() as f32
}

fn estimated_dot_s(wpm: Option<f32>) -> f32 {
    let wpm = wpm.unwrap_or(18.0);
    (1.2 / wpm.max(5.0)).clamp(0.03, 0.24)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameSpan {
    start: usize,
    end: usize,
}

fn best_pause_bounded_span(
    active: &[bool],
    candidate_start: usize,
    candidate_end: usize,
    min_pause_frames: usize,
) -> Option<FrameSpan> {
    let spans = merge_active_spans(active, min_pause_frames);
    if spans.is_empty() {
        return None;
    }

    let target = FrameSpan {
        start: candidate_start.min(active.len()),
        end: candidate_end.min(active.len()),
    };

    spans.into_iter().max_by(|left, right| {
        overlap_len(*left, target)
            .cmp(&overlap_len(*right, target))
            .then_with(|| {
                let left_distance = span_center_distance(*left, target);
                let right_distance = span_center_distance(*right, target);
                right_distance
                    .partial_cmp(&left_distance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| span_len(*left).cmp(&span_len(*right)))
    })
}

fn merge_active_spans(active: &[bool], min_pause_frames: usize) -> Vec<FrameSpan> {
    let mut raw_spans = Vec::new();
    let mut index = 0usize;
    while index < active.len() {
        while index < active.len() && !active[index] {
            index += 1;
        }
        if index >= active.len() {
            break;
        }

        let start = index;
        while index < active.len() && active[index] {
            index += 1;
        }
        raw_spans.push(FrameSpan { start, end: index });
    }

    if raw_spans.is_empty() {
        return raw_spans;
    }

    let mut merged: Vec<FrameSpan> = Vec::with_capacity(raw_spans.len());
    for span in raw_spans {
        if let Some(last) = merged.last_mut() {
            let gap = span.start.saturating_sub(last.end);
            if gap < min_pause_frames {
                last.end = span.end;
                continue;
            }
        }
        merged.push(span);
    }
    merged
}

fn adaptive_pause_frames(active: &[bool], dot_s: f32, frame_step_s: f32) -> usize {
    let base_pause_s = (dot_s * 7.0).clamp(0.35, 1.25);
    let base_pause_frames = ((base_pause_s / frame_step_s).ceil() as usize).max(2);
    let gap_lengths = interior_gap_lengths(active);
    if gap_lengths.is_empty() {
        return base_pause_frames;
    }

    let median_gap = median_usize(&gap_lengths);
    let deviations: Vec<usize> = gap_lengths
        .iter()
        .map(|gap| gap.abs_diff(median_gap))
        .collect();
    let mad = median_usize(&deviations);
    let robust_pause_frames = median_gap.saturating_add(mad.saturating_mul(2));
    let max_pause_s = (dot_s * 16.0).clamp(0.8, 2.5);
    let max_pause_frames = ((max_pause_s / frame_step_s).ceil() as usize).max(base_pause_frames);

    base_pause_frames
        .max(robust_pause_frames)
        .min(max_pause_frames)
        .max(2)
}

fn interior_gap_lengths(active: &[bool]) -> Vec<usize> {
    let mut gaps = Vec::new();
    let mut seen_active = false;
    let mut in_gap = false;
    let mut gap_len = 0usize;

    for &is_active in active {
        if is_active {
            if in_gap && seen_active {
                gaps.push(gap_len);
                gap_len = 0;
                in_gap = false;
            }
            seen_active = true;
            continue;
        }

        if seen_active {
            in_gap = true;
            gap_len += 1;
        }
    }

    gaps
}

fn median_usize(values: &[usize]) -> usize {
    if values.is_empty() {
        return 0;
    }

    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn overlap_len(left: FrameSpan, right: FrameSpan) -> usize {
    left.end
        .min(right.end)
        .saturating_sub(left.start.max(right.start))
}

fn span_len(span: FrameSpan) -> usize {
    span.end.saturating_sub(span.start)
}

fn span_center_distance(left: FrameSpan, right: FrameSpan) -> f32 {
    let left_center = (left.start + left.end) as f32 * 0.5;
    let right_center = (right.start + right.end) as f32 * 0.5;
    (left_center - right_center).abs()
}

fn goertzel_power(samples: &[f32], sample_rate: u32, target_hz: f32) -> f32 {
    let omega = (2.0 * std::f32::consts::PI * target_hz) / sample_rate as f32;
    let coeff = 2.0 * omega.cos();
    let mut q1 = 0.0_f32;
    let mut q2 = 0.0_f32;

    for &sample in samples {
        let q0 = coeff * q1 - q2 + sample;
        q2 = q1;
        q1 = q0;
    }

    q1 * q1 + q2 * q2 - coeff * q1 * q2
}

fn percentile_sorted(sorted: &[f32], q: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }

    let clamped_q = q.clamp(0.0, 1.0);
    let index = ((sorted.len() - 1) as f32 * clamped_q).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

fn candidates_belong_to_same_region(
    left: &WindowCandidate,
    right: &WindowCandidate,
    max_gap_s: f32,
) -> bool {
    let gap_ok = right.start_s <= left.end_s + max_gap_s.max(0.0);
    if !gap_ok {
        return false;
    }

    let pitch_ok = match (left.stream_pitch_hz, right.stream_pitch_hz) {
        (Some(a), Some(b)) => (a - b).abs() <= 30.0,
        _ => match (left.offline_pitch_hz, right.offline_pitch_hz) {
            (Some(a), Some(b)) => (a - b).abs() <= 30.0,
            _ => true,
        },
    };

    pitch_ok && textual_continuity(left, right)
}

fn textual_continuity(left: &WindowCandidate, right: &WindowCandidate) -> bool {
    let left_offline = compact_normalized(&left.offline_text);
    let left_stream = compact_normalized(&left.stream_text);
    let right_offline = compact_normalized(&right.offline_text);
    let right_stream = compact_normalized(&right.stream_text);

    let shared = [
        longest_common_substring_len(&left_offline, &right_offline),
        longest_common_substring_len(&left_stream, &right_stream),
        longest_common_substring_len(&left_offline, &right_stream),
        longest_common_substring_len(&left_stream, &right_offline),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);

    if shared >= 2 {
        return true;
    }

    left.matched_needles.iter().any(|needle| {
        right
            .matched_needles
            .iter()
            .any(|other| other.eq_ignore_ascii_case(needle))
    })
}

fn merge_candidate_into(region: &mut WindowCandidate, incoming: WindowCandidate) {
    region.start_s = region.start_s.min(incoming.start_s);
    region.end_s = region.end_s.max(incoming.end_s);
    region.member_count += incoming.member_count;
    region.shared_chars = region.shared_chars.max(incoming.shared_chars);
    let incoming_rank = candidate_rank(&incoming);
    let region_rank = candidate_rank(region);
    if incoming_rank > region_rank {
        region.strongest_copy_len = incoming.strongest_copy_len;
        region.offline_text = incoming.offline_text;
        region.offline_pitch_hz = incoming.offline_pitch_hz;
        region.offline_wpm = incoming.offline_wpm;
        region.stream_text = incoming.stream_text;
        region.stream_pitch_hz = incoming.stream_pitch_hz;
        region.stream_wpm = incoming.stream_wpm;
        region.stream_threshold = incoming.stream_threshold;
    } else {
        region.strongest_copy_len = region.strongest_copy_len.max(incoming.strongest_copy_len);
    }

    for needle in incoming.matched_needles {
        if !region
            .matched_needles
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&needle))
        {
            region.matched_needles.push(needle);
        }
    }
}

fn candidate_rank(candidate: &WindowCandidate) -> (usize, usize, usize) {
    (
        candidate.matched_needles.len(),
        candidate.shared_chars,
        candidate.strongest_copy_len,
    )
}

fn stream_decode_window(
    samples: &[f32],
    sample_rate: u32,
    cfg: DecoderConfig,
    chunk_ms: u32,
) -> Result<StreamSummary> {
    let mut decoder = StreamingDecoder::new(sample_rate)?;
    decoder.set_config(cfg);
    let chunk_samples = (((sample_rate as u64 * chunk_ms as u64) / 1000) as usize).max(64);

    let mut transcript = String::new();
    for chunk in samples.chunks(chunk_samples) {
        let events = decoder.feed(chunk)?;
        append_stream_events(&mut transcript, events);
    }
    append_stream_events(&mut transcript, decoder.flush());

    Ok(StreamSummary {
        transcript,
        pitch_hz: decoder.pitch(),
        wpm: decoder.current_wpm(),
        threshold: decoder.current_threshold(),
    })
}

fn append_stream_events(transcript: &mut String, events: Vec<StreamEvent>) {
    for ev in events {
        match ev {
            StreamEvent::Char { ch, .. } => transcript.push(ch),
            StreamEvent::Word => transcript.push(' '),
            StreamEvent::Garbled { .. } => transcript.push('?'),
            StreamEvent::PitchUpdate { .. }
            | StreamEvent::PitchLost { .. }
            | StreamEvent::WpmUpdate { .. }
            | StreamEvent::Power { .. } => {}
        }
    }
}

fn matched_needles(needles: &[String], offline_compact: &str, stream_compact: &str) -> Vec<String> {
    needles
        .iter()
        .filter(|needle| {
            offline_compact.contains(needle.as_str()) || stream_compact.contains(needle.as_str())
        })
        .cloned()
        .collect()
}

fn should_keep_without_needles(
    shared_chars: usize,
    strongest_copy_len: usize,
    min_shared_chars: usize,
    offline_pitch_hz: Option<f32>,
    stream_pitch_hz: Option<f32>,
    offline_wpm: Option<f32>,
    stream_wpm: Option<f32>,
) -> bool {
    if strongest_copy_len < min_shared_chars {
        return false;
    }

    if shared_chars >= min_shared_chars {
        return true;
    }

    let pitch_aligned = match (offline_pitch_hz, stream_pitch_hz) {
        (Some(a), Some(b)) => (a - b).abs() <= 25.0,
        _ => false,
    };
    if !pitch_aligned {
        return false;
    }

    offline_wpm.is_some() || stream_wpm.is_some()
}

pub fn compact_normalized(text: &str) -> String {
    text.chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_uppercase())
            } else {
                None
            }
        })
        .collect()
}

fn longest_common_substring_len(a: &str, b: &str) -> usize {
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let mut prev = vec![0usize; b_bytes.len() + 1];
    let mut best = 0usize;

    for &a_byte in a_bytes {
        let mut curr = vec![0usize; b_bytes.len() + 1];
        for (j, &b_byte) in b_bytes.iter().enumerate() {
            if a_byte == b_byte {
                curr[j + 1] = prev[j] + 1;
                best = best.max(curr[j + 1]);
            }
        }
        prev = curr;
    }

    best
}

#[cfg(test)]
mod tests {
    use super::{
        adaptive_pause_frames, best_pause_bounded_span, build_signal_profile, compact_normalized,
        longest_common_substring_len, matched_needles, merge_overlapping_candidates,
        pause_bounded_span, should_keep_without_needles, tail_extension_budget_s, FrameSpan,
        WindowCandidate,
    };

    #[test]
    fn compact_normalized_keeps_only_ascii_alnum_uppercase() {
        assert_eq!(compact_normalized("K5zd 5nn/tu?"), "K5ZD5NNTU");
    }

    #[test]
    fn longest_common_substring_len_finds_shared_anchor() {
        assert_eq!(longest_common_substring_len("K5ZD5NN38", "ZZK5ZD5NNYY"), 7);
    }

    #[test]
    fn matched_needles_checks_both_paths() {
        let needles = vec!["5NN".to_string(), "TU".to_string(), "K5ZD".to_string()];
        let hits = matched_needles(&needles, "K5ZD5NN38", "TU");
        assert_eq!(hits, needles);
    }

    #[test]
    fn blank_needles_accepts_pitch_aligned_window_with_copy() {
        assert!(should_keep_without_needles(
            1,
            4,
            4,
            Some(594.7),
            Some(591.8),
            Some(7.0),
            Some(17.8),
        ));
    }

    #[test]
    fn blank_needles_rejects_weak_or_unlocked_window() {
        assert!(!should_keep_without_needles(
            1,
            3,
            4,
            Some(594.7),
            Some(591.8),
            Some(7.0),
            Some(17.8),
        ));
        assert!(!should_keep_without_needles(
            1,
            6,
            4,
            None,
            Some(591.8),
            Some(7.0),
            Some(17.8),
        ));
    }

    #[test]
    fn overlapping_candidates_merge_into_region() {
        let merged = merge_overlapping_candidates(
            vec![
                WindowCandidate {
                    start_s: 7.0,
                    end_s: 11.0,
                    is_fallback: false,
                    member_count: 1,
                    shared_chars: 3,
                    strongest_copy_len: 5,
                    matched_needles: vec!["QST".into()],
                    offline_text: "?ST QST".into(),
                    offline_pitch_hz: Some(591.8),
                    offline_wpm: Some(7.0),
                    stream_text: "QST".into(),
                    stream_pitch_hz: Some(591.8),
                    stream_wpm: Some(17.1),
                    stream_threshold: 1.0,
                },
                WindowCandidate {
                    start_s: 10.0,
                    end_s: 14.0,
                    is_fallback: false,
                    member_count: 1,
                    shared_chars: 2,
                    strongest_copy_len: 7,
                    matched_needles: vec!["QST".into()],
                    offline_text: "EST QST N".into(),
                    offline_pitch_hz: Some(591.8),
                    offline_wpm: Some(5.0),
                    stream_text: "ST T".into(),
                    stream_pitch_hz: Some(591.8),
                    stream_wpm: Some(18.5),
                    stream_threshold: 1.0,
                },
            ],
            1.0,
        );

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].start_s, 7.0);
        assert_eq!(merged[0].end_s, 14.0);
        assert_eq!(merged[0].member_count, 2);
    }

    #[test]
    fn pause_bounded_span_prefers_long_gap_boundaries() {
        let active = vec![
            false, false, false, true, true, true, false, false, true, true, true, true, false,
            false, false,
        ];

        let span = best_pause_bounded_span(&active, 4, 11, 3);

        assert_eq!(span, Some(FrameSpan { start: 3, end: 12 }));
    }

    #[test]
    fn pause_bounded_span_splits_on_long_pause() {
        let active = vec![
            false, true, true, false, false, false, true, true, true, false, false,
        ];

        let span = best_pause_bounded_span(&active, 6, 8, 3);

        assert_eq!(span, Some(FrameSpan { start: 6, end: 9 }));
    }

    #[test]
    fn tail_extension_budget_scales_with_window_and_caps() {
        let candidate = WindowCandidate {
            start_s: 10.0,
            end_s: 14.0,
            is_fallback: false,
            member_count: 1,
            shared_chars: 0,
            strongest_copy_len: 0,
            matched_needles: Vec::new(),
            offline_text: String::new(),
            offline_pitch_hz: None,
            offline_wpm: None,
            stream_text: String::new(),
            stream_pitch_hz: Some(600.0),
            stream_wpm: Some(20.0),
            stream_threshold: 0.0,
        };
        assert_eq!(tail_extension_budget_s(&candidate), 3.0);

        let short_candidate = WindowCandidate {
            end_s: 11.0,
            ..candidate
        };
        assert_eq!(tail_extension_budget_s(&short_candidate), 0.75);
    }

    #[test]
    fn adaptive_pause_frames_uses_word_gap_floor() {
        let active = vec![
            true, true, false, true, false, false, true, true, false, true, false, true,
        ];

        let pause_frames = adaptive_pause_frames(&active, 0.08, 0.01);

        assert_eq!(pause_frames, 56);
    }

    #[test]
    fn pause_bounded_span_can_extend_tail_beyond_base_margin() {
        let sample_rate = 8_000u32;
        let total_secs = 3.2f32;
        let total_samples = (total_secs * sample_rate as f32) as usize;
        let mut samples = vec![0.0f32; total_samples];
        let tone_hz = 600.0f32;
        let tone_start_s = 0.4f32;
        let tone_end_s = 2.35f32;
        for index in 0..total_samples {
            let t = index as f32 / sample_rate as f32;
            if (tone_start_s..tone_end_s).contains(&t) {
                samples[index] = (2.0 * std::f32::consts::PI * tone_hz * t).sin() * 0.8;
            }
        }

        let candidate = WindowCandidate {
            start_s: 0.75,
            end_s: 1.75,
            is_fallback: false,
            member_count: 1,
            shared_chars: 0,
            strongest_copy_len: 0,
            matched_needles: Vec::new(),
            offline_text: String::new(),
            offline_pitch_hz: Some(tone_hz),
            offline_wpm: Some(20.0),
            stream_text: String::new(),
            stream_pitch_hz: Some(tone_hz),
            stream_wpm: Some(20.0),
            stream_threshold: 0.0,
        };

        let (start_s, end_s) =
            pause_bounded_span(&candidate, &samples, sample_rate, 0.5).expect("bounded span");

        assert!(start_s <= candidate.start_s);
        assert!(end_s > 2.1, "expected tail extension, got {end_s}");
        assert!(end_s < 2.7, "expected stop at first pause, got {end_s}");
    }

    #[test]
    fn build_signal_profile_returns_context_and_suggestion() {
        let sample_rate = 8_000u32;
        let total_secs = 4.0f32;
        let total_samples = (total_secs * sample_rate as f32) as usize;
        let mut samples = vec![0.0f32; total_samples];
        let tone_hz = 620.0f32;
        for index in 0..total_samples {
            let t = index as f32 / sample_rate as f32;
            if (0.8..2.6).contains(&t) {
                samples[index] = (2.0 * std::f32::consts::PI * tone_hz * t).sin() * 0.7;
            }
        }

        let profile =
            build_signal_profile(&samples, sample_rate, 1.1, 1.8, Some(tone_hz), Some(20.0))
            .expect("profile");

        assert!(profile.display_start_s < 1.1);
        assert!(profile.display_end_s > 1.8);
        assert!(!profile.points.is_empty());
        assert!(profile.suggested_end_s > 2.2);
    }

    #[test]
    fn build_signal_profile_falls_back_to_broadband_when_pitch_is_missing() {
        let sample_rate = 8_000u32;
        let total_secs = 3.0f32;
        let total_samples = (total_secs * sample_rate as f32) as usize;
        let mut samples = vec![0.0f32; total_samples];
        for index in (sample_rate / 2) as usize..(sample_rate as usize + sample_rate as usize / 2) {
            samples[index] = 0.35;
        }

        let profile = build_signal_profile(&samples, sample_rate, 0.4, 2.2, None, None)
            .expect("broadband profile");

        assert!(profile.pitch_hz.abs() < f32::EPSILON);
        assert!(profile.display_end_s > profile.display_start_s);
        assert!(!profile.points.is_empty());
    }
}

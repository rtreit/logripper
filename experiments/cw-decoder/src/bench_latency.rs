//! Cold-start acquisition latency benchmark harness.
//!
//! Measures, in *audio sample-time* (deterministic, not wall clock), how
//! long it takes the streaming decoder to go from "no signal" to
//! "emitting trustworthy correct CW characters" given a known onset
//! and a known truth transcript.
//!
//! Two scenario sources are supported:
//!
//! * **Synthetic** — build an audio buffer of `[lead_in][synth PARIS]`
//!   where the lead-in is silence, white noise, pink-ish noise, or a
//!   coarse "voice" simulation. Reproducible, no labels needed.
//! * **Real** — load a recording, the operator supplies `--cw-onset-ms`
//!   and `--truth`, and we measure the same metrics.
//!
//! The headline metric is `t_stable_n_correct_ms`: the first time the
//! running transcript contains a contiguous run of `n` characters that
//! is also a contiguous substring of the truth (case-insensitive).

use std::f32::consts::TAU;

use anyhow::Result;

use crate::audio::DecodedAudio;
use crate::streaming::{ConfidenceState, DecoderConfig, StreamEvent, StreamingDecoder};

/// Default value for the stable-N-correct headline metric.
pub const DEFAULT_STABLE_N: usize = 5;

/// Single benchmark scenario.
pub struct Scenario {
    pub name: String,
    pub audio: DecodedAudio,
    /// Where in `audio` the CW signal actually starts (ms from the
    /// beginning of the buffer).
    pub cw_onset_ms: u32,
    /// Expected uppercase transcript starting at `cw_onset_ms`.
    pub truth: String,
}

/// All metrics for a single scenario run, in audio sample-time (ms from
/// the start of the audio buffer fed to the decoder). `None` for
/// metrics whose triggering event never fired during the run.
#[derive(Debug, Clone, Default)]
pub struct BenchResult {
    pub scenario: String,
    pub config_label: String,
    pub cw_onset_ms: u32,
    pub truth: String,
    pub transcript: String,
    pub final_pitch_hz: Option<f32>,
    pub t_first_pitch_update_ms: Option<u32>,
    pub t_first_pitch_lost_ms: Option<u32>,
    pub t_first_probation_ms: Option<u32>,
    pub t_first_locked_ms: Option<u32>,
    pub t_first_char_ms: Option<u32>,
    pub t_first_correct_char_ms: Option<u32>,
    pub t_stable_n_correct_ms: Option<u32>,
    pub stable_n: usize,
    pub locked_pitch_hz: Option<f32>,
    /// Number of false characters emitted before the stable-N point. Useful
    /// as a "garbage before lock" proxy.
    pub false_chars_before_stable: usize,

    // --- Lock-stability metrics (post-first-lock, while CW is present) --
    /// Number of `PitchLost` events that fired after the first
    /// `Confidence::Locked`. Ideally 0 for clean CW.
    pub n_pitch_lost_after_lock: usize,
    /// Number of complete "Locked → !Locked → Locked" cycles after the
    /// first lock. Each one is a stretch of audio the operator missed
    /// despite the signal being present.
    pub n_relock_cycles: usize,
    /// Total milliseconds spent in any non-Locked confidence state
    /// AFTER the first lock and BEFORE the end of CW (so trailing
    /// silence after CW does not penalise stability).
    pub total_unlocked_ms_after_lock: u32,
    /// Longest single non-Locked stretch (ms) after first lock during
    /// the CW segment.
    pub longest_unlocked_gap_ms: u32,
    /// Locked uptime ratio across the CW segment after first lock, in
    /// `[0.0, 1.0]`. `None` if first-lock never happened or if the CW
    /// segment after first-lock is empty. 1.0 = perfectly sticky lock.
    pub lock_uptime_ratio: Option<f32>,
}

impl BenchResult {
    /// Acquisition latency = `t_stable_n_correct - cw_onset`. `None` if
    /// stable-N was never reached.
    pub fn acquisition_latency_ms(&self) -> Option<i64> {
        self.t_stable_n_correct_ms
            .map(|t| t as i64 - self.cw_onset_ms as i64)
    }
}

/// Run a single scenario through a fresh `StreamingDecoder` configured
/// with `cfg`. Returns the metrics. Audio is fed in `chunk_ms`-sized
/// chunks at the scenario's native sample rate.
pub fn run_scenario(
    scenario: &Scenario,
    cfg: DecoderConfig,
    chunk_ms: u32,
    stable_n: usize,
    config_label: &str,
) -> Result<BenchResult> {
    let sr = scenario.audio.sample_rate;
    let chunk_samples = ((sr as u64 * chunk_ms as u64) / 1000).max(1) as usize;
    let truth_upper = scenario.truth.to_uppercase();

    let mut dec = StreamingDecoder::new(sr)?;
    dec.set_config(cfg);

    let mut result = BenchResult {
        scenario: scenario.name.clone(),
        config_label: config_label.to_string(),
        cw_onset_ms: scenario.cw_onset_ms,
        truth: truth_upper.clone(),
        stable_n,
        ..Default::default()
    };

    let mut transcript = String::new();
    // For each char in transcript, time it was emitted (audio-ms).
    let mut char_times: Vec<u32> = Vec::new();

    // Lock-stability state: track time spent in each confidence phase
    // after first lock, bounded by the CW segment.
    let total_samples = scenario.audio.samples.len();
    let total_audio_ms = ((total_samples as u64 * 1000) / sr as u64) as u32;
    // CW segment ends at end of audio (the synth/real loaders place CW
    // from cw_onset_ms to the end). If we ever introduce trailing
    // silence we'd parametrise this; for now use end-of-audio.
    let cw_end_ms = total_audio_ms;
    let mut stab = StabilityTracker::new(cw_end_ms);

    let total = scenario.audio.samples.len();
    let mut consumed: usize = 0;
    while consumed < total {
        let end = (consumed + chunk_samples).min(total);
        let events = dec.feed(&scenario.audio.samples[consumed..end])?;
        // Sample-time at the END of this chunk, in ms.
        let t_ms = ((end as u64 * 1000) / sr as u64) as u32;
        process_events(
            &events,
            t_ms,
            &mut transcript,
            &mut char_times,
            &mut result,
            &mut stab,
        );
        consumed = end;
    }
    let flushed = dec.flush();
    let t_end = ((total as u64 * 1000) / sr as u64) as u32;
    process_events(
        &flushed,
        t_end,
        &mut transcript,
        &mut char_times,
        &mut result,
        &mut stab,
    );
    stab.finalize(t_end, &mut result);

    result.transcript = transcript;
    result.final_pitch_hz = dec.pitch();
    let transcript_owned = result.transcript.clone();
    update_truth_metrics(
        &transcript_owned,
        &truth_upper,
        &char_times,
        stable_n,
        &mut result,
    );
    Ok(result)
}

/// Tracks lock-uptime accounting as the run progresses. The decoder's
/// state is queried at chunk boundaries (since we only see events at
/// chunk-end), and every transition closes the previous interval and
/// opens a new one.
struct StabilityTracker {
    /// End of the CW segment in audio-ms. Intervals are clipped here so
    /// trailing silence doesn't penalise the lock.
    cw_end_ms: u32,
    /// Current confidence state.
    state: ConfidenceState,
    /// When (audio-ms) the current state began.
    state_started_ms: u32,
    /// Audio-ms at which the first Locked event fired. Time before
    /// this is excluded from uptime accounting (we are not yet past
    /// the cold-start phase).
    first_locked_ms: Option<u32>,
    /// Accumulated locked time after first lock.
    locked_ms: u32,
    /// Accumulated non-locked time after first lock.
    unlocked_ms: u32,
    /// Longest contiguous non-locked stretch after first lock.
    longest_unlocked: u32,
}

impl StabilityTracker {
    fn new(cw_end_ms: u32) -> Self {
        Self {
            cw_end_ms,
            state: ConfidenceState::Hunting,
            state_started_ms: 0,
            first_locked_ms: None,
            locked_ms: 0,
            unlocked_ms: 0,
            longest_unlocked: 0,
        }
    }

    /// Account for time spent in the previous state up to `now_ms`,
    /// then move to `new_state` (if different).
    fn transition_to(&mut self, new_state: ConfidenceState, now_ms: u32) {
        if self.state == new_state {
            return;
        }
        // Close the previous interval.
        if let Some(first_lock) = self.first_locked_ms {
            let interval_start = self.state_started_ms.max(first_lock);
            let interval_end = now_ms.min(self.cw_end_ms);
            if interval_end > interval_start {
                let dur = interval_end - interval_start;
                if matches!(self.state, ConfidenceState::Locked) {
                    self.locked_ms += dur;
                } else {
                    self.unlocked_ms += dur;
                    self.longest_unlocked = self.longest_unlocked.max(dur);
                }
            }
        }
        self.state = new_state;
        self.state_started_ms = now_ms;
        if matches!(new_state, ConfidenceState::Locked) && self.first_locked_ms.is_none() {
            self.first_locked_ms = Some(now_ms);
        }
    }

    fn finalize(&mut self, end_ms: u32, out: &mut BenchResult) {
        // Close the final open interval at end_ms.
        self.transition_to(ConfidenceState::Hunting, end_ms);
        // Reverse the dummy hunting transition we just did so accounting
        // is correct: it already credited the trailing interval to its
        // real owner inside transition_to.
        out.total_unlocked_ms_after_lock = self.unlocked_ms;
        out.longest_unlocked_gap_ms = self.longest_unlocked;
        if let Some(first_lock) = self.first_locked_ms {
            let cw_after_first_lock = self.cw_end_ms.saturating_sub(first_lock);
            if cw_after_first_lock > 0 {
                let total = (self.locked_ms + self.unlocked_ms).max(1);
                out.lock_uptime_ratio = Some(self.locked_ms as f32 / total as f32);
            }
        }
    }
}

fn process_events(
    events: &[StreamEvent],
    t_ms: u32,
    transcript: &mut String,
    char_times: &mut Vec<u32>,
    out: &mut BenchResult,
    stab: &mut StabilityTracker,
) {
    for ev in events {
        match ev {
            StreamEvent::PitchUpdate { pitch_hz } => {
                if out.t_first_pitch_update_ms.is_none() {
                    out.t_first_pitch_update_ms = Some(t_ms);
                }
                out.locked_pitch_hz = Some(*pitch_hz);
            }
            StreamEvent::PitchLost { .. } => {
                if out.t_first_pitch_lost_ms.is_none() {
                    out.t_first_pitch_lost_ms = Some(t_ms);
                }
                if out.t_first_locked_ms.is_some() {
                    out.n_pitch_lost_after_lock += 1;
                }
            }
            StreamEvent::Confidence { state } => {
                match state {
                    ConfidenceState::Probation => {
                        if out.t_first_probation_ms.is_none() {
                            out.t_first_probation_ms = Some(t_ms);
                        }
                    }
                    ConfidenceState::Locked => {
                        if out.t_first_locked_ms.is_none() {
                            out.t_first_locked_ms = Some(t_ms);
                        } else if !matches!(stab.state, ConfidenceState::Locked) {
                            // Re-acquired after a previous drop.
                            out.n_relock_cycles += 1;
                        }
                    }
                    ConfidenceState::Hunting => {}
                }
                stab.transition_to(*state, t_ms);
            }
            StreamEvent::Char { ch, .. } => {
                if out.t_first_char_ms.is_none() {
                    out.t_first_char_ms = Some(t_ms);
                }
                transcript.push(*ch);
                char_times.push(t_ms);
            }
            StreamEvent::Word => {
                transcript.push(' ');
                char_times.push(t_ms);
            }
            _ => {}
        }
    }
}

/// After the run, scan the transcript for the first contiguous N-char
/// substring that is also a substring of the truth. Records both the
/// first-correct-char and stable-N latencies, plus the false-char count.
fn update_truth_metrics(
    transcript: &str,
    truth: &str,
    char_times: &[u32],
    stable_n: usize,
    out: &mut BenchResult,
) {
    let t = transcript.to_uppercase();
    let chars: Vec<char> = t.chars().collect();
    // First "correct char" = any character in transcript that exists in
    // the truth (a generous sanity check; the headline metric is
    // stable-N).
    for (i, c) in chars.iter().enumerate() {
        if !c.is_whitespace() && truth.contains(*c) {
            out.t_first_correct_char_ms = Some(char_times[i]);
            break;
        }
    }
    if chars.len() >= stable_n {
        for end in stable_n..=chars.len() {
            let start = end - stable_n;
            let sub: String = chars[start..end].iter().collect();
            if !sub.trim().is_empty() && truth.contains(&sub) {
                let stable_idx = end - 1;
                out.t_stable_n_correct_ms = Some(char_times[stable_idx]);
                // Count "garbage" chars emitted before the stable run started.
                out.false_chars_before_stable = chars[..start]
                    .iter()
                    .filter(|c| !c.is_whitespace())
                    .count();
                break;
            }
        }
    }
}

// --- Synthetic generators -----------------------------------------------

/// Synthesised PARIS-style CW at the given pitch and WPM, repeating
/// PARIS until `secs` of audio have been produced. Uses a 5 ms cosine
/// edge ramp to avoid key-click broadband transients (the decoder's
/// purity gate would suppress those anyway).
pub fn synth_paris(sample_rate: u32, pitch_hz: f32, wpm: f32, secs: f32) -> Vec<f32> {
    let dot_secs = 1.2 / wpm;
    let dot_n = (dot_secs * sample_rate as f32) as usize;
    let ramp_n = ((sample_rate as f32) * 0.005) as usize;
    let pattern: Vec<(bool, usize)> = {
        let mut p: Vec<(bool, usize)> = Vec::new();
        let codes = [".--.", ".-", ".-.", "..", "..."];
        for (li, code) in codes.iter().enumerate() {
            let mut first = true;
            for c in code.chars() {
                if !first {
                    p.push((false, 1));
                }
                first = false;
                let on_units = if c == '.' { 1 } else { 3 };
                p.push((true, on_units));
            }
            if li + 1 < codes.len() {
                p.push((false, 3));
            }
        }
        p.push((false, 7));
        p
    };
    let total_n = (secs * sample_rate as f32) as usize;
    let mut out: Vec<f32> = Vec::with_capacity(total_n);
    let mut t = 0usize;
    'outer: loop {
        for (on, units) in &pattern {
            let n = dot_n * units;
            for k in 0..n {
                let env = if *on {
                    let rise = if k < ramp_n {
                        0.5 * (1.0 - ((std::f32::consts::PI * k as f32) / ramp_n as f32).cos())
                    } else {
                        1.0
                    };
                    let fall = if k + ramp_n > n {
                        let kk = (n - k) as f32;
                        0.5 * (1.0 - ((std::f32::consts::PI * kk) / ramp_n as f32).cos())
                    } else {
                        1.0
                    };
                    rise.min(fall)
                } else {
                    0.0
                };
                let s = (TAU * pitch_hz * (t as f32) / sample_rate as f32).sin() * 0.5 * env;
                out.push(s);
                t += 1;
                if out.len() >= total_n {
                    break 'outer;
                }
            }
        }
    }
    out
}

/// Repeating PARIS produces "PARIS PARIS PARIS ..." — return that as
/// truth so the benchmark's correctness check has something to align
/// against. Matches `synth_paris` symbol order.
pub fn paris_truth(secs: f32, wpm: f32) -> String {
    // Each PARIS cycle = 50 dot units = 60/wpm seconds.
    let cycle_secs = 60.0 / wpm;
    let cycles = (secs / cycle_secs).ceil() as usize + 2;
    let mut s = String::new();
    for i in 0..cycles {
        if i > 0 {
            s.push(' ');
        }
        s.push_str("PARIS");
    }
    s
}

fn lcg_white_noise(n: usize, amp: f32, seed: u32) -> Vec<f32> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            ((s >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 2.0 * amp
        })
        .collect()
}

/// Coarse "voice"-shaped background: three formant-ish sines at 500 /
/// 1500 / 2500 Hz amplitude-modulated by a slow envelope and a faster
/// "syllable" envelope, plus low-level noise. Designed to plausibly
/// trigger an out-of-band lock in a decoder that doesn't gate on
/// keying-aware Fisher.
fn synth_voice(n: usize, sample_rate: u32, amp: f32, seed: u32) -> Vec<f32> {
    let noise = lcg_white_noise(n, amp * 0.2, seed);
    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            let syllable = 0.5 + 0.5 * (TAU * 4.0 * t).sin();
            let breath = 0.6 + 0.4 * (TAU * 0.5 * t).sin();
            let f1 = (TAU * 500.0 * t).sin();
            let f2 = (TAU * 1500.0 * t).sin() * 0.6;
            let f3 = (TAU * 2500.0 * t).sin() * 0.3;
            (f1 + f2 + f3) * amp * syllable * breath + noise[i]
        })
        .collect()
}

/// Build a synthetic scenario: `lead_in_secs` of background, then
/// `cw_secs` of synthesised PARIS. Background is selected by `lead_in`.
pub fn make_synth_scenario(
    name: &str,
    sample_rate: u32,
    lead_in: LeadIn,
    lead_in_secs: f32,
    cw_secs: f32,
    cw_pitch_hz: f32,
    wpm: f32,
) -> Scenario {
    let lead_n = (lead_in_secs * sample_rate as f32) as usize;
    let mut samples: Vec<f32> = match lead_in {
        LeadIn::Silence => vec![0.0; lead_n],
        LeadIn::WhiteNoise { amp, seed } => lcg_white_noise(lead_n, amp, seed),
        LeadIn::Voice { amp, seed } => synth_voice(lead_n, sample_rate, amp, seed),
    };
    let cw = synth_paris(sample_rate, cw_pitch_hz, wpm, cw_secs);
    samples.extend(cw);
    let truth = paris_truth(cw_secs, wpm);
    let cw_onset_ms = (lead_in_secs * 1000.0) as u32;
    Scenario {
        name: name.to_string(),
        audio: DecodedAudio {
            samples,
            sample_rate,
        },
        cw_onset_ms,
        truth,
    }
}

/// Lead-in audio shape for `make_synth_scenario`.
#[derive(Debug, Clone, Copy)]
pub enum LeadIn {
    Silence,
    WhiteNoise { amp: f32, seed: u32 },
    Voice { amp: f32, seed: u32 },
}

/// The default scenario matrix used by `cw-decoder bench-latency` when
/// no explicit `--from-file` is provided. Designed to exercise
/// acquisition behavior across realistic operator scenarios:
///
/// * **silence_then_cw** — best case; how fast can we lock cold?
/// * **noise_then_cw** — band hiss before a station appears
/// * **voice_then_cw** — the YouTube-clip case: voice intro followed
///   by CW. The decoder must not lock on the voice formants.
/// * **strong_voice_then_cw** — same but louder voice; stress test for
///   the confidence machine.
/// * **long_clean_cw** — 30 s of clean CW, no lead-in. Lock-stability
///   stress: any `drops` / `relock` / non-100% uptime is a bug because
///   the audio never deviates from clean keying.
pub fn default_scenarios(sample_rate: u32) -> Vec<Scenario> {
    let cw_pitch = 700.0;
    let wpm = 20.0;
    let cw_secs = 12.0;
    vec![
        make_synth_scenario(
            "silence_then_cw",
            sample_rate,
            LeadIn::Silence,
            3.0,
            cw_secs,
            cw_pitch,
            wpm,
        ),
        make_synth_scenario(
            "noise_then_cw",
            sample_rate,
            LeadIn::WhiteNoise {
                amp: 0.05,
                seed: 0xDEAD_BEEF,
            },
            3.0,
            cw_secs,
            cw_pitch,
            wpm,
        ),
        make_synth_scenario(
            "voice_then_cw",
            sample_rate,
            LeadIn::Voice {
                amp: 0.15,
                seed: 0xCAFE_F00D,
            },
            5.0,
            cw_secs,
            cw_pitch,
            wpm,
        ),
        make_synth_scenario(
            "strong_voice_then_cw",
            sample_rate,
            LeadIn::Voice {
                amp: 0.35,
                seed: 0xFEED_FACE,
            },
            5.0,
            cw_secs,
            cw_pitch,
            wpm,
        ),
        make_synth_scenario(
            "long_clean_cw",
            sample_rate,
            LeadIn::Silence,
            1.0,
            30.0,
            cw_pitch,
            wpm,
        ),
    ]
}

/// Pretty-print a result table for human consumption.
pub fn print_results_table(results: &[BenchResult]) {
    println!();
    println!(
        "{:<24} {:>9} {:>9} {:>10} {:>7} {:>5} {:>6} {:>9} {:>9}",
        "scenario",
        "stableN_ms",
        "lat_ms",
        "uptime",
        "drops",
        "false",
        "relock",
        "longest",
        "pitch_hz",
    );
    println!("{}", "-".repeat(110));
    for r in results {
        let fmt = |v: Option<u32>| v.map(|x| format!("{x}")).unwrap_or_else(|| "-".into());
        let lat = r
            .acquisition_latency_ms()
            .map(|x| format!("{x:+}"))
            .unwrap_or_else(|| "-".into());
        let uptime = r
            .lock_uptime_ratio
            .map(|u| format!("{:.1}%", u * 100.0))
            .unwrap_or_else(|| "-".into());
        let pitch = r
            .locked_pitch_hz
            .map(|p| format!("{p:.1}"))
            .unwrap_or_else(|| "-".into());
        let longest = if r.longest_unlocked_gap_ms > 0 {
            format!("{} ms", r.longest_unlocked_gap_ms)
        } else {
            "-".into()
        };
        println!(
            "{:<24} {:>9} {:>9} {:>10} {:>7} {:>5} {:>6} {:>9} {:>9}",
            truncate(&r.scenario, 24),
            fmt(r.t_stable_n_correct_ms),
            lat,
            uptime,
            r.n_pitch_lost_after_lock,
            r.false_chars_before_stable,
            r.n_relock_cycles,
            longest,
            pitch,
        );
    }
    println!();
    println!("Legend:");
    println!("  stableN_ms = first time the next N decoded chars match a substring of truth");
    println!("  lat_ms     = stableN_ms - cw_onset_ms (lower = faster cold-start acquisition)");
    println!("  uptime     = % of post-first-lock CW spent in Locked confidence state");
    println!("  drops      = PitchLost events fired AFTER first lock (ideally 0 on clean CW)");
    println!("  false      = chars emitted before the stable-N point (ghost copy)");
    println!("  relock     = number of Hunting->Locked cycles after the first lock");
    println!("  longest    = longest single non-Locked stretch after first lock");
    println!();
    println!("Transcripts:");
    for r in results {
        let preview = r.transcript.chars().take(80).collect::<String>();
        println!("  {:<24}  truth={:?}", r.scenario, truncate(&r.truth, 50));
        println!("  {:<24}  trans={preview:?}", "");
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// Aggregate stats across a results set. Useful as an A/B summary when
/// re-running with a different `DecoderConfig`.
pub struct Aggregate {
    pub n: usize,
    pub stable_hits: usize,
    pub mean_latency_ms: Option<f64>,
    pub median_latency_ms: Option<i64>,
    pub worst_latency_ms: Option<i64>,
    pub total_false_chars: usize,
    /// Total `PitchLost` events across all scenarios after first lock.
    /// Lower is better; on the synthetic suite this should be near 0.
    pub total_pitch_drops: usize,
    /// Total relock cycles across scenarios.
    pub total_relock_cycles: usize,
    /// Mean lock-uptime ratio across scenarios that ever locked.
    pub mean_uptime_ratio: Option<f32>,
    /// Worst (lowest) lock-uptime ratio across scenarios that ever locked.
    pub worst_uptime_ratio: Option<f32>,
}

pub fn aggregate(results: &[BenchResult]) -> Aggregate {
    let lats: Vec<i64> = results
        .iter()
        .filter_map(|r| r.acquisition_latency_ms())
        .collect();
    let mean = if lats.is_empty() {
        None
    } else {
        Some(lats.iter().sum::<i64>() as f64 / lats.len() as f64)
    };
    let mut sorted = lats.clone();
    sorted.sort_unstable();
    let median = sorted.get(sorted.len() / 2).copied();
    let worst = sorted.last().copied();
    let uptimes: Vec<f32> = results.iter().filter_map(|r| r.lock_uptime_ratio).collect();
    let mean_uptime = if uptimes.is_empty() {
        None
    } else {
        Some(uptimes.iter().sum::<f32>() / uptimes.len() as f32)
    };
    let worst_uptime = uptimes
        .iter()
        .cloned()
        .fold(None, |acc: Option<f32>, x| Some(acc.map_or(x, |a| a.min(x))));
    Aggregate {
        n: results.len(),
        stable_hits: lats.len(),
        mean_latency_ms: mean,
        median_latency_ms: median,
        worst_latency_ms: worst,
        total_false_chars: results.iter().map(|r| r.false_chars_before_stable).sum(),
        total_pitch_drops: results.iter().map(|r| r.n_pitch_lost_after_lock).sum(),
        total_relock_cycles: results.iter().map(|r| r.n_relock_cycles).sum(),
        mean_uptime_ratio: mean_uptime,
        worst_uptime_ratio: worst_uptime,
    }
}

pub fn print_aggregate(label: &str, agg: &Aggregate) {
    let mean = agg
        .mean_latency_ms
        .map(|m| format!("{m:>6.0}"))
        .unwrap_or_else(|| "     -".into());
    let median = agg
        .median_latency_ms
        .map(|m| format!("{m:>6}"))
        .unwrap_or_else(|| "     -".into());
    let worst = agg
        .worst_latency_ms
        .map(|m| format!("{m:>6}"))
        .unwrap_or_else(|| "     -".into());
    let mean_up = agg
        .mean_uptime_ratio
        .map(|u| format!("{:>5.1}%", u * 100.0))
        .unwrap_or_else(|| "    -".into());
    let worst_up = agg
        .worst_uptime_ratio
        .map(|u| format!("{:>5.1}%", u * 100.0))
        .unwrap_or_else(|| "    -".into());
    println!(
        "[agg] {label}: stable_hits={}/{}  lat(ms) mean={mean} median={median} worst={worst}  uptime mean={mean_up} worst={worst_up}  drops={}  relocks={}  ghost_chars={}",
        agg.stable_hits,
        agg.n,
        agg.total_pitch_drops,
        agg.total_relock_cycles,
        agg.total_false_chars,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paris_truth_repeats_paris() {
        let s = paris_truth(2.0, 20.0);
        // 2s @ 20 wpm = 0.667 cycles -> >= 1 PARIS
        assert!(s.contains("PARIS"));
    }

    #[test]
    fn synth_silence_then_cw_acquires() {
        let scen = make_synth_scenario(
            "silence_then_cw_test",
            16000,
            LeadIn::Silence,
            2.0,
            8.0,
            700.0,
            20.0,
        );
        let r =
            run_scenario(&scen, DecoderConfig::defaults(), 100, DEFAULT_STABLE_N, "default").unwrap();
        assert!(
            r.t_first_pitch_update_ms.is_some(),
            "expected pitch lock on synth CW"
        );
        // Stable-5 may or may not hit on every config; do not over-assert
        // here. The rest is a smoke test.
        assert!(!r.transcript.is_empty(), "expected some transcript");
    }

    #[test]
    fn voice_lead_in_does_not_emit_chars_before_cw() {
        // Voice section is 5s; CW starts at 5s. Even if the decoder
        // attempts a bogus voice lock, the confidence machine should
        // suppress emitted Char events.
        let scen = make_synth_scenario(
            "voice_then_cw_test",
            16000,
            LeadIn::Voice {
                amp: 0.2,
                seed: 0xCAFE_F00D,
            },
            5.0,
            8.0,
            700.0,
            20.0,
        );
        let r =
            run_scenario(&scen, DecoderConfig::defaults(), 100, DEFAULT_STABLE_N, "default").unwrap();
        // Any chars emitted before the CW onset are necessarily false
        // positives, because the audio before onset is voice formants.
        if let Some(first_char) = r.t_first_char_ms {
            assert!(
                first_char >= scen.cw_onset_ms,
                "decoder emitted char at {} ms, before CW onset at {} ms",
                first_char,
                scen.cw_onset_ms,
            );
        }
    }

    #[test]
    fn long_clean_cw_holds_lock() {
        // 25s of clean synth CW with 1s lead-in. After first lock the
        // decoder must not drop, and uptime should be near-perfect.
        let scen = make_synth_scenario(
            "long_clean_cw_test",
            16000,
            LeadIn::Silence,
            1.0,
            25.0,
            700.0,
            20.0,
        );
        let r =
            run_scenario(&scen, DecoderConfig::defaults(), 100, DEFAULT_STABLE_N, "default").unwrap();
        assert!(
            r.t_first_locked_ms.is_some(),
            "expected the decoder to reach Locked on clean CW"
        );
        assert_eq!(
            r.n_pitch_lost_after_lock, 0,
            "decoder dropped pitch lock {} times during clean CW",
            r.n_pitch_lost_after_lock
        );
        if let Some(uptime) = r.lock_uptime_ratio {
            assert!(
                uptime > 0.9,
                "lock uptime ratio {uptime:.2} below 0.9 on clean CW",
            );
        }
    }
}

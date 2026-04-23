//! Quantitative eval harness for the streaming CW decoder.
//!
//! Runs the decoder against a fixed test suite (synthesized + real
//! recordings), prints per-test metrics, and computes aggregate
//! anti-ghost scores. Designed to be the regression metric we iterate
//! against while hardening the decoder.
//!
//! Usage:
//!     cargo run --release --bin eval                  # default suite
//!     cargo run --release --bin eval -- --json        # machine-readable
//!     cargo run --release --bin eval -- --only real   # filter by name

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::Result;
use cw_decoder_poc::audio;
use cw_decoder_poc::ditdah_streaming::{
    run_causal_baseline, run_causal_baseline_trace, CausalBaselineConfig, CausalBaselineTrace,
};
use cw_decoder_poc::streaming::{DecoderConfig, StreamEvent, StreamingDecoder};
use rayon::prelude::*;
use serde::Deserialize;

const SYNTH_RATE: u32 = 12_000;
const SYNTH_TONE_HZ: f32 = 700.0;
const PARIS_REF: &str = "PARIS PARIS PARIS PARIS PARIS";

#[derive(Debug, Clone)]
struct TestCase {
    name: &'static str,
    source: Source,
    expectation: Expectation,
}

#[derive(Debug, Clone)]
enum Source {
    File(PathBuf),
    Silence {
        secs: f32,
    },
    WhiteNoise {
        secs: f32,
        amplitude: f32,
    },
    /// Noise with periodic impulse spikes (simulates QRN/static crashes
    /// from atmospherics or switching equipment near the rig).
    BurstyNoise {
        secs: f32,
        floor: f32,
        spike_amp: f32,
        spike_hz: f32,
        spike_dur_ms: f32,
    },
    /// White noise plus a colored peak around `peak_hz` to mimic an open
    /// receiver passband with a hiss "shape" — the kind of audio that's
    /// most likely to false-lock a pitch detector.
    ColoredHiss {
        secs: f32,
        amplitude: f32,
        peak_hz: f32,
    },
    SynthCw {
        text: &'static str,
        wpm: f32,
        tone_hz: f32,
        snr_db: Option<f32>,
        secs_padding: f32,
    },
}

#[derive(Debug, Clone)]
struct Expectation {
    reference: Option<String>,
    /// Hard upper bound on decoded chars (None = no cap).
    max_chars: Option<usize>,
    /// Lower bound on decoded chars (None = no floor).
    min_chars: Option<usize>,
}

#[derive(Debug, Default)]
struct Metrics {
    duration_s: f32,
    decoded: String,
    char_count: usize,
    wpm_last: Option<f32>,
    pitch_hz: Option<f32>,
    lock_time_s: Option<f32>,
    chars_per_minute: f32,
    cer: Option<f32>,
    pass: bool,
    notes: String,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json_mode = args.iter().any(|a| a == "--json");
    let label_files = collect_label_files(&args)?;
    if !label_files.is_empty() {
        let score_cfg = LabelScoreConfig {
            baseline: CausalBaselineConfig {
                window_seconds: arg_value_f32(&args, "--window").unwrap_or(20.0),
                min_window_seconds: arg_value_f32(&args, "--min-window").unwrap_or(4.0),
                decode_every_ms: arg_value_u32(&args, "--decode-every-ms").unwrap_or(1000),
                required_confirmations: arg_value_usize(&args, "--confirmations").unwrap_or(2),
            },
            mode: parse_label_score_mode(&args),
            pre_roll_s: arg_value_f32(&args, "--pre-roll-ms").unwrap_or(0.0) / 1000.0,
            post_roll_s: arg_value_f32(&args, "--post-roll-ms").unwrap_or(0.0) / 1000.0,
            streaming_cfg: parse_streaming_decoder_config(&args),
        };
        let top = arg_value_usize(&args, "--top").unwrap_or(10);
        if args.iter().any(|a| a == "--sweep-ditdah") {
            let wide = args.iter().any(|a| a == "--wide-sweep");
            return run_label_sweep(&label_files, top, wide, score_cfg, json_mode);
        }
        return run_label_score(&label_files, score_cfg, json_mode);
    }

    let only: Option<String> = args
        .windows(2)
        .find(|w| w[0] == "--only")
        .map(|w| w[1].clone());

    let cases = build_suite();
    let cfg = DecoderConfig::defaults();

    if !json_mode {
        println!(
            "CW DECODER EVAL  ({} tests, defaults: min_snr={:.1}dB pitch={:.1}dB scale={:.2})",
            cases.len(),
            cfg.min_snr_db,
            cfg.pitch_min_snr_db,
            cfg.threshold_scale,
        );
        println!("{}", "=".repeat(96));
    }

    let mut results = Vec::new();
    let mut ghost_chars = 0usize;

    for case in &cases {
        if let Some(filter) = &only {
            if !case.name.contains(filter.as_str()) {
                continue;
            }
        }
        let metrics = run_case(case, cfg)?;
        if case.expectation.reference.is_none() && case.expectation.max_chars.is_some() {
            ghost_chars += metrics.char_count;
        }
        if !json_mode {
            print_case(case, &metrics);
        }
        results.push((case.clone(), metrics));
    }

    if json_mode {
        let summary = serde_json::json!({
            "config": {
                "min_snr_db": cfg.min_snr_db,
                "pitch_min_snr_db": cfg.pitch_min_snr_db,
                "threshold_scale": cfg.threshold_scale,
            },
            "ghost_chars": ghost_chars,
            "results": results.iter().map(|(c, m)| serde_json::json!({
                "name": c.name,
                "pass": m.pass,
                "duration_s": m.duration_s,
                "decoded": m.decoded,
                "chars": m.char_count,
                "cpm": m.chars_per_minute,
                "wpm": m.wpm_last,
                "pitch_hz": m.pitch_hz,
                "lock_time_s": m.lock_time_s,
                "cer": m.cer,
                "notes": m.notes,
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!("{}", "=".repeat(96));
        println!(
            "AGGREGATE  ghost_chars={}  pass={}/{}",
            ghost_chars,
            results.iter().filter(|(_, m)| m.pass).count(),
            results.len(),
        );
    }

    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
struct LabelRow {
    source: PathBuf,
    start_s: f32,
    end_s: f32,
    correct_copy: String,
}

#[derive(Debug, Clone)]
struct LabelExample {
    name: String,
    source: PathBuf,
    start_s: f32,
    end_s: f32,
    truth: String,
}

#[derive(Debug, Clone)]
struct LabelScore {
    example: LabelExample,
    decoded: String,
    distance: usize,
    cer: f32,
    exact: bool,
    failure_class: &'static str,
}

#[derive(Debug, Clone)]
struct SweepSummary {
    cfg: CausalBaselineConfig,
    exact: usize,
    total_distance: usize,
    total_cer: f32,
    average_cer: f32,
    worst_cer: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LabelScoreMode {
    ExactWindow,
    FullStream,
}

#[derive(Debug, Clone, Copy)]
struct LabelScoreConfig {
    baseline: CausalBaselineConfig,
    mode: LabelScoreMode,
    pre_roll_s: f32,
    post_roll_s: f32,
    streaming_cfg: DecoderConfig,
}

impl LabelScoreMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::ExactWindow => "exact-window",
            Self::FullStream => "full-stream",
        }
    }
}

fn run_label_score(
    label_files: &[PathBuf],
    score_cfg: LabelScoreConfig,
    json_mode: bool,
) -> Result<()> {
    let labels = load_label_examples(label_files)?;
    let audio_cache = load_audio_cache(&labels)?;
    let scores = score_labels(&labels, &audio_cache, score_cfg);
    let exact = scores.iter().filter(|s| s.exact).count();
    let total_distance: usize = scores.iter().map(|s| s.distance).sum();
    let avg_cer = scores.iter().map(|s| s.cer).sum::<f32>() / scores.len().max(1) as f32;
    if json_mode {
        let summary = serde_json::json!({
            "kind": "label-score",
            "labels": scores.len(),
            "mode": score_cfg.mode.as_str(),
            "pre_roll_ms": (score_cfg.pre_roll_s * 1000.0).round() as u32,
            "post_roll_ms": (score_cfg.post_roll_s * 1000.0).round() as u32,
            "baseline": {
                "window_seconds": score_cfg.baseline.window_seconds,
                "min_window_seconds": score_cfg.baseline.min_window_seconds,
                "decode_every_ms": score_cfg.baseline.decode_every_ms,
                "required_confirmations": score_cfg.baseline.required_confirmations,
            },
            "streaming": {
                "min_snr_db": score_cfg.streaming_cfg.min_snr_db,
                "pitch_min_snr_db": score_cfg.streaming_cfg.pitch_min_snr_db,
                "threshold_scale": score_cfg.streaming_cfg.threshold_scale,
                "auto_threshold": score_cfg.streaming_cfg.auto_threshold,
                "experimental_range_lock": score_cfg.streaming_cfg.experimental_range_lock,
                "range_lock_min_hz": score_cfg.streaming_cfg.range_lock_min_hz,
                "range_lock_max_hz": score_cfg.streaming_cfg.range_lock_max_hz,
            },
            "summary": {
                "exact": exact,
                "total_distance": total_distance,
                "average_cer": avg_cer,
            },
            "rows": scores.iter().map(|score| serde_json::json!({
                "name": score.example.name,
                "source": score.example.source,
                "start_s": score.example.start_s,
                "end_s": score.example.end_s,
                "truth": score.example.truth,
                "decoded": score.decoded,
                "distance": score.distance,
                "cer": score.cer,
                "exact": score.exact,
                "failure_class": score.failure_class,
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    println!(
        "LABEL SCORE  labels={}  mode={}  pre={}ms  post={}ms",
        scores.len(),
        score_cfg.mode.as_str(),
        (score_cfg.pre_roll_s * 1000.0).round() as u32,
        (score_cfg.post_roll_s * 1000.0).round() as u32
    );
    for score in &scores {
        println!(
            "{:<30} {:>5}  cer={:.2}  class={:<18} truth={}  decoded={}",
            score.example.name,
            if score.exact { "EXACT" } else { "MISS" },
            score.cer,
            score.failure_class,
            score.example.truth,
            score.decoded
        );
    }
    println!(
        "\nSUMMARY  exact={}/{}  total_distance={}  avg_cer={:.2}  window={:.1}s min={:.1}s every={}ms confirmations={}",
        exact,
        scores.len(),
        total_distance,
        avg_cer,
        score_cfg.baseline.window_seconds,
        score_cfg.baseline.min_window_seconds,
        score_cfg.baseline.decode_every_ms,
        score_cfg.baseline.required_confirmations
    );
    Ok(())
}

fn run_label_sweep(
    label_files: &[PathBuf],
    top: usize,
    wide: bool,
    score_cfg: LabelScoreConfig,
    json_mode: bool,
) -> Result<()> {
    let labels = load_label_examples(label_files)?;
    let audio_cache = load_audio_cache(&labels)?;
    let mut seen = HashSet::new();
    let coarse_configs = build_initial_sweep_configs(wide, &mut seen);
    let coarse_count = coarse_configs.len();
    let mut summaries = summarize_sweep_configs(&coarse_configs, &labels, &audio_cache, score_cfg);
    sort_sweep_summaries(&mut summaries);

    let refined_configs = build_refined_sweep_configs(&summaries, wide, &mut seen);
    let refined_count = refined_configs.len();
    summaries.extend(summarize_sweep_configs(
        &refined_configs,
        &labels,
        &audio_cache,
        score_cfg,
    ));
    sort_sweep_summaries(&mut summaries);

    if json_mode {
        let summary = serde_json::json!({
            "kind": "label-sweep",
            "labels": labels.len(),
            "mode": score_cfg.mode.as_str(),
            "pre_roll_ms": (score_cfg.pre_roll_s * 1000.0).round() as u32,
            "post_roll_ms": (score_cfg.post_roll_s * 1000.0).round() as u32,
            "sweep_mode": if wide { "wide" } else { "interactive" },
            "coarse_configs": coarse_count,
            "refined_configs": refined_count,
            "results": summaries.iter().map(|summary| serde_json::json!({
                "exact": summary.exact,
                "total_distance": summary.total_distance,
                "total_cer": summary.total_cer,
                "average_cer": summary.average_cer,
                "worst_cer": summary.worst_cer,
                "window_seconds": summary.cfg.window_seconds,
                "min_window_seconds": summary.cfg.min_window_seconds,
                "decode_every_ms": summary.cfg.decode_every_ms,
                "required_confirmations": summary.cfg.required_confirmations,
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    println!(
        "LABEL SWEEP  labels={}  configs={} (coarse={} refined={})  sweep={}  score={}  pre={}ms  post={}ms",
        labels.len(),
        summaries.len(),
        coarse_count,
        refined_count,
        if wide { "wide" } else { "interactive" },
        score_cfg.mode.as_str(),
        (score_cfg.pre_roll_s * 1000.0).round() as u32,
        (score_cfg.post_roll_s * 1000.0).round() as u32
    );
    println!(
        "{:<6} {:<8} {:<10} {:<10} {:<8} {:<6} {:<6} {:<12}",
        "exact", "dist", "avg_cer", "worst", "window", "min", "every", "confirmations"
    );
    println!("{}", "-".repeat(80));
    for summary in summaries.iter().take(top) {
        println!(
            "{:<6} {:<8} {:<10.3} {:<10.3} {:<8.1} {:<6.1} {:<6} {:<12}",
            format!("{}/{}", summary.exact, labels.len()),
            summary.total_distance,
            summary.average_cer,
            summary.worst_cer,
            summary.cfg.window_seconds,
            summary.cfg.min_window_seconds,
            summary.cfg.decode_every_ms,
            summary.cfg.required_confirmations
        );
    }
    Ok(())
}

fn summarize_sweep_configs(
    configs: &[CausalBaselineConfig],
    labels: &[LabelExample],
    audio_cache: &HashMap<PathBuf, audio::DecodedAudio>,
    score_cfg: LabelScoreConfig,
) -> Vec<SweepSummary> {
    configs
        .par_iter()
        .copied()
        .map(|cfg| {
            let scores = score_labels(
                labels,
                audio_cache,
                LabelScoreConfig {
                    baseline: cfg,
                    ..score_cfg
                },
            );
            let total_cer = scores.iter().map(|s| s.cer).sum::<f32>();
            let average_cer = total_cer / scores.len().max(1) as f32;
            let worst_cer = scores.iter().map(|s| s.cer).fold(0.0_f32, f32::max);
            SweepSummary {
                cfg,
                exact: scores.iter().filter(|s| s.exact).count(),
                total_distance: scores.iter().map(|s| s.distance).sum(),
                total_cer,
                average_cer,
                worst_cer,
            }
        })
        .collect()
}

fn sort_sweep_summaries(summaries: &mut [SweepSummary]) {
    summaries.sort_by(|a, b| {
        b.exact
            .cmp(&a.exact)
            .then_with(|| a.total_distance.cmp(&b.total_distance))
            .then_with(|| {
                a.average_cer
                    .partial_cmp(&b.average_cer)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                a.worst_cer
                    .partial_cmp(&b.worst_cer)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                a.cfg
                    .window_seconds
                    .partial_cmp(&b.cfg.window_seconds)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                a.cfg
                    .min_window_seconds
                    .partial_cmp(&b.cfg.min_window_seconds)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.cfg.decode_every_ms.cmp(&b.cfg.decode_every_ms))
            .then_with(|| {
                a.cfg
                    .required_confirmations
                    .cmp(&b.cfg.required_confirmations)
            })
    });
}

fn build_initial_sweep_configs(
    wide: bool,
    seen: &mut HashSet<(i32, i32, u32, usize)>,
) -> Vec<CausalBaselineConfig> {
    let window_values: &[f32] = if wide {
        &[12.0, 16.0, 20.0, 24.0, 30.0]
    } else {
        &[16.0, 20.0, 24.0]
    };
    let min_window_values: &[f32] = if wide {
        &[0.5, 1.0, 2.0, 3.0, 4.0]
    } else {
        &[0.5, 1.0, 2.0]
    };
    let decode_every_values: &[u32] = if wide {
        &[250, 500, 750, 1000, 1500]
    } else {
        &[500, 1000]
    };
    let mut configs = Vec::new();
    for window_seconds in window_values {
        for min_window_seconds in min_window_values {
            if min_window_seconds > window_seconds {
                continue;
            }
            for decode_every_ms in decode_every_values {
                for required_confirmations in [1usize, 2, 3] {
                    push_sweep_config(
                        &mut configs,
                        seen,
                        CausalBaselineConfig {
                            window_seconds: *window_seconds,
                            min_window_seconds: *min_window_seconds,
                            decode_every_ms: *decode_every_ms,
                            required_confirmations,
                        },
                    );
                }
            }
        }
    }
    configs
}

fn build_refined_sweep_configs(
    seeds: &[SweepSummary],
    wide: bool,
    seen: &mut HashSet<(i32, i32, u32, usize)>,
) -> Vec<CausalBaselineConfig> {
    let seed_limit = if wide { 2 } else { 1 };
    let window_deltas: &[f32] = if wide {
        &[-2.0, -1.0, 1.0, 2.0]
    } else {
        &[-1.0, 1.0]
    };
    let min_deltas: &[f32] = if wide {
        &[-0.5, -0.25, 0.25, 0.5]
    } else {
        &[-0.25, 0.25]
    };
    let every_deltas: &[i32] = if wide {
        &[-250, -100, 100, 250]
    } else {
        &[-100, 100]
    };

    let mut configs = Vec::new();
    for seed in seeds.iter().take(seed_limit) {
        let base = seed.cfg;
        push_sweep_config(&mut configs, seen, base);

        for window_delta in window_deltas {
            push_sweep_config(
                &mut configs,
                seen,
                refined_sweep_cfg(base, *window_delta, 0.0, 0, base.required_confirmations),
            );
        }
        for min_delta in min_deltas {
            push_sweep_config(
                &mut configs,
                seen,
                refined_sweep_cfg(base, 0.0, *min_delta, 0, base.required_confirmations),
            );
        }
        for every_delta in every_deltas {
            push_sweep_config(
                &mut configs,
                seen,
                refined_sweep_cfg(base, 0.0, 0.0, *every_delta, base.required_confirmations),
            );
        }

        for confirmations in [
            base.required_confirmations.saturating_sub(1),
            base.required_confirmations + 1,
        ] {
            push_sweep_config(
                &mut configs,
                seen,
                refined_sweep_cfg(base, 0.0, 0.0, 0, confirmations),
            );
        }

        for &(window_delta, min_delta) in &[(-1.0, -0.25), (-1.0, 0.25), (1.0, -0.25), (1.0, 0.25)]
        {
            let scaled_window_delta = if wide {
                window_delta * 2.0
            } else {
                window_delta
            };
            let scaled_min_delta = if wide { min_delta * 2.0 } else { min_delta };
            push_sweep_config(
                &mut configs,
                seen,
                refined_sweep_cfg(
                    base,
                    scaled_window_delta,
                    scaled_min_delta,
                    0,
                    base.required_confirmations,
                ),
            );
        }

        for &(every_delta, confirmation_delta) in &[(-1, -1), (-1, 1), (1, -1), (1, 1)] {
            let scaled_every_delta = if wide {
                every_delta * 250
            } else {
                every_delta * 100
            };
            let confirmations =
                (base.required_confirmations as isize + confirmation_delta).clamp(1, 6) as usize;
            push_sweep_config(
                &mut configs,
                seen,
                refined_sweep_cfg(base, 0.0, 0.0, scaled_every_delta, confirmations),
            );
        }
    }

    configs
}

fn refined_sweep_cfg(
    base: CausalBaselineConfig,
    window_delta: f32,
    min_delta: f32,
    every_delta: i32,
    confirmations: usize,
) -> CausalBaselineConfig {
    CausalBaselineConfig {
        window_seconds: (base.window_seconds + window_delta).clamp(8.0, 40.0),
        min_window_seconds: (base.min_window_seconds + min_delta).clamp(0.25, 8.0),
        decode_every_ms: (base.decode_every_ms as i32 + every_delta).clamp(100, 3000) as u32,
        required_confirmations: confirmations.clamp(1, 6),
    }
}

fn push_sweep_config(
    configs: &mut Vec<CausalBaselineConfig>,
    seen: &mut HashSet<(i32, i32, u32, usize)>,
    cfg: CausalBaselineConfig,
) {
    if cfg.min_window_seconds > cfg.window_seconds {
        return;
    }

    let key = (
        (cfg.window_seconds * 100.0).round() as i32,
        (cfg.min_window_seconds * 100.0).round() as i32,
        cfg.decode_every_ms,
        cfg.required_confirmations,
    );
    if seen.insert(key) {
        configs.push(cfg);
    }
}

fn load_label_examples(label_files: &[PathBuf]) -> Result<Vec<LabelExample>> {
    let mut examples = Vec::new();
    for path in label_files {
        let label_dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
        for (index, line) in std::fs::read_to_string(path)?.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let row: LabelRow = serde_json::from_str(line)?;
            let name = format!(
                "{}#{}",
                path.file_stem().and_then(|s| s.to_str()).unwrap_or("label"),
                index + 1
            );
            examples.push(LabelExample {
                name,
                source: resolve_label_source(&row.source, label_dir),
                start_s: row.start_s,
                end_s: row.end_s,
                truth: normalize_copy(&row.correct_copy),
            });
        }
    }
    Ok(examples)
}

fn load_audio_cache(labels: &[LabelExample]) -> Result<HashMap<PathBuf, audio::DecodedAudio>> {
    let mut cache = HashMap::new();
    for source in labels.iter().map(|label| label.source.clone()) {
        cache
            .entry(source.clone())
            .or_insert(audio::decode_file(&source)?);
    }
    Ok(cache)
}

fn score_labels(
    labels: &[LabelExample],
    audio_cache: &HashMap<PathBuf, audio::DecodedAudio>,
    score_cfg: LabelScoreConfig,
) -> Vec<LabelScore> {
    match score_cfg.mode {
        LabelScoreMode::ExactWindow => score_labels_exact_window(labels, audio_cache, score_cfg),
        LabelScoreMode::FullStream => score_labels_full_stream(labels, audio_cache, score_cfg),
    }
}

fn score_labels_exact_window(
    labels: &[LabelExample],
    audio_cache: &HashMap<PathBuf, audio::DecodedAudio>,
    score_cfg: LabelScoreConfig,
) -> Vec<LabelScore> {
    if score_cfg.streaming_cfg.experimental_range_lock {
        return score_labels_exact_window_streaming(labels, audio_cache, score_cfg);
    }

    labels
        .iter()
        .map(|example| {
            let audio = audio_cache
                .get(&example.source)
                .expect("audio cache missing source");
            let (start, end) = expanded_sample_bounds(audio, example, score_cfg);
            let samples = if end > start {
                &audio.samples[start..end]
            } else {
                &[]
            };
            let outcome = run_causal_baseline(samples, audio.sample_rate, score_cfg.baseline);
            build_label_score(example, &normalize_copy(&outcome.transcript))
        })
        .collect()
}

fn score_labels_exact_window_streaming(
    labels: &[LabelExample],
    audio_cache: &HashMap<PathBuf, audio::DecodedAudio>,
    score_cfg: LabelScoreConfig,
) -> Vec<LabelScore> {
    labels
        .iter()
        .map(|example| {
            let audio = audio_cache
                .get(&example.source)
                .expect("audio cache missing source");
            let (start, end) = expanded_sample_bounds(audio, example, score_cfg);
            let samples = if end > start {
                &audio.samples[start..end]
            } else {
                &[]
            };
            let trace = run_streaming_trace(samples, audio.sample_rate, score_cfg.streaming_cfg)
                .expect("streaming exact-window trace failed");
            build_label_score(example, &normalize_copy(&trace.transcript))
        })
        .collect()
}

fn score_labels_full_stream(
    labels: &[LabelExample],
    audio_cache: &HashMap<PathBuf, audio::DecodedAudio>,
    score_cfg: LabelScoreConfig,
) -> Vec<LabelScore> {
    if score_cfg.streaming_cfg.experimental_range_lock {
        return score_labels_full_stream_streaming(labels, audio_cache, score_cfg);
    }

    let mut traces = HashMap::new();
    for source in labels.iter().map(|label| label.source.clone()) {
        traces.entry(source.clone()).or_insert_with(|| {
            let audio = audio_cache
                .get(&source)
                .expect("audio cache missing source");
            run_causal_baseline_trace(&audio.samples, audio.sample_rate, score_cfg.baseline)
        });
    }

    labels
        .iter()
        .map(|example| {
            let audio = audio_cache
                .get(&example.source)
                .expect("audio cache missing source");
            let trace = traces.get(&example.source).expect("trace missing source");
            let (start, end) = expanded_sample_bounds(audio, example, score_cfg);
            let before = transcript_at_or_before(trace, start);
            let after = transcript_at_or_before(trace, end);
            let decoded = normalize_copy(&extract_transcript_delta(before, after));
            build_label_score(example, &decoded)
        })
        .collect()
}

fn score_labels_full_stream_streaming(
    labels: &[LabelExample],
    audio_cache: &HashMap<PathBuf, audio::DecodedAudio>,
    score_cfg: LabelScoreConfig,
) -> Vec<LabelScore> {
    let mut traces = HashMap::new();
    for source in labels.iter().map(|label| label.source.clone()) {
        traces.entry(source.clone()).or_insert_with(|| {
            let audio = audio_cache
                .get(&source)
                .expect("audio cache missing source");
            run_streaming_trace(&audio.samples, audio.sample_rate, score_cfg.streaming_cfg)
                .expect("streaming full-trace failed")
        });
    }

    labels
        .iter()
        .map(|example| {
            let audio = audio_cache
                .get(&example.source)
                .expect("audio cache missing source");
            let trace = traces
                .get(&example.source)
                .expect("streaming trace missing source");
            let (start, end) = expanded_sample_bounds(audio, example, score_cfg);
            let before = transcript_at_or_before_streaming(trace, start);
            let after = transcript_at_or_before_streaming(trace, end);
            let decoded = normalize_copy(&extract_transcript_delta(before, after));
            build_label_score(example, &decoded)
        })
        .collect()
}

fn build_label_score(example: &LabelExample, decoded: &str) -> LabelScore {
    let normalized = normalize_copy(decoded);
    let distance = edit_distance(&example.truth, &normalized);
    let cer = distance as f32 / example.truth.chars().count().max(1) as f32;
    LabelScore {
        example: example.clone(),
        decoded: normalized.clone(),
        distance,
        cer,
        exact: normalized == example.truth,
        failure_class: classify_failure(&example.truth, &normalized, distance, cer),
    }
}

fn normalize_copy(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

fn edit_distance(reference: &str, hypothesis: &str) -> usize {
    let r: Vec<char> = reference.chars().collect();
    let h: Vec<char> = hypothesis.chars().collect();
    if r.is_empty() {
        return h.len();
    }
    let mut prev: Vec<usize> = (0..=h.len()).collect();
    let mut cur = vec![0usize; h.len() + 1];
    for (i, rc) in r.iter().enumerate() {
        cur[0] = i + 1;
        for (j, hc) in h.iter().enumerate() {
            let cost = if rc == hc { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[h.len()]
}

fn expanded_sample_bounds(
    audio: &audio::DecodedAudio,
    example: &LabelExample,
    score_cfg: LabelScoreConfig,
) -> (usize, usize) {
    let start_s = (example.start_s - score_cfg.pre_roll_s).max(0.0);
    let end_s = (example.end_s + score_cfg.post_roll_s)
        .min(audio.samples.len() as f32 / audio.sample_rate.max(1) as f32);
    let start = ((start_s * audio.sample_rate as f32).floor() as usize).min(audio.samples.len());
    let end = ((end_s * audio.sample_rate as f32).ceil() as usize).min(audio.samples.len());
    (start, end.max(start))
}

fn transcript_at_or_before(trace: &CausalBaselineTrace, sample_index: usize) -> &str {
    let index = trace
        .snapshots
        .partition_point(|snapshot| snapshot.end_sample <= sample_index);
    if index == 0 {
        ""
    } else {
        trace.snapshots[index - 1].transcript.as_str()
    }
}

#[derive(Debug, Clone)]
struct StreamingTraceSnapshot {
    end_sample: usize,
    transcript: String,
}

#[derive(Debug, Clone)]
struct StreamingTrace {
    transcript: String,
    snapshots: Vec<StreamingTraceSnapshot>,
}

fn run_streaming_trace(
    samples: &[f32],
    sample_rate: u32,
    cfg: DecoderConfig,
) -> Result<StreamingTrace> {
    let mut decoder = StreamingDecoder::new(sample_rate)?;
    decoder.set_config(cfg);
    let chunk_samples = (sample_rate as usize / 20).max(64);
    let mut transcript = String::new();
    let mut consumed = 0usize;
    let mut snapshots = Vec::new();

    for chunk in samples.chunks(chunk_samples) {
        consumed += chunk.len();
        for ev in decoder.feed(chunk)? {
            if append_stream_event_to_transcript(&mut transcript, &ev) {
                snapshots.push(StreamingTraceSnapshot {
                    end_sample: consumed,
                    transcript: transcript.trim().to_string(),
                });
            }
        }
    }

    for ev in decoder.flush() {
        if append_stream_event_to_transcript(&mut transcript, &ev) {
            snapshots.push(StreamingTraceSnapshot {
                end_sample: consumed,
                transcript: transcript.trim().to_string(),
            });
        }
    }

    Ok(StreamingTrace {
        transcript: transcript.trim().to_string(),
        snapshots,
    })
}

fn append_stream_event_to_transcript(transcript: &mut String, ev: &StreamEvent) -> bool {
    match ev {
        StreamEvent::Char { ch, .. } => {
            transcript.push(*ch);
            true
        }
        StreamEvent::Word => {
            transcript.push(' ');
            true
        }
        StreamEvent::Garbled { .. } => {
            transcript.push('?');
            true
        }
        _ => false,
    }
}

fn transcript_at_or_before_streaming(trace: &StreamingTrace, sample_index: usize) -> &str {
    let index = trace
        .snapshots
        .partition_point(|snapshot| snapshot.end_sample <= sample_index);
    if index == 0 {
        ""
    } else {
        trace.snapshots[index - 1].transcript.as_str()
    }
}

fn extract_transcript_delta(before: &str, after: &str) -> String {
    let before_tokens: Vec<&str> = before.split_whitespace().collect();
    let after_tokens: Vec<&str> = after.split_whitespace().collect();
    if before_tokens.is_empty() {
        return after_tokens.join(" ");
    }
    if after_tokens.is_empty() {
        return String::new();
    }
    if before_tokens.len() <= after_tokens.len()
        && before_tokens == after_tokens[..before_tokens.len()]
    {
        return after_tokens[before_tokens.len()..].join(" ");
    }

    let max_overlap = before_tokens.len().min(after_tokens.len());
    for overlap in (1..=max_overlap).rev() {
        if before_tokens[before_tokens.len() - overlap..] == after_tokens[..overlap] {
            return after_tokens[overlap..].join(" ");
        }
    }

    after_tokens.join(" ")
}

fn classify_failure(truth: &str, decoded: &str, distance: usize, cer: f32) -> &'static str {
    if decoded.is_empty() {
        return "empty_output";
    }
    if decoded == truth {
        return "exact";
    }
    if truth.replace(' ', "") == decoded.replace(' ', "") {
        return "spacing_only_error";
    }

    let truth_chars: Vec<char> = truth.chars().collect();
    let decoded_chars: Vec<char> = decoded.chars().collect();
    let common_prefix = truth_chars
        .iter()
        .zip(decoded_chars.iter())
        .take_while(|(left, right)| left == right)
        .count();
    let common_suffix = truth_chars
        .iter()
        .rev()
        .zip(decoded_chars.iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let max_len = truth_chars.len().max(decoded_chars.len());

    if common_suffix + 2 >= max_len && common_prefix < common_suffix {
        return "leading_edge_error";
    }
    if common_prefix + 2 >= max_len && common_suffix < common_prefix {
        return "trailing_edge_error";
    }
    if truth.contains(decoded) {
        return "dropped_region";
    }
    if decoded.contains(truth) {
        return "extra_output";
    }
    if distance <= 2 || cer <= 0.2 {
        return "near_match";
    }
    "garbage_decode"
}

fn arg_values(args: &[String], key: &str) -> Vec<String> {
    args.windows(2)
        .filter(|w| w[0] == key)
        .map(|w| w[1].clone())
        .collect()
}

fn collect_label_files(args: &[String]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for value in arg_values(args, "--labels") {
        collect_label_path(&mut files, PathBuf::from(value))?;
    }
    for value in arg_values(args, "--labels-dir") {
        collect_labels_from_dir(&mut files, PathBuf::from(value))?;
    }
    if args.iter().any(|a| a == "--all-labels") {
        collect_labels_from_dir(&mut files, PathBuf::from("data").join("cw-samples"))?;
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn collect_label_path(files: &mut Vec<PathBuf>, path: PathBuf) -> Result<()> {
    if path.is_dir() {
        collect_labels_from_dir(files, path)?;
    } else {
        files.push(path);
    }
    Ok(())
}

fn collect_labels_from_dir(files: &mut Vec<PathBuf>, dir: PathBuf) -> Result<()> {
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".labels.jsonl"))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn resolve_label_source(source: &std::path::Path, label_dir: &std::path::Path) -> PathBuf {
    if source.is_absolute() {
        source.to_path_buf()
    } else {
        label_dir.join(source)
    }
}

fn parse_label_score_mode(args: &[String]) -> LabelScoreMode {
    if args.iter().any(|arg| arg == "--full-stream") {
        return LabelScoreMode::FullStream;
    }

    match args
        .windows(2)
        .find(|w| w[0] == "--mode")
        .map(|w| w[1].as_str())
    {
        Some("full-stream") => LabelScoreMode::FullStream,
        _ => LabelScoreMode::ExactWindow,
    }
}

fn parse_streaming_decoder_config(args: &[String]) -> DecoderConfig {
    let defaults = DecoderConfig::default();
    DecoderConfig {
        min_snr_db: arg_value_f32(args, "--min-snr-db").unwrap_or(defaults.min_snr_db),
        pitch_min_snr_db: arg_value_f32(args, "--pitch-min-snr-db")
            .unwrap_or(defaults.pitch_min_snr_db),
        threshold_scale: arg_value_f32(args, "--threshold-scale")
            .unwrap_or(defaults.threshold_scale),
        auto_threshold: !args.iter().any(|a| a == "--no-auto-threshold"),
        experimental_range_lock: args.iter().any(|a| a == "--experimental-range-lock"),
        range_lock_min_hz: arg_value_f32(args, "--range-lock-min-hz")
            .unwrap_or(cw_decoder_poc::streaming::DEFAULT_RANGE_LOCK_MIN_HZ),
        range_lock_max_hz: arg_value_f32(args, "--range-lock-max-hz")
            .unwrap_or(cw_decoder_poc::streaming::DEFAULT_RANGE_LOCK_MAX_HZ),
        min_tone_purity: arg_value_f32(args, "--min-tone-purity")
            .unwrap_or(cw_decoder_poc::streaming::DEFAULT_MIN_TONE_PURITY),
        force_pitch_hz: arg_value_f32(args, "--force-pitch-hz").filter(|x| *x > 0.0),
        wide_bin_count: arg_value_f32(args, "--wide-bin-count")
            .map(|x| x.clamp(0.0, 16.0) as u8)
            .unwrap_or(0),
        min_pulse_dot_fraction: arg_value_f32(args, "--min-pulse-dot-fraction")
            .map(|x| x.max(0.0))
            .unwrap_or(0.0),
        min_gap_dot_fraction: arg_value_f32(args, "--min-gap-dot-fraction")
            .map(|x| x.max(0.0))
            .unwrap_or(0.0),
    }
}

fn arg_value_f32(args: &[String], key: &str) -> Option<f32> {
    args.windows(2)
        .find(|w| w[0] == key)
        .and_then(|w| w[1].parse::<f32>().ok())
}

fn arg_value_u32(args: &[String], key: &str) -> Option<u32> {
    args.windows(2)
        .find(|w| w[0] == key)
        .and_then(|w| w[1].parse::<u32>().ok())
}

fn arg_value_usize(args: &[String], key: &str) -> Option<usize> {
    args.windows(2)
        .find(|w| w[0] == key)
        .and_then(|w| w[1].parse::<usize>().ok())
}

fn build_suite() -> Vec<TestCase> {
    let mut v = vec![
        TestCase {
            name: "silence-30s",
            source: Source::Silence { secs: 30.0 },
            expectation: Expectation {
                reference: None,
                max_chars: Some(0),
                min_chars: None,
            },
        },
        TestCase {
            name: "white-noise-30s",
            source: Source::WhiteNoise {
                secs: 30.0,
                amplitude: 0.05,
            },
            expectation: Expectation {
                reference: None,
                max_chars: Some(0),
                min_chars: None,
            },
        },
        TestCase {
            name: "white-noise-loud-30s",
            source: Source::WhiteNoise {
                secs: 30.0,
                amplitude: 0.3,
            },
            expectation: Expectation {
                reference: None,
                max_chars: Some(0),
                min_chars: None,
            },
        },
        TestCase {
            name: "bursty-noise-30s",
            source: Source::BurstyNoise {
                secs: 30.0,
                floor: 0.03,
                spike_amp: 0.4,
                spike_hz: 4.0,
                spike_dur_ms: 60.0,
            },
            expectation: Expectation {
                reference: None,
                max_chars: Some(0),
                min_chars: None,
            },
        },
        TestCase {
            name: "colored-hiss-700hz",
            source: Source::ColoredHiss {
                secs: 30.0,
                amplitude: 0.1,
                peak_hz: 700.0,
            },
            expectation: Expectation {
                reference: None,
                max_chars: Some(0),
                min_chars: None,
            },
        },
        TestCase {
            name: "colored-hiss-500hz",
            source: Source::ColoredHiss {
                secs: 30.0,
                amplitude: 0.1,
                peak_hz: 500.0,
            },
            expectation: Expectation {
                reference: None,
                max_chars: Some(0),
                min_chars: None,
            },
        },
        TestCase {
            name: "clean-cw-20wpm",
            source: Source::SynthCw {
                text: PARIS_REF,
                wpm: 20.0,
                tone_hz: SYNTH_TONE_HZ,
                snr_db: None,
                secs_padding: 1.0,
            },
            expectation: Expectation {
                reference: Some(PARIS_REF.to_string()),
                max_chars: None,
                min_chars: Some(20),
            },
        },
        TestCase {
            name: "noisy-cw-snr10",
            source: Source::SynthCw {
                text: PARIS_REF,
                wpm: 20.0,
                tone_hz: SYNTH_TONE_HZ,
                snr_db: Some(10.0),
                secs_padding: 1.0,
            },
            expectation: Expectation {
                reference: Some(PARIS_REF.to_string()),
                max_chars: None,
                min_chars: Some(10),
            },
        },
        TestCase {
            name: "noisy-cw-snr5",
            source: Source::SynthCw {
                text: PARIS_REF,
                wpm: 20.0,
                tone_hz: SYNTH_TONE_HZ,
                snr_db: Some(5.0),
                secs_padding: 1.0,
            },
            expectation: Expectation {
                reference: Some(PARIS_REF.to_string()),
                max_chars: None,
                min_chars: None,
            },
        },
        TestCase {
            name: "noisy-cw-snr0",
            source: Source::SynthCw {
                text: PARIS_REF,
                wpm: 20.0,
                tone_hz: SYNTH_TONE_HZ,
                snr_db: Some(0.0),
                secs_padding: 1.0,
            },
            expectation: Expectation {
                // At 0 dB SNR we don't insist on accurate text (CER will be
                // bad), but we DO insist that we don't fabricate dozens of
                // ghost chars. Set max_chars at ~2x the reference so a wild
                // fabrication trips it.
                reference: None,
                max_chars: Some(60),
                min_chars: None,
            },
        },
    ];

    let recordings = [
        ("real-141552", "radio-20260421-141552.mp3", true),
        ("real-230617", "radio-20260421-230617.mp3", false),
        ("real-230649", "radio-20260421-230649.mp3", false),
        ("real-231323", "radio-20260421-231323.mp3", true),
    ];
    let repo_root = repo_root();
    for (name, fname, expect_signal) in recordings {
        let p = repo_root.join(fname);
        if !p.exists() {
            continue;
        }
        v.push(TestCase {
            name: leak(name.to_string()),
            source: Source::File(p),
            expectation: if expect_signal {
                Expectation {
                    reference: None,
                    max_chars: None,
                    min_chars: Some(5),
                }
            } else {
                Expectation {
                    reference: None,
                    max_chars: Some(0),
                    min_chars: None,
                }
            },
        });
    }
    v
}

fn repo_root() -> PathBuf {
    let here = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut p = here.clone();
    for _ in 0..6 {
        if p.join("proto").exists() && p.join("README.md").exists() {
            return p;
        }
        if !p.pop() {
            break;
        }
    }
    here
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn run_case(case: &TestCase, cfg: DecoderConfig) -> Result<Metrics> {
    let (samples, rate) = synthesize(&case.source)?;
    let mut decoder = StreamingDecoder::new(rate)?;
    decoder.set_config(cfg);

    let chunk_samples = (rate as usize / 20).max(64); // 50 ms chunks
    let mut transcript = String::new();
    let mut last_wpm: Option<f32> = None;
    let mut pitch: Option<f32> = None;
    let mut lock_time: Option<f32> = None;
    let mut consumed: usize = 0;

    for chunk in samples.chunks(chunk_samples) {
        consumed += chunk.len();
        let t = consumed as f32 / rate as f32;
        let events = decoder.feed(chunk)?;
        for ev in events {
            match ev {
                StreamEvent::Char { ch, .. } => transcript.push(ch),
                StreamEvent::Word => transcript.push(' '),
                StreamEvent::Garbled { .. } => transcript.push('?'),
                StreamEvent::WpmUpdate { wpm } => last_wpm = Some(wpm),
                StreamEvent::PitchUpdate { pitch_hz } => {
                    pitch = Some(pitch_hz);
                    if lock_time.is_none() {
                        lock_time = Some(t);
                    }
                }
                _ => {}
            }
        }
    }

    let dur = samples.len() as f32 / rate as f32;
    let cleaned = transcript.trim().to_string();
    let nchars = cleaned.chars().filter(|c| !c.is_whitespace()).count();
    let cpm = if dur > 0.0 {
        nchars as f32 * 60.0 / dur
    } else {
        0.0
    };
    let cer = case
        .expectation
        .reference
        .as_ref()
        .map(|r| char_error_rate(r, &cleaned));

    let mut notes = String::new();
    let mut pass = true;
    if let Some(max) = case.expectation.max_chars {
        if nchars > max {
            pass = false;
            notes.push_str(&format!("ghost: got {nchars} chars, max {max}; "));
        }
    }
    if let Some(min) = case.expectation.min_chars {
        if nchars < min {
            pass = false;
            notes.push_str(&format!("recall: got {nchars} chars, min {min}; "));
        }
    }
    if let Some(c) = cer {
        if c > 0.5 {
            pass = false;
            notes.push_str(&format!("cer={c:.2} > 0.5; "));
        }
    }

    Ok(Metrics {
        duration_s: dur,
        decoded: cleaned,
        char_count: transcript.chars().filter(|c| !c.is_whitespace()).count(),
        wpm_last: last_wpm,
        pitch_hz: pitch,
        lock_time_s: lock_time,
        chars_per_minute: cpm,
        cer,
        pass,
        notes: notes.trim_end_matches("; ").to_string(),
    })
}

fn synthesize(src: &Source) -> Result<(Vec<f32>, u32)> {
    match src {
        Source::File(p) => {
            let a = audio::decode_file(p)?;
            Ok((a.samples, a.sample_rate))
        }
        Source::Silence { secs } => {
            Ok((vec![0.0; (SYNTH_RATE as f32 * secs) as usize], SYNTH_RATE))
        }
        Source::WhiteNoise { secs, amplitude } => {
            let n = (SYNTH_RATE as f32 * secs) as usize;
            let mut rng = SmallRng::new(0xC0FFEE);
            let mut v = Vec::with_capacity(n);
            for _ in 0..n {
                v.push(rng.normal() * amplitude);
            }
            Ok((v, SYNTH_RATE))
        }
        Source::BurstyNoise {
            secs,
            floor,
            spike_amp,
            spike_hz,
            spike_dur_ms,
        } => {
            let n = (SYNTH_RATE as f32 * secs) as usize;
            let mut rng = SmallRng::new(0xBADBEEF);
            let mut v: Vec<f32> = (0..n).map(|_| rng.normal() * floor).collect();
            // Sprinkle short, broadband impulses ("static crashes"). Each
            // burst is wideband Gaussian noise, not a sine; this is the
            // worst case for a Goertzel-only decoder because the impulse
            // contains energy at the locked pitch.
            let period = (SYNTH_RATE as f32 / spike_hz.max(0.1)) as usize;
            let dur = (SYNTH_RATE as f32 * spike_dur_ms / 1000.0) as usize;
            let mut t = 0usize;
            while t < n {
                let end = (t + dur).min(n);
                for s in &mut v[t..end] {
                    *s += rng.normal() * spike_amp;
                }
                // Jitter the period so the spikes don't form perfect rhythm.
                let jitter = ((rng.next_f32() - 0.5) * period as f32 * 0.4) as i32;
                t = ((t as i32 + period as i32 + jitter).max(0) as usize).max(t + dur);
            }
            Ok((v, SYNTH_RATE))
        }
        Source::ColoredHiss {
            secs,
            amplitude,
            peak_hz,
        } => {
            // Simple resonator on white noise to add a colored peak around
            // peak_hz. Mimics receiver hiss with passband shape.
            let n = (SYNTH_RATE as f32 * secs) as usize;
            let mut rng = SmallRng::new(0xFEED);
            let r = 0.97_f32; // pole radius => ~Q of 30
            let theta = 2.0 * std::f32::consts::PI * peak_hz / SYNTH_RATE as f32;
            let a1 = -2.0 * r * theta.cos();
            let a2 = r * r;
            let mut y1 = 0.0_f32;
            let mut y2 = 0.0_f32;
            let mut v = Vec::with_capacity(n);
            for _ in 0..n {
                let x = rng.normal();
                let y = x - a1 * y1 - a2 * y2;
                y2 = y1;
                y1 = y;
                v.push(y * amplitude * 0.1);
            }
            Ok((v, SYNTH_RATE))
        }
        Source::SynthCw {
            text,
            wpm,
            tone_hz,
            snr_db,
            secs_padding,
        } => {
            let mut samples = synth_morse(text, *wpm, *tone_hz, SYNTH_RATE, *secs_padding);
            if let Some(snr_db) = snr_db {
                let signal_amp = 0.5_f32;
                let snr_lin = 10f32.powf(snr_db / 10.0);
                let noise_std = signal_amp / snr_lin.sqrt();
                let mut rng = SmallRng::new(0xCAFEF00D);
                for s in samples.iter_mut() {
                    *s += rng.normal() * noise_std;
                }
            }
            Ok((samples, SYNTH_RATE))
        }
    }
}

fn synth_morse(text: &str, wpm: f32, tone_hz: f32, rate: u32, secs_padding: f32) -> Vec<f32> {
    let dot_s = 1.2 / wpm;
    let r = rate as f32;
    let dot_n = (dot_s * r) as usize;
    let dah_n = dot_n * 3;
    let intra_n = dot_n;
    let inter_n = dot_n * 3;
    let word_n = dot_n * 7;

    let mut out: Vec<f32> = vec![0.0; (secs_padding * r) as usize];
    for ch in text.chars() {
        if ch == ' ' {
            out.extend(std::iter::repeat_n(0.0, word_n));
            continue;
        }
        let morse = char_to_morse(ch.to_ascii_uppercase());
        for (i, m) in morse.chars().enumerate() {
            let n = if m == '.' { dot_n } else { dah_n };
            push_tone(&mut out, n, tone_hz, rate);
            if i + 1 < morse.len() {
                out.extend(std::iter::repeat_n(0.0, intra_n));
            }
        }
        out.extend(std::iter::repeat_n(0.0, inter_n));
    }
    out.extend(std::iter::repeat_n(0.0, (secs_padding * r) as usize));
    out
}

fn push_tone(out: &mut Vec<f32>, n: usize, freq_hz: f32, rate: u32) {
    let amp = 0.5_f32;
    let ramp = (rate as usize / 200).min(n / 4);
    let phase0 = out.len() as f32;
    for i in 0..n {
        let t = (phase0 + i as f32) / rate as f32;
        let mut env = amp;
        if i < ramp && ramp > 0 {
            env *= 0.5 - 0.5 * ((std::f32::consts::PI * i as f32) / ramp as f32).cos();
        } else if i + ramp >= n && ramp > 0 {
            let j = n - 1 - i;
            env *= 0.5 - 0.5 * ((std::f32::consts::PI * j as f32) / ramp as f32).cos();
        }
        out.push(env * (2.0 * std::f32::consts::PI * freq_hz * t).sin());
    }
}

fn char_to_morse(c: char) -> &'static str {
    match c {
        'A' => ".-",
        'B' => "-...",
        'C' => "-.-.",
        'D' => "-..",
        'E' => ".",
        'F' => "..-.",
        'G' => "--.",
        'H' => "....",
        'I' => "..",
        'J' => ".---",
        'K' => "-.-",
        'L' => ".-..",
        'M' => "--",
        'N' => "-.",
        'O' => "---",
        'P' => ".--.",
        'Q' => "--.-",
        'R' => ".-.",
        'S' => "...",
        'T' => "-",
        'U' => "..-",
        'V' => "...-",
        'W' => ".--",
        'X' => "-..-",
        'Y' => "-.--",
        'Z' => "--..",
        '0' => "-----",
        '1' => ".----",
        '2' => "..---",
        '3' => "...--",
        '4' => "....-",
        '5' => ".....",
        '6' => "-....",
        '7' => "--...",
        '8' => "---..",
        '9' => "----.",
        _ => "",
    }
}

fn char_error_rate(reference: &str, hypothesis: &str) -> f32 {
    let r: Vec<char> = reference.chars().collect();
    let h: Vec<char> = hypothesis.chars().collect();
    if r.is_empty() {
        return if h.is_empty() { 0.0 } else { 1.0 };
    }
    let mut prev: Vec<usize> = (0..=h.len()).collect();
    let mut cur = vec![0usize; h.len() + 1];
    for (i, rc) in r.iter().enumerate() {
        cur[0] = i + 1;
        for (j, hc) in h.iter().enumerate() {
            let cost = if rc == hc { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[h.len()] as f32 / r.len() as f32
}

fn print_case(case: &TestCase, m: &Metrics) {
    let mark = if m.pass { "PASS" } else { "FAIL" };
    let pitch = m
        .pitch_hz
        .map(|p| format!("{p:>6.1}Hz"))
        .unwrap_or_else(|| "    --".into());
    let wpm = m
        .wpm_last
        .map(|w| format!("{w:>4.1}"))
        .unwrap_or_else(|| "  --".into());
    let cer = m
        .cer
        .map(|c| format!("cer={c:.2}"))
        .unwrap_or_else(|| "        ".into());
    println!(
        "{:<22} {} {:>5}c  cpm={:>5.1}  wpm={}  {} {}",
        case.name, mark, m.char_count, m.chars_per_minute, wpm, pitch, cer
    );
    if !m.decoded.is_empty() {
        let preview: String = m.decoded.chars().take(80).collect();
        println!("    > {preview}");
    }
    if !m.notes.is_empty() {
        println!("    ! {}", m.notes);
    }
}

// --- Tiny deterministic PRNG (avoid pulling in `rand` for one helper) ---
struct SmallRng {
    state: u64,
}
impl SmallRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_mul(0xDEADBEEF).wrapping_add(1),
        }
    }
    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn next_f32(&mut self) -> f32 {
        ((self.next_u64() >> 8) as f32) / ((1u64 << 56) as f32)
    }
    fn normal(&mut self) -> f32 {
        let u1 = self.next_f32().clamp(1e-7, 1.0 - 1e-7);
        let u2 = self.next_f32();
        let r = (-2.0_f32 * u1.ln()).sqrt();
        let theta = 2.0 * std::f32::consts::PI * u2;
        r * theta.cos()
    }
}

#[cfg(test)]
mod label_eval_tests {
    use super::{classify_failure, extract_transcript_delta};

    #[test]
    fn extract_transcript_delta_removes_prefix_tokens() {
        assert_eq!(
            extract_transcript_delta("QST QST", "QST QST QST DE W1AW"),
            "QST DE W1AW"
        );
    }

    #[test]
    fn extract_transcript_delta_handles_overlap() {
        assert_eq!(
            extract_transcript_delta("QST QST QST", "QST DE W1AW"),
            "DE W1AW"
        );
    }

    #[test]
    fn classify_failure_detects_spacing_only() {
        assert_eq!(
            classify_failure("N6ZO 5NN 5", "N6ZO5NN5", 2, 0.2),
            "spacing_only_error"
        );
    }

    #[test]
    fn classify_failure_detects_leading_edge() {
        assert_eq!(
            classify_failure("QST QST QST", "TUST QST QST", 1, 0.09),
            "leading_edge_error"
        );
    }
}

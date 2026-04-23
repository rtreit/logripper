use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use cw_decoder_poc::{
    audio, decoder, ditdah_streaming, harvest, json, log_capture, preview, streaming, tui,
};

#[derive(Parser, Debug)]
#[command(
    name = "cw-decoder",
    about = "QsoRipper CW PoC: ditdah-based decoder + live WPM"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Decode a file (mp3, wav, m4a, ...) once and print the result.
    File {
        /// Path to the audio file.
        path: PathBuf,

        /// Echo all ditdah log messages to stderr.
        #[arg(long)]
        verbose: bool,

        /// Run a sliding-window decode and print per-window WPM, instead of
        /// a single whole-file pass.
        #[arg(long)]
        sliding: bool,

        /// Window length in seconds for sliding mode.
        #[arg(long, default_value_t = 6.0)]
        window: f32,

        /// Hop length in seconds for sliding mode.
        #[arg(long, default_value_t = 3.0)]
        hop: f32,
    },

    /// List available input audio devices.
    Devices,

    /// Live capture + TUI from a USB Audio Codec / soundcard input.
    Live {
        /// Substring matched against device names (case-insensitive). When
        /// omitted, the host default input device is used. Try
        /// `--device "USB Audio CODEC"`.
        #[arg(long)]
        device: Option<String>,

        /// Rolling buffer length in seconds (also the max window passed to
        /// ditdah for each decode).
        #[arg(long, default_value_t = 8.0)]
        window: f32,
    },

    /// Stream a file through the streaming decoder, printing events as they fire.
    StreamFile {
        path: PathBuf,
        /// Chunk size in milliseconds when feeding the decoder.
        #[arg(long, default_value_t = 50)]
        chunk_ms: u32,
        /// If true, sleep between chunks to simulate real-time playback.
        #[arg(long)]
        realtime: bool,
        /// Suppress per-character event lines and only print the final transcript.
        #[arg(long)]
        quiet: bool,
        /// Emit one JSON object per event to stdout (for the Avalonia GUI bridge).
        #[arg(long)]
        json: bool,
        /// Initial minimum tone-vs-noise ratio in dB (operator sensitivity).
        #[arg(long, default_value_t = streaming::DEFAULT_MIN_SNR_DB)]
        min_snr_db: f32,
        /// Initial pitch-lock confidence in dB (peak-vs-median ratio).
        #[arg(long, default_value_t = streaming::DEFAULT_PITCH_MIN_SNR_DB)]
        pitch_min_snr_db: f32,
        /// Initial threshold scale (>1 = less sensitive amplitude gate).
        #[arg(long, default_value_t = streaming::DEFAULT_THRESHOLD_SCALE)]
        threshold_scale: f32,
        /// Disable auto threshold tuning. By default the decoder picks the
        /// scale dynamically from the running SNR margin so it follows
        /// QSB. Pass this to honour `--threshold-scale` verbatim instead.
        #[arg(long)]
        no_auto_threshold: bool,
        /// Read NDJSON config-update lines from stdin while streaming.
        /// Each line: {"type":"config","min_snr_db":...,"pitch_min_snr_db":...,"threshold_scale":...}
        #[arg(long)]
        stdin_control: bool,
    },

    /// Stream a file causally by repeatedly re-running whole-window ditdah.
    StreamFileDitdah {
        path: PathBuf,
        /// Chunk size in milliseconds for causal playback.
        #[arg(long, default_value_t = 50)]
        chunk_ms: u32,
        /// Maximum decode history window in seconds.
        #[arg(long, default_value_t = 20.0)]
        window: f32,
        /// Minimum buffered audio before the first decode.
        #[arg(long, default_value_t = 4.0)]
        min_window: f32,
        /// How often to re-run ditdah on the current rolling buffer.
        #[arg(long, default_value_t = 1000)]
        decode_every_ms: u32,
        /// Number of repeated snapshots that must agree before committing text.
        #[arg(long, default_value_t = 3)]
        confirmations: usize,
        /// If true, sleep between chunks to simulate real-time playback.
        #[arg(long)]
        realtime: bool,
        /// Suppress per-snapshot lines and only print the final transcript.
        #[arg(long)]
        quiet: bool,
        /// Emit newline-delimited JSON events for the GUI bridge.
        #[arg(long)]
        json: bool,
    },

    /// Stream live audio through the causal ditdah baseline.
    StreamLiveDitdah {
        #[arg(long)]
        device: Option<String>,
        /// How long to capture before exiting (seconds). 0 = run forever.
        #[arg(long, default_value_t = 0.0)]
        seconds: f32,
        /// Chunk size in milliseconds for internal polling.
        #[arg(long, default_value_t = 50)]
        chunk_ms: u32,
        /// Maximum decode history window in seconds.
        #[arg(long, default_value_t = 20.0)]
        window: f32,
        /// Minimum buffered audio before the first decode.
        #[arg(long, default_value_t = 0.5)]
        min_window: f32,
        /// How often to re-run ditdah on the current rolling buffer.
        #[arg(long, default_value_t = 1000)]
        decode_every_ms: u32,
        /// Number of repeated snapshots that must agree before committing text.
        #[arg(long, default_value_t = 3)]
        confirmations: usize,
        /// Optional WAV path to mirror raw mono samples to (16-bit PCM at the
        /// device's native sample rate). Useful for post-stop offline analysis.
        #[arg(long)]
        record: Option<PathBuf>,
        /// Emit newline-delimited JSON events for the GUI bridge.
        #[arg(long)]
        json: bool,
    },

    /// Scan a file in overlapping windows and surface candidate "golden copy"
    /// spans by comparing baseline ditdah output with the streaming decoder.
    HarvestFile {
        path: PathBuf,
        /// Window length in seconds.
        #[arg(long, default_value_t = 4.0)]
        window: f32,
        /// Hop length in seconds.
        #[arg(long, default_value_t = 1.0)]
        hop: f32,
        /// Streaming chunk size in milliseconds for the comparison pass.
        #[arg(long, default_value_t = 50)]
        chunk_ms: u32,
        /// Maximum number of candidate windows to print.
        #[arg(long, default_value_t = 12)]
        top: usize,
        /// Minimum shared compact characters between offline and streaming
        /// outputs when no needles are supplied.
        #[arg(long, default_value_t = 4)]
        min_shared_chars: usize,
        /// Anchor string to hunt for (repeatable). Matches are compacted to
        /// uppercase alphanumerics, so `K5ZD 5NN` matches `K5ZD5NN`.
        #[arg(long = "needle")]
        needles: Vec<String>,
        /// Emit machine-readable JSON instead of human text.
        #[arg(long)]
        json: bool,
        /// Initial minimum tone-vs-noise ratio in dB for the streaming path.
        #[arg(long, default_value_t = streaming::DEFAULT_MIN_SNR_DB)]
        min_snr_db: f32,
        /// Initial pitch-lock confidence in dB for the streaming path.
        #[arg(long, default_value_t = streaming::DEFAULT_PITCH_MIN_SNR_DB)]
        pitch_min_snr_db: f32,
        /// Initial threshold scale for the streaming path.
        #[arg(long, default_value_t = streaming::DEFAULT_THRESHOLD_SCALE)]
        threshold_scale: f32,
        /// Disable auto threshold tuning for the streaming pass.
        #[arg(long)]
        no_auto_threshold: bool,
    },

    /// Render a slowed WAV preview for a specific file window so a human can
    /// listen and enter verified copy.
    PreviewWindow {
        path: PathBuf,
        /// Window start in seconds.
        #[arg(long)]
        start: f32,
        /// Window length in seconds.
        #[arg(long, default_value_t = 4.0)]
        window: f32,
        /// Slowdown factor (>1 = slower / longer).
        #[arg(long, default_value_t = 2.5)]
        slowdown: f32,
        /// Leading/trailing silence padding in milliseconds.
        #[arg(long, default_value_t = 150)]
        padding_ms: u32,
        /// Output WAV path.
        #[arg(long)]
        output: PathBuf,
    },

    /// Build a tone-energy profile around a candidate region for the labeling UI.
    ProfileWindow {
        path: PathBuf,
        /// Selected region start in seconds.
        #[arg(long)]
        start: f32,
        /// Selected region end in seconds.
        #[arg(long)]
        end: f32,
        /// Tone to inspect.
        #[arg(long)]
        pitch_hz: f32,
        /// Optional WPM estimate used for pause suggestions.
        #[arg(long)]
        wpm: Option<f32>,
    },

    /// Stream live audio through the streaming decoder, printing events to stdout.
    StreamLive {
        #[arg(long)]
        device: Option<String>,
        /// How long to capture before exiting (seconds). 0 = run forever.
        #[arg(long, default_value_t = 0.0)]
        seconds: f32,
        /// Emit one JSON object per event to stdout (for the Avalonia GUI bridge).
        #[arg(long)]
        json: bool,
        /// Initial minimum tone-vs-noise ratio in dB (operator sensitivity).
        #[arg(long, default_value_t = streaming::DEFAULT_MIN_SNR_DB)]
        min_snr_db: f32,
        /// Initial pitch-lock confidence in dB.
        #[arg(long, default_value_t = streaming::DEFAULT_PITCH_MIN_SNR_DB)]
        pitch_min_snr_db: f32,
        /// Initial threshold scale.
        #[arg(long, default_value_t = streaming::DEFAULT_THRESHOLD_SCALE)]
        threshold_scale: f32,
        /// Disable auto threshold tuning. By default the decoder follows
        /// QSB by adapting the scale to the running SNR margin.
        #[arg(long)]
        no_auto_threshold: bool,
        /// Optional WAV path to mirror raw mono samples to (16-bit PCM at the
        /// device's native sample rate). Useful for post-stop offline analysis.
        #[arg(long)]
        record: Option<PathBuf>,
        /// Read NDJSON config-update lines from stdin while streaming.
        #[arg(long)]
        stdin_control: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Always install the log capture so we can read ditdah's WPM/pitch lines.
    let echo = matches!(cli.cmd, Cmd::File { verbose: true, .. });
    let log_capture = log_capture::DitdahLogCapture::new(echo);
    log_capture.clone().install().ok();

    match cli.cmd {
        Cmd::File {
            path,
            verbose: _,
            sliding,
            window,
            hop,
        } => run_file(&path, sliding, window, hop, &log_capture),
        Cmd::Devices => {
            let names = audio::list_input_devices().context("listing devices")?;
            if names.is_empty() {
                println!("(no input devices found)");
            } else {
                println!("Input devices:");
                for n in names {
                    println!("  - {n}");
                }
            }
            Ok(())
        }
        Cmd::Live { device, window } => {
            let capture = audio::open_input(device.as_deref(), window)
                .context("opening live audio device")?;
            tui::run(capture, log_capture)
        }
        Cmd::StreamFile {
            path,
            chunk_ms,
            realtime,
            quiet,
            json,
            min_snr_db,
            pitch_min_snr_db,
            threshold_scale,
            no_auto_threshold,
            stdin_control,
        } => {
            let cfg = streaming::DecoderConfig {
                min_snr_db,
                pitch_min_snr_db,
                threshold_scale,
                auto_threshold: !no_auto_threshold,
            };
            run_stream_file(&path, chunk_ms, realtime, quiet, json, cfg, stdin_control)
        }
        Cmd::StreamFileDitdah {
            path,
            chunk_ms,
            window,
            min_window,
            decode_every_ms,
            confirmations,
            realtime,
            quiet,
            json,
        } => run_stream_file_ditdah(
            &path,
            chunk_ms,
            window,
            min_window,
            decode_every_ms,
            confirmations,
            realtime,
            quiet,
            json,
            &log_capture,
        ),
        Cmd::StreamLiveDitdah {
            device,
            seconds,
            chunk_ms,
            window,
            min_window,
            decode_every_ms,
            confirmations,
            record,
            json,
        } => run_stream_live_ditdah(
            device.as_deref(),
            seconds,
            chunk_ms,
            window,
            min_window,
            decode_every_ms,
            confirmations,
            record.as_deref(),
            json,
            &log_capture,
        ),
        Cmd::HarvestFile {
            path,
            window,
            hop,
            chunk_ms,
            top,
            min_shared_chars,
            needles,
            json,
            min_snr_db,
            pitch_min_snr_db,
            threshold_scale,
            no_auto_threshold,
        } => {
            let stream_cfg = streaming::DecoderConfig {
                min_snr_db,
                pitch_min_snr_db,
                threshold_scale,
                auto_threshold: !no_auto_threshold,
            };
            let harvest_cfg = harvest::HarvestConfig {
                window_seconds: window,
                hop_seconds: hop,
                chunk_ms,
                top,
                min_shared_chars,
                needles,
            };
            run_harvest_file(&path, harvest_cfg, stream_cfg, json, &log_capture)
        }
        Cmd::PreviewWindow {
            path,
            start,
            window,
            slowdown,
            padding_ms,
            output,
        } => run_preview_window(&path, start, window, slowdown, padding_ms, &output),
        Cmd::ProfileWindow {
            path,
            start,
            end,
            pitch_hz,
            wpm,
        } => run_profile_window(&path, start, end, pitch_hz, wpm),
        Cmd::StreamLive {
            device,
            seconds,
            json,
            min_snr_db,
            pitch_min_snr_db,
            threshold_scale,
            no_auto_threshold,
            record,
            stdin_control,
        } => {
            let cfg = streaming::DecoderConfig {
                min_snr_db,
                pitch_min_snr_db,
                threshold_scale,
                auto_threshold: !no_auto_threshold,
            };
            run_stream_live(
                device.as_deref(),
                seconds,
                json,
                cfg,
                record.as_deref(),
                stdin_control,
            )
        }
    }
}

fn run_file(
    path: &std::path::Path,
    sliding: bool,
    window: f32,
    hop: f32,
    log_capture: &log_capture::DitdahLogCapture,
) -> Result<()> {
    println!("Decoding: {}", path.display());
    let audio = audio::decode_file(path).context("decoding audio file")?;
    let dur = audio.samples.len() as f32 / audio.sample_rate as f32;
    println!(
        "  sample_rate = {} Hz, duration = {:.2} s, samples = {}",
        audio.sample_rate,
        dur,
        audio.samples.len()
    );

    if !sliding {
        let out = decoder::decode_window(&audio.samples, audio.sample_rate, log_capture)?;
        let stats = out.stats;
        println!();
        println!("== ditdah stats ==");
        println!(
            "  pitch:     {}",
            stats
                .pitch_hz
                .map(|p| format!("{p:.1} Hz"))
                .unwrap_or_else(|| "(unknown)".into())
        );
        println!(
            "  WPM:       {}",
            stats
                .wpm
                .map(|w| format!("{w:.1}"))
                .unwrap_or_else(|| "(unknown)".into())
        );
        println!(
            "  threshold: {}",
            stats
                .threshold
                .map(|t| format!("{t:.4e}"))
                .unwrap_or_else(|| "(unknown)".into())
        );
        println!();
        println!("== decoded text ==");
        println!("{}", out.text);
        return Ok(());
    }

    // Sliding-window mode: report per-window WPM + decoded text.
    let win_samples = ((window * audio.sample_rate as f32) as usize).max(1);
    let hop_samples = ((hop * audio.sample_rate as f32) as usize).max(1);
    println!(
        "Sliding window: {:.1}s window, {:.1}s hop ({} samples / {} samples)",
        window, hop, win_samples, hop_samples
    );
    println!("{:>7}  {:>6}  {:>7}  text", "t (s)", "WPM", "pitch");
    println!("{}", "-".repeat(60));

    let mut start = 0usize;
    while start + win_samples <= audio.samples.len() {
        let end = start + win_samples;
        let slice = &audio.samples[start..end];
        let out = decoder::decode_window(slice, audio.sample_rate, log_capture)?;
        let t = start as f32 / audio.sample_rate as f32;
        let wpm = out
            .stats
            .wpm
            .map(|w| format!("{w:>5.1}"))
            .unwrap_or_else(|| "  -- ".into());
        let pitch = out
            .stats
            .pitch_hz
            .map(|p| format!("{p:>5.0}Hz"))
            .unwrap_or_else(|| "    --".into());
        let text = out.text.replace('\n', " ");
        println!("{:>7.2}  {}  {}  {}", t, wpm, pitch, text);
        start += hop_samples;
    }

    Ok(())
}

fn run_harvest_file(
    path: &std::path::Path,
    harvest_cfg: harvest::HarvestConfig,
    stream_cfg: streaming::DecoderConfig,
    json: bool,
    log_capture: &log_capture::DitdahLogCapture,
) -> Result<()> {
    let audio = audio::decode_file(path).context("decoding audio file")?;
    let dur = audio.samples.len() as f32 / audio.sample_rate as f32;
    let candidates = harvest::harvest_candidates_with_progress(
        &audio.samples,
        audio.sample_rate,
        log_capture,
        stream_cfg,
        &harvest_cfg,
        |completed, total, start_s, end_s| {
            if json {
                eprintln!(
                    "HARVEST_PROGRESS\t{}\t{}\t{:.3}\t{:.3}",
                    completed, total, start_s, end_s
                );
            }
        },
    )?;

    if json {
        let payload = serde_json::json!({
            "path": path.display().to_string(),
            "sample_rate": audio.sample_rate,
            "duration_s": dur,
            "window_s": harvest_cfg.window_seconds,
            "hop_s": harvest_cfg.hop_seconds,
            "chunk_ms": harvest_cfg.chunk_ms,
            "needles": harvest_cfg.needles,
            "candidates": candidates.iter().map(|c| serde_json::json!({
                "start_s": c.start_s,
                "end_s": c.end_s,
                "shared_chars": c.shared_chars,
                "strongest_copy_len": c.strongest_copy_len,
                "matched_needles": c.matched_needles,
                "offline": {
                    "text": c.offline_text,
                    "pitch_hz": c.offline_pitch_hz,
                    "wpm": c.offline_wpm,
                },
                "stream": {
                    "text": c.stream_text,
                    "pitch_hz": c.stream_pitch_hz,
                    "wpm": c.stream_wpm,
                    "threshold": c.stream_threshold,
                },
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    println!(
        "Harvest: {} ({} Hz, {:.2} s)",
        path.display(),
        audio.sample_rate,
        dur
    );
    println!(
        "Window {:.1}s / hop {:.1}s / chunk {}ms / top {}",
        harvest_cfg.window_seconds, harvest_cfg.hop_seconds, harvest_cfg.chunk_ms, harvest_cfg.top
    );
    if harvest_cfg.needles.is_empty() {
        println!(
            "Mode: agreement harvest (min shared compact chars = {})",
            harvest_cfg.min_shared_chars
        );
    } else {
        println!("Needles: {}", harvest_cfg.needles.join(", "));
    }
    println!("{}", "=".repeat(96));

    if candidates.is_empty() {
        println!("No candidate windows matched.");
        return Ok(());
    }

    for c in candidates {
        let offline_pitch = c
            .offline_pitch_hz
            .map(|v| format!("{v:.1}Hz"))
            .unwrap_or_else(|| "--".to_string());
        let offline_wpm = c
            .offline_wpm
            .map(|v| format!("{v:.1}"))
            .unwrap_or_else(|| "--".to_string());
        let stream_pitch = c
            .stream_pitch_hz
            .map(|v| format!("{v:.1}Hz"))
            .unwrap_or_else(|| "--".to_string());
        let stream_wpm = c
            .stream_wpm
            .map(|v| format!("{v:.1}"))
            .unwrap_or_else(|| "--".to_string());
        let needles = if c.matched_needles.is_empty() {
            "-".to_string()
        } else {
            c.matched_needles.join(",")
        };

        println!(
            "[{:>6.2}-{:<6.2}] shared={} best={} needles={}",
            c.start_s, c.end_s, c.shared_chars, c.strongest_copy_len, needles
        );
        println!(
            "  offline  pitch={} wpm={}  {}",
            offline_pitch, offline_wpm, c.offline_text
        );
        println!(
            "  stream   pitch={} wpm={} thr={:.4e}  {}",
            stream_pitch, stream_wpm, c.stream_threshold, c.stream_text
        );
        println!();
    }

    Ok(())
}

fn run_preview_window(
    path: &std::path::Path,
    start: f32,
    window: f32,
    slowdown: f32,
    padding_ms: u32,
    output: &std::path::Path,
) -> Result<()> {
    preview::render_preview_wav(path, start, window, slowdown, padding_ms, output)
}

fn run_profile_window(
    path: &std::path::Path,
    start: f32,
    end: f32,
    pitch_hz: f32,
    wpm: Option<f32>,
) -> Result<()> {
    let audio = audio::decode_file(path).context("decoding audio file")?;
    let profile =
        harvest::build_signal_profile(&audio.samples, audio.sample_rate, start, end, pitch_hz, wpm)
            .context("building signal profile")?;
    let payload = serde_json::json!({
        "path": path.display().to_string(),
        "sample_rate": audio.sample_rate,
        "display_start_s": profile.display_start_s,
        "display_end_s": profile.display_end_s,
        "selection_start_s": profile.selection_start_s,
        "selection_end_s": profile.selection_end_s,
        "suggested_start_s": profile.suggested_start_s,
        "suggested_end_s": profile.suggested_end_s,
        "pitch_hz": profile.pitch_hz,
        "threshold": profile.threshold,
        "frame_step_s": profile.frame_step_s,
        "frame_len_s": profile.frame_len_s,
        "points": profile.points.iter().map(|p| serde_json::json!({
            "time_s": p.time_s,
            "power": p.power,
            "active": p.active,
        })).collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string(&payload)?);
    Ok(())
}

fn run_stream_file(
    path: &std::path::Path,
    chunk_ms: u32,
    realtime: bool,
    quiet: bool,
    json: bool,
    cfg: streaming::DecoderConfig,
    stdin_control: bool,
) -> Result<()> {
    use std::time::Instant;
    let audio = audio::decode_file(path).context("decoding audio file")?;
    let dur = audio.samples.len() as f32 / audio.sample_rate as f32;
    let mut emitter = if json {
        Some(json::JsonEmitter::new())
    } else {
        None
    };
    if let Some(em) = emitter.as_mut() {
        em.emit(
            0.0,
            serde_json::json!({
                "type": "ready",
                "source": "file",
                "path": path.display().to_string(),
                "rate": audio.sample_rate,
                "duration": dur,
                "config": serde_json::json!({
                    "min_snr_db": cfg.min_snr_db,
                    "pitch_min_snr_db": cfg.pitch_min_snr_db,
                    "threshold_scale": cfg.threshold_scale,
                    "auto_threshold": cfg.auto_threshold,
                }),
            }),
        );
    } else {
        println!(
            "Streaming: {} ({} Hz, {:.2} s, {} samples)",
            path.display(),
            audio.sample_rate,
            dur,
            audio.samples.len()
        );
    }

    let mut decoder = streaming::StreamingDecoder::new(audio.sample_rate)?;
    decoder.set_config(cfg);
    let cfg_channel = stdin_control.then(|| spawn_stdin_config_channel(None));
    let chunk_samples = ((audio.sample_rate as u64 * chunk_ms as u64) / 1000) as usize;
    let chunk_samples = chunk_samples.max(64);

    let mut transcript = String::new();
    let mut last_wpm: Option<f32> = None;
    let started = Instant::now();
    let mut consumed: usize = 0;

    for chunk in audio.samples.chunks(chunk_samples) {
        let t_in_audio = consumed as f32 / audio.sample_rate as f32;
        consumed += chunk.len();

        if let Some(rx) = cfg_channel.as_ref() {
            let mut latest: Option<streaming::DecoderConfig> = None;
            while let Ok(c) = rx.try_recv() {
                latest = Some(c);
            }
            if let Some(c) = latest {
                decoder.set_config(c);
                if let Some(em) = emitter.as_mut() {
                    em.emit(
                        t_in_audio,
                        serde_json::json!({
                            "type": "config_ack",
                            "min_snr_db": c.min_snr_db,
                            "pitch_min_snr_db": c.pitch_min_snr_db,
                            "threshold_scale": c.threshold_scale,
                            "auto_threshold": c.auto_threshold,
                        }),
                    );
                }
            }
        }

        let events = decoder.feed(chunk)?;
        let now_real = started.elapsed().as_secs_f32();
        let lag_ms = ((now_real - t_in_audio) * 1000.0) as i32;

        for ev in events {
            if let Some(em) = emitter.as_mut() {
                em.emit_event(t_in_audio, &ev);
                // Keep the transcript locally so the closing summary still works.
                match &ev {
                    streaming::StreamEvent::Char { ch, .. } => transcript.push(*ch),
                    streaming::StreamEvent::Word => transcript.push(' '),
                    streaming::StreamEvent::Garbled { .. } => transcript.push('?'),
                    _ => {}
                }
                continue;
            }
            match ev {
                streaming::StreamEvent::PitchUpdate { pitch_hz } => {
                    if !quiet {
                        println!(
                            "[t={:>6.2}s real+{:>4}ms] PITCH lock: {:.1} Hz",
                            t_in_audio, lag_ms, pitch_hz
                        );
                    }
                }
                streaming::StreamEvent::WpmUpdate { wpm } => {
                    let changed = last_wpm.map(|w| (w - wpm).abs() >= 1.0).unwrap_or(true);
                    if changed {
                        if !quiet {
                            println!(
                                "[t={:>6.2}s real+{:>4}ms] WPM    -> {:.1}",
                                t_in_audio, lag_ms, wpm
                            );
                        }
                        last_wpm = Some(wpm);
                    }
                }
                streaming::StreamEvent::Char { ch, morse } => {
                    transcript.push(ch);
                    if !quiet {
                        println!(
                            "[t={:>6.2}s real+{:>4}ms] CHAR  '{}' ({:>5})  transcript: {}",
                            t_in_audio, lag_ms, ch, morse, transcript
                        );
                    }
                }
                streaming::StreamEvent::Word => {
                    transcript.push(' ');
                    if !quiet {
                        println!("[t={:>6.2}s real+{:>4}ms] WORD  break", t_in_audio, lag_ms);
                    }
                }
                streaming::StreamEvent::Garbled { morse } => {
                    transcript.push('?');
                    if !quiet {
                        println!(
                            "[t={:>6.2}s real+{:>4}ms] ???  garbled morse: {}",
                            t_in_audio, lag_ms, morse
                        );
                    }
                }
                streaming::StreamEvent::Power { .. } => {
                    // Power events are JSON-only by default; suppress in human output.
                }
            }
        }

        if realtime {
            let target_real = t_in_audio;
            let now_real = started.elapsed().as_secs_f32();
            if now_real < target_real {
                std::thread::sleep(std::time::Duration::from_secs_f32(target_real - now_real));
            }
        }
    }

    // Flush any pending letter.
    for ev in decoder.flush() {
        if let Some(em) = emitter.as_mut() {
            em.emit_event(consumed as f32 / audio.sample_rate as f32, &ev);
        }
        match ev {
            streaming::StreamEvent::Char { ch, .. } => transcript.push(ch),
            streaming::StreamEvent::Garbled { .. } => transcript.push('?'),
            _ => {}
        }
    }

    if let Some(em) = emitter.as_mut() {
        em.emit(
            consumed as f32 / audio.sample_rate as f32,
            serde_json::json!({
                "type": "end",
                "transcript": transcript.trim(),
                "wpm": decoder.current_wpm(),
                "pitch": decoder.pitch(),
            }),
        );
        return Ok(());
    }

    println!();
    println!(
        "Final pitch:    {}",
        decoder
            .pitch()
            .map(|p| format!("{p:.1} Hz"))
            .unwrap_or_else(|| "(none)".into())
    );
    println!(
        "Final WPM:      {}",
        decoder
            .current_wpm()
            .map(|w| format!("{w:.1}"))
            .unwrap_or_else(|| "(none)".into())
    );
    println!("Threshold:      {:.4e}", decoder.current_threshold());
    println!();
    println!("Transcript:");
    println!("{}", transcript.trim());
    Ok(())
}

fn run_stream_file_ditdah(
    path: &std::path::Path,
    chunk_ms: u32,
    window_seconds: f32,
    min_window_seconds: f32,
    decode_every_ms: u32,
    required_confirmations: usize,
    realtime: bool,
    quiet: bool,
    json: bool,
    log_capture: &log_capture::DitdahLogCapture,
) -> Result<()> {
    use std::time::Instant;

    let audio = audio::decode_file(path).context("decoding audio file")?;
    let dur = audio.samples.len() as f32 / audio.sample_rate as f32;
    let chunk_samples = (((audio.sample_rate as u64) * chunk_ms as u64) / 1000) as usize;
    let chunk_samples = chunk_samples.max(64);
    let baseline_cfg = ditdah_streaming::CausalBaselineConfig {
        window_seconds,
        min_window_seconds: min_window_seconds.clamp(0.1, window_seconds.max(0.5)),
        decode_every_ms,
        required_confirmations,
    };
    let mut streamer =
        ditdah_streaming::CausalBaselineStreamer::new(audio.sample_rate, baseline_cfg);
    let mut emitter = if json {
        Some(json::JsonEmitter::new())
    } else {
        None
    };

    if let Some(em) = emitter.as_mut() {
        em.emit(
            0.0,
            serde_json::json!({
                "type": "ready",
                "source": "file-baseline",
                "path": path.display().to_string(),
                "rate": audio.sample_rate,
                "duration": dur,
                "config": {
                    "window_seconds": baseline_cfg.window_seconds,
                    "min_window_seconds": baseline_cfg.min_window_seconds,
                    "decode_every_ms": baseline_cfg.decode_every_ms,
                    "required_confirmations": baseline_cfg.required_confirmations,
                },
            }),
        );
    } else if !quiet {
        println!(
            "Rolling ditdah streaming: {} ({} Hz, {:.2} s, {} samples)",
            path.display(),
            audio.sample_rate,
            dur,
            audio.samples.len()
        );
        println!(
            "Window {:.1}s / min {:.1}s / decode every {}ms / chunk {}ms",
            window_seconds, min_window_seconds, decode_every_ms, chunk_ms
        );
    }

    let started = Instant::now();
    let mut last_stats: Option<log_capture::DitdahStats> = None;
    let mut consumed = 0usize;
    let mut decode_count = 0usize;
    let mut last_emitted_pitch: Option<f32> = None;
    let mut last_emitted_wpm: Option<f32> = None;

    for chunk in audio.samples.chunks(chunk_samples) {
        let t_in_audio = consumed as f32 / audio.sample_rate as f32;
        consumed += chunk.len();
        let snapshots = streamer.feed(chunk);
        if !snapshots.is_empty() {
            decode_count += 1;
            let window_samples = streamer.window_snapshot();
            let out = decoder::decode_window(&window_samples, audio.sample_rate, log_capture)?;
            last_stats = Some(out.stats);
            for snapshot in snapshots {
                handle_baseline_snapshot(
                    &snapshot,
                    snapshot.end_sample as f32 / audio.sample_rate as f32,
                    started.elapsed().as_secs_f32(),
                    decode_count,
                    quiet,
                    emitter.as_mut(),
                    out.stats,
                    &mut last_emitted_pitch,
                    &mut last_emitted_wpm,
                );
            }
        }

        if realtime {
            let target_real = t_in_audio;
            let now_real = started.elapsed().as_secs_f32();
            if now_real < target_real {
                std::thread::sleep(std::time::Duration::from_secs_f32(target_real - now_real));
            }
        }
    }

    let window_samples = streamer.window_snapshot();
    if !window_samples.is_empty() {
        let out = decoder::decode_window(&window_samples, audio.sample_rate, log_capture)?;
        last_stats = Some(out.stats);
    }
    let final_snapshots = streamer.flush();
    for snapshot in &final_snapshots {
        handle_baseline_snapshot(
            snapshot,
            snapshot.end_sample as f32 / audio.sample_rate as f32,
            started.elapsed().as_secs_f32(),
            decode_count,
            quiet,
            emitter.as_mut(),
            last_stats.unwrap_or_default(),
            &mut last_emitted_pitch,
            &mut last_emitted_wpm,
        );
    }
    let transcript = streamer.transcript().to_string();

    if let Some(em) = emitter.as_mut() {
        em.emit(
            streamer.processed_samples() as f32 / audio.sample_rate as f32,
            serde_json::json!({
                "type": "end",
                "transcript": transcript.trim(),
                "wpm": last_stats.and_then(|s| s.wpm),
                "pitch": last_stats.and_then(|s| s.pitch_hz),
            }),
        );
        return Ok(());
    }

    if !quiet {
        println!();
        println!(
            "Final pitch:    {}",
            last_stats
                .and_then(|s| s.pitch_hz)
                .map(|p| format!("{p:.1} Hz"))
                .unwrap_or_else(|| "(none)".into())
        );
        println!(
            "Final WPM:      {}",
            last_stats
                .and_then(|s| s.wpm)
                .map(|w| format!("{w:.1}"))
                .unwrap_or_else(|| "(none)".into())
        );
        println!(
            "Threshold:      {}",
            last_stats
                .and_then(|s| s.threshold)
                .map(|t| format!("{t:.4e}"))
                .unwrap_or_else(|| "(none)".into())
        );
        println!();
    }

    println!("Transcript:");
    println!("{transcript}");
    Ok(())
}

fn run_stream_live_ditdah(
    device: Option<&str>,
    seconds: f32,
    chunk_ms: u32,
    window_seconds: f32,
    min_window_seconds: f32,
    decode_every_ms: u32,
    required_confirmations: usize,
    record_path: Option<&std::path::Path>,
    json: bool,
    log_capture: &log_capture::DitdahLogCapture,
) -> Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    let capture = audio::open_input_with_recording(device, 1.0, record_path)?;
    let baseline_cfg = ditdah_streaming::CausalBaselineConfig {
        window_seconds,
        min_window_seconds: min_window_seconds.clamp(0.1, window_seconds.max(0.5)),
        decode_every_ms,
        required_confirmations,
    };
    let mut streamer =
        ditdah_streaming::CausalBaselineStreamer::new(capture.sample_rate, baseline_cfg);
    let mut emitter = if json {
        Some(json::JsonEmitter::new())
    } else {
        None
    };
    if let Some(em) = emitter.as_mut() {
        em.emit(
            0.0,
            serde_json::json!({
                "type": "ready",
                "source": "live-baseline",
                "device": capture.device_name,
                "rate": capture.sample_rate,
                "recording": capture.record_path().map(|p| p.display().to_string()),
                "config": {
                    "window_seconds": baseline_cfg.window_seconds,
                    "min_window_seconds": baseline_cfg.min_window_seconds,
                    "decode_every_ms": baseline_cfg.decode_every_ms,
                    "required_confirmations": baseline_cfg.required_confirmations,
                    "chunk_ms": chunk_ms,
                },
            }),
        );
    } else {
        println!("Live baseline streaming from: {}", capture.device_name);
    }

    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = Arc::clone(&stop);
        ctrlc_setup(move || {
            stop.store(true, Ordering::Relaxed);
        });
    }
    // Watch stdin for EOF or any byte — the GUI signals a graceful
    // shutdown by writing "stop\n" and then closing our stdin. Either a
    // successful read or EOF triggers the shutdown path so Drop runs on
    // LiveCapture and the WAV writer flushes the data chunk + RIFF header.
    // Without this, Kill leaves a header-only WAV that Replay can't read.
    //
    // NOTE: on Windows, std::io::stdin() with an anonymous pipe can fail
    // to surface EOF reliably — writing a sentinel byte from the parent
    // is the robust signal. We accept either path.
    {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = [0u8; 256];
            let mut stdin = std::io::stdin();
            let _ = stdin.read(&mut buf);
            stop.store(true, Ordering::Relaxed);
        });
    }

    let started = Instant::now();
    let mut last_drain_at: u64 = 0;
    let mut last_stats: Option<log_capture::DitdahStats> = None;
    let mut decode_count = 0usize;
    let mut last_emitted_pitch: Option<f32> = None;
    let mut last_emitted_wpm: Option<f32> = None;
    // Lightweight power meter for the GUI signal-strength bars in baseline
    // mode. We don't have a Goertzel running here (the ditdah library does
    // its own decoding internally), so approximate with chunk RMS-squared
    // as `power` and a rolling 25th-percentile as the noise/threshold.
    let mut power_history: std::collections::VecDeque<f32> =
        std::collections::VecDeque::with_capacity(128);
    let mut last_power_emit = Instant::now();

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if seconds > 0.0 && started.elapsed().as_secs_f32() >= seconds {
            break;
        }
        // Single sleep per iteration — a previous shutdown-fix accidentally
        // left two back-to-back sleeps here, doubling per-loop latency from
        // chunk_ms to 2 × chunk_ms and halving the effective drain rate.
        std::thread::sleep(Duration::from_millis(chunk_ms.max(10) as u64));
        if stop.load(Ordering::Relaxed) {
            eprintln!("[cw-decoder] main-loop: stop detected, breaking");
            break;
        }
        if seconds > 0.0 && started.elapsed().as_secs_f32() >= seconds {
            break;
        }

        let chunk = {
            let lock = capture.buffer.lock();
            let total = lock.written;
            let avail_in_ring = lock.len();
            let want = (total - last_drain_at).min(avail_in_ring as u64) as usize;
            if want == 0 {
                Vec::new()
            } else {
                let snap = lock.snapshot();
                last_drain_at = total;
                let start = snap.len() - want;
                snap[start..].to_vec()
            }
        };
        if chunk.is_empty() {
            continue;
        }

        // Update power meter from chunk RMS² and emit a power event every
        // ~50ms regardless of whether the streamer produced a snapshot —
        // this keeps the signal-strength indicator alive between decodes.
        let power = if !chunk.is_empty() {
            let sum_sq: f32 = chunk.iter().map(|s| s * s).sum();
            sum_sq / chunk.len() as f32
        } else {
            0.0
        };
        if power_history.len() == 128 {
            power_history.pop_front();
        }
        power_history.push_back(power);
        if let Some(em) = emitter.as_mut() {
            if last_power_emit.elapsed().as_millis() >= 50 {
                last_power_emit = Instant::now();
                let mut sorted: Vec<f32> = power_history.iter().copied().collect();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                // Use the 10th percentile (not Q25) so the noise floor stays
                // representative even when CW is continuously active and ~50%
                // of the history is "key down". With Q25 plus a 4× threshold
                // the keying meter would never light up on a strong signal
                // because Q25 itself sits inside the on-state distribution.
                let q10 = sorted[sorted.len() / 10];
                let noise = q10.max(1e-10);
                let threshold = noise * 4.0; // ~6 dB above noise floor
                let snr = power / noise;
                let signal = power > threshold;
                em.emit(
                    started.elapsed().as_secs_f32(),
                    serde_json::json!({
                        "type": "power",
                        "power": power,
                        "threshold": threshold,
                        "noise": noise,
                        "signal": signal,
                        "snr": snr,
                    }),
                );
            }
        }

        let snapshots = streamer.feed(&chunk);
        if snapshots.is_empty() {
            continue;
        }

        decode_count += 1;
        let window_samples = streamer.window_snapshot();
        let out = decoder::decode_window(&window_samples, capture.sample_rate, log_capture)?;
        last_stats = Some(out.stats);
        for snapshot in snapshots {
            handle_baseline_snapshot(
                &snapshot,
                snapshot.end_sample as f32 / capture.sample_rate as f32,
                started.elapsed().as_secs_f32(),
                decode_count,
                false,
                emitter.as_mut(),
                out.stats,
                &mut last_emitted_pitch,
                &mut last_emitted_wpm,
            );
        }
    }

    // Finalize the recording as early as possible on shutdown so the WAV
    // header is valid even if the GUI falls back to Kill before we reach
    // the end of this function.
    let recording_path = capture.record_path().map(|p| p.display().to_string());
    let recording_saved = capture
        .finalize_recording()
        .map(|p| p.display().to_string());

    // A final flush() runs one more whole-buffer decode. That's expensive
    // on a 20s buffer and rarely produces new committed text (the prefix
    // stabilizer has already emitted everything stable). Skip it when the
    // user stopped us — prioritize exiting quickly over squeezing out one
    // more snapshot.
    let user_initiated_stop = stop.load(Ordering::Relaxed);
    if !user_initiated_stop {
        let final_snapshots = streamer.flush();
        for snapshot in &final_snapshots {
            handle_baseline_snapshot(
                snapshot,
                snapshot.end_sample as f32 / capture.sample_rate as f32,
                started.elapsed().as_secs_f32(),
                decode_count,
                false,
                emitter.as_mut(),
                last_stats.unwrap_or_default(),
                &mut last_emitted_pitch,
                &mut last_emitted_wpm,
            );
        }
    }

    let transcript = streamer.transcript().to_string();
    if let Some(em) = emitter.as_mut() {
        em.emit(
            started.elapsed().as_secs_f32(),
            serde_json::json!({
                "type": "end",
                "transcript": transcript.trim(),
                "wpm": last_stats.and_then(|s| s.wpm),
                "pitch": last_stats.and_then(|s| s.pitch_hz),
                "recording": recording_saved.or(recording_path),
            }),
        );
        return Ok(());
    }

    println!();
    println!("Final transcript:");
    println!("{}", transcript.trim());
    Ok(())
}

fn handle_baseline_snapshot(
    snapshot: &ditdah_streaming::CausalBaselineSnapshot,
    t_in_audio: f32,
    now_real: f32,
    decode_count: usize,
    quiet: bool,
    emitter: Option<&mut json::JsonEmitter>,
    stats: log_capture::DitdahStats,
    last_emitted_pitch: &mut Option<f32>,
    last_emitted_wpm: &mut Option<f32>,
) {
    if let Some(em) = emitter {
        emit_baseline_stats(em, t_in_audio, stats, last_emitted_pitch, last_emitted_wpm);
        emit_baseline_transcript_delta(em, t_in_audio, &snapshot.appended);
        return;
    }

    if quiet {
        return;
    }

    let lag_ms = ((now_real - t_in_audio) * 1000.0) as i32;
    let pitch = stats
        .pitch_hz
        .map(|p| format!("{p:.1} Hz"))
        .unwrap_or_else(|| "(unknown)".into());
    let wpm = stats
        .wpm
        .map(|w| format!("{w:.1}"))
        .unwrap_or_else(|| "(unknown)".into());
    println!(
        "[t={:>6.2}s real+{:>4}ms] SNAP #{:>2} pitch={} wpm={} appended: {}",
        t_in_audio,
        lag_ms,
        decode_count,
        pitch,
        wpm,
        if snapshot.appended.is_empty() {
            "(none)"
        } else {
            snapshot.appended.trim_start()
        }
    );
    if !snapshot.transcript.is_empty() {
        println!("                      transcript: {}", snapshot.transcript);
    }
}

fn emit_baseline_stats(
    emitter: &mut json::JsonEmitter,
    t_in_audio: f32,
    stats: log_capture::DitdahStats,
    last_emitted_pitch: &mut Option<f32>,
    last_emitted_wpm: &mut Option<f32>,
) {
    if let Some(pitch_hz) = stats.pitch_hz {
        let changed = last_emitted_pitch
            .map(|prev| (prev - pitch_hz).abs() >= 0.5)
            .unwrap_or(true);
        if changed {
            emitter.emit(
                t_in_audio,
                serde_json::json!({ "type": "pitch", "hz": pitch_hz }),
            );
            *last_emitted_pitch = Some(pitch_hz);
        }
    }

    if let Some(wpm) = stats.wpm {
        let changed = last_emitted_wpm
            .map(|prev| (prev - wpm).abs() >= 0.5)
            .unwrap_or(true);
        if changed {
            emitter.emit(t_in_audio, serde_json::json!({ "type": "wpm", "wpm": wpm }));
            *last_emitted_wpm = Some(wpm);
        }
    }

    emitter.emit(
        t_in_audio,
        serde_json::json!({
            "type": "stats",
            "wpm": stats.wpm,
            "pitch": stats.pitch_hz,
            "threshold": stats.threshold,
        }),
    );
}

fn emit_baseline_transcript_delta(
    emitter: &mut json::JsonEmitter,
    t_in_audio: f32,
    appended: &str,
) {
    for ch in appended.chars() {
        if ch == ' ' {
            emitter.emit(t_in_audio, serde_json::json!({ "type": "word" }));
        } else {
            emitter.emit(
                t_in_audio,
                serde_json::json!({
                    "type": "char",
                    "ch": ch.to_string(),
                    "morse": "",
                }),
            );
        }
    }
}

fn run_stream_live(
    device: Option<&str>,
    seconds: f32,
    json: bool,
    cfg: streaming::DecoderConfig,
    record_path: Option<&std::path::Path>,
    stdin_control: bool,
) -> Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    let capture = audio::open_input_with_recording(device, 1.0, record_path)?;
    let mut decoder = streaming::StreamingDecoder::new(capture.sample_rate)?;
    decoder.set_config(cfg);

    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = Arc::clone(&stop);
        ctrlc_setup(move || {
            stop.store(true, Ordering::Relaxed);
        });
    }
    // GUI signals graceful shutdown by closing our stdin. We need that so
    // Drop runs on LiveCapture and hound flushes the WAV header. The
    // stdin-control config thread will set `stop` on EOF; otherwise spawn
    // a dedicated EOF watcher.
    let cfg_channel = stdin_control.then(|| spawn_stdin_config_channel(Some(Arc::clone(&stop))));
    if !stdin_control {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = [0u8; 256];
            let mut stdin = std::io::stdin();
            loop {
                match stdin.read(&mut buf) {
                    Ok(0) | Err(_) => {
                        stop.store(true, Ordering::Relaxed);
                        break;
                    }
                    Ok(_) => {}
                }
            }
        });
    }

    let mut emitter = if json {
        Some(json::JsonEmitter::new())
    } else {
        None
    };
    if let Some(em) = emitter.as_mut() {
        em.emit(
            0.0,
            serde_json::json!({
                "type": "ready",
                "source": "live",
                "device": capture.device_name,
                "rate": capture.sample_rate,
                "recording": capture.record_path().map(|p| p.display().to_string()),
                "config": serde_json::json!({
                    "min_snr_db": cfg.min_snr_db,
                    "pitch_min_snr_db": cfg.pitch_min_snr_db,
                    "threshold_scale": cfg.threshold_scale,
                    "auto_threshold": cfg.auto_threshold,
                }),
            }),
        );
    } else {
        println!("Live streaming from: {}", capture.device_name);
    }

    let started = Instant::now();
    let mut transcript = String::new();
    let mut last_wpm: Option<f32> = None;
    let mut last_drain_at: u64 = 0;

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if seconds > 0.0 && started.elapsed().as_secs_f32() >= seconds {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));

        if let Some(rx) = cfg_channel.as_ref() {
            let mut latest: Option<streaming::DecoderConfig> = None;
            while let Ok(c) = rx.try_recv() {
                latest = Some(c);
            }
            if let Some(c) = latest {
                decoder.set_config(c);
                if let Some(em) = emitter.as_mut() {
                    em.emit(
                        started.elapsed().as_secs_f32(),
                        serde_json::json!({
                            "type": "config_ack",
                            "min_snr_db": c.min_snr_db,
                            "pitch_min_snr_db": c.pitch_min_snr_db,
                            "threshold_scale": c.threshold_scale,
                            "auto_threshold": c.auto_threshold,
                        }),
                    );
                }
            }
        }

        let chunk = {
            let lock = capture.buffer.lock();
            // Drain only NEW samples since last drain.
            let total = lock.written;
            let avail_in_ring = lock.len();
            let want = (total - last_drain_at).min(avail_in_ring as u64) as usize;
            if want == 0 {
                Vec::new()
            } else {
                let snap = lock.snapshot();
                last_drain_at = total;
                let start = snap.len() - want;
                snap[start..].to_vec()
            }
        };
        if chunk.is_empty() {
            continue;
        }

        let events = decoder.feed(&chunk)?;
        let t = started.elapsed().as_secs_f32();
        for ev in events {
            if let Some(em) = emitter.as_mut() {
                em.emit_event(t, &ev);
                match &ev {
                    streaming::StreamEvent::Char { ch, .. } => transcript.push(*ch),
                    streaming::StreamEvent::Word => transcript.push(' '),
                    streaming::StreamEvent::Garbled { .. } => transcript.push('?'),
                    _ => {}
                }
                continue;
            }
            match ev {
                streaming::StreamEvent::PitchUpdate { pitch_hz } => {
                    println!("[t={t:>6.2}s] PITCH lock: {pitch_hz:.1} Hz");
                }
                streaming::StreamEvent::WpmUpdate { wpm } => {
                    let changed = last_wpm.map(|w| (w - wpm).abs() >= 1.0).unwrap_or(true);
                    if changed {
                        println!("[t={t:>6.2}s] WPM    -> {wpm:.1}");
                        last_wpm = Some(wpm);
                    }
                }
                streaming::StreamEvent::Char { ch, morse } => {
                    transcript.push(ch);
                    println!("[t={t:>6.2}s] CHAR  '{ch}' ({morse})  transcript: {transcript}");
                }
                streaming::StreamEvent::Word => {
                    transcript.push(' ');
                    println!("[t={t:>6.2}s] WORD  break");
                }
                streaming::StreamEvent::Garbled { morse } => {
                    transcript.push('?');
                    println!("[t={t:>6.2}s] ???  garbled morse: {morse}");
                }
                streaming::StreamEvent::Power { .. } => {}
            }
        }
    }

    for ev in decoder.flush() {
        if let Some(em) = emitter.as_mut() {
            em.emit_event(started.elapsed().as_secs_f32(), &ev);
        }
        if let streaming::StreamEvent::Char { ch, .. } = ev {
            transcript.push(ch);
        }
    }

    let recording_path = capture.record_path().map(|p| p.display().to_string());
    let recording_saved = capture
        .finalize_recording()
        .map(|p| p.display().to_string());
    if let Some(em) = emitter.as_mut() {
        em.emit(
            started.elapsed().as_secs_f32(),
            serde_json::json!({
                "type": "end",
                "transcript": transcript.trim(),
                "wpm": decoder.current_wpm(),
                "pitch": decoder.pitch(),
                "recording": recording_saved.or(recording_path),
            }),
        );
        return Ok(());
    }

    println!();
    println!("Final transcript:");
    println!("{}", transcript.trim());
    Ok(())
}

fn ctrlc_setup<F: FnMut() + Send + 'static>(_f: F) {
    // Best-effort: no ctrlc crate, rely on terminal interrupt for now.
}

/// Spawn a background thread that reads NDJSON config-update lines from
/// stdin and forwards parsed [`streaming::DecoderConfig`] values to the
/// returned receiver. Lines that don't parse as a config command are
/// silently ignored so unknown messages don't crash the decoder.
///
/// Wire format (one JSON object per line):
///   {"type":"config","min_snr_db":6.0,"pitch_min_snr_db":8.0,"threshold_scale":1.0}
///
/// Any field may be omitted; omitted fields keep their previous value.
fn spawn_stdin_config_channel(
    stop_on_eof: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> std::sync::mpsc::Receiver<streaming::DecoderConfig> {
    use std::io::BufRead;
    let (tx, rx) = std::sync::mpsc::channel::<streaming::DecoderConfig>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut state = streaming::DecoderConfig::defaults();
        for line in stdin.lock().lines().map_while(Result::ok) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let v: serde_json::Value = match serde_json::from_str(trimmed) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("type").and_then(|t| t.as_str()) != Some("config") {
                continue;
            }
            if let Some(x) = v.get("min_snr_db").and_then(|x| x.as_f64()) {
                state.min_snr_db = x as f32;
            }
            if let Some(x) = v.get("pitch_min_snr_db").and_then(|x| x.as_f64()) {
                state.pitch_min_snr_db = x as f32;
            }
            if let Some(x) = v.get("threshold_scale").and_then(|x| x.as_f64()) {
                state.threshold_scale = x as f32;
            }
            if let Some(b) = v.get("auto_threshold").and_then(|x| x.as_bool()) {
                state.auto_threshold = b;
            }
            if tx.send(state).is_err() {
                break;
            }
        }
        // Stdin EOF — propagate as graceful stop so Drop runs and the WAV
        // recording (if any) gets a valid header.
        if let Some(stop) = stop_on_eof {
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    });
    rx
}

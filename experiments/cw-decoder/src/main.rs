// CLI handler functions take many decoder/render parameters by design.
// Bundling them into structs would add ceremony without improving clarity.
#![allow(clippy::too_many_arguments)]

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use cw_decoder_poc::{
    audio, bench_latency, decoder, ditdah_streaming, harvest, json, log_capture, preview,
    streaming, tui,
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
    Devices {
        /// Output as JSON for programmatic consumers (the QsoRipper GUI
        /// uses this to populate the radio-monitor device drop-down).
        #[arg(long)]
        json: bool,
    },

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
        /// Experimental: constrain pitch lock to a user-selected Hz band and
        /// relock faster within that range.
        #[arg(long)]
        experimental_range_lock: bool,
        /// Lower bound for the experimental pitch-lock band.
        #[arg(long, default_value_t = streaming::DEFAULT_RANGE_LOCK_MIN_HZ)]
        range_lock_min_hz: f32,
        /// Upper bound for the experimental pitch-lock band.
        #[arg(long, default_value_t = streaming::DEFAULT_RANGE_LOCK_MAX_HZ)]
        range_lock_max_hz: f32,
        /// Minimum instantaneous adjacent-bin tone-purity ratio required
        /// to treat a power sample as "tone present." 0 disables the gate.
        /// Higher values reject impulses more aggressively at the cost of
        /// rejecting weaker tones.
        #[arg(long, default_value_t = streaming::DEFAULT_MIN_TONE_PURITY)]
        min_tone_purity: f32,
        /// Force the streaming decoder to lock to this exact pitch
        /// (Hz) instead of running pitch acquisition. 0 (the default)
        /// leaves auto-acquisition enabled. When set, the Fisher
        /// quality watchdog is disabled so the lock cannot be dropped.
        #[arg(long, default_value_t = 0.0)]
        force_pitch_hz: f32,
        /// Wide-bin sniff: number of side bins per side to add to the
        /// target Goertzel. 0 (default) = single 40-Hz-wide integration.
        /// N=2 captures ~200 Hz of bandwidth — useful for acoustically
        /// re-captured CW (speaker→mic round-trip) where speaker
        /// frequency response smears the tone across many bins.
        #[arg(long, default_value_t = 0)]
        wide_bin_count: u8,
        /// Drop on-runs shorter than this fraction of one dot length.
        /// 0 = disabled. 0.3 suppresses ghost characters in silent
        /// stretches (constant low-level noise crossing threshold).
        #[arg(long, default_value_t = 0.0)]
        min_pulse_dot_fraction: f32,
        /// Bridge off-runs shorter than this fraction of one dot length.
        /// 0 = disabled. Twin of --min-pulse-dot-fraction. 0.3 stops a
        /// dah from being fragmented into adjacent dits when the mic
        /// envelope chatters around threshold inside a key-down.
        #[arg(long, default_value_t = 0.0)]
        min_gap_dot_fraction: f32,
        /// Asymmetric hysteresis fraction on the keying threshold. 0 =
        /// disabled (single threshold, historical behaviour). When >0,
        /// the gate requires p > threshold * (1 + h/2) to flip ON and
        /// accepts p > threshold * (1 - h/2) to stay ON. Typical useful
        /// range 0.2..0.6. Stops the envelope chattering across a single
        /// threshold under dense in-band noise (#320).
        #[arg(long, default_value_t = 0.0)]
        hysteresis_fraction: f32,
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
        /// Tone to inspect. If omitted, build a broadband activity profile.
        #[arg(long)]
        pitch_hz: Option<f32>,
        /// Optional WPM estimate used for pause suggestions.
        #[arg(long)]
        wpm: Option<f32>,
    },

    /// Play an audio file through the default output device and emit progress.
    PlayFile {
        path: PathBuf,
        /// Emit newline-delimited JSON progress events for the GUI bridge.
        #[arg(long)]
        json: bool,
    },

    /// Play an audio file through the default output device AND stream
    /// the same samples through the CW decoder in lockstep. Audio output
    /// is the master clock — the decoder feeds exactly what the operator
    /// hears. Supports region trim, pause/resume, and seek over stdin.
    DecodeAndPlay {
        path: PathBuf,
        /// Region start in seconds. 0 = beginning of file.
        #[arg(long, default_value_t = 0.0)]
        start: f32,
        /// Region end in seconds. 0 (default) = end of file. Must be >
        /// `--start`. The decoder runs only over [start, end] and is
        /// reset at start so prior audio (e.g. talking before CW)
        /// cannot contaminate pitch lock or threshold history.
        #[arg(long, default_value_t = 0.0)]
        end: f32,
        /// Emit NDJSON events to stdout (decoder + playback).
        #[arg(long)]
        json: bool,
        /// Read NDJSON control commands on stdin: pause, resume, seek
        /// (position in seconds, region-relative), config (decoder).
        #[arg(long)]
        stdin_control: bool,
        /// Initial minimum tone-vs-noise ratio in dB.
        #[arg(long, default_value_t = streaming::DEFAULT_MIN_SNR_DB)]
        min_snr_db: f32,
        #[arg(long, default_value_t = streaming::DEFAULT_PITCH_MIN_SNR_DB)]
        pitch_min_snr_db: f32,
        #[arg(long, default_value_t = streaming::DEFAULT_THRESHOLD_SCALE)]
        threshold_scale: f32,
        #[arg(long)]
        no_auto_threshold: bool,
        #[arg(long)]
        experimental_range_lock: bool,
        #[arg(long, default_value_t = streaming::DEFAULT_RANGE_LOCK_MIN_HZ)]
        range_lock_min_hz: f32,
        #[arg(long, default_value_t = streaming::DEFAULT_RANGE_LOCK_MAX_HZ)]
        range_lock_max_hz: f32,
        #[arg(long, default_value_t = streaming::DEFAULT_MIN_TONE_PURITY)]
        min_tone_purity: f32,
        #[arg(long, default_value_t = 0.0)]
        force_pitch_hz: f32,
        #[arg(long, default_value_t = 0)]
        wide_bin_count: u8,
        #[arg(long, default_value_t = 0.0)]
        min_pulse_dot_fraction: f32,
        #[arg(long, default_value_t = 0.0)]
        min_gap_dot_fraction: f32,
        /// Asymmetric hysteresis fraction on the keying threshold. See
        /// stream-file --hysteresis-fraction.
        #[arg(long, default_value_t = 0.0)]
        hysteresis_fraction: f32,
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
        /// Experimental: constrain pitch lock to a user-selected Hz band and
        /// relock faster within that range.
        #[arg(long)]
        experimental_range_lock: bool,
        /// Lower bound for the experimental pitch-lock band.
        #[arg(long, default_value_t = streaming::DEFAULT_RANGE_LOCK_MIN_HZ)]
        range_lock_min_hz: f32,
        /// Upper bound for the experimental pitch-lock band.
        #[arg(long, default_value_t = streaming::DEFAULT_RANGE_LOCK_MAX_HZ)]
        range_lock_max_hz: f32,
        /// Minimum instantaneous adjacent-bin tone-purity ratio. 0 disables.
        #[arg(long, default_value_t = streaming::DEFAULT_MIN_TONE_PURITY)]
        min_tone_purity: f32,
        /// Force the streaming decoder to lock to this exact pitch
        /// (Hz) instead of running pitch acquisition. 0 (default) =
        /// auto. When set, the Fisher watchdog is disabled. Useful for
        /// live mic capture where speaker→mic acoustics shift the
        /// apparent pitch and acquisition picks the wrong tone.
        #[arg(long, default_value_t = 0.0)]
        force_pitch_hz: f32,
        /// Wide-bin sniff side count (0=off). See StreamFile docs.
        #[arg(long, default_value_t = 0)]
        wide_bin_count: u8,
        /// Drop on-runs shorter than this fraction of one dot length.
        /// 0 = disabled. 0.3 is a good mic-mode default to suppress
        /// constant-noise ghost characters in silent stretches.
        #[arg(long, default_value_t = 0.0)]
        min_pulse_dot_fraction: f32,
        /// Bridge off-runs shorter than this fraction of one dot length.
        /// 0 = disabled. Twin of --min-pulse-dot-fraction.
        #[arg(long, default_value_t = 0.0)]
        min_gap_dot_fraction: f32,
        /// Asymmetric hysteresis fraction on the keying threshold. See
        /// stream-file --hysteresis-fraction.
        #[arg(long, default_value_t = 0.0)]
        hysteresis_fraction: f32,
        /// Optional WAV path to mirror raw mono samples to (16-bit PCM at the
        /// device's native sample rate). Useful for post-stop offline analysis.
        #[arg(long)]
        record: Option<PathBuf>,
        /// Read NDJSON config-update lines from stdin while streaming.
        #[arg(long)]
        stdin_control: bool,
        /// Capture from a system OUTPUT device in WASAPI loopback mode
        /// instead of an input device. With this set, `--device` matches
        /// against output-device names. Bypasses the speaker→room→mic
        /// chain entirely — recommended for decoding YouTube / file
        /// playback. Windows-only (WASAPI host).
        #[arg(long)]
        loopback: bool,
    },
    /// Diagnostic: scan candidate pitches across an audio file and print
    /// the trial-decode Fisher score per pitch. Use this to compare
    /// faint signals vs noise and tune lock thresholds.
    ProbeFisher {
        /// Path to audio file (mp3/wav/m4a/...).
        path: PathBuf,
        /// Lowest candidate pitch (Hz).
        #[arg(long, default_value_t = 350.0)]
        min_hz: f32,
        /// Highest candidate pitch (Hz).
        #[arg(long, default_value_t = 1500.0)]
        max_hz: f32,
        /// Step between candidate pitches (Hz).
        #[arg(long, default_value_t = 10.0)]
        step_hz: f32,
        /// Only emit the top-N pitches by Fisher score.
        #[arg(long, default_value_t = 8)]
        top: usize,
    },
    /// Cold-start acquisition latency + lock-stability benchmark.
    ///
    /// Without flags, runs a deterministic synthetic scenario matrix
    /// (silence/noise/voice lead-ins, plus a long-clean-CW lock-
    /// stability stress) and prints latency + uptime metrics.
    /// With `--from-file <path> --truth <T> --cw-onset-ms <N>`,
    /// measures the same metrics on a real recording.
    BenchLatency {
        /// Real-audio scenario: path to an audio file. If omitted, the
        /// built-in synthetic suite runs instead.
        #[arg(long)]
        from_file: Option<PathBuf>,
        /// Audio-ms at which CW actually starts in `--from-file`.
        #[arg(long, default_value_t = 0)]
        cw_onset_ms: u32,
        /// Expected uppercase transcript starting at `cw_onset_ms`.
        /// Required when using `--from-file`.
        #[arg(long)]
        truth: Option<String>,
        /// Sample rate to use for the synthetic suite (only used when
        /// `--from-file` is not set).
        #[arg(long, default_value_t = 16000)]
        synth_rate: u32,
        /// Chunk size (ms) used to feed the streaming decoder. The
        /// decoder doesn't see chunk boundaries explicitly, but smaller
        /// chunks give finer-grained event timestamps.
        #[arg(long, default_value_t = 100)]
        chunk_ms: u32,
        /// Stable-N parameter: latency = first time a contiguous run of
        /// N decoded chars is also a substring of truth.
        #[arg(long, default_value_t = bench_latency::DEFAULT_STABLE_N)]
        stable_n: usize,
        /// Optional label printed alongside the result (use this to tag
        /// runs when comparing different decoder configurations).
        #[arg(long, default_value = "default")]
        label: String,
        /// Override the streaming decoder's tone-purity gate.
        #[arg(long)]
        purity: Option<f32>,
        /// Override the wide-bin sniff side count.
        #[arg(long)]
        wide_bins: Option<u8>,
        /// Disable the auto-threshold path so the IQR threshold is
        /// frozen during the run (debug aid for stability regressions).
        #[arg(long, default_value_t = false)]
        no_auto_threshold: bool,
        /// Force the decoder to a specific pitch and bypass acquisition.
        /// Use 0 to leave acquisition on.
        #[arg(long, default_value_t = 0.0)]
        force_pitch_hz: f32,
        /// Asymmetric hysteresis fraction on the keying threshold.
        /// 0 = disabled. Typical useful range 0.2..0.6. Stops the
        /// envelope from chattering across a single threshold under
        /// dense in-band noise.
        #[arg(long, default_value_t = 0.0)]
        hysteresis_fraction: f32,
        /// Bridge off-runs shorter than this fraction of one dot length.
        /// 0 = disabled. Engages the chatter-merge sanitizer (Patch 2
        /// of #320) which fuses the surrounding ON runs into a single
        /// element instead of just dropping the short OFF.
        #[arg(long, default_value_t = 0.0)]
        min_gap_dot_fraction: f32,
        /// Drop on-runs shorter than this fraction of one dot length.
        /// 0 = disabled.
        #[arg(long, default_value_t = 0.0)]
        min_pulse_dot_fraction: f32,
        /// Enable CFAR-style keying: feed the threshold detector
        /// `max(0, smoothed_target - smoothed_noise)` instead of raw
        /// smoothed target power. Targets harsh same-band noise
        /// (white-noise-bandpassed-to-CW-passband, deep tremolo on a
        /// noise bed) where the rolling-quantile threshold ends up
        /// inside the noise itself. Issue #322.
        #[arg(long, default_value_t = false)]
        cfar_keying: bool,
        /// Emit one NDJSON record per scenario in addition to the table
        /// (handy for collecting comparison runs into a file).
        #[arg(long, default_value_t = false)]
        json: bool,
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
        Cmd::Devices { json } => {
            let names = audio::list_input_devices().context("listing devices")?;
            let outs = audio::list_output_devices().context("listing output devices")?;
            if json {
                let escape = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
                let join = |items: &[String]| {
                    items
                        .iter()
                        .map(|n| format!("\"{}\"", escape(n)))
                        .collect::<Vec<_>>()
                        .join(",")
                };
                println!(
                    "{{\"inputs\":[{}],\"loopback\":[{}]}}",
                    join(&names),
                    join(&outs)
                );
            } else {
                if names.is_empty() {
                    println!("(no input devices found)");
                } else {
                    println!("Input devices:");
                    for n in names {
                        println!("  - {n}");
                    }
                }
                if !outs.is_empty() {
                    println!();
                    println!("Output devices (usable as --loopback for stream-live):");
                    for n in outs {
                        println!("  - {n}");
                    }
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
            experimental_range_lock,
            range_lock_min_hz,
            range_lock_max_hz,
            min_tone_purity,
            force_pitch_hz,
            wide_bin_count,
            min_pulse_dot_fraction,
            min_gap_dot_fraction,
            hysteresis_fraction,
            stdin_control,
        } => {
            let cfg = streaming::DecoderConfig {
                min_snr_db,
                pitch_min_snr_db,
                threshold_scale,
                auto_threshold: !no_auto_threshold,
                experimental_range_lock,
                range_lock_min_hz,
                range_lock_max_hz,
                min_tone_purity,
                force_pitch_hz: (force_pitch_hz > 0.0).then_some(force_pitch_hz),
                wide_bin_count,
                min_pulse_dot_fraction,
                min_gap_dot_fraction,
                hysteresis_fraction,

                cfar_keying: false,
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
                experimental_range_lock: false,
                range_lock_min_hz: streaming::DEFAULT_RANGE_LOCK_MIN_HZ,
                range_lock_max_hz: streaming::DEFAULT_RANGE_LOCK_MAX_HZ,
                min_tone_purity: streaming::DEFAULT_MIN_TONE_PURITY,
                force_pitch_hz: None,
                wide_bin_count: 0,
                min_pulse_dot_fraction: 0.0,
                min_gap_dot_fraction: 0.0,
                hysteresis_fraction: 0.0,
                cfar_keying: false,
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
        Cmd::PlayFile { path, json } => run_play_file(&path, json),
        Cmd::DecodeAndPlay {
            path,
            start,
            end,
            json,
            stdin_control,
            min_snr_db,
            pitch_min_snr_db,
            threshold_scale,
            no_auto_threshold,
            experimental_range_lock,
            range_lock_min_hz,
            range_lock_max_hz,
            min_tone_purity,
            force_pitch_hz,
            wide_bin_count,
            min_pulse_dot_fraction,
            min_gap_dot_fraction,
            hysteresis_fraction,
        } => {
            let cfg = streaming::DecoderConfig {
                min_snr_db,
                pitch_min_snr_db,
                threshold_scale,
                auto_threshold: !no_auto_threshold,
                experimental_range_lock,
                range_lock_min_hz,
                range_lock_max_hz,
                min_tone_purity,
                force_pitch_hz: (force_pitch_hz > 0.0).then_some(force_pitch_hz),
                wide_bin_count,
                min_pulse_dot_fraction,
                min_gap_dot_fraction,
                hysteresis_fraction,

                cfar_keying: false,
            };
            run_decode_and_play(&path, start, end, json, stdin_control, cfg)
        }
        Cmd::StreamLive {
            device,
            seconds,
            json,
            min_snr_db,
            pitch_min_snr_db,
            threshold_scale,
            no_auto_threshold,
            experimental_range_lock,
            range_lock_min_hz,
            range_lock_max_hz,
            min_tone_purity,
            force_pitch_hz,
            wide_bin_count,
            min_pulse_dot_fraction,
            min_gap_dot_fraction,
            hysteresis_fraction,
            record,
            stdin_control,
            loopback,
        } => {
            let cfg = streaming::DecoderConfig {
                min_snr_db,
                pitch_min_snr_db,
                threshold_scale,
                auto_threshold: !no_auto_threshold,
                experimental_range_lock,
                range_lock_min_hz,
                range_lock_max_hz,
                min_tone_purity,
                force_pitch_hz: (force_pitch_hz > 0.0).then_some(force_pitch_hz),
                wide_bin_count,
                min_pulse_dot_fraction,
                min_gap_dot_fraction,
                hysteresis_fraction,

                cfar_keying: false,
            };
            run_stream_live(
                device.as_deref(),
                seconds,
                json,
                cfg,
                record.as_deref(),
                stdin_control,
                loopback,
            )
        }
        Cmd::ProbeFisher {
            path,
            min_hz,
            max_hz,
            step_hz,
            top,
        } => run_probe_fisher(&path, min_hz, max_hz, step_hz, top),
        Cmd::BenchLatency {
            from_file,
            cw_onset_ms,
            truth,
            synth_rate,
            chunk_ms,
            stable_n,
            label,
            purity,
            wide_bins,
            no_auto_threshold,
            force_pitch_hz,
            hysteresis_fraction,
            min_gap_dot_fraction,
            min_pulse_dot_fraction,
            cfar_keying,
            json,
        } => run_bench_latency(
            from_file.as_deref(),
            cw_onset_ms,
            truth.as_deref(),
            synth_rate,
            chunk_ms,
            stable_n,
            &label,
            purity,
            wide_bins,
            no_auto_threshold,
            force_pitch_hz,
            hysteresis_fraction,
            min_gap_dot_fraction,
            min_pulse_dot_fraction,
            cfar_keying,
            json,
        ),
    }
}

fn run_probe_fisher(
    path: &std::path::Path,
    min_hz: f32,
    max_hz: f32,
    step_hz: f32,
    top: usize,
) -> Result<()> {
    let audio = audio::decode_file(path).context("decoding audio")?;
    println!(
        "Probing: {} ({} Hz, {:.2} s)",
        path.display(),
        audio.sample_rate,
        audio.samples.len() as f32 / audio.sample_rate as f32
    );
    let mut scored: Vec<(f32, f32)> = Vec::new();
    let mut f = min_hz;
    while f <= max_hz {
        let s = streaming::trial_decode_score(&audio.samples, audio.sample_rate, f);
        scored.push((f, s));
        f += step_hz;
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    println!("Top {top} pitches by trial-decode Fisher:");
    for (i, (p, s)) in scored.iter().take(top).enumerate() {
        println!("  {:>2}. pitch={:>7.1} Hz  fisher={:>9.3}", i + 1, p, s);
    }
    let max_fisher = scored.first().map(|(_, s)| *s).unwrap_or(0.0);
    let mean: f32 = scored.iter().map(|(_, s)| *s).sum::<f32>() / scored.len() as f32;
    let mut s2: Vec<f32> = scored.iter().map(|(_, s)| *s).collect();
    s2.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p50 = s2[s2.len() / 2];
    let p90 = s2[s2.len() * 9 / 10];
    println!("Distribution: max={max_fisher:.3} p90={p90:.3} p50={p50:.3} mean={mean:.3}");
    Ok(())
}

fn run_bench_latency(
    from_file: Option<&std::path::Path>,
    cw_onset_ms: u32,
    truth: Option<&str>,
    synth_rate: u32,
    chunk_ms: u32,
    stable_n: usize,
    label: &str,
    purity: Option<f32>,
    wide_bins: Option<u8>,
    no_auto_threshold: bool,
    force_pitch_hz: f32,
    hysteresis_fraction: f32,
    min_gap_dot_fraction: f32,
    min_pulse_dot_fraction: f32,
    cfar_keying: bool,
    json: bool,
) -> Result<()> {
    let mut cfg = streaming::DecoderConfig::defaults();
    if let Some(p) = purity {
        cfg.min_tone_purity = p;
    }
    if let Some(w) = wide_bins {
        cfg.wide_bin_count = w;
    }
    if no_auto_threshold {
        cfg.auto_threshold = false;
    }
    if force_pitch_hz > 0.0 {
        cfg.force_pitch_hz = Some(force_pitch_hz);
    }
    if hysteresis_fraction > 0.0 {
        cfg.hysteresis_fraction = hysteresis_fraction;
    }
    if min_gap_dot_fraction > 0.0 {
        cfg.min_gap_dot_fraction = min_gap_dot_fraction;
    }
    if min_pulse_dot_fraction > 0.0 {
        cfg.min_pulse_dot_fraction = min_pulse_dot_fraction;
    }
    cfg.cfar_keying = cfar_keying;

    let scenarios: Vec<bench_latency::Scenario> = if let Some(path) = from_file {
        let truth = truth
            .context("--truth is required with --from-file (uppercase expected transcript)")?;
        let audio = audio::decode_file(path).context("decoding audio file")?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("from_file")
            .to_string();
        vec![bench_latency::Scenario {
            name,
            audio,
            cw_onset_ms,
            truth: truth.to_string(),
        }]
    } else {
        bench_latency::default_scenarios(synth_rate)
    };

    println!(
        "Bench latency: label='{label}'  scenarios={}  chunk_ms={chunk_ms}  stable_n={stable_n}",
        scenarios.len()
    );
    println!(
        "Config: purity={:.2}  wide_bins={}  auto_threshold={}  force_pitch_hz={}  hysteresis={:.2}  min_gap={:.2}  min_pulse={:.2}  cfar={}",
        cfg.min_tone_purity,
        cfg.wide_bin_count,
        cfg.auto_threshold,
        cfg.force_pitch_hz
            .map(|f| format!("{f:.0}"))
            .unwrap_or_else(|| "off".into()),
        cfg.hysteresis_fraction,
        cfg.min_gap_dot_fraction,
        cfg.min_pulse_dot_fraction,
        cfg.cfar_keying,
    );

    let mut results = Vec::with_capacity(scenarios.len());
    for scen in &scenarios {
        let r = bench_latency::run_scenario(scen, cfg, chunk_ms, stable_n, label)?;
        if json {
            // Compact NDJSON record for off-line comparison/aggregation.
            let rec = serde_json::json!({
                "type": "bench_result",
                "label": r.config_label,
                "scenario": r.scenario,
                "cw_onset_ms": r.cw_onset_ms,
                "stable_n": r.stable_n,
                "t_first_pitch_update_ms": r.t_first_pitch_update_ms,
                "t_first_locked_ms": r.t_first_locked_ms,
                "t_first_char_ms": r.t_first_char_ms,
                "t_first_correct_char_ms": r.t_first_correct_char_ms,
                "t_stable_n_correct_ms": r.t_stable_n_correct_ms,
                "acquisition_latency_ms": r.acquisition_latency_ms(),
                "false_chars_before_stable": r.false_chars_before_stable,
                "n_pitch_lost_after_lock": r.n_pitch_lost_after_lock,
                "n_relock_cycles": r.n_relock_cycles,
                "lock_uptime_ratio": r.lock_uptime_ratio,
                "longest_unlocked_gap_ms": r.longest_unlocked_gap_ms,
                "total_unlocked_ms_after_lock": r.total_unlocked_ms_after_lock,
                "locked_pitch_hz": r.locked_pitch_hz,
                "transcript": r.transcript,
                "decoder_counters": r.decoder_counters.clone().unwrap_or(serde_json::Value::Null),
            });
            println!("{rec}");
        }
        results.push(r);
    }
    bench_latency::print_results_table(&results);
    let agg = bench_latency::aggregate(&results);
    bench_latency::print_aggregate(label, &agg);
    Ok(())
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
        "Sliding window: {window:.1}s window, {hop:.1}s hop ({win_samples} samples / {hop_samples} samples)"
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
        println!("{t:>7.2}  {wpm}  {pitch}  {text}");
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
                eprintln!("HARVEST_PROGRESS\t{completed}\t{total}\t{start_s:.3}\t{end_s:.3}");
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
                "is_fallback": c.is_fallback,
                "member_count": c.member_count,
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
            "[{:>6.2}-{:<6.2}] {} shared={} best={} needles={}",
            c.start_s,
            c.end_s,
            if c.is_fallback { "fallback" } else { "region" },
            c.shared_chars,
            c.strongest_copy_len,
            needles
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
    pitch_hz: Option<f32>,
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

fn run_play_file(path: &std::path::Path, json: bool) -> Result<()> {
    use std::time::Duration;

    let playback = audio::play_output_file(path).context("starting audio playback")?;
    let mut emitter = json.then(json::JsonEmitter::new);
    if let Some(em) = emitter.as_mut() {
        em.emit(
            0.0,
            serde_json::json!({
                "type": "playback_ready",
                "source": "playback",
                "path": path.display().to_string(),
                "device": playback.device_name,
                "rate": playback.sample_rate,
                "duration": playback.duration_s,
            }),
        );
    } else {
        println!(
            "Playing {} on {} ({:.2}s)",
            path.display(),
            playback.device_name,
            playback.duration_s
        );
    }

    let mut last_position = -1.0_f32;
    while !playback.is_finished() {
        std::thread::sleep(Duration::from_millis(50));
        let position = playback.position_s().min(playback.duration_s);
        if (position - last_position).abs() < 0.04 {
            continue;
        }
        last_position = position;

        if let Some(em) = emitter.as_mut() {
            em.emit(
                position,
                serde_json::json!({
                    "type": "playback_progress",
                    "position": position,
                    "duration": playback.duration_s,
                }),
            );
        }
    }

    let end_position = playback.position_s().min(playback.duration_s);
    if let Some(em) = emitter.as_mut() {
        em.emit(
            end_position,
            serde_json::json!({
                "type": "playback_end",
                "path": path.display().to_string(),
                "duration": playback.duration_s,
            }),
        );
    }

    Ok(())
}

/// Stdin control commands accepted by `decode-and-play`. The GUI sends
/// one NDJSON line per command; unrecognised lines are ignored.
enum PlaybackControl {
    Pause,
    Resume,
    /// Seek to a position in seconds, relative to the start of the
    /// region being played (NOT relative to the original file).
    Seek(f32),
    Stop,
    Config(streaming::DecoderConfig),
}

fn spawn_stdin_playback_control(
    stop_on_eof: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::sync::mpsc::Receiver<PlaybackControl> {
    use std::io::BufRead;
    let (tx, rx) = std::sync::mpsc::channel::<PlaybackControl>();
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
            // Two top-level shapes are accepted:
            //   {"cmd": "pause"|"resume"|"seek"|"stop"}
            //   {"type": "config", ...}
            // The two shapes are distinguished by which key is present so a
            // single stdin pipe can carry both transport and decoder updates.
            if let Some(cmd) = v.get("cmd").and_then(|c| c.as_str()) {
                let event = match cmd {
                    "pause" => Some(PlaybackControl::Pause),
                    "resume" | "play" => Some(PlaybackControl::Resume),
                    "stop" => Some(PlaybackControl::Stop),
                    "seek" => v
                        .get("position")
                        .and_then(|p| p.as_f64())
                        .map(|p| PlaybackControl::Seek(p as f32)),
                    _ => None,
                };
                if let Some(ev) = event {
                    if tx.send(ev).is_err() {
                        break;
                    }
                }
                continue;
            }

            if v.get("type").and_then(|t| t.as_str()) != Some("config") {
                continue;
            }
            // Reuse the same field set as `spawn_stdin_config_channel`.
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
            if let Some(b) = v.get("experimental_range_lock").and_then(|x| x.as_bool()) {
                state.experimental_range_lock = b;
            }
            if let Some(x) = v.get("range_lock_min_hz").and_then(|x| x.as_f64()) {
                state.range_lock_min_hz = x as f32;
            }
            if let Some(x) = v.get("range_lock_max_hz").and_then(|x| x.as_f64()) {
                state.range_lock_max_hz = x as f32;
            }
            if let Some(x) = v.get("min_tone_purity").and_then(|x| x.as_f64()) {
                state.min_tone_purity = x as f32;
            }
            if let Some(x) = v.get("force_pitch_hz").and_then(|x| x.as_f64()) {
                state.force_pitch_hz = if x > 0.0 { Some(x as f32) } else { None };
            } else if v
                .get("force_pitch_hz")
                .map(|x| x.is_null())
                .unwrap_or(false)
            {
                state.force_pitch_hz = None;
            }
            if let Some(x) = v.get("wide_bin_count").and_then(|x| x.as_i64()) {
                state.wide_bin_count = x.clamp(0, 16) as u8;
            }
            if let Some(x) = v.get("min_pulse_dot_fraction").and_then(|x| x.as_f64()) {
                state.min_pulse_dot_fraction = x.max(0.0) as f32;
            }
            if let Some(x) = v.get("min_gap_dot_fraction").and_then(|x| x.as_f64()) {
                state.min_gap_dot_fraction = x.max(0.0) as f32;
            }
            if let Some(x) = v.get("hysteresis_fraction").and_then(|x| x.as_f64()) {
                state.hysteresis_fraction = x.max(0.0) as f32;
            }
            if tx.send(PlaybackControl::Config(state)).is_err() {
                break;
            }
        }
        // Stdin EOF — graceful stop so the WAV recorder Drop runs cleanly
        // and the GUI sees the {"type":"end"} bookend.
        stop_on_eof.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    rx
}

fn run_decode_and_play(
    path: &std::path::Path,
    start_s: f32,
    end_s: f32,
    json: bool,
    stdin_control: bool,
    cfg: streaming::DecoderConfig,
) -> Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    let audio = audio::decode_file(path).context("decoding audio file")?;
    if audio.samples.is_empty() {
        return Err(anyhow::anyhow!("decoded audio was empty"));
    }
    let sr = audio.sample_rate;
    let total_samples = audio.samples.len();
    let total_dur = total_samples as f32 / sr as f32;

    // Region trim. `end_s <= 0` means "to end of file". Start is clamped
    // to [0, total_dur); end is clamped to (start, total_dur]. If the
    // region is degenerate the function returns early with an error so
    // the GUI doesn't sit on a stuck process.
    let region_start = start_s.clamp(0.0, total_dur).max(0.0);
    let region_end_raw = if end_s <= 0.0 { total_dur } else { end_s };
    let region_end = region_end_raw
        .clamp(region_start, total_dur)
        .max(region_start);
    if region_end - region_start < 0.05 {
        return Err(anyhow::anyhow!(
            "region [{region_start:.3}s, {region_end:.3}s] is too short to decode"
        ));
    }
    let start_idx = ((region_start * sr as f32) as usize).min(total_samples);
    let end_idx = ((region_end * sr as f32) as usize)
        .min(total_samples)
        .max(start_idx);
    let region_samples = audio.samples[start_idx..end_idx].to_vec();
    let region_len = region_samples.len();
    let region_dur = region_len as f32 / sr as f32;

    let stop = Arc::new(AtomicBool::new(false));
    let control_rx = stdin_control.then(|| spawn_stdin_playback_control(Arc::clone(&stop)));

    let playback = audio::play_samples_with_control(region_samples.clone(), sr)
        .context("starting decode-and-play playback")?;

    let mut emitter = json.then(json::JsonEmitter::new);
    if let Some(em) = emitter.as_mut() {
        em.emit(
            0.0,
            serde_json::json!({
                "type": "ready",
                "source": "decode-and-play",
                "path": path.display().to_string(),
                "input_rate": sr,
                "output_rate": playback.output_rate,
                "device": playback.device_name,
                "file_duration": total_dur,
                "region_start": region_start,
                "region_end": region_end,
                "duration": region_dur,
                "config": serde_json::json!({
                    "min_snr_db": cfg.min_snr_db,
                    "pitch_min_snr_db": cfg.pitch_min_snr_db,
                    "threshold_scale": cfg.threshold_scale,
                    "auto_threshold": cfg.auto_threshold,
                    "min_tone_purity": cfg.min_tone_purity,
                    "wide_bin_count": cfg.wide_bin_count,
                    "min_pulse_dot_fraction": cfg.min_pulse_dot_fraction,
                    "min_gap_dot_fraction": cfg.min_gap_dot_fraction,
                    "hysteresis_fraction": cfg.hysteresis_fraction,
                }),
            }),
        );
    } else {
        println!(
            "Decode-and-play: {} [{:.2}s..{:.2}s] on {} ({} Hz input -> {} Hz output)",
            path.display(),
            region_start,
            region_end,
            playback.device_name,
            sr,
            playback.output_rate
        );
    }

    let mut decoder = streaming::StreamingDecoder::new(sr)?;
    decoder.set_config(cfg);
    let mut current_cfg = cfg;
    let mut transcript = String::new();
    let mut consumed_input_frames: u64 = 0;
    let mut last_position_emit = Instant::now();
    let mut last_seek_epoch = playback.seek_epoch();

    // Pump tight enough for low decoder latency but loose enough not to
    // burn CPU. The decoder feed itself happens in larger chunks below.
    let pump_sleep = Duration::from_millis(8);

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        // Drain any pending stdin commands.
        if let Some(rx) = control_rx.as_ref() {
            let mut latest_cfg: Option<streaming::DecoderConfig> = None;
            let mut latest_seek: Option<f32> = None;
            let mut want_pause = false;
            let mut want_resume = false;
            let mut want_stop = false;
            while let Ok(cmd) = rx.try_recv() {
                match cmd {
                    PlaybackControl::Pause => want_pause = true,
                    PlaybackControl::Resume => {
                        want_resume = true;
                        want_pause = false;
                    }
                    PlaybackControl::Seek(pos) => latest_seek = Some(pos),
                    PlaybackControl::Stop => want_stop = true,
                    PlaybackControl::Config(c) => latest_cfg = Some(c),
                }
            }
            if want_pause {
                playback.pause();
                if let Some(em) = emitter.as_mut() {
                    em.emit(
                        playback.position_seconds(),
                        serde_json::json!({
                            "type": "paused",
                            "position": playback.position_seconds(),
                        }),
                    );
                }
            }
            if want_resume {
                playback.resume();
                if let Some(em) = emitter.as_mut() {
                    em.emit(
                        playback.position_seconds(),
                        serde_json::json!({
                            "type": "resumed",
                            "position": playback.position_seconds(),
                        }),
                    );
                }
            }
            if let Some(target) = latest_seek {
                let clamped = target.clamp(0.0, region_dur);
                playback.seek_to_seconds(clamped);
                // Wait briefly for the audio callback to ack the seek.
                let deadline = Instant::now() + Duration::from_millis(150);
                while playback.seek_epoch() == last_seek_epoch && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(2));
                }
                last_seek_epoch = playback.seek_epoch();
                // Reset decoder so threshold/pitch state from before the
                // seek can't bleed into post-seek decoding.
                decoder = streaming::StreamingDecoder::new(sr)?;
                decoder.set_config(current_cfg);
                consumed_input_frames = playback.position_input_frames();
                if let Some(em) = emitter.as_mut() {
                    em.emit(
                        playback.position_seconds(),
                        serde_json::json!({
                            "type": "seeked",
                            "position": playback.position_seconds(),
                            "epoch": last_seek_epoch,
                        }),
                    );
                }
            }
            if let Some(c) = latest_cfg {
                current_cfg = c;
                decoder.set_config(c);
                if let Some(em) = emitter.as_mut() {
                    em.emit(
                        playback.position_seconds(),
                        serde_json::json!({
                            "type": "config_ack",
                            "min_snr_db": c.min_snr_db,
                            "pitch_min_snr_db": c.pitch_min_snr_db,
                            "threshold_scale": c.threshold_scale,
                            "min_tone_purity": c.min_tone_purity,
                            "wide_bin_count": c.wide_bin_count,
                            "min_pulse_dot_fraction": c.min_pulse_dot_fraction,
                            "min_gap_dot_fraction": c.min_gap_dot_fraction,
                            "hysteresis_fraction": c.hysteresis_fraction,
                        }),
                    );
                }
            }
            if want_stop {
                break;
            }
        }

        // Detect a seek that may have happened from another path (defensive).
        let observed_epoch = playback.seek_epoch();
        if observed_epoch != last_seek_epoch {
            last_seek_epoch = observed_epoch;
            decoder = streaming::StreamingDecoder::new(sr)?;
            decoder.set_config(current_cfg);
            consumed_input_frames = playback.position_input_frames();
        }

        let played_input = playback.position_input_frames().min(region_len as u64);
        if played_input > consumed_input_frames {
            let start = consumed_input_frames as usize;
            let end = (played_input as usize).min(region_len);
            // Feed in modest chunks so very large catch-up gaps (e.g.
            // after a long pause) don't block the loop with one giant
            // decoder call.
            let max_chunk = (sr as usize / 20).max(64); // ~50 ms.
            let mut cursor = start;
            while cursor < end {
                let chunk_end = (cursor + max_chunk).min(end);
                let chunk = &region_samples[cursor..chunk_end];
                let events = decoder.feed(chunk)?;
                consumed_input_frames = chunk_end as u64;
                let t = consumed_input_frames as f32 / sr as f32;
                emit_decoder_events(emitter.as_mut(), t, events, &mut transcript);
                cursor = chunk_end;
            }
        }

        // Periodic position tick (~25 Hz) so the GUI scrubber stays smooth.
        if last_position_emit.elapsed() >= Duration::from_millis(40) {
            last_position_emit = Instant::now();
            if let Some(em) = emitter.as_mut() {
                em.emit(
                    playback.position_seconds(),
                    serde_json::json!({
                        "type": "position",
                        "position": playback.position_seconds(),
                        "duration": region_dur,
                        "paused": playback.is_paused(),
                    }),
                );
            }
        }

        if playback.is_finished() && consumed_input_frames as usize >= region_len {
            break;
        }

        std::thread::sleep(pump_sleep);
    }

    // Flush the decoder — there may be a partial letter buffered.
    let final_t = consumed_input_frames as f32 / sr as f32;
    let flush_events = decoder.flush();
    emit_decoder_events(emitter.as_mut(), final_t, flush_events, &mut transcript);

    if let Some(em) = emitter.as_mut() {
        em.emit(
            final_t,
            serde_json::json!({
                "type": "end",
                "position": final_t,
                "transcript": transcript.trim(),
                "wpm": decoder.current_wpm(),
                "pitch": decoder.pitch(),
            }),
        );
    } else {
        println!();
        println!("Transcript:");
        println!("{}", transcript.trim());
    }

    Ok(())
}

fn emit_decoder_events(
    mut emitter: Option<&mut json::JsonEmitter>,
    t: f32,
    events: Vec<streaming::StreamEvent>,
    transcript: &mut String,
) {
    for ev in events {
        match &ev {
            streaming::StreamEvent::Char { ch, .. } => transcript.push(*ch),
            streaming::StreamEvent::Word => transcript.push(' '),
            streaming::StreamEvent::Garbled { .. } => transcript.push('?'),
            _ => {}
        }
        if let Some(em) = emitter.as_deref_mut() {
            em.emit_event(t, &ev);
        }
    }
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
                            "[t={t_in_audio:>6.2}s real+{lag_ms:>4}ms] PITCH lock: {pitch_hz:.1} Hz"
                        );
                    }
                }
                streaming::StreamEvent::PitchLost { reason } => {
                    if !quiet {
                        println!("[t={t_in_audio:>6.2}s real+{lag_ms:>4}ms] PITCH lost ({reason})");
                    }
                }
                streaming::StreamEvent::WpmUpdate { wpm } => {
                    let changed = last_wpm.map(|w| (w - wpm).abs() >= 1.0).unwrap_or(true);
                    if changed {
                        if !quiet {
                            println!(
                                "[t={t_in_audio:>6.2}s real+{lag_ms:>4}ms] WPM    -> {wpm:.1}"
                            );
                        }
                        last_wpm = Some(wpm);
                    }
                }
                streaming::StreamEvent::Char {
                    ch,
                    morse,
                    pitch_hz,
                    ..
                } => {
                    transcript.push(ch);
                    if !quiet {
                        let pitch_suffix = pitch_hz
                            .map(|hz| format!("  @{hz:>6.1} Hz"))
                            .unwrap_or_default();
                        println!(
                            "[t={t_in_audio:>6.2}s real+{lag_ms:>4}ms] CHAR  '{ch}' ({morse:>5}){pitch_suffix}  transcript: {transcript}"
                        );
                    }
                }
                streaming::StreamEvent::Word => {
                    transcript.push(' ');
                    if !quiet {
                        println!("[t={t_in_audio:>6.2}s real+{lag_ms:>4}ms] WORD  break");
                    }
                }
                streaming::StreamEvent::Garbled {
                    morse, pitch_hz, ..
                } => {
                    transcript.push('?');
                    if !quiet {
                        let pitch_suffix = pitch_hz
                            .map(|hz| format!("  @{hz:>6.1} Hz"))
                            .unwrap_or_default();
                        println!(
                            "[t={t_in_audio:>6.2}s real+{lag_ms:>4}ms] ???  garbled morse: {morse}{pitch_suffix}"
                        );
                    }
                }
                streaming::StreamEvent::Power { .. } => {
                    // Power events are JSON-only by default; suppress in human output.
                }
                streaming::StreamEvent::Confidence { .. } => {
                    // Confidence events are JSON-only by default; suppress in human output.
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
            "Window {window_seconds:.1}s / min {min_window_seconds:.1}s / decode every {decode_every_ms}ms / chunk {chunk_ms}ms"
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
    loopback: bool,
) -> Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    let capture = if loopback {
        audio::open_loopback_with_recording(device, 1.0, record_path)?
    } else {
        audio::open_input_with_recording(device, 1.0, record_path)?
    };
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
                streaming::StreamEvent::PitchLost { reason } => {
                    println!("[t={t:>6.2}s] PITCH lost ({reason})");
                }
                streaming::StreamEvent::WpmUpdate { wpm } => {
                    let changed = last_wpm.map(|w| (w - wpm).abs() >= 1.0).unwrap_or(true);
                    if changed {
                        println!("[t={t:>6.2}s] WPM    -> {wpm:.1}");
                        last_wpm = Some(wpm);
                    }
                }
                streaming::StreamEvent::Char {
                    ch,
                    morse,
                    pitch_hz,
                    ..
                } => {
                    transcript.push(ch);
                    let pitch_suffix = pitch_hz
                        .map(|hz| format!("  @{hz:>6.1} Hz"))
                        .unwrap_or_default();
                    println!(
                        "[t={t:>6.2}s] CHAR  '{ch}' ({morse}){pitch_suffix}  transcript: {transcript}"
                    );
                }
                streaming::StreamEvent::Word => {
                    transcript.push(' ');
                    println!("[t={t:>6.2}s] WORD  break");
                }
                streaming::StreamEvent::Garbled {
                    morse, pitch_hz, ..
                } => {
                    transcript.push('?');
                    let pitch_suffix = pitch_hz
                        .map(|hz| format!("  @{hz:>6.1} Hz"))
                        .unwrap_or_default();
                    println!("[t={t:>6.2}s] ???  garbled morse: {morse}{pitch_suffix}");
                }
                streaming::StreamEvent::Power { .. } => {}
                streaming::StreamEvent::Confidence { .. } => {}
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
            if let Some(b) = v.get("experimental_range_lock").and_then(|x| x.as_bool()) {
                state.experimental_range_lock = b;
            }
            if let Some(x) = v.get("range_lock_min_hz").and_then(|x| x.as_f64()) {
                state.range_lock_min_hz = x as f32;
            }
            if let Some(x) = v.get("range_lock_max_hz").and_then(|x| x.as_f64()) {
                state.range_lock_max_hz = x as f32;
            }
            if let Some(x) = v.get("min_tone_purity").and_then(|x| x.as_f64()) {
                state.min_tone_purity = x as f32;
            }
            // force_pitch_hz: <number> sets a forced lock; 0/null clears it.
            if let Some(x) = v.get("force_pitch_hz").and_then(|x| x.as_f64()) {
                state.force_pitch_hz = if x > 0.0 { Some(x as f32) } else { None };
            } else if v
                .get("force_pitch_hz")
                .map(|x| x.is_null())
                .unwrap_or(false)
            {
                state.force_pitch_hz = None;
            }
            if let Some(x) = v.get("wide_bin_count").and_then(|x| x.as_i64()) {
                state.wide_bin_count = x.clamp(0, 16) as u8;
            }
            if let Some(x) = v.get("min_pulse_dot_fraction").and_then(|x| x.as_f64()) {
                state.min_pulse_dot_fraction = x.max(0.0) as f32;
            }
            if let Some(x) = v.get("min_gap_dot_fraction").and_then(|x| x.as_f64()) {
                state.min_gap_dot_fraction = x.max(0.0) as f32;
            }
            if let Some(x) = v.get("hysteresis_fraction").and_then(|x| x.as_f64()) {
                state.hysteresis_fraction = x.max(0.0) as f32;
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

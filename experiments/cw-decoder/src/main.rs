// CLI handler functions take many decoder/render parameters by design.
// Bundling them into structs would add ceremony without improving clarity.
#![allow(clippy::too_many_arguments)]

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use cw_decoder_poc::{
    audio, bench_latency, decoder, ditdah_streaming, harvest, json, log_capture, preview,
    streaming, streaming_v2, tui,
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

        /// Pin the decoder WPM (overrides ditdah's auto-detect AND its
        /// median-element-length self-calibration). Useful when auto-WPM
        /// locks onto a wrong value on noisy live signals. 0 = auto.
        #[arg(long, default_value_t = 0.0)]
        pin_wpm: f32,
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
        /// Pre-amplify input samples by this many dB before feeding the
        /// decoder. Useful for real-radio audio that comes in well below
        /// 0 dBFS (e.g. USB Audio Codec at default Windows mic level).
        /// 0 = no gain. 20 dB = 10x amplitude.
        #[arg(long, default_value_t = 0.0)]
        input_gain_db: f32,
        /// Automatically normalise input samples to a target peak of
        /// `--auto-gain-target` (default 0.5). Overrides `--input-gain-db`
        /// when set. Computes the gain from the peak of the current input
        /// (whole-file for file mode, sliding window for live), so weak
        /// real-radio signals get amplified up to where the decoder's
        /// thresholds were tuned.
        #[arg(long)]
        auto_input_gain: bool,
        /// Target peak amplitude (0..1) for `--auto-input-gain`. Default
        /// 0.5 leaves headroom while pushing weak signals into the
        /// decoder's calibrated range.
        #[arg(long, default_value_t = 0.5)]
        auto_gain_target: f32,
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
        /// Pre-amplify input samples by this many dB before feeding the
        /// decoder. See `stream-file --input-gain-db`.
        #[arg(long, default_value_t = 0.0)]
        input_gain_db: f32,
        /// Auto-normalise input samples to a target RMS level (with soft
        /// clip). See `stream-file --auto-input-gain`.
        #[arg(long)]
        auto_input_gain: bool,
        /// Target RMS amplitude (0..1) for `--auto-input-gain`.
        #[arg(long, default_value_t = 0.3)]
        auto_gain_target: f32,
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
        /// Capture from a system output device using WASAPI loopback instead
        /// of from an input device.
        #[arg(long)]
        loopback: bool,
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
    /// Round-2 live decoder: append-only audio buffer + whole-buffer
    /// ditdah redecode. Each decode emits a full-replacement transcript
    /// (no incremental commit). Empirically reaches CER ~0.06 vs the
    /// rolling-window backend's 0.83-0.89 on training-set-a 30 WPM CW.
    /// Per-decode latency stays under 100 ms even on a 3-minute buffer.
    StreamLiveV2 {
        #[arg(long)]
        device: Option<String>,
        /// How long to capture before exiting (seconds). 0 = run forever.
        #[arg(long, default_value_t = 0.0)]
        seconds: f32,
        /// Emit one JSON object per event to stdout (for the Avalonia GUI bridge).
        #[arg(long)]
        json: bool,
        /// How often to re-run ditdah on the buffered audio.
        #[arg(long, default_value_t = streaming_v2::DEFAULT_DECODE_EVERY_MS)]
        decode_every_ms: u64,
        /// Optional WAV path to mirror raw mono samples to (16-bit PCM).
        #[arg(long)]
        record: Option<PathBuf>,
        /// Read NDJSON config-update lines from stdin while streaming
        /// (currently only `{"type":"reset_lock"}` is honored, which
        /// resets the audio buffer).
        #[arg(long)]
        stdin_control: bool,
        /// Capture from a system OUTPUT device (WASAPI loopback).
        #[arg(long)]
        loopback: bool,
        /// Pin the WPM hint passed into ditdah. Overrides auto-detect AND
        /// the median-element-length self-calibration. Useful when auto-WPM
        /// locks onto a wrong value on noisy live signals. 0 = auto.
        #[arg(long, default_value_t = 0.0)]
        pin_wpm: f32,
    },
    /// Live streaming with the in-house envelope decoder, emitting full
    /// visualizer frames (audio envelope, hysteresis thresholds, classified
    /// events, k-means centroids) on every decode cycle. Powers the GUI
    /// VISUALIZER tab.
    StreamLiveV3 {
        #[arg(long)]
        device: Option<String>,
        #[arg(long, default_value_t = 0.0)]
        seconds: f32,
        #[arg(long)]
        json: bool,
        /// Re-decode the whole buffer every N ms (default 250).
        #[arg(long, default_value_t = 250)]
        decode_every_ms: u64,
        #[arg(long)]
        record: Option<PathBuf>,
        #[arg(long)]
        stdin_control: bool,
        #[arg(long)]
        loopback: bool,
        /// Pin the WPM (0 = auto / k-means lock).
        #[arg(long, default_value_t = 0.0)]
        pin_wpm: f32,
        /// Pin the pitch in Hz (0 = auto-detect). Useful when the auto
        /// detector locks onto a noise/harmonic peak instead of the CW tone.
        #[arg(long, default_value_t = 0.0)]
        pin_hz: f32,
        /// Minimum signal-to-noise ratio (dB) required to emit text.
        /// Computed as 20*log10(signal_floor / noise_floor) from envelope
        /// percentiles. Frames below this threshold still produce
        /// visualizer data but no transcript, suppressing the
        /// noise-locked dit-spam failure mode. Default 6.0 dB. Set 0 to
        /// disable.
        #[arg(long, default_value_t = cw_decoder_poc::envelope_decoder::DEFAULT_MIN_SNR_DB)]
        min_snr_db: f32,
        /// Decode an audio file (mp3/wav/m4a/...) instead of live capture.
        /// Samples are streamed at real-time pace into the same envelope
        /// pipeline so the visualizer behaves identically to live audio.
        #[arg(long)]
        file: Option<PathBuf>,
        /// When used with --file, play the file through the default output
        /// device and use the playback stream position as the decoder clock.
        /// This keeps visualizer bars/transcript aligned with audible audio
        /// instead of a separate wall-clock replay loop.
        #[arg(long)]
        play: bool,

        /// Multi-pitch (CW Skimmer-style) decode: also run K parallel
        /// per-pitch decoders alongside the existing single-pitch
        /// pipeline. K=0 disables multi-pitch (default; preserves the
        /// legacy single-track behavior). When K > 0, an additional
        /// `multi_track_transcript` JSON event is emitted per cycle
        /// alongside the single-track `transcript` event.
        #[arg(long, alias = "multi", default_value_t = 0)]
        multi_pitch: usize,
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
        /// Use the append-only V3 event-stream foundation instead of the
        /// legacy streaming decoder. For --from-file this reports transcript
        /// quality against truth; latency-specific fields are left empty.
        #[arg(long, default_value_t = false)]
        foundation: bool,
        /// Emit one NDJSON record per scenario in addition to the table
        /// (handy for collecting comparison runs into a file).
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Generate a synthetic CW WAV with rough-fist jitter (variable
    /// element/gap durations) and a `.truth.txt` sidecar containing the
    /// reference text. Stress-tests adaptive dit/dah classifiers.
    GenRoughFist {
        /// Output WAV path. A `<basename>.truth.txt` sidecar is also written.
        #[arg(long)]
        output: PathBuf,
        /// Reference text to encode (uppercased; non-Morse chars are skipped).
        #[arg(long)]
        text: String,
        /// Nominal speed in WPM.
        #[arg(long, default_value_t = 20.0)]
        wpm: f32,
        /// Carrier pitch in Hz.
        #[arg(long, default_value_t = 700.0)]
        pitch_hz: f32,
        /// Sample rate in Hz.
        #[arg(long, default_value_t = 12000)]
        sample_rate: u32,
        /// Per-element timing jitter as a fraction (e.g., 0.20 = ±20%).
        /// Applied multiplicatively to dit, dah, and gap durations.
        #[arg(long, default_value_t = 0.20)]
        jitter: f32,
        /// Dah/dit ratio (textbook is 3.0; sloppy fists often run 2.3..2.7).
        #[arg(long, default_value_t = 3.0)]
        dah_ratio: f32,
        /// Multiplicative bias on dit length (e.g., 1.1 = "heavy dits").
        #[arg(long, default_value_t = 1.0)]
        dit_weight: f32,
        /// Additive white-noise amplitude (0..1). 0 = clean.
        #[arg(long, default_value_t = 0.0)]
        noise: f32,
        /// Random seed (for reproducible jitter).
        #[arg(long, default_value_t = 1)]
        seed: u64,
    },
}

const CLI_THREAD_STACK_BYTES: usize = 16 * 1024 * 1024;

fn main() -> Result<()> {
    std::thread::Builder::new()
        .name("cw-decoder-cli".to_string())
        .stack_size(CLI_THREAD_STACK_BYTES)
        .spawn(run_cli)?
        .join()
        .map_err(|panic| {
            let message = panic
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("cw-decoder CLI thread panicked");
            anyhow::anyhow!(message.to_string())
        })?
}

fn run_cli() -> Result<()> {
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
            pin_wpm,
        } => run_file(&path, sliding, window, hop, pin_wpm, &log_capture),
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
            input_gain_db,
            auto_input_gain,
            auto_gain_target,
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
            run_stream_file(
                &path,
                chunk_ms,
                realtime,
                quiet,
                json,
                cfg,
                stdin_control,
                input_gain_db,
                auto_input_gain,
                auto_gain_target,
            )
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
            input_gain_db,
            auto_input_gain,
            auto_gain_target,
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
            input_gain_db,
            auto_input_gain,
            auto_gain_target,
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
            loopback,
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
            loopback,
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
        Cmd::StreamLiveV2 {
            device,
            seconds,
            json,
            decode_every_ms,
            record,
            stdin_control,
            loopback,
            pin_wpm,
        } => run_stream_live_v2(
            device.as_deref(),
            seconds,
            json,
            decode_every_ms,
            record.as_deref(),
            stdin_control,
            loopback,
            (pin_wpm > 0.0).then_some(pin_wpm),
        ),
        Cmd::StreamLiveV3 {
            device,
            seconds,
            json,
            decode_every_ms,
            record,
            stdin_control,
            loopback,
            pin_wpm,
            pin_hz,
            min_snr_db,
            file,
            play,
            multi_pitch,
        } => run_stream_live_v3(
            device.as_deref(),
            seconds,
            json,
            decode_every_ms,
            record.as_deref(),
            stdin_control,
            loopback,
            (pin_wpm > 0.0).then_some(pin_wpm),
            (pin_hz > 0.0).then_some(pin_hz),
            min_snr_db,
            file.as_deref(),
            play,
            multi_pitch,
        ),
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
            foundation,
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
            foundation,
            json,
        ),
        Cmd::GenRoughFist {
            output,
            text,
            wpm,
            pitch_hz,
            sample_rate,
            jitter,
            dah_ratio,
            dit_weight,
            noise,
            seed,
        } => run_gen_rough_fist(
            &output,
            &text,
            wpm,
            pitch_hz,
            sample_rate,
            jitter,
            dah_ratio,
            dit_weight,
            noise,
            seed,
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
    foundation: bool,
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
        let r = if foundation {
            let transcript = cw_decoder_poc::append_decode::decode_samples_append(
                &scen.audio.samples,
                scen.audio.sample_rate,
                None,
                None,
                cw_decoder_poc::envelope_decoder::DEFAULT_MIN_SNR_DB,
            )
            .decoded_text
            .trim()
            .to_string();
            bench_latency::BenchResult {
                scenario: scen.name.clone(),
                config_label: label.to_string(),
                cw_onset_ms: scen.cw_onset_ms,
                truth: scen.truth.clone(),
                transcript,
                stable_n,
                ..Default::default()
            }
        } else {
            bench_latency::run_scenario(scen, cfg, chunk_ms, stable_n, label)?
        };
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
    pin_wpm: f32,
    log_capture: &log_capture::DitdahLogCapture,
) -> Result<()> {
    let pin = (pin_wpm > 0.0).then_some(pin_wpm);
    println!("Decoding: {}", path.display());
    if let Some(w) = pin {
        println!("  pin_wpm = {w:.1}");
    }
    let audio = audio::decode_file(path).context("decoding audio file")?;
    let dur = audio.samples.len() as f32 / audio.sample_rate as f32;
    println!(
        "  sample_rate = {} Hz, duration = {:.2} s, samples = {}",
        audio.sample_rate,
        dur,
        audio.samples.len()
    );

    if !sliding {
        let out =
            decoder::decode_window_with_pin(&audio.samples, audio.sample_rate, log_capture, pin)?;
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
        let out = decoder::decode_window_with_pin(slice, audio.sample_rate, log_capture, pin)?;
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

/// Pre-amplify input samples in-place. Real-radio captures (USB Audio
/// Codec mic input) routinely come in 20-30 dB below the synthetic test
/// corpus the decoder was tuned against, so the keying threshold and
/// SNR gate spend the whole clip on noise. Boosting brings the input
/// back into the calibrated range. Returns the dB applied (None if
/// no-op).
fn apply_input_gain(
    samples: &mut [f32],
    input_gain_db: f32,
    auto_input_gain: bool,
    auto_gain_target: f32,
) -> Option<f32> {
    if auto_input_gain {
        // Drive a high-percentile sample to the target ceiling, then
        // tanh-clip. RMS-targeted gain under-amplified weak signals
        // because the noise floor swallows the budget; peak-targeted
        // saturation matches what manual +25..+45 dB pre-amp does for
        // real-radio captures (and that hits ~70% LCS on labelled
        // clips that pure RMS scaling caps at ~20%).
        let mut abs: Vec<f32> = samples.iter().map(|s| s.abs()).collect();
        if abs.is_empty() {
            return None;
        }
        // 99th percentile via partial sort.
        let idx = ((abs.len() as f32) * 0.99) as usize;
        let idx = idx.min(abs.len() - 1);
        abs.select_nth_unstable_by(idx, |a, b| {
            a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
        });
        let p99 = abs[idx].max(1e-6);
        // Allow target up to 4.0 so the tanh deliberately saturates.
        let target = auto_gain_target.clamp(0.05, 4.0);
        let scale = target / p99;
        for s in samples.iter_mut() {
            *s = (*s * scale).tanh();
        }
        Some(20.0 * scale.log10())
    } else if input_gain_db.abs() > f32::EPSILON {
        let scale = 10.0_f32.powf(input_gain_db / 20.0);
        for s in samples.iter_mut() {
            *s = (*s * scale).tanh();
        }
        Some(input_gain_db)
    } else {
        None
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
    input_gain_db: f32,
    auto_input_gain: bool,
    auto_gain_target: f32,
) -> Result<()> {
    use std::time::Instant;
    let mut audio = audio::decode_file(path).context("decoding audio file")?;
    let applied_gain_db = apply_input_gain(
        &mut audio.samples,
        input_gain_db,
        auto_input_gain,
        auto_gain_target,
    );
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
                    "input_gain_db": applied_gain_db,
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
        if let Some(g) = applied_gain_db {
            println!("Input gain applied: {g:+.1} dB");
        }
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
            let mut reset_lock = false;
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    StdinControlMessage::Config(c) => latest = Some(c),
                    StdinControlMessage::ResetLock => reset_lock = true,
                }
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
            if reset_lock {
                let evs = decoder.force_reset_lock();
                if let Some(em) = emitter.as_mut() {
                    for ev in &evs {
                        em.emit_event(t_in_audio, ev);
                    }
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
    input_gain_db: f32,
    auto_input_gain: bool,
    auto_gain_target: f32,
    log_capture: &log_capture::DitdahLogCapture,
) -> Result<()> {
    use std::time::Instant;

    let mut audio = audio::decode_file(path).context("decoding audio file")?;
    let _applied_gain_db = apply_input_gain(
        &mut audio.samples,
        input_gain_db,
        auto_input_gain,
        auto_gain_target,
    );
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
    loopback: bool,
    json: bool,
    log_capture: &log_capture::DitdahLogCapture,
) -> Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    let capture = if loopback {
        audio::open_loopback_with_recording(device, 1.0, record_path)?
    } else {
        audio::open_input_with_recording(device, 1.0, record_path)?
    };
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
    let manual_anchor = Arc::new(AtomicBool::new(false));
    {
        let stop = Arc::clone(&stop);
        ctrlc_setup(move || {
            stop.store(true, Ordering::Relaxed);
        });
    }
    // Watch stdin for EOF or a stop line. The GUI also sends control
    // messages such as reset_lock; those must not terminate the rolling
    // ditdah backend (it has no persistent pitch lock to reset anyway).
    // EOF still triggers shutdown so Drop runs on LiveCapture and the WAV
    // writer flushes the data chunk + RIFF header.
    // Without this, Kill leaves a header-only WAV that Replay can't read.
    //
    // NOTE: on Windows, std::io::stdin() with an anonymous pipe can fail
    // to surface EOF reliably — writing a sentinel byte from the parent
    // is the robust signal. We accept either path.
    {
        let stop = Arc::clone(&stop);
        let manual_anchor = Arc::clone(&manual_anchor);
        std::thread::spawn(move || {
            use std::io::BufRead;
            let stdin = std::io::stdin();
            for line in stdin.lock().lines() {
                match line {
                    Ok(line) if line.trim().eq_ignore_ascii_case("stop") => break,
                    Ok(line) if is_manual_anchor_command(line.trim()) => {
                        manual_anchor.store(true, Ordering::Relaxed);
                    }
                    Ok(_) => continue,
                    Err(_) => break,
                }
            }
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

        if manual_anchor.swap(false, Ordering::Relaxed) {
            streamer.force_stream_anchor();
            if let Some(em) = emitter.as_mut() {
                em.emit(
                    started.elapsed().as_secs_f32(),
                    serde_json::json!({
                        "type": "status",
                        "message": "Manual anchor armed",
                    }),
                );
            }
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

fn is_manual_anchor_command(line: &str) -> bool {
    if line.eq_ignore_ascii_case("anchor") || line.eq_ignore_ascii_case("manual_anchor") {
        return true;
    }

    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return false;
    };

    matches!(
        v.get("type").and_then(|t| t.as_str()),
        Some("manual_anchor") | Some("anchor")
    )
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
            let mut reset_lock = false;
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    StdinControlMessage::Config(c) => latest = Some(c),
                    StdinControlMessage::ResetLock => reset_lock = true,
                }
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
            if reset_lock {
                let evs = decoder.force_reset_lock();
                if let Some(em) = emitter.as_mut() {
                    let t = started.elapsed().as_secs_f32();
                    for ev in &evs {
                        em.emit_event(t, ev);
                    }
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

/// Round-2 live decoder: append-only audio buffer + whole-buffer ditdah
/// redecode. See [`streaming_v2`] for the design rationale.
fn run_stream_live_v2(
    device: Option<&str>,
    seconds: f32,
    json: bool,
    decode_every_ms: u64,
    record_path: Option<&std::path::Path>,
    stdin_control: bool,
    loopback: bool,
    pin_wpm: Option<f32>,
) -> Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    let capture = if loopback {
        audio::open_loopback_with_recording(device, 1.0, record_path)?
    } else {
        audio::open_input_with_recording(device, 1.0, record_path)?
    };
    let mut decoder = streaming_v2::WholeBufferDecoder::new(capture.sample_rate);
    decoder.set_pin_wpm(pin_wpm);

    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = Arc::clone(&stop);
        ctrlc_setup(move || {
            stop.store(true, Ordering::Relaxed);
        });
    }
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
                "source": "live-v2",
                "device": capture.device_name,
                "rate": capture.sample_rate,
                "decode_every_ms": decode_every_ms,
                "min_decode_audio_secs": streaming_v2::MIN_DECODE_AUDIO_SECS,
                "recording": capture.record_path().map(|p| p.display().to_string()),
                "pin_wpm": pin_wpm,
            }),
        );
    } else {
        println!(
            "Live streaming (v2) from: {} @ {} Hz; decode every {} ms; pin_wpm={}",
            capture.device_name,
            capture.sample_rate,
            decode_every_ms,
            pin_wpm
                .map(|w| format!("{w:.1}"))
                .unwrap_or_else(|| "auto".into())
        );
    }

    let started = Instant::now();
    let mut last_drain_at: u64 = 0;
    let mut last_decode_at = Instant::now();
    let decode_period = Duration::from_millis(decode_every_ms);
    let mut last_transcript: Option<String> = None;
    let mut last_wpm_emitted: Option<f32> = None;
    let mut last_lock_emitted: Option<streaming_v2::LockState> = None;

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if seconds > 0.0 && started.elapsed().as_secs_f32() >= seconds {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));

        if let Some(rx) = cfg_channel.as_ref() {
            // v2 currently honors only ResetLock (clears the audio
            // buffer so the operator can start a fresh QSO). Config
            // updates are no-ops because v2 has no per-decode tunables.
            while let Ok(msg) = rx.try_recv() {
                if matches!(msg, StdinControlMessage::ResetLock) {
                    decoder.reset();
                    last_drain_at = capture.buffer.lock().written;
                    last_transcript = None;
                    last_wpm_emitted = None;
                    last_lock_emitted = None;
                    if let Some(em) = emitter.as_mut() {
                        em.emit(
                            started.elapsed().as_secs_f32(),
                            serde_json::json!({"type": "reset_ack"}),
                        );
                    }
                }
            }
        }

        let chunk = {
            let lock = capture.buffer.lock();
            let total = lock.written;
            let avail = lock.len();
            let want = (total - last_drain_at).min(avail as u64) as usize;
            if want == 0 {
                Vec::new()
            } else {
                let snap = lock.snapshot();
                last_drain_at = total;
                let start = snap.len() - want;
                snap[start..].to_vec()
            }
        };
        if !chunk.is_empty() {
            decoder.feed(&chunk);
        }

        if last_decode_at.elapsed() < decode_period {
            continue;
        }
        last_decode_at = Instant::now();

        let snap = match decoder.decode()? {
            Some(s) => s,
            None => continue,
        };

        let t = started.elapsed().as_secs_f32();
        if let Some(em) = emitter.as_mut() {
            em.emit(
                t,
                serde_json::json!({
                    "type": "transcript",
                    "text": snap.text,
                    "chars": snap.text.chars().count(),
                    "wpm": snap.wpm,
                    "decode_ms": snap.decode_ms,
                    "audio_secs": snap.audio_secs,
                    "lock": snap.lock.as_str(),
                }),
            );
            if last_wpm_emitted
                .map(|w| (w - snap.wpm).abs() >= 0.5)
                .unwrap_or(true)
            {
                em.emit(t, serde_json::json!({"type": "wpm", "wpm": snap.wpm}));
                last_wpm_emitted = Some(snap.wpm);
            }
            if last_lock_emitted != Some(snap.lock) {
                em.emit(
                    t,
                    serde_json::json!({"type": "lock", "state": snap.lock.as_str()}),
                );
                last_lock_emitted = Some(snap.lock);
            }
        } else {
            println!(
                "[t={:>6.2}s audio={:>6.1}s decode={:>4}ms wpm={:>5.1} lock={}] {}",
                t,
                snap.audio_secs,
                snap.decode_ms,
                snap.wpm,
                snap.lock.as_str(),
                snap.text
            );
        }
        last_transcript = Some(snap.text);
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
                "transcript": last_transcript.unwrap_or_default(),
                "recording": recording_saved.or(recording_path),
            }),
        );
        return Ok(());
    }

    println!();
    println!("Final transcript (v2):");
    println!("{}", last_transcript.unwrap_or_default());
    Ok(())
}

fn ctrlc_setup<F: FnMut() + Send + 'static>(_f: F) {
    // Best-effort: no ctrlc crate, rely on terminal interrupt for now.
}

/// Control messages produced by [`spawn_stdin_config_channel`] for the
/// streaming-decoder consumer loops. Either a runtime config update or
/// an operator-driven force-reset of the current pitch lock (used by
/// the GUI's "new QSO" / F7 binding).
#[derive(Debug, Clone)]
pub enum StdinControlMessage {
    /// Live decoder configuration update.
    Config(streaming::DecoderConfig),
    /// Drop the active pitch lock and resume hunting. Triggered by an
    /// `{"type":"reset_lock"}` line on stdin.
    ResetLock,
}

/// Outcome of parsing one stdin control line. Pulled out as its own
/// function so the parser can be regression-tested without spawning a
/// real reader thread or wiring stdin redirection.
#[derive(Debug)]
pub(crate) enum StdinParseOutcome {
    /// Line should be ignored (empty / unrecognized JSON / malformed).
    Skip,
    /// Operator requested graceful shutdown via a literal `stop` line.
    /// The reader loop must set the stop atomic and break.
    Stop,
    /// A control message that the decoder loop should observe.
    Message(StdinControlMessage),
}

#[cfg(test)]
impl StdinParseOutcome {
    fn is_stop(&self) -> bool {
        matches!(self, StdinParseOutcome::Stop)
    }
    fn is_skip(&self) -> bool {
        matches!(self, StdinParseOutcome::Skip)
    }
    fn is_reset_lock(&self) -> bool {
        matches!(
            self,
            StdinParseOutcome::Message(StdinControlMessage::ResetLock)
        )
    }
}

/// Parse a single line received on the V3/streaming control stdin.
///
/// `state` is the current cumulative [`streaming::DecoderConfig`]; we
/// mutate it in place so omitted fields keep their previous value.
///
/// Critical invariant: a literal `stop` line MUST yield
/// [`StdinParseOutcome::Stop`] so the reader loop can finalize the WAV
/// recording. Without that, the GUI's graceful Stop signal is silently
/// dropped here, the GUI eventually falls back to `Process.Kill`,
/// `LiveCapture::drop` never runs, and every saved capture lands on
/// disk with `riffSize=0/dataSize=0` — making it unusable for offline
/// replay or regression scoring (see
/// `parser_treats_literal_stop_line_as_stop` test).
pub(crate) fn parse_stdin_control_line(
    state: &mut streaming::DecoderConfig,
    line: &str,
) -> StdinParseOutcome {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return StdinParseOutcome::Skip;
    }
    if trimmed.eq_ignore_ascii_case("stop") {
        return StdinParseOutcome::Stop;
    }
    let v: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return StdinParseOutcome::Skip,
    };
    match v.get("type").and_then(|t| t.as_str()) {
        Some("reset_lock") => return StdinParseOutcome::Message(StdinControlMessage::ResetLock),
        Some("config") => {}
        _ => return StdinParseOutcome::Skip,
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
    StdinParseOutcome::Message(StdinControlMessage::Config(*state))
}

/// Spawn a background thread that reads NDJSON control lines from stdin
/// and forwards parsed [`StdinControlMessage`] values to the returned
/// receiver. Lines that don't parse as a recognized command are
/// silently ignored so unknown messages don't crash the decoder.
///
/// Wire format (one JSON object per line):
///   {"type":"config","min_snr_db":6.0,"pitch_min_snr_db":8.0,"threshold_scale":1.0}
///   {"type":"reset_lock"}
///   stop
///
/// For `config`, any field may be omitted; omitted fields keep their
/// previous value.
fn spawn_stdin_config_channel(
    stop_on_eof: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> std::sync::mpsc::Receiver<StdinControlMessage> {
    use std::io::BufRead;
    let (tx, rx) = std::sync::mpsc::channel::<StdinControlMessage>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut state = streaming::DecoderConfig::defaults();
        for line in stdin.lock().lines().map_while(Result::ok) {
            match parse_stdin_control_line(&mut state, &line) {
                StdinParseOutcome::Skip => continue,
                StdinParseOutcome::Stop => {
                    if let Some(stop) = stop_on_eof.as_ref() {
                        stop.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    break;
                }
                StdinParseOutcome::Message(msg) => {
                    if tx.send(msg).is_err() {
                        break;
                    }
                }
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

fn morse_for_char(c: char) -> Option<&'static str> {
    Some(match c.to_ascii_uppercase() {
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
        '.' => ".-.-.-",
        ',' => "--..--",
        '?' => "..--..",
        '/' => "-..-.",
        '=' => "-...-",
        '+' => ".-.-.",
        '-' => "-....-",
        _ => return None,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_gen_rough_fist(
    output: &std::path::Path,
    text: &str,
    wpm: f32,
    pitch_hz: f32,
    sample_rate: u32,
    jitter: f32,
    dah_ratio: f32,
    dit_weight: f32,
    noise: f32,
    seed: u64,
) -> Result<()> {
    use std::f32::consts::TAU;
    // Cheap deterministic LCG so we don't add a `rand` dep.
    let mut rng_state = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let mut rand_unit = || -> f32 {
        rng_state = rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let r = ((rng_state >> 33) as u32) as f32 / u32::MAX as f32;
        // Map [0,1) -> [-1,1)
        2.0 * r - 1.0
    };
    let jitter_mul = |base: f32, rng: &mut dyn FnMut() -> f32| -> f32 {
        let factor = (1.0 + jitter * rng()).max(0.1);
        base * factor
    };

    let dot_secs = 1.2 / wpm;
    let ramp_n = ((sample_rate as f32) * 0.005) as usize;
    let mut samples: Vec<f32> = Vec::new();
    let mut t: usize = 0;

    // Leading silence so the decoder has time to lock pitch.
    let lead_n = (sample_rate as f32 * 0.5) as usize;
    samples.resize(lead_n, 0.0);

    let mut canonical_text = String::new();

    let words: Vec<&str> = text.split_whitespace().collect();
    for (wi, w) in words.iter().enumerate() {
        let mut word_chars = String::new();
        let mut word_emitted = false;
        for c in w.chars() {
            let code = match morse_for_char(c) {
                Some(s) => s,
                None => continue,
            };
            if word_emitted {
                // Inter-character gap: 3 dot units (jittered).
                let gap_secs = jitter_mul(dot_secs * 3.0, &mut rand_unit);
                let n = (gap_secs * sample_rate as f32) as usize;
                samples.resize(samples.len() + n, 0.0);
            }
            word_emitted = true;
            word_chars.push(c.to_ascii_uppercase());

            let mut first_elem = true;
            for el in code.chars() {
                if !first_elem {
                    let gap_secs = jitter_mul(dot_secs, &mut rand_unit);
                    let n = (gap_secs * sample_rate as f32) as usize;
                    samples.resize(samples.len() + n, 0.0);
                }
                first_elem = false;
                let base = if el == '.' {
                    dot_secs * dit_weight
                } else {
                    dot_secs * dah_ratio
                };
                let on_secs = jitter_mul(base, &mut rand_unit);
                let n = (on_secs * sample_rate as f32) as usize;
                for k in 0..n {
                    let env = {
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
                    };
                    let s = (TAU * pitch_hz * (t as f32) / sample_rate as f32).sin() * 0.6 * env;
                    samples.push(s);
                    t += 1;
                }
            }
        }
        if !word_chars.is_empty() {
            if wi > 0 {
                canonical_text.push(' ');
            }
            canonical_text.push_str(&word_chars);
        }
        if wi + 1 < words.len() && word_emitted {
            // Inter-word gap: 7 dot units (jittered).
            let gap_secs = jitter_mul(dot_secs * 7.0, &mut rand_unit);
            let n = (gap_secs * sample_rate as f32) as usize;
            samples.resize(samples.len() + n, 0.0);
        }
    }

    // Trailing silence.
    let tail_n = (sample_rate as f32 * 0.5) as usize;
    samples.resize(samples.len() + tail_n, 0.0);

    if noise > 0.0 {
        for s in samples.iter_mut() {
            *s = (*s + noise * rand_unit()).clamp(-1.0, 1.0);
        }
    }

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(output, spec)
        .with_context(|| format!("creating WAV {}", output.display()))?;
    for s in &samples {
        let v = (s * i16::MAX as f32) as i16;
        writer.write_sample(v).context("writing sample")?;
    }
    writer.finalize().context("finalising WAV")?;

    let truth_path = output.with_extension("truth.txt");
    std::fs::write(&truth_path, canonical_text.as_bytes())
        .with_context(|| format!("writing truth {}", truth_path.display()))?;

    let dur_s = samples.len() as f32 / sample_rate as f32;
    println!(
        "Wrote {} ({:.2}s, {} WPM, jitter±{:.0}%, dah/dit={:.2}, dit_weight={:.2}, noise={:.2})",
        output.display(),
        dur_s,
        wpm,
        jitter * 100.0,
        dah_ratio,
        dit_weight,
        noise,
    );
    println!("Truth: {}", truth_path.display());
    println!("Text:  {canonical_text}");
    Ok(())
}

fn run_stream_live_v3(
    device: Option<&str>,
    seconds: f32,
    json: bool,
    decode_every_ms: u64,
    record_path: Option<&std::path::Path>,
    stdin_control: bool,
    loopback: bool,
    pin_wpm: Option<f32>,
    pin_hz: Option<f32>,
    min_snr_db: f32,
    file: Option<&std::path::Path>,
    play_file_audio: bool,
    multi_pitch: usize,
) -> Result<()> {
    if let Some(path) = file {
        return run_stream_live_v3_file(
            path,
            seconds,
            json,
            decode_every_ms,
            pin_wpm,
            pin_hz,
            min_snr_db,
            play_file_audio,
            multi_pitch,
        );
    }
    use cw_decoder_poc::envelope_decoder::{
        LiveEnvelopeStreamer, LiveMultiPitchStreamer, VizEventKind, MAX_VIZ_ENVELOPE_SAMPLES,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    let capture = if loopback {
        audio::open_loopback_with_recording(device, 1.0, record_path)?
    } else {
        audio::open_input_with_recording(device, 1.0, record_path)?
    };
    let new_streamer = || {
        let mut streamer = LiveEnvelopeStreamer::new(capture.sample_rate);
        streamer.set_pinned_hz(pin_hz);
        streamer.set_pinned_wpm(pin_wpm);
        streamer.set_min_snr_db(min_snr_db);
        streamer
    };
    let mut streamer = new_streamer();

    let new_multi = || {
        if multi_pitch == 0 {
            None
        } else {
            let mut s = LiveMultiPitchStreamer::new(capture.sample_rate, multi_pitch);
            s.set_pinned_wpm(pin_wpm);
            s.set_min_snr_db(min_snr_db);
            Some(s)
        }
    };
    let mut multi_streamer = new_multi();

    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = Arc::clone(&stop);
        ctrlc_setup(move || {
            stop.store(true, Ordering::Relaxed);
        });
    }
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
                "source": "live-v3",
                "device": capture.device_name,
                "rate": capture.sample_rate,
                "decode_every_ms": decode_every_ms,
                "max_viz_envelope_samples": MAX_VIZ_ENVELOPE_SAMPLES,
                "recording": capture.record_path().map(|p| p.display().to_string()),
                "pin_wpm": pin_wpm,
            }),
        );
    } else {
        println!(
            "Live streaming (v3 envelope+viz) from: {} @ {} Hz; decode every {} ms",
            capture.device_name, capture.sample_rate, decode_every_ms
        );
    }

    let started = Instant::now();
    let mut last_drain_at: u64 = 0;
    let mut last_decode_at = Instant::now();
    let decode_period = Duration::from_millis(decode_every_ms);
    let decode_every_s = decode_every_ms as f32 / 1000.0;
    let mut commit_cursor =
        ditdah_streaming::LiveCommitCursor::new(MAX_V3_SESSION_TRANSCRIPT_CHARS);
    let mut append_decoder = cw_decoder_poc::append_decode::AppendEventDecoder::new();
    let mut diag = DiagWriter::from_env();

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if seconds > 0.0 && started.elapsed().as_secs_f32() >= seconds {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));

        if let Some(rx) = cfg_channel.as_ref() {
            while let Ok(msg) = rx.try_recv() {
                if matches!(msg, StdinControlMessage::ResetLock) {
                    streamer = new_streamer();
                    multi_streamer = new_multi();
                    last_drain_at = capture.buffer.lock().written;
                    commit_cursor.reset_all();
                    append_decoder = cw_decoder_poc::append_decode::AppendEventDecoder::new();
                    if let Some(em) = emitter.as_mut() {
                        em.emit(
                            started.elapsed().as_secs_f32(),
                            serde_json::json!({"type": "reset_ack"}),
                        );
                    }
                }
            }
        }

        let chunk = {
            let lock = capture.buffer.lock();
            let total = lock.written;
            let avail = lock.len();
            let want = (total - last_drain_at).min(avail as u64) as usize;
            if want == 0 {
                Vec::new()
            } else {
                let snap = lock.snapshot();
                last_drain_at = total;
                let start = snap.len() - want;
                snap[start..].to_vec()
            }
        };
        if !chunk.is_empty() {
            // Buffer the audio without forcing a viz decode (cheap path).
            streamer.feed(&chunk);
            if let Some(m) = multi_streamer.as_mut() {
                let _ = m.feed(&chunk);
            }
        }

        if last_decode_at.elapsed() < decode_period {
            continue;
        }
        last_decode_at = Instant::now();

        // Force a viz-producing decode now.
        let snap = streamer.flush_with_viz();
        let t = started.elapsed().as_secs_f32();
        // Approach A+: event-driven commit cursor (sample-indexed,
        // idempotent across re-decodes) replaces the old
        // string-stitching path that produced ghost-repeats like
        // "TSA USA EE   SA USA EE   ..." when the rolling window's
        // first character drifted between cycles.
        let commit = if let Some(viz) = snap.viz.as_ref() {
            commit_cursor.update_from_viz(viz, decode_every_s)
        } else {
            ditdah_streaming::CommitUpdate::default()
        };
        let append = if let Some(viz) = snap.viz.as_ref() {
            append_decoder.ingest_viz(viz)
        } else {
            cw_decoder_poc::append_decode::AppendDecodeUpdate::default()
        };
        let session_transcript: String = format!(
            "{}{}{}",
            commit.committed_text,
            if !commit.committed_text.is_empty()
                && !commit.provisional_tail.is_empty()
                && !commit.committed_text.ends_with(' ')
            {
                " "
            } else {
                ""
            },
            commit.provisional_tail
        );
        let stitched = !commit.committed_text.is_empty() || !commit.provisional_tail.is_empty();
        let appended_session = String::new();
        if let Some(d) = diag.as_mut() {
            d.record(
                t,
                &snap,
                stitched,
                &appended_session,
                &session_transcript,
                "live",
            );
        }

        if let Some(em) = emitter.as_mut() {
            em.emit(
                t,
                serde_json::json!({
                    "type": "transcript",
                    "text": append.decoded_text,
                    "appended": appended_session,
                    "transcript": append.decoded_text,
                    "committed": commit.committed_text,
                    "provisional": commit.provisional_tail,
                    "cursor_transcript": session_transcript.clone(),
                    "raw_morse": append.raw_stream,
                    "window_text": snap.transcript,
                    "wpm": snap.wpm,
                }),
            );
            if let Some(viz) = snap.viz {
                let events_json: Vec<serde_json::Value> = viz
                    .events
                    .iter()
                    .map(|e| {
                        let kind = match e.kind {
                            VizEventKind::OnDit => "on_dit",
                            VizEventKind::OnDah => "on_dah",
                            VizEventKind::OffIntra => "off_intra",
                            VizEventKind::OffChar => "off_char",
                            VizEventKind::OffWord => "off_word",
                        };
                        serde_json::json!({
                            "start_s": e.start_s,
                            "end_s": e.end_s,
                            "duration_s": e.duration_s,
                            "kind": kind,
                        })
                    })
                    .collect();
                em.emit(
                    t,
                    serde_json::json!({
                        "type": "viz",
                        "sample_rate": viz.sample_rate,
                        "window_start_sample": viz.window_start_sample,
                        "window_end_sample": viz.window_end_sample,
                        "buffer_seconds": viz.buffer_seconds,
                        "frame_step_s": viz.frame_step_s,
                        "pitch_hz": viz.pitch_hz,
                        "envelope": viz.envelope,
                        "envelope_max": viz.envelope_max,
                        "noise_floor": viz.noise_floor,
                        "signal_floor": viz.signal_floor,
                        "snr_db": viz.snr_db,
                        "snr_suppressed": viz.snr_suppressed,
                        "hyst_high": viz.hyst_high,
                        "hyst_low": viz.hyst_low,
                        "events": events_json,
                        "on_durations": viz.on_durations,
                        "dot_seconds": viz.dot_seconds,
                        "wpm": viz.wpm,
                        "wpm_kmeans": viz.wpm_kmeans,
                        "centroid_dot": viz.centroid_dot,
                        "centroid_dah": viz.centroid_dah,
                        "locked_wpm": viz.locked_wpm,
                    }),
                );
            }
            // Multi-pitch event: emit alongside the single-track
            // events so back-compat consumers keep working.
            if let Some(m) = multi_streamer.as_mut() {
                let snaps = m.flush_with_viz();
                if !snaps.is_empty() {
                    let tracks_json: Vec<serde_json::Value> = snaps
                        .iter()
                        .map(|s| {
                            serde_json::json!({
                                "track_id": s.track_id,
                                "pitch_hz": s.pitch_hz,
                                "wpm": s.wpm,
                                "transcript": s.transcript,
                                "appended": s.appended,
                            })
                        })
                        .collect();
                    em.emit(
                        t,
                        serde_json::json!({
                            "event": "multi_track_transcript",
                            "type": "multi_track_transcript",
                            "tracks": tracks_json,
                        }),
                    );
                }
            }
        } else {
            println!("[t={:>6.2}s wpm={:>5.1}] {}", t, snap.wpm, snap.transcript);
            if let Some(m) = multi_streamer.as_mut() {
                let snaps = m.flush();
                for s in snaps {
                    println!(
                        "  track {} @ {:>5.1} Hz wpm={:>5.1}: {}",
                        s.track_id, s.pitch_hz, s.wpm, s.transcript
                    );
                }
            }
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
                "transcript": commit_cursor.committed_text(),
                "committed": commit_cursor.committed_text(),
                "recording": recording_saved.or(recording_path),
            }),
        );
    } else {
        println!();
        println!("Final transcript (v3):");
        println!("{}", commit_cursor.committed_text());
    }
    Ok(())
}

fn run_stream_live_v3_file(
    path: &std::path::Path,
    seconds: f32,
    json: bool,
    decode_every_ms: u64,
    pin_wpm: Option<f32>,
    pin_hz: Option<f32>,
    min_snr_db: f32,
    play_file_audio: bool,
    multi_pitch: usize,
) -> Result<()> {
    use cw_decoder_poc::envelope_decoder::{
        LiveEnvelopeStreamer, LiveMultiPitchStreamer, VizEventKind, MAX_VIZ_ENVELOPE_SAMPLES,
    };
    use std::time::{Duration, Instant};

    let decoded = audio::decode_file(path)?;
    let sr = decoded.sample_rate;
    let total_samples = decoded.samples.len();
    let duration_s = total_samples as f32 / sr as f32;
    let playback = if play_file_audio {
        Some(
            audio::play_samples_with_control(decoded.samples.clone(), sr)
                .context("starting file playback")?,
        )
    } else {
        None
    };
    let mut streamer = LiveEnvelopeStreamer::new(sr);
    streamer.set_pinned_hz(pin_hz);
    streamer.set_pinned_wpm(pin_wpm);
    streamer.set_min_snr_db(min_snr_db);

    let mut multi_streamer = if multi_pitch == 0 {
        None
    } else {
        let mut s = LiveMultiPitchStreamer::new(sr, multi_pitch);
        s.set_pinned_wpm(pin_wpm);
        s.set_min_snr_db(min_snr_db);
        Some(s)
    };

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
                "source": "live-v3-file",
                "device": format!("file:{}", path.display()),
                "rate": sr,
                "playback_device": playback.as_ref().map(|p| p.device_name.as_str()),
                "decode_every_ms": decode_every_ms,
                "max_viz_envelope_samples": MAX_VIZ_ENVELOPE_SAMPLES,
                "recording": serde_json::Value::Null,
                "pin_wpm": pin_wpm,
                "duration_s": duration_s,
            }),
        );
    } else {
        println!(
            "Streaming file (v3 envelope+viz): {} @ {} Hz ({:.1}s); decode every {} ms",
            path.display(),
            sr,
            duration_s,
            decode_every_ms
        );
    }

    let started = Instant::now();
    let mut last_decode_at = Instant::now();
    let decode_period = Duration::from_millis(decode_every_ms);
    let chunk_period = Duration::from_millis(50);
    let chunk_samples = ((sr as u64 * 50) / 1000) as usize;
    let pump_period = if playback.is_some() {
        Duration::from_millis(8)
    } else {
        chunk_period
    };
    let mut cursor = 0usize;
    let decode_every_s = decode_every_ms as f32 / 1000.0;
    let mut commit_cursor =
        ditdah_streaming::LiveCommitCursor::new(MAX_V3_SESSION_TRANSCRIPT_CHARS);
    let mut append_decoder = cw_decoder_poc::append_decode::AppendEventDecoder::new();
    let mut diag = DiagWriter::from_env();

    while cursor < total_samples {
        if seconds > 0.0 && started.elapsed().as_secs_f32() >= seconds {
            break;
        }
        std::thread::sleep(pump_period);
        let end = if let Some(p) = playback.as_ref() {
            (p.position_input_frames() as usize).min(total_samples)
        } else {
            (cursor + chunk_samples).min(total_samples)
        };
        if end > cursor {
            streamer.feed(&decoded.samples[cursor..end]);
            if let Some(m) = multi_streamer.as_mut() {
                let _ = m.feed(&decoded.samples[cursor..end]);
            }
            cursor = end;
        }

        if last_decode_at.elapsed() < decode_period {
            continue;
        }
        last_decode_at = Instant::now();

        let snap = streamer.flush_with_viz();
        let t = started.elapsed().as_secs_f32();
        // Approach A+: drive the event-driven commit cursor instead of
        // string-stitching. The cursor is sample-indexed and idempotent
        // across re-decodes of the same audio region, so the
        // "TSA USA EE   SA USA EE   ..." ghost-repeat pattern that the
        // old `append_snapshot_text` produced is structurally
        // impossible.
        let commit = if let Some(viz) = snap.viz.as_ref() {
            commit_cursor.update_from_viz(viz, decode_every_s)
        } else {
            ditdah_streaming::CommitUpdate::default()
        };
        let append = if let Some(viz) = snap.viz.as_ref() {
            append_decoder.ingest_viz(viz)
        } else {
            cw_decoder_poc::append_decode::AppendDecodeUpdate::default()
        };
        let session_transcript: String = format!(
            "{}{}{}",
            commit.committed_text,
            if !commit.committed_text.is_empty()
                && !commit.provisional_tail.is_empty()
                && !commit.committed_text.ends_with(' ')
            {
                " "
            } else {
                ""
            },
            commit.provisional_tail
        );
        let stitched = !commit.committed_text.is_empty() || !commit.provisional_tail.is_empty();
        let appended_session = String::new();
        if let Some(d) = diag.as_mut() {
            d.record(
                t,
                &snap,
                stitched,
                &appended_session,
                &session_transcript,
                "file",
            );
        }

        if let Some(em) = emitter.as_mut() {
            em.emit(
                t,
                serde_json::json!({
                    "type": "transcript",
                    "text": append.decoded_text,
                    "appended": appended_session,
                    "transcript": append.decoded_text,
                    "committed": commit.committed_text,
                    "provisional": commit.provisional_tail,
                    "cursor_transcript": session_transcript.clone(),
                    "raw_morse": append.raw_stream,
                    "window_text": snap.transcript,
                    "wpm": snap.wpm,
                }),
            );
            if let Some(viz) = snap.viz {
                let events_json: Vec<serde_json::Value> = viz
                    .events
                    .iter()
                    .map(|e| {
                        let kind = match e.kind {
                            VizEventKind::OnDit => "on_dit",
                            VizEventKind::OnDah => "on_dah",
                            VizEventKind::OffIntra => "off_intra",
                            VizEventKind::OffChar => "off_char",
                            VizEventKind::OffWord => "off_word",
                        };
                        serde_json::json!({
                            "start_s": e.start_s,
                            "end_s": e.end_s,
                            "duration_s": e.duration_s,
                            "kind": kind,
                        })
                    })
                    .collect();
                em.emit(
                    t,
                    serde_json::json!({
                        "type": "viz",
                        "sample_rate": viz.sample_rate,
                        "window_start_sample": viz.window_start_sample,
                        "window_end_sample": viz.window_end_sample,
                        "buffer_seconds": viz.buffer_seconds,
                        "frame_step_s": viz.frame_step_s,
                        "pitch_hz": viz.pitch_hz,
                        "envelope": viz.envelope,
                        "envelope_max": viz.envelope_max,
                        "noise_floor": viz.noise_floor,
                        "signal_floor": viz.signal_floor,
                        "snr_db": viz.snr_db,
                        "snr_suppressed": viz.snr_suppressed,
                        "hyst_high": viz.hyst_high,
                        "hyst_low": viz.hyst_low,
                        "events": events_json,
                        "on_durations": viz.on_durations,
                        "dot_seconds": viz.dot_seconds,
                        "wpm": viz.wpm,
                        "wpm_kmeans": viz.wpm_kmeans,
                        "centroid_dot": viz.centroid_dot,
                        "centroid_dah": viz.centroid_dah,
                        "locked_wpm": viz.locked_wpm,
                    }),
                );
            }
            if let Some(m) = multi_streamer.as_mut() {
                let snaps = m.flush_with_viz();
                if !snaps.is_empty() {
                    let tracks_json: Vec<serde_json::Value> = snaps
                        .iter()
                        .map(|s| {
                            serde_json::json!({
                                "track_id": s.track_id,
                                "pitch_hz": s.pitch_hz,
                                "wpm": s.wpm,
                                "transcript": s.transcript,
                                "appended": s.appended,
                            })
                        })
                        .collect();
                    em.emit(
                        t,
                        serde_json::json!({
                            "event": "multi_track_transcript",
                            "type": "multi_track_transcript",
                            "tracks": tracks_json,
                        }),
                    );
                }
            }
        } else {
            println!("[t={:>6.2}s wpm={:>5.1}] {}", t, snap.wpm, snap.transcript);
            if let Some(m) = multi_streamer.as_mut() {
                let snaps = m.flush();
                for s in snaps {
                    println!(
                        "  track {} @ {:>5.1} Hz wpm={:>5.1}: {}",
                        s.track_id, s.pitch_hz, s.wpm, s.transcript
                    );
                }
            }
        }
    }

    if let Some(em) = emitter.as_mut() {
        em.emit(
            started.elapsed().as_secs_f32(),
            serde_json::json!({
                "type": "end",
                "transcript": commit_cursor.committed_text(),
                "committed": commit_cursor.committed_text(),
                "recording": serde_json::Value::Null,
            }),
        );
    } else {
        println!();
        println!("Final transcript (v3 file):");
        println!("{}", commit_cursor.committed_text());
    }
    Ok(())
}

/// Returns true when the per-cycle decoder snapshot is trustworthy
/// enough to be stitched into the persistent session transcript.
///
/// The streamer enters `LOCKED` (i.e. `viz.locked_wpm = Some(_)`) only
/// after `lock_after_elements` consistent symbol observations. Cycles
/// before that point are still useful for the live envelope viz but
/// their `text` is typically classifier garbage from the keying
/// hysteresis sliding around. SNR-suppressed cycles are also skipped
/// even if locked, because by definition they emitted no transcript.
///
/// This intentionally does NOT inspect the snapshot text itself — the
/// "operator sees what the decoder is making" principle from PR #362
/// still applies to *which characters* end up in the session. We are
/// only filtering *which cycles* are allowed to contribute.
#[allow(dead_code)] // Retained for legacy tests; V3 now uses LiveCommitCursor.
fn should_stitch_to_session(snap: &cw_decoder_poc::envelope_decoder::LiveEnvelopeSnapshot) -> bool {
    let Some(viz) = snap.viz.as_ref() else {
        return false;
    };
    viz.locked_wpm.is_some() && !viz.snr_suppressed
}

/// Per-cycle diagnostic recorder for the V3 live path.
///
/// TEMPORARY — added under #364 follow-up to gather rich data before
/// hypothesizing about why locked windows still drop characters into
/// the session transcript. Remove or gate behind a build flag once the
/// transcript-fidelity work is done.
///
/// Activated by setting the env var `QSORIPPER_CW_DIAG_LOG` to a file
/// path before launching `cw-decoder.exe stream-live-v3` (directly or
/// via the GUI). One JSON line per decode cycle is appended.
struct DiagWriter {
    file: std::fs::File,
    cycle: u64,
}

impl DiagWriter {
    fn from_env() -> Option<Self> {
        let path = std::env::var("QSORIPPER_CW_DIAG_LOG").ok()?;
        let path = path.trim();
        if path.is_empty() {
            return None;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| eprintln!("[diag] cannot open {path}: {e}"))
            .ok()?;
        eprintln!("[diag] writing per-cycle diagnostics to {path}");
        Some(Self { file, cycle: 0 })
    }

    #[allow(clippy::too_many_arguments)]
    fn record(
        &mut self,
        t_seconds: f32,
        snap: &cw_decoder_poc::envelope_decoder::LiveEnvelopeSnapshot,
        stitched: bool,
        appended_session: &str,
        session_transcript: &str,
        source: &str,
    ) {
        use std::io::Write;
        let viz = snap.viz.as_ref();
        let locked = viz.is_some_and(|v| v.locked_wpm.is_some());
        let locked_wpm = viz.and_then(|v| v.locked_wpm);
        let snr_db = viz.map(|v| v.snr_db).unwrap_or(0.0);
        let snr_suppressed = viz.is_some_and(|v| v.snr_suppressed);
        let pitch_hz = viz.map(|v| v.pitch_hz).unwrap_or(0.0);
        let signal_floor = viz.map(|v| v.signal_floor).unwrap_or(0.0);
        let noise_floor = viz.map(|v| v.noise_floor).unwrap_or(0.0);
        let envelope_max = viz.map(|v| v.envelope_max).unwrap_or(0.0);
        let dyn_range = if envelope_max > 0.0 {
            (signal_floor - noise_floor) / envelope_max
        } else {
            0.0
        };
        let dot_seconds = viz.map(|v| v.dot_seconds).unwrap_or(0.0);
        let viz_wpm = viz.map(|v| v.wpm).unwrap_or(0.0);
        let n_events = viz.map(|v| v.events.len()).unwrap_or(0);
        let n_on_durations = viz.map(|v| v.on_durations.len()).unwrap_or(0);
        let session_len = session_transcript.chars().count();
        let line = serde_json::json!({
            "type": "diag_cycle",
            "source": source,
            "cycle": self.cycle,
            "t_s": t_seconds,
            "snap": {
                "text": snap.transcript,
                "appended": snap.appended,
                "wpm": snap.wpm,
            },
            "viz": {
                "locked": locked,
                "locked_wpm": locked_wpm,
                "snr_db": snr_db,
                "snr_suppressed": snr_suppressed,
                "dyn_range_ratio": dyn_range,
                "signal_floor": signal_floor,
                "noise_floor": noise_floor,
                "envelope_max": envelope_max,
                "pitch_hz": pitch_hz,
                "dot_seconds": dot_seconds,
                "wpm": viz_wpm,
                "n_events": n_events,
                "n_on_durations": n_on_durations,
            },
            "session": {
                "stitched": stitched,
                "appended": appended_session,
                "len_chars": session_len,
            },
        });
        if let Err(e) = writeln!(self.file, "{line}") {
            eprintln!("[diag] write failed: {e}");
        }
        self.cycle += 1;
    }
}

/// Maximum characters retained in the V3 visualizer's session transcript.
/// When exceeded, [`cap_session_transcript`] trims back to ~80% on a
/// whitespace boundary so old garbage is evicted as fresh copy lands.
const MAX_V3_SESSION_TRANSCRIPT_CHARS: usize = 12_000;

/// Cap the running session transcript at `max_chars`. When over the limit,
/// keep roughly the last 80% of the buffer, snapping the trim point to the
/// nearest whitespace so we don't shear a token in half.
#[allow(dead_code)] // Retained for legacy tests; V3 now uses LiveCommitCursor.
fn cap_session_transcript(transcript: &mut String, max_chars: usize) {
    if transcript.chars().count() <= max_chars {
        return;
    }
    let target = (max_chars * 4) / 5;
    let total = transcript.chars().count();
    let drop_chars = total.saturating_sub(target);
    let mut byte_cut = 0usize;
    for (i, (b, _)) in transcript.char_indices().enumerate() {
        if i >= drop_chars {
            byte_cut = b;
            break;
        }
    }
    let tail = &transcript[byte_cut..];
    let trimmed = match tail.find(char::is_whitespace) {
        Some(ws) => tail[ws..].trim_start().to_string(),
        None => tail.to_string(),
    };
    *transcript = trimmed;
}

#[cfg(test)]
mod v3_session_transcript_tests {
    use super::*;
    use cw_decoder_poc::envelope_decoder;

    #[test]
    fn rolling_snapshots_grow_session_via_token_overlap() {
        let mut session = String::new();
        ditdah_streaming::append_snapshot_text(&mut session, "CQ DE K7XYZ");
        ditdah_streaming::append_snapshot_text(&mut session, "DE K7XYZ K7XYZ");
        ditdah_streaming::append_snapshot_text(&mut session, "K7XYZ K UR RST");
        // The session grows monotonically and contains the original tokens.
        assert!(session.contains("CQ"));
        assert!(session.contains("K7XYZ"));
        assert!(session.contains("RST"));
    }

    #[test]
    fn rolling_snapshots_dont_double_repeat_callsigns() {
        let mut session = String::new();
        ditdah_streaming::append_snapshot_text(&mut session, "CQ DE K7XYZ");
        ditdah_streaming::append_snapshot_text(&mut session, "CQ DE K7XYZ");
        // Repeating the same snapshot should NOT double the content.
        let count = session.matches("K7XYZ").count();
        assert_eq!(count, 1, "session was: {session:?}");
    }

    #[test]
    fn unrelated_garbage_snapshot_is_appended_not_filtered() {
        // Explicit anti-anchor test: we deliberately do NOT gate or filter
        // garbage snapshots. The operator sees what the decoder is making.
        let mut session = String::new();
        ditdah_streaming::append_snapshot_text(&mut session, "CQ DE K7XYZ");
        ditdah_streaming::append_snapshot_text(&mut session, "?Q??E??ZZ");
        assert!(
            session.contains("?Q??E??ZZ") || session.len() > "CQ DE K7XYZ".len(),
            "garbage snapshot should reach the session unmodified, got: {session:?}",
        );
    }

    #[test]
    fn cap_session_transcript_keeps_recent_tail_on_word_boundary() {
        let mut s = ('A'..='Z')
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        // Repeat to exceed the cap.
        let mut big = String::new();
        for _ in 0..600 {
            big.push_str(&s);
            big.push(' ');
        }
        s = big;
        let original_len = s.chars().count();
        assert!(original_len > 12_000);
        cap_session_transcript(&mut s, 12_000);
        assert!(s.chars().count() < original_len);
        assert!(s.chars().count() <= 12_000);
        // Trim should snap to whitespace so first chunk isn't a half token.
        assert!(s.chars().next().is_some_and(|c| c.is_ascii_uppercase()));
    }

    #[test]
    fn cap_session_transcript_noop_when_under_limit() {
        let original = "CQ DE K7XYZ".to_string();
        let mut s = original.clone();
        cap_session_transcript(&mut s, 12_000);
        assert_eq!(s, original);
    }

    fn make_snapshot(
        text: &str,
        viz: Option<envelope_decoder::VizFrame>,
    ) -> envelope_decoder::LiveEnvelopeSnapshot {
        envelope_decoder::LiveEnvelopeSnapshot {
            transcript: text.to_string(),
            appended: text.to_string(),
            wpm: 20.0,
            viz,
        }
    }

    fn make_viz(locked: bool, suppressed: bool) -> envelope_decoder::VizFrame {
        envelope_decoder::VizFrame {
            sample_rate: 8000,
            frame_step_s: 0.005,
            buffer_seconds: 1.0,
            pitch_hz: 700.0,
            envelope: vec![],
            envelope_max: 1.0,
            noise_floor: 0.05,
            signal_floor: 0.5,
            snr_db: 20.0,
            snr_suppressed: suppressed,
            hyst_high: 0.55,
            hyst_low: 0.35,
            events: vec![],
            on_durations: vec![],
            dot_seconds: 0.06,
            wpm: 20.0,
            wpm_kmeans: 20.0,
            centroid_dot: 0.06,
            centroid_dah: 0.18,
            locked_wpm: if locked { Some(20.0) } else { None },
            window_start_sample: 0,
            window_end_sample: 8000,
        }
    }

    #[test]
    fn should_stitch_to_session_requires_lock() {
        // ACQUIRING (locked_wpm = None) → no stitch even if the cycle
        // produced text. This filters the per-cycle classifier garbage
        // that PR #362 made visible in the operator-facing transcript.
        let snap = make_snapshot("US AS USE", Some(make_viz(false, false)));
        assert!(!should_stitch_to_session(&snap));
    }

    #[test]
    fn should_stitch_to_session_blocks_snr_suppressed_cycles() {
        // Even if the streamer is locked, an SNR-suppressed cycle
        // contributed nothing meaningful. Don't stitch (and avoid
        // race conditions where text is non-empty due to leftover
        // state but the gate fired).
        let snap = make_snapshot("CQ", Some(make_viz(true, true)));
        assert!(!should_stitch_to_session(&snap));
    }

    #[test]
    fn should_stitch_to_session_allows_locked_clean_cycles() {
        let snap = make_snapshot("CQ DE K7XYZ", Some(make_viz(true, false)));
        assert!(should_stitch_to_session(&snap));
    }

    #[test]
    fn should_stitch_to_session_skips_when_viz_absent() {
        // Snapshots produced by the non-viz path have no information
        // about lock state, so be conservative and skip.
        let snap = make_snapshot("CQ", None);
        assert!(!should_stitch_to_session(&snap));
    }
}

#[cfg(test)]
mod stdin_control_tests {
    use super::*;

    /// Critical regression: the GUI's graceful Stop signal arrives as a
    /// literal `stop\n` line, not as JSON. Without the `Stop` outcome,
    /// the V3 child never observes the request, the GUI eventually falls
    /// back to Process.Kill, LiveCapture::drop never runs, and every
    /// saved capture lands on disk with riffSize=0/dataSize=0 — making
    /// real-radio diagnostic captures unusable for offline replay /
    /// regression scoring.
    #[test]
    fn parser_treats_literal_stop_line_as_stop() {
        let mut state = streaming::DecoderConfig::defaults();
        assert!(parse_stdin_control_line(&mut state, "stop").is_stop());
        assert!(parse_stdin_control_line(&mut state, "  STOP  \r").is_stop());
        assert!(parse_stdin_control_line(&mut state, "Stop\n").is_stop());
    }

    #[test]
    fn parser_skips_blank_and_unknown_lines() {
        let mut state = streaming::DecoderConfig::defaults();
        assert!(parse_stdin_control_line(&mut state, "").is_skip());
        assert!(parse_stdin_control_line(&mut state, "    ").is_skip());
        assert!(parse_stdin_control_line(&mut state, "garbage{").is_skip());
        // Valid JSON without a recognized type is also a no-op.
        assert!(parse_stdin_control_line(&mut state, r#"{"hello":"world"}"#).is_skip());
    }

    #[test]
    fn parser_recognizes_reset_lock_and_config() {
        let mut state = streaming::DecoderConfig::defaults();
        assert!(parse_stdin_control_line(&mut state, r#"{"type":"reset_lock"}"#).is_reset_lock());
        match parse_stdin_control_line(
            &mut state,
            r#"{"type":"config","min_snr_db":12.5,"threshold_scale":1.5}"#,
        ) {
            StdinParseOutcome::Message(StdinControlMessage::Config(cfg)) => {
                assert!((cfg.min_snr_db - 12.5).abs() < 1e-3);
                assert!((cfg.threshold_scale - 1.5).abs() < 1e-3);
            }
            other => panic!("expected Config message, got {other:?}"),
        }
        // Cumulative state: a partial config message preserves prior values.
        match parse_stdin_control_line(&mut state, r#"{"type":"config","min_snr_db":3.0}"#) {
            StdinParseOutcome::Message(StdinControlMessage::Config(cfg)) => {
                assert!((cfg.min_snr_db - 3.0).abs() < 1e-3);
                assert!(
                    (cfg.threshold_scale - 1.5).abs() < 1e-3,
                    "threshold_scale should persist across config updates",
                );
            }
            other => panic!("expected Config message, got {other:?}"),
        }
    }
}

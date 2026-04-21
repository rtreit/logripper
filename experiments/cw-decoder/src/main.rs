use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

mod audio;
mod decoder;
mod log_capture;
mod tui;

#[derive(Parser, Debug)]
#[command(name = "cw-decoder", about = "QsoRipper CW PoC: ditdah-based decoder + live WPM")]
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

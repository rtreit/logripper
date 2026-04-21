use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

mod audio;
mod decoder;
mod json;
mod log_capture;
mod streaming;
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
        Cmd::StreamFile { path, chunk_ms, realtime, quiet, json } => {
            run_stream_file(&path, chunk_ms, realtime, quiet, json)
        }
        Cmd::StreamLive { device, seconds, json } => {
            run_stream_live(device.as_deref(), seconds, json)
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


fn run_stream_file(path: &std::path::Path, chunk_ms: u32, realtime: bool, quiet: bool, json: bool) -> Result<()> {
    use std::time::Instant;
    let audio = audio::decode_file(path).context("decoding audio file")?;
    let dur = audio.samples.len() as f32 / audio.sample_rate as f32;
    let mut emitter = if json { Some(json::JsonEmitter::new()) } else { None };
    if let Some(em) = emitter.as_mut() {
        em.emit(0.0, serde_json::json!({
            "type": "ready",
            "source": "file",
            "path": path.display().to_string(),
            "rate": audio.sample_rate,
            "duration": dur,
        }));
    } else {
        println!(
            "Streaming: {} ({} Hz, {:.2} s, {} samples)",
            path.display(), audio.sample_rate, dur, audio.samples.len()
        );
    }

    let mut decoder = streaming::StreamingDecoder::new(audio.sample_rate)?;
    let chunk_samples = ((audio.sample_rate as u64 * chunk_ms as u64) / 1000) as usize;
    let chunk_samples = chunk_samples.max(64);

    let mut transcript = String::new();
    let mut last_wpm: Option<f32> = None;
    let started = Instant::now();
    let mut consumed: usize = 0;

    for chunk in audio.samples.chunks(chunk_samples) {
        let t_in_audio = consumed as f32 / audio.sample_rate as f32;
        consumed += chunk.len();

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
                        println!("[t={:>6.2}s real+{:>4}ms] PITCH lock: {:.1} Hz", t_in_audio, lag_ms, pitch_hz);
                    }
                }
                streaming::StreamEvent::WpmUpdate { wpm } => {
                    let changed = last_wpm.map(|w| (w - wpm).abs() >= 1.0).unwrap_or(true);
                    if changed {
                        if !quiet {
                            println!("[t={:>6.2}s real+{:>4}ms] WPM    -> {:.1}", t_in_audio, lag_ms, wpm);
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
                        println!("[t={:>6.2}s real+{:>4}ms] ???  garbled morse: {}", t_in_audio, lag_ms, morse);
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
        em.emit(consumed as f32 / audio.sample_rate as f32, serde_json::json!({
            "type": "end",
            "transcript": transcript.trim(),
            "wpm": decoder.current_wpm(),
            "pitch": decoder.pitch(),
        }));
        return Ok(());
    }

    println!();
    println!("Final pitch:    {}", decoder.pitch().map(|p| format!("{p:.1} Hz")).unwrap_or_else(|| "(none)".into()));
    println!("Final WPM:      {}", decoder.current_wpm().map(|w| format!("{w:.1}")).unwrap_or_else(|| "(none)".into()));
    println!("Threshold:      {:.4e}", decoder.current_threshold());
    println!();
    println!("Transcript:");
    println!("{}", transcript.trim());
    Ok(())
}

fn run_stream_live(device: Option<&str>, seconds: f32, json: bool) -> Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    let capture = audio::open_input(device, 1.0)?;
    let mut decoder = streaming::StreamingDecoder::new(capture.sample_rate)?;
    let mut emitter = if json { Some(json::JsonEmitter::new()) } else { None };
    if let Some(em) = emitter.as_mut() {
        em.emit(0.0, serde_json::json!({
            "type": "ready",
            "source": "live",
            "device": capture.device_name,
            "rate": capture.sample_rate,
        }));
    } else {
        println!("Live streaming from: {}", capture.device_name);
    }

    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = Arc::clone(&stop);
        ctrlc_setup(move || { stop.store(true, Ordering::Relaxed); });
    }

    let started = Instant::now();
    let mut transcript = String::new();
    let mut last_wpm: Option<f32> = None;
    let mut last_drain_at: u64 = 0;

    loop {
        if stop.load(Ordering::Relaxed) { break; }
        if seconds > 0.0 && started.elapsed().as_secs_f32() >= seconds { break; }
        std::thread::sleep(Duration::from_millis(50));

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
        if chunk.is_empty() { continue; }

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
        if let streaming::StreamEvent::Char { ch, .. } = ev { transcript.push(ch); }
    }

    if let Some(em) = emitter.as_mut() {
        em.emit(started.elapsed().as_secs_f32(), serde_json::json!({
            "type": "end",
            "transcript": transcript.trim(),
            "wpm": decoder.current_wpm(),
            "pitch": decoder.pitch(),
        }));
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

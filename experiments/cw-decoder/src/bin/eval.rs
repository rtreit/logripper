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

use std::path::PathBuf;

use anyhow::Result;
use cw_decoder_poc::audio;
use cw_decoder_poc::streaming::{DecoderConfig, StreamEvent, StreamingDecoder};

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
            notes.push_str(&format!("ghost: got {} chars, max {}; ", nchars, max));
        }
    }
    if let Some(min) = case.expectation.min_chars {
        if nchars < min {
            pass = false;
            notes.push_str(&format!("recall: got {} chars, min {}; ", nchars, min));
        }
    }
    if let Some(c) = cer {
        if c > 0.5 {
            pass = false;
            notes.push_str(&format!("cer={:.2} > 0.5; ", c));
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
            out.extend(std::iter::repeat(0.0).take(word_n));
            continue;
        }
        let morse = char_to_morse(ch.to_ascii_uppercase());
        for (i, m) in morse.chars().enumerate() {
            let n = if m == '.' { dot_n } else { dah_n };
            push_tone(&mut out, n, tone_hz, rate);
            if i + 1 < morse.len() {
                out.extend(std::iter::repeat(0.0).take(intra_n));
            }
        }
        out.extend(std::iter::repeat(0.0).take(inter_n));
    }
    out.extend(std::iter::repeat(0.0).take((secs_padding * r) as usize));
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
        .map(|c| format!("cer={:.2}", c))
        .unwrap_or_else(|| "        ".into());
    println!(
        "{:<22} {} {:>5}c  cpm={:>5.1}  wpm={}  {} {}",
        case.name, mark, m.char_count, m.chars_per_minute, wpm, pitch, cer
    );
    if !m.decoded.is_empty() {
        let preview: String = m.decoded.chars().take(80).collect();
        println!("    > {}", preview);
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
        let u1 = (self.next_f32().max(1e-7)).min(1.0 - 1e-7);
        let u2 = self.next_f32();
        let r = (-2.0_f32 * u1.ln()).sqrt();
        let theta = 2.0 * std::f32::consts::PI * u2;
        r * theta.cos()
    }
}

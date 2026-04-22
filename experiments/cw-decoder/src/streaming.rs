//! Streaming Morse decoder.
//!
//! Reorganizes ditdah's offline pipeline into a true streaming decoder that
//! emits decoded characters, WPM updates, and pitch updates as audio is fed in.
//!
//! Pipeline:
//!   raw f32 chunks
//!     -> rubato SincFixedIn (chunked, 1024 in) -> 12 kHz mono
//!     -> Biquad HP (200 Hz) -> Biquad LP (1200 Hz)
//!     -> Goertzel(window=25 ms, step=window/4) tuned to detected pitch
//!     -> rolling 20 ms moving average -> "power signal"
//!     -> rolling-quantile threshold + on/off state machine
//!     -> dot-length running median + dit/dah/letter/word classifier
//!     -> StreamEvent::{Char, Word, WpmUpdate, PitchUpdate, ...}
//!
//! Pitch is locked from the first 2 s of resampled audio, and re-evaluated
//! every 5 s of *active* signal with hysteresis so we don't drift mid-letter.

use std::collections::VecDeque;

use anyhow::{anyhow, Result};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use rustfft::{num_complex::Complex, FftPlanner};

// --- Tunables (mirror ditdah's choices) --------------------------------
const TARGET_RATE: u32 = 12_000;
const FREQ_MIN_HZ: f32 = 200.0;
const FREQ_MAX_HZ: f32 = 1200.0;
const RESAMPLER_CHUNK: usize = 1024;
const GOERTZEL_WIN_MS: f32 = 25.0;
const POWER_SMOOTH_MS: f32 = 20.0;
const PITCH_LOCK_SECONDS: f32 = 2.0;
const PITCH_REEVAL_SECONDS: f32 = 5.0;
const POWER_HISTORY_SECONDS: f32 = 4.0;
const DIT_HISTORY: usize = 32;
const DIT_DAH_BOUNDARY: f32 = 2.0;
const LETTER_SPACE_BOUNDARY: f32 = 2.0;
const WORD_SPACE_BOUNDARY: f32 = 5.0;
const DEBOUNCE_FRAC: f32 = 0.3;
/// Minimum number of (on+short-off) intervals to collect before we believe
/// the k-means separation of dits vs dahs.
const PRIME_INTERVALS: usize = 8;
/// EMA smoothing factor for post-bootstrap dit-length updates.
const DIT_EMA_ALPHA: f32 = 0.15;

/// Off-band noise reference offsets from the locked pitch (Hz). We run extra
/// Goertzels at ± each offset to estimate broadband noise around the same
/// instant. CW is narrow-band so a real signal will dominate the pitch bin
/// while staying well below ALL the side bins. Using multiple offsets and
/// taking the *median* makes the gate robust against:
///   * a single side bin landing on an adjacent CW signal
///   * mains hum / harmonics at one specific offset
///   * partial filter rolloff at the band edges
const NOISE_OFFSETS_HZ: &[f32] = &[150.0, 300.0, 500.0, 700.0];
/// Default required signal-to-noise ratio (dB) for a power sample to be
/// considered "tone present" by the keying state machine. Below this, the
/// sample is treated as noise even if it crosses the adaptive amplitude
/// threshold. Operator-tunable at runtime via [`DecoderConfig::min_snr_db`].
pub const DEFAULT_MIN_SNR_DB: f32 = 6.0;
/// Default pitch-lock confidence (dB). The peak FFT bin must be at least
/// this many dB above the in-band median for `detect_pitch` to succeed,
/// otherwise we refuse to lock. This is what stops the decoder from
/// pretending a pure-noise band has a tone in it.
pub const DEFAULT_PITCH_MIN_SNR_DB: f32 = 12.0;
/// Default scale on the IQR-derived adaptive amplitude threshold. >1 makes
/// the decoder less sensitive (raises the on/off cutoff); <1 more sensitive.
pub const DEFAULT_THRESHOLD_SCALE: f32 = 1.0;
/// Smoothing window for the noise reference (ms). Longer than the signal
/// smoother so brief tone leakage into side bins doesn't lift the noise
/// floor and disable the gate.
const NOISE_SMOOTH_MS: f32 = 200.0;

/// Runtime-tunable decoder parameters. Cloneable so the GUI/CLI can
/// snapshot, mutate, and resend without locking. All fields use
/// natural units (dB, dimensionless scale) so the wire protocol
/// matches what the operator sees in the UI.
#[derive(Debug, Clone, Copy)]
pub struct DecoderConfig {
    /// Minimum tone-vs-noise ratio (dB) the streaming gate requires before
    /// a power sample is treated as "tone present".
    pub min_snr_db: f32,
    /// Minimum FFT-peak vs in-band median (dB) required at pitch-lock
    /// time. Higher = more conservative, refuses to lock onto pure noise.
    pub pitch_min_snr_db: f32,
    /// Scale factor on the IQR-derived adaptive amplitude threshold. 1.0
    /// is the classic value; >1 desensitises, <1 sensitises.
    pub threshold_scale: f32,
}

impl DecoderConfig {
    pub fn defaults() -> Self {
        Self {
            min_snr_db: DEFAULT_MIN_SNR_DB,
            pitch_min_snr_db: DEFAULT_PITCH_MIN_SNR_DB,
            threshold_scale: DEFAULT_THRESHOLD_SCALE,
        }
    }
    /// Convert min_snr_db → linear power ratio for the inner gate.
    pub fn min_snr_linear(&self) -> f32 {
        10.0_f32.powf(self.min_snr_db / 10.0)
    }
    pub fn pitch_min_snr_linear(&self) -> f32 {
        10.0_f32.powf(self.pitch_min_snr_db / 10.0)
    }
}

impl Default for DecoderConfig {
    fn default() -> Self {
        Self::defaults()
    }
}

// --- Events ------------------------------------------------------------
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Pitch lock acquired or refreshed.
    PitchUpdate { pitch_hz: f32 },
    /// New WPM estimate (smoothed).
    WpmUpdate { wpm: f32 },
    /// A decoded character emitted in real time.
    Char { ch: char, morse: String },
    /// Word boundary detected.
    Word,
    /// Letter could not be decoded (unknown morse pattern).
    Garbled { morse: String },
    /// Periodic snapshot of the smoothed Goertzel power vs threshold.
    /// Emitted at roughly `POWER_EVENT_HZ` Hz, throttled in `feed_goertzel`.
    /// `noise` is the smoothed off-band reference; `snr` is power/noise.
    Power {
        power: f32,
        threshold: f32,
        noise: f32,
        snr: f32,
        signal: bool,
    },
}

/// Target rate (events / sec) for `StreamEvent::Power`. A subset of the
/// per-step power samples are forwarded; the rest are decimated away.
const POWER_EVENT_HZ: f32 = 30.0;

// --- Biquad filter (lifted unchanged from ditdah) -----------------------
#[derive(Debug, Clone, Copy)]
enum FilterType {
    HighPass,
    LowPass,
}
struct Biquad {
    a0: f32,
    a1: f32,
    a2: f32,
    b1: f32,
    b2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}
impl Biquad {
    fn new(filter_type: FilterType, cutoff_hz: f32, sample_rate: u32) -> Self {
        let mut f = Self {
            a0: 1.0,
            a1: 0.0,
            a2: 0.0,
            b1: 0.0,
            b2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        };
        let c = (std::f32::consts::PI * cutoff_hz / sample_rate as f32).tan();
        let sqrt2 = 2.0_f32.sqrt();
        match filter_type {
            FilterType::LowPass => {
                let d = 1.0 / (1.0 + sqrt2 * c + c * c);
                f.a0 = c * c * d;
                f.a1 = 2.0 * f.a0;
                f.a2 = f.a0;
                f.b1 = 2.0 * (c * c - 1.0) * d;
                f.b2 = (1.0 - sqrt2 * c + c * c) * d;
            }
            FilterType::HighPass => {
                let d = 1.0 / (1.0 + sqrt2 * c + c * c);
                f.a0 = d;
                f.a1 = -2.0 * d;
                f.a2 = d;
                f.b1 = 2.0 * (c * c - 1.0) * d;
                f.b2 = (1.0 - sqrt2 * c + c * c) * d;
            }
        }
        f
    }
    fn process_in_place(&mut self, samples: &mut [f32]) {
        for s in samples.iter_mut() {
            let x0 = *s;
            let y0 = self.a0 * x0 + self.a1 * self.x1 + self.a2 * self.x2
                - self.b1 * self.y1
                - self.b2 * self.y2;
            self.x2 = self.x1;
            self.x1 = x0;
            self.y2 = self.y1;
            self.y1 = y0;
            *s = y0;
        }
    }
}

// --- Goertzel filter, sliding-window streaming version -----------------
struct Goertzel {
    coeff: f32,
    window: Vec<f32>,
    win_size: usize,
    step: usize,
    /// Pending samples not yet consumed by the next Goertzel evaluation.
    /// We keep a rolling buffer of at least `win_size` samples and slide by `step`.
    buf: VecDeque<f32>,
    /// How many *new* samples have accumulated since the last Goertzel evaluation.
    accumulated_since_eval: usize,
}
impl Goertzel {
    fn new(target_freq: f32, sample_rate: u32, win_size: usize, step: usize) -> Self {
        let k = 0.5 + (win_size as f32 * target_freq) / sample_rate as f32;
        let omega = (2.0 * std::f32::consts::PI * k) / win_size as f32;
        let coeff = 2.0 * omega.cos();
        let window = (0..win_size)
            .map(|i| 0.54 - 0.46 * (2.0 * std::f32::consts::PI * i as f32 / win_size as f32).cos())
            .collect();
        Self {
            coeff,
            window,
            win_size,
            step,
            buf: VecDeque::with_capacity(win_size * 2),
            accumulated_since_eval: 0,
        }
    }

    fn run_once(&self) -> f32 {
        let mut q1 = 0.0_f32;
        let mut q2 = 0.0_f32;
        // Iterate the most recent win_size samples.
        let start = self.buf.len() - self.win_size;
        for (i, sample) in self.buf.iter().skip(start).enumerate() {
            let q0 = self.coeff * q1 - q2 + sample * self.window[i];
            q2 = q1;
            q1 = q0;
        }
        q1 * q1 + q2 * q2 - self.coeff * q1 * q2
    }

    /// Push samples and emit a power value every `step` samples once the
    /// rolling buffer holds at least `win_size` samples.
    fn push(&mut self, samples: &[f32], out: &mut Vec<f32>) {
        for &s in samples {
            self.buf.push_back(s);
            // Keep the buffer bounded to win_size + step (only need recent history).
            while self.buf.len() > self.win_size + self.step {
                self.buf.pop_front();
            }
            self.accumulated_since_eval += 1;
            if self.buf.len() >= self.win_size && self.accumulated_since_eval >= self.step {
                self.accumulated_since_eval = 0;
                out.push(self.run_once());
            }
        }
    }
}

// --- Pitch detection: STFT over a buffered slice -----------------------
/// Detect the dominant in-band tone. `min_snr_linear` is the required ratio
/// of peak power to in-band median power; if not met we refuse to lock and
/// return Err so the caller stays in "no signal" state instead of
/// hallucinating decodes from background noise.
fn detect_pitch(samples: &[f32], sample_rate: u32, min_snr_linear: f32) -> Result<f32> {
    let fft_size = 4096;
    let step = fft_size / 4;
    if samples.len() < fft_size {
        return Err(anyhow!("not enough samples for pitch detection"));
    }
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(fft_size);
    let win: Vec<f32> = (0..fft_size)
        .map(|i| 0.54 - 0.46 * (2.0 * std::f32::consts::PI * i as f32 / fft_size as f32).cos())
        .collect();
    let mut sum = vec![0.0_f32; fft_size / 2];
    let mut count = 0;
    for chunk in samples.windows(fft_size).step_by(step) {
        let mut buf: Vec<Complex<f32>> = chunk
            .iter()
            .zip(win.iter())
            .map(|(s, w)| Complex::new(s * w, 0.0))
            .collect();
        fft.process(&mut buf);
        for (i, v) in buf.iter().take(fft_size / 2).enumerate() {
            sum[i] += v.norm_sqr();
        }
        count += 1;
    }
    if count == 0 {
        return Err(anyhow!("no FFT frames"));
    }
    let df = sample_rate as f32 / fft_size as f32;
    let mut best_idx = 0;
    let mut best_p = 0.0;
    let mut in_band: Vec<f32> = Vec::new();
    for (i, &p) in sum.iter().enumerate() {
        let f = i as f32 * df;
        if (FREQ_MIN_HZ..=FREQ_MAX_HZ).contains(&f) {
            in_band.push(p);
            if p > best_p {
                best_p = p;
                best_idx = i;
            }
        }
    }
    if best_p == 0.0 || in_band.is_empty() {
        return Err(anyhow!("no dominant pitch in band"));
    }
    // Peakiness check: peak power must clear the in-band median by at least
    // `min_snr_linear`. Pure noise has roughly flat in-band power so the
    // peak/median ratio sits near 1. A genuine CW tone produces a sharp
    // bin much higher than median.
    in_band.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = in_band[in_band.len() / 2].max(1e-30);
    let ratio = best_p / median;
    if ratio < min_snr_linear {
        return Err(anyhow!(
            "pitch peak {:.1} dB above median, need {:.1} dB",
            10.0 * ratio.log10(),
            10.0 * min_snr_linear.log10()
        ));
    }
    Ok(best_idx as f32 * df)
}

// --- Decoder state machine ---------------------------------------------
/// An on/off edge event produced by the state machine; used during bootstrap
/// to hold intervals before we have a reliable dot length.
#[derive(Clone, Copy, Debug)]
struct Interval {
    len: usize,
    is_on: bool,
}

/// Sliding-window Morse rhythm coherence gate.
///
/// Holds the last `WINDOW` intervals and asks "do these look like Morse at
/// the current dot estimate?" Three independent checks that ALL must pass:
///
/// 1. **Per-interval fit**: each interval's normalised length (L/dot) must
///    sit close to its nearest valid Morse unit (1 / 3 for ON; 1 / 3 / 7
///    for OFF). At least `OPEN_FRACTION` of recent intervals must fit.
/// 2. **Bimodality**: ON intervals must split into both a 1-cluster and a
///    3-cluster, with cluster means actually near 1 and 3. Pure noise
///    rarely produces a clean bimodal ON distribution.
/// 3. **Duty cycle**: on-time fraction over the window must lie in the
///    band typical of human CW (~25-65%). Receiver hiss with a bad
///    threshold sits well outside this band.
///
/// All three together are much more discriminating than any single test
/// because the noise-derived k-means dot estimate trivially passes (1) —
/// the dot length is *defined* as where noise intervals cluster — but
/// noise rarely passes (2) and (3) at the same time.
struct RhythmGate {
    window: VecDeque<Interval>,
    is_open: bool,
    bad_streak: usize,
    /// Consecutive intervals during which `compute_open` returned true.
    /// We require this to exceed `MATURITY` before reporting `is_open()`,
    /// which forces a brief noise-burst that *looks* like CW for a few
    /// intervals to still be suppressed.
    good_streak: usize,
}

impl RhythmGate {
    const WINDOW: usize = 24;
    const OPEN_FRACTION: f32 = 0.80;
    const MIN_INTERVALS: usize = 12;
    const STICKY_BAD: usize = 6;
    const DUTY_MIN: f32 = 0.25;
    const DUTY_MAX: f32 = 0.65;
    const TOL_FRAC: f32 = 0.30;
    const TOL_FLOOR: f32 = 0.35;
    const BIMODAL_MIN: usize = 2;
    /// Number of consecutive good intervals required after the gate first
    /// opens before character emission is allowed.
    const MATURITY: usize = 6;

    fn new() -> Self {
        Self {
            window: VecDeque::with_capacity(Self::WINDOW),
            is_open: false,
            bad_streak: 0,
            good_streak: 0,
        }
    }

    fn push(&mut self, ivl: Interval, dot: f32) {
        if self.window.len() == Self::WINDOW {
            self.window.pop_front();
        }
        self.window.push_back(ivl);
        let was_open = self.is_open;
        let now_open = self.compute_open(dot);
        if was_open && !now_open {
            self.bad_streak += 1;
            self.is_open = self.bad_streak < Self::STICKY_BAD;
            if !self.is_open {
                self.bad_streak = 0;
                self.good_streak = 0;
            }
        } else {
            self.is_open = now_open;
            if now_open {
                self.bad_streak = 0;
                self.good_streak = self.good_streak.saturating_add(1);
            } else {
                self.good_streak = 0;
            }
        }
    }

    fn is_open(&self) -> bool {
        self.is_open && self.good_streak >= Self::MATURITY
    }

    #[allow(dead_code)]
    fn reset(&mut self) {
        self.window.clear();
        self.is_open = false;
        self.bad_streak = 0;
        self.good_streak = 0;
    }

    fn compute_open(&self, dot: f32) -> bool {
        if self.window.len() < Self::MIN_INTERVALS || dot <= 0.0 {
            return false;
        }
        // (1) Per-interval fit.
        let good = self
            .window
            .iter()
            .filter(|i| Self::interval_good(**i, dot))
            .count();
        let frac = good as f32 / self.window.len() as f32;
        if frac < Self::OPEN_FRACTION {
            return false;
        }
        // (2) Bimodality of ON intervals.
        let mut dits = Vec::new();
        let mut dahs = Vec::new();
        for i in &self.window {
            if i.is_on {
                let n = i.len as f32 / dot;
                if n < 2.0 {
                    dits.push(n);
                } else if n <= 5.0 {
                    dahs.push(n);
                }
            }
        }
        if dits.len() < Self::BIMODAL_MIN || dahs.len() < Self::BIMODAL_MIN {
            return false;
        }
        let dit_mean = dits.iter().copied().sum::<f32>() / dits.len() as f32;
        let dah_mean = dahs.iter().copied().sum::<f32>() / dahs.len() as f32;
        if !(0.6..=1.4).contains(&dit_mean) {
            return false;
        }
        if !(2.4..=3.6).contains(&dah_mean) {
            return false;
        }
        // (3) Duty cycle. Word/silence gaps are excluded so a station
        // pausing between exchanges doesn't pull duty below DUTY_MIN.
        let mut on_total = 0usize;
        let mut counted_total = 0usize;
        for i in &self.window {
            let n = i.len as f32 / dot;
            if !i.is_on && n > 8.0 {
                continue; // ambient silence — excluded from duty calculation
            }
            counted_total += i.len;
            if i.is_on {
                on_total += i.len;
            }
        }
        if counted_total == 0 {
            return false;
        }
        let duty = on_total as f32 / counted_total as f32;
        if !(Self::DUTY_MIN..=Self::DUTY_MAX).contains(&duty) {
            return false;
        }
        true
    }

    fn interval_good(ivl: Interval, dot: f32) -> bool {
        let n = ivl.len as f32 / dot;
        let targets: &[f32] = if ivl.is_on {
            if n > 5.0 {
                return false;
            }
            &[1.0, 3.0]
        } else {
            if n > 14.0 {
                return true;
            }
            &[1.0, 3.0, 7.0]
        };
        targets.iter().any(|&t| {
            let dist = (n - t).abs();
            let tol = (Self::TOL_FRAC * t).max(Self::TOL_FLOOR);
            dist <= tol
        })
    }
}

/// Run 1-D k-means (k=2) on a slice of lengths and return (low_mean, high_mean)
/// if the two clusters are plausibly "dit" and "dah" (ratio between ~2.3x and
/// ~4.0x, with each cluster holding at least two points). Requiring both
/// clusters to have real membership prevents a single noise outlier from
/// collapsing the dit estimate.
fn kmeans_dit_dah(lengths: &[f32]) -> Option<(f32, f32)> {
    if lengths.len() < 4 {
        return None;
    }
    let mut sorted = lengths.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let min = sorted[0];
    let max = sorted[sorted.len() - 1];
    if max <= 0.0 || min <= 0.0 {
        return None;
    }

    // If all lengths are similar, we can't separate dits from dahs yet.
    if max / min < 1.8 {
        return None;
    }

    let mut low = min;
    let mut high = max;
    let mut lows_n = 0usize;
    let mut highs_n = 0usize;
    for _ in 0..16 {
        let prev_low = low;
        let prev_high = high;
        let mid = (low + high) / 2.0;
        let (mut lows_sum, mut ln) = (0.0_f32, 0_usize);
        let (mut highs_sum, mut hn) = (0.0_f32, 0_usize);
        for &x in &sorted {
            if x < mid {
                lows_sum += x;
                ln += 1;
            } else {
                highs_sum += x;
                hn += 1;
            }
        }
        if ln == 0 || hn == 0 {
            return None;
        }
        low = lows_sum / ln as f32;
        high = highs_sum / hn as f32;
        lows_n = ln;
        highs_n = hn;
        if (low - prev_low).abs() < 1e-3 && (high - prev_high).abs() < 1e-3 {
            break;
        }
    }
    // Sanity: both clusters need real support (not 1 outlier), and the ratio
    // needs to match Morse theory (dah = 3 × dit), with a modest tolerance.
    if lows_n < 2 || highs_n < 2 {
        return None;
    }
    let ratio = high / low;
    if !(2.3..=4.2).contains(&ratio) {
        return None;
    }
    Some((low, high))
}

struct Decoder {
    /// Power-signal samples kept for IQR threshold estimation.
    power_history: VecDeque<f32>,
    power_capacity: usize,
    power_rate: f32,

    threshold: f32,
    threshold_dirty_count: usize,
    /// Operator-controlled multiplier on the IQR-derived threshold.
    threshold_scale: f32,

    is_on: bool,
    current_run: usize,
    have_first_sample: bool,

    /// Rolling history of the most recent intervals (both on and off) kept
    /// for periodic k-means re-calibration of the dit length.
    interval_history: VecDeque<Interval>,
    dot_len: Option<f32>,
    dah_len: Option<f32>,

    /// Bootstrap buffer: we hold all intervals here (events not yet emitted)
    /// until we have a confident dot length estimate. Once the decoder is
    /// primed, this buffer is replayed through the decode logic and remains
    /// empty thereafter.
    bootstrap: Vec<Interval>,
    primed: bool,

    /// Sliding rhythm-coherence gate: gates emission on whether recent
    /// intervals actually look like Morse (1/3/7 dot units). Open by
    /// default after enough intervals; closed on noise.
    rhythm: RhythmGate,

    current_letter: String,
}

impl Decoder {
    fn new(power_rate: f32) -> Self {
        let capacity = (power_rate * POWER_HISTORY_SECONDS) as usize;
        Self {
            power_history: VecDeque::with_capacity(capacity.max(64)),
            power_capacity: capacity.max(64),
            power_rate,
            threshold: 0.0,
            threshold_dirty_count: 0,
            threshold_scale: DEFAULT_THRESHOLD_SCALE,
            is_on: false,
            current_run: 0,
            have_first_sample: false,
            interval_history: VecDeque::with_capacity(DIT_HISTORY),
            dot_len: None,
            dah_len: None,
            bootstrap: Vec::new(),
            primed: false,
            rhythm: RhythmGate::new(),
            current_letter: String::new(),
        }
    }

    fn update_threshold(&mut self) {
        if self.power_history.len() < 32 {
            return;
        }
        let mut v: Vec<f32> = self
            .power_history
            .iter()
            .copied()
            .filter(|x| *x > 0.0)
            .collect();
        if v.len() < 16 {
            return;
        }
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p25 = v[v.len() / 4];
        let p75 = v[3 * v.len() / 4];
        let iqr = p75 - p25;
        // Multiply by `threshold_scale` so the operator can desensitise on
        // noisy bands (>1.0) or sensitise on quiet bands (<1.0).
        self.threshold = p25 + iqr * 0.5 * self.threshold_scale;
    }

    fn push_interval(&mut self, ivl: Interval) {
        if self.interval_history.len() == DIT_HISTORY {
            self.interval_history.pop_front();
        }
        self.interval_history.push_back(ivl);
    }

    /// Attempt to separate dits from dahs using all recent short intervals
    /// (on + off where off < 5× max-on). Returns the dit cluster mean.
    fn recalibrate_from_history(&mut self) {
        // Build candidate set: all on-intervals plus off-intervals that look
        // like element/letter gaps (exclude word gaps which skew high).
        let ons: Vec<f32> = self
            .interval_history
            .iter()
            .filter(|i| i.is_on)
            .map(|i| i.len as f32)
            .collect();
        if ons.len() < 3 {
            return;
        }
        let max_on = ons.iter().cloned().fold(0.0_f32, f32::max);
        if max_on <= 0.0 {
            return;
        }
        let mut combined: Vec<f32> = ons.clone();
        for i in &self.interval_history {
            if !i.is_on {
                let l = i.len as f32;
                // Drop obvious word gaps (> ~5× max-on is outside Morse timing).
                if l < max_on * 2.0 {
                    combined.push(l);
                }
            }
        }

        if let Some((lo, hi)) = kmeans_dit_dah(&combined) {
            // Post-bootstrap: smooth with EMA to suppress jitter; first call
            // sets the initial estimate directly.
            let new_dot = if let Some(prev) = self.dot_len {
                prev * (1.0 - DIT_EMA_ALPHA) + lo * DIT_EMA_ALPHA
            } else {
                lo
            };
            let new_dah = if let Some(prev) = self.dah_len {
                prev * (1.0 - DIT_EMA_ALPHA) + hi * DIT_EMA_ALPHA
            } else {
                hi
            };
            self.dot_len = Some(new_dot);
            self.dah_len = Some(new_dah);
        } else if self.dot_len.is_none() {
            // Ambiguous single-cluster before we've ever locked. We do NOT
            // guess here; decoding stays paused until we see dit/dah contrast.
        }
    }

    fn current_wpm(&self) -> Option<f32> {
        let dot = self.dot_len?;
        if dot <= 0.0 {
            return None;
        }
        let dot_ms = (dot / self.power_rate) * 1000.0;
        Some(1200.0 / dot_ms)
    }

    fn classify_letter(&mut self) -> Option<StreamEvent> {
        if self.current_letter.is_empty() {
            return None;
        }
        let morse = std::mem::take(&mut self.current_letter);
        if let Some(c) = morse_to_char(&morse) {
            Some(StreamEvent::Char { ch: c, morse })
        } else {
            Some(StreamEvent::Garbled { morse })
        }
    }

    /// Consume a single interval and emit decode events. This is the logic
    /// that was previously inline in `push_power`, factored out so we can
    /// replay the bootstrap buffer once we've primed the dot length.
    fn consume_interval(&mut self, ivl: Interval, events: &mut Vec<StreamEvent>) {
        let dot = match self.dot_len {
            Some(d) if d > 0.0 => d,
            _ => return,
        };
        let len_norm = ivl.len as f32 / dot;
        // Feed the rhythm gate first so it always sees every interval.
        self.rhythm.push(ivl, dot);
        if ivl.is_on {
            if len_norm < DIT_DAH_BOUNDARY {
                self.current_letter.push('.');
            } else {
                self.current_letter.push('-');
            }
            // WPM events are cheap and informative even when the gate is
            // closed (they help the operator see when the decoder has at
            // least estimated a tempo from the noise).
            if self.rhythm.is_open() {
                if let Some(w) = self.current_wpm() {
                    events.push(StreamEvent::WpmUpdate { wpm: w });
                }
            }
        } else if len_norm > LETTER_SPACE_BOUNDARY {
            // Letter / word boundary. Only flush characters when the rhythm
            // gate believes this is a real signal.
            if self.rhythm.is_open() {
                if let Some(ev) = self.classify_letter() {
                    events.push(ev);
                }
                if len_norm > WORD_SPACE_BOUNDARY {
                    events.push(StreamEvent::Word);
                }
            } else {
                // Drop the in-progress letter so we don't accumulate a long
                // tail of dots/dashes from noise that suddenly emerges as
                // garbage when the gate finally opens.
                self.current_letter.clear();
            }
        }
    }

    /// Push one edge-terminated run into the decoder. During bootstrap we
    /// buffer; once primed, we emit events immediately.
    fn push_edge(&mut self, ivl: Interval, events: &mut Vec<StreamEvent>) {
        self.push_interval(ivl);
        self.recalibrate_from_history();

        if !self.primed {
            self.bootstrap.push(ivl);
            let have_contrast = self.dot_len.is_some()
                && self.dah_len.is_some()
                && self.bootstrap.len() >= PRIME_INTERVALS;
            if have_contrast {
                // Announce WPM once, then replay the whole buffer.
                if let Some(w) = self.current_wpm() {
                    events.push(StreamEvent::WpmUpdate { wpm: w });
                }
                let buffered = std::mem::take(&mut self.bootstrap);
                self.primed = true;
                for b in buffered {
                    self.consume_interval(b, events);
                }
            }
            // Still priming; hold off on emission.
            return;
        }

        self.consume_interval(ivl, events);
    }

    /// Feed one power-signal sample. `snr_ok` indicates whether the off-band
    /// noise reference says the bin actually contains a tone (gain-independent).
    /// Emits zero or more events.
    fn push_power(&mut self, p: f32, snr_ok: bool, events: &mut Vec<StreamEvent>) {
        if self.power_history.len() == self.power_capacity {
            self.power_history.pop_front();
        }
        self.power_history.push_back(p);

        self.threshold_dirty_count += 1;
        if self.threshold_dirty_count >= (self.power_rate * 0.25) as usize + 1 {
            self.threshold_dirty_count = 0;
            self.update_threshold();
        }
        if self.threshold == 0.0 {
            return;
        }

        // Both gates must agree: amplitude above adaptive threshold AND the
        // tone bin is meaningfully louder than the off-band reference. The
        // SNR gate is what keeps background hiss from being decoded as morse.
        let above = p > self.threshold && snr_ok;
        if !self.have_first_sample {
            self.is_on = above;
            self.current_run = 1;
            self.have_first_sample = true;
            return;
        }

        if above == self.is_on {
            self.current_run += 1;
            return;
        }

        let ended_len = self.current_run;
        let prev_was_on = self.is_on;

        // Debounce. Pre-prime we use a conservative 30 ms floor (just below a
        // 40 WPM dit of ~30 ms) so Goertzel edge flicker can't sneak tiny
        // outliers into the k-means low cluster. Post-prime we scale by the
        // estimated dot length.
        let debounce = if let Some(dot) = self.dot_len {
            (dot * DEBOUNCE_FRAC).round() as usize
        } else {
            (self.power_rate * 0.03) as usize
        };

        if ended_len > debounce {
            self.push_edge(
                Interval {
                    len: ended_len,
                    is_on: prev_was_on,
                },
                events,
            );
        }

        self.is_on = above;
        self.current_run = 1;
    }
}

// --- Top-level streaming decoder ---------------------------------------
pub struct StreamingDecoder {
    resampler: Option<SincFixedIn<f32>>,
    raw_in: Vec<f32>,
    hp: Biquad,
    lp: Biquad,

    /// Operator-tunable runtime configuration.
    config: DecoderConfig,

    /// Resampled+filtered audio waiting for pitch lock.
    pre_lock_buf: Vec<f32>,
    pitch_locked: Option<f32>,
    samples_since_pitch_eval: usize,
    pitch_reeval_threshold: usize,

    goertzel: Option<Goertzel>,
    /// Off-band noise references at multiple offsets around the locked
    /// pitch. We take the *median* per power sample to estimate noise,
    /// which is robust to a single bin landing on adjacent CW or hum.
    noise_bins: Vec<Goertzel>,
    smooth_window: usize,
    /// Recent power samples for moving average.
    smooth_buf: VecDeque<f32>,
    smooth_sum: f32,

    /// Smoother for the off-band noise reference.
    noise_smooth_window: usize,
    noise_smooth_buf: VecDeque<f32>,
    noise_smooth_sum: f32,

    /// Decimation counter for `StreamEvent::Power` throttling.
    power_emit_accum: f32,
    power_emit_step: f32,

    decoder: Decoder,
}

impl StreamingDecoder {
    pub fn new(source_rate: u32) -> Result<Self> {
        let resampler = if source_rate != TARGET_RATE {
            Some(SincFixedIn::new(
                TARGET_RATE as f64 / source_rate as f64,
                2.0,
                SincInterpolationParameters {
                    sinc_len: 256,
                    f_cutoff: 0.95,
                    interpolation: SincInterpolationType::Linear,
                    oversampling_factor: 256,
                    window: WindowFunction::BlackmanHarris,
                },
                RESAMPLER_CHUNK,
                1,
            )?)
        } else {
            None
        };

        let win_size = (TARGET_RATE as f32 * GOERTZEL_WIN_MS / 1000.0) as usize;
        let step = (win_size / 4).max(1);
        let power_rate = TARGET_RATE as f32 / step as f32;
        let smooth_window = ((power_rate * POWER_SMOOTH_MS / 1000.0).round() as usize).max(1);
        let noise_smooth_window = ((power_rate * NOISE_SMOOTH_MS / 1000.0).round() as usize).max(1);
        let power_emit_step = (power_rate / POWER_EVENT_HZ).max(1.0);

        Ok(Self {
            resampler,
            raw_in: Vec::with_capacity(RESAMPLER_CHUNK * 2),
            hp: Biquad::new(FilterType::HighPass, FREQ_MIN_HZ, TARGET_RATE),
            lp: Biquad::new(FilterType::LowPass, FREQ_MAX_HZ, TARGET_RATE),
            config: DecoderConfig::defaults(),
            pre_lock_buf: Vec::with_capacity((TARGET_RATE as f32 * PITCH_LOCK_SECONDS) as usize),
            pitch_locked: None,
            samples_since_pitch_eval: 0,
            pitch_reeval_threshold: (TARGET_RATE as f32 * PITCH_REEVAL_SECONDS) as usize,
            goertzel: None,
            noise_bins: Vec::new(),
            smooth_window,
            smooth_buf: VecDeque::with_capacity(smooth_window + 1),
            smooth_sum: 0.0,
            noise_smooth_window,
            noise_smooth_buf: VecDeque::with_capacity(noise_smooth_window + 1),
            noise_smooth_sum: 0.0,
            power_emit_accum: 0.0,
            power_emit_step,
            decoder: Decoder::new(power_rate),
        })
    }

    pub fn pitch(&self) -> Option<f32> {
        self.pitch_locked
    }
    pub fn current_wpm(&self) -> Option<f32> {
        self.decoder.current_wpm()
    }
    pub fn current_threshold(&self) -> f32 {
        self.decoder.threshold
    }
    #[allow(dead_code)]
    pub fn config(&self) -> DecoderConfig {
        self.config
    }

    /// Apply a new runtime configuration. Safe to call mid-stream — only
    /// affects subsequent power samples and the next pitch re-lock.
    pub fn set_config(&mut self, cfg: DecoderConfig) {
        self.config = cfg;
        self.decoder.threshold_scale = cfg.threshold_scale;
    }

    /// Feed a chunk of raw audio at `source_rate`. Returns events emitted by
    /// this call (decoded characters, WPM updates, pitch lock).
    pub fn feed(&mut self, samples: &[f32]) -> Result<Vec<StreamEvent>> {
        let mut events = Vec::new();
        let resampled = self.resample(samples)?;
        if resampled.is_empty() {
            return Ok(events);
        }
        let mut filtered = resampled;
        self.hp.process_in_place(&mut filtered);
        self.lp.process_in_place(&mut filtered);

        // --- Pitch lock / re-eval -------------------------------------
        if self.pitch_locked.is_none() {
            self.pre_lock_buf.extend_from_slice(&filtered);
            let need = (TARGET_RATE as f32 * PITCH_LOCK_SECONDS) as usize;
            if self.pre_lock_buf.len() >= need {
                if let Ok(pitch) = detect_pitch(
                    &self.pre_lock_buf,
                    TARGET_RATE,
                    self.config.pitch_min_snr_linear(),
                ) {
                    self.pitch_locked = Some(pitch);
                    let win_size = (TARGET_RATE as f32 * GOERTZEL_WIN_MS / 1000.0) as usize;
                    let step = (win_size / 4).max(1);
                    self.goertzel = Some(Goertzel::new(pitch, TARGET_RATE, win_size, step));
                    // Multi-bin off-band noise references. Each offset is
                    // tried on both sides; only those that fit inside the
                    // audio passband [FREQ_MIN_HZ, FREQ_MAX_HZ] are used.
                    self.noise_bins.clear();
                    for &off in NOISE_OFFSETS_HZ {
                        let lo = pitch - off;
                        let hi = pitch + off;
                        if lo >= FREQ_MIN_HZ {
                            self.noise_bins
                                .push(Goertzel::new(lo, TARGET_RATE, win_size, step));
                        }
                        if hi <= FREQ_MAX_HZ {
                            self.noise_bins
                                .push(Goertzel::new(hi, TARGET_RATE, win_size, step));
                        }
                    }
                    events.push(StreamEvent::PitchUpdate { pitch_hz: pitch });
                    // Replay the pre-lock audio through Goertzel so we don't lose it.
                    let drained = std::mem::take(&mut self.pre_lock_buf);
                    self.feed_goertzel(&drained, &mut events);
                } else {
                    // Drop oldest half so we don't keep growing forever on silence.
                    let drop = self.pre_lock_buf.len() / 2;
                    self.pre_lock_buf.drain(..drop);
                }
            }
            return Ok(events);
        }

        self.feed_goertzel(&filtered, &mut events);

        // --- Periodic pitch re-eval (with hysteresis) ------------------
        self.samples_since_pitch_eval += filtered.len();
        if self.samples_since_pitch_eval >= self.pitch_reeval_threshold {
            self.samples_since_pitch_eval = 0;
            // Use the active power history window's audio time to test.
            // We don't keep raw audio history, so just trust the locked pitch unless
            // ditdah detects a strongly different one in fresh post-lock audio.
            // (For PoC simplicity, we skip mid-stream re-lock; pitch tends to be very
            // stable for a single QSO/channel.)
        }

        Ok(events)
    }

    fn feed_goertzel(&mut self, audio: &[f32], events: &mut Vec<StreamEvent>) {
        let Some(goertzel) = self.goertzel.as_mut() else {
            return;
        };
        let mut power_out = Vec::new();
        goertzel.push(audio, &mut power_out);

        // Run all noise-bin Goertzels in lockstep (identical win_size/step,
        // so each emits exactly the same number of samples as `power_out`).
        let mut noise_outs: Vec<Vec<f32>> =
            (0..self.noise_bins.len()).map(|_| Vec::new()).collect();
        for (idx, nb) in self.noise_bins.iter_mut().enumerate() {
            nb.push(audio, &mut noise_outs[idx]);
        }
        let snr_threshold = self.config.min_snr_linear();

        for (i, p) in power_out.iter().copied().enumerate() {
            // Signal moving average.
            self.smooth_buf.push_back(p);
            self.smooth_sum += p;
            if self.smooth_buf.len() > self.smooth_window {
                if let Some(old) = self.smooth_buf.pop_front() {
                    self.smooth_sum -= old;
                }
            }
            let smoothed = self.smooth_sum / self.smooth_buf.len() as f32;

            // Noise reference: take the *median* of the side-bin readings
            // at this instant. Median (vs min) is robust to a single bin
            // landing on adjacent CW/hum and stays representative even
            // when one bin happens to be quiet.
            let noise_raw = if noise_outs.is_empty() {
                0.0
            } else {
                let mut buf: Vec<f32> = noise_outs
                    .iter()
                    .filter_map(|v| v.get(i).copied())
                    .collect();
                if buf.is_empty() {
                    0.0
                } else {
                    buf.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    buf[buf.len() / 2]
                }
            };
            self.noise_smooth_buf.push_back(noise_raw);
            self.noise_smooth_sum += noise_raw;
            if self.noise_smooth_buf.len() > self.noise_smooth_window {
                if let Some(old) = self.noise_smooth_buf.pop_front() {
                    self.noise_smooth_sum -= old;
                }
            }
            let noise = self.noise_smooth_sum / self.noise_smooth_buf.len() as f32;

            // SNR: how many times louder is the tone bin vs the off-band
            // reference. With no noise reading yet we default to "OK" so the
            // existing amplitude threshold still does its job.
            let snr = if noise > 0.0 {
                smoothed / noise
            } else {
                f32::INFINITY
            };
            let snr_ok = snr >= snr_threshold;

            // Throttled Power event for UI meters (~POWER_EVENT_HZ).
            self.power_emit_accum += 1.0;
            if self.power_emit_accum >= self.power_emit_step {
                self.power_emit_accum -= self.power_emit_step;
                let threshold = self.decoder.threshold;
                let signal = threshold > 0.0 && smoothed > threshold && snr_ok;
                let snr_clean = if snr.is_finite() { snr } else { 0.0 };
                events.push(StreamEvent::Power {
                    power: smoothed,
                    threshold,
                    noise,
                    snr: snr_clean,
                    signal,
                });
            }

            self.decoder.push_power(smoothed, snr_ok, events);
        }
    }

    fn resample(&mut self, samples: &[f32]) -> Result<Vec<f32>> {
        let Some(resampler) = self.resampler.as_mut() else {
            return Ok(samples.to_vec());
        };
        self.raw_in.extend_from_slice(samples);
        let mut out = Vec::new();
        while self.raw_in.len() >= RESAMPLER_CHUNK {
            let waves_in = &[&self.raw_in[..RESAMPLER_CHUNK]];
            let mut resampled = resampler.process(waves_in, None)?;
            self.raw_in.drain(..RESAMPLER_CHUNK);
            out.extend(resampled.remove(0));
        }
        Ok(out)
    }

    /// Force-decode whatever letter is currently buffered. Useful at end of
    /// input or when the user pauses.
    pub fn flush(&mut self) -> Vec<StreamEvent> {
        let mut events = Vec::new();
        if let Some(ev) = self.decoder.classify_letter() {
            events.push(ev);
        }
        events
    }
}

// --- morse table (lifted from ditdah) ----------------------------------
fn morse_to_char(s: &str) -> Option<char> {
    match s {
        ".-" => Some('A'),
        "-..." => Some('B'),
        "-.-." => Some('C'),
        "-.." => Some('D'),
        "." => Some('E'),
        "..-." => Some('F'),
        "--." => Some('G'),
        "...." => Some('H'),
        ".." => Some('I'),
        ".---" => Some('J'),
        "-.-" => Some('K'),
        ".-.." => Some('L'),
        "--" => Some('M'),
        "-." => Some('N'),
        "---" => Some('O'),
        ".--." => Some('P'),
        "--.-" => Some('Q'),
        ".-." => Some('R'),
        "..." => Some('S'),
        "-" => Some('T'),
        "..-" => Some('U'),
        "...-" => Some('V'),
        ".--" => Some('W'),
        "-..-" => Some('X'),
        "-.--" => Some('Y'),
        "--.." => Some('Z'),
        ".----" => Some('1'),
        "..---" => Some('2'),
        "...--" => Some('3'),
        "....-" => Some('4'),
        "....." => Some('5'),
        "-...." => Some('6'),
        "--..." => Some('7'),
        "---.." => Some('8'),
        "----." => Some('9'),
        "-----" => Some('0'),
        _ => None,
    }
}

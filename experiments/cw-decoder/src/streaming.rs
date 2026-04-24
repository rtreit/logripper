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
//! Pitch is locked from the first ~2 s of resampled audio, and re-evaluated
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
const PITCH_LOCK_SECONDS: f32 = 6.0;
const PITCH_REEVAL_SECONDS: f32 = 8.0;
/// Quality watchdog: rolling Fisher check on this many seconds of recent
/// post-lock audio. Long enough to span character/word gaps, short
/// enough to react when conditions go bad.
const QUALITY_WINDOW_SECONDS: f32 = 8.0;
/// Run the post-lock quality check this often.
const QUALITY_CHECK_SECONDS: f32 = 4.0;
/// Minimum trial-decode Fisher to keep the lock once acquired. Set
/// lower than `MIN_LOCK_FISHER` so a brief signal dip doesn't
/// immediately drop the lock — that's what `QUALITY_DROP_CONSECUTIVE`
/// is for.
/// Lowered from 3.5 → 2.0 after the 30 WPM abbrev bench showed real
/// CW windows producing Fisher ≈ 2.4–2.5 when the 8 s window happens
/// to capture an unbalanced mix of dits and dahs (e.g. the run of
/// long-dah characters around `73 TNX RST`). Healthy steady-state
/// Fisher on this clip is 4–10; values in [2, 3) are "ambiguous
/// window" not "bad signal" and should not drop a working lock.
/// `QUALITY_FAST_DROP_FISHER = 1.0` still catches real loss.
const MIN_HOLD_FISHER: f32 = 2.0;
/// Number of consecutive failed quality checks required before we
/// drop the lock (hysteresis against QSB / brief pauses). Bumped
/// from 2 → 3 so a single ambiguous window followed by another
/// ambiguous one (still potentially benign) doesn't drop lock; it
/// takes 12 s of sustained sub-threshold Fisher to drop.
const QUALITY_DROP_CONSECUTIVE: u32 = 3;
/// Trial-Fisher score below which the watchdog drops the lock on a
/// SINGLE failed check, bypassing `QUALITY_DROP_CONSECUTIVE`. This
/// catches the "we just locked onto voice/garbage" case where Fisher
/// is essentially zero (no coherent dit/dah clusters at all).
/// Borderline-bad signals (Fisher in [FAST_DROP, MIN_HOLD)) still get
/// the normal 2-check hysteresis so QSB doesn't drop a real lock.
const QUALITY_FAST_DROP_FISHER: f32 = 1.0;
/// RMS of the post-filter quality-window audio below which we treat
/// the buffer as "silent" (key-up between transmissions, deep QSB
/// null, pause between QSOs) and SKIP the watchdog check entirely.
/// Fisher on a silent buffer is ~0 because there are no dit/dah
/// clusters at all, but that's "no signal" not "bad signal" — we
/// must hold the lock through it. Tuned well below the envelope of
/// real CW chars (typical RMS 0.05–0.2 normalized) but above the
/// post-HP+LP noise floor of typical recordings (~0.001–0.005).
const QUALITY_SILENCE_RMS: f32 = 0.01;
/// Pitch-lock buffer size after a recent watchdog drop. Smaller than
/// PITCH_LOCK_SECONDS so we can re-acquire on the new clean signal
/// quickly (typical case: voice ended, real CW just started — we
/// don't want to lose another 6 s of valid CW characters waiting for
/// a full window to refill). The flag is sticky only until the next
/// lock survives its probation check, so this doesn't degrade
/// steady-state false-lock resistance.
const RELOCK_SECONDS: f32 = 3.0;
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
/// Hard sanity gate on ON-interval duration in dot units. Anything
/// shorter than `MIN_ON_DOT_FRAC` cannot be a real dit; anything longer
/// than `MAX_ON_DOT_FRAC` cannot be a real dah and is treated as bad
/// keying / impulsive noise. These are enforced after the chatter
/// merge in `consume_interval`, in addition to the (configurable)
/// `min_pulse_dot_fraction` knob.
const MIN_ON_DOT_FRAC: f32 = 0.40;
const MAX_ON_DOT_FRAC: f32 = 4.80;

/// Off-band noise reference offsets from the locked pitch (Hz). We run extra
/// Goertzels at ± each offset to estimate broadband noise around the same
/// instant. CW is narrow-band so a real signal will dominate the pitch bin
/// while staying well below ALL the side bins. Using multiple offsets and
/// taking the *median* makes the gate robust against:
///   * a single side bin landing on an adjacent CW signal
///   * mains hum / harmonics at one specific offset
///   * partial filter rolloff at the band edges
const NOISE_OFFSETS_HZ: &[f32] = &[150.0, 300.0, 500.0, 700.0];
// (Tone-purity uses the existing off-band noise bins at instantaneous time
// rather than a separate set of close-in bins. See `feed_goertzel`.)
/// Default required signal-to-noise ratio (dB) for a power sample to be
/// considered "tone present" by the keying state machine. Below this, the
/// sample is treated as noise even if it crosses the adaptive amplitude
/// threshold. Operator-tunable at runtime via [`DecoderConfig::min_snr_db`].
pub const DEFAULT_MIN_SNR_DB: f32 = 3.0;
/// Default pitch-lock confidence (dB). The peak FFT bin must be at least
/// this many dB above the in-band median for `detect_pitch` to succeed,
/// otherwise we refuse to lock. This is what stops the decoder from
/// pretending a pure-noise band has a tone in it.
pub const DEFAULT_PITCH_MIN_SNR_DB: f32 = 6.0;
/// Default scale on the IQR-derived adaptive amplitude threshold. >1 makes
/// the decoder less sensitive (raises the on/off cutoff); <1 more sensitive.
pub const DEFAULT_THRESHOLD_SCALE: f32 = 1.0;
/// Default lower bound for the experimental range-lock mode.
pub const DEFAULT_RANGE_LOCK_MIN_HZ: f32 = 550.0;
/// Default upper bound for the experimental range-lock mode.
pub const DEFAULT_RANGE_LOCK_MAX_HZ: f32 = 850.0;
/// Default minimum tone-purity ratio required for a power sample to be
/// treated as "narrowband tone present." Computed instantaneously per power
/// sample as `target_bin / max(adjacent_bin)` using bins at
/// `PURITY_OFFSETS_HZ` around the locked pitch. Real CW tones routinely
/// score 5–20+; broadband impulses score ~1.
///
/// 3.0 is a conservative lower bound chosen to keep weak but real CW
/// (Q5 copy) decoding while killing finger snaps and key clicks. Tunable
/// per-operator via [`DecoderConfig::min_tone_purity`].
pub const DEFAULT_MIN_TONE_PURITY: f32 = 3.0;
/// Smoothing window for the noise reference (ms). Longer than the signal
/// smoother so brief tone leakage into side bins doesn't lift the noise
/// floor and disable the gate.
const NOISE_SMOOTH_MS: f32 = 200.0;
/// Pre-lock broadband activity history used to keep the UI's live signal meter
/// responsive even while we're still hunting for a pitch or re-locking after a
/// watchdog drop.
const UNLOCK_POWER_HISTORY_EVENTS: usize = 80;
/// Experimental relock buffer when range-lock mode is enabled. Constraining the
/// pitch hunt to a narrow user-specified band lets us react much faster than the
/// conservative default whole-band lock path.
const RANGE_LOCK_SECONDS: f32 = 1.0;

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
    /// is the classic value; >1 desensitises, <1 sensitises. Ignored when
    /// `auto_threshold` is true.
    pub threshold_scale: f32,
    /// When true, the decoder picks `threshold_scale` automatically from
    /// the current Q90/Q10 SNR margin: clean strong signals get scale~1.0,
    /// weak/fading signals are pushed down toward ~0.4 so dits aren't
    /// missed. Lets the decoder follow QSB without operator intervention.
    pub auto_threshold: bool,
    /// Experimental mode: constrain pitch locking to `range_lock_min_hz..=range_lock_max_hz`
    /// and use a faster relock path inside that band.
    pub experimental_range_lock: bool,
    /// Lower bound for the experimental pitch-lock band.
    pub range_lock_min_hz: f32,
    /// Upper bound for the experimental pitch-lock band.
    pub range_lock_max_hz: f32,
    /// Minimum instantaneous tone-purity ratio (target / max adjacent bin)
    /// required for a power sample to be treated as "tone present." Set to
    /// 0.0 to disable the gate. See [`DEFAULT_MIN_TONE_PURITY`].
    pub min_tone_purity: f32,
    /// When `Some(hz)`, skip pitch acquisition entirely and lock the
    /// streaming Goertzel to this exact frequency. Disables the
    /// Fisher-quality watchdog so the lock cannot be dropped. Use this
    /// to force the decoder onto a known target tone (e.g. 600 Hz from
    /// a calibration recording or a known band-conditions target),
    /// or to isolate "is the failure acquisition or downstream?".
    pub force_pitch_hz: Option<f32>,
    /// Number of side bins per side to add to the target Goertzel for
    /// "wide-bin sniff." 0 (default) = a single Goertzel at the locked
    /// pitch (~40 Hz wide on a 25 ms window). N > 0 also runs Goertzels
    /// at pitch ± k*bin_width for k=1..=N and SUMs all 2N+1 powers as
    /// the effective signal power. Use this for signals whose energy is
    /// smeared across multiple bins (speaker → room → mic re-capture,
    /// drifting transmitters, wide bandpass receivers): a single
    /// Goertzel only catches a slice and the keying envelope flickers.
    /// Each extra side bin also lifts the noise floor slightly, so this
    /// trades selectivity for keying stability.
    pub wide_bin_count: u8,
    /// Drop on-runs shorter than this fraction of the estimated dot
    /// length (e.g. 0.3 → ignore pulses shorter than 30 % of one dit).
    /// 0 = disabled. Mic recordings have a constant low-level wiggle
    /// that crosses threshold for ~1–5 ms at a time, producing ghost
    /// "E"/"I" characters in silent stretches; legitimate dits are 40–
    /// 100 ms even at 30 WPM, so a fractional gate kills the ghosts
    /// without touching real keying. Mirrors the "min_pulse" advice
    /// from the field-mic review.
    pub min_pulse_dot_fraction: f32,
    /// Bridge off-runs shorter than this fraction of the estimated dot
    /// length (e.g. 0.3 → ignore gaps shorter than 30 % of one dit).
    /// 0 = disabled. Twin of [`min_pulse_dot_fraction`]: re-captured
    /// acoustic CW often chatters around threshold inside a real
    /// key-down, producing tiny false gaps that fragment a single dah
    /// into "T T" or break a dit run. Suppressing those gaps stabilises
    /// the envelope without touching legitimate intra-letter spacing
    /// (one dot ≈ 40 ms at 30 WPM, so 0.3 = ~12 ms ceiling).
    pub min_gap_dot_fraction: f32,
    /// Hysteresis fraction on the on/off keying threshold. When OFF,
    /// require `power > threshold * (1 + h/2)` to flip ON; when ON,
    /// require `power < threshold * (1 - h/2)` to flip OFF. 0 =
    /// disabled (single threshold, the historical behaviour). Built
    /// to address the "harsh in-band noise" failure mode where the
    /// smoothed envelope chatters across a single threshold many
    /// times per CW element, fragmenting one dah into `dit dit`
    /// or shattering inter-element gaps into runs of false `E`/`T`
    /// characters even when the pitch lock and Fisher quality
    /// watchdog are healthy. Typical useful range 0.2..0.6; values
    /// >= 1.0 will block all on→off transitions.
    pub hysteresis_fraction: f32,
    /// CFAR (constant-false-alarm-rate) style keying: feed the threshold
    /// detector the *residual* `max(0, smoothed_target - smoothed_noise)`
    /// instead of raw target Goertzel power. The smoothed noise is the
    /// q25 of side-bin Goertzels (already maintained for SNR), so
    /// harsh same-band stochastic noise (band-passed white noise; deep
    /// tremolo on a noise bed) collapses toward zero, leaving real
    /// key-down energy to dominate the rolling-quantile threshold. Raw
    /// `smoothed` is still emitted in `Power` events so the UI is
    /// unchanged. Default false. See issue #322 for the harsh-tier
    /// 30 WPM scenarios this targets.
    pub cfar_keying: bool,
}

impl DecoderConfig {
    pub fn defaults() -> Self {
        Self {
            min_snr_db: DEFAULT_MIN_SNR_DB,
            pitch_min_snr_db: DEFAULT_PITCH_MIN_SNR_DB,
            threshold_scale: DEFAULT_THRESHOLD_SCALE,
            auto_threshold: true,
            experimental_range_lock: false,
            range_lock_min_hz: DEFAULT_RANGE_LOCK_MIN_HZ,
            range_lock_max_hz: DEFAULT_RANGE_LOCK_MAX_HZ,
            min_tone_purity: DEFAULT_MIN_TONE_PURITY,
            force_pitch_hz: None,
            wide_bin_count: 0,
            min_pulse_dot_fraction: 0.0,
            min_gap_dot_fraction: 0.0,
            hysteresis_fraction: 0.0,
            cfar_keying: false,
        }
    }
    /// Convert min_snr_db → linear power ratio for the inner gate.
    pub fn min_snr_linear(&self) -> f32 {
        10.0_f32.powf(self.min_snr_db / 10.0)
    }
    pub fn pitch_min_snr_linear(&self) -> f32 {
        10.0_f32.powf(self.pitch_min_snr_db / 10.0)
    }

    pub fn pitch_lock_bounds(&self) -> Option<(f32, f32)> {
        if !self.experimental_range_lock {
            return None;
        }

        let lo = self
            .range_lock_min_hz
            .min(self.range_lock_max_hz)
            .max(FREQ_MIN_HZ);
        let hi = self
            .range_lock_min_hz
            .max(self.range_lock_max_hz)
            .min(FREQ_MAX_HZ);
        (lo < hi).then_some((lo, hi))
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
    /// Pitch lock dropped because the trial-decode quality watchdog
    /// concluded the signal is no longer coherent CW. The decoder
    /// resets and starts hunting for a new lock from scratch.
    PitchLost { reason: String },
    /// New WPM estimate (smoothed).
    WpmUpdate { wpm: f32 },
    /// A decoded character emitted in real time.
    Char {
        ch: char,
        morse: String,
        pitch_hz: Option<f32>,
        /// Peak instantaneous tone-purity ratio observed during the on-runs
        /// that produced this character. None until the purity gate has run
        /// at least once. Useful as a per-character debug overlay so the
        /// operator can see when emitted letters were marginal vs solid.
        tone_purity: Option<f32>,
    },
    /// Word boundary detected.
    Word,
    /// Letter could not be decoded (unknown morse pattern).
    Garbled {
        morse: String,
        pitch_hz: Option<f32>,
        tone_purity: Option<f32>,
    },
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
    /// Coarse confidence state that surfaces decoder lifecycle to the
    /// UI so operators can see when emitted characters are trusted vs
    /// when the decoder is still searching/verifying.
    ///
    /// Lifecycle:
    /// * `Hunting` — no pitch lock; decoder is searching the spectrum
    ///   for a viable CW tone. No `Char` events will be emitted.
    /// * `Probation` — a candidate pitch was just locked but has not
    ///   yet passed its first quality check. `Char`/`Garbled`/`Word`/
    ///   `WpmUpdate` events are SUPPRESSED so the operator never sees
    ///   characters from a bogus lock (e.g. one made on voice
    ///   formants in audio recorded before the CW transmission
    ///   actually starts).
    /// * `Locked` — lock survived its probation; decoder is emitting
    ///   characters with normal confidence. The watchdog can still
    ///   drop the lock later if the signal degrades.
    Confidence { state: ConfidenceState },
}

/// See [`StreamEvent::Confidence`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfidenceState {
    Hunting,
    Probation,
    Locked,
}

impl ConfidenceState {
    /// Stable lowercase label, used by the JSON event bridge so the
    /// .NET GUI can render a status badge.
    pub fn as_str(self) -> &'static str {
        match self {
            ConfidenceState::Hunting => "hunting",
            ConfidenceState::Probation => "probation",
            ConfidenceState::Locked => "locked",
        }
    }
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
fn detect_pitch(
    samples: &[f32],
    sample_rate: u32,
    min_snr_linear: f32,
    pitch_bounds: Option<(f32, f32)>,
) -> Result<f32> {
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
    let half = fft_size / 2;
    // Per-bin time-series power (one entry per FFT frame), for keying-score eval.
    let mut frames: Vec<Vec<f32>> = vec![Vec::new(); half];
    let mut sum = vec![0.0_f32; half];
    let mut count = 0;
    for chunk in samples.windows(fft_size).step_by(step) {
        let mut buf: Vec<Complex<f32>> = chunk
            .iter()
            .zip(win.iter())
            .map(|(s, w)| Complex::new(s * w, 0.0))
            .collect();
        fft.process(&mut buf);
        for (i, v) in buf.iter().take(half).enumerate() {
            let p = v.norm_sqr();
            sum[i] += p;
            frames[i].push(p);
        }
        count += 1;
    }
    if count == 0 {
        return Err(anyhow!("no FFT frames"));
    }
    let df = sample_rate as f32 / fft_size as f32;
    // Build candidates: every in-band bin that is a local maximum (peak)
    // in the cumulative spectrum. We then score each by combined power AND
    // bimodality of its time-series, so a continuous carrier (loud but flat
    // over time) loses to a real CW keying signal (slightly quieter but
    // strongly ON/OFF over time).
    let mut in_band_powers: Vec<f32> = Vec::new();
    let mut candidates: Vec<usize> = Vec::new();
    let range_center = pitch_bounds.map(|(lo, hi)| (lo + hi) * 0.5);
    for (i, &p) in sum.iter().enumerate() {
        let f = i as f32 * df;
        if !(FREQ_MIN_HZ..=FREQ_MAX_HZ).contains(&f) {
            continue;
        }
        if let Some((lo, hi)) = pitch_bounds {
            if !(lo..=hi).contains(&f) {
                continue;
            }
        }
        in_band_powers.push(p);
        // Local max with a 5-bin neighbourhood. This avoids picking flat
        // shoulders of a single broader peak as separate candidates.
        let lo = i.saturating_sub(2);
        let hi = (i + 2).min(half - 1);
        let mut is_peak = true;
        for (j, &val) in sum.iter().enumerate().take(hi + 1).skip(lo) {
            if j != i && val > p {
                is_peak = false;
                break;
            }
        }
        if is_peak && p > 0.0 {
            candidates.push(i);
        }
    }
    if candidates.is_empty() || in_band_powers.is_empty() {
        return Err(anyhow!("no candidate peaks in band"));
    }
    // Compute peak/median ratio over in-band power for the SNR gate.
    let mut sorted = in_band_powers.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = sorted[sorted.len() / 2].max(1e-30);

    // Score each candidate.
    //
    // Power alone is misleading: a chorus of nearby stations or a strong
    // continuous carrier can be loud yet decode to garbage. We add a
    // bimodality (keying-ratio) factor so a slightly quieter strongly
    // keyed bin can beat a louder near-flat one. SNR uses a sqrt
    // weighting so absolute loudness can't completely dominate.
    //
    // First pass: build a shortlist of candidates that clear the SNR
    // gate and have at least some bimodality. We use a permissive
    // keying threshold (3.0 instead of 5.0) so faint signals — where
    // background noise raises the floor and shrinks q90/q10 — still
    // make the list. The trial-decode Fisher score below is what
    // ultimately separates real CW from noise.
    let mut shortlist: Vec<(usize, f32)> = Vec::new();
    for &i in &candidates {
        let p = sum[i];
        let snr = p / median;
        if snr < min_snr_linear {
            continue;
        }
        let mut series = frames[i].clone();
        if series.len() < 8 {
            continue;
        }
        series.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let q10 = series[series.len() / 10].max(1e-30);
        let q90 = series[series.len() * 9 / 10].max(1e-30);
        let keying = (q90 / q10).clamp(1.0, 5000.0);
        if keying < 3.0 {
            continue;
        }
        let prelim = snr.sqrt() * keying;
        shortlist.push((i, prelim));
    }
    if shortlist.is_empty() {
        return Err(anyhow!(
            "no candidate cleared SNR gate of {:.1} dB",
            10.0 * min_snr_linear.log10()
        ));
    }
    shortlist.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let center = range_center.unwrap_or(0.0);
                let a_dist = (a.0 as f32 * df - center).abs();
                let b_dist = (b.0 as f32 * df - center).abs();
                a_dist
                    .partial_cmp(&b_dist)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    // Trial-decode the top N candidates and rank by dit/dah cluster
    // separation (Fisher discriminant on on-pulse durations). For
    // very faint signals the FFT-based prelim score is dominated by
    // noise statistics, so we lean heavily on the trial Fisher score
    // for the actual lock decision and apply a hard minimum so we
    // never lock onto pure noise.
    //
    // Also evaluate adjacent-bin Fisher (±1 bin) and take the max so
    // we don't reject a real signal just because the FFT bin centre
    // lies a few Hz off the actual tone.
    const TRIAL_TOP_N: usize = 8;
    const MIN_LOCK_FISHER: f32 = 5.0;
    // Multiplier on prelim leader's Fisher that a challenger must
    // beat to oust it. Keeps stable single-signal cases from
    // flapping on near-ties.
    const TRIAL_OUST_MARGIN: f32 = 1.25;
    let n = shortlist.len().min(TRIAL_TOP_N);
    let debug = std::env::var("CW_PITCH_DEBUG").is_ok();
    let eval_fisher = |idx: usize| -> f32 {
        let mut best = 0.0_f32;
        for di in -1i32..=1 {
            let bin = idx as i32 + di;
            if bin <= 0 {
                continue;
            }
            let p = bin as f32 * df;
            if !(FREQ_MIN_HZ..=FREQ_MAX_HZ).contains(&p) {
                continue;
            }
            let s = trial_decode_score(samples, sample_rate, p);
            if s > best {
                best = s;
            }
        }
        best
    };
    let mut scored: Vec<(usize, f32, f32)> = Vec::with_capacity(n); // (idx, prelim, fisher)
    for &(idx, prelim) in &shortlist[..n] {
        let fisher = eval_fisher(idx);
        if debug {
            eprintln!(
                "[cw-decoder pitch trial] cand {:.1} Hz prelim={:.2} fisher={:.3}",
                idx as f32 * df,
                prelim,
                fisher
            );
        }
        scored.push((idx, prelim, fisher));
    }
    // Hard quality gate: refuse to lock unless at least one candidate
    // produces dit/dah clusters cleanly separated enough to be
    // distinguishable from background noise.
    let max_fisher = scored.iter().map(|(_, _, f)| *f).fold(0.0_f32, f32::max);
    if max_fisher < MIN_LOCK_FISHER {
        if debug {
            eprintln!(
                "[cw-decoder pitch trial] no candidate cleared MIN_LOCK_FISHER={MIN_LOCK_FISHER:.1} (best={max_fisher:.2})"
            );
        }
        return Err(anyhow!(
            "no candidate produced clean dit/dah clusters (best Fisher={max_fisher:.2}, need >={MIN_LOCK_FISHER:.1})"
        ));
    }
    // Pick the leader by Fisher; require it to clearly beat the
    // prelim FFT/keying leader before ousting it (avoids flapping).
    let prelim_leader = scored[0]; // shortlist already sorted by prelim
    let mut sorted_by_fisher = scored.clone();
    sorted_by_fisher.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    let fisher_leader = sorted_by_fisher[0];
    let chosen = if fisher_leader.0 == prelim_leader.0 {
        prelim_leader.0
    } else if fisher_leader.2 > prelim_leader.2 * TRIAL_OUST_MARGIN {
        fisher_leader.0
    } else {
        prelim_leader.0
    };
    if debug {
        eprintln!(
            "[cw-decoder pitch trial] prelim_leader={:.1} Hz fisher={:.3}, fisher_leader={:.1} Hz fisher={:.3} -> chose {:.1} Hz",
            prelim_leader.0 as f32 * df,
            prelim_leader.2,
            fisher_leader.0 as f32 * df,
            fisher_leader.2,
            chosen as f32 * df
        );
    }
    // Centroid refinement: the FFT bin with the largest *single-bin*
    // power can sit on the edge of a broad ridge (classic failure mode
    // on speaker→mic re-captures, where the tone is smeared across
    // ~200 Hz). Expand outward from the chosen bin while neighbouring
    // bins stay within −6 dB of the peak, then return the
    // power-weighted frequency centroid of that span. This pulls the
    // lock toward the *centre* of the ridge instead of an edge.
    let centroid_hz = pitch_centroid_hz(&sum, chosen, df);
    if debug {
        let spread_hz = pitch_ridge_spread_hz(&sum, chosen, df);
        eprintln!(
            "[cw-decoder pitch trial] centroid refinement: peak={:.1} Hz centroid={:.1} Hz spread={:.1} Hz",
            chosen as f32 * df,
            centroid_hz,
            spread_hz
        );
    }
    Ok(centroid_hz)
}

/// Power-weighted frequency centroid of the ridge around `peak_bin`.
///
/// Expands left and right while neighbouring bin power stays within
/// −6 dB (= 0.25×) of the peak, then returns Σ(p_i · f_i) / Σ p_i over
/// the span. Falls back to the raw peak frequency if the span is just
/// the peak alone.
fn pitch_centroid_hz(sum: &[f32], peak_bin: usize, df: f32) -> f32 {
    if sum.is_empty() || peak_bin >= sum.len() {
        return peak_bin as f32 * df;
    }
    let peak_p = sum[peak_bin];
    if peak_p <= 0.0 {
        return peak_bin as f32 * df;
    }
    let floor = peak_p * 0.25;
    let mut lo = peak_bin;
    while lo > 0 && sum[lo - 1] >= floor {
        lo -= 1;
    }
    let mut hi = peak_bin;
    while hi + 1 < sum.len() && sum[hi + 1] >= floor {
        hi += 1;
    }
    if hi == lo {
        return peak_bin as f32 * df;
    }
    let mut num = 0.0_f32;
    let mut den = 0.0_f32;
    for (j, &val) in sum.iter().enumerate().take(hi + 1).skip(lo) {
        num += val * j as f32 * df;
        den += val;
    }
    if den > 0.0 {
        num / den
    } else {
        peak_bin as f32 * df
    }
}

/// Width (Hz) of the −6 dB ridge around `peak_bin`. Diagnostic only.
fn pitch_ridge_spread_hz(sum: &[f32], peak_bin: usize, df: f32) -> f32 {
    if sum.is_empty() || peak_bin >= sum.len() {
        return 0.0;
    }
    let peak_p = sum[peak_bin];
    if peak_p <= 0.0 {
        return 0.0;
    }
    let floor = peak_p * 0.25;
    let mut lo = peak_bin;
    while lo > 0 && sum[lo - 1] >= floor {
        lo -= 1;
    }
    let mut hi = peak_bin;
    while hi + 1 < sum.len() && sum[hi + 1] >= floor {
        hi += 1;
    }
    (hi - lo) as f32 * df
}

/// Quick "does this pitch decode to clean morse?" probe.
///
/// Runs a Goertzel at `pitch_hz` over the audio, extracts on-pulse
/// durations, and returns Fisher's discriminant on the lengths after
/// 1-D k-means into two groups (dits, dahs):
///
///   F = (mean_dah - mean_dit)² / (var_dit + var_dah + ε)
///
/// Real CW produces well-separated tight clusters at ~1 and ~3 dit
/// units, giving a high F. Chorusing stations or spurious carriers
/// produce overlapping or random-looking durations and score low.
///
/// The score is further attenuated when the dah/dit ratio falls
/// outside [2.0, 4.5] (real CW is exactly 3 by spec; we tolerate
/// reasonable slop) and when there are too few intervals to be
/// statistically meaningful.
pub fn trial_decode_score(samples: &[f32], sample_rate: u32, pitch_hz: f32) -> f32 {
    let win_size = (sample_rate as f32 * GOERTZEL_WIN_MS / 1000.0) as usize;
    let win_size = win_size.max(32);
    let step = (win_size / 4).max(1);
    let mut g = Goertzel::new(pitch_hz, sample_rate, win_size, step);
    let mut power: Vec<f32> = Vec::with_capacity(samples.len() / step + 1);
    g.push(samples, &mut power);
    if power.len() < 16 {
        return 0.0;
    }
    let mut sorted = power.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let q10 = sorted[sorted.len() / 10].max(1e-30);
    let q90 = sorted[sorted.len() * 9 / 10].max(1e-30);
    if q90 / q10 < 4.0 {
        // Not bimodal enough to call it a keyed signal.
        return 0.0;
    }
    // Threshold midway in log space (matches detect_pitch's prelim threshold).
    let log_thr = 0.5 * (q10.ln() + q90.ln());
    let thr = log_thr.exp();
    // Walk power samples, collecting consecutive-on run lengths.
    let mut on_runs: Vec<f32> = Vec::new();
    let mut cur_on = 0usize;
    for &p in &power {
        if p > thr {
            cur_on += 1;
        } else if cur_on > 0 {
            on_runs.push(cur_on as f32);
            cur_on = 0;
        }
    }
    if cur_on > 0 {
        on_runs.push(cur_on as f32);
    }
    if on_runs.len() < 6 {
        return 0.0;
    }
    // K-means with k=2, init by min/max.
    let init_min = on_runs.iter().cloned().fold(f32::INFINITY, f32::min);
    let init_max = on_runs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let mut c_low = init_min;
    let mut c_high = init_max;
    if (c_high - c_low) < 0.5 {
        return 0.0;
    }
    for _ in 0..20 {
        let (mut sum_lo, mut sum_hi, mut n_lo, mut n_hi) = (0.0_f32, 0.0_f32, 0usize, 0usize);
        for &x in &on_runs {
            if (x - c_low).abs() <= (x - c_high).abs() {
                sum_lo += x;
                n_lo += 1;
            } else {
                sum_hi += x;
                n_hi += 1;
            }
        }
        if n_lo == 0 || n_hi == 0 {
            return 0.0;
        }
        let new_lo = sum_lo / n_lo as f32;
        let new_hi = sum_hi / n_hi as f32;
        if (new_lo - c_low).abs() < 1e-3 && (new_hi - c_high).abs() < 1e-3 {
            c_low = new_lo;
            c_high = new_hi;
            break;
        }
        c_low = new_lo;
        c_high = new_hi;
    }
    // Compute per-cluster variance.
    let (mut var_lo, mut var_hi, mut n_lo, mut n_hi) = (0.0_f32, 0.0_f32, 0usize, 0usize);
    for &x in &on_runs {
        if (x - c_low).abs() <= (x - c_high).abs() {
            var_lo += (x - c_low).powi(2);
            n_lo += 1;
        } else {
            var_hi += (x - c_high).powi(2);
            n_hi += 1;
        }
    }
    if n_lo < 2 || n_hi < 2 {
        return 0.0;
    }
    var_lo /= n_lo as f32;
    var_hi /= n_hi as f32;
    let fisher = (c_high - c_low).powi(2) / (var_lo + var_hi + 1e-6);
    // Penalise clusters whose ratio doesn't look like dit/dah.
    let ratio = c_high / c_low.max(1e-6);
    let ratio_pen = if (2.0..=4.5).contains(&ratio) {
        1.0
    } else if (1.5..=6.0).contains(&ratio) {
        0.4
    } else {
        0.1
    };
    // Penalise too few intervals — Fisher is unstable with small N.
    let n_pen = ((on_runs.len() as f32) / 10.0).min(1.0);
    fisher * ratio_pen * n_pen
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
    /// Latched once `good_streak` reaches MATURITY; only cleared when the
    /// gate fully closes via STICKY_BAD. This prevents transient QRM/fade
    /// flickers from re-arming the maturity wait and eating real letters.
    mature: bool,
}

impl RhythmGate {
    const WINDOW: usize = 24;
    const OPEN_FRACTION: f32 = 0.65;
    const MIN_INTERVALS: usize = 8;
    const STICKY_BAD: usize = 8;
    const DUTY_MIN: f32 = 0.20;
    const DUTY_MAX: f32 = 0.70;
    const TOL_FRAC: f32 = 0.35;
    const TOL_FLOOR: f32 = 0.40;
    const BIMODAL_MIN: usize = 2;
    /// Number of consecutive good intervals required after the gate first
    /// opens before character emission is allowed.
    const MATURITY: usize = 4;

    fn new() -> Self {
        Self {
            window: VecDeque::with_capacity(Self::WINDOW),
            is_open: false,
            bad_streak: 0,
            good_streak: 0,
            mature: false,
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
                self.mature = false;
            }
        } else {
            self.is_open = now_open;
            if now_open {
                self.bad_streak = 0;
                self.good_streak = self.good_streak.saturating_add(1);
                if self.good_streak >= Self::MATURITY {
                    self.mature = true;
                }
            }
            // NOTE: do NOT zero good_streak/mature on a single bad interval.
            // Real QSOs have constant micro-fading and QRM that would otherwise
            // re-trigger the maturity wait every few seconds, eating whole
            // letters at each flicker. Maturity is latched until the gate
            // actually closes via STICKY_BAD above.
        }
    }

    fn is_open(&self) -> bool {
        self.is_open && self.mature
    }

    /// True if the gate has ever reached maturity since the last full
    /// close. Used by the classifier to allow a multi-element morse
    /// rescue when the gate transiently dips during QSB without
    /// rescuing single-element ghosts (E/T) from pure noise.
    fn was_recently_mature(&self) -> bool {
        self.mature
    }

    #[allow(dead_code)]
    fn reset(&mut self) {
        self.window.clear();
        self.is_open = false;
        self.bad_streak = 0;
        self.good_streak = 0;
        self.mature = false;
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
    /// Ignored when `auto_threshold` is true.
    threshold_scale: f32,
    /// When true, `update_threshold` derives the scale dynamically from
    /// the recent Q90/Q10 SNR margin so the decoder follows QSB without
    /// operator intervention.
    auto_threshold: bool,

    /// Drop on-runs shorter than this fraction of the dot length. 0 disables.
    min_pulse_dot_fraction: f32,

    /// Bridge off-runs shorter than this fraction of the dot length. 0 disables.
    min_gap_dot_fraction: f32,

    /// Hysteresis fraction on the keying threshold. 0 disables (single
    /// threshold, historical behaviour). See [`DecoderConfig::hysteresis_fraction`].
    hysteresis_fraction: f32,

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
    /// Peak instantaneous tone-purity ratio observed during the on-runs that
    /// have contributed to `current_letter`. Reset each time a letter is
    /// emitted. None when the purity gate hasn't seen a value yet (e.g.
    /// pre-lock or with the gate disabled).
    current_letter_peak_purity: Option<f32>,

    /// Pending ON-run length (in power samples) waiting to see whether
    /// the next interval is a real OFF gap or a chatter-bridge ON. Used
    /// by the min-gap merge sanitizer in `push_edge` so a short OFF
    /// inside a real key-down element no longer fragments a dah into
    /// two adjacent dits — the surrounding ON intervals are *merged*
    /// into one, not just left to "continue accumulating".
    pending_on_len: Option<usize>,
    /// Total length of the short OFF gap(s) currently being bridged. Folded
    /// into the merged ON run when the next ON arrives; flushed as a real
    /// OFF when a long-enough OFF arrives.
    pending_short_gap_len: usize,

    // --- Bench / diagnostic counters. Cheap to maintain and surfaced via
    // `debug_counters()` so bench-latency can publish them in its result
    // JSON for #320 regression watching.
    raw_edges_total: u64,
    short_pulses_dropped: u64,
    short_gaps_bridged: u64,
    on_runs_merged: u64,
    invalid_on_duration_dropped: u64,
    rhythm_closed_letters_dropped: u64,
    single_element_rescue_suppressed: u64,
    chars_emitted: u64,
    garbled_emitted: u64,
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
            auto_threshold: true,
            min_pulse_dot_fraction: 0.0,
            min_gap_dot_fraction: 0.0,
            hysteresis_fraction: 0.0,
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
            current_letter_peak_purity: None,
            pending_on_len: None,
            pending_short_gap_len: 0,
            raw_edges_total: 0,
            short_pulses_dropped: 0,
            short_gaps_bridged: 0,
            on_runs_merged: 0,
            invalid_on_duration_dropped: 0,
            rhythm_closed_letters_dropped: 0,
            single_element_rescue_suppressed: 0,
            chars_emitted: 0,
            garbled_emitted: 0,
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
        // Use Q10 (noise floor) and Q90 (signal peak) rather than IQR,
        // because typical CW has ~30% on / 70% off (with word gaps), so
        // Q25 and Q75 both sit in the off-region for slow code, biasing
        // the threshold low and missing dits. Q10/Q90 brackets the true
        // bimodal distribution far better across realistic duty cycles.
        let p10 = v[v.len() / 10].max(1e-30);
        let p90 = v[v.len() * 9 / 10].max(1e-30);
        let span = p90 - p10;
        // Pick the threshold scale. In auto mode it tracks the recent
        // SNR margin so the decoder follows QSB without retuning:
        //
        //   * strong clean signal (>=20 dB Q90/Q10): scale ≈ 1.0,
        //     keeps the threshold safely above noise tails
        //   * weak / fading signal (<=5 dB margin): scale ≈ 0.4, drops
        //     the threshold close to noise so dits aren't missed
        //   * linear interp between, clamped to [0.4, 1.0]
        //
        // In manual mode the operator-set `threshold_scale` is honoured
        // verbatim, so existing slider workflows still work.
        let scale = if self.auto_threshold {
            let margin_db = 10.0 * (p90 / p10).log10();
            let t = ((margin_db - 5.0) / 15.0).clamp(0.0, 1.0);
            0.4 + t * 0.6
        } else {
            self.threshold_scale
        };
        // Threshold sits halfway between off- and on-state by default;
        // the chosen scale slides it toward noise (<1) or toward signal (>1).
        self.threshold = p10 + span * 0.5 * scale;
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

    fn classify_letter(&mut self, pitch_hz: Option<f32>) -> Option<StreamEvent> {
        if self.current_letter.is_empty() {
            return None;
        }
        let morse = std::mem::take(&mut self.current_letter);
        let tone_purity = self.current_letter_peak_purity.take();
        if let Some(c) = morse_to_char(&morse) {
            Some(StreamEvent::Char {
                ch: c,
                morse,
                pitch_hz,
                tone_purity,
            })
        } else {
            Some(StreamEvent::Garbled {
                morse,
                pitch_hz,
                tone_purity,
            })
        }
    }

    /// Consume a single (already-sanitized) interval and emit decode events.
    /// "Sanitized" means the chatter merge in `push_edge` has already merged
    /// short OFF gaps into the surrounding ON run when appropriate.
    fn consume_interval(
        &mut self,
        ivl: Interval,
        pitch_hz: Option<f32>,
        events: &mut Vec<StreamEvent>,
    ) {
        let dot = match self.dot_len {
            Some(d) if d > 0.0 => d,
            _ => return,
        };
        let len_norm = ivl.len as f32 / dot;
        // Min-pulse gate: drop an *on*-run that is shorter than the
        // configured fraction of the dot length. This is the field-mic
        // ghost-character fix — a constant low-level wiggle that crosses
        // threshold for a few ms otherwise gets classified as a dit and
        // produces "E"/"I" runs in silent stretches. We deliberately
        // do NOT touch off-runs here: short gaps inside a letter are
        // already handled by the merge sanitizer in `push_edge`.
        if ivl.is_on && self.min_pulse_dot_fraction > 0.0 && len_norm < self.min_pulse_dot_fraction
        {
            self.short_pulses_dropped += 1;
            return;
        }
        // Hard ON-duration sanity gate (#320): no real Morse element is
        // shorter than ~0.4 dot or longer than ~1.6 dah. Anything outside
        // [MIN_ON_DOT_FRAC, MAX_ON_DOT_FRAC] is treated as bad keying or
        // impulsive noise; we drop it AND clear any in-progress letter
        // so a giant ON blob from QRM can't be classified as a dah, and
        // a tiny chatter pulse can't be classified as a dit.
        if ivl.is_on && !(MIN_ON_DOT_FRAC..=MAX_ON_DOT_FRAC).contains(&len_norm) {
            self.invalid_on_duration_dropped += 1;
            self.current_letter.clear();
            self.current_letter_peak_purity = None;
            return;
        }
        // Feed the rhythm gate first so it always sees every interval.
        self.rhythm.push(ivl, dot);
        if ivl.is_on {
            if len_norm < DIT_DAH_BOUNDARY {
                self.current_letter.push('.');
            } else {
                self.current_letter.push('-');
            }
            // WPM is a "decoder is alive" indicator. Always emit when we have
            // a tempo estimate, so the operator can see the decoder is still
            // tracking even when the rhythm gate is briefly closed.
            if let Some(w) = self.current_wpm() {
                events.push(StreamEvent::WpmUpdate { wpm: w });
            }
        } else if len_norm > LETTER_SPACE_BOUNDARY {
            // Letter / word boundary. The original logic emitted whenever the
            // pattern decoded to a real character, which lets single-element
            // E (.) and T (-) leak through during dense in-band noise (#320).
            // The conservative version: open gate always emits; otherwise we
            // only rescue *multi-element* valid morse, and only while the
            // rhythm gate has been mature recently.
            let gate_open = self.rhythm.is_open();
            let pattern = self.current_letter.clone();
            let valid_morse = !pattern.is_empty() && morse_to_char(&pattern).is_some();
            let single_element = pattern.chars().count() == 1;
            let allow_rescue = valid_morse && !single_element && self.rhythm.was_recently_mature();
            if gate_open || allow_rescue {
                if let Some(ev) = self.classify_letter(pitch_hz) {
                    match &ev {
                        StreamEvent::Char { .. } => self.chars_emitted += 1,
                        StreamEvent::Garbled { .. } => self.garbled_emitted += 1,
                        _ => {}
                    }
                    events.push(ev);
                }
                if len_norm > WORD_SPACE_BOUNDARY {
                    events.push(StreamEvent::Word);
                }
            } else {
                // Gate is closed AND pattern is junk/single-element → drop it
                // so we don't accumulate a long tail of dots/dashes from noise.
                if valid_morse && single_element {
                    self.single_element_rescue_suppressed += 1;
                } else if !pattern.is_empty() {
                    self.rhythm_closed_letters_dropped += 1;
                }
                self.current_letter.clear();
                self.current_letter_peak_purity = None;
            }
        }
    }

    /// Pre-classifier sanitizer that merges short OFF gaps into the
    /// surrounding ON run BEFORE feeding the timing history and the
    /// classifier. The previous behaviour merely *dropped* a short OFF
    /// interval, which still left the two adjacent ON runs to be
    /// classified independently — a real dah broken by a tiny envelope
    /// chatter would still come out as ". .". This routine returns up
    /// to two intervals to forward (a flushed merged ON and/or the real
    /// OFF) and updates the merge counters.
    fn sanitize_interval(&mut self, ivl: Interval) -> (Option<Interval>, Option<Interval>) {
        let dot = self.dot_len;
        if ivl.is_on {
            if let Some(prev_on) = self.pending_on_len.take() {
                let merged = prev_on + self.pending_short_gap_len + ivl.len;
                self.pending_short_gap_len = 0;
                self.pending_on_len = Some(merged);
                self.on_runs_merged += 1;
            } else {
                self.pending_on_len = Some(ivl.len);
            }
            return (None, None);
        }

        let is_short_gap = match dot {
            Some(d) if d > 0.0 && self.min_gap_dot_fraction > 0.0 => {
                (ivl.len as f32 / d) < self.min_gap_dot_fraction
            }
            _ => false,
        };
        if is_short_gap && self.pending_on_len.is_some() {
            self.pending_short_gap_len += ivl.len;
            self.short_gaps_bridged += 1;
            return (None, None);
        }

        // Real OFF gap: flush any pending ON, then emit this OFF.
        if let Some(on_len) = self.pending_on_len.take() {
            self.pending_short_gap_len = 0;
            (
                Some(Interval {
                    len: on_len,
                    is_on: true,
                }),
                Some(ivl),
            )
        } else {
            (None, Some(ivl))
        }
    }

    /// Public accessor for bench/diagnostic counters.
    pub fn debug_counters(&self) -> serde_json::Value {
        serde_json::json!({
            "raw_edges_total": self.raw_edges_total,
            "short_pulses_dropped": self.short_pulses_dropped,
            "short_gaps_bridged": self.short_gaps_bridged,
            "on_runs_merged": self.on_runs_merged,
            "invalid_on_duration_dropped": self.invalid_on_duration_dropped,
            "rhythm_closed_letters_dropped": self.rhythm_closed_letters_dropped,
            "single_element_rescue_suppressed": self.single_element_rescue_suppressed,
            "chars_emitted": self.chars_emitted,
            "garbled_emitted": self.garbled_emitted,
        })
    }

    /// Push one edge-terminated run into the decoder. During bootstrap we
    /// buffer; once primed, we emit events immediately. Routes the raw
    /// interval through `sanitize_interval` first (Patch 2 from #320) so
    /// short OFF chatter inside a real key-down element merges the
    /// surrounding ON runs into one before they reach the timing
    /// history, the rhythm gate, or the symbol classifier.
    fn push_edge(&mut self, ivl: Interval, pitch_hz: Option<f32>, events: &mut Vec<StreamEvent>) {
        self.raw_edges_total += 1;
        let (a, b) = self.sanitize_interval(ivl);
        for forward in [a, b].into_iter().flatten() {
            self.process_sanitized(forward, pitch_hz, events);
        }
    }

    /// Process a single already-sanitized interval: update history,
    /// recalibrate dit/dah, then either buffer (bootstrap) or consume.
    fn process_sanitized(
        &mut self,
        ivl: Interval,
        pitch_hz: Option<f32>,
        events: &mut Vec<StreamEvent>,
    ) {
        self.push_interval(ivl);
        self.recalibrate_from_history();

        if !self.primed {
            self.bootstrap.push(ivl);
            let have_contrast = self.dot_len.is_some()
                && self.dah_len.is_some()
                && self.bootstrap.len() >= PRIME_INTERVALS;
            if have_contrast {
                if let Some(w) = self.current_wpm() {
                    events.push(StreamEvent::WpmUpdate { wpm: w });
                }
                let buffered = std::mem::take(&mut self.bootstrap);
                self.primed = true;
                for b in buffered {
                    self.consume_interval(b, pitch_hz, events);
                }
            }
            return;
        }

        self.consume_interval(ivl, pitch_hz, events);
    }

    /// Feed one power-signal sample.
    ///
    /// `snr_ok` indicates whether the smoothed off-band noise reference says
    /// the bin is meaningfully louder than the noise floor (gain-independent,
    /// long-window). `purity_ok` indicates whether the *instantaneous*
    /// adjacent-bin ratio says the energy in the locked bin actually looks
    /// like a narrowband tone vs a broadband impulse. Both must be true to
    /// treat the sample as key-down. `tone_purity` is the raw ratio at this
    /// sample (used for per-character debug overlay); 0.0 when not measured.
    fn push_power(
        &mut self,
        p: f32,
        snr_ok: bool,
        purity_ok: bool,
        tone_purity: f32,
        pitch_hz: Option<f32>,
        events: &mut Vec<StreamEvent>,
    ) {
        if self.power_history.len() == self.power_capacity {
            self.power_history.pop_front();
        }
        self.power_history.push_back(p);

        self.threshold_dirty_count += 1;
        if self.threshold_dirty_count > (self.power_rate * 0.25) as usize {
            self.threshold_dirty_count = 0;
            self.update_threshold();
        }
        if self.threshold == 0.0 {
            return;
        }

        // Three gates must agree before we treat a sample as key-down:
        //   1. amplitude above the adaptive threshold (kills the noise floor)
        //   2. smoothed SNR vs off-band reference (gain-independent)
        //   3. instantaneous adjacent-bin tone purity (kills broadband
        //      impulses such as finger snaps and key clicks that briefly
        //      light up the locked bin without being a tone)
        //
        // Optional asymmetric hysteresis on the amplitude gate: when
        // currently OFF, require `p > threshold * (1 + h/2)` to flip on;
        // when currently ON, accept `p > threshold * (1 - h/2)` (i.e. a
        // lower bar) to stay on. The SNR and purity gates are
        // intentionally NOT hysteretic — they're already smoothed signals
        // and adding hysteresis there causes locks to over-stay on
        // genuine impulses.
        let amp_gate = if self.hysteresis_fraction > 0.0 {
            let half = self.hysteresis_fraction * 0.5;
            let amp_threshold = if self.is_on {
                self.threshold * (1.0 - half).max(0.0)
            } else {
                self.threshold * (1.0 + half)
            };
            p > amp_threshold
        } else {
            p > self.threshold
        };
        let above = amp_gate && snr_ok && purity_ok;

        // Track peak purity during on-runs so the emitted character carries
        // a useful debug number. Only update while the gate already says we
        // are on a real tone; otherwise the value is meaningless.
        if above && tone_purity > 0.0 {
            self.current_letter_peak_purity = Some(match self.current_letter_peak_purity {
                Some(prev) => prev.max(tone_purity),
                None => tone_purity,
            });
        }

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
                pitch_hz,
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

    /// Rolling buffer of recent post-lock audio used by the quality
    /// watchdog. Capped at QUALITY_WINDOW_SECONDS worth of samples.
    quality_buf: VecDeque<f32>,
    quality_buf_capacity: usize,
    /// Samples accumulated since the last quality check fired.
    samples_since_quality_check: usize,
    quality_check_threshold: usize,
    /// Number of consecutive quality checks that scored below
    /// MIN_HOLD_FISHER. Lock is dropped when this hits
    /// QUALITY_DROP_CONSECUTIVE.
    quality_failed_consecutive: u32,
    /// Number of consecutive quality checks since we last saw a
    /// healthy Fisher (>= MIN_HOLD_FISHER). Used to gate the
    /// `fast_drop` path: a momentary degenerate window (rms healthy
    /// but Fisher == 0 because the 8 s window was all dahs / all
    /// dits / a very lopsided message) shouldn't drop a lock that
    /// was clearly working a few seconds ago. Only after several
    /// consecutive sub-threshold checks do we accept the loss.
    quality_checks_since_healthy: u32,
    /// True between acquiring a fresh lock and passing the first
    /// post-lock probation check. While set we use a SHORT quality
    /// window so we can detect and drop a bogus lock fast (e.g. one
    /// that happened on voice formants).
    just_locked: bool,
    /// True after a watchdog-driven drop, until the next surviving
    /// lock. Tells `pitch_lock_samples_needed()` to use the smaller
    /// RELOCK_SECONDS window so we re-acquire quickly when (in the
    /// typical case) the signal we lost was masking real CW arriving
    /// right after.
    had_pitch_loss: bool,
    /// Current confidence state. Used to gate which decoder events
    /// reach the operator (chars/wpm/word are suppressed unless
    /// `Locked`) and to avoid duplicate Confidence events.
    confidence: ConfidenceState,
    /// Set to `true` on the very first feed_pcm call so we emit the
    /// initial `Confidence::Hunting` event lazily (we can't push to
    /// `events` from `new()`).
    confidence_emitted: bool,
    /// While we're in `Probation`, decoded character/word/wpm events
    /// are held here instead of being passed straight to the
    /// caller. If the lock survives probation (transitions to
    /// `Locked`) we flush them in order so genuine CW that started
    /// emitting during the verification window isn't silently lost.
    /// On a probation drop we discard them — they came from a bogus
    /// lock.
    held_events: Vec<StreamEvent>,

    goertzel: Option<Goertzel>,
    /// Optional companion Goertzels at pitch ± k*bin_width for k=1..=N
    /// (N = `DecoderConfig::wide_bin_count`). Their power is summed with
    /// the main Goertzel to form the effective signal power, capturing
    /// energy from frequency-smeared signals (acoustic re-capture, drift,
    /// wide receiver bandpass). Empty when wide_bin_count == 0.
    wide_bins: Vec<Goertzel>,
    /// Off-band noise references at multiple offsets around the locked
    /// pitch. We take the *median* per power sample to estimate noise,
    /// which is robust to a single bin landing on adjacent CW or hum.
    /// These same bins are also used *instantaneously* (no smoothing) for
    /// the tone-purity gate: a broadband impulse lights up all of them at
    /// the same instant the target bin spikes; a narrowband CW tone leaves
    /// them at the noise floor.
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
    unlock_power_history: VecDeque<f32>,

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
            quality_buf: VecDeque::with_capacity(
                (TARGET_RATE as f32 * QUALITY_WINDOW_SECONDS) as usize + 1,
            ),
            quality_buf_capacity: (TARGET_RATE as f32 * QUALITY_WINDOW_SECONDS) as usize,
            samples_since_quality_check: 0,
            quality_check_threshold: (TARGET_RATE as f32 * QUALITY_CHECK_SECONDS) as usize,
            quality_failed_consecutive: 0,
            quality_checks_since_healthy: 0,
            just_locked: false,
            had_pitch_loss: false,
            confidence: ConfidenceState::Hunting,
            confidence_emitted: false,
            held_events: Vec::new(),
            goertzel: None,
            wide_bins: Vec::new(),
            noise_bins: Vec::new(),
            smooth_window,
            smooth_buf: VecDeque::with_capacity(smooth_window + 1),
            smooth_sum: 0.0,
            noise_smooth_window,
            noise_smooth_buf: VecDeque::with_capacity(noise_smooth_window + 1),
            noise_smooth_sum: 0.0,
            power_emit_accum: 0.0,
            power_emit_step,
            unlock_power_history: VecDeque::with_capacity(UNLOCK_POWER_HISTORY_EVENTS),
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
    /// Snapshot of the inner decoder's bench/diagnostic counters for
    /// the harness to publish in result JSON. See
    /// [`Decoder::debug_counters`] for the field set.
    pub fn debug_counters(&self) -> serde_json::Value {
        self.decoder.debug_counters()
    }
    #[allow(dead_code)]
    pub fn config(&self) -> DecoderConfig {
        self.config
    }

    /// Apply a new runtime configuration. Safe to call mid-stream — only
    /// affects subsequent power samples and the next pitch re-lock.
    pub fn set_config(&mut self, cfg: DecoderConfig) {
        let prev_force = self.config.force_pitch_hz;
        self.config = cfg;
        self.decoder.threshold_scale = cfg.threshold_scale;
        self.decoder.auto_threshold = cfg.auto_threshold;
        self.decoder.min_pulse_dot_fraction = cfg.min_pulse_dot_fraction.max(0.0);
        self.decoder.min_gap_dot_fraction = cfg.min_gap_dot_fraction.max(0.0);
        self.decoder.hysteresis_fraction = cfg.hysteresis_fraction.max(0.0);
        // If the operator changed (or cleared) the forced pitch, drop the
        // current lock so the next `feed` re-acquires under the new
        // policy. Otherwise we'd be stuck on a stale frequency.
        if cfg.force_pitch_hz != prev_force && self.pitch_locked.is_some() {
            self.drop_pitch_lock(false);
        }
    }

    /// Install a forced pitch lock at exactly `pitch_hz`, bypassing
    /// detect_pitch entirely. Used by `feed` when
    /// `DecoderConfig::force_pitch_hz` is set so the decoder operates
    /// on a known target tone instead of guessing.
    fn install_forced_pitch(&mut self, pitch_hz: f32, events: &mut Vec<StreamEvent>) {
        self.pitch_locked = Some(pitch_hz);
        let win_size = (TARGET_RATE as f32 * GOERTZEL_WIN_MS / 1000.0) as usize;
        let step = (win_size / 4).max(1);
        self.goertzel = Some(Goertzel::new(pitch_hz, TARGET_RATE, win_size, step));
        self.rebuild_wide_bins(pitch_hz, win_size, step);
        self.noise_bins.clear();
        for &off in NOISE_OFFSETS_HZ {
            let lo = pitch_hz - off;
            let hi = pitch_hz + off;
            if lo >= FREQ_MIN_HZ {
                self.noise_bins
                    .push(Goertzel::new(lo, TARGET_RATE, win_size, step));
            }
            if hi <= FREQ_MAX_HZ {
                self.noise_bins
                    .push(Goertzel::new(hi, TARGET_RATE, win_size, step));
            }
        }
        self.unlock_power_history.clear();
        events.push(StreamEvent::PitchUpdate { pitch_hz });
        let drained = std::mem::take(&mut self.pre_lock_buf);
        if !drained.is_empty() {
            self.feed_goertzel(&drained, events);
        }
    }

    /// Feed a chunk of raw audio at `source_rate`. Returns events emitted by
    /// this call (decoded characters, WPM updates, pitch lock).
    pub fn feed(&mut self, samples: &[f32]) -> Result<Vec<StreamEvent>> {
        let mut events = Vec::new();
        // Lazy initial Confidence emit — the operator sees "hunting"
        // from the very first frame even before any audio classifies.
        if !self.confidence_emitted {
            events.push(StreamEvent::Confidence {
                state: self.confidence,
            });
            self.confidence_emitted = true;
        }
        let resampled = self.resample(samples)?;
        if resampled.is_empty() {
            return Ok(events);
        }
        let mut filtered = resampled;
        self.hp.process_in_place(&mut filtered);
        self.lp.process_in_place(&mut filtered);

        // --- Pitch lock / re-eval -------------------------------------
        if self.pitch_locked.is_none() {
            // Forced-pitch mode: skip detection entirely and lock to the
            // operator-supplied frequency the moment we have any audio.
            // No pre-lock buffering, no Fisher gating.
            if let Some(forced) = self.config.force_pitch_hz {
                if forced.is_finite() && forced > 0.0 {
                    self.install_forced_pitch(forced, &mut events);
                    // Forced pitch is "operator declares this IS the
                    // pitch" — skip probation and go straight to
                    // Locked. The watchdog stays off in this mode too.
                    self.set_confidence(ConfidenceState::Locked, &mut events);
                    self.feed_goertzel(&filtered, &mut events);
                    return Ok(self.filter_for_confidence(events));
                }
            }
            self.emit_unlock_power(&filtered, &mut events);
            self.pre_lock_buf.extend_from_slice(&filtered);
            self.try_acquire_pitch_lock(&mut events);
            return Ok(self.filter_for_confidence(events));
        }

        for &s in &filtered {
            self.quality_buf.push_back(s);
            if self.quality_buf.len() > self.quality_buf_capacity {
                self.quality_buf.pop_front();
            }
        }

        self.feed_goertzel(&filtered, &mut events);

        // --- Quality watchdog --------------------------------------------
        // We always evaluate on the full QUALITY_WINDOW_SECONDS rolling
        // buffer (8 s at TARGET_RATE). The first time it fires after a
        // fresh lock acts as the "probation" promotion gate: if Fisher
        // is healthy we transition Probation → Locked and flush any
        // held character events; if it's too low we drop the lock and
        // discard the held events. While the lock is in Probation the
        // operator never sees decoded chars, so a bogus lock made on
        // voice formants or a random impulse gets cleared without
        // polluting the transcript.
        //
        // A Fisher essentially at zero (`< QUALITY_FAST_DROP_FISHER`)
        // drops on the FIRST failed check — there's no QSB pattern that
        // produces zero Fisher; that's "this isn't CW at all".
        self.samples_since_quality_check += filtered.len();
        if self.config.force_pitch_hz.is_none() && self.pitch_locked.is_some() {
            let in_probation = self.just_locked;
            if self.samples_since_quality_check >= self.quality_check_threshold
                && self.quality_buf.len() >= self.quality_buf_capacity
            {
                self.samples_since_quality_check = 0;
                if let Some(pitch) = self.pitch_locked {
                    let buf: Vec<f32> = self.quality_buf.iter().copied().collect();
                    // Silent-buffer skip. If the entire 8-second
                    // quality window is essentially key-up audio
                    // (operator paused, end-of-transmission, deep QSB
                    // null), Fisher will be ~0 because there are no
                    // dit/dah clusters AT ALL — but that is "no
                    // signal", not "bad signal", and dropping lock
                    // here is exactly the bug operators see ("strong
                    // CW comes back and we have to re-acquire from
                    // scratch"). We hold the lock and wait for audio.
                    let mean_sq: f64 = buf.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>()
                        / buf.len().max(1) as f64;
                    let rms = mean_sq.sqrt() as f32;
                    if rms < QUALITY_SILENCE_RMS {
                        if std::env::var("CW_PITCH_DEBUG").is_ok() {
                            eprintln!(
                                "[cw-decoder quality] silent buffer (rms={rms:.4} < {QUALITY_SILENCE_RMS:.4}); holding lock"
                            );
                        }
                        // Don't promote out of probation on a silent
                        // window (no evidence yet) and don't increment
                        // the failure counter. Just wait.
                    } else {
                        let fisher = trial_decode_score(&buf, TARGET_RATE, pitch);
                        let debug = std::env::var("CW_PITCH_DEBUG").is_ok();
                        if debug {
                            eprintln!(
                            "[cw-decoder quality] phase={} pitch={:.1} Hz fisher={:.3} rms={:.3} (hold>={:.1}, fast_drop<{:.1}, fails={})",
                            if in_probation { "probation" } else { "steady" },
                            pitch,
                            fisher,
                            rms,
                            MIN_HOLD_FISHER,
                            QUALITY_FAST_DROP_FISHER,
                            self.quality_failed_consecutive
                        );
                        }
                        let fast_drop = fisher < QUALITY_FAST_DROP_FISHER;
                        // Only allow fast_drop while still in probation
                        // (we have no track record of a healthy lock yet).
                        // Once promoted to steady-state, even Fisher == 0
                        // must accumulate QUALITY_DROP_CONSECUTIVE checks
                        // before we surrender the lock — this prevents a
                        // single degenerate window (e.g. the 8 s buffer
                        // happens to span all-dahs or a long word with no
                        // gaps, so trial_decode_score's bimodal cluster
                        // analysis can't separate dits from dahs and
                        // returns ~0) from dropping a lock that was
                        // clearly healthy a few seconds earlier and will
                        // be healthy again on the next check.
                        if fast_drop && in_probation {
                            let reason = format!(
                            "quality watchdog (probation): Fisher {fisher:.2} < {QUALITY_FAST_DROP_FISHER:.1} (fast drop, signal not coherent CW)"
                        );
                            let preserve_pre_lock_buf =
                                self.seed_fast_relock_from_recent_audio(&buf);
                            self.drop_pitch_lock(preserve_pre_lock_buf);
                            events.push(StreamEvent::PitchLost { reason });
                            self.set_confidence(ConfidenceState::Hunting, &mut events);
                        } else if fisher < MIN_HOLD_FISHER {
                            self.quality_failed_consecutive += 1;
                            self.quality_checks_since_healthy += 1;
                            if self.quality_failed_consecutive >= QUALITY_DROP_CONSECUTIVE {
                                let reason = format!(
                                "quality watchdog: Fisher {:.2} < {:.1} for {} consecutive checks",
                                fisher, MIN_HOLD_FISHER, self.quality_failed_consecutive
                            );
                                let preserve_pre_lock_buf =
                                    self.seed_fast_relock_from_recent_audio(&buf);
                                self.drop_pitch_lock(preserve_pre_lock_buf);
                                events.push(StreamEvent::PitchLost { reason });
                                self.set_confidence(ConfidenceState::Hunting, &mut events);
                            } else if in_probation {
                                // Borderline lock during probation: don't
                                // promote yet, wait for the next check.
                                // Held chars stay buffered.
                            }
                        } else {
                            // Lock is healthy. Clear hysteresis. If we're
                            // in probation, promote to Locked now —
                            // this is the gate that flushes held chars.
                            self.quality_failed_consecutive = 0;
                            self.quality_checks_since_healthy = 0;
                            if in_probation {
                                self.just_locked = false;
                                self.had_pitch_loss = false;
                                self.set_confidence(ConfidenceState::Locked, &mut events);
                            }
                        }
                    }
                }
            }
        }
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

        Ok(self.filter_for_confidence(events))
    }

    /// While the decoder is `Hunting` we drop decoded char-class
    /// events outright. While `Probation` we *hold* them — same
    /// semantics from the operator's point of view (no characters
    /// appear) but they get flushed as soon as the lock is confirmed
    /// so genuine CW that began streaming during the verification
    /// window survives. On a probation drop, the held buffer is
    /// discarded (those chars came from a bogus lock).
    fn filter_for_confidence(&mut self, events: Vec<StreamEvent>) -> Vec<StreamEvent> {
        match self.confidence {
            ConfidenceState::Locked => events,
            ConfidenceState::Hunting => events
                .into_iter()
                .filter(|ev| !is_decoded_char_event(ev))
                .collect(),
            ConfidenceState::Probation => {
                let mut out = Vec::with_capacity(events.len());
                for ev in events {
                    if is_decoded_char_event(&ev) {
                        self.held_events.push(ev);
                    } else {
                        out.push(ev);
                    }
                }
                out
            }
        }
    }

    /// Push a `Confidence` event only if the state actually changed.
    /// Updates the cached state. On promotion to `Locked` the held
    /// probation events get appended to the outgoing list. On a drop
    /// to `Hunting` the held buffer is cleared.
    fn set_confidence(&mut self, next: ConfidenceState, events: &mut Vec<StreamEvent>) {
        if self.confidence != next {
            self.confidence = next;
            events.push(StreamEvent::Confidence { state: next });
            match next {
                ConfidenceState::Locked => {
                    if !self.held_events.is_empty() {
                        events.append(&mut self.held_events);
                    }
                }
                ConfidenceState::Hunting => {
                    self.held_events.clear();
                }
                ConfidenceState::Probation => {}
            }
        }
        // Make sure the lazy-emit flag is set so we don't double-emit
        // the initial Hunting on the first feed call.
        self.confidence_emitted = true;
    }

    /// Tear down all per-lock state so the next pitch search starts
    /// fresh. Called by the quality watchdog when the locked signal
    /// degrades to noise.
    /// (Re)build the optional wide-bin sniff Goertzels around `pitch_hz`.
    /// Spaced one bin apart (40 Hz at the default 25 ms window) so the
    /// (2N+1) bins together cover (2N+1)*40 Hz of bandwidth centered on
    /// the target. Empty if `wide_bin_count == 0`. Bins outside
    /// FREQ_MIN_HZ..=FREQ_MAX_HZ are silently dropped to avoid running
    /// Goertzels in regions our HP/LP filter chain has already killed.
    fn rebuild_wide_bins(&mut self, pitch_hz: f32, win_size: usize, step: usize) {
        self.wide_bins.clear();
        let n = self.config.wide_bin_count as i32;
        if n <= 0 || win_size == 0 {
            return;
        }
        let bin_width_hz = TARGET_RATE as f32 / win_size as f32;
        for k in 1..=n {
            for &sign in &[-1.0_f32, 1.0_f32] {
                let f = pitch_hz + sign * (k as f32) * bin_width_hz;
                if (FREQ_MIN_HZ..=FREQ_MAX_HZ).contains(&f) {
                    self.wide_bins
                        .push(Goertzel::new(f, TARGET_RATE, win_size, step));
                }
            }
        }
    }

    fn drop_pitch_lock(&mut self, preserve_pre_lock_buf: bool) {
        self.pitch_locked = None;
        self.goertzel = None;
        self.wide_bins.clear();
        self.noise_bins.clear();
        self.smooth_buf.clear();
        self.smooth_sum = 0.0;
        self.noise_smooth_buf.clear();
        self.noise_smooth_sum = 0.0;
        self.power_emit_accum = 0.0;
        self.samples_since_pitch_eval = 0;
        self.samples_since_quality_check = 0;
        self.quality_failed_consecutive = 0;
        self.quality_checks_since_healthy = 0;
        self.quality_buf.clear();
        self.just_locked = false;
        self.had_pitch_loss = true;
        if !preserve_pre_lock_buf {
            self.pre_lock_buf.clear();
        }
        self.unlock_power_history.clear();
        let power_rate = TARGET_RATE as f32
            / ((TARGET_RATE as f32 * GOERTZEL_WIN_MS / 1000.0) as usize / 4).max(1) as f32;
        self.decoder = Decoder::new(power_rate);
        self.decoder.threshold_scale = self.config.threshold_scale;
        self.decoder.auto_threshold = self.config.auto_threshold;
        self.decoder.min_pulse_dot_fraction = self.config.min_pulse_dot_fraction.max(0.0);
        self.decoder.min_gap_dot_fraction = self.config.min_gap_dot_fraction.max(0.0);
        self.decoder.hysteresis_fraction = self.config.hysteresis_fraction.max(0.0);
    }

    fn try_acquire_pitch_lock(&mut self, events: &mut Vec<StreamEvent>) {
        let need = self.pitch_lock_samples_needed();
        if self.pre_lock_buf.len() < need {
            return;
        }

        if let Ok(pitch) = detect_pitch(
            &self.pre_lock_buf,
            TARGET_RATE,
            self.config.pitch_min_snr_linear(),
            self.config.pitch_lock_bounds(),
        ) {
            self.pitch_locked = Some(pitch);
            let win_size = (TARGET_RATE as f32 * GOERTZEL_WIN_MS / 1000.0) as usize;
            let step = (win_size / 4).max(1);
            self.goertzel = Some(Goertzel::new(pitch, TARGET_RATE, win_size, step));
            self.rebuild_wide_bins(pitch, win_size, step);
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
            // Adjacent purity bins are no longer needed — the same off-band
            // noise bins above are reused at instantaneous time inside
            // `feed_goertzel` for the tone-purity gate.
            self.unlock_power_history.clear();
            self.just_locked = true;
            events.push(StreamEvent::PitchUpdate { pitch_hz: pitch });
            // Surface the lock immediately as Probation so the UI
            // shows "VERIFYING SIGNAL". The probation watchdog will
            // promote to Locked (or drop back to Hunting) within a
            // few seconds.
            self.set_confidence(ConfidenceState::Probation, events);
            let drained = std::mem::take(&mut self.pre_lock_buf);
            self.feed_goertzel(&drained, events);
        } else {
            let keep = need.max(4096);
            if self.pre_lock_buf.len() > keep {
                let drop = self.pre_lock_buf.len() - keep;
                self.pre_lock_buf.drain(..drop);
            }
        }
    }

    fn pitch_lock_samples_needed(&self) -> usize {
        let seconds = if self.had_pitch_loss {
            // Recent watchdog-driven drop: prefer to re-acquire fast
            // because the typical case is "voice/QRM ended, real CW
            // started right after" and we don't want to lose more
            // characters waiting for a full PITCH_LOCK_SECONDS window.
            RELOCK_SECONDS
        } else if self.config.pitch_lock_bounds().is_some() {
            RANGE_LOCK_SECONDS
        } else {
            PITCH_LOCK_SECONDS
        };
        ((TARGET_RATE as f32 * seconds).round() as usize).max(4096)
    }

    fn seed_fast_relock_from_recent_audio(&mut self, recent_audio: &[f32]) -> bool {
        if self.config.pitch_lock_bounds().is_none() || recent_audio.is_empty() {
            return false;
        }

        let keep = self.pitch_lock_samples_needed().max(4096);
        let start = recent_audio.len().saturating_sub(keep);
        self.pre_lock_buf.clear();
        self.pre_lock_buf.extend_from_slice(&recent_audio[start..]);
        true
    }

    fn emit_unlock_power(&mut self, audio: &[f32], events: &mut Vec<StreamEvent>) {
        if audio.is_empty() {
            return;
        }

        let power = audio.iter().map(|sample| sample * sample).sum::<f32>() / audio.len() as f32;
        self.unlock_power_history.push_back(power);
        if self.unlock_power_history.len() > UNLOCK_POWER_HISTORY_EVENTS {
            self.unlock_power_history.pop_front();
        }

        let mut sorted: Vec<f32> = self.unlock_power_history.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let noise = sorted[sorted.len() / 10].max(1e-10);
        let threshold = noise * 4.0;
        let snr = power / noise;
        events.push(StreamEvent::Power {
            power,
            threshold,
            noise,
            snr: if snr.is_finite() { snr } else { 0.0 },
            signal: power > threshold,
        });
    }

    fn feed_goertzel(&mut self, audio: &[f32], events: &mut Vec<StreamEvent>) {
        let Some(goertzel) = self.goertzel.as_mut() else {
            return;
        };
        let mut power_out = Vec::new();
        goertzel.push(audio, &mut power_out);

        // Wide-bin sniff: if the operator enabled it, also run companion
        // Goertzels at pitch ± k*bin_width and ADD their power into
        // `power_out` element-wise. This widens the effective integration
        // bandwidth so we capture a frequency-smeared signal (acoustic
        // re-capture, drift, wide-bandpass receivers) instead of just one
        // ~40 Hz slice. All bins use identical win_size/step so they emit
        // the same number of samples per call as the main Goertzel.
        if !self.wide_bins.is_empty() {
            let mut wide_outs: Vec<Vec<f32>> =
                (0..self.wide_bins.len()).map(|_| Vec::new()).collect();
            for (idx, wb) in self.wide_bins.iter_mut().enumerate() {
                wb.push(audio, &mut wide_outs[idx]);
            }
            for (i, p) in power_out.iter_mut().enumerate() {
                let extra: f32 = wide_outs.iter().filter_map(|v| v.get(i).copied()).sum();
                *p += extra;
            }
        }

        // Run all noise-bin Goertzels in lockstep (identical win_size/step,
        // so each emits exactly the same number of samples as `power_out`).
        // These same outputs feed both the smoothed noise floor (for SNR)
        // *and* the instantaneous tone-purity gate.
        let mut noise_outs: Vec<Vec<f32>> =
            (0..self.noise_bins.len()).map(|_| Vec::new()).collect();
        for (idx, nb) in self.noise_bins.iter_mut().enumerate() {
            nb.push(audio, &mut noise_outs[idx]);
        }
        let snr_threshold = self.config.min_snr_linear();
        let purity_threshold = self.config.min_tone_purity;

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

            // Noise reference: take the 25th percentile of the side-bin
            // readings at this instant. Lower percentile is more robust
            // against a single bin landing on adjacent QRM (which would
            // inflate the noise floor and disable the gate). At Q25 of
            // 4-8 side bins we tolerate up to ~2 contaminated bins.
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
                    buf[buf.len() / 4]
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

            // SNR: how many times louder is the smoothed tone bin vs the
            // smoothed off-band reference. With no noise reading yet we
            // default to "OK" so the existing amplitude threshold still
            // does its job.
            let snr = if noise > 0.0 {
                smoothed / noise
            } else {
                f32::INFINITY
            };
            let snr_ok = snr >= snr_threshold;

            // Tone purity: instantaneous target-bin power vs instantaneous
            // off-band noise (q25 of noise bins at *this* sample). The SNR
            // check above smooths both numerator and denominator, so a 5 ms
            // broadband impulse beats it (the noise smoother is ~200 ms).
            // Comparing this-sample target against this-sample noise
            // exposes the impulse: the impulse spikes ALL bins together,
            // so target/noise stays near 1; a real CW tone is concentrated
            // at the locked frequency, so target/noise jumps to >>1.
            //
            // When the gate is disabled (`min_tone_purity <= 0`) or no
            // noise bins are configured, treat purity as always-OK so the
            // decoder reverts to the pre-gate behavior.
            let (tone_purity, purity_ok) = if purity_threshold <= 0.0 || noise_outs.is_empty() {
                (0.0_f32, true)
            } else if noise_raw <= 1e-12 {
                // No measurable off-band energy at this instant → trivially
                // pure (the only thing in the band is at the locked freq).
                (f32::INFINITY, true)
            } else {
                let ratio = p / noise_raw;
                (ratio, ratio >= purity_threshold)
            };

            // Throttled Power event for UI meters (~POWER_EVENT_HZ).
            self.power_emit_accum += 1.0;
            if self.power_emit_accum >= self.power_emit_step {
                self.power_emit_accum -= self.power_emit_step;
                let threshold = self.decoder.threshold;
                let signal = threshold > 0.0 && smoothed > threshold && snr_ok && purity_ok;
                let snr_clean = if snr.is_finite() { snr } else { 0.0 };
                events.push(StreamEvent::Power {
                    power: smoothed,
                    threshold,
                    noise,
                    snr: snr_clean,
                    signal,
                });
            }

            let purity_for_decoder = if tone_purity.is_finite() {
                tone_purity
            } else {
                // Use a large finite number so the decoder can still record a
                // peak without polluting later max() comparisons with NaN.
                1.0e6
            };
            // CFAR keying: opt-in scale-invariant target/noise ratio
            // metric for the threshold detector, plus bypass of the
            // static 3 dB `snr_ok` gate. This was empirically the only
            // per-frame variant tried in #322 that recovered ANY
            // stable copy on `harsh_white` (band-passed white noise
            // mixed with weak CW), but it costs baseline performance
            // because the ratio is scale-invariant and clean signals
            // (where target >> noise by 30+ dB) lose their absolute
            // headroom in the rolling-quantile threshold. Use only
            // for harsh same-band noise scenarios; defaults to off.
            //
            // The fundamental limit on per-frame methods: under
            // band-passed noise the target/side-bin SNR median is only
            // ~1 dB and per-frame variance overlaps key-up and
            // key-down. Coherent recovery requires soft element-window
            // integration (matched dit/dah scoring against the
            // post-acquisition dot estimate), tracked in #322.
            let (key_input, snr_ok_for_decoder) = if self.config.cfar_keying {
                if noise > 1e-12 {
                    (smoothed / noise, true)
                } else {
                    // Pre-warmup: noise smoother hasn't filled yet.
                    (smoothed, snr_ok)
                }
            } else {
                (smoothed, snr_ok)
            };
            self.decoder.push_power(
                key_input,
                snr_ok_for_decoder,
                purity_ok,
                purity_for_decoder,
                self.pitch_locked,
                events,
            );
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
        if let Some(ev) = self.decoder.classify_letter(self.pitch_locked) {
            events.push(ev);
        }
        // If the stream ends while still in probation, emit whatever
        // we've accumulated. The lock survived to end-of-input, which
        // is the best evidence we have that it was real.
        if matches!(self.confidence, ConfidenceState::Probation) && !self.held_events.is_empty() {
            events.append(&mut self.held_events);
        }
        events
    }
}

/// Whether a `StreamEvent` carries decoded character output that
/// should be gated by the confidence state machine. Pitch / power /
/// confidence / lifecycle events are diagnostic and always pass
/// through.
fn is_decoded_char_event(ev: &StreamEvent) -> bool {
    matches!(
        ev,
        StreamEvent::Char { .. }
            | StreamEvent::Garbled { .. }
            | StreamEvent::Word
            | StreamEvent::WpmUpdate { .. }
    )
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
        // ITU punctuation (commonly heard in QSOs).
        ".-.-.-" => Some('.'),
        "--..--" => Some(','),
        "..--.." => Some('?'),
        ".----." => Some('\''),
        "-.-.--" => Some('!'),
        "-..-." => Some('/'),
        "-.--." => Some('('),
        "-.--.-" => Some(')'),
        ".-..." => Some('&'),
        "---..." => Some(':'),
        "-.-.-." => Some(';'),
        "-...-" => Some('='), // also BT prosign
        ".-.-." => Some('+'), // also AR prosign
        "-....-" => Some('-'),
        "..--.-" => Some('_'),
        ".-..-." => Some('"'),
        "...-..-" => Some('$'),
        ".--.-." => Some('@'),
        _ => None,
    }
}

#[cfg(test)]
mod trial_decode_tests {
    use super::*;
    use std::f32::consts::TAU;

    pub(super) fn synth_paris(sample_rate: u32, pitch_hz: f32, wpm: f32, secs: f32) -> Vec<f32> {
        // PARIS = .--.  .-  .-.  ..  ...   plus inter-word gap = 50 dot units total.
        let dot_secs = 1.2 / wpm;
        let dot_n = (dot_secs * sample_rate as f32) as usize;
        // Cosine raised-edge ramp ~5ms to suppress key clicks.
        let ramp_n = ((sample_rate as f32) * 0.005) as usize;
        // pattern: list of (on, units)
        let pattern: Vec<(bool, usize)> = {
            let mut p: Vec<(bool, usize)> = Vec::new();
            let letters = ["P", "A", "R", "I", "S"];
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
                if li + 1 < letters.len() {
                    p.push((false, 3));
                }
            }
            p.push((false, 7)); // inter-word
            p
        };
        let mut out: Vec<f32> = Vec::new();
        let mut t = 0usize;
        let total_n = (secs * sample_rate as f32) as usize;
        while out.len() < total_n {
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
                        break;
                    }
                }
                if out.len() >= total_n {
                    break;
                }
            }
        }
        out
    }

    #[test]
    fn trial_decode_score_is_high_for_clean_cw() {
        let sr = 16000u32;
        let audio = synth_paris(sr, 700.0, 20.0, 8.0);
        let on_pitch = trial_decode_score(&audio, sr, 700.0);
        // White noise / silence baseline.
        let silence = vec![0.0_f32; audio.len()];
        let off_pitch = trial_decode_score(&silence, sr, 700.0);
        assert!(
            on_pitch > 5.0,
            "on-pitch score should be substantial, got {on_pitch}"
        );
        assert!(
            off_pitch < 0.5,
            "silence score should be ~0, got {off_pitch}"
        );
    }

    #[test]
    fn trial_decode_score_is_higher_at_signal_pitch_than_off_pitch() {
        let sr = 16000u32;
        // Mix CW at 700 Hz with broadband white noise at low amplitude.
        let cw = synth_paris(sr, 700.0, 20.0, 8.0);
        // Tiny LCG so the test is deterministic without depending on a crate.
        let mut s: u32 = 0xDEAD_BEEF;
        let mixed: Vec<f32> = cw
            .iter()
            .map(|&x| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                let n = ((s >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.05;
                x + n
            })
            .collect();
        let on_pitch = trial_decode_score(&mixed, sr, 700.0);
        // Compare against a pitch with no real signal (pure white noise).
        let mut s2: u32 = 0xCAFE_F00D;
        let noise: Vec<f32> = (0..mixed.len())
            .map(|_| {
                s2 = s2.wrapping_mul(1664525).wrapping_add(1013904223);
                ((s2 >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.05
            })
            .collect();
        let pure_noise = trial_decode_score(&noise, sr, 700.0);
        assert!(
            on_pitch > pure_noise * 2.0,
            "on-pitch CW ({on_pitch}) should beat pure-noise ({pure_noise}) by a wide margin"
        );
    }
}

#[cfg(test)]
mod measure_fisher {
    use super::trial_decode_tests::synth_paris;
    use super::*;
    #[test]
    fn measure_fisher_noise_vs_cw() {
        let sr = 16000u32;
        let mut s: u32 = 0xDEAD_BEEF;
        let mut rng = || {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            ((s >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 2.0
        };
        // 1) Pure white noise (no signal)
        let noise: Vec<f32> = (0..sr as usize * 6).map(|_| rng() * 0.1).collect();
        // Probe many candidate pitches
        let mut max_noise_fisher: f32 = 0.0;
        for f in (350..1500).step_by(15) {
            let s = trial_decode_score(&noise, sr, f as f32);
            if s > max_noise_fisher {
                max_noise_fisher = s;
            }
        }
        // 2) Faint CW at 700 Hz
        let cw_clean = synth_paris(sr, 700.0, 18.0, 6.0);
        for &snr_db in &[20.0_f32, 10.0, 6.0, 3.0, 0.0, -3.0, -6.0] {
            let cw_amp = 10f32.powf(snr_db / 20.0) * 0.1;
            let mixed: Vec<f32> = cw_clean.iter().map(|&x| x * cw_amp + rng() * 0.1).collect();
            let on = trial_decode_score(&mixed, sr, 700.0);
            let off = trial_decode_score(&mixed, sr, 1200.0);
            eprintln!("SNR={snr_db:>5.1}dB  Fisher@700={on:>8.2}  Fisher@1200={off:>8.2}");
        }
        eprintln!("PURE NOISE max-Fisher across all candidate pitches = {max_noise_fisher:.2}");
    }
}

#[cfg(test)]
mod lock_behavior_tests {
    use super::trial_decode_tests::synth_paris;
    use super::*;

    fn lcg_noise(n: usize, amp: f32, seed: u32) -> Vec<f32> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                ((s >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 2.0 * amp
            })
            .collect()
    }

    fn run_decoder(audio: &[f32], sample_rate: u32) -> (bool, bool, usize) {
        // Returns (locked_at_least_once, lost_after_lock, char_count).
        let mut dec = StreamingDecoder::new(sample_rate).expect("decoder");
        let chunk = (sample_rate / 10) as usize; // 100 ms
        let mut locked = false;
        let mut lost = false;
        let mut chars = 0usize;
        for c in audio.chunks(chunk) {
            let events = dec.feed(c).expect("feed");
            for ev in events {
                match ev {
                    StreamEvent::PitchUpdate { .. } => locked = true,
                    StreamEvent::PitchLost { .. } => {
                        if locked {
                            lost = true;
                        }
                    }
                    StreamEvent::Char { .. } => chars += 1,
                    _ => {}
                }
            }
        }
        for ev in dec.flush() {
            if let StreamEvent::Char { .. } = ev {
                chars += 1;
            }
        }
        (locked, lost, chars)
    }

    #[test]
    fn pure_noise_does_not_lock() {
        let sr = 16000u32;
        let audio = lcg_noise(sr as usize * 20, 0.1, 0xDEAD_BEEF);
        let (locked, _lost, chars) = run_decoder(&audio, sr);
        assert!(!locked, "decoder should not lock on pure white noise");
        assert_eq!(chars, 0, "decoder should not emit characters on pure noise");
    }

    #[test]
    fn clean_cw_locks_and_decodes() {
        let sr = 16000u32;
        let audio = synth_paris(sr, 700.0, 20.0, 12.0);
        let (locked, _lost, chars) = run_decoder(&audio, sr);
        assert!(locked, "decoder should lock on clean CW");
        assert!(
            chars >= 5,
            "expected several decoded chars from PARIS, got {chars}"
        );
    }

    #[test]
    fn emits_power_before_pitch_lock() {
        let sr = 16000u32;
        let audio = synth_paris(sr, 700.0, 20.0, 1.0);
        let mut dec = StreamingDecoder::new(sr).expect("decoder");
        let mut saw_power = false;
        let mut saw_pitch = false;
        for chunk in audio.chunks((sr / 10) as usize) {
            for ev in dec.feed(chunk).expect("feed") {
                match ev {
                    StreamEvent::Power { .. } => saw_power = true,
                    StreamEvent::PitchUpdate { .. } => saw_pitch = true,
                    _ => {}
                }
            }
        }
        assert!(
            saw_power,
            "decoder should emit live power events before pitch lock"
        );
        assert!(
            !saw_pitch,
            "1 second of audio should be below the pitch-lock buffer"
        );
    }

    #[test]
    fn signal_loss_drops_lock() {
        let sr = 16000u32;
        // 12s of CW followed by 20s of pure noise — watchdog should drop within ~8s.
        let mut audio = synth_paris(sr, 700.0, 20.0, 12.0);
        audio.extend(lcg_noise(sr as usize * 20, 0.05, 0xCAFE_F00D));
        let (locked, lost, _chars) = run_decoder(&audio, sr);
        assert!(locked, "decoder should lock during the CW segment");
        assert!(
            lost,
            "decoder should drop lock after sustained noise-only input"
        );
    }

    #[test]
    fn emits_power_after_pitch_loss_while_relocking() {
        let sr = 16000u32;
        let mut audio = synth_paris(sr, 700.0, 20.0, 12.0);
        audio.extend(lcg_noise(sr as usize * 20, 0.05, 0x1234_5678));
        let mut dec = StreamingDecoder::new(sr).expect("decoder");
        let chunk = (sr / 10) as usize;
        let mut lost = false;
        let mut power_after_loss = false;
        for c in audio.chunks(chunk) {
            let events = dec.feed(c).expect("feed");
            for ev in events {
                match ev {
                    StreamEvent::PitchLost { .. } => lost = true,
                    StreamEvent::Power { .. } if lost => power_after_loss = true,
                    _ => {}
                }
            }
            if lost && power_after_loss {
                break;
            }
        }
        assert!(
            lost,
            "decoder should eventually lose lock on sustained noise"
        );
        assert!(
            power_after_loss,
            "decoder should keep emitting power events while hunting for a new lock"
        );
    }

    #[test]
    fn steady_lock_holds_through_silence_and_degenerate_window() {
        // Regression: 30 WPM clean clips were dropping lock when the
        // 8 s quality window happened to span (a) long key-up gaps
        // between transmissions or (b) a stretch dominated by either
        // dits or dahs, so trial_decode_score's bimodal cluster
        // analysis returned ~0. Once a lock has been promoted out of
        // probation, it must take QUALITY_DROP_CONSECUTIVE consecutive
        // sub-threshold checks to drop — a single bad window is not
        // enough.
        let sr = TARGET_RATE;
        // 12 s of clean PARIS to acquire and survive probation, then
        // 6 s of pure silence (worst case for trial_decode_score: zero
        // power, zero on-runs), then 12 s more of clean PARIS.
        let mut audio = synth_paris(sr, 700.0, 20.0, 12.0);
        audio.extend(vec![0.0_f32; (sr as f32 * 6.0) as usize]);
        audio.extend(synth_paris(sr, 700.0, 20.0, 12.0));

        let mut dec = StreamingDecoder::new(sr).expect("decoder");
        let chunk = (sr / 10) as usize;
        let mut became_locked = false;
        let mut lost_after_lock = false;
        for c in audio.chunks(chunk) {
            let events = dec.feed(c).expect("feed");
            for ev in events {
                match ev {
                    StreamEvent::Confidence {
                        state: ConfidenceState::Locked,
                    } => {
                        became_locked = true;
                    }
                    StreamEvent::PitchLost { .. } if became_locked => {
                        lost_after_lock = true;
                    }
                    _ => {}
                }
            }
        }
        assert!(
            became_locked,
            "decoder should reach a steady lock on clean PARIS"
        );
        assert!(
            !lost_after_lock,
            "steady lock must hold through key-up silence and degenerate quality windows"
        );
    }

    #[test]
    fn preserves_seeded_relock_buffer_when_range_lock_drops_pitch() {
        let sr = 16000u32;
        let recent_audio = synth_paris(sr, 700.0, 20.0, 2.0);
        let mut dec = StreamingDecoder::new(sr).expect("decoder");
        let cfg = DecoderConfig {
            experimental_range_lock: true,
            ..Default::default()
        };
        dec.set_config(cfg);

        let preserved = dec.seed_fast_relock_from_recent_audio(&recent_audio);
        let before = dec.pre_lock_buf.len();
        assert!(
            preserved,
            "range-lock mode should preserve a seeded relock buffer"
        );
        assert!(
            before > 0,
            "seeded relock buffer should contain recent audio"
        );

        dec.drop_pitch_lock(preserved);

        assert_eq!(
            dec.pre_lock_buf.len(),
            before,
            "drop_pitch_lock should keep the seeded relock buffer intact"
        );
    }

    #[test]
    fn tone_purity_gate_suppresses_broadband_impulses() {
        // Synthesize an audio stream consisting of a steady CW-like tone at
        // 800 Hz with isolated broadband impulses (deterministic
        // pseudo-noise bursts that briefly dump energy across all bins,
        // the same shape as a finger snap or key click). With the purity
        // gate enabled the impulses must not produce Char events.

        // Tiny LCG so we don't pull in `rand` for one test.
        let mut state: u64 = 0xDEAD_BEEF_C0FF_EE42;
        let mut next_noise = || -> f32 {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let u = ((state >> 33) as u32) as f32 / u32::MAX as f32;
            (u - 0.5) * 2.0
        };

        let sr = TARGET_RATE;
        let dur_sec = 2.0_f32;
        let total = (sr as f32 * dur_sec) as usize;
        let mut audio = vec![0.0_f32; total];
        for (i, sample) in audio.iter_mut().enumerate() {
            let t = i as f32 / sr as f32;
            *sample = (2.0 * std::f32::consts::PI * 800.0 * t).sin() * 0.05;
        }
        let impulse_centers_ms = [200, 600, 1000, 1400, 1800];
        let impulse_half_ms = 3.0_f32;
        let half_n = (sr as f32 * impulse_half_ms / 1000.0) as usize;
        for c_ms in impulse_centers_ms {
            let center = (sr as f32 * c_ms as f32 / 1000.0) as usize;
            let lo = center.saturating_sub(half_n);
            let hi = (center + half_n).min(audio.len());
            for sample in &mut audio[lo..hi] {
                *sample += next_noise() * 0.8;
            }
        }

        // Gate enabled: any emitted Char is impulse-driven (the steady tone
        // has no dits/dahs).
        let mut dec = StreamingDecoder::new(sr).expect("decoder");
        let mut cfg = DecoderConfig::defaults();
        cfg.experimental_range_lock = true;
        cfg.range_lock_min_hz = 700.0;
        cfg.range_lock_max_hz = 900.0;
        cfg.min_tone_purity = DEFAULT_MIN_TONE_PURITY;
        dec.set_config(cfg);

        let chunk = sr as usize / 10; // 100 ms chunks
        let mut all_events_gated = Vec::new();
        for win in audio.chunks(chunk) {
            let evs = dec.feed(win).expect("feed");
            all_events_gated.extend(evs);
        }
        all_events_gated.extend(dec.flush());
        let chars_gated = all_events_gated
            .iter()
            .filter(|ev| matches!(ev, StreamEvent::Char { .. }))
            .count();

        // Gate disabled: confirm impulses do trip the decoder. Self-validating.
        let mut dec_off = StreamingDecoder::new(sr).expect("decoder");
        let mut cfg_off = DecoderConfig::defaults();
        cfg_off.experimental_range_lock = true;
        cfg_off.range_lock_min_hz = 700.0;
        cfg_off.range_lock_max_hz = 900.0;
        cfg_off.min_tone_purity = 0.0;
        dec_off.set_config(cfg_off);

        let mut all_events_ungated = Vec::new();
        for win in audio.chunks(chunk) {
            let evs = dec_off.feed(win).expect("feed");
            all_events_ungated.extend(evs);
        }
        all_events_ungated.extend(dec_off.flush());
        let chars_ungated = all_events_ungated
            .iter()
            .filter(|ev| matches!(ev, StreamEvent::Char { .. }))
            .count();

        assert!(
            chars_ungated >= chars_gated,
            "ungated decoder should produce at least as many chars as gated one \
             (ungated={chars_ungated}, gated={chars_gated})"
        );
        assert!(
            chars_gated <= 1,
            "tone-purity gate should suppress impulse-driven chars (gated={chars_gated}, \
             ungated={chars_ungated})"
        );
    }
}

#[cfg(test)]
mod wide_bin_sniff_tests {
    use super::*;
    use std::f32::consts::PI;

    /// Synthesize CW that drifts in pitch by a small amount across the
    /// recording — a stand-in for an acoustically re-captured signal
    /// whose energy is smeared across several Goertzel bins.
    fn synth_drifting_cw(
        sr: u32,
        center_hz: f32,
        drift_hz: f32,
        wpm: f32,
        seconds: f32,
    ) -> Vec<f32> {
        let dot_s = 1.2 / wpm;
        let mut audio = vec![0.0f32; (sr as f32 * seconds) as usize];
        // Simple PARIS-style key sequence: ".- -... -.-." (ABC) repeated.
        let pattern: &[(f32, bool)] = &[
            (1.0, true),
            (1.0, false),
            (3.0, true),
            (1.0, false), // A
            (3.0, false),
            (3.0, true),
            (1.0, false),
            (1.0, true),
            (1.0, false),
            (1.0, true),
            (1.0, false),
            (1.0, true), // B
            (3.0, false),
            (3.0, true),
            (1.0, false),
            (1.0, true),
            (1.0, false),
            (3.0, true),
            (1.0, false),
            (1.0, true), // C
            (7.0, false),
        ];
        let mut t_samples: usize = 0;
        let mut phase: f32 = 0.0;
        let mut cycle = pattern.iter().cycle();
        while t_samples < audio.len() {
            let (units, on) = *cycle.next().unwrap();
            let dur = (units * dot_s * sr as f32) as usize;
            for i in 0..dur {
                if t_samples + i >= audio.len() {
                    break;
                }
                if on {
                    let frac = (t_samples + i) as f32 / audio.len() as f32;
                    let f = center_hz + drift_hz * (frac - 0.5);
                    phase += 2.0 * PI * f / sr as f32;
                    audio[t_samples + i] = 0.4 * phase.sin();
                } else {
                    phase = 0.0;
                }
            }
            t_samples += dur;
        }
        audio
    }

    fn count_chars(audio: &[f32], sr: u32, wide_bin_count: u8) -> usize {
        let mut dec = StreamingDecoder::new(sr).expect("decoder");
        let mut cfg = DecoderConfig::defaults();
        cfg.wide_bin_count = wide_bin_count;
        cfg.min_tone_purity = 0.0; // isolate the wide-bin effect
        dec.set_config(cfg);
        let mut chars = 0usize;
        for c in audio.chunks((sr / 10) as usize) {
            for ev in dec.feed(c).unwrap_or_default() {
                if matches!(ev, StreamEvent::Char { .. }) {
                    chars += 1;
                }
            }
        }
        for ev in dec.flush() {
            if matches!(ev, StreamEvent::Char { .. }) {
                chars += 1;
            }
        }
        chars
    }

    #[test]
    fn wide_bins_recover_drifting_signal() {
        // Drift 80 Hz across the recording — 2 Goertzel bins worth at the
        // default 25 ms / 12 kHz window — so a single-bin Goertzel will
        // see the signal fall out of band repeatedly.
        let sr = 12000u32;
        let audio = synth_drifting_cw(sr, 700.0, 80.0, 18.0, 8.0);
        let narrow = count_chars(&audio, sr, 0);
        let wide = count_chars(&audio, sr, 2);
        assert!(
            wide >= narrow,
            "wide-bin sniff should not regress narrow case (narrow={narrow}, wide={wide})"
        );
        // The wide path should at least decode several characters even
        // when the narrow path struggles. We do not assert exact counts
        // because the synthesis is deliberately rough.
        assert!(
            wide >= 3,
            "wide-bin sniff should recover at least a few drifting chars (got {wide})"
        );
    }
}

#[cfg(test)]
mod min_pulse_filter_tests {
    use super::trial_decode_tests::synth_paris;
    use super::*;

    fn lcg(seed: u32, n: usize, amp: f32) -> Vec<f32> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                ((s >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 2.0 * amp
            })
            .collect()
    }

    fn count_chars(audio: &[f32], sr: u32, min_pulse_dot_fraction: f32) -> usize {
        let mut dec = StreamingDecoder::new(sr).expect("decoder");
        let mut cfg = DecoderConfig::defaults();
        cfg.min_tone_purity = 0.0;
        cfg.min_pulse_dot_fraction = min_pulse_dot_fraction;
        dec.set_config(cfg);
        let mut chars = 0usize;
        for c in audio.chunks((sr / 10) as usize) {
            for ev in dec.feed(c).unwrap_or_default() {
                if matches!(ev, StreamEvent::Char { .. }) {
                    chars += 1;
                }
            }
        }
        for ev in dec.flush() {
            if matches!(ev, StreamEvent::Char { .. }) {
                chars += 1;
            }
        }
        chars
    }

    #[test]
    fn min_pulse_filter_does_not_regress_clean_cw() {
        // Real CW dits should sit at len_norm ~= 1.0; a 0.3 cutoff
        // must not drop them.
        let sr = 12000u32;
        let audio = synth_paris(sr, 700.0, 18.0, 6.0);
        let baseline = count_chars(&audio, sr, 0.0);
        let gated = count_chars(&audio, sr, 0.3);
        assert!(
            baseline >= 5,
            "baseline should decode PARIS (got {baseline})"
        );
        assert!(
            gated as i32 >= baseline as i32 - 1,
            "min-pulse gate must not drop more than one clean character \
             (baseline={baseline}, gated={gated})"
        );
    }

    #[test]
    fn min_gap_filter_does_not_regress_clean_cw() {
        // The inter-element gaps inside PARIS are ~1 dot (intra-letter)
        // and ~3 dot (inter-letter). A 0.3 cutoff sits below both, so
        // it must not collapse adjacent dits into one big on-run.
        let sr = 12000u32;
        let audio = synth_paris(sr, 700.0, 18.0, 6.0);
        let baseline = count_chars_with_gap(&audio, sr, 0.0, 0.0);
        let gated = count_chars_with_gap(&audio, sr, 0.0, 0.3);
        assert!(
            baseline >= 5,
            "baseline should decode PARIS (got {baseline})"
        );
        assert!(
            gated as i32 >= baseline as i32 - 1,
            "min-gap gate must not collapse clean PARIS \
             (baseline={baseline}, gated={gated})"
        );
    }

    fn count_chars_with_gap(
        audio: &[f32],
        sr: u32,
        min_pulse_dot_fraction: f32,
        min_gap_dot_fraction: f32,
    ) -> usize {
        let mut dec = StreamingDecoder::new(sr).expect("decoder");
        let cfg = DecoderConfig {
            min_tone_purity: 0.0,
            min_pulse_dot_fraction,
            min_gap_dot_fraction,
            ..DecoderConfig::defaults()
        };
        dec.set_config(cfg);
        let mut chars = 0usize;
        for c in audio.chunks((sr / 10) as usize) {
            for ev in dec.feed(c).unwrap_or_default() {
                if matches!(ev, StreamEvent::Char { .. }) {
                    chars += 1;
                }
            }
        }
        for ev in dec.flush() {
            if matches!(ev, StreamEvent::Char { .. }) {
                chars += 1;
            }
        }
        chars
    }

    #[test]
    fn min_pulse_filter_suppresses_short_blips_in_silence() {
        // Construct: pure silence with a few sub-dot-length tone blips
        // sprinkled in. Without the gate the streaming decoder would
        // happily turn each one into an "E".
        let sr = 12000u32;
        let dot_s = 1.2 / 18.0_f32;
        // 60% of dot length — well above the threshold/Goertzel detect
        // floor, but should be filtered at fraction=0.85.
        let blip_n = (sr as f32 * dot_s * 0.6) as usize;
        let gap_n = (sr as f32 * dot_s * 8.0) as usize;
        let mut audio: Vec<f32> = Vec::new();
        // Lead-in noise so pitch lock can warm up on a real-ish backdrop.
        audio.extend(lcg(0xCAFE, sr as usize * 2, 0.005));
        // Add a stretch of clean CW first so the decoder has a real
        // dot-length estimate to compare blips against. Length is
        // sized so that the probation watchdog has clean CW in its
        // ~2s rolling buffer when it fires (lock acquires after
        // ~6 s of pre-buffer; probation runs ~2 s after that).
        audio.extend(synth_paris(sr, 700.0, 18.0, 8.0));
        // Then silence punctuated by short blips.
        for k in 0..6 {
            audio.extend(lcg(0xBEEF + k, gap_n, 0.005));
            let phase_step = std::f32::consts::TAU * 700.0 / sr as f32;
            let mut phase = 0.0f32;
            for _ in 0..blip_n {
                phase += phase_step;
                audio.push(0.4 * phase.sin());
            }
        }
        let chars_ungated = count_chars(&audio, sr, 0.0);
        let chars_gated = count_chars(&audio, sr, 0.85);
        assert!(
            chars_gated <= chars_ungated,
            "min-pulse gate must not introduce new chars \
             (ungated={chars_ungated}, gated={chars_gated})"
        );
        // At fraction=0.5, the 30% blips must be suppressed entirely
        // relative to the ungated case.
        assert!(
            chars_ungated > chars_gated,
            "expected at least one short blip to be filtered out \
             (ungated={chars_ungated}, gated={chars_gated})"
        );
    }
}

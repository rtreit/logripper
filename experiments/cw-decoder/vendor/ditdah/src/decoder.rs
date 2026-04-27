use anyhow::{Result, bail};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use rustfft::{FftPlanner, num_complex::Complex};
use std::collections::VecDeque;
use std::io::Write;
// --- DSP Constants ---
const FREQ_MIN_HZ: f32 = 200.0;
const FREQ_MAX_HZ: f32 = 1200.0;
const RESAMPLER_CHUNK_SIZE: usize = 1024;

// --- Decoding Constants ---
const DIT_DAH_BOUNDARY: f32 = 2.0;
const LETTER_SPACE_BOUNDARY: f32 = 2.0; // Gaps > 2x dot length end the current letter
const WORD_SPACE_BOUNDARY: f32 = 5.0; // Gaps > 5x dot length add word space
const MORSE_BEAM_WIDTH: usize = 96;

// --- BiquadFilter (Unchanged) ---
#[derive(Debug, Clone, Copy)]
pub enum FilterType {
    HighPass,
    LowPass,
}
pub struct BiquadFilter {
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
impl BiquadFilter {
    pub fn new(filter_type: FilterType, cutoff_hz: f32, sample_rate: u32) -> Self {
        let mut filter = Self {
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
        let sqrt2 = 2.0f32.sqrt();
        match filter_type {
            FilterType::LowPass => {
                let d = 1.0 / (1.0 + sqrt2 * c + c * c);
                filter.a0 = c * c * d;
                filter.a1 = 2.0 * filter.a0;
                filter.a2 = filter.a0;
                filter.b1 = 2.0 * (c * c - 1.0) * d;
                filter.b2 = (1.0 - sqrt2 * c + c * c) * d;
            }
            FilterType::HighPass => {
                let d = 1.0 / (1.0 + sqrt2 * c + c * c);
                filter.a0 = d;
                filter.a1 = -2.0 * d;
                filter.a2 = d;
                filter.b1 = 2.0 * (c * c - 1.0) * d;
                filter.b2 = (1.0 - sqrt2 * c + c * c) * d;
            }
        }
        filter
    }
    pub fn process(&mut self, input: &mut [f32]) {
        for sample in input.iter_mut() {
            let x0 = *sample;
            let y0 = self.a0 * x0 + self.a1 * self.x1 + self.a2 * self.x2
                - self.b1 * self.y1
                - self.b2 * self.y2;
            self.x2 = self.x1;
            self.x1 = x0;
            self.y2 = self.y1;
            self.y1 = y0;
            *sample = y0;
        }
    }
}

// --- Goertzel Filter (Unchanged) ---
struct Goertzel {
    coeff: f32,
    window: Vec<f32>,
}
impl Goertzel {
    fn new(target_freq: f32, sample_rate: u32, window_size: usize) -> Self {
        let k = 0.5 + (window_size as f32 * target_freq) / sample_rate as f32;
        let omega = (2.0 * std::f32::consts::PI * k) / window_size as f32;
        let coeff = 2.0 * omega.cos();
        let window = (0..window_size)
            .map(|i| {
                0.54 - 0.46 * (2.0 * std::f32::consts::PI * i as f32 / window_size as f32).cos()
            })
            .collect();
        Self { coeff, window }
    }
    fn run(&self, samples: &[f32]) -> f32 {
        let mut q1 = 0.0;
        let mut q2 = 0.0;
        for (i, &sample) in samples.iter().enumerate() {
            let q0 = self.coeff * q1 - q2 + sample * self.window[i];
            q2 = q1;
            q1 = q0;
        }
        q1 * q1 + q2 * q2 - self.coeff * q1 * q2
    }
    fn process_decimated(&self, samples: &[f32], step_size: usize) -> Vec<f32> {
        if samples.len() < self.window.len() {
            return Vec::new();
        }
        samples
            .windows(self.window.len())
            .step_by(step_size)
            .map(|chunk| self.run(chunk))
            .collect()
    }
}
pub struct MorseDecoder {
    resampler: Option<SincFixedIn<f32>>,
    filter_hp: BiquadFilter,
    filter_lp: BiquadFilter,
    input_buffer: Vec<f32>, // Buffer for raw audio before resampling
    audio_buffer: Vec<f32>, // Buffer for resampled, filtered audio
    target_sample_rate: u32,
}

impl MorseDecoder {
    pub fn new(source_sample_rate: u32, target_sample_rate: u32) -> Result<Self> {
        let resampler = if source_sample_rate != target_sample_rate {
            Some(SincFixedIn::new(
                target_sample_rate as f64 / source_sample_rate as f64,
                2.0,
                SincInterpolationParameters {
                    sinc_len: 256,
                    f_cutoff: 0.95,
                    interpolation: SincInterpolationType::Linear,
                    oversampling_factor: 256,
                    window: WindowFunction::BlackmanHarris,
                },
                RESAMPLER_CHUNK_SIZE,
                1,
            )?)
        } else {
            None
        };

        Ok(Self {
            resampler,
            filter_hp: BiquadFilter::new(FilterType::HighPass, FREQ_MIN_HZ, target_sample_rate),
            filter_lp: BiquadFilter::new(FilterType::LowPass, FREQ_MAX_HZ, target_sample_rate),
            input_buffer: Vec::new(),
            audio_buffer: Vec::new(),
            target_sample_rate,
        })
    }

    /// Processes a chunk of audio. Buffers input to meet the resampler's requirements.
    pub fn process(&mut self, chunk: &[f32]) -> Result<()> {
        if let Some(resampler) = &mut self.resampler {
            // Add new audio to our input buffer
            self.input_buffer.extend_from_slice(chunk);

            // Process full chunks from the buffer
            while self.input_buffer.len() >= RESAMPLER_CHUNK_SIZE {
                let waves_in = &[&self.input_buffer[..RESAMPLER_CHUNK_SIZE]];
                let mut resampled = resampler.process(waves_in, None)?;
                self.input_buffer.drain(..RESAMPLER_CHUNK_SIZE);

                let mut processed_chunk = resampled.remove(0);
                self.filter_hp.process(&mut processed_chunk);
                self.filter_lp.process(&mut processed_chunk);
                self.audio_buffer.extend(processed_chunk);
            }
        } else {
            // No resampling, just filter and add to the main buffer
            let mut processed_chunk = chunk.to_vec();
            self.filter_hp.process(&mut processed_chunk);
            self.filter_lp.process(&mut processed_chunk);
            self.audio_buffer.extend(processed_chunk);
        }
        Ok(())
    }

    /// Finalizes decoding. Processes any remaining buffered audio and decodes the full signal.
    pub fn finalize(&mut self) -> Result<String> {
        // --- Flush remaining audio from the input buffer ---
        if let Some(resampler) = &mut self.resampler {
            if !self.input_buffer.is_empty() {
                // Pad the remaining buffer to the required chunk size if needed
                while self.input_buffer.len() < RESAMPLER_CHUNK_SIZE {
                    self.input_buffer.push(0.0);
                }
                let waves_in = &[self.input_buffer.as_slice()];
                let mut resampled = resampler.process(waves_in, None)?;
                self.input_buffer.clear();

                let mut processed_chunk = resampled.remove(0);
                self.filter_hp.process(&mut processed_chunk);
                self.filter_lp.process(&mut processed_chunk);
                self.audio_buffer.extend(processed_chunk);
            }
        }

        if self.audio_buffer.is_empty() {
            bail!("Audio buffer is empty, cannot process.");
        }

        // --- The rest of the decoding pipeline is unchanged ---
        let pitch = self.detect_pitch_stft()?;
        log::info!("Estimated pitch: {:.2} Hz", pitch);

        let goertzel_window_size = (self.target_sample_rate as f32 * 0.025) as usize;
        let step_size = (goertzel_window_size / 4).max(1);
        let goertzel_filter = Goertzel::new(pitch, self.target_sample_rate, goertzel_window_size);
        let raw_power = goertzel_filter.process_decimated(&self.audio_buffer, step_size);
        let power_signal_rate = self.target_sample_rate as f32 / step_size as f32;

        let smooth_window = (power_signal_rate * 0.02).round() as usize;
        let smoothed_power = moving_average(&raw_power, smooth_window.max(1));
        if smoothed_power.is_empty() {
            bail!("No power signal after processing");
        }

        let (best_wpm, best_threshold) =
            self.find_best_params(&smoothed_power, power_signal_rate)?;
        log::info!(
            "Best fit: WPM = {:.1}, Threshold = {:.4e}",
            best_wpm,
            best_threshold
        );

        if log::log_enabled!(log::Level::Trace) {
            trace_signal(&smoothed_power, best_threshold, best_wpm)?;
            log::trace!("Wrote signal trace to signal_trace.txt");
        }

        let text =
            self.decode_with_params(&smoothed_power, best_wpm, best_threshold, power_signal_rate);
        Ok(text)
    }

    // --- The complex analysis functions below are unchanged ---
    fn detect_pitch_stft(&self) -> Result<f32> {
        let fft_size = 4096;
        let step_size = fft_size / 4;
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(fft_size);
        let window: Vec<f32> = (0..fft_size)
            .map(|i| 0.54 - 0.46 * (2.0 * std::f32::consts::PI * i as f32 / fft_size as f32).cos())
            .collect();
        let mut spectrum_sum = vec![0.0; fft_size / 2];
        let mut count = 0;
        for chunk in self.audio_buffer.windows(fft_size).step_by(step_size) {
            let mut buffer: Vec<Complex<f32>> = chunk
                .iter()
                .zip(window.iter())
                .map(|(s, w)| Complex::new(s * w, 0.0))
                .collect();
            fft.process(&mut buffer);
            for (i, v) in buffer.iter().take(fft_size / 2).enumerate() {
                spectrum_sum[i] += v.norm_sqr();
            }
            count += 1;
        }
        if count == 0 {
            bail!("Not enough audio data for pitch detection");
        }
        let df = self.target_sample_rate as f32 / fft_size as f32;
        let (max_idx, max_power) =
            spectrum_sum
                .iter()
                .enumerate()
                .fold((0, 0.0), |(max_i, max_p), (i, &p)| {
                    let freq = i as f32 * df;
                    if (FREQ_MIN_HZ..=FREQ_MAX_HZ).contains(&freq) && p > max_p {
                        (i, p)
                    } else {
                        (max_i, max_p)
                    }
                });
        if max_power == 0.0 {
            bail!("Could not find a dominant frequency in the specified range.");
        }
        Ok(max_idx as f32 * df)
    }

    fn find_best_params(&self, power_signal: &[f32], power_signal_rate: f32) -> Result<(f32, f32)> {
        if power_signal.is_empty() {
            bail!("Power signal is empty");
        }
        let mut sorted_power: Vec<f32> =
            power_signal.iter().cloned().filter(|&p| p > 0.0).collect();
        if sorted_power.len() < 10 {
            bail!("Not enough signal to determine parameters");
        }
        sorted_power.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        let p25 = sorted_power[(sorted_power.len() as f32 * 0.25) as usize];
        let p75 = sorted_power[(sorted_power.len() as f32 * 0.75) as usize];
        let iqr = p75 - p25;
        let threshold_candidates = [
            p25 + iqr * 0.01,
            p25 + iqr * 0.02,
            p25 + iqr * 0.05,
            p25 + iqr * 0.10,
            p25 + iqr * 0.25,
            p25 + iqr * 0.50,
            p25 + iqr * 0.75,
        ];
        let mut best_cost = f32::MAX;
        let mut best_wpm = 20.0;
        let mut best_threshold = threshold_candidates[1];
        for &threshold in &threshold_candidates {
            for wpm_int in 5..=50 {
                let wpm = wpm_int as f32;
                let timing_cost =
                    self.calculate_cost(power_signal, wpm, threshold, power_signal_rate);
                if !timing_cost.is_finite() {
                    continue;
                }
                let decoded =
                    self.decode_with_params(power_signal, wpm, threshold, power_signal_rate);
                let cost = timing_cost + 0.35 * decode_quality_cost(&decoded);
                if cost < best_cost {
                    best_cost = cost;
                    best_wpm = wpm;
                    best_threshold = threshold;
                }
            }
        }
        Ok((best_wpm, best_threshold))
    }

    fn calculate_cost(
        &self,
        power_signal: &[f32],
        wpm: f32,
        threshold: f32,
        power_signal_rate: f32,
    ) -> f32 {
        let (on_intervals, off_intervals) = get_raw_intervals(power_signal, threshold);
        if on_intervals.len() < 3 || off_intervals.len() < 3 {
            return f32::MAX;
        }
        let dot_len_samples = (1200.0 / wpm / 1000.0) * power_signal_rate;
        if dot_len_samples < 1.0 {
            return f32::MAX;
        }
        let on_norm: Vec<f32> = on_intervals
            .iter()
            .map(|&s| s as f32 / dot_len_samples)
            .collect();
        let off_norm: Vec<f32> = off_intervals
            .iter()
            .map(|&s| s as f32 / dot_len_samples)
            .collect();
        let mut short_elements: Vec<f32> = on_norm
            .iter()
            .chain(off_norm.iter())
            .cloned()
            .filter(|&l| l < 2.0)
            .collect();
        if short_elements.is_empty() {
            return f32::MAX;
        }
        short_elements.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        let median_dot_len = short_elements[short_elements.len() / 2];
        if median_dot_len < 0.25 {
            return f32::MAX;
        }
        // Reject degenerate WPM hypotheses where every on-interval falls
        // into a single cluster (all dits or all dahs). Without this
        // guard a wildly-wrong WPM that maps every element to "1 dot"
        // wins because each per-element cost is small. Real Morse for
        // anything beyond a single character has both dits and dahs;
        // require at least one short and one long ON element relative
        // to the median dot length, with the long cluster mean roughly
        // 2..5 dot lengths (dah ratios in the wild fall in 2.3..3.5).
        let on_short_n = on_norm.iter().filter(|&&l| l < 2.0).count();
        let on_long_n = on_norm.iter().filter(|&&l| l >= 2.0).count();
        if on_short_n == 0 || on_long_n == 0 {
            return f32::MAX;
        }
        let on_long_mean: f32 =
            on_norm.iter().filter(|&&l| l >= 2.0).copied().sum::<f32>() / on_long_n as f32;
        let dah_ratio = on_long_mean / median_dot_len;
        if !(2.0..=5.0).contains(&dah_ratio) {
            return f32::MAX;
        }
        // Penalize WPM hypotheses where median_dot_len drifts far from 1.0.
        // Without this, two different WPM candidates produce near-identical
        // residual costs (the residual is shape-invariant under WPM
        // rescaling once it's normalized by median_dot_len), so the picker
        // would happily settle on a WPM 30..50% off the true value, which
        // shifts both the dit/dah and letter/word boundaries enough to
        // shred multi-element letters under high jitter. The correct WPM
        // makes median_dot_len ≈ 1.0; deviation in either direction is bad.
        // Use log-space distance so the penalty is symmetric for halving vs
        // doubling.
        let wpm_drift = median_dot_len.ln().powi(2);
        let cost_on: f32 = on_norm
            .iter()
            .map(|&len| {
                (len / median_dot_len - 1.0)
                    .powi(2)
                    .min((len / median_dot_len - 3.0).powi(2))
            })
            .sum();
        let cost_off: f32 = off_norm
            .iter()
            .map(|&len| {
                (len / median_dot_len - 1.0)
                    .powi(2)
                    .min((len / median_dot_len - 3.0).powi(2))
                    .min((len / median_dot_len - 7.0).powi(2))
            })
            .sum();
        (cost_on / on_intervals.len() as f32)
            + (cost_off / off_intervals.len() as f32)
            + 0.25 * wpm_drift
    }

    fn decode_with_params(
        &self,
        power_signal: &[f32],
        wpm: f32,
        threshold: f32,
        power_signal_rate: f32,
    ) -> String {
        // First pass: collect all element lengths for self-calibration
        let (on_intervals, off_intervals) = get_raw_intervals(power_signal, threshold);

        if on_intervals.is_empty() {
            return String::new();
        }

        // Self-calibrate: detect if we have mixed dots/dashes or all same type
        let mut sorted_lengths = on_intervals.clone();
        sorted_lengths.sort_unstable();

        let min_len = sorted_lengths[0] as f32;
        let max_len = sorted_lengths[sorted_lengths.len() - 1] as f32;
        let length_ratio = max_len / min_len;

        let lengths_for_high: Vec<f32> = sorted_lengths.iter().map(|&x| x as f32).collect();
        let on_centroids = if length_ratio > 1.8 {
            kmeans_two_centroids(&lengths_for_high)
        } else {
            None
        };

        let raw_dot_len = if length_ratio > 1.8 {
            // Mixed signal: 2-means cluster the on-intervals to find the
            // dit cluster centroid. Robust against rough fists where the
            // dah/dit ratio drops below the textbook 3:1, which used to
            // collapse the old "median of shortest half" heuristic into
            // a number small enough that every real dit looked like a dah.
            on_centroids
                .map(|(low, _high)| low)
                .unwrap_or_else(|| sorted_lengths[sorted_lengths.len() / 4] as f32)
        } else {
            // All similar lengths: Use a simple heuristic based on absolute length
            // This is more robust than relying on potentially inaccurate WPM estimates
            let median_len = sorted_lengths[sorted_lengths.len() / 2] as f32;

            // Based on actual observed values:
            // - EEEE (dots): median ~10 power signal samples
            // - TTTT (dashes): median ~29 power signal samples
            // Use a breakpoint between these ranges
            let breakpoint = 18.0;

            if median_len > breakpoint {
                // Likely all dashes - use theoretical dot length
                median_len / 3.0
            } else {
                // Likely all dots
                median_len
            }
        };
        let theoretical_dot_len = ((1200.0 / wpm / 1000.0) * power_signal_rate).max(1.0);
        let actual_dot_len = if length_ratio > 1.8 && raw_dot_len < theoretical_dot_len * 0.65 {
            // A very low ON centroid usually means the tone envelope is being
            // sliced into half-dits by threshold chatter, not that the sender
            // suddenly doubled speed.  The WPM search has already selected a
            // timing hypothesis from both ON and OFF intervals, so use it as a
            // lower-bound prior for self-calibration.  Without this, long
            // well-toned QSOs can normalize true element gaps to ~2 dots and
            // the beam happily emits E/T-heavy fragments.
            (raw_dot_len * theoretical_dot_len).sqrt()
        } else {
            raw_dot_len
        };

        // Log calibration for debugging
        log::debug!(
            "Self-calibration: WPM={:.1}, raw_dot_len={:.1}, actual_dot_len={:.1}, theoretical_dot_len={:.1} samples",
            wpm,
            raw_dot_len,
            actual_dot_len,
            theoretical_dot_len
        );
        log::debug!("Element lengths: {:?}", on_intervals);

        let dit_dah_boundary_norm = if length_ratio > 1.8 {
            if let Some((low, high)) = on_centroids {
                if low > 0.0 && high > actual_dot_len * 1.3 {
                    // Midpoint between dit and dah centroids in normalized
                    // (dot-length) units. This adapts to rough fists where
                    // the textbook 2.0× boundary misses dahs that fall
                    // closer to 1.7..2.5 of the dit length.
                    0.5 * (1.0 + high / actual_dot_len)
                } else {
                    DIT_DAH_BOUNDARY
                }
            } else {
                DIT_DAH_BOUNDARY
            }
        } else {
            DIT_DAH_BOUNDARY
        };

        // Adaptive letter-space and word-space boundaries via 3-means
        // cluster on the off-interval distribution (element / letter / word
        // gaps). Boundaries are placed at the midpoints of adjacent
        // centroids. With heavy timing jitter the canonical (1, 3, 7) dot
        // gaps drift enough that fixed thresholds shred letters; a 3-cluster
        // model adapts to the actual fist while a 2-cluster fallback covers
        // short clips with no word gaps at all.
        let mut gap_centroids_norm = (1.0_f32, 3.0_f32, 7.0_f32);
        let (letter_space_norm, word_space_norm) = {
            let off_norm: Vec<f32> = off_intervals
                .iter()
                .map(|&s| s as f32 / actual_dot_len)
                // Drop the trailing silence and other gross outliers.
                .filter(|&l| l < 12.0)
                .collect();
            if off_norm.len() >= 6 {
                if let Some((c1, c2, c3)) = kmeans_three_centroids(&off_norm) {
                    gap_centroids_norm = (c1, c2, c3);
                    // c1 = element gap, c2 = letter gap, c3 = word gap
                    let letter_b = if c2 > c1 * 1.3 {
                        (0.5 * (c1 + c2)).clamp(1.8, 3.4)
                    } else {
                        // Bad split (clusters too close); fall back.
                        (2.0 * c1).clamp(1.7, 2.6)
                    };
                    let word_b = if c3 > c2 * 1.3 {
                        (0.5 * (c2 + c3)).clamp(letter_b + 0.5, 8.0)
                    } else {
                        WORD_SPACE_BOUNDARY
                    };
                    (letter_b, word_b)
                } else if let Some((low, _high)) = kmeans_two_centroids(&off_norm) {
                    gap_centroids_norm = (low, (low * 3.0).max(3.0), 7.0);
                    let letter_b = if low > 0.0 {
                        (2.0 * low).clamp(1.7, 2.6)
                    } else {
                        LETTER_SPACE_BOUNDARY
                    };
                    (letter_b, WORD_SPACE_BOUNDARY)
                } else {
                    (LETTER_SPACE_BOUNDARY, WORD_SPACE_BOUNDARY)
                }
            } else if off_norm.len() >= 3 {
                if let Some((low, _high)) = kmeans_two_centroids(&off_norm) {
                    gap_centroids_norm = (low, (low * 3.0).max(3.0), 7.0);
                    let letter_b = if low > 0.0 {
                        (2.0 * low).clamp(1.7, 2.6)
                    } else {
                        LETTER_SPACE_BOUNDARY
                    };
                    (letter_b, WORD_SPACE_BOUNDARY)
                } else {
                    (LETTER_SPACE_BOUNDARY, WORD_SPACE_BOUNDARY)
                }
            } else {
                (LETTER_SPACE_BOUNDARY, WORD_SPACE_BOUNDARY)
            }
        };
        log::debug!(
            "Gap centroids/boundaries (dot units): c1={:.2}, c2={:.2}, c3={:.2}, letter_b={:.2}, word_b={:.2}",
            gap_centroids_norm.0,
            gap_centroids_norm.1,
            gap_centroids_norm.2,
            letter_space_norm,
            word_space_norm
        );

        let mut result = String::new();
        let mut current_letter = String::new();
        if power_signal.is_empty() {
            return result;
        }
        let mut current_len = 0;
        let mut is_on = power_signal[0] > threshold;
        let debounce_samples = (actual_dot_len * 0.3).round() as usize;
        log::debug!("Debounce threshold: {} samples", debounce_samples);
        log::debug!(
            "Dit/dah boundary (in dot units): {:.2}",
            dit_dah_boundary_norm
        );

        let dah_len_norm = on_centroids
            .map(|(_low, high)| {
                if actual_dot_len > 0.0 {
                    high / actual_dot_len
                } else {
                    3.0
                }
            })
            .unwrap_or(3.0)
            .clamp(1.8, 4.5);
        let beam_result = decode_with_morse_beam(
            power_signal,
            threshold,
            actual_dot_len,
            dah_len_norm,
            gap_centroids_norm,
            letter_space_norm,
            word_space_norm,
            debounce_samples,
        );
        if !beam_result.is_empty() {
            return beam_result;
        }

        for &p in power_signal.iter().chain(std::iter::once(&0.0)) {
            if (p > threshold) == is_on {
                current_len += 1;
            } else {
                if current_len > debounce_samples {
                    let len_norm = current_len as f32 / actual_dot_len;
                    if is_on {
                        if len_norm < dit_dah_boundary_norm {
                            current_letter.push('.');
                        } else {
                            current_letter.push('-');
                        }
                    } else {
                        // Handle gaps (off periods)
                        if len_norm > letter_space_norm {
                            // Gap is long enough to end the current letter
                            if !current_letter.is_empty() {
                                if let Some(c) = morse_to_char(&current_letter) {
                                    result.push(c);
                                } else {
                                    result.push('?');
                                }
                                current_letter.clear();
                            }
                            // If gap is also long enough for word boundary, add space
                            if len_norm > word_space_norm && !result.ends_with(' ') {
                                result.push(' ');
                            }
                        }
                        // If gap is shorter than LETTER_SPACE_BOUNDARY, it's just an element gap - ignore
                    }
                }
                is_on = !is_on;
                current_len = 1;
            }
        }

        // Process any remaining letter at the end
        if !current_letter.is_empty() {
            if let Some(c) = morse_to_char(&current_letter) {
                result.push(c);
            } else {
                result.push('?');
            }
        }

        result.trim().to_string()
    }
}

// --- Helper Functions ---

/// 1-D k-means with k=2 on positive lengths. Returns `(low_centroid,
/// high_centroid)` if the input contains at least two distinct non-empty
/// clusters after convergence, otherwise `None`.
fn kmeans_two_centroids(values: &[f32]) -> Option<(f32, f32)> {
    if values.len() < 2 {
        return None;
    }
    let min = values.iter().cloned().fold(f32::INFINITY, f32::min);
    let max = values.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    if (max - min).abs() < f32::EPSILON {
        return None;
    }
    // Seed centroids at 25th and 75th percentile (sorted assumed by caller
    // in our usage; sort defensively otherwise).
    let mut sorted: Vec<f32> = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut c_low = sorted[sorted.len() / 4];
    let mut c_high = sorted[(3 * sorted.len()) / 4];
    if (c_high - c_low).abs() < f32::EPSILON {
        c_low = min;
        c_high = max;
    }
    for _ in 0..32 {
        let mut sum_low = 0.0;
        let mut n_low = 0usize;
        let mut sum_high = 0.0;
        let mut n_high = 0usize;
        for &v in values {
            if (v - c_low).abs() <= (v - c_high).abs() {
                sum_low += v;
                n_low += 1;
            } else {
                sum_high += v;
                n_high += 1;
            }
        }
        if n_low == 0 || n_high == 0 {
            return None;
        }
        let new_low = sum_low / n_low as f32;
        let new_high = sum_high / n_high as f32;
        if (new_low - c_low).abs() < 1e-3 && (new_high - c_high).abs() < 1e-3 {
            c_low = new_low;
            c_high = new_high;
            break;
        }
        c_low = new_low;
        c_high = new_high;
    }
    if c_low > c_high {
        std::mem::swap(&mut c_low, &mut c_high);
    }
    Some((c_low, c_high))
}

/// 1-D k-means with k=3 on positive lengths. Returns
/// `(c1, c2, c3)` sorted ascending if the input partitions into three
/// non-empty clusters, otherwise `None`. Used to split off-interval
/// distributions into element/letter/word gap buckets.
fn kmeans_three_centroids(values: &[f32]) -> Option<(f32, f32, f32)> {
    if values.len() < 3 {
        return None;
    }
    let mut sorted: Vec<f32> = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut c1 = sorted[sorted.len() / 6];
    let mut c2 = sorted[sorted.len() / 2];
    let mut c3 = sorted[(5 * sorted.len()) / 6];
    if (c2 - c1).abs() < f32::EPSILON || (c3 - c2).abs() < f32::EPSILON {
        c1 = sorted[0];
        c2 = sorted[sorted.len() / 2];
        c3 = sorted[sorted.len() - 1];
    }
    if (c3 - c1).abs() < f32::EPSILON {
        return None;
    }
    for _ in 0..32 {
        let (mut s1, mut n1) = (0.0f32, 0usize);
        let (mut s2, mut n2) = (0.0f32, 0usize);
        let (mut s3, mut n3) = (0.0f32, 0usize);
        for &v in values {
            let d1 = (v - c1).abs();
            let d2 = (v - c2).abs();
            let d3 = (v - c3).abs();
            if d1 <= d2 && d1 <= d3 {
                s1 += v;
                n1 += 1;
            } else if d2 <= d3 {
                s2 += v;
                n2 += 1;
            } else {
                s3 += v;
                n3 += 1;
            }
        }
        if n1 == 0 || n2 == 0 || n3 == 0 {
            return None;
        }
        let nc1 = s1 / n1 as f32;
        let nc2 = s2 / n2 as f32;
        let nc3 = s3 / n3 as f32;
        let conv = (nc1 - c1).abs() < 1e-3 && (nc2 - c2).abs() < 1e-3 && (nc3 - c3).abs() < 1e-3;
        c1 = nc1;
        c2 = nc2;
        c3 = nc3;
        if conv {
            break;
        }
    }
    let mut sorted_c = [c1, c2, c3];
    sorted_c.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some((sorted_c[0], sorted_c[1], sorted_c[2]))
}

#[derive(Clone)]
struct MorseBeamState {
    text: String,
    pattern: String,
    score: f32,
}

fn decode_with_morse_beam(
    power_signal: &[f32],
    threshold: f32,
    dot_len: f32,
    dah_len_norm: f32,
    gap_centroids_norm: (f32, f32, f32),
    letter_space_norm: f32,
    word_space_norm: f32,
    debounce_samples: usize,
) -> String {
    if power_signal.is_empty() || dot_len <= 0.0 {
        return String::new();
    }

    let intervals = ordered_intervals(power_signal, threshold, debounce_samples.max(1));
    if intervals.iter().filter(|(is_on, _)| *is_on).count() < 2 {
        return String::new();
    }

    let mut beams = vec![MorseBeamState {
        text: String::new(),
        pattern: String::new(),
        score: 0.0,
    }];

    for (is_on, len) in intervals {
        let len_norm = len as f32 / dot_len;
        let mut next = Vec::with_capacity(beams.len() * 4);
        if is_on {
            for state in &beams {
                extend_symbol(&mut next, state, '.', len_norm, 1.0);
                extend_symbol(&mut next, state, '-', len_norm, dah_len_norm);
            }
        } else {
            for state in &beams {
                extend_gap(
                    &mut next,
                    state,
                    len_norm,
                    gap_centroids_norm,
                    letter_space_norm,
                    word_space_norm,
                );
            }
        }
        beams = prune_beam(next);
        if beams.is_empty() {
            return String::new();
        }
    }

    let mut finals = Vec::with_capacity(beams.len() * 2);
    for state in beams {
        if state.pattern.is_empty() {
            finals.push(state);
        } else if let Some(ch) = morse_alnum_to_char(&state.pattern) {
            let mut emitted = state.clone();
            push_char(&mut emitted.text, ch);
            emitted.pattern.clear();
            finals.push(emitted);
        }
    }

    finals
        .into_iter()
        .min_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|state| state.text.trim().to_string())
        .unwrap_or_default()
}

fn extend_symbol(
    next: &mut Vec<MorseBeamState>,
    state: &MorseBeamState,
    symbol: char,
    len_norm: f32,
    target_norm: f32,
) {
    let mut pattern = state.pattern.clone();
    pattern.push(symbol);
    if pattern.len() > 5 || !is_morse_alnum_prefix(&pattern) {
        return;
    }
    next.push(MorseBeamState {
        text: state.text.clone(),
        pattern,
        score: state.score + morse_len_cost(len_norm, target_norm),
    });
}

fn extend_gap(
    next: &mut Vec<MorseBeamState>,
    state: &MorseBeamState,
    len_norm: f32,
    gap_centroids_norm: (f32, f32, f32),
    letter_space_norm: f32,
    word_space_norm: f32,
) {
    let (element_gap, letter_gap, word_gap) = gap_centroids_norm;

    if state.pattern.is_empty() {
        let mut carry = state.clone();
        if !carry.text.is_empty() && len_norm > (letter_gap + word_gap) * 0.5 {
            push_space(&mut carry.text);
        }
        next.push(carry);
        return;
    }

    if state.pattern.len() < 5 && is_morse_alnum_prefix(&state.pattern) {
        next.push(MorseBeamState {
            text: state.text.clone(),
            pattern: state.pattern.clone(),
            score: state.score + morse_len_cost(len_norm, element_gap),
        });
    }

    if len_norm >= letter_space_norm
        && let Some(ch) = morse_alnum_to_char(&state.pattern)
    {
        let mut text = state.text.clone();
        push_char(&mut text, ch);
        next.push(MorseBeamState {
            text: text.clone(),
            pattern: String::new(),
            score: state.score + morse_len_cost(len_norm, letter_gap),
        });

        if len_norm >= word_space_norm {
            push_space(&mut text);
            next.push(MorseBeamState {
                text,
                pattern: String::new(),
                score: state.score + morse_len_cost(len_norm, word_gap),
            });
        }
    }
}

fn prune_beam(mut states: Vec<MorseBeamState>) -> Vec<MorseBeamState> {
    states.sort_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    states.truncate(MORSE_BEAM_WIDTH);
    states
}

fn morse_len_cost(observed: f32, target: f32) -> f32 {
    let observed = observed.max(0.05);
    let target = target.max(0.05);
    let ratio = observed / target;
    ratio.ln().powi(2)
}

fn push_char(text: &mut String, ch: char) {
    text.push(ch);
}

fn push_space(text: &mut String) {
    if !text.is_empty() && !text.ends_with(' ') {
        text.push(' ');
    }
}

fn decode_quality_cost(text: &str) -> f32 {
    let mut alnum_count = 0usize;
    let mut unknown_count = 0usize;
    let mut single_element_count = 0usize;
    let mut element_sum = 0usize;

    for ch in text.chars() {
        if ch == '?' {
            unknown_count += 1;
            continue;
        }
        if ch.is_ascii_alphanumeric() {
            alnum_count += 1;
            if let Some(morse) = morse_for_alnum(ch) {
                let element_count = morse.len();
                element_sum += element_count;
                if element_count == 1 {
                    single_element_count += 1;
                }
            }
        }
    }

    if alnum_count == 0 {
        return unknown_count as f32 * 2.0;
    }

    let avg_elements = element_sum as f32 / alnum_count as f32;
    let single_fraction = single_element_count as f32 / alnum_count as f32;
    let unknown_fraction = unknown_count as f32 / (alnum_count + unknown_count).max(1) as f32;

    let complexity_penalty = (2.2 - avg_elements).max(0.0).powi(2) * 8.0;
    let fragmentation_penalty = (single_fraction - 0.45).max(0.0).powi(2) * 10.0;
    let unknown_penalty = unknown_fraction * 4.0;

    complexity_penalty + fragmentation_penalty + unknown_penalty
}

fn morse_for_alnum(ch: char) -> Option<&'static str> {
    let ch = ch.to_ascii_uppercase();
    MORSE_ALNUM_TABLE
        .iter()
        .find_map(|&(morse, c)| (c == ch).then_some(morse))
}

fn ordered_intervals(
    power_signal: &[f32],
    threshold: f32,
    debounce_samples: usize,
) -> Vec<(bool, usize)> {
    fn push_interval(intervals: &mut Vec<(bool, usize)>, is_on: bool, len: usize) {
        if len == 0 {
            return;
        }
        if let Some((last_is_on, last_len)) = intervals.last_mut() {
            if *last_is_on == is_on {
                *last_len += len;
                return;
            }
        }
        intervals.push((is_on, len));
    }

    let mut raw_intervals = Vec::new();
    if power_signal.is_empty() {
        return raw_intervals;
    }

    let mut current_len = 0usize;
    let mut is_on = power_signal[0] > threshold;
    for &p in power_signal {
        if (p > threshold) == is_on {
            current_len += 1;
        } else {
            raw_intervals.push((is_on, current_len));
            is_on = !is_on;
            current_len = 1;
        }
    }
    raw_intervals.push((is_on, current_len));

    if debounce_samples == 0 {
        return raw_intervals;
    }

    let mut intervals = Vec::with_capacity(raw_intervals.len());
    for (idx, (run_is_on, run_len)) in raw_intervals.iter().copied().enumerate() {
        if run_len > debounce_samples {
            push_interval(&mut intervals, run_is_on, run_len);
            continue;
        }

        if let Some((last_is_on, last_len)) = intervals.last_mut() {
            // Treat sub-dwell opposite-polarity runs as threshold chatter:
            // bridge tiny OFF holes inside key-downs and absorb tiny ON
            // spikes inside key-up gaps, but only when a following accepted
            // run confirms the previous state.  Trailing sub-dwell runs are
            // dropped instead of inflating the final symbol/gap.
            if raw_intervals
                .iter()
                .skip(idx + 1)
                .find(|(_, len)| *len > debounce_samples)
                .is_some_and(|(next_is_on, _)| next_is_on == last_is_on)
            {
                *last_len += run_len;
            }
        }
        // If the capture starts with a tiny glitch, there is no previous
        // accepted state. Drop it rather than emitting a standalone E/T-sized
        // run.
    }
    intervals
}

fn get_raw_intervals(power_signal: &[f32], threshold: f32) -> (Vec<usize>, Vec<usize>) {
    let mut on = Vec::new();
    let mut off = Vec::new();
    if power_signal.is_empty() {
        return (on, off);
    }

    let mut current_len = 0;
    let mut is_on = power_signal[0] > threshold;
    for &p in power_signal {
        if (p > threshold) == is_on {
            current_len += 1;
        } else {
            if is_on {
                on.push(current_len);
            } else {
                off.push(current_len);
            }
            is_on = !is_on;
            current_len = 1;
        }
    }
    if is_on {
        on.push(current_len);
    } else {
        off.push(current_len);
    }
    (on, off)
}

fn moving_average(data: &[f32], window_size: usize) -> Vec<f32> {
    if window_size <= 1 {
        return data.to_vec();
    }
    let mut smoothed = Vec::with_capacity(data.len());
    let mut sum = 0.0;
    let mut window = VecDeque::with_capacity(window_size);
    for &x in data {
        if window.len() == window_size {
            sum -= window.pop_front().unwrap();
        }
        sum += x;
        window.push_back(x);
        smoothed.push(sum / window.len() as f32);
    }
    smoothed
}

fn trace_signal(signal: &[f32], threshold: f32, wpm: f32) -> std::io::Result<()> {
    let mut file = std::fs::File::create("signal_trace.txt")?;
    writeln!(file, "# WPM: {:.1}, Threshold: {:.4e}", wpm, threshold)?;
    let max_val = signal.iter().cloned().fold(f32::MIN, f32::max);
    if max_val <= 0.0 {
        return Ok(());
    }

    for &val in signal {
        let bar_len = (val / max_val * 100.0).round() as usize;
        let thresh_pos = (threshold / max_val * 100.0).round() as usize;
        let mut line = vec![' '; 101];
        for item in line.iter_mut().take(bar_len.min(100)) {
            *item = '#';
        }
        if thresh_pos <= 100 {
            line[thresh_pos] = '|';
        }
        writeln!(file, "{}", line.into_iter().collect::<String>())?;
    }
    Ok(())
}

fn morse_to_char(s: &str) -> Option<char> {
    MORSE_TABLE
        .iter()
        .find_map(|&(morse, ch)| (morse == s).then_some(ch))
}

fn morse_alnum_to_char(s: &str) -> Option<char> {
    MORSE_ALNUM_TABLE
        .iter()
        .find_map(|&(morse, ch)| (morse == s).then_some(ch))
}

fn is_morse_alnum_prefix(s: &str) -> bool {
    MORSE_ALNUM_TABLE
        .iter()
        .any(|&(morse, _)| morse.starts_with(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_power_run(signal: &mut Vec<f32>, is_on: bool, len: usize) {
        signal.extend(std::iter::repeat(if is_on { 1.0 } else { 0.0 }).take(len));
    }

    #[test]
    fn ordered_intervals_bridges_sub_dwell_gaps() {
        let mut signal = Vec::new();
        push_power_run(&mut signal, true, 6);
        push_power_run(&mut signal, false, 2);
        push_power_run(&mut signal, true, 7);
        push_power_run(&mut signal, false, 8);

        let intervals = ordered_intervals(&signal, 0.5, 3);

        assert_eq!(intervals, vec![(true, 15), (false, 8)]);
    }

    #[test]
    fn ordered_intervals_absorbs_sub_dwell_on_blips() {
        let mut signal = Vec::new();
        push_power_run(&mut signal, false, 7);
        push_power_run(&mut signal, true, 2);
        push_power_run(&mut signal, false, 6);
        push_power_run(&mut signal, true, 5);

        let intervals = ordered_intervals(&signal, 0.5, 3);

        assert_eq!(intervals, vec![(false, 15), (true, 5)]);
    }

    #[test]
    fn ordered_intervals_drops_trailing_sub_dwell_runs() {
        let mut signal = Vec::new();
        push_power_run(&mut signal, true, 6);
        push_power_run(&mut signal, false, 2);

        let intervals = ordered_intervals(&signal, 0.5, 3);

        assert_eq!(intervals, vec![(true, 6)]);
    }
}

const MORSE_ALNUM_TABLE: &[(&str, char)] = &[
    (".-", 'A'),
    ("-...", 'B'),
    ("-.-.", 'C'),
    ("-..", 'D'),
    (".", 'E'),
    ("..-.", 'F'),
    ("--.", 'G'),
    ("....", 'H'),
    ("..", 'I'),
    (".---", 'J'),
    ("-.-", 'K'),
    (".-..", 'L'),
    ("--", 'M'),
    ("-.", 'N'),
    ("---", 'O'),
    (".--.", 'P'),
    ("--.-", 'Q'),
    (".-.", 'R'),
    ("...", 'S'),
    ("-", 'T'),
    ("..-", 'U'),
    ("...-", 'V'),
    (".--", 'W'),
    ("-..-", 'X'),
    ("-.--", 'Y'),
    ("--..", 'Z'),
    (".----", '1'),
    ("..---", '2'),
    ("...--", '3'),
    ("....-", '4'),
    (".....", '5'),
    ("-....", '6'),
    ("--...", '7'),
    ("---..", '8'),
    ("----.", '9'),
    ("-----", '0'),
];

const MORSE_TABLE: &[(&str, char)] = &[
    (".-", 'A'),
    ("-...", 'B'),
    ("-.-.", 'C'),
    ("-..", 'D'),
    (".", 'E'),
    ("..-.", 'F'),
    ("--.", 'G'),
    ("....", 'H'),
    ("..", 'I'),
    (".---", 'J'),
    ("-.-", 'K'),
    (".-..", 'L'),
    ("--", 'M'),
    ("-.", 'N'),
    ("---", 'O'),
    (".--.", 'P'),
    ("--.-", 'Q'),
    (".-.", 'R'),
    ("...", 'S'),
    ("-", 'T'),
    ("..-", 'U'),
    ("...-", 'V'),
    (".--", 'W'),
    ("-..-", 'X'),
    ("-.--", 'Y'),
    ("--..", 'Z'),
    (".----", '1'),
    ("..---", '2'),
    ("...--", '3'),
    ("....-", '4'),
    (".....", '5'),
    ("-....", '6'),
    ("--...", '7'),
    ("---..", '8'),
    ("----.", '9'),
    ("-----", '0'),
    (".-.-.-", '.'),
    ("--..--", ','),
    ("..--..", '?'),
    (".----.", '\''),
    ("-.-.--", '!'),
    ("-..-.", '/'),
    ("-.--.", '('),
    ("-.--.-", ')'),
    (".-...", '&'),
    ("---...", ':'),
    ("-.-.-.", ';'),
    ("-...-", '='),
    (".-.-.", '+'),
    ("-....-", '-'),
    ("..--.-", '_'),
    (".-..-.", '"'),
    ("...-..-", '$'),
    (".--.-.", '@'),
];

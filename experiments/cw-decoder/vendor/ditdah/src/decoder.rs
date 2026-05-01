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
        log::info!("Estimated pitch: {pitch:.2} Hz");

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
        log::info!("Best fit: WPM = {best_wpm:.1}, Threshold = {best_threshold:.4e}");

        if log::log_enabled!(log::Level::Trace) {
            trace_signal(&smoothed_power, best_threshold, best_wpm)?;
            log::trace!("Wrote signal trace to signal_trace.txt");
        }

        let text =
            self.decode_with_params(&smoothed_power, best_wpm, best_threshold, power_signal_rate);
        Ok(text)
    }

    /// Like `finalize`, but returns the (text, wpm, threshold) actually used so
    /// callers can pin parameters for subsequent decodes (streaming use case).
    /// If `pin_wpm` is `Some`, skips the WPM grid search and uses that WPM. If
    /// `pin_threshold` is `Some`, skips threshold re-fitting too.
    pub fn finalize_with_params(
        &mut self,
        pin_wpm: Option<f32>,
        pin_threshold: Option<f32>,
    ) -> Result<(String, f32, f32)> {
        if let Some(resampler) = &mut self.resampler {
            if !self.input_buffer.is_empty() {
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

        let pitch = self.detect_pitch_stft()?;
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

        let (wpm, threshold) = match (pin_wpm, pin_threshold) {
            (Some(w), Some(t)) => (w, t),
            (Some(w), None) => {
                let mut sorted = smoothed_power.clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let n = sorted.len();
                let p25 = sorted[n / 4];
                let p75 = sorted[(3 * n) / 4];
                let iqr = p75 - p25;
                (w, p25 + iqr * 0.50)
            }
            (None, Some(t)) => {
                let mut best_wpm = 20.0;
                let mut best_cost = f32::INFINITY;
                for wpm_int in 5..=40 {
                    let w = wpm_int as f32;
                    let cost = self.calculate_cost(&smoothed_power, w, t, power_signal_rate);
                    if cost < best_cost {
                        best_cost = cost;
                        best_wpm = w;
                    }
                }
                (best_wpm, t)
            }
            (None, None) => self.find_best_params(&smoothed_power, power_signal_rate)?,
        };

        let wpm_is_authoritative = pin_wpm.is_some();
        let text = self.decode_with_params_inner(
            &smoothed_power,
            wpm,
            threshold,
            power_signal_rate,
            wpm_is_authoritative,
        );
        Ok((text, wpm, threshold))
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
        let threshold_candidates = [p25 + iqr * 0.25, p25 + iqr * 0.50, p25 + iqr * 0.75];
        let mut best_cost = f32::MAX;
        let mut best_wpm = 20.0;
        let mut best_threshold = threshold_candidates[1];
        for &threshold in &threshold_candidates {
            for wpm_int in 5..=40 {
                let wpm = wpm_int as f32;
                let cost = self.calculate_cost(power_signal, wpm, threshold, power_signal_rate);
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
        (cost_on / on_intervals.len() as f32) + (cost_off / off_intervals.len() as f32)
    }

    fn decode_with_params(
        &self,
        power_signal: &[f32],
        wpm: f32,
        threshold: f32,
        power_signal_rate: f32,
    ) -> String {
        self.decode_with_params_inner(power_signal, wpm, threshold, power_signal_rate, false)
    }

    /// Internal decoder body. When `wpm_is_authoritative` is true, the dot length
    /// is taken from `wpm` (theoretical timing) instead of the median-element-length
    /// self-calibration. The self-calibration heuristic is robust on clean studio
    /// audio but breaks on real-world live signals where the element-length
    /// distribution gets distorted by noise, fading, or partial elements.
    fn decode_with_params_inner(
        &self,
        power_signal: &[f32],
        wpm: f32,
        threshold: f32,
        power_signal_rate: f32,
        wpm_is_authoritative: bool,
    ) -> String {
        // First pass: collect all element lengths for self-calibration
        let (on_intervals, _off_intervals) = get_raw_intervals(power_signal, threshold);

        if on_intervals.is_empty() {
            return String::new();
        }

        let actual_dot_len = if wpm_is_authoritative && wpm > 0.0 {
            // Theoretical dot length in power-signal samples for the pinned WPM.
            (1200.0 / wpm / 1000.0) * power_signal_rate
        } else {
            // Self-calibrate: detect if we have mixed dots/dashes or all same type
            let mut sorted_lengths = on_intervals.clone();
            sorted_lengths.sort_unstable();

            let min_len = sorted_lengths[0] as f32;
            let max_len = sorted_lengths[sorted_lengths.len() - 1] as f32;
            let length_ratio = max_len / min_len;

            if length_ratio > 2.0 {
                // Mixed signal: use shortest elements as dots
                let shortest_half = &sorted_lengths[0..=(sorted_lengths.len() / 2)];
                shortest_half[shortest_half.len() / 2] as f32
            } else {
                // All similar lengths: Use a simple heuristic based on absolute length
                // This is more robust than relying on potentially inaccurate WPM estimates
                let median_len = sorted_lengths[sorted_lengths.len() / 2] as f32;

                // Based on actual observed values:
                // - EEEE (dots): median ~10 power signal samples
                // - TTTT (dashes): median ~29 power signal samples
                let breakpoint = 18.0;

                if median_len > breakpoint {
                    median_len / 3.0
                } else {
                    median_len
                }
            }
        };

        // Log calibration for debugging
        log::debug!(
            "Self-calibration: WPM={wpm:.1} (authoritative={wpm_is_authoritative}), actual_dot_len={actual_dot_len:.1} samples"
        );
        log::debug!("Element lengths: {on_intervals:?}");

        let mut result = String::new();
        let mut current_letter = String::new();
        if power_signal.is_empty() {
            return result;
        }
        let mut current_len = 0;
        let mut is_on = power_signal[0] > threshold;
        let debounce_samples = (actual_dot_len * 0.3).round() as usize;
        log::debug!("Debounce threshold: {debounce_samples} samples");
        for &p in power_signal.iter().chain(std::iter::once(&0.0)) {
            if (p > threshold) == is_on {
                current_len += 1;
            } else {
                if current_len > debounce_samples {
                    let len_norm = current_len as f32 / actual_dot_len;
                    if is_on {
                        if len_norm < DIT_DAH_BOUNDARY {
                            current_letter.push('.');
                        } else {
                            current_letter.push('-');
                        }
                    } else {
                        // Handle gaps (off periods)
                        if len_norm > LETTER_SPACE_BOUNDARY {
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
                            if len_norm > WORD_SPACE_BOUNDARY && !result.ends_with(' ') {
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
    writeln!(file, "# WPM: {wpm:.1}, Threshold: {threshold:.4e}")?;
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

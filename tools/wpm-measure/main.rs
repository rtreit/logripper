// Independent WPM measurement.
//
// Three measurements, all from raw audio, no dependency on the cw-decoder
// or ditdah:
//
//   M1 — Goertzel envelope at adaptive carrier, threshold by Otsu, run-length
//        encode ON intervals, k-means(2) on ON durations -> dot centroid.
//        WPM = 1.2 / dot_seconds (PARIS standard).
//
//   M2 — Same envelope, but compute the median of the lower-half of ON
//        durations (everything <= median(all)). This is robust to dah-heavy
//        text. WPM = 1.2 / median_dot.
//
//   M3 — Histogram bin-mode of ON durations: 200 bins 0..400ms, smoothed,
//        find the lower of the top two peaks -> dot peak. WPM = 1.2 / peak.
//
// Also reports counts, dot/dah ratio, and inter-element gap statistics.

use hound::{SampleFormat, WavReader};
use std::env;
use std::f32::consts::PI;

fn load_wav(path: &str) -> (Vec<f32>, u32) {
    let mut r = WavReader::open(path).expect("open wav");
    let spec = r.spec();
    let s: Vec<f32> = match spec.sample_format {
        SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().map(|x| x.unwrap() as f32 / max).collect()
        }
        SampleFormat::Float => r.samples::<f32>().map(|x| x.unwrap()).collect(),
    };
    // Mix to mono if needed
    let mono: Vec<f32> = if spec.channels == 1 {
        s
    } else {
        let ch = spec.channels as usize;
        s.chunks(ch).map(|c| c.iter().sum::<f32>() / ch as f32).collect()
    };
    (mono, spec.sample_rate)
}

/// Estimate dominant carrier frequency in 400-1200 Hz via DFT-style scan.
fn estimate_carrier(samples: &[f32], rate: u32) -> f32 {
    let n_take = (samples.len()).min(rate as usize * 4);
    let s = &samples[..n_take];
    let mut best_f = 700.0;
    let mut best_p = 0.0;
    let mut f = 400.0;
    while f <= 1200.0 {
        let omega = 2.0 * PI * f / rate as f32;
        let coeff = 2.0 * omega.cos();
        let mut q1 = 0.0;
        let mut q2 = 0.0;
        for &x in s {
            let q0 = coeff * q1 - q2 + x;
            q2 = q1;
            q1 = q0;
        }
        let p = q1 * q1 + q2 * q2 - q1 * q2 * coeff;
        if p > best_p {
            best_p = p;
            best_f = f;
        }
        f += 5.0;
    }
    best_f
}

/// Sliding-window Goertzel envelope at given carrier, returns one envelope
/// sample per window (window=hop=`hop_samples`).
fn goertzel_envelope(samples: &[f32], rate: u32, carrier_hz: f32, hop_samples: usize) -> (Vec<f32>, f32) {
    let win = hop_samples;
    let omega = 2.0 * PI * carrier_hz / rate as f32;
    let coeff = 2.0 * omega.cos();
    let mut out = Vec::with_capacity(samples.len() / hop_samples);
    let mut i = 0;
    while i + win <= samples.len() {
        let mut q1 = 0.0;
        let mut q2 = 0.0;
        for &x in &samples[i..i + win] {
            let q0 = coeff * q1 - q2 + x;
            q2 = q1;
            q1 = q0;
        }
        let mag2 = q1 * q1 + q2 * q2 - q1 * q2 * coeff;
        out.push(mag2.max(0.0).sqrt());
        i += hop_samples;
    }
    let hop_seconds = hop_samples as f32 / rate as f32;
    (out, hop_seconds)
}

/// Otsu's threshold for a 1-D float series (positive, scaled to 0..255).
fn otsu_threshold(env: &[f32]) -> f32 {
    let max_v = env.iter().cloned().fold(0.0f32, f32::max).max(1e-9);
    let bins = 256;
    let mut hist = vec![0u32; bins];
    for &v in env {
        let b = ((v / max_v) * (bins as f32 - 1.0)).clamp(0.0, bins as f32 - 1.0) as usize;
        hist[b] += 1;
    }
    let total: u32 = env.len() as u32;
    let sum: f32 = (0..bins).map(|i| i as f32 * hist[i] as f32).sum();
    let mut sum_b = 0.0f32;
    let mut w_b = 0u32;
    let mut max_var = 0.0f32;
    let mut thr_bin = 0usize;
    for i in 0..bins {
        w_b += hist[i];
        if w_b == 0 { continue; }
        let w_f = total - w_b;
        if w_f == 0 { break; }
        sum_b += i as f32 * hist[i] as f32;
        let m_b = sum_b / w_b as f32;
        let m_f = (sum - sum_b) / w_f as f32;
        let var = (w_b as f32) * (w_f as f32) * (m_b - m_f).powi(2);
        if var > max_var {
            max_var = var;
            thr_bin = i;
        }
    }
    (thr_bin as f32 / (bins as f32 - 1.0)) * max_v
}

/// Run-length encode ON intervals from a thresholded envelope.
/// Returns (on_durations_seconds, off_durations_seconds).
fn rle(env: &[f32], thr: f32, hop_seconds: f32, min_run: usize) -> (Vec<f32>, Vec<f32>) {
    let mut on_runs = Vec::new();
    let mut off_runs = Vec::new();
    let mut i = 0;
    let n = env.len();
    while i < n {
        let on = env[i] >= thr;
        let mut j = i;
        while j < n && (env[j] >= thr) == on { j += 1; }
        let run = j - i;
        if run >= min_run {
            let dur = run as f32 * hop_seconds;
            if on { on_runs.push(dur); } else { off_runs.push(dur); }
        }
        i = j;
    }
    (on_runs, off_runs)
}

/// Simple 1-D 2-means on durations, returns (low_centroid, high_centroid).
fn kmeans2(data: &[f32]) -> (f32, f32) {
    if data.len() < 2 { return (0.0, 0.0); }
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut c_lo = sorted[sorted.len() / 8];
    let mut c_hi = sorted[sorted.len() * 7 / 8];
    for _ in 0..50 {
        let mut sum_lo = 0.0;
        let mut n_lo = 0;
        let mut sum_hi = 0.0;
        let mut n_hi = 0;
        for &d in data {
            if (d - c_lo).abs() <= (d - c_hi).abs() {
                sum_lo += d;
                n_lo += 1;
            } else {
                sum_hi += d;
                n_hi += 1;
            }
        }
        let new_lo = if n_lo > 0 { sum_lo / n_lo as f32 } else { c_lo };
        let new_hi = if n_hi > 0 { sum_hi / n_hi as f32 } else { c_hi };
        if (new_lo - c_lo).abs() < 1e-6 && (new_hi - c_hi).abs() < 1e-6 {
            c_lo = new_lo;
            c_hi = new_hi;
            break;
        }
        c_lo = new_lo;
        c_hi = new_hi;
    }
    (c_lo, c_hi)
}

fn median(data: &[f32]) -> f32 {
    if data.is_empty() { return 0.0; }
    let mut s = data.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[s.len() / 2]
}

/// Histogram-mode dot peak: 200 bins from 0..400 ms, find local maxima,
/// return position of the LOWEST significant peak.
fn histogram_dot_peak(on_durations: &[f32]) -> f32 {
    let bins = 200;
    let max_s = 0.400;
    let bin_w = max_s / bins as f32;
    let mut hist = vec![0u32; bins];
    for &d in on_durations {
        if d < max_s {
            let b = (d / bin_w) as usize;
            if b < bins { hist[b] += 1; }
        }
    }
    // 5-bin moving average smoothing
    let mut smooth = vec![0.0f32; bins];
    for i in 0..bins {
        let lo = i.saturating_sub(2);
        let hi = (i + 3).min(bins);
        let s: u32 = hist[lo..hi].iter().sum();
        smooth[i] = s as f32 / (hi - lo) as f32;
    }
    // Find peaks
    let global_max = smooth.iter().cloned().fold(0.0f32, f32::max);
    let mut peaks = Vec::new();
    for i in 2..bins - 2 {
        if smooth[i] > smooth[i - 1] && smooth[i] >= smooth[i + 1]
            && smooth[i] >= 0.15 * global_max
        {
            peaks.push((i, smooth[i]));
        }
    }
    if peaks.is_empty() { return median(on_durations); }
    // The lowest peak in time = dot peak
    let dot_bin = peaks[0].0;
    (dot_bin as f32 + 0.5) * bin_w
}

fn pct(data: &[f32], p: f32) -> f32 {
    if data.is_empty() { return 0.0; }
    let mut s = data.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = ((s.len() as f32 - 1.0) * p).round() as usize;
    s[idx.min(s.len() - 1)]
}

fn main() {
    let path = env::args().nth(1).expect("usage: wpm-measure <wav>");
    let (samples, rate) = load_wav(&path);
    let dur_s = samples.len() as f32 / rate as f32;

    println!("=== WPM measurement: {} ===", path);
    println!("audio: {:.2}s @ {} Hz, {} samples", dur_s, rate, samples.len());

    let carrier = estimate_carrier(&samples, rate);
    println!("carrier: {:.0} Hz", carrier);

    // Use ~5 ms hop for tight resolution at 30+ WPM (dot ~40ms => ~8 hops/dot)
    let hop = ((rate as f32 * 0.005) as usize).max(8);
    let (env, hop_s) = goertzel_envelope(&samples, rate, carrier, hop);
    println!("envelope: hop={:.2}ms, {} frames", hop_s * 1000.0, env.len());

    let thr = otsu_threshold(&env);
    let max_e = env.iter().cloned().fold(0.0f32, f32::max);
    println!("otsu threshold: {:.3} (envelope max {:.3}, ratio {:.2})", thr, max_e, thr / max_e);

    // Require 2 frames minimum to count an edge (10 ms @ hop=5ms -> avoids
    // single-sample flicker but still captures dots at >40 WPM if needed)
    let (on, off) = rle(&env, thr, hop_s, 2);
    println!("RLE: {} on, {} off", on.len(), off.len());

    if on.is_empty() {
        eprintln!("no ON intervals — no signal?");
        std::process::exit(1);
    }

    println!("\n--- ON duration stats (sec) ---");
    println!("  min={:.4} p10={:.4} p25={:.4} median={:.4} p75={:.4} p90={:.4} max={:.4}",
        pct(&on, 0.0), pct(&on, 0.1), pct(&on, 0.25), pct(&on, 0.5),
        pct(&on, 0.75), pct(&on, 0.9), pct(&on, 1.0));

    // M1: k-means(2) on ON durations
    let (c_dot, c_dah) = kmeans2(&on);
    let wpm_m1 = if c_dot > 0.0 { 1.2 / c_dot } else { 0.0 };
    let ratio = if c_dot > 0.0 { c_dah / c_dot } else { 0.0 };
    println!("\n--- M1: k-means(2) ---");
    println!("  dot_centroid = {:.4}s ({:.2} ms)", c_dot, c_dot * 1000.0);
    println!("  dah_centroid = {:.4}s ({:.2} ms)", c_dah, c_dah * 1000.0);
    println!("  dah/dot      = {:.3} (ideal: 3.000)", ratio);
    println!("  WPM (1.2/dot) = {:.2}", wpm_m1);

    // M2: median of lower half of ON
    let med_all = median(&on);
    let lower: Vec<f32> = on.iter().cloned().filter(|&d| d <= med_all).collect();
    let med_dot = if !lower.is_empty() { median(&lower) } else { med_all };
    let wpm_m2 = if med_dot > 0.0 { 1.2 / med_dot } else { 0.0 };
    println!("\n--- M2: median(lower-half ON) ---");
    println!("  median_dot   = {:.4}s ({:.2} ms)", med_dot, med_dot * 1000.0);
    println!("  WPM          = {:.2}", wpm_m2);

    // M3: histogram peak
    let peak_dot = histogram_dot_peak(&on);
    let wpm_m3 = if peak_dot > 0.0 { 1.2 / peak_dot } else { 0.0 };
    println!("\n--- M3: histogram dot peak ---");
    println!("  peak_dot     = {:.4}s ({:.2} ms)", peak_dot, peak_dot * 1000.0);
    println!("  WPM          = {:.2}", wpm_m3);

    // OFF stats
    if !off.is_empty() {
        println!("\n--- OFF duration stats (sec) ---");
        println!("  min={:.4} p25={:.4} median={:.4} p75={:.4} p90={:.4} max={:.4}",
            pct(&off, 0.0), pct(&off, 0.25), pct(&off, 0.5),
            pct(&off, 0.75), pct(&off, 0.9), pct(&off, 1.0));
        let (c_intra, c_inter) = kmeans2(&off);
        println!("  k-means: intra-element={:.4}s, inter-letter/word={:.4}s",
            c_intra, c_inter);
        println!("  intra-element / dot ratio = {:.3} (ideal: 1.000)", c_intra / c_dot.max(1e-6));
    }

    // Mode-rate sanity: average WPM if k-means is right
    println!("\n--- summary ---");
    println!("  M1 (k-means dot)       : {:.2} WPM   dot={:.2} ms", wpm_m1, c_dot * 1000.0);
    println!("  M2 (lower-half median) : {:.2} WPM   dot={:.2} ms", wpm_m2, med_dot * 1000.0);
    println!("  M3 (histogram peak)    : {:.2} WPM   dot={:.2} ms", wpm_m3, peak_dot * 1000.0);
    let mean_wpm = (wpm_m1 + wpm_m2 + wpm_m3) / 3.0;
    println!("  mean of three          : {:.2} WPM", mean_wpm);

    // ===========================================================
    // Decoder pipeline emulation + fix experiments
    // ===========================================================
    println!("\n=== DECODER PIPELINE EMULATION ===");
    // Mirror cw-decoder envelope pipeline: 10ms goertzel window, 5ms hop.
    let frame_len = (rate as f32 * 0.010) as usize;
    let frame_step = (rate as f32 * 0.005) as usize;
    let omega = 2.0 * PI * carrier / rate as f32;
    let coeff = 2.0 * omega.cos();
    let mut env_dec: Vec<f32> = Vec::new();
    let mut i = 0;
    while i + frame_len <= samples.len() {
        let mut q1 = 0.0;
        let mut q2 = 0.0;
        for &x in &samples[i..i + frame_len] {
            let q0 = coeff * q1 - q2 + x;
            q2 = q1;
            q1 = q0;
        }
        env_dec.push((q1 * q1 + q2 * q2 - q1 * q2 * coeff).max(0.0));
        i += frame_step;
    }
    let frame_dt = 0.005;

    // Decoder's percentiles + hysteresis
    let (noise_d, signal_d) = (pct(&env_dec, 0.20), pct(&env_dec, 0.90));
    let span_d = (signal_d - noise_d).max(1e-9);
    println!("envelope: {} frames @ 5ms; noise={:.3} signal={:.3} span={:.3}",
        env_dec.len(), noise_d, signal_d, span_d);

    fn run_hyst(env: &[f32], high: f32, low: f32, frame_dt: f32, min_dur: f32) -> Vec<f32> {
        let mut on_runs = Vec::new();
        let mut keyed = false;
        let mut start = 0usize;
        for (i, &v) in env.iter().enumerate() {
            if keyed {
                if v < low {
                    let dur = (i - start) as f32 * frame_dt;
                    if dur >= min_dur { on_runs.push(dur); }
                    keyed = false;
                }
            } else if v > high {
                keyed = true;
                start = i;
            }
        }
        on_runs
    }

    let configs: &[(&str, f32, f32)] = &[
        ("decoder current  HIGH=0.55 LOW=0.35", 0.55, 0.35),
        ("symmetric 50/50  HIGH=0.50 LOW=0.50", 0.50, 0.50),
        ("centered 60/40   HIGH=0.60 LOW=0.40", 0.60, 0.40),
        ("centered 65/45   HIGH=0.65 LOW=0.45", 0.65, 0.45),
        ("centered 70/50   HIGH=0.70 LOW=0.50", 0.70, 0.50),
        ("symmetric 50     HIGH=0.50 LOW=0.50", 0.50, 0.50),
        ("symmetric 60     HIGH=0.60 LOW=0.60", 0.60, 0.60),
    ];
    println!("\n{:<42} | dot(ms) | dah(ms) | ratio | WPM   | n_on", "config");
    println!("{}", "-".repeat(95));
    for &(name, h, l) in configs {
        let high_t = noise_d + h * span_d;
        let low_t  = noise_d + l * span_d;
        let on_runs = run_hyst(&env_dec, high_t, low_t, frame_dt, 0.012);
        if on_runs.len() < 4 { continue; }
        let (cd, ch) = kmeans2(&on_runs);
        let wpm_x = if cd > 0.0 { 1.2 / cd } else { 0.0 };
        let ratio = if cd > 0.0 { ch / cd } else { 0.0 };
        println!("{:<42} | {:7.2} | {:7.2} | {:5.2} | {:5.2} | {}",
            name, cd * 1000.0, ch * 1000.0, ratio, wpm_x, on_runs.len());
    }

    // ===========================================================
    // Alternative fixes: midpoint correction & subframe interpolation
    // ===========================================================
    println!("\n=== ALTERNATIVE: midpoint estimation (use rise+fall midpoints) ===");
    fn run_midpoint(env: &[f32], high: f32, low: f32, frame_dt: f32, min_dur: f32) -> Vec<f32> {
        // Find HIGH-up-cross and LOW-down-cross frames, but compute duration
        // as time between MIDPOINT of (HIGH up-cross frame, LOW up-cross
        // frame backward search) on rise and (HIGH down-cross frame, LOW
        // down-cross frame) on fall. This gives the half-amplitude crossing
        // estimate.
        let mid = (high + low) * 0.5;
        let mut on_runs = Vec::new();
        let mut keyed = false;
        let mut start_mid = 0usize;
        for (i, &v) in env.iter().enumerate() {
            if keyed {
                if v < low {
                    // walk back to find first frame >= mid (downcross)
                    let mut j = i;
                    while j > 0 && env[j - 1] < mid { j -= 1; }
                    let dur = (j - start_mid) as f32 * frame_dt;
                    if dur >= min_dur { on_runs.push(dur); }
                    keyed = false;
                }
            } else if v > high {
                keyed = true;
                // walk back to find first frame <= mid (upcross)
                let mut j = i;
                while j > 0 && env[j - 1] >= mid { j -= 1; }
                start_mid = j;
            }
        }
        on_runs
    }
    let high_t = noise_d + 0.55 * span_d;
    let low_t  = noise_d + 0.35 * span_d;
    let on_mp = run_midpoint(&env_dec, high_t, low_t, frame_dt, 0.012);
    let (cd_mp, ch_mp) = kmeans2(&on_mp);
    println!("  midpoint (HIGH=0.55 LOW=0.35): dot={:.2}ms dah={:.2}ms ratio={:.2} WPM={:.2}",
        cd_mp * 1000.0, ch_mp * 1000.0, ch_mp / cd_mp.max(1e-6), 1.2 / cd_mp.max(1e-6));

    // Use period-based estimator: total cycles / total elements gives baud-rate
    println!("\n=== ALTERNATIVE: period-based (rising-edge to rising-edge) ===");
    // For PARIS-style: period of dot+gap = 2 dots. Median rising-edge interval
    // among the SHORTEST class corresponds to (dot + intra-element gap) = 2 dot.
    let high_t = noise_d + 0.55 * span_d;
    let low_t  = noise_d + 0.35 * span_d;
    let mut keyed = false;
    let mut rising_edges = Vec::new();
    for (i, &v) in env_dec.iter().enumerate() {
        if keyed {
            if v < low_t { keyed = false; }
        } else if v > high_t {
            keyed = true;
            rising_edges.push(i as f32 * frame_dt);
        }
    }
    let mut intervals: Vec<f32> = rising_edges.windows(2).map(|w| w[1] - w[0]).collect();
    intervals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if intervals.len() > 4 {
        // median of shortest 1/3 = (dot + intra-gap) ≈ 2*dot
        let third = intervals.len() / 3;
        let short_med = intervals[third / 2];
        let dot_period = short_med / 2.0;
        println!("  shortest-third median rising-edge interval = {:.2}ms",
            short_med * 1000.0);
        println!("  inferred dot+gap period       = {:.2}ms each (dot ~= {:.2}ms)",
            short_med * 1000.0, dot_period * 1000.0);
        println!("  WPM = 1.2 / {:.4}s = {:.2}", dot_period, 1.2 / dot_period.max(1e-6));
    }

    // ===========================================================
    // SHORT-WINDOW SIMULATION: replicate the streaming decoder
    // which only sees a 3-second analysis window with ~10-20 events
    // ===========================================================
    println!("\n=== SHORT-WINDOW (3s) SIMULATION ===");
    let win_frames = (3.0 / frame_dt) as usize;
    let step_frames = (1.0 / frame_dt) as usize;
    let mut dots_kmeans = Vec::new();
    let mut dots_period = Vec::new();
    let mut dots_p10 = Vec::new();
    let mut start = 0;
    while start + win_frames <= env_dec.len() {
        let win = &env_dec[start..start + win_frames];
        let (n, s) = (pct(win, 0.20), pct(win, 0.90));
        let sp = (s - n).max(1e-9);
        let h = n + 0.55 * sp;
        let l = n + 0.35 * sp;
        let on_runs = run_hyst(win, h, l, frame_dt, 0.012);
        if on_runs.len() >= 4 {
            let (cd, _) = kmeans2(&on_runs);
            dots_kmeans.push(cd);
            dots_p10.push(pct(&on_runs, 0.10));
            let mut keyed = false;
            let mut edges = Vec::new();
            for (i, &v) in win.iter().enumerate() {
                if keyed { if v < l { keyed = false; } }
                else if v > h { keyed = true; edges.push(i as f32 * frame_dt); }
            }
            if edges.len() >= 3 {
                let mut ints: Vec<f32> = edges.windows(2).map(|w| w[1] - w[0]).collect();
                ints.sort_by(|a, b| a.partial_cmp(b).unwrap());
                dots_period.push(ints[ints.len() / 5] / 2.0);
            }
        }
        start += step_frames;
    }
    fn summarize(name: &str, dots: &[f32]) {
        if dots.is_empty() { println!("  {}: no data", name); return; }
        let mut s = dots.to_vec();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let med = s[s.len() / 2];
        let p25 = s[s.len() / 4];
        let p75 = s[s.len() * 3 / 4];
        println!("  {:<25}: n={:3}  dot p25={:5.1}ms med={:5.1}ms p75={:5.1}ms  WPM(med)={:.2}",
            name, s.len(), p25 * 1000.0, med * 1000.0, p75 * 1000.0, 1.2 / med.max(1e-6));
    }
    summarize("k-means (current)", &dots_kmeans);
    summarize("p10-of-ON (alt)", &dots_p10);
    summarize("period/2 (rising-edge)", &dots_period);
}

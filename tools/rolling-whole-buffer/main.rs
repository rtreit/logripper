// Simulate "live" by decoding growing prefixes every N seconds, but report
// each decode as a replacement transcript (not an incremental commit).
// Measures: per-decode CER, per-decode latency, final CER.
use hound::{SampleFormat, WavReader};
use std::time::Instant;

fn lev(a: &str, b: &str) -> usize {
    let av: Vec<char> = a.chars().collect();
    let bv: Vec<char> = b.chars().collect();
    let mut m = vec![vec![0usize; bv.len() + 1]; av.len() + 1];
    for i in 0..=av.len() { m[i][0] = i; }
    for j in 0..=bv.len() { m[0][j] = j; }
    for i in 1..=av.len() {
        for j in 1..=bv.len() {
            let c = if av[i - 1] == bv[j - 1] { 0 } else { 1 };
            m[i][j] = (m[i - 1][j] + 1).min(m[i][j - 1] + 1).min(m[i - 1][j - 1] + c);
        }
    }
    m[av.len()][bv.len()]
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = &args[1];
    let truth = &args[2];
    let step_s: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(5.0);
    let mut r = WavReader::open(path).unwrap();
    let spec = r.spec();
    let rate = spec.sample_rate;
    let samples_f32: Vec<f32> = if spec.sample_format == SampleFormat::Int {
        r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
    } else {
        r.samples::<f32>().map(|s| s.unwrap()).collect()
    };
    let total_s = samples_f32.len() as f32 / rate as f32;
    let truth_n = norm(truth);
    let truth_len = truth_n.chars().count();
    println!("audio={:.1}s rate={} step={:.1}s truth_chars={}", total_s, rate, step_s, truth_len);

    let mut t = step_s;
    while t <= total_s + 0.1 {
        let n = ((t * rate as f32) as usize).min(samples_f32.len());
        let prefix = &samples_f32[..n];
        let start = Instant::now();
        let result = ditdah::decode_samples(prefix, rate).unwrap_or_default();
        let elapsed_ms = start.elapsed().as_millis();
        let r_n = norm(&result);
        let dist = lev(&truth_n, &r_n);
        let cer = dist as f32 / truth_len.max(1) as f32;
        println!("t={:6.1}s  decode={:5}ms  out_chars={:4}  CER={:.3}  dist={:4}", t, elapsed_ms, r_n.chars().count(), cer, dist);
        t += step_s;
    }
}

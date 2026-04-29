use hound::{SampleFormat, WavReader};

fn main() {
    let path = std::env::args().nth(1).expect("usage: probe <wav> [step_s] [mode]");
    let step_s: f32 = std::env::args().nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10.0);
    // mode: "auto" (re-fit each time), "pin-wpm" (pin wpm only),
    // "pin-both" (pin wpm+threshold from first stable detection).
    let mode: String = std::env::args().nth(3).unwrap_or_else(|| "pin-both".to_string());

    let mut r = WavReader::open(&path).unwrap();
    let spec = r.spec();
    let rate = spec.sample_rate;
    let ch = spec.channels as usize;
    let samples_f32: Vec<f32> = if spec.sample_format == SampleFormat::Int {
        r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
    } else {
        r.samples::<f32>().map(|s| s.unwrap()).collect()
    };
    let mono: Vec<f32> = if ch > 1 {
        samples_f32.chunks_exact(ch).map(|c| c.iter().sum::<f32>() / ch as f32).collect()
    } else {
        samples_f32
    };
    let total_s = mono.len() as f32 / rate as f32;
    println!("MODE = {}", mode);

    let mut t = step_s;
    let mut prev = String::new();
    let mut pinned_wpm: Option<f32> = None;
    let mut pinned_thr: Option<f32> = None;
    while t <= total_s + 0.1 {
        let n = ((t * rate as f32) as usize).min(mono.len());
        let prefix = &mono[..n];
        let (pw, pt) = match mode.as_str() {
            "auto" => (None, None),
            "pin-wpm" => (pinned_wpm, None),
            "pin-both" => (pinned_wpm, pinned_thr),
            _ => (None, None),
        };
        let (result, wpm, thr) = ditdah::decode_samples_with_params(prefix, rate, pw, pt)
            .unwrap_or_else(|_| (String::new(), 0.0, 0.0));
        // Pin after first successful decode.
        if pinned_wpm.is_none() && wpm > 0.0 {
            pinned_wpm = Some(wpm);
            pinned_thr = Some(thr);
        }
        let result = result.replace('\n', " ");
        let new_tail = if result.starts_with(&prev) {
            result[prev.len()..].trim().to_string()
        } else {
            format!("[NON-PREFIX shrink:{}->{}]", prev.len(), result.len())
        };
        println!("t={:6.1}s wpm={:5.1} thr={:.2e} len={:4} new=[{}]", t, wpm, thr, result.len(), new_tail);
        if new_tail.starts_with("[NON-PREFIX") {
            println!("    full=[{}]", &result);
        }
        prev = result;
        t += step_s;
    }
    println!("\nFINAL: {}", prev);
}

use std::env;
fn main() {
    let path = env::args().nth(1).expect("path");
    let wpm: f32 = env::args().nth(2).map(|s| s.parse().unwrap()).unwrap_or(20.0);
    let mut r = hound::WavReader::open(&path).unwrap();
    let spec = r.spec();
    let samples: Vec<f32> = if spec.sample_format == hound::SampleFormat::Int {
        r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
    } else {
        r.samples::<f32>().map(|s| s.unwrap()).collect()
    };
    println!("=== auto WPM ===");
    let (txt, w, t) = ditdah::decode_samples_with_params(&samples, spec.sample_rate, None, None).unwrap();
    println!("wpm={:.1} thr={:.3}\n{}", w, t, txt);
    for w_pin in [15.0_f32, 18.0, 20.0, 22.0, 25.0, 28.0, wpm] {
        let (txt, w, t) = ditdah::decode_samples_with_params(&samples, spec.sample_rate, Some(w_pin), None).unwrap();
        println!("\n=== pin_wpm={:.0} (used wpm={:.1} thr={:.3}) ===\n{}", w_pin, w, t, txt);
    }
}

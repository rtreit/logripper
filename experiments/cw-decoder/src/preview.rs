use std::path::Path;

use anyhow::{Context, Result};
use hound::{SampleFormat, WavSpec, WavWriter};

use crate::audio;

pub fn render_preview_wav(
    path: &Path,
    start_s: f32,
    window_s: f32,
    slowdown: f32,
    padding_ms: u32,
    output: &Path,
) -> Result<()> {
    let audio = audio::decode_file(path).with_context(|| format!("decoding {}", path.display()))?;
    let start_s = start_s.max(0.0);
    let window_s = window_s.max(0.05);
    let slowdown = slowdown.max(1.0);
    let padding_samples = ((audio.sample_rate as f32 * padding_ms as f32) / 1000.0) as usize;

    let start = (start_s * audio.sample_rate as f32) as usize;
    let len = (window_s * audio.sample_rate as f32) as usize;
    let end = start.saturating_add(len).min(audio.samples.len());
    let slice = if start < end {
        &audio.samples[start..end]
    } else {
        &[]
    };

    let stretched = stretch(slice, slowdown);
    let mut padded = Vec::with_capacity(stretched.len() + padding_samples * 2);
    padded.extend(std::iter::repeat_n(0.0, padding_samples));
    padded.extend(stretched);
    padded.extend(std::iter::repeat_n(0.0, padding_samples));

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let spec = WavSpec {
        channels: 1,
        sample_rate: audio.sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(output, spec)
        .with_context(|| format!("creating {}", output.display()))?;
    for sample in padded {
        let clamped = sample.clamp(-1.0, 1.0);
        let pcm = (clamped * i16::MAX as f32) as i16;
        writer.write_sample(pcm)?;
    }
    writer.finalize()?;
    Ok(())
}

fn stretch(samples: &[f32], slowdown: f32) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }

    let last = samples.len() - 1;
    let out_len = ((samples.len() as f32) * slowdown).ceil() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f32 / slowdown;
        // Clamp both indices: with large sample counts the f32 conversion of `i`
        // loses precision (mantissa is 24 bits) and `floor()` can land at or past
        // `samples.len()`, so we must guard `lo` not just `hi`.
        let lo = (src.floor() as usize).min(last);
        let hi = (lo + 1).min(last);
        let frac = (src - lo as f32).clamp(0.0, 1.0);
        let interp = samples[lo] * (1.0 - frac) + samples[hi] * frac;
        out.push(interp);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::stretch;

    #[test]
    fn stretch_expands_sample_count() {
        let input = [0.0_f32, 1.0, -1.0, 0.5];
        let out = stretch(&input, 2.5);
        assert!(out.len() >= 10);
        assert!(out.iter().all(|v| (-1.0..=1.0).contains(v)));
    }

    #[test]
    fn stretch_handles_large_inputs_without_panic() {
        // f32 mantissa is 24 bits so indices > ~16M lose precision.
        // Build an input large enough that `i as f32 / slowdown` can
        // round to `samples.len()` and exercise the lo/hi clamp.
        let len: usize = 14_233_578;
        let input = vec![0.25_f32; len];
        let out = stretch(&input, 2.5);
        assert!(!out.is_empty());
        // No panic = success; spot-check the tail value is finite.
        assert!(out.last().copied().unwrap_or(f32::NAN).is_finite());
    }
}

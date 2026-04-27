// src/lib.rs
// Library interface for ditdah

mod decoder;
pub mod generator;

use decoder::MorseDecoder;
pub use generator::MorseGenerator;

use anyhow::Result;

/// High-level convenience function to decode a WAV file directly
///
/// # Example
/// ```no_run
/// use ditdah::decode_wav_file;
///
/// let decoded_text = decode_wav_file("morse.wav").unwrap();
/// println!("Decoded: {}", decoded_text);
/// ```
pub fn decode_wav_file<P: AsRef<std::path::Path>>(path: P) -> Result<String> {
    use hound::{SampleFormat, WavReader};

    let mut reader = WavReader::open(path)?;
    let spec = reader.spec();

    // Check supported formats
    if spec.sample_format != SampleFormat::Int && spec.sample_format != SampleFormat::Float {
        anyhow::bail!(
            "Unsupported sample format: {:?}. Only 16-bit Int and 32-bit Float are supported.",
            spec.sample_format
        );
    }

    // Create decoder with automatic sample rate conversion
    let mut decoder = MorseDecoder::new(spec.sample_rate, 12000)?;

    // Read all samples
    let samples_f32: Vec<f32> = if spec.sample_format == SampleFormat::Int {
        reader
            .samples::<i16>()
            .map(|s| s.unwrap() as f32 / 32768.0)
            .collect()
    } else {
        reader.samples::<f32>().map(|s| s.unwrap()).collect()
    };

    // Convert to mono if necessary
    let mono_samples: Vec<f32> = if spec.channels > 1 {
        samples_f32
            .chunks_exact(spec.channels as usize)
            .map(|chunk| chunk.iter().sum::<f32>() / spec.channels as f32)
            .collect()
    } else {
        samples_f32
    };

    // Process in chunks for better memory efficiency
    const CHUNK_SIZE: usize = 4096;
    for chunk in mono_samples.chunks(CHUNK_SIZE) {
        decoder.process(chunk)?;
    }

    decoder.finalize()
}

/// High-level convenience function to decode audio samples directly
///
/// # Example
/// ```no_run
/// use ditdah::decode_samples;
///
/// // For real audio data with sufficient length for processing
/// let samples = vec![0.0; 48000]; // 4 seconds of audio at 12kHz
/// let decoded_text = decode_samples(&samples, 12000).unwrap();
/// println!("Decoded: {}", decoded_text);
/// ```
pub fn decode_samples(samples: &[f32], sample_rate: u32) -> Result<String> {
    let mut decoder = MorseDecoder::new(sample_rate, 12000)?;

    const CHUNK_SIZE: usize = 4096;
    for chunk in samples.chunks(CHUNK_SIZE) {
        decoder.process(chunk)?;
    }

    decoder.finalize()
}

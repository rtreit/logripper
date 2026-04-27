# ditdah - Morse Code Decoder

[![CI](https://github.com/yuvadm/ditdah/workflows/CI/badge.svg)](https://github.com/yuvadm/ditdah/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A high-performance Rust implementation of a Morse code decoder that can process WAV audio files and decode them into text with **100% accuracy** on the comprehensive test suite.

## Features

- **High Accuracy**: Achieves 100% pass rate on comprehensive test suite
- **Clean Library API**: High-level functions for easy integration (`decode_wav_file`, `decode_samples`)
- **Full-Featured CLI**: Decode files, generate test audio, verbose output, timing information
- **Audio Processing**: Supports WAV files with various sample rates (12kHz, 44.1kHz) and formats
- **Signal Processing**: Uses FFT-based pitch detection, Goertzel filtering, and adaptive threshold detection
- **Self-Calibrating**: Automatically determines timing, WPM, and optimal thresholds
- **Robust Decoding**: Handles uniform dot/dash sequences and complex multi-letter words
- **Comprehensive Testing**: Built-in test suite with Morse code generator for validation

## Performance

✅ **All tests passing with 100% accuracy:**
- Basic signals (SOS, HELLO WORLD)
- Full alphabet (A-Z)
- Numbers (0-9)
- Different frequencies (300Hz - 1000Hz)
- Variable speeds (10-30 WPM)
- Different sample rates (12kHz, 44.1kHz)
- Complex content (CQ DE W1AW)

## Installation

```bash
git clone https://github.com/yuvadm/ditdah
cd ditdah
cargo build --release
```

## Usage

### Command Line Interface

**Decode a WAV file:**
```bash
cargo run -- input.wav
```

**With verbose output and timing:**
```bash
cargo run -- input.wav --verbose --time
```

**Generate test Morse code WAV files:**
```bash
cargo run -- --generate "SOS" --verbose
cargo run -- --generate "HELLO WORLD" --output test.wav --frequency 800 --wpm 25
```

**With debug logging:**
```bash
RUST_LOG=info cargo run -- input.wav
```

### Library Usage

**High-level API (recommended):**
```rust
use ditdah::{decode_wav_file, decode_samples, MorseGenerator};

// Decode a WAV file directly
let decoded_text = decode_wav_file("morse.wav")?;
println!("Decoded: {}", decoded_text);

// Decode audio samples directly
let samples: Vec<f32> = /* your audio data */;
let decoded_text = decode_samples(&samples, 12000)?;
println!("Decoded: {}", decoded_text);

// Generate Morse code WAV files
let generator = MorseGenerator::new(12000, 600.0, 20.0);
generator.generate_wav_file("SOS", "output.wav")?;
```

## Testing

### Run All Tests

```bash
cargo test
```

### Baseline Tests (Quick Verification)

```bash
# Basic test
cargo test baseline_decoder_test -- --nocapture

# With debug output
RUST_LOG=info cargo test baseline_decoder_test -- --nocapture
```

### Comprehensive Test Suite

```bash
cargo test run_comprehensive_test_suite -- --nocapture
```

The test suite automatically:
- Generates test WAV files with known Morse content
- Decodes them using the library
- Measures accuracy and reports results
- Cleans up temporary files automatically

## Algorithm

The decoder uses a sophisticated multi-stage approach:

1. **Audio Preprocessing**: Resampling, bandpass filtering (200Hz-1200Hz)
2. **Pitch Detection**: STFT-based frequency analysis
3. **Signal Extraction**: Goertzel filtering tuned to detected frequency
4. **Self-Calibration**: Intelligent timing analysis for dots vs dashes
5. **Letter Boundary Detection**: Proper gap analysis for multi-letter words
6. **Character Assembly**: Morse pattern to text conversion

### Key Innovations

- **Self-calibrating timing**: Handles both uniform sequences (EEEE, TTTT) and mixed patterns
- **Adaptive gap detection**: Distinguishes element gaps, letter gaps, and word gaps
- **Robust parameter estimation**: Works across different speeds and frequencies

## Library API

The library provides a clean, high-level API:

```rust
pub fn decode_wav_file<P: AsRef<std::path::Path>>(path: P) -> Result<String>
pub fn decode_samples(samples: &[f32], sample_rate: u32) -> Result<String>
pub use generator::MorseGenerator;
```

**All signal processing complexity is handled internally** - the library automatically:
- Detects audio format and converts to the required sample rate
- Performs frequency analysis and filtering
- Calibrates timing parameters
- Decodes Morse patterns to text

## Project Structure

```
ditdah/
├── src/
│   ├── main.rs          # CLI application
│   ├── lib.rs           # Public library API
│   ├── decoder.rs       # Internal Morse decoder implementation
│   └── generator.rs     # Public Morse code generator
├── tests/
│   └── integration_tests.rs  # Comprehensive test suite
├── .github/workflows/   # CI pipeline
├── Cargo.toml           # Rust 2024 edition project configuration
├── LICENSE              # MIT License
└── README.md           # This file
```

## Attribution

This implementation is based on the excellent work from [ggmorse](https://github.com/ggerganov/ggmorse) by Georgi Gerganov, which provided inspiration for the signal processing pipeline. The Rust implementation includes significant enhancements for robustness and accuracy.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Contributing

1. Run the test suite to verify functionality: `cargo test`
2. All tests should pass with 100% accuracy
3. Add tests for new features or edge cases
4. Ensure code is properly formatted: `cargo fmt`
5. Run clippy for additional checks: `cargo clippy`

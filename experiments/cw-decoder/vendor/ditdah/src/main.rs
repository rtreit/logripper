use anyhow::{Result, bail};
use clap::Parser;
use std::path::PathBuf;
use std::time::Instant;

mod generator;

#[derive(Parser)]
#[command(
    name = "ditdah",
    version,
    about = "High-performance Morse code decoder",
    long_about = "A high-performance Rust implementation of a Morse code decoder that can process WAV audio files and decode them into text with 100% accuracy on the comprehensive test suite."
)]
struct Cli {
    /// Path to the input WAV file
    #[arg(value_name = "FILE", help = "WAV file containing Morse code audio")]
    wav_file: Option<PathBuf>,

    /// Show detailed processing information
    #[arg(short, long, help = "Enable verbose output")]
    verbose: bool,

    /// Show timing information
    #[arg(short, long, help = "Show processing time")]
    time: bool,

    /// Generate a test WAV file instead of decoding
    #[arg(
        long,
        value_name = "TEXT",
        help = "Generate test WAV file with given text"
    )]
    generate: Option<String>,

    /// Output file for generation (default: output.wav)
    #[arg(
        short,
        long,
        value_name = "FILE",
        help = "Output file for generated WAV"
    )]
    output: Option<PathBuf>,

    /// Frequency for generated audio (default: 600 Hz)
    #[arg(
        long,
        value_name = "HZ",
        default_value = "600",
        help = "Frequency in Hz for generated audio"
    )]
    frequency: f32,

    /// Words per minute for generated audio (default: 20 WPM)
    #[arg(
        long,
        value_name = "WPM",
        default_value = "20",
        help = "Words per minute for generated audio"
    )]
    wpm: f32,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set up logging - user can control with RUST_LOG environment variable
    env_logger::try_init().ok();

    // Handle generation mode
    if let Some(text) = &cli.generate {
        return generate_wav_file(&cli, text);
    }

    // Validate we have an input file for decoding
    let wav_file = cli.wav_file.as_ref().ok_or_else(|| {
        anyhow::anyhow!("No input file specified. Use --help for usage information.")
    })?;

    // Validate input file exists
    if !wav_file.exists() {
        bail!("File not found: {}", wav_file.display());
    }

    let start_time = Instant::now();

    if cli.verbose {
        println!("Processing: {}", wav_file.display());
    }

    // Use the new high-level API
    let decoded_text = ditdah::decode_wav_file(wav_file).map_err(|e| {
        // Provide user-friendly error messages
        match e.to_string().as_str() {
            s if s.contains("No such file") => {
                anyhow::anyhow!("File not found: {}", wav_file.display())
            }
            s if s.contains("Unsupported sample format") => {
                anyhow::anyhow!("Unsupported audio format. Please use 16-bit or 32-bit WAV files.")
            }
            s if s.contains("Could not find a dominant frequency") => anyhow::anyhow!(
                "No Morse code signal detected. Check that the file contains clear Morse audio."
            ),
            _ => e,
        }
    })?;

    let duration = start_time.elapsed();

    // Output results
    if decoded_text.is_empty() {
        println!("No Morse code detected");
        if !cli.verbose {
            println!("Try using --verbose flag for detailed processing information");
        }
    } else {
        println!("Decoded: {}", decoded_text);

        if cli.time {
            println!("Processing time: {:.2?}", duration);
        }

        if cli.verbose {
            println!("Length: {} characters", decoded_text.len());
            if decoded_text.len() > 50 {
                println!("Text: {}...", &decoded_text[..50]);
            }
        }
    }

    Ok(())
}

fn generate_wav_file(cli: &Cli, text: &str) -> Result<()> {
    use generator::MorseGenerator;

    let output_path = cli
        .output
        .clone()
        .unwrap_or_else(|| PathBuf::from("output.wav"));

    if cli.verbose {
        println!(
            "Generating: '{}' at {} Hz, {} WPM",
            text, cli.frequency, cli.wpm
        );
        println!("Output: {}", output_path.display());
    }

    let generator = MorseGenerator::new(12000, cli.frequency, cli.wpm);
    generator.generate_wav_file(text, &output_path)?;

    println!("Generated: {}", output_path.display());

    if cli.verbose {
        println!(
            "Settings: {} Hz, {} WPM, 12kHz sample rate",
            cli.frequency, cli.wpm
        );

        // Show what the Morse pattern looks like
        let morse_pattern = text_to_morse_pattern(text);
        if !morse_pattern.is_empty() {
            println!("Morse: {}", morse_pattern);
        }
    }

    Ok(())
}

fn text_to_morse_pattern(text: &str) -> String {
    let morse_map = [
        ('A', ".-"),
        ('B', "-..."),
        ('C', "-.-."),
        ('D', "-.."),
        ('E', "."),
        ('F', "..-."),
        ('G', "--."),
        ('H', "...."),
        ('I', ".."),
        ('J', ".---"),
        ('K', "-.-"),
        ('L', ".-.."),
        ('M', "--"),
        ('N', "-."),
        ('O', "---"),
        ('P', ".--."),
        ('Q', "--.-"),
        ('R', ".-."),
        ('S', "..."),
        ('T', "-"),
        ('U', "..-"),
        ('V', "...-"),
        ('W', ".--"),
        ('X', "-..-"),
        ('Y', "-.--"),
        ('Z', "--.."),
        ('1', ".----"),
        ('2', "..---"),
        ('3', "...--"),
        ('4', "....-"),
        ('5', "....."),
        ('6', "-...."),
        ('7', "--..."),
        ('8', "---.."),
        ('9', "----."),
        ('0', "-----"),
    ]
    .into_iter()
    .collect::<std::collections::HashMap<_, _>>();

    text.to_uppercase()
        .chars()
        .filter_map(|c| {
            if c == ' ' {
                Some("  ".to_string())
            } else {
                morse_map.get(&c).map(|&morse| format!("{} ", morse))
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

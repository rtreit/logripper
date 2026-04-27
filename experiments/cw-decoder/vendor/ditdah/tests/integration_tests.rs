// tests/integration_tests.rs
// Comprehensive integration tests for the Morse decoder

use anyhow::Result;
use ditdah::{MorseGenerator, decode_wav_file};
use std::{fs, io::Write};

#[derive(Debug)]
struct TestCase {
    name: &'static str,
    text: &'static str,
    frequency: f32,
    wpm: f32,
    sample_rate: u32,
    expected_accuracy: f32, // Minimum accuracy threshold (0.0 to 1.0)
}

const TEST_CASES: &[TestCase] = &[
    // Basic tests
    TestCase {
        name: "simple_sos",
        text: "SOS",
        frequency: 600.0,
        wpm: 20.0,
        sample_rate: 12000,
        expected_accuracy: 0.8,
    },
    TestCase {
        name: "hello_world",
        text: "HELLO WORLD",
        frequency: 600.0,
        wpm: 20.0,
        sample_rate: 12000,
        expected_accuracy: 0.7,
    },
    TestCase {
        name: "alphabet",
        text: "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        frequency: 600.0,
        wpm: 15.0,
        sample_rate: 12000,
        expected_accuracy: 0.6,
    },
    // Different frequencies
    TestCase {
        name: "low_freq",
        text: "TEST",
        frequency: 300.0,
        wpm: 20.0,
        sample_rate: 12000,
        expected_accuracy: 0.7,
    },
    TestCase {
        name: "high_freq",
        text: "TEST",
        frequency: 1000.0,
        wpm: 20.0,
        sample_rate: 12000,
        expected_accuracy: 0.7,
    },
    // Different WPM speeds
    TestCase {
        name: "slow_wpm",
        text: "SLOW",
        frequency: 600.0,
        wpm: 10.0,
        sample_rate: 12000,
        expected_accuracy: 0.8,
    },
    TestCase {
        name: "fast_wpm",
        text: "FAST",
        frequency: 600.0,
        wpm: 30.0,
        sample_rate: 12000,
        expected_accuracy: 0.6,
    },
    // Numbers
    TestCase {
        name: "numbers",
        text: "12345",
        frequency: 600.0,
        wpm: 20.0,
        sample_rate: 12000,
        expected_accuracy: 0.7,
    },
    // Mixed content
    TestCase {
        name: "mixed",
        text: "CQ DE W1AW",
        frequency: 600.0,
        wpm: 20.0,
        sample_rate: 12000,
        expected_accuracy: 0.6,
    },
    // Different sample rates
    TestCase {
        name: "different_sample_rate",
        text: "RATE",
        frequency: 600.0,
        wpm: 20.0,
        sample_rate: 44100,
        expected_accuracy: 0.7,
    },
];

#[test]
fn run_comprehensive_test_suite() -> Result<()> {
    println!("Running comprehensive Morse decoder test suite...");

    // Create test directory
    fs::create_dir_all("test_outputs")?;

    let mut results = Vec::new();
    let mut total_tests = 0;
    let mut passed_tests = 0;

    // Create a detailed report file
    let mut report_file = fs::File::create("test_outputs/test_report.txt")?;
    writeln!(report_file, "Morse Decoder Test Report")?;
    writeln!(report_file, "=========================")?;
    writeln!(report_file)?;

    for test_case in TEST_CASES {
        total_tests += 1;
        println!("Running test: {}", test_case.name);

        let result = run_single_test(test_case);
        let passed = result.is_ok();
        if passed {
            passed_tests += 1;
        }

        // Log detailed results
        match &result {
            Ok(test_result) => {
                println!(
                    "  ✓ PASSED - Accuracy: {:.1}%",
                    test_result.accuracy * 100.0
                );
                writeln!(
                    report_file,
                    "TEST: {} - PASSED\n  Expected: '{}'\n  Decoded: '{}'\n  Accuracy: {:.1}%\n  WPM: {}, Freq: {}Hz, SR: {}Hz\n",
                    test_case.name,
                    test_case.text,
                    test_result.decoded_text,
                    test_result.accuracy * 100.0,
                    test_case.wpm,
                    test_case.frequency,
                    test_case.sample_rate
                )?;
            }
            Err(e) => {
                println!("  ✗ FAILED - {}", e);
                writeln!(
                    report_file,
                    "TEST: {} - FAILED\n  Expected: '{}'\n  Error: {}\n  WPM: {}, Freq: {}Hz, SR: {}Hz\n",
                    test_case.name,
                    test_case.text,
                    e,
                    test_case.wpm,
                    test_case.frequency,
                    test_case.sample_rate
                )?;
            }
        }

        results.push((test_case, result));
    }

    // Summary
    let pass_rate = (passed_tests as f32 / total_tests as f32) * 100.0;
    println!("\nTest Summary:");
    println!("  Total tests: {}", total_tests);
    println!("  Passed: {}", passed_tests);
    println!("  Failed: {}", total_tests - passed_tests);
    println!("  Pass rate: {:.1}%", pass_rate);

    writeln!(report_file, "\nSUMMARY:")?;
    writeln!(report_file, "  Total tests: {}", total_tests)?;
    writeln!(report_file, "  Passed: {}", passed_tests)?;
    writeln!(report_file, "  Failed: {}", total_tests - passed_tests)?;
    writeln!(report_file, "  Pass rate: {:.1}%", pass_rate)?;

    // Analyze failure patterns
    let failed_tests: Vec<_> = results.iter().filter(|(_, r)| r.is_err()).collect();
    if !failed_tests.is_empty() {
        writeln!(report_file, "\nFAILURE ANALYSIS:")?;
        for (test_case, error) in failed_tests {
            writeln!(
                report_file,
                "  {} - {}",
                test_case.name,
                error.as_ref().unwrap_err()
            )?;
        }
    }

    // Clean up test directory
    std::fs::remove_dir_all("test_outputs").ok();

    // If overall pass rate is too low, fail the test
    if pass_rate < 50.0 {
        panic!("Test suite failed with pass rate of {:.1}%.", pass_rate);
    }

    Ok(())
}

#[derive(Debug)]
struct TestResult {
    decoded_text: String,
    accuracy: f32,
}

fn run_single_test(test_case: &TestCase) -> Result<TestResult> {
    // Generate the test WAV file in a temporary location
    let generator = MorseGenerator::new(test_case.sample_rate, test_case.frequency, test_case.wpm);
    let wav_path = format!("test_outputs/{}.wav", test_case.name);
    generator.generate_wav_file(test_case.text, &wav_path)?;

    // Decode the WAV file
    let decoded_text = decode_test_wav_file(&wav_path)?;

    // Calculate accuracy
    let accuracy = calculate_accuracy(test_case.text, &decoded_text);

    let result = TestResult {
        decoded_text,
        accuracy,
    };

    // Clean up the temporary WAV file
    std::fs::remove_file(&wav_path).ok();

    // Check if accuracy meets threshold
    if accuracy >= test_case.expected_accuracy {
        Ok(result)
    } else {
        Err(anyhow::anyhow!(
            "Accuracy {:.1}% below threshold {:.1}%",
            accuracy * 100.0,
            test_case.expected_accuracy * 100.0
        ))
    }
}

fn decode_test_wav_file(path: &str) -> Result<String> {
    decode_wav_file(path)
}

fn calculate_accuracy(expected: &str, actual: &str) -> f32 {
    if expected.is_empty() {
        return if actual.is_empty() { 1.0 } else { 0.0 };
    }

    let expected_clean = expected.to_uppercase().replace(" ", "");
    let actual_clean = actual.to_uppercase().replace(" ", "").replace("?", "");

    if expected_clean.is_empty() {
        return if actual_clean.is_empty() { 1.0 } else { 0.0 };
    }

    // Simple character-by-character comparison
    let expected_chars: Vec<char> = expected_clean.chars().collect();
    let actual_chars: Vec<char> = actual_clean.chars().collect();

    let max_len = expected_chars.len().max(actual_chars.len());
    let mut matches = 0;

    for i in 0..max_len {
        let expected_char = expected_chars.get(i);
        let actual_char = actual_chars.get(i);

        if expected_char == actual_char {
            matches += 1;
        }
    }

    matches as f32 / max_len as f32
}

#[test]
fn test_accuracy_calculation() {
    assert_eq!(calculate_accuracy("SOS", "SOS"), 1.0);
    assert_eq!(calculate_accuracy("SOS", "SO"), 2.0 / 3.0);
    assert_eq!(calculate_accuracy("SOS", "XOS"), 2.0 / 3.0);
    assert_eq!(calculate_accuracy("HELLO", "WORLD"), 1.0 / 5.0); // Only L matches
    assert_eq!(calculate_accuracy("", ""), 1.0);
    assert_eq!(calculate_accuracy("A", ""), 0.0);
}

#[test]
fn baseline_decoder_test() -> Result<()> {
    // Clean baseline test to establish current decoder status
    println!("=== DECODER BASELINE TEST ===");

    // Note: Set RUST_LOG=info environment variable to see decoder output during testing
    env_logger::try_init().ok();

    let generator = MorseGenerator::new(12000, 600.0, 20.0);

    let test_cases = [
        ("EEEE", "4 dots"),
        ("TTTT", "4 dashes"),
        ("ETET", "dot-dash-dot-dash"),
    ];

    for (i, (test_text, description)) in test_cases.iter().enumerate() {
        println!("\n--- Test {}: {} ({}) ---", i + 1, test_text, description);

        let temp_file = format!("baseline_test_{}.wav", i);
        generator.generate_wav_file(test_text, &temp_file)?;
        let decoded = decode_test_wav_file(&temp_file)?;
        println!(
            "Expected: {} | Decoded: {} | Success: {}",
            test_text,
            decoded,
            decoded == *test_text
        );

        // Clean up immediately
        std::fs::remove_file(&temp_file).ok();
    }

    // Summary
    println!("\n=== BASELINE SUMMARY ===");
    println!("This establishes our current decoder capabilities");
    println!("Focus on getting these 3 simple patterns working first");

    Ok(())
}

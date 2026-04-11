//! Adversarial stress test: hunt for panics in the LogRipper engine.
//!
//! Each attack vector spawns tasks that call public API functions with
//! pathological inputs. Panics are caught via `tokio::task::spawn` (which
//! converts panics to `JoinError`) and collected into a report.
//!
//! The test PASSES if it successfully runs all vectors and reports findings.
//! Any discovered panic is printed with its message for bug filing.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use logripper_core::adif::{parse_adi_qsos, serialize_adi_qsos, AdifMapper};
use logripper_core::domain::band::{band_from_adif, band_from_frequency_mhz};
use logripper_core::domain::mode::mode_from_adif;
use logripper_core::ffi;
use logripper_core::proto::logripper::domain::QsoRecord;

#[derive(Debug)]
struct PanicReport {
    vector: String,
    input_description: String,
    panic_message: String,
}

fn extract_panic_message(err: tokio::task::JoinError) -> String {
    if err.is_panic() {
        let panic_val = err.into_panic();
        if let Some(s) = panic_val.downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = panic_val.downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic payload".to_string()
        }
    } else {
        format!("non-panic join error: {err}")
    }
}

fn adversarial_strings() -> Vec<String> {
    vec![
        String::new(),
        " ".into(),
        "\t\n\r".into(),
        "\0".into(),
        "\0\0\0\0".into(),
        "a]b".into(),
        "\u{FFFD}".into(),
        "\u{200F}".into(),
        "\u{202E}".into(),
        "\u{1F4A9}".into(),
        "\u{0000}CALL\u{0000}".into(),
        "A\u{0301}".into(),
        "\u{FEFF}BOM".into(),
        "W1AW".into(),
        "K7DBG/P".into(),
        "VE3/W1AW/QRP".into(),
        "X".repeat(10_000),
        "\u{00FC}".repeat(5_000),
        "20250230".into(),
        "99991399".into(),
        "-1".into(),
        "NaN".into(),
        "Infinity".into(),
        "-Infinity".into(),
        "1e308".into(),
        "1e-308".into(),
        "-0.0".into(),
    ]
}

// ---- Attack Vector 1: ADIF Garbage Fuzzing ----

async fn fuzz_parse_adi_qsos() -> Vec<PanicReport> {
    let mut reports = Vec::new();

    let garbage_payloads: Vec<(&str, Vec<u8>)> = vec![
        ("empty", vec![]),
        ("single null byte", vec![0]),
        ("all 0xFF", vec![0xFF; 1024]),
        ("random-ish bytes", (0..=255).collect()),
        ("truncated tag", b"<CALL:4>W1A".to_vec()),
        ("negative length tag", b"<CALL:-1>W1AW".to_vec()),
        ("huge length tag", b"<CALL:999999999>W1AW".to_vec()),
        ("unclosed tag", b"<CALL:4".to_vec()),
        ("no value after tag", b"<CALL:4>".to_vec()),
        ("eor only", b"<eor>".to_vec()),
        ("header only", b"test header <eoh>".to_vec()),
        ("valid then garbage", b"<CALL:4>W1AW<BAND:3>20M<eor>\xFF\xFF\xFF".to_vec()),
        ("multibyte in field value", format!("<CALL:6>W1\u{00FC}AW<eor>").into_bytes()),
        ("null bytes in value", b"<CALL:5>W1\x00AW<eor>".to_vec()),
        ("emoji in callsign", "<CALL:4>\u{1F4A9}<eor>".to_owned().into_bytes()),
        ("1MB payload", vec![b'A'; 1_000_000]),
        (
            "adversarial date",
            format!(
                "<CALL:4>W1AW<QSO_DATE:8>202\u{00fc}123<TIME_ON:4>1200<BAND:3>20M<MODE:2>CW<eor>"
            )
            .into_bytes(),
        ),
    ];

    for (desc, payload) in &garbage_payloads {
        let payload = payload.clone();
        let desc = desc.to_string();
        let handle = tokio::task::spawn(async move {
            let _ = parse_adi_qsos(&payload).await;
        });

        if let Err(err) = handle.await {
            reports.push(PanicReport {
                vector: "ADIF parse fuzzing".into(),
                input_description: desc,
                panic_message: extract_panic_message(err),
            });
        }
    }

    reports
}

// ---- Attack Vector 2: ADIF Mapper with adversarial Records ----

async fn fuzz_adif_mapper() -> Vec<PanicReport> {
    let mut reports = Vec::new();

    let huge_callsign = "W".repeat(100_000);
    let field_combos: Vec<(&str, Vec<(&str, &str)>)> = vec![
        ("empty record", vec![]),
        ("call only", vec![("CALL", "W1AW")]),
        ("non-ASCII date", vec![
            ("CALL", "W1AW"),
            ("QSO_DATE", "202\u{00fc}123"),
        ]),
        ("non-ASCII time", vec![
            ("CALL", "W1AW"),
            ("QSO_DATE", "20250115"),
            ("TIME_ON", "12\u{00e4}0"),
        ]),
        ("negative freq", vec![
            ("CALL", "W1AW"),
            ("FREQ", "-14.074"),
        ]),
        ("huge freq", vec![
            ("CALL", "W1AW"),
            ("FREQ", "999999999999.0"),
        ]),
        ("NaN freq", vec![
            ("CALL", "W1AW"),
            ("FREQ", "NaN"),
        ]),
        ("infinity freq", vec![
            ("CALL", "W1AW"),
            ("FREQ", "Infinity"),
        ]),
        ("null byte callsign", vec![
            ("CALL", "\0\0\0\0"),
        ]),
        ("emoji callsign", vec![
            ("CALL", "\u{1F4A9}\u{1F4A9}"),
        ]),
        ("huge callsign", vec![
            ("CALL", &huge_callsign),
        ]),
        ("empty band", vec![
            ("CALL", "W1AW"),
            ("BAND", ""),
        ]),
        ("garbage band", vec![
            ("CALL", "W1AW"),
            ("BAND", "\u{FFFD}\u{FFFD}"),
        ]),
        ("all fields adversarial", vec![
            ("CALL", "\u{200F}\u{202E}"),
            ("QSO_DATE", "\0\0\0\0\0\0\0\0"),
            ("TIME_ON", "ZZZZ"),
            ("BAND", "999ZZZ"),
            ("MODE", "\t\n\r"),
            ("FREQ", "-0.0"),
            ("RST_SENT", "\u{FEFF}"),
            ("RST_RCVD", ""),
        ]),
    ];

    for (desc, fields) in &field_combos {
        let desc = desc.to_string();
        let fields: Vec<(String, String)> = fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let handle = tokio::task::spawn_blocking(move || {
            let mut rec = difa::Record::new();
            for (key, value) in &fields {
                let _ = rec.insert(key.as_str(), value.as_str());
            }
            let _ = AdifMapper::record_to_qso(&rec);
        });

        if let Err(err) = handle.await {
            reports.push(PanicReport {
                vector: "ADIF mapper fuzzing".into(),
                input_description: desc,
                panic_message: extract_panic_message(err),
            });
        }
    }

    reports
}

// ---- Attack Vector 3: QSO Record Chaos (round-trip through ADIF export) ----

async fn fuzz_qso_roundtrip() -> Vec<PanicReport> {
    let mut reports = Vec::new();

    let adversarial_qsos: Vec<(&str, QsoRecord)> = vec![
        ("default record", QsoRecord::default()),
        ("negative nanos", QsoRecord {
            worked_callsign: "W1AW".into(),
            utc_timestamp: Some(prost_types::Timestamp {
                seconds: 1_700_000_000,
                nanos: -1,
            }),
            ..Default::default()
        }),
        ("i32::MIN nanos", QsoRecord {
            worked_callsign: "W1AW".into(),
            utc_timestamp: Some(prost_types::Timestamp {
                seconds: 0,
                nanos: i32::MIN,
            }),
            ..Default::default()
        }),
        ("i64::MIN seconds", QsoRecord {
            worked_callsign: "W1AW".into(),
            utc_timestamp: Some(prost_types::Timestamp {
                seconds: i64::MIN,
                nanos: 0,
            }),
            ..Default::default()
        }),
        ("i64::MAX seconds", QsoRecord {
            worked_callsign: "W1AW".into(),
            utc_timestamp: Some(prost_types::Timestamp {
                seconds: i64::MAX,
                nanos: 999_999_999,
            }),
            ..Default::default()
        }),
        ("extreme band enum", QsoRecord {
            worked_callsign: "W1AW".into(),
            band: i32::MAX,
            mode: i32::MIN,
            ..Default::default()
        }),
        ("null-byte strings", QsoRecord {
            worked_callsign: "\0\0\0".into(),
            station_callsign: "\0".into(),
            comment: Some("\0".into()),
            ..Default::default()
        }),
        ("huge extra_fields", {
            let mut qso = QsoRecord {
                worked_callsign: "W1AW".into(),
                ..Default::default()
            };
            for i in 0..1000 {
                qso.extra_fields.insert(
                    format!("FIELD_{i}"),
                    "\u{1F4A9}".repeat(100),
                );
            }
            qso
        }),
    ];

    for (desc, qso) in adversarial_qsos {
        let desc = desc.to_string();
        let handle = tokio::task::spawn_blocking(move || {
            let fields = AdifMapper::qso_to_adif_fields(&qso);
            let adi = AdifMapper::fields_to_adi(&fields);
            let _ = serialize_adi_qsos(&[qso], true);
            let _parsed_back = adi.as_bytes();
        });

        if let Err(err) = handle.await {
            reports.push(PanicReport {
                vector: "QSO round-trip chaos".into(),
                input_description: desc,
                panic_message: extract_panic_message(err),
            });
        }
    }

    reports
}

// ---- Attack Vector 4: Concurrent Lookup Hammering ----

async fn hammer_lookup_coordinator() -> Vec<PanicReport> {
    use logripper_core::lookup::coordinator::{LookupCoordinator, LookupCoordinatorConfig};
    use logripper_core::lookup::provider::{
        CallsignProvider, ProviderLookup, ProviderLookupError,
    };

    #[derive(Debug)]
    struct ChaosProvider {
        call_count: AtomicUsize,
    }

    #[tonic::async_trait]
    impl CallsignProvider for ChaosProvider {
        async fn lookup_callsign(
            &self,
            _callsign: &str,
        ) -> Result<ProviderLookup, ProviderLookupError> {
            let n = self.call_count.fetch_add(1, Ordering::Relaxed);
            match n % 4 {
                0 => Ok(ProviderLookup::not_found(Vec::new())),
                1 => Ok(ProviderLookup::found(
                    logripper_core::proto::logripper::domain::CallsignRecord {
                        callsign: "TEST".into(),
                        first_name: "Chaos".into(),
                        ..Default::default()
                    },
                    Vec::new(),
                )),
                2 => {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    Ok(ProviderLookup::not_found(Vec::new()))
                }
                _ => Err(ProviderLookupError::transport(
                    "intentional chaos failure".to_string(),
                    Vec::new(),
                )),
            }
        }
    }

    let mut reports = Vec::new();
    let provider = Arc::new(ChaosProvider {
        call_count: AtomicUsize::new(0),
    });
    let coordinator = Arc::new(LookupCoordinator::new(
        provider,
        LookupCoordinatorConfig::new(Duration::from_millis(1), Duration::from_millis(1)),
    ));

    let callsigns = adversarial_strings();

    let mut handles = Vec::new();
    for _ in 0..10 {
        for callsign in &callsigns {
            let coord = Arc::clone(&coordinator);
            let cs = callsign.to_string();
            handles.push(tokio::spawn(async move {
                let _ = coord.lookup(&cs, false).await;
                let _ = coord.lookup(&cs, true).await;
                let _ = coord.stream_lookup(&cs, false).await;
                let _ = coord.get_cached_callsign(&cs).await;
            }));
        }
    }

    for (i, handle) in handles.into_iter().enumerate() {
        if let Err(err) = handle.await {
            reports.push(PanicReport {
                vector: "Lookup coordinator hammering".into(),
                input_description: format!("task {i}"),
                panic_message: extract_panic_message(err),
            });
        }
    }

    reports
}

// ---- Attack Vector 5: FFI Boundary Abuse ----

async fn fuzz_ffi() -> Vec<PanicReport> {
    let mut reports = Vec::new();

    let ffi_cases: Vec<(&str, Box<dyn FnOnce() + Send + 'static>)> = vec![
        ("version", Box::new(|| { let _ = ffi::dsp_version(); })),
        ("hz_to_khz(0)", Box::new(|| { let _ = ffi::hz_to_khz(0); })),
        ("hz_to_khz(u64::MAX)", Box::new(|| { let _ = ffi::hz_to_khz(u64::MAX); })),
        ("hz_to_khz(1)", Box::new(|| { let _ = ffi::hz_to_khz(1); })),
        ("moving_average(empty)", Box::new(|| { let _ = ffi::moving_average(&[]); })),
        ("moving_average(single)", Box::new(|| { let _ = ffi::moving_average(&[42.0]); })),
        ("moving_average(NaN)", Box::new(|| { let _ = ffi::moving_average(&[f64::NAN]); })),
        ("moving_average(Infinity)", Box::new(|| { let _ = ffi::moving_average(&[f64::INFINITY, f64::NEG_INFINITY]); })),
        ("moving_average(large)", Box::new(|| {
            let data = vec![1.0_f64; 100_000];
            let _ = ffi::moving_average(&data);
        })),
    ];

    for (desc, func) in ffi_cases {
        let desc = desc.to_string();
        let handle = tokio::task::spawn_blocking(func);
        if let Err(err) = handle.await {
            reports.push(PanicReport {
                vector: "FFI boundary abuse".into(),
                input_description: desc,
                panic_message: extract_panic_message(err),
            });
        }
    }

    reports
}

// ---- Attack Vector 6: Band/Mode/Location Parsing Chaos ----

async fn fuzz_band_mode_parsing() -> Vec<PanicReport> {
    let mut reports = Vec::new();

    let frequency_values: Vec<(&str, f64)> = vec![
        ("NaN", f64::NAN),
        ("Infinity", f64::INFINITY),
        ("-Infinity", f64::NEG_INFINITY),
        ("zero", 0.0),
        ("negative", -14.074),
        ("tiny subnormal", f64::MIN_POSITIVE * 0.5),
        ("f64::MAX", f64::MAX),
        ("f64::MIN", f64::MIN),
        ("-0.0", -0.0),
        ("1e308", 1e308),
        ("normal 14.074", 14.074),
    ];

    for (desc, freq) in &frequency_values {
        let desc = desc.to_string();
        let freq = *freq;
        let handle = tokio::task::spawn_blocking(move || {
            let _ = band_from_frequency_mhz(freq);
        });
        if let Err(err) = handle.await {
            reports.push(PanicReport {
                vector: "Band from frequency".into(),
                input_description: desc,
                panic_message: extract_panic_message(err),
            });
        }
    }

    for adversarial in adversarial_strings() {
        let desc = if adversarial.len() > 30 {
            format!("{}... ({} bytes)", &adversarial[..30.min(adversarial.len())], adversarial.len())
        } else {
            format!("{adversarial:?}")
        };
        let handle = tokio::task::spawn_blocking(move || {
            let _ = band_from_adif(&adversarial);
            let _ = mode_from_adif(&adversarial);
        });
        if let Err(err) = handle.await {
            reports.push(PanicReport {
                vector: "Band/mode string parsing".into(),
                input_description: desc,
                panic_message: extract_panic_message(err),
            });
        }
    }

    reports
}

// ---- Main Test ----

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn stress_test_panic_hunt() {
    let (adif_parse, adif_mapper, qso_roundtrip, lookup, ffi_results, parsing) = tokio::join!(
        fuzz_parse_adi_qsos(),
        fuzz_adif_mapper(),
        fuzz_qso_roundtrip(),
        hammer_lookup_coordinator(),
        fuzz_ffi(),
        fuzz_band_mode_parsing(),
    );

    let mut all_reports = Vec::new();
    all_reports.extend(adif_parse);
    all_reports.extend(adif_mapper);
    all_reports.extend(qso_roundtrip);
    all_reports.extend(lookup);
    all_reports.extend(ffi_results);
    all_reports.extend(parsing);

    eprintln!("\n============================================================");
    eprintln!("STRESS TEST PANIC REPORT");
    eprintln!("============================================================");

    if all_reports.is_empty() {
        eprintln!("No panics found. The engine held up.");
    } else {
        eprintln!("Found {} panic(s):\n", all_reports.len());
        for (i, report) in all_reports.iter().enumerate() {
            eprintln!(
                "  [{i}] Vector: {}\n      Input: {}\n      Panic: {}\n",
                report.vector, report.input_description, report.panic_message
            );
        }
    }

    eprintln!("============================================================\n");

    // Report findings. The test passes either way -- it's a diagnostic tool.
    // Finding 0 panics means the engine is robust. Finding panics means we have bugs to file.
    assert!(
        all_reports.is_empty(),
        "Found {} panic(s) -- see report above for details.",
        all_reports.len()
    );
}

//! Captures the WPM and pitch values that ditdah emits via the `log` crate.
//!
//! ditdah's MorseDecoder reports its self-calibrated parameters with `log::info!`
//! lines like `Best fit: WPM = 18.0, Threshold = 1.2345e-3` and
//! `Estimated pitch: 612.34 Hz`. We don't want to fork the upstream library, so
//! we install a custom `log::Log` implementation that scrapes those messages and
//! makes them available to the rest of the program.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use log::{LevelFilter, Log, Metadata, Record};
use parking_lot::Mutex;

#[derive(Debug, Default, Clone, Copy)]
pub struct DitdahStats {
    pub wpm: Option<f32>,
    pub pitch_hz: Option<f32>,
    pub threshold: Option<f32>,
}

#[derive(Default)]
struct Inner {
    wpm_bits: AtomicU32,
    pitch_bits: AtomicU32,
    threshold_bits: AtomicU32,
    have_wpm: AtomicU32,
    have_pitch: AtomicU32,
    have_threshold: AtomicU32,
}

#[derive(Clone, Default)]
pub struct DitdahLogCapture {
    inner: Arc<Inner>,
    /// Echo to stderr if true (for debug builds).
    echo: Arc<Mutex<bool>>,
}

impl DitdahLogCapture {
    pub fn new(echo: bool) -> Self {
        Self {
            inner: Arc::new(Inner::default()),
            echo: Arc::new(Mutex::new(echo)),
        }
    }

    pub fn snapshot(&self) -> DitdahStats {
        let i = &self.inner;
        DitdahStats {
            wpm: if i.have_wpm.load(Ordering::Relaxed) != 0 {
                Some(f32::from_bits(i.wpm_bits.load(Ordering::Relaxed)))
            } else {
                None
            },
            pitch_hz: if i.have_pitch.load(Ordering::Relaxed) != 0 {
                Some(f32::from_bits(i.pitch_bits.load(Ordering::Relaxed)))
            } else {
                None
            },
            threshold: if i.have_threshold.load(Ordering::Relaxed) != 0 {
                Some(f32::from_bits(i.threshold_bits.load(Ordering::Relaxed)))
            } else {
                None
            },
        }
    }

    pub fn install(self) -> anyhow::Result<()> {
        log::set_max_level(LevelFilter::Info);
        log::set_boxed_logger(Box::new(self))
            .map_err(|e| anyhow::anyhow!("failed to install logger: {e}"))?;
        Ok(())
    }
}

impl Log for DitdahLogCapture {
    fn enabled(&self, _meta: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let msg = record.args().to_string();
        if let Some(rest) = msg.strip_prefix("Best fit: WPM = ") {
            // Example: "Best fit: WPM = 18.0, Threshold = 1.2345e-3"
            if let Some((wpm_str, rest)) = rest.split_once(',') {
                if let Ok(wpm) = wpm_str.trim().parse::<f32>() {
                    self.inner.wpm_bits.store(wpm.to_bits(), Ordering::Relaxed);
                    self.inner.have_wpm.store(1, Ordering::Relaxed);
                }
                if let Some(thr_str) = rest.trim().strip_prefix("Threshold = ") {
                    if let Ok(thr) = thr_str.trim().parse::<f32>() {
                        self.inner
                            .threshold_bits
                            .store(thr.to_bits(), Ordering::Relaxed);
                        self.inner.have_threshold.store(1, Ordering::Relaxed);
                    }
                }
            }
        } else if let Some(rest) = msg.strip_prefix("Estimated pitch: ") {
            // Example: "Estimated pitch: 612.34 Hz"
            let trimmed = rest.trim_end_matches(" Hz").trim();
            if let Ok(pitch) = trimmed.parse::<f32>() {
                self.inner
                    .pitch_bits
                    .store(pitch.to_bits(), Ordering::Relaxed);
                self.inner.have_pitch.store(1, Ordering::Relaxed);
            }
        }

        if *self.echo.lock() {
            eprintln!("[ditdah] {} {}", record.level(), msg);
        }
    }

    fn flush(&self) {}
}

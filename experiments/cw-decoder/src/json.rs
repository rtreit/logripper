//! Newline-delimited JSON event output for the Avalonia GUI bridge.
//!
//! Each call writes one JSON object followed by a newline to stdout and
//! flushes immediately, so the consumer can read an event-stream by
//! `ReadLineAsync()` without buffering surprises.
//!
//! Schema (see `gui/Services/CwDecoderProcess.cs` for the C# parser):
//!   {"t": <secs since start>, "type": "ready",  "device": "...", "rate": 48000}
//!   {"t": ..., "type": "pitch",   "hz": 700.5}
//!   {"t": ..., "type": "wpm",     "wpm": 18.4}
//!   {"t": ..., "type": "char",    "ch": "A", "morse": ".-"}
//!   {"t": ..., "type": "word"}
//!   {"t": ..., "type": "garbled", "morse": "...-.."}
//!   {"t": ..., "type": "power",   "power": 0.0123, "threshold": 0.005, "noise": 0.0008, "snr": 15.4, "signal": true}
//!   {"t": ..., "type": "stats",   "wpm": 18.4, "pitch": 700.5, "threshold": 0.005}
//!   {"t": ..., "type": "end"}

use std::io::{self, Write};

use serde_json::{json, Value};

use crate::streaming::StreamEvent;

pub struct JsonEmitter {
    out: io::Stdout,
}

impl JsonEmitter {
    pub fn new() -> Self {
        Self { out: io::stdout() }
    }

    pub fn emit(&mut self, t: f32, value: Value) {
        let mut handle = self.out.lock();
        // Prepend the event timestamp so consumers don't have to wrap.
        let mut obj = value;
        if let Some(map) = obj.as_object_mut() {
            map.insert("t".to_string(), json!(round3(t)));
        }
        let _ = serde_json::to_writer(&mut handle, &obj);
        let _ = handle.write_all(b"\n");
        let _ = handle.flush();
    }

    pub fn emit_event(&mut self, t: f32, ev: &StreamEvent) {
        let v = match ev {
            StreamEvent::PitchUpdate { pitch_hz } => json!({
                "type": "pitch",
                "hz": round1(*pitch_hz),
            }),
            StreamEvent::WpmUpdate { wpm } => json!({
                "type": "wpm",
                "wpm": round2(*wpm),
            }),
            StreamEvent::Char { ch, morse } => json!({
                "type": "char",
                "ch": ch.to_string(),
                "morse": morse,
            }),
            StreamEvent::Word => json!({
                "type": "word",
            }),
            StreamEvent::Garbled { morse } => json!({
                "type": "garbled",
                "morse": morse,
            }),
            StreamEvent::Power { power, threshold, noise, snr, signal } => json!({
                "type": "power",
                "power": *power,
                "threshold": *threshold,
                "noise": *noise,
                "snr": round2(*snr),
                "signal": *signal,
            }),
        };
        self.emit(t, v);
    }
}

fn round1(x: f32) -> f32 {
    (x * 10.0).round() / 10.0
}
fn round2(x: f32) -> f32 {
    (x * 100.0).round() / 100.0
}
fn round3(x: f32) -> f32 {
    (x * 1000.0).round() / 1000.0
}

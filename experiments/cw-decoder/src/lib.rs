//! Library facade: exposes the streaming decoder + audio helpers to
//! sibling binaries (CLI, eval harness, GUI bridge). Keeps the crate
//! single-target while letting `src/bin/*.rs` reuse the heavy modules.

pub mod audio;
pub mod bench_latency;
pub mod decoder;
pub mod ditdah_streaming;
pub mod harvest;
pub mod json;
pub mod log_capture;
pub mod preview;
pub mod streaming;
pub mod tui;

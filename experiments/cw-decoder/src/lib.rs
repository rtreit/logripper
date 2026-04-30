//! Library facade: exposes the streaming decoder + audio helpers to
//! sibling binaries (CLI, eval harness, GUI bridge). Keeps the crate
//! single-target while letting `src/bin/*.rs` reuse the heavy modules.

pub mod append_decode;
pub mod audio;
pub mod bench_latency;
pub mod decoder;
pub mod ditdah_streaming;
pub mod envelope_decoder;
pub mod harvest;
pub mod json;
pub mod log_capture;
pub mod preprocess;
pub mod preview;
pub mod region_stream;
pub mod streaming;
pub mod streaming_v2;
pub mod tui;

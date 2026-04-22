//! Library facade: exposes the streaming decoder + audio helpers to
//! sibling binaries (CLI, eval harness, GUI bridge). Keeps the crate
//! single-target while letting `src/bin/*.rs` reuse the heavy modules.

pub mod audio;
pub mod streaming;

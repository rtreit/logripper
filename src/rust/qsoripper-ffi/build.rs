//! Build script intentionally left as a no-op.
//!
//! The FFI header is checked in at `qsoripper_ffi.h` and copied to the C
//! project during build packaging; generating it via cbindgen in CI violates
//! this repository's cargo-deny license and duplicate-crate policy.

fn main() {}

//! Core Rust engine modules for `LogRipper`.
#![deny(unsafe_code)]

/// ADIF import/export adapters.
pub mod adif;
/// Domain helpers for QSO and lookup-related types.
pub mod domain;
/// FFI boundary for DSP helpers.
pub mod ffi;
/// Lookup orchestration, providers, and cache policy.
pub mod lookup;
/// Generated protobuf and gRPC bindings.
pub mod proto;

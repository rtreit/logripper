//! Core Rust engine modules for `QsoRipper`.
#![deny(unsafe_code)]

/// ADIF import/export adapters.
pub mod adif;
/// Application services that coordinate engine workflows above storage ports.
pub mod application;
/// Domain helpers for QSO and lookup-related types.
pub mod domain;
/// FFI boundary for DSP helpers.
pub mod ffi;
/// Lookup orchestration, providers, and cache policy.
pub mod lookup;
/// Generated protobuf and gRPC bindings.
pub mod proto;
/// Storage ports, errors, and query types for engine-owned persistence.
pub mod storage;

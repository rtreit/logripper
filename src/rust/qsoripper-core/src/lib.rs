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
/// QRZ Logbook API client for bidirectional QSO sync.
pub mod qrz_logbook;
/// Rig control providers, normalization, and cached rig state snapshots.
pub mod rig_control;
/// Space weather providers, caching, and normalization.
pub mod space_weather;
/// Storage ports, errors, and query types for engine-owned persistence.
pub mod storage;

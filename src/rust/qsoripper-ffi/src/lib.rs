//! C-ABI gRPC client shim for the QsoRipper engine.
//!
//! This crate compiles to a shared library (`cdylib`) that exposes simple C functions
//! for communicating with the QsoRipper Rust engine over gRPC. It replaces the
//! CLI-based `CreateProcess` approach used by the win32 app with direct gRPC calls.
//!
//! # Architecture
//!
//! ```text
//! Win32 app (C) ──► qsoripper-ffi.dll (Rust, tonic gRPC client) ──► qsoripper-server
//! ```
//!
//! The FFI library owns a tokio runtime internally; C callers see only synchronous functions.

#![allow(unsafe_code, clippy::doc_markdown)]

mod client;
mod types;

use std::os::raw::c_char;

pub use types::{
    QsrLogQsoRequest, QsrLogQsoResult, QsrLookupResult, QsrQsoDetail, QsrQsoList, QsrQsoSummary,
    QsrRigStatus, QsrRstReport, QsrSpaceWeather, QsrUpdateQsoRequest,
};

use client::QsrClient;

/// Connect to the QsoRipper engine at the given endpoint.
///
/// # Parameters
/// - `endpoint`: Null-terminated UTF-8 string, e.g. `"http://127.0.0.1:50051"`.
///
/// # Returns
/// An opaque client handle, or null on failure (call `qsr_last_error`).
///
/// # Safety
/// `endpoint` must be a valid null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qsr_connect(endpoint: *const c_char) -> *mut QsrClient {
    let ep = unsafe { client::cstr_to_str(endpoint) };
    if let Ok(boxed) = QsrClient::connect(ep) {
        Box::into_raw(boxed)
    } else {
        client::set_error("WIN32-BUG-3: connect failed");
        std::ptr::null_mut()
    }
}

/// Disconnect and free the client handle.
///
/// # Safety
/// `client` must be a pointer returned by `qsr_connect`, or null (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qsr_disconnect(client: *mut QsrClient) {
    if !client.is_null() {
        drop(unsafe { Box::from_raw(client) });
    }
}

/// Get the last error message as a null-terminated UTF-8 string.
///
/// The returned pointer is valid until the next FFI call from the same thread.
#[unsafe(no_mangle)]
pub extern "C" fn qsr_last_error() -> *const c_char {
    client::last_error_cstr()
}

/// Log a new QSO.
///
/// # Returns
/// 0 on success, -1 on failure.
///
/// # Safety
/// `client` must be a valid client pointer. `req` and `out` must be valid pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qsr_log_qso(
    client: *mut QsrClient,
    req: *const QsrLogQsoRequest,
    out: *mut QsrLogQsoResult,
) -> i32 {
    let Some(c) = (unsafe { client.as_mut() }) else {
        return -1;
    };
    let Some(r) = (unsafe { req.as_ref() }) else {
        return -1;
    };
    let Some(o) = (unsafe { out.as_mut() }) else {
        return -1;
    };
    c.log_qso(r, o)
}

/// Update an existing QSO.
///
/// # Returns
/// 0 on success, -1 on failure.
///
/// # Safety
/// `client` and `req` must be valid pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qsr_update_qso(
    client: *mut QsrClient,
    req: *const QsrUpdateQsoRequest,
) -> i32 {
    let Some(c) = (unsafe { client.as_mut() }) else {
        return -1;
    };
    let Some(r) = (unsafe { req.as_ref() }) else {
        return -1;
    };
    c.update_qso(r)
}

/// Get a single QSO by local UUID.
///
/// # Returns
/// 0 on success, -1 on failure (not found or gRPC error).
///
/// # Safety
/// `client` must be valid. `local_id` must be a null-terminated C string. `out` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qsr_get_qso(
    client: *mut QsrClient,
    local_id: *const c_char,
    out: *mut QsrQsoDetail,
) -> i32 {
    let Some(c) = (unsafe { client.as_mut() }) else {
        return -1;
    };
    let id = unsafe { client::cstr_to_str(local_id) };
    let Some(o) = (unsafe { out.as_mut() }) else {
        return -1;
    };
    c.get_qso(id, o)
}

/// Delete a QSO by local UUID.
///
/// # Returns
/// 0 on success, -1 on failure.
///
/// # Safety
/// `client` must be valid. `local_id` must be a null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qsr_delete_qso(client: *mut QsrClient, local_id: *const c_char) -> i32 {
    let Some(c) = (unsafe { client.as_mut() }) else {
        return -1;
    };
    let id = unsafe { client::cstr_to_str(local_id) };
    c.delete_qso(id)
}

/// List all QSOs into a heap-allocated list.
///
/// On success, `out->items` and `out->count` are populated. Free with `qsr_free_qso_list`.
///
/// # Returns
/// 0 on success, -1 on failure.
///
/// # Safety
/// `client` and `out` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qsr_list_qsos(client: *mut QsrClient, out: *mut QsrQsoList) -> i32 {
    let Some(c) = (unsafe { client.as_mut() }) else {
        return -1;
    };
    let Some(o) = (unsafe { out.as_mut() }) else {
        return -1;
    };
    c.list_qsos(o)
}

/// Free a QSO list returned by `qsr_list_qsos`.
///
/// # Safety
/// `list` must be a `QsrQsoList` populated by `qsr_list_qsos`, or have null `items`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qsr_free_qso_list(list: *mut QsrQsoList) {
    let Some(l) = (unsafe { list.as_mut() }) else {
        return;
    };
    if !l.items.is_null() && l.count > 0 {
        #[allow(clippy::cast_sign_loss)]
        let slice =
            unsafe { Box::from_raw(std::slice::from_raw_parts_mut(l.items, l.count as usize)) };
        drop(slice);
        l.items = std::ptr::null_mut();
        l.count = 0;
    }
}

/// Look up a callsign.
///
/// # Returns
/// 0 on success, -1 on failure.
///
/// # Safety
/// `client` must be valid. `callsign` must be a null-terminated C string. `out` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qsr_lookup(
    client: *mut QsrClient,
    callsign: *const c_char,
    out: *mut QsrLookupResult,
) -> i32 {
    let Some(c) = (unsafe { client.as_mut() }) else {
        return -1;
    };
    let call = unsafe { client::cstr_to_str(callsign) };
    let Some(o) = (unsafe { out.as_mut() }) else {
        return -1;
    };
    c.lookup(call, o)
}

/// Get the current rig status / snapshot.
///
/// # Returns
/// 0 on success, -1 on failure.
///
/// # Safety
/// `client` and `out` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qsr_get_rig_status(client: *mut QsrClient, out: *mut QsrRigStatus) -> i32 {
    let Some(c) = (unsafe { client.as_mut() }) else {
        return -1;
    };
    let Some(o) = (unsafe { out.as_mut() }) else {
        return -1;
    };
    c.get_rig_snapshot(o)
}

/// Get current space weather data.
///
/// # Returns
/// 0 on success, -1 on failure.
///
/// # Safety
/// `client` and `out` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qsr_get_space_weather(
    client: *mut QsrClient,
    out: *mut QsrSpaceWeather,
) -> i32 {
    let Some(c) = (unsafe { client.as_mut() }) else {
        return -1;
    };
    let Some(o) = (unsafe { out.as_mut() }) else {
        return -1;
    };
    c.get_space_weather(o)
}

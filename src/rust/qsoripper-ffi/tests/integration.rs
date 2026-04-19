//! Integration tests for the QsoRipper FFI library.
//!
//! These tests exercise the `extern "C"` functions end-to-end against a
//! running `qsoripper-server`.  They are **skipped** when no server is
//! reachable on `QSORIPPER_TEST_ENDPOINT` (default `http://127.0.0.1:50051`).
//!
//! To run:
//! ```text
//! # Terminal 1 — start the engine with in-memory storage
//! ./start-qsoripper.ps1 -Storage memory
//!
//! # Terminal 2 — run the tests
//! cargo test -p qsoripper-ffi --test integration
//! ```

#![allow(
    unsafe_code,
    clippy::undocumented_unsafe_blocks,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::doc_markdown,
    clippy::borrow_as_ptr
)]

use std::ffi::CString;
use std::net::TcpStream;

use qsoripper_ffi::{
    QsrLogQsoRequest, QsrLogQsoResult, QsrLookupResult, QsrQsoDetail, QsrQsoList, QsrRigStatus,
    QsrRstReport, QsrSpaceWeather, QsrUpdateQsoRequest,
};

// Re-declare FFI functions for linking.
// When running integration tests the crate is compiled as an rlib, so we can
// call the public `extern "C"` symbols directly.
extern "C" {
    fn qsr_connect(endpoint: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
    fn qsr_disconnect(client: *mut std::ffi::c_void);
    fn qsr_last_error() -> *const std::os::raw::c_char;
    fn qsr_log_qso(
        client: *mut std::ffi::c_void,
        req: *const QsrLogQsoRequest,
        out: *mut QsrLogQsoResult,
    ) -> i32;
    fn qsr_update_qso(client: *mut std::ffi::c_void, req: *const QsrUpdateQsoRequest) -> i32;
    fn qsr_get_qso(
        client: *mut std::ffi::c_void,
        local_id: *const std::os::raw::c_char,
        out: *mut QsrQsoDetail,
    ) -> i32;
    fn qsr_delete_qso(client: *mut std::ffi::c_void, local_id: *const std::os::raw::c_char) -> i32;
    fn qsr_list_qsos(client: *mut std::ffi::c_void, out: *mut QsrQsoList) -> i32;
    fn qsr_free_qso_list(list: *mut QsrQsoList);
    fn qsr_lookup(
        client: *mut std::ffi::c_void,
        callsign: *const std::os::raw::c_char,
        out: *mut QsrLookupResult,
    ) -> i32;
    fn qsr_get_rig_status(client: *mut std::ffi::c_void, out: *mut QsrRigStatus) -> i32;
    fn qsr_get_space_weather(client: *mut std::ffi::c_void, out: *mut QsrSpaceWeather) -> i32;
}

/// Read a null-terminated `[u8]` field as a Rust `&str`.
fn buf_as_str(buf: &[u8]) -> &str {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    std::str::from_utf8(&buf[..end]).unwrap_or("")
}

/// Write a Rust string into a fixed-size `[u8]` field, null-terminated.
fn fill_buf(buf: &mut [u8], s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(buf.len().saturating_sub(1));
    buf[..len].copy_from_slice(&bytes[..len]);
    buf[len] = 0;
    for b in &mut buf[len + 1..] {
        *b = 0;
    }
}

fn test_endpoint() -> String {
    std::env::var("QSORIPPER_TEST_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:50051".to_string())
}

/// Returns true if the gRPC server is listening.
fn server_is_reachable() -> bool {
    let endpoint = test_endpoint();
    let addr = endpoint
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    TcpStream::connect(addr).is_ok()
}

/// Connect to the server, returning a raw client pointer.
/// Panics if connection fails.
fn connect() -> *mut std::ffi::c_void {
    let endpoint = test_endpoint();
    let c_endpoint = CString::new(endpoint.clone()).expect("CString::new failed");
    let client = unsafe { qsr_connect(c_endpoint.as_ptr()) };
    assert!(
        !client.is_null(),
        "qsr_connect failed for {endpoint}: {}",
        last_error()
    );
    client
}

fn last_error() -> String {
    let ptr = unsafe { qsr_last_error() };
    if ptr.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

macro_rules! skip_if_no_server {
    () => {
        if !server_is_reachable() {
            eprintln!(
                "SKIP: QsoRipper server not reachable at {}",
                test_endpoint()
            );
            return;
        }
    };
}

#[test]
fn connect_and_disconnect() {
    skip_if_no_server!();
    let client = connect();
    unsafe { qsr_disconnect(client) };
}

#[test]
fn connect_failure_sets_last_error() {
    let bad_endpoint = CString::new("not-a-uri").expect("CString::new failed");
    let client = unsafe { qsr_connect(bad_endpoint.as_ptr()) };
    assert!(
        client.is_null(),
        "expected qsr_connect to fail for invalid URI"
    );
    assert_eq!(
        last_error(),
        "WIN32-BUG-3: connect failed",
        "connect failure must set deterministic last_error"
    );
}

#[test]
fn log_list_get_delete_round_trip() {
    skip_if_no_server!();
    let client = connect();

    // 1. Log a QSO
    let mut req: QsrLogQsoRequest = unsafe { std::mem::zeroed() };
    fill_buf(&mut req.callsign, "W1AW");
    fill_buf(&mut req.band, "20M");
    fill_buf(&mut req.mode, "SSB");
    fill_buf(&mut req.datetime, "2025-01-15 14:30");
    req.rst_sent = QsrRstReport {
        readability: 5,
        strength: 9,
        tone: 0,
    };
    req.rst_rcvd = QsrRstReport {
        readability: 5,
        strength: 9,
        tone: 0,
    };
    req.freq_khz = 14225;
    fill_buf(&mut req.comment, "FFI integration test");

    let mut result: QsrLogQsoResult = unsafe { std::mem::zeroed() };
    let rc = unsafe { qsr_log_qso(client, &req, &mut result) };
    assert_eq!(rc, 0, "qsr_log_qso failed: {}", last_error());

    let local_id = buf_as_str(&result.local_id);
    assert!(!local_id.is_empty(), "local_id should not be empty");

    // 2. List QSOs — the one we just logged should be present
    let mut list: QsrQsoList = unsafe { std::mem::zeroed() };
    let rc = unsafe { qsr_list_qsos(client, &mut list) };
    assert_eq!(rc, 0, "qsr_list_qsos failed: {}", last_error());
    assert!(list.count > 0, "list should have at least one QSO");

    // Find our QSO in the list
    let found = (0..list.count).any(|i| {
        let item = unsafe { &*list.items.add(i as usize) };
        buf_as_str(&item.local_id) == local_id
    });
    assert!(found, "logged QSO not found in list");
    unsafe { qsr_free_qso_list(&mut list) };

    // 3. Get the QSO by ID
    let c_id = CString::new(local_id).expect("CString::new failed");
    let mut detail: QsrQsoDetail = unsafe { std::mem::zeroed() };
    let rc = unsafe { qsr_get_qso(client, c_id.as_ptr(), &mut detail) };
    assert_eq!(rc, 0, "qsr_get_qso failed: {}", last_error());
    assert_eq!(buf_as_str(&detail.callsign), "W1AW");
    assert_eq!(buf_as_str(&detail.band), "20M");
    assert_eq!(buf_as_str(&detail.mode), "SSB");
    assert_eq!(buf_as_str(&detail.comment), "FFI integration test");

    // 4. Delete the QSO
    let rc = unsafe { qsr_delete_qso(client, c_id.as_ptr()) };
    assert_eq!(rc, 0, "qsr_delete_qso failed: {}", last_error());

    // 5. Verify it's gone
    let mut detail2: QsrQsoDetail = unsafe { std::mem::zeroed() };
    let rc = unsafe { qsr_get_qso(client, c_id.as_ptr(), &mut detail2) };
    assert_ne!(rc, 0, "qsr_get_qso should fail for deleted QSO");

    unsafe { qsr_disconnect(client) };
}

#[test]
fn update_qso_round_trip() {
    skip_if_no_server!();
    let client = connect();

    // Log a QSO
    let mut req: QsrLogQsoRequest = unsafe { std::mem::zeroed() };
    fill_buf(&mut req.callsign, "K1ABC");
    fill_buf(&mut req.band, "40M");
    fill_buf(&mut req.mode, "CW");
    fill_buf(&mut req.datetime, "2025-01-15 20:00");
    req.rst_sent = QsrRstReport {
        readability: 5,
        strength: 9,
        tone: 9,
    };
    req.rst_rcvd = QsrRstReport {
        readability: 5,
        strength: 8,
        tone: 9,
    };

    let mut result: QsrLogQsoResult = unsafe { std::mem::zeroed() };
    let rc = unsafe { qsr_log_qso(client, &req, &mut result) };
    assert_eq!(rc, 0, "qsr_log_qso failed: {}", last_error());
    let local_id = buf_as_str(&result.local_id).to_string();

    // Update the QSO — change comment
    let mut ureq: QsrUpdateQsoRequest = unsafe { std::mem::zeroed() };
    fill_buf(&mut ureq.local_id, &local_id);
    ureq.qso = req;
    fill_buf(&mut ureq.qso.comment, "updated via FFI");

    let rc = unsafe { qsr_update_qso(client, &ureq) };
    assert_eq!(rc, 0, "qsr_update_qso failed: {}", last_error());

    // Verify the update
    let c_id = CString::new(local_id.as_str()).expect("CString::new failed");
    let mut detail: QsrQsoDetail = unsafe { std::mem::zeroed() };
    let rc = unsafe { qsr_get_qso(client, c_id.as_ptr(), &mut detail) };
    assert_eq!(rc, 0, "qsr_get_qso failed: {}", last_error());
    assert_eq!(buf_as_str(&detail.comment), "updated via FFI");

    // Clean up
    let rc = unsafe { qsr_delete_qso(client, c_id.as_ptr()) };
    assert_eq!(rc, 0);

    unsafe { qsr_disconnect(client) };
}

#[test]
fn space_weather_does_not_crash() {
    skip_if_no_server!();
    let client = connect();

    let mut sw: QsrSpaceWeather = unsafe { std::mem::zeroed() };
    // May succeed or fail depending on server config — we just check it doesn't crash
    let _rc = unsafe { qsr_get_space_weather(client, &mut sw) };

    unsafe { qsr_disconnect(client) };
}

#[test]
fn rig_status_does_not_crash() {
    skip_if_no_server!();
    let client = connect();

    let mut rs: QsrRigStatus = unsafe { std::mem::zeroed() };
    let _rc = unsafe { qsr_get_rig_status(client, &mut rs) };

    unsafe { qsr_disconnect(client) };
}

#[test]
fn lookup_does_not_crash() {
    skip_if_no_server!();
    let client = connect();

    let callsign = CString::new("W1AW").expect("CString::new failed");
    let mut lr: QsrLookupResult = unsafe { std::mem::zeroed() };
    let _rc = unsafe { qsr_lookup(client, callsign.as_ptr(), &mut lr) };

    unsafe { qsr_disconnect(client) };
}

#[test]
fn null_client_returns_error() {
    let mut list: QsrQsoList = unsafe { std::mem::zeroed() };
    let rc = unsafe { qsr_list_qsos(std::ptr::null_mut(), &mut list) };
    assert_eq!(rc, -1, "null client should return -1");
}

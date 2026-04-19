//! Internal gRPC client wrapper.
//!
//! Manages a tokio runtime and tonic channels, providing synchronous
//! blocking calls suitable for the C FFI boundary.

// The date-parsing and struct-population code uses index-based access
// with validated lengths. Suppress pedantic indexing/slicing warnings.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::similar_names
)]

use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::Mutex;

use tonic::transport::Channel;

use qsoripper_core::domain::band::{band_from_adif, band_to_adif};
use qsoripper_core::domain::mode::{mode_from_adif, mode_to_adif};
use qsoripper_core::proto::qsoripper::domain::{
    Band, Mode, QsoRecord, RigConnectionStatus, RstReport,
};
use qsoripper_core::proto::qsoripper::services::{
    logbook_service_client::LogbookServiceClient, lookup_service_client::LookupServiceClient,
    rig_control_service_client::RigControlServiceClient,
    space_weather_service_client::SpaceWeatherServiceClient, DeleteQsoRequest,
    GetCurrentSpaceWeatherRequest, GetQsoRequest, GetRigSnapshotRequest, ListQsosRequest,
    LogQsoRequest, LookupRequest, UpdateQsoRequest,
};

use crate::types::{
    str_to_buf, QsrLogQsoRequest, QsrLogQsoResult, QsrLookupResult, QsrQsoDetail, QsrQsoList,
    QsrQsoSummary, QsrRigStatus, QsrRstReport, QsrSpaceWeather, QsrUpdateQsoRequest,
};

/// Thread-local last error message.
static LAST_ERROR: Mutex<String> = Mutex::new(String::new());

/// Set the last error message.
pub(crate) fn set_error(msg: impl Into<String>) {
    if let Ok(mut guard) = LAST_ERROR.lock() {
        *guard = msg.into();
    }
}

/// Get the last error message as a C string pointer.
/// The pointer is valid until the next FFI call.
/// Returns an empty string if no error.
pub(crate) fn last_error_cstr() -> *const c_char {
    // Use a thread-local to hold the null-terminated copy
    thread_local! {
        static BUF: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    let msg = LAST_ERROR
        .lock()
        .map_or_else(|_| String::new(), |g| g.clone());
    BUF.with(|buf| {
        let mut b = buf.borrow_mut();
        b.clear();
        b.extend_from_slice(msg.as_bytes());
        b.push(0);
        b.as_ptr().cast::<c_char>()
    })
}

/// Read a null-terminated UTF-8 C string, returning an empty string on null/invalid.
pub(crate) unsafe fn cstr_to_str<'a>(ptr: *const c_char) -> &'a str {
    if ptr.is_null() {
        return "";
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().unwrap_or("")
}

/// Read a null-terminated string from a fixed-size byte buffer.
pub(crate) fn buf_to_str(buf: &[u8]) -> &str {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    std::str::from_utf8(&buf[..end]).unwrap_or("")
}

/// Format a frequency in Hz to radio-style display: "14.225.00" or "3.536.50".
/// Pattern: `<MHz>.<kHz>.<fractional kHz in 10s of Hz>`
fn format_freq_radio_style(freq_hz: u64) -> String {
    let mhz = freq_hz / 1_000_000;
    let khz = (freq_hz % 1_000_000) / 1_000;
    let tens_hz = (freq_hz % 1_000) / 10;
    format!("{mhz}.{khz:03}.{tens_hz:02}")
}

/// Opaque client handle holding the runtime and gRPC channels.
pub struct QsrClient {
    runtime: tokio::runtime::Runtime,
    logbook: LogbookServiceClient<Channel>,
    lookup: LookupServiceClient<Channel>,
    rig: RigControlServiceClient<Channel>,
    weather: SpaceWeatherServiceClient<Channel>,
}

impl QsrClient {
    /// Connect to the engine at the given endpoint (e.g. `"http://127.0.0.1:50051"`).
    pub(crate) fn connect(endpoint: &str) -> Result<Box<Self>, String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("Failed to create tokio runtime: {e}"))?;

        let channel = runtime
            .block_on(
                Channel::from_shared(endpoint.to_string())
                    .map_err(|e| e.to_string())?
                    .connect(),
            )
            .map_err(|e| format!("Failed to connect to {endpoint}: {e}"))?;

        Ok(Box::new(Self {
            runtime,
            logbook: LogbookServiceClient::new(channel.clone()),
            lookup: LookupServiceClient::new(channel.clone()),
            rig: RigControlServiceClient::new(channel.clone()),
            weather: SpaceWeatherServiceClient::new(channel),
        }))
    }

    /// Log a new QSO. Returns 0 on success.
    pub(crate) fn log_qso(&mut self, req: &QsrLogQsoRequest, out: &mut QsrLogQsoResult) -> i32 {
        let qso = match build_qso_record(req) {
            Ok(q) => q,
            Err(e) => {
                set_error(e);
                return -1;
            }
        };

        match self.runtime.block_on(self.logbook.log_qso(LogQsoRequest {
            qso: Some(qso),
            sync_to_qrz: false,
        })) {
            Ok(resp) => {
                str_to_buf(&resp.into_inner().local_id, &mut out.local_id);
                0
            }
            Err(e) => {
                set_error(format!("LogQso failed: {}", e.message()));
                -1
            }
        }
    }

    /// Update an existing QSO. Returns 0 on success.
    pub(crate) fn update_qso(&mut self, req: &QsrUpdateQsoRequest) -> i32 {
        let mut qso = match build_qso_record(&req.qso) {
            Ok(q) => q,
            Err(e) => {
                set_error(e);
                return -1;
            }
        };
        qso.local_id = buf_to_str(&req.local_id).to_string();

        match self
            .runtime
            .block_on(self.logbook.update_qso(UpdateQsoRequest {
                qso: Some(qso),
                sync_to_qrz: false,
            })) {
            Ok(_) => 0,
            Err(e) => {
                set_error(format!("UpdateQso failed: {}", e.message()));
                -1
            }
        }
    }

    /// Get a single QSO by local_id. Returns 0 on success.
    pub(crate) fn get_qso(&mut self, local_id: &str, out: &mut QsrQsoDetail) -> i32 {
        match self.runtime.block_on(self.logbook.get_qso(GetQsoRequest {
            local_id: local_id.to_string(),
        })) {
            Ok(resp) => {
                if let Some(qso) = resp.into_inner().qso {
                    populate_qso_detail(&qso, out);
                }
                0
            }
            Err(e) => {
                set_error(format!("GetQso failed: {}", e.message()));
                -1
            }
        }
    }

    /// Delete a QSO by local_id. Returns 0 on success.
    pub(crate) fn delete_qso(&mut self, local_id: &str) -> i32 {
        match self
            .runtime
            .block_on(self.logbook.delete_qso(DeleteQsoRequest {
                local_id: local_id.to_string(),
                delete_from_qrz: false,
            })) {
            Ok(_) => 0,
            Err(e) => {
                set_error(format!("DeleteQso failed: {}", e.message()));
                -1
            }
        }
    }

    /// List all QSOs. Returns a heap-allocated list.
    pub(crate) fn list_qsos(&mut self, out: &mut QsrQsoList) -> i32 {
        let stream_result = self
            .runtime
            .block_on(self.logbook.list_qsos(ListQsosRequest {
                limit: 0,
                ..Default::default()
            }));

        let mut stream = match stream_result {
            Ok(resp) => resp.into_inner(),
            Err(e) => {
                set_error(format!("ListQsos failed: {}", e.message()));
                return -1;
            }
        };

        let mut items: Vec<QsrQsoSummary> = Vec::new();

        loop {
            match self.runtime.block_on(stream.message()) {
                Ok(Some(resp)) => {
                    if let Some(qso) = resp.qso {
                        items.push(qso_to_summary(&qso));
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    set_error(format!("ListQsos stream error: {}", e.message()));
                    return -1;
                }
            }
        }

        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let count = items.len() as i32;

        if items.is_empty() {
            out.items = std::ptr::null_mut();
            out.count = 0;
        } else {
            let boxed = items.into_boxed_slice();
            out.count = count;
            out.items = Box::into_raw(boxed).cast::<QsrQsoSummary>();
        }

        0
    }

    /// Lookup a callsign. Returns 0 on success.
    pub(crate) fn lookup(&mut self, callsign: &str, out: &mut QsrLookupResult) -> i32 {
        match self.runtime.block_on(self.lookup.lookup(LookupRequest {
            callsign: callsign.to_string(),
            skip_cache: false,
        })) {
            Ok(resp) => {
                let inner = resp.into_inner();
                if let Some(result) = inner.result {
                    populate_lookup_result(&result, out);
                }
                0
            }
            Err(e) => {
                set_error(format!("Lookup failed: {}", e.message()));
                -1
            }
        }
    }

    /// Get rig snapshot. Returns 0 on success.
    pub(crate) fn get_rig_snapshot(&mut self, out: &mut QsrRigStatus) -> i32 {
        match self
            .runtime
            .block_on(self.rig.get_rig_snapshot(GetRigSnapshotRequest {}))
        {
            Ok(resp) => {
                if let Some(snapshot) = resp.into_inner().snapshot {
                    populate_rig_status(&snapshot, out);
                }
                0
            }
            Err(e) => {
                set_error(format!("GetRigSnapshot failed: {}", e.message()));
                -1
            }
        }
    }

    /// Get current space weather. Returns 0 on success.
    pub(crate) fn get_space_weather(&mut self, out: &mut QsrSpaceWeather) -> i32 {
        match self.runtime.block_on(
            self.weather
                .get_current_space_weather(GetCurrentSpaceWeatherRequest {}),
        ) {
            Ok(resp) => {
                if let Some(snapshot) = resp.into_inner().snapshot {
                    out.has_data = 1;
                    out.k_index = snapshot.planetary_k_index.unwrap_or(0.0);
                    out.solar_flux = snapshot.solar_flux_index.unwrap_or(0.0);
                    #[allow(clippy::cast_possible_wrap)]
                    {
                        out.sunspot_number = snapshot.sunspot_number.unwrap_or(0) as i32;
                    }
                }
                0
            }
            Err(e) => {
                set_error(format!("GetSpaceWeather failed: {}", e.message()));
                -1
            }
        }
    }
}

// ── Conversion helpers ──────────────────────────────────────────────────

/// Build a `QsoRecord` proto from the FFI request struct.
fn build_qso_record(req: &QsrLogQsoRequest) -> Result<QsoRecord, String> {
    let callsign = buf_to_str(&req.callsign);
    let band_str = buf_to_str(&req.band);
    let mode_str = buf_to_str(&req.mode);
    let datetime_str = buf_to_str(&req.datetime);

    let band = band_from_adif(&band_str.to_uppercase())
        .ok_or_else(|| format!("Unknown band: {band_str}"))?;
    let mode = mode_from_adif(&mode_str.to_uppercase())
        .ok_or_else(|| format!("Unknown mode: {mode_str}"))?;

    let timestamp = parse_datetime(datetime_str)?;

    let mut qso = QsoRecord {
        worked_callsign: callsign.to_uppercase(),
        band: band.into(),
        mode: mode.into(),
        utc_timestamp: Some(timestamp),
        ..Default::default()
    };

    // RST
    if req.rst_sent.readability > 0 {
        qso.rst_sent = Some(build_rst_report(&req.rst_sent));
    }

    if req.rst_rcvd.readability > 0 {
        qso.rst_received = Some(build_rst_report(&req.rst_rcvd));
    }

    if req.freq_khz > 0 {
        qso.frequency_khz = Some(req.freq_khz);
    }

    set_optional_str(&req.comment, |s| qso.comment = Some(s.to_string()));
    set_optional_str(&req.notes, |s| qso.notes = Some(s.to_string()));
    set_optional_str(&req.worked_name, |s| {
        qso.worked_operator_name = Some(s.to_string());
    });
    set_optional_str(&req.tx_power, |s| qso.tx_power = Some(s.to_string()));
    set_optional_str(&req.submode, |s| qso.submode = Some(s.to_string()));
    set_optional_str(&req.contest_id, |s| qso.contest_id = Some(s.to_string()));
    set_optional_str(&req.serial_sent, |s| qso.serial_sent = Some(s.to_string()));
    set_optional_str(&req.serial_rcvd, |s| {
        qso.serial_received = Some(s.to_string());
    });
    set_optional_str(&req.exchange_sent, |s| {
        qso.exchange_sent = Some(s.to_string());
    });
    set_optional_str(&req.exchange_rcvd, |s| {
        qso.exchange_received = Some(s.to_string());
    });
    set_optional_str(&req.prop_mode, |s| qso.prop_mode = Some(s.to_string()));
    set_optional_str(&req.sat_name, |s| qso.sat_name = Some(s.to_string()));
    set_optional_str(&req.sat_mode, |s| qso.sat_mode = Some(s.to_string()));
    set_optional_str(&req.iota, |s| qso.worked_iota = Some(s.to_string()));
    set_optional_str(&req.arrl_section, |s| {
        qso.worked_arrl_section = Some(s.to_string());
    });
    set_optional_str(&req.worked_state, |s| {
        qso.worked_state = Some(s.to_string());
    });
    set_optional_str(&req.worked_county, |s| {
        qso.worked_county = Some(s.to_string());
    });
    set_optional_str(&req.skcc, |s| qso.skcc = Some(s.to_string()));

    let time_off = buf_to_str(&req.time_off);
    if !time_off.is_empty() {
        if let Ok(ts) = parse_datetime(time_off) {
            qso.utc_end_timestamp = Some(ts);
        }
    }

    Ok(qso)
}

/// If the buffer is non-empty, call the setter with the string value.
fn set_optional_str(buf: &[u8], setter: impl FnOnce(&str)) {
    let s = buf_to_str(buf);
    if !s.is_empty() {
        setter(s);
    }
}

/// Build a proto `RstReport` from the FFI `QsrRstReport`.
#[allow(clippy::cast_sign_loss)]
fn build_rst_report(rst: &QsrRstReport) -> RstReport {
    let r = rst.readability as u32;
    let s = rst.strength as u32;
    let t = rst.tone as u32;
    let raw = if t > 0 {
        format!("{r}{s}{t}")
    } else {
        format!("{r}{s}")
    };
    RstReport {
        readability: Some(r),
        strength: Some(s),
        tone: if t > 0 { Some(t) } else { None },
        raw,
    }
}

/// Parse "YYYY-MM-DD HH:MM" or "YYYY-MM-DD HH:MM:SS" into a protobuf Timestamp.
fn parse_datetime(s: &str) -> Result<prost_types::Timestamp, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Empty datetime".to_string());
    }

    // Try "YYYY-MM-DD HH:MM:SS" then "YYYY-MM-DD HH:MM"
    let parts: Vec<&str> = s.splitn(2, ' ').collect();
    if parts.len() < 2 {
        return Err(format!("Invalid datetime format: {s}"));
    }

    let date_parts: Vec<&str> = parts[0].split('-').collect();
    if date_parts.len() != 3 {
        return Err(format!("Invalid date: {}", parts[0]));
    }

    let year: i32 = date_parts[0]
        .parse()
        .map_err(|_| format!("Invalid year: {}", date_parts[0]))?;
    let month: u32 = date_parts[1]
        .parse()
        .map_err(|_| format!("Invalid month: {}", date_parts[1]))?;
    let day: u32 = date_parts[2]
        .parse()
        .map_err(|_| format!("Invalid day: {}", date_parts[2]))?;

    let time_parts: Vec<&str> = parts[1].split(':').collect();
    if time_parts.len() < 2 {
        return Err(format!("Invalid time: {}", parts[1]));
    }

    let hour: u32 = time_parts[0]
        .parse()
        .map_err(|_| format!("Invalid hour: {}", time_parts[0]))?;
    let minute: u32 = time_parts[1]
        .parse()
        .map_err(|_| format!("Invalid minute: {}", time_parts[1]))?;
    let second: u32 = if time_parts.len() > 2 {
        time_parts[2].parse().unwrap_or(0)
    } else {
        0
    };

    // Calculate Unix timestamp from components (UTC)
    #[allow(clippy::cast_possible_wrap)]
    let days = days_from_civil(year, month, day);
    #[allow(clippy::cast_possible_wrap)]
    let secs = i64::from(days) * 86400
        + i64::from(hour) * 3600
        + i64::from(minute) * 60
        + i64::from(second);

    Ok(prost_types::Timestamp {
        seconds: secs,
        nanos: 0,
    })
}

/// Days from civil date (Chrono-free algorithm from Howard Hinnant).
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
fn days_from_civil(year: i32, month: u32, day: u32) -> i32 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let m = month;
    let doy = if m > 2 {
        (153 * (m - 3) + 2) / 5 + day - 1
    } else {
        (153 * (m + 9) + 2) / 5 + day - 1
    };
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i32 - 719_468
}

/// Convert a proto `QsoRecord` to a `QsrQsoSummary`.
fn qso_to_summary(qso: &QsoRecord) -> QsrQsoSummary {
    let mut s = QsrQsoSummary {
        utc: [0; 24],
        callsign: [0; 16],
        band: [0; 8],
        mode: [0; 8],
        rst_sent: [0; 8],
        rst_rcvd: [0; 8],
        country: [0; 32],
        grid: [0; 8],
        local_id: [0; 64],
    };

    // Format UTC time as HH:MM
    if let Some(ts) = &qso.utc_timestamp {
        let total_secs = ts.seconds;
        let day_secs = total_secs.rem_euclid(86400);
        let h = day_secs / 3600;
        let m = (day_secs % 3600) / 60;
        let utc_str = format!("{h:02}:{m:02}");
        str_to_buf(&utc_str, &mut s.utc);
    }

    str_to_buf(&qso.worked_callsign, &mut s.callsign);

    let band = Band::try_from(qso.band).unwrap_or(Band::Unspecified);
    if let Some(band_str) = band_to_adif(band) {
        str_to_buf(band_str, &mut s.band);
    }

    let mode = Mode::try_from(qso.mode).unwrap_or(Mode::Unspecified);
    if let Some(mode_str) = mode_to_adif(mode) {
        str_to_buf(mode_str, &mut s.mode);
    }

    // RST sent
    if let Some(rst) = &qso.rst_sent {
        let rst_str = format_rst(rst);
        str_to_buf(&rst_str, &mut s.rst_sent);
    }

    // RST received
    if let Some(rst) = &qso.rst_received {
        let rst_str = format_rst(rst);
        str_to_buf(&rst_str, &mut s.rst_rcvd);
    }

    if let Some(country) = &qso.worked_country {
        str_to_buf(country, &mut s.country);
    }

    if let Some(grid) = &qso.worked_grid {
        str_to_buf(grid, &mut s.grid);
    }

    str_to_buf(&qso.local_id, &mut s.local_id);

    s
}

/// Format an RST report as a string like "59" or "599".
fn format_rst(rst: &RstReport) -> String {
    let r = rst.readability.unwrap_or(0);
    let s = rst.strength.unwrap_or(0);
    match rst.tone {
        Some(t) if t > 0 => format!("{r}{s}{t}"),
        _ => format!("{r}{s}"),
    }
}

fn populate_qso_optional_fields(qso: &QsoRecord, out: &mut QsrQsoDetail) {
    if let Some(v) = &qso.comment {
        str_to_buf(v, &mut out.comment);
    }
    if let Some(v) = &qso.notes {
        str_to_buf(v, &mut out.notes);
    }
    if let Some(v) = &qso.worked_operator_name {
        str_to_buf(v, &mut out.worked_name);
    }
    if let Some(v) = &qso.tx_power {
        str_to_buf(v, &mut out.tx_power);
    }
    if let Some(v) = &qso.submode {
        str_to_buf(v, &mut out.submode);
    }
    if let Some(v) = &qso.contest_id {
        str_to_buf(v, &mut out.contest_id);
    }
    if let Some(v) = &qso.serial_sent {
        str_to_buf(v, &mut out.serial_sent);
    }
    if let Some(v) = &qso.serial_received {
        str_to_buf(v, &mut out.serial_rcvd);
    }
    if let Some(v) = &qso.exchange_sent {
        str_to_buf(v, &mut out.exchange_sent);
    }
    if let Some(v) = &qso.exchange_received {
        str_to_buf(v, &mut out.exchange_rcvd);
    }
    if let Some(v) = &qso.prop_mode {
        str_to_buf(v, &mut out.prop_mode);
    }
    if let Some(v) = &qso.sat_name {
        str_to_buf(v, &mut out.sat_name);
    }
    if let Some(v) = &qso.sat_mode {
        str_to_buf(v, &mut out.sat_mode);
    }
    if let Some(v) = &qso.worked_iota {
        str_to_buf(v, &mut out.iota);
    }
    if let Some(v) = &qso.worked_arrl_section {
        str_to_buf(v, &mut out.arrl_section);
    }
    if let Some(v) = &qso.worked_state {
        str_to_buf(v, &mut out.worked_state);
    }
    if let Some(v) = &qso.worked_county {
        str_to_buf(v, &mut out.worked_county);
    }
    if let Some(v) = &qso.skcc {
        str_to_buf(v, &mut out.skcc);
    }
}

/// Populate a `QsrQsoDetail` from a proto `QsoRecord`.
fn populate_qso_detail(qso: &QsoRecord, out: &mut QsrQsoDetail) {
    *out = QsrQsoDetail {
        callsign: [0; 32],
        band: [0; 8],
        mode: [0; 8],
        date: [0; 16],
        time: [0; 16],
        freq_mhz: [0; 16],
        rst_sent: [0; 8],
        rst_rcvd: [0; 8],
        comment: [0; 256],
        notes: [0; 256],
        local_id: [0; 64],
        time_off: [0; 16],
        worked_name: [0; 64],
        tx_power: [0; 16],
        submode: [0; 16],
        contest_id: [0; 32],
        serial_sent: [0; 16],
        serial_rcvd: [0; 16],
        exchange_sent: [0; 64],
        exchange_rcvd: [0; 64],
        prop_mode: [0; 16],
        sat_name: [0; 32],
        sat_mode: [0; 16],
        iota: [0; 16],
        arrl_section: [0; 16],
        worked_state: [0; 16],
        worked_county: [0; 32],
        skcc: [0; 16],
    };

    str_to_buf(&qso.worked_callsign, &mut out.callsign);
    str_to_buf(&qso.local_id, &mut out.local_id);

    let band = Band::try_from(qso.band).unwrap_or(Band::Unspecified);
    if let Some(band_str) = band_to_adif(band) {
        str_to_buf(band_str, &mut out.band);
    }

    let mode = Mode::try_from(qso.mode).unwrap_or(Mode::Unspecified);
    if let Some(mode_str) = mode_to_adif(mode) {
        str_to_buf(mode_str, &mut out.mode);
    }

    // Date and time from timestamp
    if let Some(ts) = &qso.utc_timestamp {
        let total_secs = ts.seconds;
        let days = (total_secs / 86400) + 719_468;
        let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
        let doe = days - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };

        let date_str = format!("{y:04}-{m:02}-{d:02}");
        str_to_buf(&date_str, &mut out.date);

        let day_secs = total_secs.rem_euclid(86400);
        let h = day_secs / 3600;
        let min = (day_secs % 3600) / 60;
        let time_str = format!("{h:02}:{min:02}");
        str_to_buf(&time_str, &mut out.time);
    }

    // Frequency
    if let Some(freq_khz) = qso.frequency_khz {
        if freq_khz > 0 {
            let mhz = freq_khz as f64 / 1000.0;
            let freq_str = format!("{mhz:.5}");
            str_to_buf(&freq_str, &mut out.freq_mhz);
        }
    }

    // RST
    if let Some(rst) = &qso.rst_sent {
        str_to_buf(&format_rst(rst), &mut out.rst_sent);
    }
    if let Some(rst) = &qso.rst_received {
        str_to_buf(&format_rst(rst), &mut out.rst_rcvd);
    }

    // Optional string fields
    populate_qso_optional_fields(qso, out);

    // Time off
    if let Some(ts) = &qso.utc_end_timestamp {
        let day_secs = ts.seconds.rem_euclid(86400);
        let h = day_secs / 3600;
        let min = (day_secs % 3600) / 60;
        let time_off_str = format!("{h:02}:{min:02}");
        str_to_buf(&time_off_str, &mut out.time_off);
    }
}

/// Populate a `QsrLookupResult` from a proto `LookupResult`.
fn populate_lookup_result(
    result: &qsoripper_core::proto::qsoripper::domain::LookupResult,
    out: &mut QsrLookupResult,
) {
    use qsoripper_core::proto::qsoripper::domain::LookupState;

    *out = QsrLookupResult {
        has_data: 0,
        not_found: 0,
        error_msg: [0; 128],
        name: [0; 64],
        qth: [0; 64],
        grid: [0; 16],
        country: [0; 64],
        cq_zone: 0,
    };

    match result.state() {
        LookupState::NotFound => {
            out.not_found = 1;
            return;
        }
        LookupState::Error => {
            if let Some(msg) = &result.error_message {
                str_to_buf(msg, &mut out.error_msg);
            } else {
                str_to_buf("Lookup error", &mut out.error_msg);
            }
            return;
        }
        _ => {}
    }

    if result.state() != LookupState::Found {
        return;
    }

    if let Some(record) = &result.record {
        out.has_data = 1;

        // Prefer formatted_name, fallback to first_name
        if let Some(name) = &record.formatted_name {
            str_to_buf(name, &mut out.name);
        } else if !record.first_name.is_empty() {
            str_to_buf(&record.first_name, &mut out.name);
        }

        // QTH: prefer addr2, fallback to state
        if let Some(addr2) = &record.addr2 {
            str_to_buf(addr2, &mut out.qth);
        }

        if let Some(grid) = &record.grid_square {
            str_to_buf(grid, &mut out.grid);
        }

        if let Some(country) = &record.country {
            str_to_buf(country, &mut out.country);
        }

        #[allow(clippy::cast_possible_wrap)]
        if let Some(cq) = record.cq_zone {
            out.cq_zone = cq as i32;
        }
    }
}

/// Populate a `QsrRigStatus` from a proto `RigSnapshot`.
fn populate_rig_status(
    snapshot: &qsoripper_core::proto::qsoripper::domain::RigSnapshot,
    out: &mut QsrRigStatus,
) {
    *out = QsrRigStatus {
        connected: 0,
        freq_display: [0; 32],
        freq_mhz: [0; 16],
        band: [0; 8],
        mode: [0; 16],
    };

    if snapshot.status() != RigConnectionStatus::Connected {
        return;
    }

    out.connected = 1;

    if snapshot.frequency_hz > 0 {
        let mhz = snapshot.frequency_hz as f64 / 1_000_000.0;
        let display = format_freq_radio_style(snapshot.frequency_hz);
        let mhz_str = format!("{mhz:.5}");
        str_to_buf(&display, &mut out.freq_display);
        str_to_buf(&mhz_str, &mut out.freq_mhz);
    }

    let band_enum = Band::try_from(snapshot.band).unwrap_or(Band::Unspecified);
    if let Some(band_str) = band_to_adif(band_enum) {
        str_to_buf(band_str, &mut out.band);
    }

    let mode_enum = Mode::try_from(snapshot.mode).unwrap_or(Mode::Unspecified);
    if let Some(mode_str) = mode_to_adif(mode_enum) {
        str_to_buf(mode_str, &mut out.mode);
    }
}

#[cfg(test)]
mod tests {
    use super::{buf_to_str, qso_to_summary};
    use qsoripper_core::proto::qsoripper::domain::{QsoRecord, StationSnapshot};

    #[test]
    fn qso_to_summary_does_not_use_station_snapshot_country_when_worked_country_missing() {
        let qso = QsoRecord {
            worked_callsign: "JA1ABC".to_string(),
            station_snapshot: Some(StationSnapshot {
                country: Some("United States".to_string()),
                ..StationSnapshot::default()
            }),
            ..QsoRecord::default()
        };

        let summary = qso_to_summary(&qso);

        assert_eq!("", buf_to_str(&summary.country));
    }

    #[test]
    fn qso_to_summary_uses_explicit_worked_country_when_present() {
        let qso = QsoRecord {
            worked_callsign: "JA1ABC".to_string(),
            worked_country: Some("Japan".to_string()),
            station_snapshot: Some(StationSnapshot {
                country: Some("United States".to_string()),
                ..StationSnapshot::default()
            }),
            ..QsoRecord::default()
        };

        let summary = qso_to_summary(&qso);

        assert_eq!("Japan", buf_to_str(&summary.country));
    }
}

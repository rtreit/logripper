//! gRPC client helpers: channel creation, QSO logging, listing, lookup, and space weather.

use anyhow::Context;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use tonic::transport::Channel;

use qsoripper_core::domain::band::{band_from_adif, band_to_adif};
use qsoripper_core::domain::mode::{mode_from_adif, mode_to_adif};
use qsoripper_core::proto::qsoripper::domain::{Band, LookupState, Mode, RstReport};
use qsoripper_core::proto::qsoripper::services::{
    logbook_service_client::LogbookServiceClient, lookup_service_client::LookupServiceClient,
    rig_control_service_client::RigControlServiceClient,
    space_weather_service_client::SpaceWeatherServiceClient, DeleteQsoRequest,
    GetCurrentSpaceWeatherRequest, GetRigSnapshotRequest, ListQsosRequest, LogQsoRequest,
    LookupRequest, UpdateQsoRequest,
};

use crate::app::{CallsignInfo, RecentQso, RigInfo, RigStatus, SpaceWeatherInfo};
use crate::form::{LogForm, BANDS, MODES};

/// Enrichment snapshot from a callsign lookup: `(grid, country, cq_zone, dxcc)`.
type LookupEnrichment = Option<(Option<String>, Option<String>, Option<u32>, Option<u32>)>;

/// Create a tonic transport channel connected to the given endpoint URI.
pub(crate) async fn create_channel(endpoint: &str) -> anyhow::Result<Channel> {
    Channel::from_shared(endpoint.to_string())
        .context("invalid endpoint URI")?
        .connect()
        .await
        .context("failed to connect to qsoripper-server")
}

/// Log a QSO from the form and return the engine-assigned `local_id`.
///
/// `lookup` carries enrichment from the callsign lookup: `(grid, country, cq_zone, dxcc)`.
pub(crate) async fn log_qso(
    channel: Channel,
    form: &LogForm,
    lookup: LookupEnrichment,
) -> anyhow::Result<String> {
    let mut client = LogbookServiceClient::new(channel);

    let band: Band = BANDS
        .get(form.band_idx)
        .and_then(|s| band_from_adif(s))
        .unwrap_or(Band::Unspecified);

    let mode_str = MODES.get(form.mode_idx).copied().unwrap_or("SSB");
    let (mode, submode) = resolve_mode(mode_str);

    let utc_timestamp = parse_timestamp(&form.date, &form.time).ok();
    let utc_end_timestamp = if form.time_off.is_empty() {
        None
    } else {
        parse_timestamp(&form.date, &form.time_off).ok()
    };

    let frequency_khz = form.frequency_mhz.parse::<f64>().ok().map(mhz_to_khz);

    let (worked_grid, worked_country, worked_cq_zone, worked_dxcc) =
        lookup.unwrap_or((None, None, None, None));

    let qso = qsoripper_core::proto::qsoripper::domain::QsoRecord {
        worked_callsign: form.callsign.to_uppercase(),
        band: i32::from(band),
        mode: i32::from(mode),
        utc_timestamp,
        utc_end_timestamp,
        frequency_khz,
        submode: if form.submode_override.is_empty() {
            submode.map(str::to_string)
        } else {
            Some(form.submode_override.clone())
        },
        rst_sent: parse_rst(&form.rst_sent),
        rst_received: parse_rst(&form.rst_rcvd),
        comment: opt_string(&form.comment),
        notes: opt_string(&form.notes),
        tx_power: opt_string(&form.tx_power),
        contest_id: opt_string(&form.contest_id),
        serial_sent: opt_string(&form.serial_sent),
        serial_received: opt_string(&form.serial_rcvd),
        exchange_sent: opt_string(&form.exchange_sent),
        exchange_received: opt_string(&form.exchange_rcvd),
        worked_grid,
        worked_country,
        worked_cq_zone,
        worked_dxcc,
        worked_operator_name: opt_string(&form.worked_name),
        skcc: opt_string(&form.skcc),
        ..Default::default()
    };

    let request = LogQsoRequest {
        qso: Some(qso),
        sync_to_qrz: false,
    };

    let response = client.log_qso(request).await?.into_inner();
    Ok(response.local_id)
}

/// Fetch the most recent `limit` QSOs from the logbook service.
pub(crate) async fn list_recent_qsos(
    channel: Channel,
    limit: u32,
) -> anyhow::Result<Vec<RecentQso>> {
    let mut client = LogbookServiceClient::new(channel);

    let request = ListQsosRequest {
        limit,
        offset: 0,
        sort: 0, // QSO_SORT_ORDER_NEWEST_FIRST
        ..Default::default()
    };

    let mut stream = client.list_qsos(request).await?.into_inner();
    let mut result = Vec::new();

    while let Some(response) = stream.message().await? {
        let Some(qso) = response.qso else { continue };

        let utc = qso
            .utc_timestamp
            .as_ref()
            .and_then(|ts| chrono::DateTime::from_timestamp(ts.seconds, 0))
            .map(|dt| dt.format("%H:%M").to_string())
            .unwrap_or_default();

        let band = Band::try_from(qso.band)
            .ok()
            .and_then(band_to_adif)
            .unwrap_or("?")
            .to_string();

        let mode = Mode::try_from(qso.mode)
            .ok()
            .and_then(mode_to_adif)
            .unwrap_or("?")
            .to_string();

        let rst_sent = qso
            .rst_sent
            .as_ref()
            .map(|r| r.raw.clone())
            .unwrap_or_default();
        let rst_rcvd = qso
            .rst_received
            .as_ref()
            .map(|r| r.raw.clone())
            .unwrap_or_default();

        result.push(RecentQso {
            local_id: qso.local_id.clone(),
            utc,
            callsign: qso.worked_callsign.clone(),
            band,
            mode,
            rst_sent,
            rst_rcvd,
            country: qso.worked_country.clone(),
            grid: qso.worked_grid.clone(),
            name: qso.worked_operator_name.clone(),
            source_record: qso,
        });
    }

    Ok(result)
}

/// Look up a callsign via the lookup service and return display-ready info.
pub(crate) async fn lookup_callsign(
    channel: Channel,
    callsign: &str,
) -> anyhow::Result<Option<CallsignInfo>> {
    let mut client = LookupServiceClient::new(channel);

    let request = LookupRequest {
        callsign: callsign.to_string(),
        skip_cache: false,
    };

    let response = client.lookup(request).await?.into_inner();

    let Some(result) = response.result else {
        return Ok(None);
    };

    if result.state != LookupState::Found as i32 {
        return Ok(None);
    }

    let Some(record) = result.record else {
        return Ok(None);
    };

    let name = record.formatted_name.or_else(|| {
        let full = format!("{} {}", record.first_name, record.last_name);
        let trimmed = full.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    Ok(Some(CallsignInfo {
        callsign: record.callsign,
        name,
        qth: record.addr2,
        grid: record.grid_square,
        country: record.country,
        cq_zone: record.cq_zone,
        dxcc: if record.dxcc_entity_id == 0 {
            None
        } else {
            Some(record.dxcc_entity_id)
        },
    }))
}

/// Fetch the current space weather snapshot.
pub(crate) async fn get_space_weather(
    channel: Channel,
) -> anyhow::Result<Option<SpaceWeatherInfo>> {
    let mut client = SpaceWeatherServiceClient::new(channel);

    let response = client
        .get_current_space_weather(GetCurrentSpaceWeatherRequest {})
        .await?
        .into_inner();

    let Some(snapshot) = response.snapshot else {
        return Ok(None);
    };

    Ok(Some(SpaceWeatherInfo {
        k_index: snapshot.planetary_k_index,
        solar_flux: snapshot.solar_flux_index,
        sunspot_number: snapshot.sunspot_number,
    }))
}

/// Fetch the current rig snapshot from the rig control service.
pub(crate) async fn get_rig_snapshot(channel: Channel) -> anyhow::Result<Option<RigInfo>> {
    use qsoripper_core::proto::qsoripper::domain::RigConnectionStatus;

    let mut client = RigControlServiceClient::new(channel);

    let response = client
        .get_rig_snapshot(GetRigSnapshotRequest {})
        .await?
        .into_inner();

    let Some(snapshot) = response.snapshot else {
        return Ok(None);
    };

    let status = match RigConnectionStatus::try_from(snapshot.status) {
        Ok(RigConnectionStatus::Connected) => RigStatus::Connected,
        Ok(RigConnectionStatus::Error) => RigStatus::Error,
        Ok(RigConnectionStatus::Disabled) => RigStatus::Disabled,
        _ => RigStatus::Disconnected,
    };

    let band = Band::try_from(snapshot.band)
        .ok()
        .and_then(band_to_adif)
        .map(str::to_string);

    let mode = Mode::try_from(snapshot.mode)
        .ok()
        .and_then(mode_to_adif)
        .map(str::to_string);

    #[expect(
        clippy::cast_precision_loss,
        reason = "ham radio frequencies are well within f64 mantissa range"
    )]
    let freq_mhz = snapshot.frequency_hz as f64 / 1_000_000.0;
    let frequency_display = if snapshot.frequency_hz > 0 {
        format!("{freq_mhz:.3} MHz")
    } else {
        String::new()
    };

    Ok(Some(RigInfo {
        frequency_display,
        frequency_hz: snapshot.frequency_hz,
        band,
        mode,
        submode: snapshot.submode,
        status,
        error_message: snapshot.error_message,
    }))
}

/// Update an existing QSO record identified by `local_id` with data from the form.
///
/// `base` is the original `QsoRecord` loaded during editing. When present, form values
/// are overlaid on the clone so that non-form fields (QSL status, metadata, extra ADIF
/// fields, etc.) are preserved.  When `None`, falls back to a default record.
pub(crate) async fn update_qso(
    channel: Channel,
    local_id: &str,
    form: &LogForm,
    lookup: LookupEnrichment,
    base: Option<qsoripper_core::proto::qsoripper::domain::QsoRecord>,
) -> anyhow::Result<()> {
    let mut client = LogbookServiceClient::new(channel);

    let band: Band = BANDS
        .get(form.band_idx)
        .and_then(|s| band_from_adif(s))
        .unwrap_or(Band::Unspecified);

    let mode_str = MODES.get(form.mode_idx).copied().unwrap_or("SSB");
    let (mode, submode) = resolve_mode(mode_str);

    let utc_timestamp = parse_timestamp(&form.date, &form.time).ok();
    let utc_end_timestamp = if form.time_off.is_empty() {
        None
    } else {
        parse_timestamp(&form.date, &form.time_off).ok()
    };
    let frequency_khz = form.frequency_mhz.parse::<f64>().ok().map(mhz_to_khz);

    let (worked_grid, worked_country, worked_cq_zone, worked_dxcc) =
        lookup.unwrap_or((None, None, None, None));

    // Start from the original record to preserve non-form fields, then overlay
    // every field that the edit form controls.
    let mut qso = base.unwrap_or_default();
    qso.local_id = local_id.to_string();
    qso.worked_callsign = form.callsign.to_uppercase();
    qso.band = i32::from(band);
    qso.mode = i32::from(mode);
    qso.utc_timestamp = utc_timestamp;
    qso.utc_end_timestamp = utc_end_timestamp;
    qso.frequency_khz = frequency_khz;
    qso.submode = if form.submode_override.is_empty() {
        submode.map(str::to_string)
    } else {
        Some(form.submode_override.clone())
    };
    qso.rst_sent = parse_rst(&form.rst_sent);
    qso.rst_received = parse_rst(&form.rst_rcvd);
    qso.comment = opt_string(&form.comment);
    qso.notes = opt_string(&form.notes);
    qso.tx_power = opt_string(&form.tx_power);
    qso.contest_id = opt_string(&form.contest_id);
    qso.serial_sent = opt_string(&form.serial_sent);
    qso.serial_received = opt_string(&form.serial_rcvd);
    qso.exchange_sent = opt_string(&form.exchange_sent);
    qso.exchange_received = opt_string(&form.exchange_rcvd);
    qso.worked_grid = worked_grid;
    qso.worked_country = worked_country;
    qso.worked_cq_zone = worked_cq_zone;
    qso.worked_dxcc = worked_dxcc;
    qso.worked_operator_name = opt_string(&form.worked_name);
    qso.worked_iota = opt_string(&form.iota);
    qso.worked_arrl_section = opt_string(&form.arrl_section);
    qso.worked_state = opt_string(&form.worked_state);
    qso.worked_county = opt_string(&form.worked_county);
    qso.skcc = opt_string(&form.skcc);
    qso.prop_mode = opt_string(&form.prop_mode);
    qso.sat_name = opt_string(&form.sat_name);
    qso.sat_mode = opt_string(&form.sat_mode);

    client
        .update_qso(UpdateQsoRequest {
            qso: Some(qso),
            sync_to_qrz: false,
        })
        .await?;
    Ok(())
}

/// Delete a QSO by its local ID.
pub(crate) async fn delete_qso(channel: Channel, local_id: &str) -> anyhow::Result<()> {
    let mut client = LogbookServiceClient::new(channel);
    let request = DeleteQsoRequest {
        local_id: local_id.to_string(),
        delete_from_qrz: false,
    };
    client.delete_qso(request).await?;
    Ok(())
}

/// Convert a frequency in MHz to kHz as a `u64`.
fn mhz_to_khz(mhz: f64) -> u64 {
    let khz = mhz * 1_000.0_f64;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "frequency is always a small positive value well within u64 range"
    )]
    {
        khz as u64
    }
}

/// Return `Some(s.to_string())` if non-empty, `None` otherwise.
fn opt_string(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Parse a date string (`YYYY-MM-DD`) and time string (`HH:MM`) into a protobuf timestamp.
fn parse_timestamp(date: &str, time: &str) -> anyhow::Result<prost_types::Timestamp> {
    let naive_date = NaiveDate::parse_from_str(date, "%Y-%m-%d").context("invalid date")?;
    let naive_time =
        NaiveTime::parse_from_str(&format!("{time}:00"), "%H:%M:%S").context("invalid time")?;
    let naive = NaiveDateTime::new(naive_date, naive_time);
    let seconds = naive.and_utc().timestamp();
    Ok(prost_types::Timestamp { seconds, nanos: 0 })
}

/// Parse an RST string (e.g., `"59"` or `"599"`) into an [`RstReport`].
fn parse_rst(s: &str) -> Option<RstReport> {
    if s.is_empty() {
        return None;
    }
    let digits: Vec<u32> = s.chars().filter_map(|c| c.to_digit(10)).collect();
    let raw = s.to_string();
    let report = match digits.as_slice() {
        [r, st] => RstReport {
            readability: Some(*r),
            strength: Some(*st),
            tone: None,
            raw,
        },
        [r, st, t] => RstReport {
            readability: Some(*r),
            strength: Some(*st),
            tone: Some(*t),
            raw,
        },
        _ => RstReport {
            readability: None,
            strength: None,
            tone: None,
            raw,
        },
    };
    Some(report)
}

/// Map a MODES display string to a proto [`Mode`] enum value plus an optional submode string.
fn resolve_mode(mode_str: &str) -> (Mode, Option<&'static str>) {
    match mode_str {
        "FT4" => (Mode::Mfsk, Some("FT4")),
        "PSK31" => (Mode::Psk, Some("PSK31")),
        s => (mode_from_adif(s).unwrap_or(Mode::Unspecified), None),
    }
}

//! Maps `difa::Record` values to the proto `QsoRecord` and back.
//!
//! This is the edge adapter between the ADIF file format and `QsoRipper`'s
//! internal domain types. All ADIF-specific logic is contained here.

use std::{borrow::Cow, fmt::Write as _};

use crate::adif::normalize::{enrich_from_dxcc, parse_rst_report};
use crate::domain::band::band_from_adif;
use crate::domain::mode::normalize_mode_from_adif;
use crate::domain::qso::{new_local_id, qsl_status_from_adif};
use crate::domain::station::{effective_station_snapshot, station_snapshot_has_values};
use crate::proto::qsoripper::domain::{Band, Mode, QsoRecord, StationSnapshot, SyncStatus};
use difa::Record;

/// Borrowed or owned ADIF field data suitable for ADI serialization.
type AdifField<'a> = (Cow<'a, str>, Cow<'a, str>);

/// Maps ADIF records to and from proto `QsoRecord` values.
pub struct AdifMapper;

impl AdifMapper {
    /// Convert a `difa::Record` (parsed ADIF QSO) into a proto `QsoRecord`.
    ///
    /// Recognized ADIF fields are mapped to dedicated `QsoRecord` fields.
    /// Unrecognized fields are stored in `extra_fields` for round-trip fidelity.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn record_to_qso(record: &Record) -> QsoRecord {
        let mut qso = QsoRecord {
            local_id: new_local_id(),
            sync_status: SyncStatus::LocalOnly.into(),
            ..Default::default()
        };
        let mut station_snapshot: Option<StationSnapshot> = None;

        // Collect QSO_DATE and TIME_ON separately for timestamp construction
        let mut qso_date: Option<Cow<'_, str>> = None;
        let mut time_on: Option<Cow<'_, str>> = None;
        let mut qso_date_off: Option<Cow<'_, str>> = None;
        let mut time_off: Option<Cow<'_, str>> = None;

        for (key, datum) in record.fields() {
            let value = datum.as_str();
            let value_str = value.as_ref();
            let key_upper = key.to_uppercase();

            match key_upper.as_str() {
                // --- Core fields ---
                "CALL" => value_str.clone_into(&mut qso.worked_callsign),
                "STATION_CALLSIGN" => {
                    value_str.clone_into(&mut qso.station_callsign);
                    value_str.clone_into(
                        &mut station_snapshot
                            .get_or_insert_with(StationSnapshot::default)
                            .station_callsign,
                    );
                }
                "OPERATOR" => {
                    station_snapshot
                        .get_or_insert_with(StationSnapshot::default)
                        .operator_callsign = Some(value_str.to_owned());
                    if qso.station_callsign.is_empty() {
                        value_str.clone_into(&mut qso.station_callsign);
                        value_str.clone_into(
                            &mut station_snapshot
                                .get_or_insert_with(StationSnapshot::default)
                                .station_callsign,
                        );
                    }
                }
                "QSO_DATE" => qso_date = Some(value.clone()),
                "TIME_ON" => time_on = Some(value.clone()),
                "QSO_DATE_OFF" => qso_date_off = Some(value.clone()),
                "TIME_OFF" => time_off = Some(value.clone()),
                "BAND" => {
                    if let Some(band) = band_from_adif(value_str) {
                        qso.band = band.into();
                    } else {
                        qso.extra_fields.insert(key_upper, value_str.to_owned());
                    }
                }
                "MODE" => {
                    if let Some((mode, submode)) = normalize_mode_from_adif(value_str) {
                        qso.mode = mode.into();
                        if let Some(submode) = submode.filter(|_| qso.submode.is_none()) {
                            qso.submode = Some(submode.to_owned());
                        }
                    } else {
                        qso.extra_fields.insert(key_upper, value_str.to_owned());
                    }
                }
                "SUBMODE" => qso.submode = Some(value_str.to_owned()),
                "FREQ" => {
                    if let Ok(mhz) = value_str.parse::<f64>() {
                        if let Some(khz) = mhz_to_khz(mhz) {
                            qso.frequency_khz = Some(khz);
                        } else {
                            qso.extra_fields.insert(key_upper, value_str.to_owned());
                        }
                    } else {
                        qso.extra_fields.insert(key_upper, value_str.to_owned());
                    }
                }
                // --- Signal reports ---
                "RST_SENT" => qso.rst_sent = Some(parse_rst_report(value_str)),
                "RST_RCVD" => qso.rst_received = Some(parse_rst_report(value_str)),
                "TX_PWR" => qso.tx_power = Some(value_str.to_owned()),

                // --- Geographic / enrichment ---
                "CONTACTED_OP" => qso.worked_operator_callsign = Some(value_str.to_owned()),
                "NAME" => qso.worked_operator_name = Some(value_str.to_owned()),
                "GRIDSQUARE" => qso.worked_grid = Some(value_str.to_owned()),
                "COUNTRY" => qso.worked_country = Some(value_str.to_owned()),
                "DXCC" => {
                    if let Ok(code) = value_str.parse::<u32>() {
                        qso.worked_dxcc = Some(code);
                    }
                }
                "STATE" => qso.worked_state = Some(value_str.to_owned()),
                "CNTY" => qso.worked_county = Some(value_str.to_owned()),
                "CONT" => qso.worked_continent = Some(value_str.to_owned()),
                "CQZ" => {
                    if let Ok(zone) = value_str.parse::<u32>() {
                        qso.worked_cq_zone = Some(zone);
                    }
                }
                "ITUZ" => {
                    if let Ok(zone) = value_str.parse::<u32>() {
                        qso.worked_itu_zone = Some(zone);
                    }
                }
                "IOTA" => qso.worked_iota = Some(value_str.to_owned()),
                "ARRL_SECT" => qso.worked_arrl_section = Some(value_str.to_owned()),
                "SKCC" => qso.skcc = Some(value_str.to_owned()),
                "MY_NAME" => {
                    station_snapshot
                        .get_or_insert_with(StationSnapshot::default)
                        .operator_name = Some(value_str.to_owned());
                }
                "MY_GRIDSQUARE" => {
                    station_snapshot
                        .get_or_insert_with(StationSnapshot::default)
                        .grid = Some(value_str.to_owned());
                }
                "MY_CNTY" => {
                    station_snapshot
                        .get_or_insert_with(StationSnapshot::default)
                        .county = Some(value_str.to_owned());
                }
                "MY_STATE" => {
                    station_snapshot
                        .get_or_insert_with(StationSnapshot::default)
                        .state = Some(value_str.to_owned());
                }
                "MY_COUNTRY" => {
                    station_snapshot
                        .get_or_insert_with(StationSnapshot::default)
                        .country = Some(value_str.to_owned());
                }
                "MY_DXCC" => {
                    if let Ok(dxcc) = value_str.parse::<u32>() {
                        station_snapshot
                            .get_or_insert_with(StationSnapshot::default)
                            .dxcc = Some(dxcc);
                    } else {
                        qso.extra_fields.insert(key_upper, value_str.to_owned());
                    }
                }
                "MY_CQ_ZONE" => {
                    if let Ok(zone) = value_str.parse::<u32>() {
                        station_snapshot
                            .get_or_insert_with(StationSnapshot::default)
                            .cq_zone = Some(zone);
                    } else {
                        qso.extra_fields.insert(key_upper, value_str.to_owned());
                    }
                }
                "MY_ITU_ZONE" => {
                    if let Ok(zone) = value_str.parse::<u32>() {
                        station_snapshot
                            .get_or_insert_with(StationSnapshot::default)
                            .itu_zone = Some(zone);
                    } else {
                        qso.extra_fields.insert(key_upper, value_str.to_owned());
                    }
                }
                "MY_LAT" => {
                    if let Some(latitude) = parse_adif_location(value_str, true) {
                        station_snapshot
                            .get_or_insert_with(StationSnapshot::default)
                            .latitude = Some(latitude);
                    } else {
                        qso.extra_fields.insert(key_upper, value_str.to_owned());
                    }
                }
                "MY_LON" => {
                    if let Some(longitude) = parse_adif_location(value_str, false) {
                        station_snapshot
                            .get_or_insert_with(StationSnapshot::default)
                            .longitude = Some(longitude);
                    } else {
                        qso.extra_fields.insert(key_upper, value_str.to_owned());
                    }
                }
                "MY_ARRL_SECT" => {
                    station_snapshot
                        .get_or_insert_with(StationSnapshot::default)
                        .arrl_section = Some(value_str.to_owned());
                }

                // --- QSL ---
                "QSL_SENT" => qso.qsl_sent_status = qsl_status_from_adif(value_str).into(),
                "QSL_RCVD" => qso.qsl_received_status = qsl_status_from_adif(value_str).into(),
                "QSLSDATE" => {
                    if let Some(ts) = parse_adif_datetime(value_str, None) {
                        qso.qsl_sent_date = Some(ts);
                    } else {
                        qso.extra_fields.insert(key_upper, value_str.to_owned());
                    }
                }
                "QSLRDATE" => {
                    if let Some(ts) = parse_adif_datetime(value_str, None) {
                        qso.qsl_received_date = Some(ts);
                    } else {
                        qso.extra_fields.insert(key_upper, value_str.to_owned());
                    }
                }
                "LOTW_QSL_SENT" => map_confirmation_field(
                    &mut qso.lotw_sent,
                    &mut qso.extra_fields,
                    key_upper.as_str(),
                    value_str,
                ),
                "LOTW_QSL_RCVD" => map_confirmation_field(
                    &mut qso.lotw_received,
                    &mut qso.extra_fields,
                    key_upper.as_str(),
                    value_str,
                ),
                "EQSL_QSL_SENT" => map_confirmation_field(
                    &mut qso.eqsl_sent,
                    &mut qso.extra_fields,
                    key_upper.as_str(),
                    value_str,
                ),
                "EQSL_QSL_RCVD" => map_confirmation_field(
                    &mut qso.eqsl_received,
                    &mut qso.extra_fields,
                    key_upper.as_str(),
                    value_str,
                ),

                // --- Contest ---
                "CONTEST_ID" => qso.contest_id = Some(value_str.to_owned()),
                "SRX" => qso.serial_received = Some(value_str.to_owned()),
                "STX" => qso.serial_sent = Some(value_str.to_owned()),
                "SRX_STRING" => qso.exchange_received = Some(value_str.to_owned()),
                "STX_STRING" => qso.exchange_sent = Some(value_str.to_owned()),

                // --- Propagation ---
                "PROP_MODE" => qso.prop_mode = Some(value_str.to_owned()),
                "SAT_NAME" => qso.sat_name = Some(value_str.to_owned()),
                "SAT_MODE" => qso.sat_mode = Some(value_str.to_owned()),

                // --- Notes ---
                "COMMENT" => qso.comment = Some(value_str.to_owned()),
                "NOTES" => qso.notes = Some(value_str.to_owned()),

                // --- Everything else → extra_fields for round-trip ---
                _ => {
                    qso.extra_fields.insert(key_upper, value_str.to_owned());
                }
            }
        }

        // Combine QSO_DATE + TIME_ON into UTC timestamp
        if let Some(date_str) = qso_date.as_deref() {
            if let Some(ts) = parse_adif_datetime(date_str, time_on.as_deref()) {
                qso.utc_timestamp = Some(ts);
            } else {
                qso.extra_fields
                    .insert("QSO_DATE".to_string(), date_str.to_owned());
                if let Some(time_on) = time_on.as_deref() {
                    qso.extra_fields
                        .insert("TIME_ON".to_string(), time_on.to_owned());
                }
            }
        } else if let Some(time_on) = time_on.as_deref() {
            qso.extra_fields
                .insert("TIME_ON".to_string(), time_on.to_owned());
        }

        if let Some(time_off) = time_off.as_deref() {
            let end_date = qso_date_off.as_deref().or(qso_date.as_deref());
            if let Some(end_date) = end_date {
                if let Some(ts) = parse_adif_datetime(end_date, Some(time_off)) {
                    qso.utc_end_timestamp = Some(ts);
                } else {
                    if let Some(raw_date_off) = qso_date_off.as_deref() {
                        qso.extra_fields
                            .insert("QSO_DATE_OFF".to_string(), raw_date_off.to_owned());
                    }
                    qso.extra_fields
                        .insert("TIME_OFF".to_string(), time_off.to_owned());
                }
            } else {
                qso.extra_fields
                    .insert("TIME_OFF".to_string(), time_off.to_owned());
            }
        } else if let Some(raw_date_off) = qso_date_off.as_deref() {
            qso.extra_fields
                .insert("QSO_DATE_OFF".to_string(), raw_date_off.to_owned());
        }

        enrich_from_dxcc(&mut qso);

        if let Some(snapshot) = station_snapshot.filter(station_snapshot_has_values) {
            qso.station_snapshot = Some(snapshot);
        }

        qso
    }

    /// Convert a proto `QsoRecord` into a list of ADIF field key-value pairs.
    /// Suitable for generating ADI output.
    #[must_use]
    pub fn qso_to_adif_fields(qso: &QsoRecord) -> Vec<(String, String)> {
        Self::qso_to_adif_fields_borrowed(qso)
            .into_iter()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect()
    }

    /// Convert a proto `QsoRecord` directly into an ADI-formatted string.
    #[must_use]
    pub fn qso_to_adi(qso: &QsoRecord) -> String {
        let fields = Self::qso_to_adif_fields_borrowed(qso);
        Self::fields_to_adi(&fields)
    }

    #[must_use]
    #[allow(clippy::too_many_lines)]
    fn qso_to_adif_fields_borrowed(qso: &QsoRecord) -> Vec<AdifField<'_>> {
        let mut fields = Vec::with_capacity(qso.extra_fields.len() + 32);
        let station_snapshot = effective_station_snapshot(qso);

        // Core
        if let Some(station_callsign) = station_snapshot
            .as_ref()
            .map(|snapshot| snapshot.station_callsign.as_str())
            .filter(|value| !value.is_empty())
            .or_else(|| (!qso.station_callsign.is_empty()).then_some(qso.station_callsign.as_str()))
        {
            push_field(
                &mut fields,
                "STATION_CALLSIGN",
                station_callsign.to_string(),
            );
        }
        if !qso.worked_callsign.is_empty() {
            push_field(&mut fields, "CALL", qso.worked_callsign.as_str());
        }
        if let Some(operator_callsign) = station_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.operator_callsign.as_deref())
        {
            push_field(&mut fields, "OPERATOR", operator_callsign.to_string());
        }

        // Timestamp → QSO_DATE + TIME_ON
        if let Some(ref ts) = qso.utc_timestamp {
            if let Some((date_str, time_str)) = format_adif_datetime(ts) {
                push_field(&mut fields, "QSO_DATE", date_str);
                push_field(&mut fields, "TIME_ON", time_str);
            }
        }
        if let Some(ref ts) = qso.utc_end_timestamp {
            if let Some((date_str, time_str)) = format_adif_datetime(ts) {
                push_field(&mut fields, "QSO_DATE_OFF", date_str);
                push_field(&mut fields, "TIME_OFF", time_str);
            }
        }

        // Band
        let band = Band::try_from(qso.band).unwrap_or(Band::Unspecified);
        if let Some(band_str) = crate::domain::band::band_to_adif(band) {
            push_field(&mut fields, "BAND", band_str);
        }

        // Mode + submode
        let mode = Mode::try_from(qso.mode).unwrap_or(Mode::Unspecified);
        if let Some(mode_str) = crate::domain::mode::mode_to_adif(mode) {
            push_field(&mut fields, "MODE", mode_str);
        }
        if let Some(ref sub) = qso.submode {
            push_field(&mut fields, "SUBMODE", sub.as_str());
        }

        // Frequency
        if let Some(khz) = qso.frequency_khz {
            let whole_mhz = khz / 1000;
            let fractional_khz = khz % 1000;
            push_field(
                &mut fields,
                "FREQ",
                format!("{whole_mhz}.{fractional_khz:03}"),
            );
        }

        // Signal reports
        if let Some(ref rst) = qso.rst_sent {
            push_field(&mut fields, "RST_SENT", rst.raw.as_str());
        }
        if let Some(ref rst) = qso.rst_received {
            push_field(&mut fields, "RST_RCVD", rst.raw.as_str());
        }
        if let Some(ref pwr) = qso.tx_power {
            push_field(&mut fields, "TX_PWR", pwr.as_str());
        }

        // Geographic
        if let Some(v) = qso.worked_operator_callsign.as_deref() {
            push_field(&mut fields, "CONTACTED_OP", v);
        }
        if let Some(v) = qso.worked_operator_name.as_deref() {
            push_field(&mut fields, "NAME", v);
        }
        if let Some(v) = qso.worked_grid.as_deref() {
            push_field(&mut fields, "GRIDSQUARE", v);
        }
        if let Some(v) = qso.worked_country.as_deref() {
            push_field(&mut fields, "COUNTRY", v);
        }
        if let Some(dxcc) = qso.worked_dxcc {
            push_field(&mut fields, "DXCC", dxcc.to_string());
        }
        if let Some(v) = qso.worked_state.as_deref() {
            push_field(&mut fields, "STATE", v);
        }
        if let Some(v) = qso.worked_county.as_deref() {
            push_field(&mut fields, "CNTY", v);
        }
        if let Some(v) = qso.worked_continent.as_deref() {
            push_field(&mut fields, "CONT", v);
        }
        if let Some(z) = qso.worked_cq_zone {
            push_field(&mut fields, "CQZ", z.to_string());
        }
        if let Some(z) = qso.worked_itu_zone {
            push_field(&mut fields, "ITUZ", z.to_string());
        }
        if let Some(v) = qso.worked_iota.as_deref() {
            push_field(&mut fields, "IOTA", v);
        }
        if let Some(v) = qso.worked_arrl_section.as_deref() {
            push_field(&mut fields, "ARRL_SECT", v);
        }
        if let Some(v) = qso.skcc.as_deref() {
            push_field(&mut fields, "SKCC", v);
        }
        if let Some(snapshot) = station_snapshot.as_ref() {
            if let Some(v) = snapshot.operator_name.as_deref() {
                push_field(&mut fields, "MY_NAME", v.to_string());
            }
            if let Some(v) = snapshot.grid.as_deref() {
                push_field(&mut fields, "MY_GRIDSQUARE", v.to_string());
            }
            if let Some(v) = snapshot.county.as_deref() {
                push_field(&mut fields, "MY_CNTY", v.to_string());
            }
            if let Some(v) = snapshot.state.as_deref() {
                push_field(&mut fields, "MY_STATE", v.to_string());
            }
            if let Some(v) = snapshot.country.as_deref() {
                push_field(&mut fields, "MY_COUNTRY", v.to_string());
            }
            if let Some(dxcc) = snapshot.dxcc {
                push_field(&mut fields, "MY_DXCC", dxcc.to_string());
            }
            if let Some(zone) = snapshot.cq_zone {
                push_field(&mut fields, "MY_CQ_ZONE", zone.to_string());
            }
            if let Some(zone) = snapshot.itu_zone {
                push_field(&mut fields, "MY_ITU_ZONE", zone.to_string());
            }
            if let Some(latitude) = snapshot
                .latitude
                .and_then(|value| format_adif_location(value, true))
            {
                push_field(&mut fields, "MY_LAT", latitude);
            }
            if let Some(longitude) = snapshot
                .longitude
                .and_then(|value| format_adif_location(value, false))
            {
                push_field(&mut fields, "MY_LON", longitude);
            }
            if let Some(v) = snapshot.arrl_section.as_deref() {
                push_field(&mut fields, "MY_ARRL_SECT", v.to_string());
            }
        }

        // QSL
        let sent = crate::domain::qso::qsl_status_to_adif(
            crate::proto::qsoripper::domain::QslStatus::try_from(qso.qsl_sent_status)
                .unwrap_or(crate::proto::qsoripper::domain::QslStatus::Unspecified),
        );
        if let Some(s) = sent {
            push_field(&mut fields, "QSL_SENT", s);
        }

        let rcvd = crate::domain::qso::qsl_status_to_adif(
            crate::proto::qsoripper::domain::QslStatus::try_from(qso.qsl_received_status)
                .unwrap_or(crate::proto::qsoripper::domain::QslStatus::Unspecified),
        );
        if let Some(s) = rcvd {
            push_field(&mut fields, "QSL_RCVD", s);
        }
        if let Some(ts) = qso.qsl_sent_date.as_ref().and_then(format_adif_date) {
            push_field(&mut fields, "QSLSDATE", ts);
        }
        if let Some(ts) = qso.qsl_received_date.as_ref().and_then(format_adif_date) {
            push_field(&mut fields, "QSLRDATE", ts);
        }

        push_confirmation_field(&mut fields, "LOTW_QSL_SENT", qso.lotw_sent);
        push_confirmation_field(&mut fields, "LOTW_QSL_RCVD", qso.lotw_received);
        push_confirmation_field(&mut fields, "EQSL_QSL_SENT", qso.eqsl_sent);
        push_confirmation_field(&mut fields, "EQSL_QSL_RCVD", qso.eqsl_received);

        // Contest
        if let Some(v) = qso.contest_id.as_deref() {
            push_field(&mut fields, "CONTEST_ID", v);
        }
        if let Some(v) = qso.serial_sent.as_deref() {
            push_field(&mut fields, "STX", v);
        }
        if let Some(v) = qso.serial_received.as_deref() {
            push_field(&mut fields, "SRX", v);
        }
        if let Some(v) = qso.exchange_sent.as_deref() {
            push_field(&mut fields, "STX_STRING", v);
        }
        if let Some(v) = qso.exchange_received.as_deref() {
            push_field(&mut fields, "SRX_STRING", v);
        }

        // Propagation
        if let Some(v) = qso.prop_mode.as_deref() {
            push_field(&mut fields, "PROP_MODE", v);
        }
        if let Some(v) = qso.sat_name.as_deref() {
            push_field(&mut fields, "SAT_NAME", v);
        }
        if let Some(v) = qso.sat_mode.as_deref() {
            push_field(&mut fields, "SAT_MODE", v);
        }

        // Notes
        if let Some(v) = qso.comment.as_deref() {
            push_field(&mut fields, "COMMENT", v);
        }
        if let Some(v) = qso.notes.as_deref() {
            push_field(&mut fields, "NOTES", v);
        }

        // Extra fields (round-trip overflow)
        for (k, v) in &qso.extra_fields {
            if field_is_overridden(qso, station_snapshot.as_ref(), k) {
                continue;
            }
            push_field(&mut fields, k.as_str(), v.as_str());
        }

        fields
    }

    /// Generate an ADI-format string from ADIF field pairs.
    #[must_use]
    pub fn fields_to_adi<K, V>(fields: &[(K, V)]) -> String
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let capacity = fields.iter().fold(6, |acc, (key, value)| {
            acc + key.as_ref().len() + value.as_ref().len() + 16
        });
        let mut out = String::with_capacity(capacity);

        for (key, value) in fields {
            let key = key.as_ref();
            let value = value.as_ref();
            out.push('<');
            out.push_str(key);
            out.push(':');
            let _ = write!(out, "{}>", value.len());
            out.push_str(value);
            out.push('\n');
        }
        out.push_str("<eor>\n");
        out
    }
}

fn push_field<'a>(
    fields: &mut Vec<AdifField<'a>>,
    key: impl Into<Cow<'a, str>>,
    value: impl Into<Cow<'a, str>>,
) {
    fields.push((key.into(), value.into()));
}

/// Parse ADIF date (YYYYMMDD) + optional time (HHMM or HHMMSS) into prost Timestamp.
fn parse_adif_datetime(date_str: &str, time_str: Option<&str>) -> Option<prost_types::Timestamp> {
    if date_str.len() != 8 || !date_str.is_ascii() {
        return None;
    }

    if let Some(ts) = time_str {
        if !ts.is_ascii() {
            return None;
        }
    }

    let year: i32 = date_str[0..4].parse().ok()?;
    let month: u32 = date_str[4..6].parse().ok()?;
    let day: u32 = date_str[6..8].parse().ok()?;

    let date = chrono::NaiveDate::from_ymd_opt(year, month, day)?;

    let time = if let Some(ts) = time_str {
        match ts.len() {
            4 => {
                let h: u32 = ts[0..2].parse().ok()?;
                let m: u32 = ts[2..4].parse().ok()?;
                chrono::NaiveTime::from_hms_opt(h, m, 0)?
            }
            6 => {
                let h: u32 = ts[0..2].parse().ok()?;
                let m: u32 = ts[2..4].parse().ok()?;
                let s: u32 = ts[4..6].parse().ok()?;
                chrono::NaiveTime::from_hms_opt(h, m, s)?
            }
            _ => chrono::NaiveTime::from_hms_opt(0, 0, 0)?,
        }
    } else {
        chrono::NaiveTime::from_hms_opt(0, 0, 0)?
    };

    let dt = chrono::NaiveDateTime::new(date, time);
    let secs = dt.and_utc().timestamp();
    Some(prost_types::Timestamp {
        seconds: secs,
        nanos: 0,
    })
}

/// Format a prost Timestamp as ADIF date + time strings.
fn format_adif_datetime(ts: &prost_types::Timestamp) -> Option<(String, String)> {
    let nanos = u32::try_from(ts.nanos).ok()?;
    let dt = chrono::DateTime::from_timestamp(ts.seconds, nanos)?.naive_utc();
    let date = dt.format("%Y%m%d").to_string();
    let time = dt.format("%H%M%S").to_string();
    Some((date, time))
}

fn format_adif_date(ts: &prost_types::Timestamp) -> Option<String> {
    let nanos = u32::try_from(ts.nanos).ok()?;
    let dt = chrono::DateTime::from_timestamp(ts.seconds, nanos)?.naive_utc();
    Some(dt.format("%Y%m%d").to_string())
}

fn parse_adif_location(raw_value: &str, latitude: bool) -> Option<f64> {
    let trimmed = raw_value.trim();
    if trimmed.len() != 11 || !trimmed.is_ascii() {
        return None;
    }

    let direction = trimmed.chars().next()?.to_ascii_uppercase();
    match (latitude, direction) {
        (true, 'N' | 'S') | (false, 'E' | 'W') => {}
        _ => return None,
    }

    if trimmed.as_bytes().get(4).copied() != Some(b' ') {
        return None;
    }

    let degrees: f64 = trimmed.get(1..4)?.parse().ok()?;
    let minutes: f64 = trimmed.get(5..11)?.parse().ok()?;
    if !(0.0..60.0).contains(&minutes) {
        return None;
    }

    let signed = (degrees + (minutes / 60.0))
        * if matches!(direction, 'S' | 'W') {
            -1.0
        } else {
            1.0
        };
    let limit = if latitude { 90.0 } else { 180.0 };
    (signed.is_finite() && signed.abs() <= limit).then_some(signed)
}

fn format_adif_location(value: f64, latitude: bool) -> Option<String> {
    if !value.is_finite() {
        return None;
    }

    let limit = if latitude { 90.0 } else { 180.0 };
    if value.abs() > limit {
        return None;
    }

    let direction = if latitude {
        if value.is_sign_negative() {
            'S'
        } else {
            'N'
        }
    } else if value.is_sign_negative() {
        'W'
    } else {
        'E'
    };

    let absolute = value.abs();
    let mut degrees = absolute.floor();
    let mut minutes = ((absolute - degrees) * 60.0 * 1000.0).round() / 1000.0;
    if minutes >= 60.0 {
        degrees += 1.0;
        minutes = 0.0;
    }
    if degrees > limit {
        return None;
    }

    Some(format!("{direction}{degrees:03.0} {minutes:06.3}"))
}

fn mhz_to_khz(mhz: f64) -> Option<u64> {
    if !mhz.is_finite() || mhz.is_sign_negative() {
        return None;
    }

    let rounded = (mhz * 1000.0).round();
    if rounded < 0.0 {
        return None;
    }

    format!("{rounded:.0}").parse().ok()
}

fn parse_confirmation_bool(value: &str) -> Option<bool> {
    if value.eq_ignore_ascii_case("Y") {
        Some(true)
    } else if value.eq_ignore_ascii_case("N") {
        Some(false)
    } else {
        None
    }
}

fn map_confirmation_field(
    target: &mut Option<bool>,
    extra_fields: &mut std::collections::HashMap<String, String>,
    key: &str,
    value: &str,
) {
    if let Some(parsed) = parse_confirmation_bool(value) {
        *target = Some(parsed);
    } else {
        extra_fields.insert(key.to_owned(), value.to_owned());
    }
}

fn push_confirmation_field(
    fields: &mut Vec<AdifField<'_>>,
    key: &'static str,
    value: Option<bool>,
) {
    match value {
        Some(true) => push_field(fields, key, "Y"),
        Some(false) => push_field(fields, key, "N"),
        None => {}
    }
}

fn field_is_overridden(
    qso: &QsoRecord,
    station_snapshot: Option<&StationSnapshot>,
    key: &str,
) -> bool {
    if key.eq_ignore_ascii_case("LOTW_QSL_SENT") {
        qso.lotw_sent.is_some()
    } else if key.eq_ignore_ascii_case("LOTW_QSL_RCVD") {
        qso.lotw_received.is_some()
    } else if key.eq_ignore_ascii_case("EQSL_QSL_SENT") {
        qso.eqsl_sent.is_some()
    } else if key.eq_ignore_ascii_case("EQSL_QSL_RCVD") {
        qso.eqsl_received.is_some()
    } else if key.eq_ignore_ascii_case("STATION_CALLSIGN") {
        station_snapshot.map_or(!qso.station_callsign.is_empty(), |snapshot| {
            !snapshot.station_callsign.is_empty()
        })
    } else if key.eq_ignore_ascii_case("OPERATOR") {
        station_snapshot
            .and_then(|snapshot| snapshot.operator_callsign.as_ref())
            .is_some()
    } else if key.eq_ignore_ascii_case("CONTACTED_OP") {
        qso.worked_operator_callsign.is_some()
    } else if key.eq_ignore_ascii_case("ARRL_SECT") {
        qso.worked_arrl_section.is_some()
    } else if key.eq_ignore_ascii_case("SKCC") {
        qso.skcc.is_some()
    } else if key.eq_ignore_ascii_case("MY_NAME") {
        station_snapshot
            .and_then(|snapshot| snapshot.operator_name.as_ref())
            .is_some()
    } else if key.eq_ignore_ascii_case("MY_GRIDSQUARE") {
        station_snapshot
            .and_then(|snapshot| snapshot.grid.as_ref())
            .is_some()
    } else if key.eq_ignore_ascii_case("MY_CNTY") {
        station_snapshot
            .and_then(|snapshot| snapshot.county.as_ref())
            .is_some()
    } else if key.eq_ignore_ascii_case("MY_STATE") {
        station_snapshot
            .and_then(|snapshot| snapshot.state.as_ref())
            .is_some()
    } else if key.eq_ignore_ascii_case("MY_COUNTRY") {
        station_snapshot
            .and_then(|snapshot| snapshot.country.as_ref())
            .is_some()
    } else if key.eq_ignore_ascii_case("MY_DXCC") {
        station_snapshot
            .and_then(|snapshot| snapshot.dxcc)
            .is_some()
    } else if key.eq_ignore_ascii_case("MY_CQ_ZONE") {
        station_snapshot
            .and_then(|snapshot| snapshot.cq_zone)
            .is_some()
    } else if key.eq_ignore_ascii_case("MY_ITU_ZONE") {
        station_snapshot
            .and_then(|snapshot| snapshot.itu_zone)
            .is_some()
    } else if key.eq_ignore_ascii_case("MY_LAT") {
        station_snapshot
            .and_then(|snapshot| snapshot.latitude)
            .is_some()
    } else if key.eq_ignore_ascii_case("MY_LON") {
        station_snapshot
            .and_then(|snapshot| snapshot.longitude)
            .is_some()
    } else if key.eq_ignore_ascii_case("MY_ARRL_SECT") {
        station_snapshot
            .and_then(|snapshot| snapshot.arrl_section.as_ref())
            .is_some()
    } else if key.eq_ignore_ascii_case("QSLSDATE") {
        qso.qsl_sent_date
            .as_ref()
            .and_then(format_adif_date)
            .is_some()
    } else if key.eq_ignore_ascii_case("QSLRDATE") {
        qso.qsl_received_date
            .as_ref()
            .and_then(format_adif_date)
            .is_some()
    } else if key.eq_ignore_ascii_case("QSO_DATE_OFF") || key.eq_ignore_ascii_case("TIME_OFF") {
        qso.utc_end_timestamp.is_some()
    } else {
        false
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_test_record() -> Record {
        let mut rec = Record::new();
        rec.insert("CALL", "VK9NS").unwrap();
        rec.insert("QSO_DATE", "20260115").unwrap();
        rec.insert("TIME_ON", "1523").unwrap();
        rec.insert("BAND", "20M").unwrap();
        rec.insert("MODE", "RTTY").unwrap();
        rec.insert("FREQ", "14.085").unwrap();
        rec.insert("RST_SENT", "59").unwrap();
        rec.insert("RST_RCVD", "57").unwrap();
        rec.insert("STATION_CALLSIGN", "AA7BQ").unwrap();
        rec
    }

    #[test]
    fn record_to_qso_generates_uuid_local_id() {
        let qso = AdifMapper::record_to_qso(&make_test_record());

        assert!(Uuid::parse_str(&qso.local_id).is_ok());
    }

    #[test]
    fn record_to_qso_core_fields() {
        let rec = make_test_record();
        let qso = AdifMapper::record_to_qso(&rec);

        assert_eq!(qso.worked_callsign, "VK9NS");
        assert_eq!(qso.station_callsign, "AA7BQ");
        assert_eq!(qso.band, Band::Band20m as i32);
        assert_eq!(qso.mode, Mode::Rtty as i32);
        assert_eq!(qso.frequency_khz, Some(14085));
        assert!(qso.utc_timestamp.is_some());
        assert_eq!(
            qso.station_snapshot
                .as_ref()
                .map(|snapshot| snapshot.station_callsign.as_str()),
            Some("AA7BQ")
        );
    }

    #[test]
    fn record_to_qso_signal_reports() {
        let rec = make_test_record();
        let qso = AdifMapper::record_to_qso(&rec);

        let rst_sent = qso.rst_sent.unwrap();
        assert_eq!(rst_sent.raw, "59");
        assert_eq!(rst_sent.readability, Some(5));
        assert_eq!(rst_sent.strength, Some(9));
        assert_eq!(rst_sent.tone, None);
        let rst_rcvd = qso.rst_received.unwrap();
        assert_eq!(rst_rcvd.raw, "57");
        assert_eq!(rst_rcvd.readability, Some(5));
        assert_eq!(rst_rcvd.strength, Some(7));
        assert_eq!(rst_rcvd.tone, None);
    }

    #[test]
    fn record_to_qso_parses_three_digit_rst_tone() {
        let mut rec = Record::new();
        rec.insert("CALL", "W1AW").unwrap();
        rec.insert("RST_SENT", "599").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        let rst = qso.rst_sent.expect("rst");
        assert_eq!(rst.raw, "599");
        assert_eq!(rst.readability, Some(5));
        assert_eq!(rst.strength, Some(9));
        assert_eq!(rst.tone, Some(9));
    }

    #[test]
    fn record_to_qso_timestamp_4digit() {
        let rec = make_test_record();
        let qso = AdifMapper::record_to_qso(&rec);
        let ts = qso.utc_timestamp.unwrap();

        let dt = chrono::DateTime::from_timestamp(ts.seconds, 0)
            .unwrap()
            .naive_utc();
        assert_eq!(dt.format("%Y%m%d").to_string(), "20260115");
        assert_eq!(dt.format("%H%M").to_string(), "1523");
    }

    #[test]
    fn record_to_qso_timestamp_6digit() {
        let mut rec = Record::new();
        rec.insert("QSO_DATE", "20260115").unwrap();
        rec.insert("TIME_ON", "152345").unwrap();
        rec.insert("CALL", "W1AW").unwrap();
        let qso = AdifMapper::record_to_qso(&rec);
        let ts = qso.utc_timestamp.unwrap();
        let dt = chrono::DateTime::from_timestamp(ts.seconds, 0)
            .unwrap()
            .naive_utc();
        assert_eq!(dt.format("%H%M%S").to_string(), "152345");
    }

    #[test]
    fn record_to_qso_qsl_fields() {
        let mut rec = Record::new();
        rec.insert("CALL", "W1AW").unwrap();
        rec.insert("QSL_SENT", "Y").unwrap();
        rec.insert("QSL_RCVD", "Y").unwrap();
        rec.insert("QSLSDATE", "20260120").unwrap();
        rec.insert("QSLRDATE", "20260122").unwrap();
        rec.insert("LOTW_QSL_SENT", "Y").unwrap();
        rec.insert("LOTW_QSL_RCVD", "N").unwrap();
        rec.insert("EQSL_QSL_SENT", "N").unwrap();
        rec.insert("EQSL_QSL_RCVD", "Y").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        assert_eq!(
            qso.qsl_sent_status,
            crate::proto::qsoripper::domain::QslStatus::Yes as i32
        );
        assert_eq!(
            qso.qsl_received_status,
            crate::proto::qsoripper::domain::QslStatus::Yes as i32
        );
        assert_eq!(qso.lotw_sent, Some(true));
        assert_eq!(qso.lotw_received, Some(false));
        assert_eq!(qso.eqsl_sent, Some(false));
        assert_eq!(qso.eqsl_received, Some(true));
        assert_eq!(
            format_adif_date(&qso.qsl_sent_date.expect("qsl sent date")).as_deref(),
            Some("20260120")
        );
        assert_eq!(
            format_adif_date(&qso.qsl_received_date.expect("qsl received date")).as_deref(),
            Some("20260122")
        );
    }

    #[test]
    fn non_boolean_lotw_eqsl_values_preserved_for_round_trip() {
        let mut rec = Record::new();
        rec.insert("CALL", "W1AW").unwrap();
        rec.insert("LOTW_QSL_SENT", "R").unwrap();
        rec.insert("EQSL_QSL_RCVD", "I").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        assert_eq!(qso.lotw_sent, None);
        assert_eq!(qso.eqsl_received, None);
        assert_eq!(
            qso.extra_fields.get("LOTW_QSL_SENT").map(String::as_str),
            Some("R")
        );
        assert_eq!(
            qso.extra_fields.get("EQSL_QSL_RCVD").map(String::as_str),
            Some("I")
        );

        let fields = AdifMapper::qso_to_adif_fields(&qso);
        let field_map: std::collections::HashMap<&str, &str> = fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        assert_eq!(field_map.get("LOTW_QSL_SENT"), Some(&"R"));
        assert_eq!(field_map.get("EQSL_QSL_RCVD"), Some(&"I"));
    }

    #[test]
    fn record_to_qso_contest_fields() {
        let mut rec = Record::new();
        rec.insert("CALL", "DL1ABC").unwrap();
        rec.insert("CONTEST_ID", "CQ-WW-SSB").unwrap();
        rec.insert("SRX", "142").unwrap();
        rec.insert("STX", "033").unwrap();
        rec.insert("STX_STRING", "05 OH").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        assert_eq!(qso.contest_id.as_deref(), Some("CQ-WW-SSB"));
        assert_eq!(qso.serial_received.as_deref(), Some("142"));
        assert_eq!(qso.serial_sent.as_deref(), Some("033"));
        assert_eq!(qso.exchange_sent.as_deref(), Some("05 OH"));
    }

    #[test]
    fn invalid_core_fields_are_preserved_for_round_trip() {
        let mut rec = Record::new();
        rec.insert("CALL", "W1AW").unwrap();
        rec.insert("QSO_DATE", "2025ABCD").unwrap();
        rec.insert("TIME_ON", "250000").unwrap();
        rec.insert("BAND", "11M").unwrap();
        rec.insert("MODE", "TOTALLYNEW").unwrap();
        rec.insert("FREQ", "-1").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);

        assert!(qso.utc_timestamp.is_none());
        assert_eq!(qso.band, Band::Unspecified as i32);
        assert_eq!(qso.mode, Mode::Unspecified as i32);
        assert_eq!(qso.frequency_khz, None);
        assert_eq!(
            qso.extra_fields.get("QSO_DATE").map(String::as_str),
            Some("2025ABCD")
        );
        assert_eq!(
            qso.extra_fields.get("TIME_ON").map(String::as_str),
            Some("250000")
        );
        assert_eq!(
            qso.extra_fields.get("BAND").map(String::as_str),
            Some("11M")
        );
        assert_eq!(
            qso.extra_fields.get("MODE").map(String::as_str),
            Some("TOTALLYNEW")
        );
        assert_eq!(qso.extra_fields.get("FREQ").map(String::as_str), Some("-1"));
    }

    #[test]
    fn record_to_qso_geographic_enrichment() {
        let mut rec = Record::new();
        rec.insert("CALL", "JA1ABC").unwrap();
        rec.insert("GRIDSQUARE", "PM95vk").unwrap();
        rec.insert("COUNTRY", "Japan").unwrap();
        rec.insert("DXCC", "339").unwrap();
        rec.insert("CONT", "AS").unwrap();
        rec.insert("CQZ", "25").unwrap();
        rec.insert("ITUZ", "45").unwrap();
        rec.insert("IOTA", "AS-007").unwrap();
        rec.insert("ARRL_SECT", "DX").unwrap();
        rec.insert("CONTACTED_OP", "JH1XYZ").unwrap();
        rec.insert("NAME", "Taro").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        assert_eq!(qso.worked_grid.as_deref(), Some("PM95vk"));
        assert_eq!(qso.worked_country.as_deref(), Some("Japan"));
        assert_eq!(qso.worked_dxcc, Some(339));
        assert_eq!(qso.worked_continent.as_deref(), Some("AS"));
        assert_eq!(qso.worked_cq_zone, Some(25));
        assert_eq!(qso.worked_itu_zone, Some(45));
        assert_eq!(qso.worked_iota.as_deref(), Some("AS-007"));
        assert_eq!(qso.worked_arrl_section.as_deref(), Some("DX"));
        assert_eq!(qso.worked_operator_callsign.as_deref(), Some("JH1XYZ"));
        assert_eq!(qso.worked_operator_name.as_deref(), Some("Taro"));
    }

    #[test]
    fn dxcc_enrichment_populates_missing_country_continent_and_zones() {
        let mut rec = Record::new();
        rec.insert("CALL", "JA1ABC").unwrap();
        rec.insert("DXCC", "339").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        assert_eq!(qso.worked_dxcc, Some(339));
        assert_eq!(qso.worked_country.as_deref(), Some("JAPAN"));
        assert_eq!(qso.worked_continent.as_deref(), Some("AS"));
        assert_eq!(qso.worked_cq_zone, Some(25));
        assert_eq!(qso.worked_itu_zone, Some(45));
    }

    #[test]
    fn record_to_qso_maps_station_snapshot_fields() {
        let mut rec = Record::new();
        rec.insert("CALL", "W1AW").unwrap();
        rec.insert("STATION_CALLSIGN", "K7RND").unwrap();
        rec.insert("OPERATOR", "N7OPS").unwrap();
        rec.insert("MY_NAME", "Randy").unwrap();
        rec.insert("MY_GRIDSQUARE", "CN87").unwrap();
        rec.insert("MY_STATE", "WA").unwrap();
        rec.insert("MY_DXCC", "291").unwrap();
        rec.insert("MY_LAT", "N047 36.372").unwrap();
        rec.insert("MY_LON", "W122 19.866").unwrap();
        rec.insert("MY_ARRL_SECT", "WWA").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        let snapshot = qso.station_snapshot.expect("snapshot");

        assert_eq!("K7RND", qso.station_callsign);
        assert_eq!("K7RND", snapshot.station_callsign);
        assert_eq!(Some("N7OPS"), snapshot.operator_callsign.as_deref());
        assert_eq!(Some("Randy"), snapshot.operator_name.as_deref());
        assert_eq!(Some("CN87"), snapshot.grid.as_deref());
        assert_eq!(Some("WA"), snapshot.state.as_deref());
        assert_eq!(Some(291), snapshot.dxcc);
        assert_eq!(Some(47.6062), snapshot.latitude);
        assert_eq!(Some(-122.3311), snapshot.longitude);
        assert_eq!(Some("WWA"), snapshot.arrl_section.as_deref());
    }

    #[test]
    fn record_to_qso_propagation_fields() {
        let mut rec = Record::new();
        rec.insert("CALL", "W1AW").unwrap();
        rec.insert("PROP_MODE", "SAT").unwrap();
        rec.insert("SAT_NAME", "ISS").unwrap();
        rec.insert("SAT_MODE", "V").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        assert_eq!(qso.prop_mode.as_deref(), Some("SAT"));
        assert_eq!(qso.sat_name.as_deref(), Some("ISS"));
        assert_eq!(qso.sat_mode.as_deref(), Some("V"));
    }

    #[test]
    fn record_to_qso_extra_fields_preserved() {
        let mut rec = Record::new();
        rec.insert("CALL", "W1AW").unwrap();
        rec.insert("MY_RIG", "Icom IC-7300").unwrap();
        rec.insert("MY_ANTENNA", "Yagi 3-element").unwrap();
        rec.insert("ANT_AZ", "045").unwrap();
        rec.insert("APP_QSORIPPER_SYNC_STATUS", "synced").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        assert_eq!(
            qso.extra_fields.get("MY_RIG").map(String::as_str),
            Some("Icom IC-7300")
        );
        assert_eq!(
            qso.extra_fields.get("MY_ANTENNA").map(String::as_str),
            Some("Yagi 3-element")
        );
        assert_eq!(
            qso.extra_fields.get("ANT_AZ").map(String::as_str),
            Some("045")
        );
        assert_eq!(
            qso.extra_fields
                .get("APP_QSORIPPER_SYNC_STATUS")
                .map(String::as_str),
            Some("synced")
        );
    }

    #[test]
    fn import_only_mode_c4fm_sets_submode() {
        let mut rec = Record::new();
        rec.insert("CALL", "W1AW").unwrap();
        rec.insert("MODE", "C4FM").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        assert_eq!(qso.mode, Mode::Digitalvoice as i32);
        assert_eq!(qso.submode.as_deref(), Some("C4FM"));
    }

    #[test]
    fn submode_aliases_in_mode_field_map_to_parent_mode() {
        for (raw_mode, expected_mode, expected_submode) in [
            ("PSK31", Mode::Psk, "PSK31"),
            ("USB", Mode::Ssb, "USB"),
            ("DMR", Mode::Digitalvoice, "DMR"),
            ("Q65", Mode::Mfsk, "Q65"),
        ] {
            let mut rec = Record::new();
            rec.insert("CALL", "W1AW").unwrap();
            rec.insert("MODE", raw_mode).unwrap();

            let qso = AdifMapper::record_to_qso(&rec);
            assert_eq!(
                qso.mode, expected_mode as i32,
                "mode mismatch for {raw_mode}"
            );
            assert_eq!(
                qso.submode.as_deref(),
                Some(expected_submode),
                "submode mismatch for {raw_mode}"
            );
        }
    }

    #[test]
    fn qso_to_adif_fields_round_trip() {
        let rec = make_test_record();
        let qso = AdifMapper::record_to_qso(&rec);
        let adif_fields = AdifMapper::qso_to_adif_fields(&qso);

        // Check key fields are present
        let field_map: std::collections::HashMap<&str, &str> = adif_fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        assert_eq!(field_map.get("CALL"), Some(&"VK9NS"));
        assert_eq!(field_map.get("STATION_CALLSIGN"), Some(&"AA7BQ"));
        assert_eq!(field_map.get("BAND"), Some(&"20M"));
        assert_eq!(field_map.get("MODE"), Some(&"RTTY"));
        assert!(field_map.contains_key("QSO_DATE"));
        assert!(field_map.contains_key("TIME_ON"));
    }

    #[test]
    fn record_to_qso_maps_end_timestamp() {
        let mut rec = Record::new();
        rec.insert("CALL", "W1AW").unwrap();
        rec.insert("QSO_DATE", "20260115").unwrap();
        rec.insert("TIME_ON", "235500").unwrap();
        rec.insert("QSO_DATE_OFF", "20260116").unwrap();
        rec.insert("TIME_OFF", "000200").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        let ts = qso.utc_end_timestamp.expect("end timestamp");
        let dt = chrono::DateTime::from_timestamp(ts.seconds, 0)
            .unwrap()
            .naive_utc();
        assert_eq!(dt.format("%Y%m%d").to_string(), "20260116");
        assert_eq!(dt.format("%H%M%S").to_string(), "000200");
    }

    #[test]
    fn qso_to_adif_fields_emits_end_timestamp() {
        let qso = crate::proto::qsoripper::domain::QsoRecord {
            worked_callsign: "W1AW".into(),
            utc_timestamp: Some(prost_types::Timestamp {
                seconds: 1_768_578_000,
                nanos: 0,
            }),
            utc_end_timestamp: Some(prost_types::Timestamp {
                seconds: 1_768_578_120,
                nanos: 0,
            }),
            ..Default::default()
        };

        let fields = AdifMapper::qso_to_adif_fields(&qso);
        let field_map: std::collections::HashMap<&str, &str> = fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        assert_eq!(field_map.get("QSO_DATE_OFF"), Some(&"20260116"));
        assert_eq!(field_map.get("TIME_OFF"), Some(&"154200"));
    }

    #[test]
    fn qso_to_adif_fields_emits_station_snapshot_fields() {
        let qso = crate::proto::qsoripper::domain::QsoRecord {
            station_callsign: "K7RND".into(),
            worked_callsign: "W1AW".into(),
            station_snapshot: Some(StationSnapshot {
                station_callsign: "K7RND".into(),
                operator_callsign: Some("N7OPS".into()),
                operator_name: Some("Randy".into()),
                grid: Some("CN87".into()),
                state: Some("WA".into()),
                latitude: Some(47.6062),
                longitude: Some(-122.3311),
                arrl_section: Some("WWA".into()),
                ..StationSnapshot::default()
            }),
            ..Default::default()
        };

        let fields = AdifMapper::qso_to_adif_fields(&qso);
        let field_map: std::collections::HashMap<&str, &str> = fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        assert_eq!(field_map.get("STATION_CALLSIGN"), Some(&"K7RND"));
        assert_eq!(field_map.get("OPERATOR"), Some(&"N7OPS"));
        assert_eq!(field_map.get("MY_NAME"), Some(&"Randy"));
        assert_eq!(field_map.get("MY_GRIDSQUARE"), Some(&"CN87"));
        assert_eq!(field_map.get("MY_STATE"), Some(&"WA"));
        assert_eq!(field_map.get("MY_LAT"), Some(&"N047 36.372"));
        assert_eq!(field_map.get("MY_LON"), Some(&"W122 19.866"));
        assert_eq!(field_map.get("MY_ARRL_SECT"), Some(&"WWA"));
    }

    #[test]
    fn qso_to_adif_fields_emits_contacted_operator_arrl_section_and_qsl_dates() {
        let qso = crate::proto::qsoripper::domain::QsoRecord {
            worked_callsign: "W1AW".into(),
            worked_operator_callsign: Some("K1OP".into()),
            worked_arrl_section: Some("EMA".into()),
            qsl_sent_date: Some(prost_types::Timestamp {
                seconds: 1_737_331_200,
                nanos: 0,
            }),
            qsl_received_date: Some(prost_types::Timestamp {
                seconds: 1_737_504_000,
                nanos: 0,
            }),
            ..Default::default()
        };

        let fields = AdifMapper::qso_to_adif_fields(&qso);
        let field_map: std::collections::HashMap<&str, &str> = fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        assert_eq!(field_map.get("CONTACTED_OP"), Some(&"K1OP"));
        assert_eq!(field_map.get("ARRL_SECT"), Some(&"EMA"));
        assert_eq!(field_map.get("QSLSDATE"), Some(&"20250120"));
        assert_eq!(field_map.get("QSLRDATE"), Some(&"20250122"));
    }

    #[test]
    fn adi_format_output() {
        let fields = vec![
            ("CALL".to_string(), "W1AW".to_string()),
            ("BAND".to_string(), "20M".to_string()),
        ];
        let adi = AdifMapper::fields_to_adi(&fields);
        assert!(adi.contains("<CALL:4>W1AW"));
        assert!(adi.contains("<BAND:3>20M"));
        assert!(adi.contains("<eor>"));
    }

    #[test]
    fn qso_to_adif_fields_emits_n_for_false_confirmation_fields() {
        let qso = crate::proto::qsoripper::domain::QsoRecord {
            worked_callsign: "W1AW".into(),
            lotw_sent: Some(false),
            lotw_received: Some(true),
            eqsl_sent: Some(false),
            eqsl_received: Some(true),
            ..Default::default()
        };

        let fields = AdifMapper::qso_to_adif_fields(&qso);
        let field_map: std::collections::HashMap<&str, &str> = fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        assert_eq!(field_map.get("LOTW_QSL_SENT"), Some(&"N"));
        assert_eq!(field_map.get("LOTW_QSL_RCVD"), Some(&"Y"));
        assert_eq!(field_map.get("EQSL_QSL_SENT"), Some(&"N"));
        assert_eq!(field_map.get("EQSL_QSL_RCVD"), Some(&"Y"));
    }

    #[test]
    fn dedicated_fields_do_not_duplicate_overridden_extra_fields() {
        let qso = crate::proto::qsoripper::domain::QsoRecord {
            station_callsign: "K7RND".into(),
            worked_callsign: "W1AW".into(),
            lotw_sent: Some(true),
            eqsl_received: Some(false),
            station_snapshot: Some(StationSnapshot {
                station_callsign: "K7RND".into(),
                grid: Some("CN87".into()),
                operator_name: Some("Randy".into()),
                arrl_section: Some("WWA".into()),
                ..StationSnapshot::default()
            }),
            extra_fields: [
                ("station_callsign".to_string(), "OLD".to_string()),
                ("contacted_op".to_string(), "OLDOP".to_string()),
                ("arrl_sect".to_string(), "OLDSECT".to_string()),
                ("my_name".to_string(), "OLDNAME".to_string()),
                ("my_gridsquare".to_string(), "OLDGRID".to_string()),
                ("my_arrl_sect".to_string(), "OLDWWA".to_string()),
                ("qslsdate".to_string(), "19990101".to_string()),
                ("qslrdate".to_string(), "19990102".to_string()),
                ("lotw_qsl_sent".to_string(), "R".to_string()),
                ("eqsl_qsl_rcvd".to_string(), "I".to_string()),
            ]
            .into_iter()
            .collect(),
            worked_operator_callsign: Some("K1OP".into()),
            worked_arrl_section: Some("EMA".into()),
            qsl_sent_date: Some(prost_types::Timestamp {
                seconds: 1_737_331_200,
                nanos: 0,
            }),
            qsl_received_date: Some(prost_types::Timestamp {
                seconds: 1_737_504_000,
                nanos: 0,
            }),
            ..Default::default()
        };

        let fields = AdifMapper::qso_to_adif_fields(&qso);
        let lotw_values: Vec<&str> = fields
            .iter()
            .filter(|(k, _)| k == "LOTW_QSL_SENT")
            .map(|(_, v)| v.as_str())
            .collect();
        let eqsl_values: Vec<&str> = fields
            .iter()
            .filter(|(k, _)| k == "EQSL_QSL_RCVD")
            .map(|(_, v)| v.as_str())
            .collect();
        let station_callsign_values: Vec<&str> = fields
            .iter()
            .filter(|(k, _)| k == "STATION_CALLSIGN")
            .map(|(_, v)| v.as_str())
            .collect();
        let contacted_op_values: Vec<&str> = fields
            .iter()
            .filter(|(k, _)| k == "CONTACTED_OP")
            .map(|(_, v)| v.as_str())
            .collect();
        let arrl_sect_values: Vec<&str> = fields
            .iter()
            .filter(|(k, _)| k == "ARRL_SECT")
            .map(|(_, v)| v.as_str())
            .collect();
        let my_name_values: Vec<&str> = fields
            .iter()
            .filter(|(k, _)| k == "MY_NAME")
            .map(|(_, v)| v.as_str())
            .collect();
        let grid_values: Vec<&str> = fields
            .iter()
            .filter(|(k, _)| k == "MY_GRIDSQUARE")
            .map(|(_, v)| v.as_str())
            .collect();
        let my_arrl_sect_values: Vec<&str> = fields
            .iter()
            .filter(|(k, _)| k == "MY_ARRL_SECT")
            .map(|(_, v)| v.as_str())
            .collect();
        let qsl_sent_date_values: Vec<&str> = fields
            .iter()
            .filter(|(k, _)| k == "QSLSDATE")
            .map(|(_, v)| v.as_str())
            .collect();
        let qsl_received_date_values: Vec<&str> = fields
            .iter()
            .filter(|(k, _)| k == "QSLRDATE")
            .map(|(_, v)| v.as_str())
            .collect();

        assert_eq!(lotw_values, vec!["Y"]);
        assert_eq!(eqsl_values, vec!["N"]);
        assert_eq!(station_callsign_values, vec!["K7RND"]);
        assert_eq!(contacted_op_values, vec!["K1OP"]);
        assert_eq!(arrl_sect_values, vec!["EMA"]);
        assert_eq!(my_name_values, vec!["Randy"]);
        assert_eq!(grid_values, vec!["CN87"]);
        assert_eq!(my_arrl_sect_values, vec!["WWA"]);
        assert_eq!(qsl_sent_date_values, vec!["20250120"]);
        assert_eq!(qsl_received_date_values, vec!["20250122"]);
    }

    #[test]
    fn operator_fallback_when_no_station_callsign() {
        let mut rec = Record::new();
        rec.insert("CALL", "W1AW").unwrap();
        rec.insert("OPERATOR", "AA7BQ").unwrap();
        // No STATION_CALLSIGN

        let qso = AdifMapper::record_to_qso(&rec);
        assert_eq!(qso.station_callsign, "AA7BQ");
        assert_eq!(
            qso.station_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.operator_callsign.as_deref()),
            Some("AA7BQ")
        );
    }

    #[test]
    fn frequency_to_khz_conversion() {
        let mut rec = Record::new();
        rec.insert("CALL", "W1AW").unwrap();
        rec.insert("FREQ", "14.074").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        assert_eq!(qso.frequency_khz, Some(14074));
    }

    #[test]
    fn frequency_to_khz_precise() {
        let mut rec = Record::new();
        rec.insert("CALL", "W1AW").unwrap();
        rec.insert("FREQ", "7.0385").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        assert_eq!(qso.frequency_khz, Some(7039)); // 7038.5 rounds to 7039
    }

    #[test]
    fn negative_frequency_is_rejected() {
        let mut rec = Record::new();
        rec.insert("CALL", "W1AW").unwrap();
        rec.insert("FREQ", "-1.0").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        assert_eq!(
            qso.frequency_khz, None,
            "Negative frequency should be rejected, not mapped to 0"
        );
    }

    #[test]
    fn negative_nanos_does_not_corrupt_date() {
        let qso = crate::proto::qsoripper::domain::QsoRecord {
            worked_callsign: "W1AW".into(),
            utc_timestamp: Some(prost_types::Timestamp {
                seconds: 1_700_000_000, // 2023-11-14
                nanos: -1,
            }),
            ..Default::default()
        };

        let fields = AdifMapper::qso_to_adif_fields(&qso);
        let field_map: std::collections::HashMap<&str, &str> = fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        assert_ne!(
            field_map.get("QSO_DATE").copied(),
            Some("19700101"),
            "Negative nanos should not silently produce epoch date"
        );
    }

    #[test]
    fn invalid_qsl_dates_fall_back_to_overflow_fields() {
        let qso = crate::proto::qsoripper::domain::QsoRecord {
            worked_callsign: "W1AW".into(),
            qsl_sent_date: Some(prost_types::Timestamp {
                seconds: 1_700_000_000,
                nanos: -1,
            }),
            qsl_received_date: Some(prost_types::Timestamp {
                seconds: 1_700_000_100,
                nanos: -1,
            }),
            extra_fields: [
                ("QSLSDATE".to_string(), "20250120".to_string()),
                ("QSLRDATE".to_string(), "20250122".to_string()),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        };

        let fields = AdifMapper::qso_to_adif_fields(&qso);
        let field_map: std::collections::HashMap<&str, &str> = fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        assert_eq!(field_map.get("QSLSDATE"), Some(&"20250120"));
        assert_eq!(field_map.get("QSLRDATE"), Some(&"20250122"));
    }

    #[test]
    fn non_ascii_date_does_not_panic() {
        // "202ü123" is 8 bytes but byte 4 is inside the ü (continuation byte),
        // so str[0..4] would panic on a char boundary check without a guard.
        let result = parse_adif_datetime("202\u{00fc}123", None);
        assert!(
            result.is_none(),
            "Non-ASCII date should return None, not panic"
        );
    }

    #[test]
    fn adif_location_round_trips() {
        let latitude = parse_adif_location("N047 36.372", true).expect("latitude");
        let longitude = parse_adif_location("W122 19.866", false).expect("longitude");

        assert_eq!(
            Some("N047 36.372".to_string()),
            format_adif_location(latitude, true)
        );
        assert_eq!(
            Some("W122 19.866".to_string()),
            format_adif_location(longitude, false)
        );
    }

    #[test]
    fn record_to_qso_imports_skcc() {
        let mut rec = Record::new();
        rec.insert("CALL", "W1AW").unwrap();
        rec.insert("SKCC", "12345T").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        assert_eq!(qso.skcc.as_deref(), Some("12345T"));
    }

    #[test]
    fn qso_to_adif_fields_exports_skcc() {
        let qso = crate::proto::qsoripper::domain::QsoRecord {
            worked_callsign: "W1AW".into(),
            skcc: Some("12345T".into()),
            ..Default::default()
        };

        let fields = AdifMapper::qso_to_adif_fields(&qso);
        let skcc_values: Vec<&str> = fields
            .iter()
            .filter(|(k, _)| k == "SKCC")
            .map(|(_, v)| v.as_str())
            .collect();
        assert_eq!(skcc_values, vec!["12345T"]);
    }

    #[test]
    fn skcc_round_trips_through_adif() {
        let mut rec = Record::new();
        rec.insert("CALL", "K7ABC").unwrap();
        rec.insert("SKCC", "999S").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        let fields = AdifMapper::qso_to_adif_fields(&qso);
        let skcc_out: Vec<&str> = fields
            .iter()
            .filter(|(k, _)| k == "SKCC")
            .map(|(_, v)| v.as_str())
            .collect();
        assert_eq!(skcc_out, vec!["999S"]);
    }
}

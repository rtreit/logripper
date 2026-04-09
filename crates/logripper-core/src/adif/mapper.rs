//! Maps difa::Record → proto QsoRecord and back.
//!
//! This is the edge adapter between the ADIF file format and LogRipper's
//! internal domain types. All ADIF-specific logic is contained here.

use crate::domain::band::band_from_adif;
use crate::domain::mode::{import_only_submode, mode_from_adif};
use crate::domain::qso::qsl_status_from_adif;
use crate::proto::logripper::domain::{Band, Mode, QsoRecord, SyncStatus};
use difa::Record;

/// Maps difa ADIF records to/from proto QsoRecords.
pub struct AdifMapper;

impl AdifMapper {
    /// Convert a difa::Record (parsed ADIF QSO) into a proto QsoRecord.
    ///
    /// Recognized ADIF fields are mapped to dedicated QsoRecord fields.
    /// Unrecognized fields are stored in `extra_fields` for round-trip fidelity.
    pub fn record_to_qso(record: &Record) -> QsoRecord {
        let mut qso = QsoRecord {
            local_id: new_local_id(),
            sync_status: SyncStatus::LocalOnly.into(),
            ..Default::default()
        };

        // Collect QSO_DATE and TIME_ON separately for timestamp construction
        let mut qso_date: Option<String> = None;
        let mut time_on: Option<String> = None;

        for (key, datum) in record.fields() {
            let value_str = datum.as_str().to_string();
            let key_upper = key.to_uppercase();

            match key_upper.as_str() {
                // --- Core fields ---
                "CALL" => qso.worked_callsign = value_str,
                "STATION_CALLSIGN" => qso.station_callsign = value_str,
                "OPERATOR" => {
                    if qso.station_callsign.is_empty() {
                        qso.station_callsign = value_str;
                    } else {
                        qso.extra_fields.insert(key_upper, value_str);
                    }
                }
                "QSO_DATE" => qso_date = Some(value_str),
                "TIME_ON" => time_on = Some(value_str),
                "BAND" => {
                    if let Some(band) = band_from_adif(&value_str) {
                        qso.band = band.into();
                    }
                }
                "BAND_RX" => {
                    qso.extra_fields.insert(key_upper, value_str);
                }
                "MODE" => {
                    if let Some(mode) = mode_from_adif(&value_str) {
                        qso.mode = mode.into();
                        // Handle import-only modes that map to submode
                        if let Some(sub) = import_only_submode(&value_str) {
                            if qso.submode.is_none() {
                                qso.submode = Some(sub.to_string());
                            }
                        }
                    }
                }
                "SUBMODE" => qso.submode = Some(value_str),
                "FREQ" => {
                    if let Ok(mhz) = value_str.parse::<f64>() {
                        qso.frequency_khz = Some((mhz * 1000.0).round() as u64);
                    }
                }
                "FREQ_RX" => {
                    qso.extra_fields.insert(key_upper, value_str);
                }

                // --- Signal reports ---
                "RST_SENT" => {
                    qso.rst_sent = Some(crate::proto::logripper::domain::RstReport {
                        raw: value_str,
                        ..Default::default()
                    });
                }
                "RST_RCVD" => {
                    qso.rst_received = Some(crate::proto::logripper::domain::RstReport {
                        raw: value_str,
                        ..Default::default()
                    });
                }
                "TX_PWR" => qso.tx_power = Some(value_str),

                // --- Geographic / enrichment ---
                "NAME" => qso.worked_operator_name = Some(value_str),
                "GRIDSQUARE" => qso.worked_grid = Some(value_str),
                "COUNTRY" => qso.worked_country = Some(value_str),
                "DXCC" => {
                    if let Ok(code) = value_str.parse::<u32>() {
                        qso.worked_dxcc = Some(code);
                    }
                }
                "STATE" => qso.worked_state = Some(value_str),
                "CNTY" => qso.worked_county = Some(value_str),
                "CONT" => qso.worked_continent = Some(value_str),
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
                "IOTA" => qso.worked_iota = Some(value_str),

                // --- QSL ---
                "QSL_SENT" => qso.qsl_sent_status = qsl_status_from_adif(&value_str).into(),
                "QSL_RCVD" => qso.qsl_received_status = qsl_status_from_adif(&value_str).into(),
                "LOTW_QSL_SENT" => qso.lotw_sent = Some(value_str == "Y"),
                "LOTW_QSL_RCVD" => qso.lotw_received = Some(value_str == "Y"),
                "EQSL_QSL_SENT" => qso.eqsl_sent = Some(value_str == "Y"),
                "EQSL_QSL_RCVD" => qso.eqsl_received = Some(value_str == "Y"),

                // --- Contest ---
                "CONTEST_ID" => qso.contest_id = Some(value_str),
                "SRX" => qso.serial_received = Some(value_str),
                "STX" => qso.serial_sent = Some(value_str),
                "SRX_STRING" => qso.exchange_received = Some(value_str),
                "STX_STRING" => qso.exchange_sent = Some(value_str),

                // --- Propagation ---
                "PROP_MODE" => qso.prop_mode = Some(value_str),
                "SAT_NAME" => qso.sat_name = Some(value_str),
                "SAT_MODE" => qso.sat_mode = Some(value_str),

                // --- Notes ---
                "COMMENT" => qso.comment = Some(value_str),
                "NOTES" => qso.notes = Some(value_str),

                // --- Everything else → extra_fields for round-trip ---
                _ => {
                    qso.extra_fields.insert(key_upper, value_str);
                }
            }
        }

        // Combine QSO_DATE + TIME_ON into UTC timestamp
        if let Some(ref date_str) = qso_date {
            if let Some(ts) = parse_adif_datetime(date_str, time_on.as_deref()) {
                qso.utc_timestamp = Some(ts);
            }
        }

        qso
    }

    /// Convert a proto QsoRecord into a list of ADIF field key-value pairs.
    /// Suitable for generating ADI output.
    pub fn qso_to_adif_fields(qso: &QsoRecord) -> Vec<(String, String)> {
        let mut fields = Vec::new();

        // Core
        if !qso.station_callsign.is_empty() {
            fields.push(("STATION_CALLSIGN".into(), qso.station_callsign.clone()));
        }
        if !qso.worked_callsign.is_empty() {
            fields.push(("CALL".into(), qso.worked_callsign.clone()));
        }

        // Timestamp → QSO_DATE + TIME_ON
        if let Some(ref ts) = qso.utc_timestamp {
            let (date_str, time_str) = format_adif_datetime(ts);
            fields.push(("QSO_DATE".into(), date_str));
            fields.push(("TIME_ON".into(), time_str));
        }

        // Band
        let band = Band::try_from(qso.band).unwrap_or(Band::Unspecified);
        if let Some(band_str) = crate::domain::band::band_to_adif(band) {
            fields.push(("BAND".into(), band_str.to_string()));
        }

        // Mode + submode
        let mode = Mode::try_from(qso.mode).unwrap_or(Mode::Unspecified);
        if let Some(mode_str) = crate::domain::mode::mode_to_adif(mode) {
            fields.push(("MODE".into(), mode_str.to_string()));
        }
        if let Some(ref sub) = qso.submode {
            fields.push(("SUBMODE".into(), sub.clone()));
        }

        // Frequency
        if let Some(khz) = qso.frequency_khz {
            let mhz = khz as f64 / 1000.0;
            fields.push(("FREQ".into(), format!("{mhz:.3}")));
        }

        // Signal reports
        if let Some(ref rst) = qso.rst_sent {
            fields.push(("RST_SENT".into(), rst.raw.clone()));
        }
        if let Some(ref rst) = qso.rst_received {
            fields.push(("RST_RCVD".into(), rst.raw.clone()));
        }
        if let Some(ref pwr) = qso.tx_power {
            fields.push(("TX_PWR".into(), pwr.clone()));
        }

        // Geographic
        if let Some(ref v) = qso.worked_operator_name { fields.push(("NAME".into(), v.clone())); }
        if let Some(ref v) = qso.worked_grid { fields.push(("GRIDSQUARE".into(), v.clone())); }
        if let Some(ref v) = qso.worked_country { fields.push(("COUNTRY".into(), v.clone())); }
        if let Some(dxcc) = qso.worked_dxcc { fields.push(("DXCC".into(), dxcc.to_string())); }
        if let Some(ref v) = qso.worked_state { fields.push(("STATE".into(), v.clone())); }
        if let Some(ref v) = qso.worked_county { fields.push(("CNTY".into(), v.clone())); }
        if let Some(ref v) = qso.worked_continent { fields.push(("CONT".into(), v.clone())); }
        if let Some(z) = qso.worked_cq_zone { fields.push(("CQZ".into(), z.to_string())); }
        if let Some(z) = qso.worked_itu_zone { fields.push(("ITUZ".into(), z.to_string())); }
        if let Some(ref v) = qso.worked_iota { fields.push(("IOTA".into(), v.clone())); }

        // QSL
        let sent = crate::domain::qso::qsl_status_to_adif(
            crate::proto::logripper::domain::QslStatus::try_from(qso.qsl_sent_status)
                .unwrap_or(crate::proto::logripper::domain::QslStatus::Unspecified),
        );
        if let Some(s) = sent { fields.push(("QSL_SENT".into(), s.to_string())); }

        let rcvd = crate::domain::qso::qsl_status_to_adif(
            crate::proto::logripper::domain::QslStatus::try_from(qso.qsl_received_status)
                .unwrap_or(crate::proto::logripper::domain::QslStatus::Unspecified),
        );
        if let Some(s) = rcvd { fields.push(("QSL_RCVD".into(), s.to_string())); }

        if let Some(true) = qso.lotw_sent { fields.push(("LOTW_QSL_SENT".into(), "Y".into())); }
        if let Some(true) = qso.lotw_received { fields.push(("LOTW_QSL_RCVD".into(), "Y".into())); }
        if let Some(true) = qso.eqsl_sent { fields.push(("EQSL_QSL_SENT".into(), "Y".into())); }
        if let Some(true) = qso.eqsl_received { fields.push(("EQSL_QSL_RCVD".into(), "Y".into())); }

        // Contest
        if let Some(ref v) = qso.contest_id { fields.push(("CONTEST_ID".into(), v.clone())); }
        if let Some(ref v) = qso.serial_sent { fields.push(("STX".into(), v.clone())); }
        if let Some(ref v) = qso.serial_received { fields.push(("SRX".into(), v.clone())); }
        if let Some(ref v) = qso.exchange_sent { fields.push(("STX_STRING".into(), v.clone())); }
        if let Some(ref v) = qso.exchange_received { fields.push(("SRX_STRING".into(), v.clone())); }

        // Propagation
        if let Some(ref v) = qso.prop_mode { fields.push(("PROP_MODE".into(), v.clone())); }
        if let Some(ref v) = qso.sat_name { fields.push(("SAT_NAME".into(), v.clone())); }
        if let Some(ref v) = qso.sat_mode { fields.push(("SAT_MODE".into(), v.clone())); }

        // Notes
        if let Some(ref v) = qso.comment { fields.push(("COMMENT".into(), v.clone())); }
        if let Some(ref v) = qso.notes { fields.push(("NOTES".into(), v.clone())); }

        // Extra fields (round-trip overflow)
        for (k, v) in &qso.extra_fields {
            fields.push((k.clone(), v.clone()));
        }

        fields
    }

    /// Generate an ADI-format string from ADIF field pairs.
    pub fn fields_to_adi(fields: &[(String, String)]) -> String {
        let mut out = String::new();
        for (key, value) in fields {
            let len = value.len();
            out.push_str(&format!("<{key}:{len}>{value}\n"));
        }
        out.push_str("<eor>\n");
        out
    }
}

/// Parse ADIF date (YYYYMMDD) + optional time (HHMM or HHMMSS) into prost Timestamp.
fn parse_adif_datetime(date_str: &str, time_str: Option<&str>) -> Option<prost_types::Timestamp> {
    if date_str.len() != 8 {
        return None;
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
fn format_adif_datetime(ts: &prost_types::Timestamp) -> (String, String) {
    let dt = chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
        .unwrap_or_default()
        .naive_utc();
    let date = dt.format("%Y%m%d").to_string();
    let time = dt.format("%H%M%S").to_string();
    (date, time)
}

/// Generate a simple local ID. Will be replaced with proper UUID crate later.
fn new_local_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("local-{n:012}")
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn record_to_qso_core_fields() {
        let rec = make_test_record();
        let qso = AdifMapper::record_to_qso(&rec);

        assert_eq!(qso.worked_callsign, "VK9NS");
        assert_eq!(qso.station_callsign, "AA7BQ");
        assert_eq!(qso.band, Band::Band20m as i32);
        assert_eq!(qso.mode, Mode::Rtty as i32);
        assert_eq!(qso.frequency_khz, Some(14085));
        assert!(qso.utc_timestamp.is_some());
    }

    #[test]
    fn record_to_qso_signal_reports() {
        let rec = make_test_record();
        let qso = AdifMapper::record_to_qso(&rec);

        let rst_sent = qso.rst_sent.unwrap();
        assert_eq!(rst_sent.raw, "59");
        let rst_rcvd = qso.rst_received.unwrap();
        assert_eq!(rst_rcvd.raw, "57");
    }

    #[test]
    fn record_to_qso_timestamp_4digit() {
        let rec = make_test_record();
        let qso = AdifMapper::record_to_qso(&rec);
        let ts = qso.utc_timestamp.unwrap();

        let dt = chrono::DateTime::from_timestamp(ts.seconds, 0).unwrap().naive_utc();
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
        let dt = chrono::DateTime::from_timestamp(ts.seconds, 0).unwrap().naive_utc();
        assert_eq!(dt.format("%H%M%S").to_string(), "152345");
    }

    #[test]
    fn record_to_qso_qsl_fields() {
        let mut rec = Record::new();
        rec.insert("CALL", "W1AW").unwrap();
        rec.insert("QSL_SENT", "Y").unwrap();
        rec.insert("QSL_RCVD", "N").unwrap();
        rec.insert("LOTW_QSL_SENT", "Y").unwrap();
        rec.insert("EQSL_QSL_RCVD", "Y").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        assert_eq!(
            qso.qsl_sent_status,
            crate::proto::logripper::domain::QslStatus::Yes as i32
        );
        assert_eq!(
            qso.qsl_received_status,
            crate::proto::logripper::domain::QslStatus::No as i32
        );
        assert_eq!(qso.lotw_sent, Some(true));
        assert_eq!(qso.eqsl_received, Some(true));
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
        rec.insert("NAME", "Taro").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        assert_eq!(qso.worked_grid.as_deref(), Some("PM95vk"));
        assert_eq!(qso.worked_country.as_deref(), Some("Japan"));
        assert_eq!(qso.worked_dxcc, Some(339));
        assert_eq!(qso.worked_continent.as_deref(), Some("AS"));
        assert_eq!(qso.worked_cq_zone, Some(25));
        assert_eq!(qso.worked_itu_zone, Some(45));
        assert_eq!(qso.worked_iota.as_deref(), Some("AS-007"));
        assert_eq!(qso.worked_operator_name.as_deref(), Some("Taro"));
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
        rec.insert("APP_LOGRIPPER_SYNC_STATUS", "synced").unwrap();

        let qso = AdifMapper::record_to_qso(&rec);
        assert_eq!(qso.extra_fields.get("MY_RIG").map(|s| s.as_str()), Some("Icom IC-7300"));
        assert_eq!(qso.extra_fields.get("MY_ANTENNA").map(|s| s.as_str()), Some("Yagi 3-element"));
        assert_eq!(qso.extra_fields.get("ANT_AZ").map(|s| s.as_str()), Some("045"));
        assert_eq!(qso.extra_fields.get("APP_LOGRIPPER_SYNC_STATUS").map(|s| s.as_str()), Some("synced"));
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
    fn operator_fallback_when_no_station_callsign() {
        let mut rec = Record::new();
        rec.insert("CALL", "W1AW").unwrap();
        rec.insert("OPERATOR", "AA7BQ").unwrap();
        // No STATION_CALLSIGN

        let qso = AdifMapper::record_to_qso(&rec);
        assert_eq!(qso.station_callsign, "AA7BQ");
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
}

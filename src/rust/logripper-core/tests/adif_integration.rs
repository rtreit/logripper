#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used)]

//! Integration tests: parse real ADI files using `difa` and map to `QsoRecord`.

use difa::RecordStream;
use futures::StreamExt;
use logripper_core::adif::AdifMapper;
use logripper_core::proto::logripper::domain::{Band, Mode, QslStatus};

/// Helper: parse an ADI byte slice and return all QSO records (skip header).
async fn parse_adi_to_qsos(
    data: &[u8],
) -> Vec<logripper_core::proto::logripper::domain::QsoRecord> {
    let mut stream = RecordStream::new(data, true);
    let mut qsos = Vec::new();

    while let Some(result) = stream.next().await {
        let record = result.expect("Failed to parse ADIF record");
        if record.is_header() {
            continue;
        }
        qsos.push(AdifMapper::record_to_qso(&record));
    }

    qsos
}

#[tokio::test]
async fn parse_basic_qsos_file() {
    let data = include_bytes!("../../../../tests/fixtures/basic_qsos.adi");
    let qsos = parse_adi_to_qsos(data).await;

    assert_eq!(qsos.len(), 3, "Expected 3 QSO records");

    // First QSO: VK9NS on 20M RTTY
    let q1 = &qsos[0];
    assert_eq!(q1.worked_callsign, "VK9NS");
    assert_eq!(q1.station_callsign, "AA7BQ");
    assert_eq!(q1.band, Band::Band20m as i32);
    assert_eq!(q1.mode, Mode::Rtty as i32);
    assert_eq!(q1.frequency_khz, Some(14085));
    assert_eq!(q1.rst_sent.as_ref().unwrap().raw, "59");
    assert_eq!(q1.rst_received.as_ref().unwrap().raw, "57");
    assert_eq!(q1.comment.as_deref(), Some("Good signal!"));
    assert_eq!(q1.tx_power.as_deref(), Some("100"));
    // MY_GRIDSQUARE should be in extra_fields
    assert!(q1.extra_fields.contains_key("MY_GRIDSQUARE"));

    // Second QSO: ON4UN on 40M SSB/USB (contest)
    let q2 = &qsos[1];
    assert_eq!(q2.worked_callsign, "ON4UN");
    assert_eq!(q2.band, Band::Band40m as i32);
    assert_eq!(q2.mode, Mode::Ssb as i32);
    assert_eq!(q2.submode.as_deref(), Some("USB"));
    assert_eq!(q2.frequency_khz, Some(7180));
    assert_eq!(q2.contest_id.as_deref(), Some("CQ-WW-SSB"));
    assert_eq!(q2.serial_received.as_deref(), Some("142"));
    assert_eq!(q2.serial_sent.as_deref(), Some("033"));
    assert_eq!(q2.exchange_sent.as_deref(), Some("05 OH"));
    assert_eq!(q2.qsl_sent_status, QslStatus::Yes as i32);
    assert_eq!(q2.qsl_received_status, QslStatus::No as i32);
    assert_eq!(q2.lotw_sent, Some(true));
    assert_eq!(q2.eqsl_received, Some(true));

    // Third QSO: JA1ABC on 15M FT8 with geographic enrichment
    let q3 = &qsos[2];
    assert_eq!(q3.worked_callsign, "JA1ABC");
    assert_eq!(q3.band, Band::Band15m as i32);
    assert_eq!(q3.mode, Mode::Ft8 as i32);
    assert_eq!(q3.frequency_khz, Some(21074));
    assert_eq!(q3.worked_grid.as_deref(), Some("PM95vk"));
    assert_eq!(q3.worked_country.as_deref(), Some("Japan"));
    assert_eq!(q3.worked_dxcc, Some(339));
    assert_eq!(q3.worked_continent.as_deref(), Some("AS"));
    assert_eq!(q3.worked_cq_zone, Some(25));
    assert_eq!(q3.worked_itu_zone, Some(45));
    assert_eq!(q3.worked_operator_name.as_deref(), Some("Taro Yama"));
    assert_eq!(q3.prop_mode.as_deref(), Some("F2"));
}

#[tokio::test]
async fn parse_headerless_file() {
    let data = include_bytes!("../../../../tests/fixtures/no_header.adi");
    let qsos = parse_adi_to_qsos(data).await;

    assert_eq!(qsos.len(), 2, "Expected 2 QSOs from headerless file");
    assert_eq!(qsos[0].worked_callsign, "W1AW");
    assert_eq!(qsos[0].mode, Mode::Cw as i32);
    assert_eq!(qsos[1].worked_callsign, "K3LR");
    assert_eq!(qsos[1].band, Band::Band40m as i32);
}

#[tokio::test]
async fn parse_contest_log() {
    let data = include_bytes!("../../../../tests/fixtures/contest_log.adi");
    let qsos = parse_adi_to_qsos(data).await;

    assert_eq!(qsos.len(), 2, "Expected 2 contest QSOs");

    let q1 = &qsos[0];
    assert_eq!(q1.contest_id.as_deref(), Some("CQ-WW-SSB"));
    assert_eq!(q1.serial_sent.as_deref(), Some("001"));
    assert_eq!(q1.exchange_sent.as_deref(), Some("05 OH"));
    assert_eq!(q1.station_callsign, "AA7BQ");
    // Contest-specific fields go to extra_fields
    assert!(q1.extra_fields.contains_key("CHECK"));
    assert!(q1.extra_fields.contains_key("CLASS"));
    assert!(q1.extra_fields.contains_key("PRECEDENCE"));
    assert!(q1.extra_fields.contains_key("ARRL_SECT"));

    let q2 = &qsos[1];
    assert_eq!(q2.worked_callsign, "DL1ABC");
    assert_eq!(q2.worked_country.as_deref(), Some("Germany"));
    assert_eq!(q2.worked_dxcc, Some(230));
    assert_eq!(q2.worked_continent.as_deref(), Some("EU"));
    assert_eq!(q2.worked_cq_zone, Some(14));
}

#[tokio::test]
async fn parse_extra_fields_preserved() {
    let data = include_bytes!("../../../../tests/fixtures/extra_fields.adi");
    let qsos = parse_adi_to_qsos(data).await;

    assert_eq!(qsos.len(), 1);
    let q = &qsos[0];

    assert_eq!(q.worked_callsign, "VE3XY");
    assert_eq!(q.band, Band::Band2m as i32);
    assert_eq!(q.mode, Mode::Fm as i32);

    // Propagation fields mapped to dedicated fields
    assert_eq!(q.sat_name.as_deref(), Some("ISS"));
    assert_eq!(q.sat_mode.as_deref(), Some("V"));

    // MY_ fields and other extras go to extra_fields map
    assert_eq!(
        q.extra_fields.get("MY_RIG").map(String::as_str),
        Some("Icom IC-7300")
    );
    assert_eq!(
        q.extra_fields.get("MY_ANTENNA").map(String::as_str),
        Some("Yagi 3-element")
    );
    assert_eq!(
        q.extra_fields.get("MY_CITY").map(String::as_str),
        Some("Seattle")
    );
    assert_eq!(
        q.extra_fields.get("MY_STATE").map(String::as_str),
        Some("WA")
    );
    assert_eq!(
        q.extra_fields.get("MY_GRIDSQUARE").map(String::as_str),
        Some("CN87up")
    );
    assert_eq!(
        q.extra_fields.get("ANT_AZ").map(String::as_str),
        Some("045")
    );
    assert_eq!(q.extra_fields.get("ANT_EL").map(String::as_str), Some("15"));
    assert_eq!(
        q.extra_fields
            .get("APP_LOGRIPPER_SYNC_STATUS")
            .map(String::as_str),
        Some("synced")
    );
}

#[tokio::test]
async fn round_trip_qso_through_adif() {
    // Parse a QSO from ADIF
    let data = include_bytes!("../../../../tests/fixtures/basic_qsos.adi");
    let qsos = parse_adi_to_qsos(data).await;
    let original = &qsos[0];

    // Convert back to ADIF
    let adi_string = AdifMapper::qso_to_adi(original);

    // Parse the generated ADIF back
    let mut stream = RecordStream::new(adi_string.as_bytes(), true);
    let parsed_back = stream.next().await.unwrap().expect("Failed to re-parse");
    assert!(!parsed_back.is_header());
    let round_tripped = AdifMapper::record_to_qso(&parsed_back);

    // Verify key fields survived the round-trip
    assert_eq!(round_tripped.worked_callsign, original.worked_callsign);
    assert_eq!(round_tripped.station_callsign, original.station_callsign);
    assert_eq!(round_tripped.band, original.band);
    assert_eq!(round_tripped.mode, original.mode);
    assert_eq!(round_tripped.frequency_khz, original.frequency_khz);
    assert_eq!(
        round_tripped.rst_sent.as_ref().map(|r| r.raw.as_str()),
        original.rst_sent.as_ref().map(|r| r.raw.as_str())
    );
    assert_eq!(
        round_tripped.rst_received.as_ref().map(|r| r.raw.as_str()),
        original.rst_received.as_ref().map(|r| r.raw.as_str())
    );
    assert_eq!(round_tripped.comment, original.comment);
}

#[tokio::test]
async fn round_trip_extra_fields_preserved() {
    let data = include_bytes!("../../../../tests/fixtures/extra_fields.adi");
    let qsos = parse_adi_to_qsos(data).await;
    let original = &qsos[0];

    // Round-trip
    let adi_string = AdifMapper::qso_to_adi(original);
    let mut stream = RecordStream::new(adi_string.as_bytes(), true);
    let parsed_back = stream.next().await.unwrap().expect("Failed to re-parse");
    let round_tripped = AdifMapper::record_to_qso(&parsed_back);

    // Extra fields should survive
    assert_eq!(
        round_tripped.extra_fields.get("MY_RIG").map(String::as_str),
        original.extra_fields.get("MY_RIG").map(String::as_str),
    );
    assert_eq!(
        round_tripped
            .extra_fields
            .get("APP_LOGRIPPER_SYNC_STATUS")
            .map(String::as_str),
        original
            .extra_fields
            .get("APP_LOGRIPPER_SYNC_STATUS")
            .map(String::as_str),
    );
}

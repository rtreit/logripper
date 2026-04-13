//! `QsoRecord` helpers: construction, QSL status mapping, and ADIF field mapping.

use crate::proto::qsoripper::domain::{Band, Mode, QslStatus, QsoRecord, SyncStatus};
use uuid::Uuid;

/// Map an ADIF QSL Sent/Received status character to the `QslStatus` enum.
#[must_use]
pub fn qsl_status_from_adif(s: &str) -> QslStatus {
    match s.to_uppercase().as_str() {
        "Y" => QslStatus::Yes,
        "N" => QslStatus::No,
        "R" => QslStatus::Requested,
        "Q" => QslStatus::Queued,
        "I" => QslStatus::Ignore,
        _ => QslStatus::Unspecified,
    }
}

/// Convert the `QslStatus` enum to its ADIF character representation.
#[must_use]
pub fn qsl_status_to_adif(status: QslStatus) -> Option<&'static str> {
    match status {
        QslStatus::Yes => Some("Y"),
        QslStatus::No => Some("N"),
        QslStatus::Requested => Some("R"),
        QslStatus::Queued => Some("Q"),
        QslStatus::Ignore => Some("I"),
        QslStatus::Unspecified => None,
    }
}

/// Builder for constructing `QsoRecord` values with sensible defaults.
pub struct QsoRecordBuilder {
    record: QsoRecord,
}

impl QsoRecordBuilder {
    /// Create a builder for a QSO between the local station and the worked station.
    pub fn new(station_callsign: impl Into<String>, worked_callsign: impl Into<String>) -> Self {
        Self {
            record: QsoRecord {
                local_id: new_local_id(),
                station_callsign: station_callsign.into(),
                worked_callsign: worked_callsign.into(),
                sync_status: SyncStatus::LocalOnly.into(),
                ..Default::default()
            },
        }
    }

    #[must_use]
    /// Set the band for the QSO.
    pub fn band(mut self, band: Band) -> Self {
        self.record.band = band.into();
        self
    }

    #[must_use]
    /// Set the mode for the QSO.
    pub fn mode(mut self, mode: Mode) -> Self {
        self.record.mode = mode.into();
        self
    }

    #[must_use]
    /// Set the submode for the QSO.
    pub fn submode(mut self, submode: impl Into<String>) -> Self {
        self.record.submode = Some(submode.into());
        self
    }

    #[must_use]
    /// Set the transmit frequency in kHz.
    pub fn frequency_khz(mut self, khz: u64) -> Self {
        self.record.frequency_khz = Some(khz);
        self
    }

    #[must_use]
    /// Set the UTC timestamp for the QSO.
    pub fn timestamp(mut self, ts: prost_types::Timestamp) -> Self {
        self.record.utc_timestamp = Some(ts);
        self
    }

    #[must_use]
    /// Set the contest identifier for the QSO.
    pub fn contest(mut self, contest_id: impl Into<String>) -> Self {
        self.record.contest_id = Some(contest_id.into());
        self
    }

    #[must_use]
    /// Set the operator comment for the QSO.
    pub fn comment(mut self, comment: impl Into<String>) -> Self {
        self.record.comment = Some(comment.into());
        self
    }

    #[must_use]
    /// Set operator notes for the QSO.
    pub fn notes(mut self, notes: impl Into<String>) -> Self {
        self.record.notes = Some(notes.into());
        self
    }

    #[must_use]
    /// Add an extra ADIF field for round-trip preservation.
    pub fn extra_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.record.extra_fields.insert(key.into(), value.into());
        self
    }

    #[must_use]
    /// Finalize and return the built `QsoRecord`.
    pub fn build(self) -> QsoRecord {
        self.record
    }
}

/// Generate a QSO local ID using the documented UUID format.
#[must_use]
pub fn new_local_id() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn qsl_status_round_trips() {
        let cases = [
            ("Y", QslStatus::Yes),
            ("N", QslStatus::No),
            ("R", QslStatus::Requested),
            ("Q", QslStatus::Queued),
            ("I", QslStatus::Ignore),
        ];
        for (adif_str, expected_status) in &cases {
            let parsed = qsl_status_from_adif(adif_str);
            assert_eq!(parsed, *expected_status, "Parse mismatch for '{adif_str}'");

            let back = qsl_status_to_adif(parsed).unwrap();
            assert_eq!(back, *adif_str, "Round-trip mismatch for '{adif_str}'");
        }
    }

    #[test]
    fn qsl_status_case_insensitive() {
        assert_eq!(qsl_status_from_adif("y"), QslStatus::Yes);
        assert_eq!(qsl_status_from_adif("n"), QslStatus::No);
        assert_eq!(qsl_status_from_adif("r"), QslStatus::Requested);
    }

    #[test]
    fn qsl_status_unknown_returns_unspecified() {
        assert_eq!(qsl_status_from_adif(""), QslStatus::Unspecified);
        assert_eq!(qsl_status_from_adif("X"), QslStatus::Unspecified);
        assert_eq!(qsl_status_to_adif(QslStatus::Unspecified), None);
    }

    #[test]
    fn builder_creates_valid_record() {
        let record = QsoRecordBuilder::new("AA7BQ", "VK9NS")
            .band(Band::Band20m)
            .mode(Mode::Ft8)
            .frequency_khz(14_074)
            .comment("Great signal".to_string())
            .build();

        assert_eq!(record.station_callsign, "AA7BQ");
        assert_eq!(record.worked_callsign, "VK9NS");
        assert_eq!(record.band, Band::Band20m as i32);
        assert_eq!(record.mode, Mode::Ft8 as i32);
        assert_eq!(record.frequency_khz, Some(14_074));
        assert_eq!(record.comment.as_deref(), Some("Great signal"));
        assert_eq!(record.sync_status, SyncStatus::LocalOnly as i32);
        assert!(!record.local_id.is_empty());
    }

    #[test]
    fn builder_extra_fields_preserved() {
        let record = QsoRecordBuilder::new("AA7BQ", "VK9NS")
            .extra_field("MY_RIG", "Icom IC-7300")
            .extra_field("ANT_AZ", "270")
            .build();

        assert_eq!(
            record.extra_fields.get("MY_RIG").map(String::as_str),
            Some("Icom IC-7300")
        );
        assert_eq!(
            record.extra_fields.get("ANT_AZ").map(String::as_str),
            Some("270")
        );
        assert_eq!(record.extra_fields.len(), 2);
    }

    #[test]
    fn builder_generates_unique_local_ids() {
        let first = QsoRecordBuilder::new("AA7BQ", "VK9NS").build();
        let second = QsoRecordBuilder::new("AA7BQ", "VK9NS").build();

        assert_ne!(first.local_id, second.local_id);
        assert!(Uuid::parse_str(&first.local_id).is_ok());
        assert!(Uuid::parse_str(&second.local_id).is_ok());
    }

    #[test]
    fn builder_submode_set() {
        let record = QsoRecordBuilder::new("AA7BQ", "VK9NS")
            .mode(Mode::Ssb)
            .submode("USB")
            .build();

        assert_eq!(record.mode, Mode::Ssb as i32);
        assert_eq!(record.submode.as_deref(), Some("USB"));
    }

    #[test]
    fn default_qso_record_has_expected_defaults() {
        let record = QsoRecord::default();
        assert_eq!(record.band, Band::Unspecified as i32);
        assert_eq!(record.mode, Mode::Unspecified as i32);
        assert_eq!(record.sync_status, SyncStatus::LocalOnly as i32);
        assert_eq!(record.qsl_sent_status, QslStatus::Unspecified as i32);
        assert_eq!(record.qsl_received_status, QslStatus::Unspecified as i32);
        assert!(record.extra_fields.is_empty());
    }

    #[test]
    fn qso_record_prost_serialization_round_trip() {
        use prost::Message;

        let record = QsoRecordBuilder::new("W1AW", "JA1ABC")
            .band(Band::Band40m)
            .mode(Mode::Cw)
            .frequency_khz(7_030)
            .comment("CW contest")
            .extra_field("CHECK", "73")
            .build();

        // Serialize
        let mut buf = Vec::new();
        record.encode(&mut buf).expect("encode failed");
        assert!(!buf.is_empty());

        // Deserialize
        let decoded = QsoRecord::decode(buf.as_slice()).expect("decode failed");
        assert_eq!(decoded.station_callsign, "W1AW");
        assert_eq!(decoded.worked_callsign, "JA1ABC");
        assert_eq!(decoded.band, Band::Band40m as i32);
        assert_eq!(decoded.mode, Mode::Cw as i32);
        assert_eq!(decoded.frequency_khz, Some(7_030));
        assert_eq!(decoded.comment.as_deref(), Some("CW contest"));
        assert_eq!(
            decoded.extra_fields.get("CHECK").map(String::as_str),
            Some("73")
        );
    }
}

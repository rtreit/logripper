//! Helpers for formatting QSO durations consistently across UI surfaces.
//!
//! See <https://github.com/rtreit/qsoripper/issues/201>.

use prost_types::Timestamp;

use crate::proto::qsoripper::domain::QsoRecord;

/// Format a positive duration in whole seconds into an operator-friendly string.
///
/// Format conventions (kept compact for log columns):
/// - `<1m`     -> `"Ns"`            e.g. `"45s"`
/// - `<1h`     -> `"Mm SSs"`        e.g. `"2m 35s"`
/// - `>=1h`    -> `"Hh MMm"`        e.g. `"1h 12m"`
///
/// Returns `None` if `seconds <= 0` so callers can render `"—"` or omit the column entirely.
#[must_use]
pub fn format_duration_seconds(seconds: i64) -> Option<String> {
    if seconds <= 0 {
        return None;
    }
    let total = u64::try_from(seconds).ok()?;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    let formatted = if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else if minutes > 0 {
        format!("{minutes}m {secs:02}s")
    } else {
        format!("{secs}s")
    };
    Some(formatted)
}

/// Compute the QSO duration in seconds when both start and end timestamps are present
/// and `end > start`. Returns `None` otherwise.
#[must_use]
pub fn qso_duration_seconds(start: Option<&Timestamp>, end: Option<&Timestamp>) -> Option<i64> {
    let start = start?;
    let end = end?;
    let delta = end.seconds.checked_sub(start.seconds)?;
    if delta > 0 {
        Some(delta)
    } else {
        None
    }
}

/// Convenience helper: compute and format the duration of a `QsoRecord` directly.
///
/// `utc_timestamp` is treated as the start time and `utc_end_timestamp` as the end time
/// (matching `proto/domain/qso_record.proto`).
#[must_use]
pub fn format_qso_duration(record: &QsoRecord) -> Option<String> {
    let seconds = qso_duration_seconds(
        record.utc_timestamp.as_ref(),
        record.utc_end_timestamp.as_ref(),
    )?;
    format_duration_seconds(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(seconds: i64) -> Timestamp {
        Timestamp { seconds, nanos: 0 }
    }

    #[test]
    fn returns_none_for_non_positive_seconds() {
        assert_eq!(format_duration_seconds(0), None);
        assert_eq!(format_duration_seconds(-5), None);
    }

    #[test]
    fn formats_seconds_only_under_one_minute() {
        assert_eq!(format_duration_seconds(1).as_deref(), Some("1s"));
        assert_eq!(format_duration_seconds(45).as_deref(), Some("45s"));
        assert_eq!(format_duration_seconds(59).as_deref(), Some("59s"));
    }

    #[test]
    fn formats_minutes_and_seconds_under_one_hour() {
        assert_eq!(format_duration_seconds(60).as_deref(), Some("1m 00s"));
        assert_eq!(format_duration_seconds(155).as_deref(), Some("2m 35s"));
        assert_eq!(format_duration_seconds(3599).as_deref(), Some("59m 59s"));
    }

    #[test]
    fn formats_hours_and_minutes_for_one_hour_or_more() {
        assert_eq!(format_duration_seconds(3600).as_deref(), Some("1h 00m"));
        assert_eq!(format_duration_seconds(4320).as_deref(), Some("1h 12m"));
        assert_eq!(format_duration_seconds(7_265).as_deref(), Some("2h 01m"));
        assert_eq!(format_duration_seconds(86_400).as_deref(), Some("24h 00m"));
    }

    #[test]
    fn duration_seconds_requires_both_timestamps() {
        assert_eq!(qso_duration_seconds(None, None), None);
        assert_eq!(qso_duration_seconds(Some(&ts(100)), None), None);
        assert_eq!(qso_duration_seconds(None, Some(&ts(100))), None);
    }

    #[test]
    fn duration_seconds_requires_strictly_positive_delta() {
        assert_eq!(qso_duration_seconds(Some(&ts(100)), Some(&ts(100))), None);
        assert_eq!(qso_duration_seconds(Some(&ts(200)), Some(&ts(100))), None);
        assert_eq!(
            qso_duration_seconds(Some(&ts(100)), Some(&ts(155))),
            Some(55)
        );
    }

    #[test]
    fn format_qso_duration_returns_none_when_either_timestamp_is_missing() {
        let mut record = QsoRecord::default();
        assert_eq!(format_qso_duration(&record), None);
        record.utc_timestamp = Some(ts(100));
        assert_eq!(format_qso_duration(&record), None);
        record.utc_timestamp = None;
        record.utc_end_timestamp = Some(ts(100));
        assert_eq!(format_qso_duration(&record), None);
    }

    #[test]
    fn format_qso_duration_uses_record_timestamps() {
        let record = QsoRecord {
            utc_timestamp: Some(ts(1_700_000_000)),
            utc_end_timestamp: Some(ts(1_700_000_155)),
            ..QsoRecord::default()
        };
        assert_eq!(format_qso_duration(&record).as_deref(), Some("2m 35s"));
    }

    #[test]
    fn format_qso_duration_returns_none_when_end_not_after_start() {
        let record = QsoRecord {
            utc_timestamp: Some(ts(1_700_000_000)),
            utc_end_timestamp: Some(ts(1_700_000_000)),
            ..QsoRecord::default()
        };
        assert_eq!(format_qso_duration(&record), None);
    }
}

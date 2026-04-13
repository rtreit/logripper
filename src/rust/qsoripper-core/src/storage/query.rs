//! Query and snapshot types shared across storage adapters.

use crate::proto::qsoripper::domain::{Band, LookupResult, Mode};
use prost_types::Timestamp;

/// Filter and pagination options for listing QSOs from persistent storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QsoListQuery {
    /// Only return QSOs at or after this UTC timestamp.
    pub after: Option<Timestamp>,
    /// Only return QSOs at or before this UTC timestamp.
    pub before: Option<Timestamp>,
    /// Match callsigns containing this filter text.
    pub callsign_filter: Option<String>,
    /// Restrict results to a single band.
    pub band_filter: Option<Band>,
    /// Restrict results to a single mode.
    pub mode_filter: Option<Mode>,
    /// Restrict results to a single contest identifier.
    pub contest_id: Option<String>,
    /// Maximum number of rows to return. `None` means unbounded.
    pub limit: Option<u32>,
    /// Number of rows to skip before returning results.
    pub offset: u32,
    /// Requested sort order.
    pub sort: QsoSortOrder,
}

impl Default for QsoListQuery {
    fn default() -> Self {
        Self {
            after: None,
            before: None,
            callsign_filter: None,
            band_filter: None,
            mode_filter: None,
            contest_id: None,
            limit: None,
            offset: 0,
            sort: QsoSortOrder::NewestFirst,
        }
    }
}

/// Sort order for `QsoListQuery`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QsoSortOrder {
    /// Newest records first.
    NewestFirst,
    /// Oldest records first.
    OldestFirst,
}

/// Aggregate counts derived from locally persisted QSOs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LogbookCounts {
    /// Total number of locally stored QSOs.
    pub local_qso_count: u32,
    /// Number of locally stored QSOs that still need sync work.
    pub pending_upload_count: u32,
}

/// Persisted remote logbook metadata reported by sync workflows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncMetadata {
    /// Remote QRZ QSO count from the last known status snapshot.
    pub qrz_qso_count: u32,
    /// Timestamp of the last successful remote sync.
    pub last_sync: Option<Timestamp>,
    /// Remote logbook owner reported by QRZ.
    pub qrz_logbook_owner: Option<String>,
}

/// A persisted callsign lookup snapshot stored below the hot in-memory cache.
#[derive(Debug, Clone, PartialEq)]
pub struct LookupSnapshot {
    /// Normalized callsign key for the stored snapshot.
    pub callsign: String,
    /// Persisted lookup result payload.
    pub result: LookupResult,
    /// Timestamp when the snapshot was stored.
    pub stored_at: Timestamp,
    /// Optional expiry timestamp for stale-data decisions.
    pub expires_at: Option<Timestamp>,
}

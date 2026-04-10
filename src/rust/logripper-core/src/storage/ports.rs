//! Domain-facing storage contracts implemented by persistence adapters.

use crate::proto::logripper::domain::QsoRecord;
use crate::storage::{LogbookCounts, LookupSnapshot, QsoListQuery, StorageError, SyncMetadata};

/// Root engine-owned storage abstraction used by application services.
pub trait EngineStorage: Send + Sync {
    /// Return the logbook-oriented storage surface.
    fn logbook(&self) -> &dyn LogbookStore;

    /// Return the persisted lookup snapshot surface.
    fn lookup_snapshots(&self) -> &dyn LookupSnapshotStore;

    /// Return a stable backend name for diagnostics and bootstrap logs.
    fn backend_name(&self) -> &'static str;
}

/// Persistence operations for the QSO logbook and sync metadata.
#[tonic::async_trait]
pub trait LogbookStore: Send + Sync {
    /// Insert a new QSO.
    async fn insert_qso(&self, qso: &QsoRecord) -> Result<(), StorageError>;

    /// Update an existing QSO. Returns `true` when a row was updated.
    async fn update_qso(&self, qso: &QsoRecord) -> Result<bool, StorageError>;

    /// Delete a QSO by local ID. Returns `true` when a row was removed.
    async fn delete_qso(&self, local_id: &str) -> Result<bool, StorageError>;

    /// Load a single QSO by local ID.
    async fn get_qso(&self, local_id: &str) -> Result<Option<QsoRecord>, StorageError>;

    /// List QSOs using the provided query object.
    async fn list_qsos(&self, query: &QsoListQuery) -> Result<Vec<QsoRecord>, StorageError>;

    /// Return aggregate counts derived from locally persisted QSOs.
    async fn qso_counts(&self) -> Result<LogbookCounts, StorageError>;

    /// Return the persisted remote sync metadata snapshot.
    async fn get_sync_metadata(&self) -> Result<SyncMetadata, StorageError>;

    /// Replace the persisted remote sync metadata snapshot.
    async fn upsert_sync_metadata(&self, metadata: &SyncMetadata) -> Result<(), StorageError>;
}

/// Persistence operations for callsign lookup snapshots stored below the hot cache.
#[tonic::async_trait]
pub trait LookupSnapshotStore: Send + Sync {
    /// Load a persisted lookup snapshot by callsign.
    async fn get_lookup_snapshot(
        &self,
        callsign: &str,
    ) -> Result<Option<LookupSnapshot>, StorageError>;

    /// Insert or replace a persisted lookup snapshot.
    async fn upsert_lookup_snapshot(&self, snapshot: &LookupSnapshot) -> Result<(), StorageError>;

    /// Delete a persisted lookup snapshot by callsign. Returns `true` when removed.
    async fn delete_lookup_snapshot(&self, callsign: &str) -> Result<bool, StorageError>;
}

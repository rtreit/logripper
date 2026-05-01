//! In-memory storage adapter for `QsoRipper` engine services.

use qsoripper_core::application::logbook::is_pending_sync_status;
use qsoripper_core::domain::lookup::normalize_callsign;
use qsoripper_core::proto::qsoripper::domain::QsoRecord;
use qsoripper_core::storage::{
    DeletedRecordsFilter, EngineStorage, LogbookCounts, LogbookStore, LookupSnapshot,
    LookupSnapshotStore, QsoHistoryPage, QsoListQuery, QsoSortOrder, StorageError, SyncMetadata,
};
use std::cmp::Reverse;
use std::collections::BTreeMap;
use tokio::sync::RwLock;

/// In-memory storage implementation used for tests and backend-swapping validation.
#[derive(Default)]
pub struct MemoryStorage {
    state: RwLock<MemoryState>,
}

#[derive(Debug, Default)]
struct MemoryState {
    qsos: BTreeMap<String, QsoRecord>,
    sync_metadata: SyncMetadata,
    lookup_snapshots: BTreeMap<String, LookupSnapshot>,
}

impl MemoryStorage {
    /// Create a new empty in-memory storage backend.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl EngineStorage for MemoryStorage {
    fn logbook(&self) -> &dyn LogbookStore {
        self
    }

    fn lookup_snapshots(&self) -> &dyn LookupSnapshotStore {
        self
    }

    fn backend_name(&self) -> &'static str {
        "memory"
    }
}

#[tonic::async_trait]
impl LogbookStore for MemoryStorage {
    async fn insert_qso(&self, qso: &QsoRecord) -> Result<(), StorageError> {
        let mut state = self.state.write().await;
        if state.qsos.contains_key(&qso.local_id) {
            return Err(StorageError::duplicate("qso", &qso.local_id));
        }

        state.qsos.insert(qso.local_id.clone(), qso.clone());
        Ok(())
    }

    async fn update_qso(&self, qso: &QsoRecord) -> Result<bool, StorageError> {
        let mut state = self.state.write().await;
        if !state.qsos.contains_key(&qso.local_id) {
            return Ok(false);
        }

        state.qsos.insert(qso.local_id.clone(), qso.clone());
        Ok(true)
    }

    async fn delete_qso(&self, local_id: &str) -> Result<bool, StorageError> {
        let mut state = self.state.write().await;
        Ok(state.qsos.remove(local_id).is_some())
    }

    async fn soft_delete_qso(
        &self,
        local_id: &str,
        deleted_at_ms: i64,
        pending_remote_delete: bool,
    ) -> Result<bool, StorageError> {
        let mut state = self.state.write().await;
        let Some(record) = state.qsos.get_mut(local_id) else {
            return Ok(false);
        };
        record.deleted_at = Some(millis_to_timestamp(deleted_at_ms));
        record.pending_remote_delete = pending_remote_delete;
        Ok(true)
    }

    async fn restore_qso(&self, local_id: &str) -> Result<bool, StorageError> {
        let mut state = self.state.write().await;
        let Some(record) = state.qsos.get_mut(local_id) else {
            return Ok(false);
        };
        record.deleted_at = None;
        record.pending_remote_delete = false;
        Ok(true)
    }

    async fn get_qso(&self, local_id: &str) -> Result<Option<QsoRecord>, StorageError> {
        let state = self.state.read().await;
        Ok(state.qsos.get(local_id).cloned())
    }

    async fn list_qsos(&self, query: &QsoListQuery) -> Result<Vec<QsoRecord>, StorageError> {
        let state = self.state.read().await;
        let mut records = state
            .qsos
            .values()
            .filter(|record| matches_query(record, query))
            .cloned()
            .collect::<Vec<_>>();

        match query.sort {
            QsoSortOrder::NewestFirst => {
                records.sort_by_key(|record| {
                    (
                        Reverse(timestamp_to_millis(record.utc_timestamp.as_ref())),
                        Reverse(record.local_id.clone()),
                    )
                });
            }
            QsoSortOrder::OldestFirst => {
                records.sort_by_key(|record| {
                    (
                        timestamp_to_millis(record.utc_timestamp.as_ref()),
                        record.local_id.clone(),
                    )
                });
            }
        }

        let offset = usize::try_from(query.offset)
            .map_err(|_| StorageError::backend("offset does not fit in usize"))?;
        let limit = query
            .limit
            .map(|value| {
                usize::try_from(value)
                    .map_err(|_| StorageError::backend("limit does not fit in usize"))
            })
            .transpose()?;

        let sliced = records.into_iter().skip(offset);
        let result = if let Some(limit) = limit {
            sliced.take(limit).collect()
        } else {
            sliced.collect()
        };

        Ok(result)
    }

    async fn list_qso_history(
        &self,
        worked_callsign: &str,
        limit: u32,
    ) -> Result<QsoHistoryPage, StorageError> {
        let normalized = normalize_callsign(worked_callsign);
        if normalized.is_empty() {
            return Ok(QsoHistoryPage::default());
        }

        let state = self.state.read().await;
        let mut matching = state
            .qsos
            .values()
            .filter(|record| {
                record.deleted_at.is_none()
                    && record.worked_callsign.eq_ignore_ascii_case(&normalized)
            })
            .cloned()
            .collect::<Vec<_>>();

        let total = u32::try_from(matching.len())
            .map_err(|_| StorageError::backend("history total exceeds u32"))?;

        if limit == 0 {
            return Ok(QsoHistoryPage {
                entries: Vec::new(),
                total,
            });
        }

        matching.sort_by_key(|record| {
            (
                Reverse(timestamp_to_millis(record.utc_timestamp.as_ref())),
                Reverse(record.local_id.clone()),
            )
        });

        let take = usize::try_from(limit)
            .map_err(|_| StorageError::backend("limit does not fit in usize"))?;
        matching.truncate(take);

        Ok(QsoHistoryPage {
            entries: matching,
            total,
        })
    }

    async fn qso_counts(&self) -> Result<LogbookCounts, StorageError> {
        let state = self.state.read().await;
        let active_iter = state
            .qsos
            .values()
            .filter(|record| record.deleted_at.is_none());
        let local_qso_count = active_iter.clone().count();
        let pending_upload_count = active_iter
            .filter(|record| is_pending_sync_status(record.sync_status))
            .count();

        Ok(LogbookCounts {
            local_qso_count: u32::try_from(local_qso_count)
                .map_err(|_| StorageError::backend("local_qso_count exceeds u32"))?,
            pending_upload_count: u32::try_from(pending_upload_count)
                .map_err(|_| StorageError::backend("pending_upload_count exceeds u32"))?,
        })
    }

    async fn get_sync_metadata(&self) -> Result<SyncMetadata, StorageError> {
        let state = self.state.read().await;
        Ok(state.sync_metadata.clone())
    }

    async fn upsert_sync_metadata(&self, metadata: &SyncMetadata) -> Result<(), StorageError> {
        let mut state = self.state.write().await;
        state.sync_metadata = metadata.clone();
        Ok(())
    }

    async fn purge_deleted_qsos(
        &self,
        local_ids: &[String],
        older_than_ms: Option<i64>,
    ) -> Result<u32, StorageError> {
        let mut state = self.state.write().await;
        let ids_to_purge: Vec<String> = state
            .qsos
            .iter()
            .filter(|(_, record)| record.deleted_at.is_some())
            .filter(|(id, _)| local_ids.is_empty() || local_ids.contains(id))
            .filter(|(_, record)| {
                older_than_ms
                    .is_none_or(|cutoff| timestamp_to_millis(record.deleted_at.as_ref()) <= cutoff)
            })
            .map(|(id, _)| id.clone())
            .collect();

        let count = u32::try_from(ids_to_purge.len()).unwrap_or(u32::MAX);
        for id in &ids_to_purge {
            state.qsos.remove(id);
        }
        Ok(count)
    }
}

#[tonic::async_trait]
impl LookupSnapshotStore for MemoryStorage {
    async fn get_lookup_snapshot(
        &self,
        callsign: &str,
    ) -> Result<Option<LookupSnapshot>, StorageError> {
        let state = self.state.read().await;
        Ok(state
            .lookup_snapshots
            .get(&normalize_callsign(callsign))
            .cloned())
    }

    async fn upsert_lookup_snapshot(&self, snapshot: &LookupSnapshot) -> Result<(), StorageError> {
        let mut state = self.state.write().await;
        let key = normalize_callsign(&snapshot.callsign);
        let mut stored_snapshot = snapshot.clone();
        stored_snapshot.callsign.clone_from(&key);
        state.lookup_snapshots.insert(key, stored_snapshot);
        Ok(())
    }

    async fn delete_lookup_snapshot(&self, callsign: &str) -> Result<bool, StorageError> {
        let mut state = self.state.write().await;
        Ok(state
            .lookup_snapshots
            .remove(&normalize_callsign(callsign))
            .is_some())
    }
}

fn matches_query(record: &QsoRecord, query: &QsoListQuery) -> bool {
    match query.deleted_filter {
        DeletedRecordsFilter::ActiveOnly => {
            if record.deleted_at.is_some() {
                return false;
            }
        }
        DeletedRecordsFilter::DeletedOnly => {
            if record.deleted_at.is_none() {
                return false;
            }
        }
        DeletedRecordsFilter::All => {}
    }

    if let Some(after) = query.after.as_ref() {
        if timestamp_to_millis(record.utc_timestamp.as_ref()) < timestamp_to_millis(Some(after)) {
            return false;
        }
    }

    if let Some(before) = query.before.as_ref() {
        if timestamp_to_millis(record.utc_timestamp.as_ref()) > timestamp_to_millis(Some(before)) {
            return false;
        }
    }

    if let Some(filter) = query.callsign_filter.as_deref() {
        let normalized_filter = filter.trim().to_ascii_uppercase();
        if !normalized_filter.is_empty()
            && !record
                .station_callsign
                .to_ascii_uppercase()
                .contains(&normalized_filter)
            && !record
                .worked_callsign
                .to_ascii_uppercase()
                .contains(&normalized_filter)
        {
            return false;
        }
    }

    if let Some(band) = query.band_filter {
        if record.band != band as i32 {
            return false;
        }
    }

    if let Some(mode) = query.mode_filter {
        if record.mode != mode as i32 {
            return false;
        }
    }

    if let Some(contest_id) = query.contest_id.as_deref() {
        if record.contest_id.as_deref() != Some(contest_id) {
            return false;
        }
    }

    true
}

fn timestamp_to_millis(timestamp: Option<&prost_types::Timestamp>) -> i64 {
    timestamp.map_or(0, |value| {
        value
            .seconds
            .saturating_mul(1_000)
            .saturating_add(i64::from(value.nanos) / 1_000_000)
    })
}

fn millis_to_timestamp(millis: i64) -> prost_types::Timestamp {
    let seconds = millis.div_euclid(1_000);
    let nanos = i32::try_from(millis.rem_euclid(1_000))
        .unwrap_or(0)
        .saturating_mul(1_000_000);
    prost_types::Timestamp { seconds, nanos }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
    use super::MemoryStorage;
    use prost_types::Timestamp;
    use qsoripper_core::application::logbook::LogbookEngine;
    use qsoripper_core::domain::qso::QsoRecordBuilder;
    use qsoripper_core::proto::qsoripper::domain::{Band, LookupResult, LookupState, Mode};
    use qsoripper_core::storage::{
        DeletedRecordsFilter, EngineStorage, LogbookStore, LookupSnapshot, LookupSnapshotStore,
        QsoListQuery, QsoSortOrder,
    };
    use std::sync::Arc;

    #[tokio::test]
    async fn memory_storage_round_trips_qsos_through_logbook_engine() {
        let storage: Arc<dyn EngineStorage> = Arc::new(MemoryStorage::new());
        let engine = LogbookEngine::new(storage);
        let qso = QsoRecordBuilder::new("W1AW", "K7ABC")
            .band(Band::Band20m)
            .mode(Mode::Ft8)
            .timestamp(Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            })
            .build();

        let stored = engine.log_qso(qso).await.unwrap();
        let loaded = engine.get_qso(&stored.local_id).await.unwrap();

        assert_eq!(loaded.local_id, stored.local_id);
        assert_eq!(loaded.worked_callsign, "K7ABC");
        assert!(loaded.created_at.is_some());
        assert!(loaded.updated_at.is_some());
    }

    #[tokio::test]
    async fn memory_storage_lists_qsos_with_filters_and_sorting() {
        let storage: Arc<dyn EngineStorage> = Arc::new(MemoryStorage::new());
        let engine = LogbookEngine::new(storage.clone());

        let older = QsoRecordBuilder::new("W1AW", "K7OLD")
            .band(Band::Band20m)
            .mode(Mode::Ft8)
            .timestamp(Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            })
            .contest("ARRL-DX")
            .build();
        let newer = QsoRecordBuilder::new("W1AW", "K7NEW")
            .band(Band::Band40m)
            .mode(Mode::Cw)
            .timestamp(Timestamp {
                seconds: 1_700_000_100,
                nanos: 0,
            })
            .build();

        let _ = engine.log_qso(older).await.unwrap();
        let _ = engine.log_qso(newer).await.unwrap();

        let records = engine
            .list_qsos(&QsoListQuery {
                callsign_filter: Some("K7".into()),
                limit: Some(1),
                sort: QsoSortOrder::NewestFirst,
                ..QsoListQuery::default()
            })
            .await
            .unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(
            records
                .first()
                .map(|record| record.worked_callsign.as_str()),
            Some("K7NEW")
        );

        let filtered = engine
            .list_qsos(&QsoListQuery {
                contest_id: Some("ARRL-DX".into()),
                ..QsoListQuery::default()
            })
            .await
            .unwrap();

        assert_eq!(filtered.len(), 1);
        assert_eq!(
            filtered
                .first()
                .map(|record| record.worked_callsign.as_str()),
            Some("K7OLD")
        );
    }

    #[tokio::test]
    async fn memory_storage_persists_lookup_snapshots() {
        let storage = MemoryStorage::new();
        let snapshot = LookupSnapshot {
            callsign: "w1aw".into(),
            result: LookupResult {
                state: LookupState::Found as i32,
                queried_callsign: "W1AW".into(),
                ..LookupResult::default()
            },
            stored_at: Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            },
            expires_at: None,
        };

        storage.upsert_lookup_snapshot(&snapshot).await.unwrap();
        let loaded = storage.get_lookup_snapshot("W1AW").await.unwrap();

        let Some(loaded) = loaded else {
            panic!("Expected persisted lookup snapshot to exist");
        };

        assert_eq!(loaded.callsign, "W1AW");
        assert_eq!(loaded.result.state, LookupState::Found as i32);
    }

    #[tokio::test]
    async fn memory_storage_soft_delete_keeps_row_with_tombstone() {
        let storage = MemoryStorage::new();
        let qso = QsoRecordBuilder::new("W1AW", "K7ABC")
            .band(Band::Band20m)
            .mode(Mode::Ft8)
            .timestamp(Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            })
            .build();
        let local_id = qso.local_id.clone();
        storage.insert_qso(&qso).await.unwrap();

        assert!(storage
            .soft_delete_qso(&local_id, 1_700_000_500_000, true)
            .await
            .unwrap());

        let fetched = storage.get_qso(&local_id).await.unwrap().unwrap();
        assert!(fetched.deleted_at.is_some());
        assert!(fetched.pending_remote_delete);
    }

    #[tokio::test]
    async fn memory_storage_list_qsos_active_only_excludes_soft_deleted() {
        let storage = MemoryStorage::new();
        for callsign in ["A1A", "B2B"] {
            let qso = QsoRecordBuilder::new("W1AW", callsign)
                .band(Band::Band20m)
                .mode(Mode::Ft8)
                .timestamp(Timestamp {
                    seconds: 1_700_000_000,
                    nanos: 0,
                })
                .build();
            storage.insert_qso(&qso).await.unwrap();
        }
        let all = storage
            .list_qsos(&QsoListQuery {
                deleted_filter: DeletedRecordsFilter::All,
                ..QsoListQuery::default()
            })
            .await
            .unwrap();
        let target = all
            .iter()
            .find(|r| r.worked_callsign == "B2B")
            .map(|r| r.local_id.clone())
            .unwrap();

        storage
            .soft_delete_qso(&target, 1_700_000_500_000, false)
            .await
            .unwrap();

        let active = storage.list_qsos(&QsoListQuery::default()).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active.first().unwrap().worked_callsign, "A1A");

        let deleted_only = storage
            .list_qsos(&QsoListQuery {
                deleted_filter: DeletedRecordsFilter::DeletedOnly,
                ..QsoListQuery::default()
            })
            .await
            .unwrap();
        assert_eq!(deleted_only.len(), 1);
        assert_eq!(deleted_only.first().unwrap().worked_callsign, "B2B");

        let all_after = storage
            .list_qsos(&QsoListQuery {
                deleted_filter: DeletedRecordsFilter::All,
                ..QsoListQuery::default()
            })
            .await
            .unwrap();
        assert_eq!(all_after.len(), 2);
    }

    #[tokio::test]
    async fn memory_storage_qso_counts_excludes_soft_deleted() {
        let storage = MemoryStorage::new();
        for cs in ["A", "B"] {
            let qso = QsoRecordBuilder::new("W1AW", cs)
                .band(Band::Band20m)
                .mode(Mode::Ft8)
                .timestamp(Timestamp {
                    seconds: 1_700_000_000,
                    nanos: 0,
                })
                .build();
            storage.insert_qso(&qso).await.unwrap();
        }
        let listed = storage
            .list_qsos(&QsoListQuery {
                deleted_filter: DeletedRecordsFilter::All,
                ..QsoListQuery::default()
            })
            .await
            .unwrap();
        let target = listed
            .iter()
            .find(|r| r.worked_callsign == "B")
            .map(|r| r.local_id.clone())
            .unwrap();
        storage
            .soft_delete_qso(&target, 1_700_000_500_000, false)
            .await
            .unwrap();

        let counts = storage.qso_counts().await.unwrap();
        assert_eq!(counts.local_qso_count, 1);
    }

    #[tokio::test]
    async fn memory_storage_restore_clears_tombstone() {
        let storage = MemoryStorage::new();
        let qso = QsoRecordBuilder::new("W1AW", "K7ABC")
            .band(Band::Band20m)
            .mode(Mode::Ft8)
            .timestamp(Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            })
            .build();
        let local_id = qso.local_id.clone();
        storage.insert_qso(&qso).await.unwrap();
        storage
            .soft_delete_qso(&local_id, 1_700_000_500_000, true)
            .await
            .unwrap();

        assert!(storage.restore_qso(&local_id).await.unwrap());

        let fetched = storage.get_qso(&local_id).await.unwrap().unwrap();
        assert!(fetched.deleted_at.is_none());
        assert!(!fetched.pending_remote_delete);
    }

    #[tokio::test]
    async fn memory_storage_soft_delete_missing_returns_false() {
        let storage = MemoryStorage::new();
        assert!(!storage
            .soft_delete_qso("missing", 1_700_000_500_000, false)
            .await
            .unwrap());
        assert!(!storage.restore_qso("missing").await.unwrap());
    }

    #[test]
    fn timestamp_to_millis_saturates_positive_overflow() {
        let value = super::timestamp_to_millis(Some(&Timestamp {
            seconds: i64::MAX,
            nanos: 999_999_999,
        }));

        assert_eq!(value, i64::MAX);
    }

    #[test]
    fn timestamp_to_millis_saturates_negative_overflow() {
        let value = super::timestamp_to_millis(Some(&Timestamp {
            seconds: i64::MIN,
            nanos: -999_999_999,
        }));

        assert_eq!(value, i64::MIN);
    }

    #[tokio::test]
    async fn memory_purge_removes_only_soft_deleted_rows() {
        let storage: Arc<dyn EngineStorage> = Arc::new(MemoryStorage::new());
        let engine = LogbookEngine::new(storage.clone());

        let active = engine
            .log_qso(
                QsoRecordBuilder::new("W1AW", "K7ACT")
                    .band(Band::Band20m)
                    .mode(Mode::Ft8)
                    .timestamp(Timestamp {
                        seconds: 1_700_000_000,
                        nanos: 0,
                    })
                    .build(),
            )
            .await
            .unwrap();
        let deleted = engine
            .log_qso(
                QsoRecordBuilder::new("W1AW", "K7DEL")
                    .band(Band::Band40m)
                    .mode(Mode::Cw)
                    .timestamp(Timestamp {
                        seconds: 1_700_001_000,
                        nanos: 0,
                    })
                    .build(),
            )
            .await
            .unwrap();
        engine.delete_qso(&deleted.local_id, false).await.unwrap();

        let purged = storage
            .logbook()
            .purge_deleted_qsos(&[], None)
            .await
            .unwrap();
        assert_eq!(purged, 1);

        // Active row survives.
        assert!(storage
            .logbook()
            .get_qso(&active.local_id)
            .await
            .unwrap()
            .is_some());
        // Deleted row is gone.
        assert!(storage
            .logbook()
            .get_qso(&deleted.local_id)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn memory_purge_filters_by_local_ids() {
        let storage: Arc<dyn EngineStorage> = Arc::new(MemoryStorage::new());
        let engine = LogbookEngine::new(storage.clone());

        let d1 = engine
            .log_qso(
                QsoRecordBuilder::new("W1AW", "K7D1")
                    .band(Band::Band20m)
                    .mode(Mode::Ft8)
                    .timestamp(Timestamp {
                        seconds: 1_700_000_000,
                        nanos: 0,
                    })
                    .build(),
            )
            .await
            .unwrap();
        let d2 = engine
            .log_qso(
                QsoRecordBuilder::new("W1AW", "K7D2")
                    .band(Band::Band40m)
                    .mode(Mode::Cw)
                    .timestamp(Timestamp {
                        seconds: 1_700_001_000,
                        nanos: 0,
                    })
                    .build(),
            )
            .await
            .unwrap();
        engine.delete_qso(&d1.local_id, false).await.unwrap();
        engine.delete_qso(&d2.local_id, false).await.unwrap();

        // Purge only d1.
        let purged = storage
            .logbook()
            .purge_deleted_qsos(&[d1.local_id.clone()], None)
            .await
            .unwrap();
        assert_eq!(purged, 1);

        // d2 is still in trash.
        assert!(storage
            .logbook()
            .get_qso(&d2.local_id)
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn memory_purge_filters_by_older_than() {
        let storage: Arc<dyn EngineStorage> = Arc::new(MemoryStorage::new());
        let logbook = storage.logbook();

        // Insert two QSOs and soft-delete at different times.
        let q1 = QsoRecordBuilder::new("W1AW", "K7OLD")
            .band(Band::Band20m)
            .mode(Mode::Ft8)
            .timestamp(Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            })
            .build();
        let q2 = QsoRecordBuilder::new("W1AW", "K7NEW")
            .band(Band::Band40m)
            .mode(Mode::Cw)
            .timestamp(Timestamp {
                seconds: 1_700_001_000,
                nanos: 0,
            })
            .build();

        let engine = LogbookEngine::new(storage.clone());
        let stored1 = engine.log_qso(q1).await.unwrap();
        let stored2 = engine.log_qso(q2).await.unwrap();

        // Soft-delete with explicit timestamps: q1 "old" at 1000ms, q2 "new" at 5000ms.
        logbook
            .soft_delete_qso(&stored1.local_id, 1000, false)
            .await
            .unwrap();
        logbook
            .soft_delete_qso(&stored2.local_id, 5000, false)
            .await
            .unwrap();

        // Purge only rows deleted before 3000ms.
        let purged = logbook.purge_deleted_qsos(&[], Some(3000)).await.unwrap();
        assert_eq!(purged, 1);

        // q1 purged, q2 survives.
        assert!(logbook.get_qso(&stored1.local_id).await.unwrap().is_none());
        assert!(logbook.get_qso(&stored2.local_id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn memory_purge_no_match_returns_zero() {
        let storage: Arc<dyn EngineStorage> = Arc::new(MemoryStorage::new());
        let purged = storage
            .logbook()
            .purge_deleted_qsos(&[], None)
            .await
            .unwrap();
        assert_eq!(purged, 0);
    }

    #[tokio::test]
    async fn memory_storage_history_returns_exact_matches_only() {
        let storage = MemoryStorage::new();
        let logbook = storage.logbook();
        for (id, worked, ts_secs) in [
            ("q1", "K7ABC", 1_700_000_000_i64),
            ("q2", "K7AB", 1_700_001_000),
            ("q3", "K7ABCD", 1_700_002_000),
            ("q4", "k7abc", 1_700_003_000),
        ] {
            let mut qso = QsoRecordBuilder::new("W1AW", worked)
                .timestamp(Timestamp {
                    seconds: ts_secs,
                    nanos: 0,
                })
                .build();
            qso.local_id = id.to_string();
            logbook.insert_qso(&qso).await.unwrap();
        }

        let page = logbook.list_qso_history("k7abc", 10).await.unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.entries.len(), 2);
        assert_eq!(page.entries[0].local_id, "q4");
        assert_eq!(page.entries[1].local_id, "q1");
    }

    #[tokio::test]
    async fn memory_storage_history_excludes_soft_deleted_and_respects_limit() {
        let storage = MemoryStorage::new();
        let logbook = storage.logbook();
        for (id, ts_secs) in [
            ("a", 1_700_000_000_i64),
            ("b", 1_700_001_000),
            ("c", 1_700_002_000),
            ("d", 1_700_003_000),
        ] {
            let mut qso = QsoRecordBuilder::new("W1AW", "K7ABC")
                .timestamp(Timestamp {
                    seconds: ts_secs,
                    nanos: 0,
                })
                .build();
            qso.local_id = id.to_string();
            logbook.insert_qso(&qso).await.unwrap();
        }
        logbook
            .soft_delete_qso("c", 1_700_500_000_000, false)
            .await
            .unwrap();

        let page = logbook.list_qso_history("K7ABC", 2).await.unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.entries.len(), 2);
        assert_eq!(page.entries[0].local_id, "d");
        assert_eq!(page.entries[1].local_id, "b");

        let zero = logbook.list_qso_history("K7ABC", 0).await.unwrap();
        assert_eq!(zero.total, 3);
        assert!(zero.entries.is_empty());

        let none = logbook.list_qso_history("NEVER", 5).await.unwrap();
        assert_eq!(none.total, 0);
        assert!(none.entries.is_empty());
    }
}

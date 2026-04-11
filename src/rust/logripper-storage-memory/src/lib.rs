//! In-memory storage adapter for `LogRipper` engine services.

use logripper_core::application::logbook::is_pending_sync_status;
use logripper_core::domain::lookup::normalize_callsign;
use logripper_core::proto::logripper::domain::QsoRecord;
use logripper_core::storage::{
    EngineStorage, LogbookCounts, LogbookStore, LookupSnapshot, LookupSnapshotStore, QsoListQuery,
    QsoSortOrder, StorageError, SyncMetadata,
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

    async fn qso_counts(&self) -> Result<LogbookCounts, StorageError> {
        let state = self.state.read().await;
        let pending_upload_count = state
            .qsos
            .values()
            .filter(|record| is_pending_sync_status(record.sync_status))
            .count();

        Ok(LogbookCounts {
            local_qso_count: u32::try_from(state.qsos.len())
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::MemoryStorage;
    use logripper_core::application::logbook::LogbookEngine;
    use logripper_core::domain::qso::QsoRecordBuilder;
    use logripper_core::proto::logripper::domain::{Band, LookupResult, LookupState, Mode};
    use logripper_core::storage::{
        EngineStorage, LookupSnapshot, LookupSnapshotStore, QsoListQuery, QsoSortOrder,
    };
    use prost_types::Timestamp;
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
}

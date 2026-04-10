//! `SQLite` storage adapter for `LogRipper` engine services.

mod builder;
mod migrations;

use logripper_core::domain::lookup::normalize_callsign;
use logripper_core::proto::logripper::domain::{LookupResult, QsoRecord, SyncStatus};
use logripper_core::storage::{
    EngineStorage, LogbookCounts, LogbookStore, LookupSnapshot, LookupSnapshotStore, QsoListQuery,
    QsoSortOrder, StorageError, SyncMetadata,
};
use prost::Message;
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, ErrorCode, OptionalExtension};
use std::sync::{Mutex, MutexGuard};

pub use builder::SqliteStorageBuilder;

/// SQLite-backed storage implementation for engine-owned persistence.
pub struct SqliteStorage {
    pub(crate) connection: Mutex<Connection>,
}

impl SqliteStorage {
    fn connection(&self) -> Result<MutexGuard<'_, Connection>, StorageError> {
        self.connection
            .lock()
            .map_err(|_| StorageError::backend("SQLite connection mutex was poisoned"))
    }
}

impl EngineStorage for SqliteStorage {
    fn logbook(&self) -> &dyn LogbookStore {
        self
    }

    fn lookup_snapshots(&self) -> &dyn LookupSnapshotStore {
        self
    }

    fn backend_name(&self) -> &'static str {
        "sqlite"
    }
}

#[tonic::async_trait]
impl LogbookStore for SqliteStorage {
    async fn insert_qso(&self, qso: &QsoRecord) -> Result<(), StorageError> {
        let connection = self.connection()?;
        let encoded = encode_message(qso);

        connection
            .execute(
                "INSERT INTO qsos (
                    local_id,
                    qrz_logid,
                    qrz_bookid,
                    station_callsign,
                    worked_callsign,
                    utc_timestamp_ms,
                    band,
                    mode,
                    contest_id,
                    created_at_ms,
                    updated_at_ms,
                    sync_status,
                    record
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    qso.local_id,
                    qso.qrz_logid,
                    qso.qrz_bookid,
                    qso.station_callsign,
                    qso.worked_callsign,
                    timestamp_to_millis(qso.utc_timestamp.as_ref()),
                    qso.band,
                    qso.mode,
                    qso.contest_id,
                    timestamp_to_millis(qso.created_at.as_ref()),
                    timestamp_to_millis(qso.updated_at.as_ref()),
                    qso.sync_status,
                    encoded,
                ],
            )
            .map_err(|err| map_insert_error(err, &qso.local_id))?;

        Ok(())
    }

    async fn update_qso(&self, qso: &QsoRecord) -> Result<bool, StorageError> {
        let connection = self.connection()?;
        let encoded = encode_message(qso);
        let rows = connection
            .execute(
                "UPDATE qsos
                 SET qrz_logid = ?,
                     qrz_bookid = ?,
                     station_callsign = ?,
                     worked_callsign = ?,
                     utc_timestamp_ms = ?,
                     band = ?,
                     mode = ?,
                     contest_id = ?,
                     created_at_ms = ?,
                     updated_at_ms = ?,
                     sync_status = ?,
                     record = ?
                 WHERE local_id = ?",
                params![
                    qso.qrz_logid,
                    qso.qrz_bookid,
                    qso.station_callsign,
                    qso.worked_callsign,
                    timestamp_to_millis(qso.utc_timestamp.as_ref()),
                    qso.band,
                    qso.mode,
                    qso.contest_id,
                    timestamp_to_millis(qso.created_at.as_ref()),
                    timestamp_to_millis(qso.updated_at.as_ref()),
                    qso.sync_status,
                    encoded,
                    qso.local_id,
                ],
            )
            .map_err(|err| StorageError::backend(err.to_string()))?;

        Ok(rows > 0)
    }

    async fn delete_qso(&self, local_id: &str) -> Result<bool, StorageError> {
        let connection = self.connection()?;
        let rows = connection
            .execute("DELETE FROM qsos WHERE local_id = ?", params![local_id])
            .map_err(|err| StorageError::backend(err.to_string()))?;

        Ok(rows > 0)
    }

    async fn get_qso(&self, local_id: &str) -> Result<Option<QsoRecord>, StorageError> {
        let connection = self.connection()?;
        let payload = connection
            .query_row(
                "SELECT record FROM qsos WHERE local_id = ?",
                params![local_id],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(|err| StorageError::backend(err.to_string()))?;

        payload.map(|bytes| decode_qso(&bytes)).transpose()
    }

    async fn list_qsos(&self, query: &QsoListQuery) -> Result<Vec<QsoRecord>, StorageError> {
        let connection = self.connection()?;
        let mut sql = String::from("SELECT record FROM qsos WHERE 1 = 1");
        let mut values = Vec::<Value>::new();

        if let Some(after) = query.after.as_ref() {
            sql.push_str(" AND utc_timestamp_ms >= ?");
            values.push(Value::Integer(
                timestamp_to_millis(Some(after)).unwrap_or(0),
            ));
        }

        if let Some(before) = query.before.as_ref() {
            sql.push_str(" AND utc_timestamp_ms <= ?");
            values.push(Value::Integer(
                timestamp_to_millis(Some(before)).unwrap_or(0),
            ));
        }

        if let Some(filter) = query.callsign_filter.as_deref() {
            let pattern = format!("%{}%", filter.trim().to_ascii_uppercase());
            sql.push_str(" AND (UPPER(station_callsign) LIKE ? OR UPPER(worked_callsign) LIKE ?)");
            values.push(Value::Text(pattern.clone()));
            values.push(Value::Text(pattern));
        }

        if let Some(band) = query.band_filter {
            sql.push_str(" AND band = ?");
            values.push(Value::Integer(i64::from(band as i32)));
        }

        if let Some(mode) = query.mode_filter {
            sql.push_str(" AND mode = ?");
            values.push(Value::Integer(i64::from(mode as i32)));
        }

        if let Some(contest_id) = query.contest_id.as_deref() {
            sql.push_str(" AND contest_id = ?");
            values.push(Value::Text(contest_id.to_string()));
        }

        match query.sort {
            QsoSortOrder::NewestFirst => {
                sql.push_str(" ORDER BY utc_timestamp_ms DESC, local_id DESC");
            }
            QsoSortOrder::OldestFirst => {
                sql.push_str(" ORDER BY utc_timestamp_ms ASC, local_id ASC");
            }
        }

        if let Some(limit) = query.limit {
            sql.push_str(" LIMIT ? OFFSET ?");
            values.push(Value::Integer(i64::from(limit)));
            values.push(Value::Integer(i64::from(query.offset)));
        } else if query.offset > 0 {
            sql.push_str(" LIMIT -1 OFFSET ?");
            values.push(Value::Integer(i64::from(query.offset)));
        }

        let mut statement = connection
            .prepare(&sql)
            .map_err(|err| StorageError::backend(err.to_string()))?;
        let rows = statement
            .query_map(params_from_iter(values), |row| row.get::<_, Vec<u8>>(0))
            .map_err(|err| StorageError::backend(err.to_string()))?;

        let payloads = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| StorageError::backend(err.to_string()))?;

        payloads
            .into_iter()
            .map(|payload| decode_qso(&payload))
            .collect()
    }

    async fn qso_counts(&self) -> Result<LogbookCounts, StorageError> {
        let connection = self.connection()?;
        let local_qso_count = connection
            .query_row("SELECT COUNT(*) FROM qsos", [], |row| row.get::<_, i64>(0))
            .map_err(|err| StorageError::backend(err.to_string()))?;
        let pending_upload_count = connection
            .query_row(
                "SELECT COUNT(*) FROM qsos WHERE sync_status != ?",
                params![SyncStatus::Synced as i32],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|err| StorageError::backend(err.to_string()))?;

        Ok(LogbookCounts {
            local_qso_count: u32::try_from(local_qso_count)
                .map_err(|_| StorageError::backend("local_qso_count exceeds u32"))?,
            pending_upload_count: u32::try_from(pending_upload_count)
                .map_err(|_| StorageError::backend("pending_upload_count exceeds u32"))?,
        })
    }

    async fn get_sync_metadata(&self) -> Result<SyncMetadata, StorageError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT qrz_qso_count, last_sync_ms, qrz_logbook_owner
                 FROM sync_metadata
                 WHERE id = 1",
                [],
                |row| {
                    Ok(SyncMetadata {
                        qrz_qso_count: row.get::<_, u32>(0)?,
                        last_sync: millis_to_timestamp(row.get::<_, Option<i64>>(1)?),
                        qrz_logbook_owner: row.get::<_, Option<String>>(2)?,
                    })
                },
            )
            .map_err(|err| StorageError::backend(err.to_string()))
    }

    async fn upsert_sync_metadata(&self, metadata: &SyncMetadata) -> Result<(), StorageError> {
        let connection = self.connection()?;
        connection
            .execute(
                "INSERT INTO sync_metadata (id, qrz_qso_count, last_sync_ms, qrz_logbook_owner)
                 VALUES (1, ?, ?, ?)
                 ON CONFLICT(id) DO UPDATE SET
                    qrz_qso_count = excluded.qrz_qso_count,
                    last_sync_ms = excluded.last_sync_ms,
                    qrz_logbook_owner = excluded.qrz_logbook_owner",
                params![
                    metadata.qrz_qso_count,
                    timestamp_to_millis(metadata.last_sync.as_ref()),
                    metadata.qrz_logbook_owner,
                ],
            )
            .map_err(|err| StorageError::backend(err.to_string()))?;

        Ok(())
    }
}

#[tonic::async_trait]
impl LookupSnapshotStore for SqliteStorage {
    async fn get_lookup_snapshot(
        &self,
        callsign: &str,
    ) -> Result<Option<LookupSnapshot>, StorageError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT callsign, result, stored_at_ms, expires_at_ms
                 FROM lookup_snapshots
                 WHERE callsign = ?",
                params![normalize_callsign(callsign)],
                |row| {
                    let payload = row.get::<_, Vec<u8>>(1)?;
                    let result = LookupResult::decode(payload.as_slice()).map_err(|err| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Blob,
                            Box::new(err),
                        )
                    })?;

                    Ok(LookupSnapshot {
                        callsign: row.get::<_, String>(0)?,
                        result,
                        stored_at: millis_to_timestamp(row.get::<_, Option<i64>>(2)?)
                            .unwrap_or_default(),
                        expires_at: millis_to_timestamp(row.get::<_, Option<i64>>(3)?),
                    })
                },
            )
            .optional()
            .map_err(|err| StorageError::backend(err.to_string()))
    }

    async fn upsert_lookup_snapshot(&self, snapshot: &LookupSnapshot) -> Result<(), StorageError> {
        let connection = self.connection()?;
        let normalized_callsign = normalize_callsign(&snapshot.callsign);
        let encoded = encode_message(&snapshot.result);

        connection
            .execute(
                "INSERT INTO lookup_snapshots (callsign, result, stored_at_ms, expires_at_ms)
                 VALUES (?, ?, ?, ?)
                 ON CONFLICT(callsign) DO UPDATE SET
                    result = excluded.result,
                    stored_at_ms = excluded.stored_at_ms,
                    expires_at_ms = excluded.expires_at_ms",
                params![
                    normalized_callsign,
                    encoded,
                    timestamp_to_millis(Some(&snapshot.stored_at)),
                    timestamp_to_millis(snapshot.expires_at.as_ref()),
                ],
            )
            .map_err(|err| StorageError::backend(err.to_string()))?;

        Ok(())
    }

    async fn delete_lookup_snapshot(&self, callsign: &str) -> Result<bool, StorageError> {
        let connection = self.connection()?;
        let rows = connection
            .execute(
                "DELETE FROM lookup_snapshots WHERE callsign = ?",
                params![normalize_callsign(callsign)],
            )
            .map_err(|err| StorageError::backend(err.to_string()))?;

        Ok(rows > 0)
    }
}

fn encode_message<T: Message>(message: &T) -> Vec<u8> {
    message.encode_to_vec()
}

fn decode_qso(payload: &[u8]) -> Result<QsoRecord, StorageError> {
    QsoRecord::decode(payload).map_err(|err| StorageError::CorruptData(err.to_string()))
}

fn map_insert_error(error: rusqlite::Error, local_id: &str) -> StorageError {
    match error {
        rusqlite::Error::SqliteFailure(result, _)
            if result.code == ErrorCode::ConstraintViolation =>
        {
            StorageError::duplicate("qso", local_id)
        }
        other => StorageError::backend(other.to_string()),
    }
}

fn timestamp_to_millis(timestamp: Option<&prost_types::Timestamp>) -> Option<i64> {
    timestamp.map(|value| value.seconds.saturating_mul(1_000) + i64::from(value.nanos) / 1_000_000)
}

fn millis_to_timestamp(millis: Option<i64>) -> Option<prost_types::Timestamp> {
    millis.map(|value| prost_types::Timestamp {
        seconds: value.div_euclid(1_000),
        nanos: i32::try_from(value.rem_euclid(1_000) * 1_000_000).unwrap_or(0),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::SqliteStorageBuilder;
    use logripper_core::application::logbook::LogbookEngine;
    use logripper_core::domain::qso::QsoRecordBuilder;
    use logripper_core::proto::logripper::domain::{Band, LookupResult, LookupState, Mode};
    use logripper_core::storage::{
        EngineStorage, LookupSnapshot, LookupSnapshotStore, QsoListQuery,
    };
    use prost_types::Timestamp;
    use std::sync::Arc;

    #[tokio::test]
    async fn sqlite_storage_round_trips_qsos_through_logbook_engine() {
        let storage: Arc<dyn EngineStorage> =
            Arc::new(SqliteStorageBuilder::new().in_memory().build().unwrap());
        let engine = LogbookEngine::new(storage);
        let qso = QsoRecordBuilder::new("W1AW", "K7ABC")
            .band(Band::Band20m)
            .mode(Mode::Ft8)
            .build();

        let stored = engine.log_qso(qso).await.unwrap();
        let loaded = engine.get_qso(&stored.local_id).await.unwrap();

        assert_eq!(loaded.local_id, stored.local_id);
        assert_eq!(loaded.worked_callsign, "K7ABC");
        assert!(loaded.created_at.is_some());
        assert!(loaded.updated_at.is_some());
    }

    #[tokio::test]
    async fn sqlite_storage_lists_qsos_with_filters() {
        let storage: Arc<dyn EngineStorage> =
            Arc::new(SqliteStorageBuilder::new().in_memory().build().unwrap());
        let engine = LogbookEngine::new(storage);

        let first = QsoRecordBuilder::new("W1AW", "K7ONE")
            .band(Band::Band20m)
            .mode(Mode::Ft8)
            .timestamp(Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            })
            .contest("CQ-WW")
            .build();
        let second = QsoRecordBuilder::new("W1AW", "K7TWO")
            .band(Band::Band40m)
            .mode(Mode::Cw)
            .timestamp(Timestamp {
                seconds: 1_700_000_100,
                nanos: 0,
            })
            .build();

        let _ = engine.log_qso(first).await.unwrap();
        let _ = engine.log_qso(second).await.unwrap();

        let listed = engine
            .list_qsos(&QsoListQuery {
                contest_id: Some("CQ-WW".into()),
                ..QsoListQuery::default()
            })
            .await
            .unwrap();

        assert_eq!(listed.len(), 1);
        assert_eq!(
            listed.first().map(|record| record.worked_callsign.as_str()),
            Some("K7ONE")
        );
    }

    #[tokio::test]
    async fn sqlite_storage_persists_lookup_snapshots() {
        let storage = SqliteStorageBuilder::new().in_memory().build().unwrap();
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
}

//! Logbook workflows built on top of the engine-owned storage ports.

use crate::domain::qso::new_local_id;
use crate::proto::logripper::domain::{QsoRecord, SyncStatus};
use crate::storage::{EngineStorage, LogbookCounts, QsoListQuery, StorageError, SyncMetadata};
use chrono::Utc;
use prost_types::Timestamp;
use std::sync::Arc;
use thiserror::Error;

/// Coordinates QSO CRUD and sync-status reads through a storage backend.
#[derive(Clone)]
pub struct LogbookEngine {
    storage: Arc<dyn EngineStorage>,
}

impl LogbookEngine {
    /// Create a logbook engine backed by the provided storage implementation.
    #[must_use]
    pub fn new(storage: Arc<dyn EngineStorage>) -> Self {
        Self { storage }
    }

    /// Return the active storage backend name for diagnostics and startup logs.
    #[must_use]
    pub fn storage_backend_name(&self) -> &'static str {
        self.storage.backend_name()
    }

    /// Persist a new QSO and return the normalized stored record.
    ///
    /// # Errors
    ///
    /// Returns [`LogbookError::Validation`] when required fields are missing and
    /// [`LogbookError::Storage`] when the configured backend rejects the write.
    pub async fn log_qso(&self, mut qso: QsoRecord) -> Result<QsoRecord, LogbookError> {
        validate_required_callsigns(&qso)?;

        if qso.local_id.trim().is_empty() {
            qso.local_id = new_local_id();
        }

        let now = now_timestamp();
        if qso.created_at.is_none() {
            qso.created_at = Some(now);
        }
        qso.updated_at = Some(now);

        self.storage.logbook().insert_qso(&qso).await?;
        Ok(qso)
    }

    /// Update an existing QSO and return the normalized stored record.
    ///
    /// # Errors
    ///
    /// Returns [`LogbookError::Validation`] when the record is missing required
    /// fields, [`LogbookError::NotFound`] when the local ID does not exist, and
    /// [`LogbookError::Storage`] when the backend write fails.
    pub async fn update_qso(&self, mut qso: QsoRecord) -> Result<QsoRecord, LogbookError> {
        validate_required_callsigns(&qso)?;
        if qso.local_id.trim().is_empty() {
            return Err(LogbookError::Validation(
                "local_id is required when updating a QSO.".into(),
            ));
        }

        if qso.created_at.is_none() {
            qso.created_at = Some(now_timestamp());
        }
        qso.updated_at = Some(now_timestamp());

        let updated = self.storage.logbook().update_qso(&qso).await?;
        if updated {
            Ok(qso)
        } else {
            Err(LogbookError::NotFound(qso.local_id))
        }
    }

    /// Delete a QSO by local identifier.
    ///
    /// # Errors
    ///
    /// Returns [`LogbookError::Validation`] when the local ID is blank,
    /// [`LogbookError::NotFound`] when the record does not exist, and
    /// [`LogbookError::Storage`] when the backend delete fails.
    pub async fn delete_qso(&self, local_id: &str) -> Result<(), LogbookError> {
        if local_id.trim().is_empty() {
            return Err(LogbookError::Validation(
                "local_id is required when deleting a QSO.".into(),
            ));
        }

        let deleted = self.storage.logbook().delete_qso(local_id).await?;
        if deleted {
            Ok(())
        } else {
            Err(LogbookError::NotFound(local_id.to_string()))
        }
    }

    /// Retrieve a persisted QSO by local identifier.
    ///
    /// # Errors
    ///
    /// Returns [`LogbookError::Validation`] when the local ID is blank,
    /// [`LogbookError::NotFound`] when the record does not exist, and
    /// [`LogbookError::Storage`] when the backend read fails.
    pub async fn get_qso(&self, local_id: &str) -> Result<QsoRecord, LogbookError> {
        if local_id.trim().is_empty() {
            return Err(LogbookError::Validation(
                "local_id is required when loading a QSO.".into(),
            ));
        }

        self.storage
            .logbook()
            .get_qso(local_id)
            .await?
            .ok_or_else(|| LogbookError::NotFound(local_id.to_string()))
    }

    /// List QSOs using the provided filter and pagination options.
    ///
    /// # Errors
    ///
    /// Returns [`LogbookError::Storage`] when the backend query fails.
    pub async fn list_qsos(&self, query: &QsoListQuery) -> Result<Vec<QsoRecord>, LogbookError> {
        self.storage
            .logbook()
            .list_qsos(query)
            .await
            .map_err(Into::into)
    }

    /// Return the current aggregate sync status derived from local and remote metadata.
    ///
    /// # Errors
    ///
    /// Returns [`LogbookError::Storage`] when the backend count or metadata reads fail.
    pub async fn get_sync_status(&self) -> Result<LogbookSyncStatus, LogbookError> {
        let counts = self.storage.logbook().qso_counts().await?;
        let metadata = self.storage.logbook().get_sync_metadata().await?;

        Ok(LogbookSyncStatus::from_parts(counts, metadata))
    }

    /// Update the stored remote sync metadata.
    ///
    /// # Errors
    ///
    /// Returns [`LogbookError::Storage`] when the backend write fails.
    pub async fn update_sync_metadata(&self, metadata: &SyncMetadata) -> Result<(), LogbookError> {
        self.storage
            .logbook()
            .upsert_sync_metadata(metadata)
            .await
            .map_err(Into::into)
    }
}

/// Aggregated sync status returned by the logbook engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogbookSyncStatus {
    /// Total number of locally persisted QSOs.
    pub local_qso_count: u32,
    /// Number of local QSOs that are not yet fully synced.
    pub pending_upload: u32,
    /// Latest remote QSO count reported by QRZ metadata.
    pub qrz_qso_count: u32,
    /// Timestamp of the last completed QRZ sync, if known.
    pub last_sync: Option<Timestamp>,
    /// Remote logbook owner reported by QRZ, if known.
    pub qrz_logbook_owner: Option<String>,
}

impl LogbookSyncStatus {
    fn from_parts(counts: LogbookCounts, metadata: SyncMetadata) -> Self {
        Self {
            local_qso_count: counts.local_qso_count,
            pending_upload: counts.pending_upload_count,
            qrz_qso_count: metadata.qrz_qso_count,
            last_sync: metadata.last_sync,
            qrz_logbook_owner: metadata.qrz_logbook_owner,
        }
    }
}

/// Errors returned by logbook workflows before they are translated to transport status codes.
#[derive(Debug, Error)]
pub enum LogbookError {
    /// The incoming request is missing required fields or contains invalid values.
    #[error("Validation failed: {0}")]
    Validation(String),
    /// The requested QSO could not be found in persistent storage.
    #[error("QSO '{0}' was not found.")]
    NotFound(String),
    /// The underlying storage layer failed to complete the requested operation.
    #[error(transparent)]
    Storage(#[from] StorageError),
}

fn validate_required_callsigns(qso: &QsoRecord) -> Result<(), LogbookError> {
    if qso.station_callsign.trim().is_empty() {
        return Err(LogbookError::Validation(
            "station_callsign is required.".into(),
        ));
    }

    if qso.worked_callsign.trim().is_empty() {
        return Err(LogbookError::Validation(
            "worked_callsign is required.".into(),
        ));
    }

    Ok(())
}

fn now_timestamp() -> Timestamp {
    let now = Utc::now();
    Timestamp {
        seconds: now.timestamp(),
        nanos: i32::try_from(now.timestamp_subsec_nanos()).unwrap_or(0),
    }
}

/// Return whether a QSO should count as pending upload work.
#[must_use]
pub fn is_pending_sync_status(sync_status: i32) -> bool {
    sync_status != SyncStatus::Synced as i32
}

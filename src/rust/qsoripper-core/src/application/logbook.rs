//! Logbook workflows built on top of the engine-owned storage ports.

use crate::domain::qso::new_local_id;
use crate::domain::station::{
    materialize_station_snapshot_for_create, materialize_station_snapshot_for_update,
    station_snapshot_has_values,
};
use crate::proto::qsoripper::domain::{Band, Mode, QsoRecord, StationProfile, SyncStatus};
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
        materialize_station_snapshot_for_create(&mut qso, None);
        normalize_qso_for_persistence(&mut qso);
        validate_qso_for_persistence(&qso)?;

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

    /// Persist a new QSO using the supplied active station profile as the base snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`LogbookError::Validation`] when required fields are missing and
    /// [`LogbookError::Storage`] when the configured backend rejects the write.
    pub async fn log_qso_with_station_profile(
        &self,
        mut qso: QsoRecord,
        active_station_profile: Option<&crate::proto::qsoripper::domain::StationProfile>,
    ) -> Result<QsoRecord, LogbookError> {
        materialize_station_snapshot_for_create(&mut qso, active_station_profile);
        normalize_qso_for_persistence(&mut qso);
        validate_qso_for_persistence(&qso)?;

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
        if qso.local_id.trim().is_empty() {
            return Err(LogbookError::Validation(
                "local_id is required when updating a QSO.".into(),
            ));
        }

        let existing = self.storage.logbook().get_qso(&qso.local_id).await?;
        let existing = existing.ok_or_else(|| LogbookError::NotFound(qso.local_id.clone()))?;
        materialize_station_snapshot_for_update(&mut qso, Some(&existing));
        normalize_qso_for_persistence(&mut qso);
        validate_qso_for_persistence(&qso)?;

        qso.created_at = existing.created_at.or_else(|| Some(now_timestamp()));
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

    /// Import ADIF-mapped QSOs into the logbook with duplicate detection and explicit fallback rules.
    ///
    /// Imported station history wins. The active station profile is only applied when a record carries
    /// no local-station context at all, and that fallback is reported in the returned warnings.
    ///
    /// # Errors
    ///
    /// Returns [`LogbookError::Storage`] when the backend fails while checking duplicates or
    /// persisting an imported record.
    pub async fn import_adif_qsos(
        &self,
        qsos: Vec<QsoRecord>,
        active_station_profile: Option<&StationProfile>,
    ) -> Result<AdifImportSummary, LogbookError> {
        let mut summary = AdifImportSummary::default();

        for (index, qso) in qsos.into_iter().enumerate() {
            let outcome = self
                .import_single_adif_qso(index + 1, qso, active_station_profile)
                .await?;

            if outcome.imported {
                summary.records_imported = summary.records_imported.saturating_add(1);
            } else {
                summary.records_skipped = summary.records_skipped.saturating_add(1);
            }

            summary.warnings.extend(outcome.warnings);
        }

        Ok(summary)
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

    async fn import_single_adif_qso(
        &self,
        record_number: usize,
        mut qso: QsoRecord,
        active_station_profile: Option<&StationProfile>,
    ) -> Result<AdifImportOutcome, LogbookError> {
        let had_imported_station_context = qso_has_station_context(&qso);
        let mut warnings = Vec::new();

        if had_imported_station_context {
            materialize_station_snapshot_for_create(&mut qso, None);
        } else if let Some(active_station_profile) = active_station_profile {
            materialize_station_snapshot_for_create(&mut qso, Some(active_station_profile));
            warnings.push(format!(
                "Record {record_number}: local-station history was absent in ADIF; applied active station profile '{}'.",
                station_profile_label(active_station_profile)
            ));
        } else {
            return Ok(AdifImportOutcome::skipped(format!(
                "Record {record_number}: local-station history was absent in ADIF and no active station profile is configured; skipped."
            )));
        }

        normalize_qso_for_persistence(&mut qso);

        if let Some(reason) = invalid_import_reason(&qso) {
            return Ok(AdifImportOutcome::skipped(format!(
                "Record {record_number}: {reason} Skipped."
            )));
        }

        if self.is_duplicate_import(&qso).await? {
            warnings.push(format!(
                "Record {record_number}: duplicate skipped; matched an existing QSO on station_callsign, worked_callsign, utc_timestamp, band, mode, and compatible submode/frequency."
            ));
            return Ok(AdifImportOutcome {
                imported: false,
                warnings,
            });
        }

        let _ = self.log_qso(qso).await?;
        Ok(AdifImportOutcome {
            imported: true,
            warnings,
        })
    }

    async fn is_duplicate_import(&self, candidate: &QsoRecord) -> Result<bool, LogbookError> {
        let Some(timestamp) = candidate.utc_timestamp else {
            return Ok(false);
        };

        let callsign_filter = non_empty_trimmed(&candidate.worked_callsign)
            .or_else(|| non_empty_trimmed(&candidate.station_callsign));
        let existing = self
            .storage
            .logbook()
            .list_qsos(&QsoListQuery {
                after: Some(timestamp),
                before: Some(timestamp),
                callsign_filter,
                ..QsoListQuery::default()
            })
            .await?;

        Ok(existing
            .iter()
            .any(|existing_qso| qsos_match_for_duplicate(existing_qso, candidate)))
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

/// Summary of an ADIF import run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdifImportSummary {
    /// Number of records inserted into storage.
    pub records_imported: u32,
    /// Number of records skipped due to duplicates or validation problems.
    pub records_skipped: u32,
    /// Human-readable warnings collected during import.
    pub warnings: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct AdifImportOutcome {
    imported: bool,
    warnings: Vec<String>,
}

impl AdifImportOutcome {
    fn skipped(warning: String) -> Self {
        Self {
            imported: false,
            warnings: vec![warning],
        }
    }
}

fn normalize_qso_for_persistence(qso: &mut QsoRecord) {
    qso.station_callsign = qso.station_callsign.trim().to_string();
    qso.worked_callsign = qso.worked_callsign.trim().to_string();
}

fn validate_qso_for_persistence(qso: &QsoRecord) -> Result<(), LogbookError> {
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

    if qso.utc_timestamp.is_none() {
        return Err(LogbookError::Validation(
            "utc_timestamp is required.".into(),
        ));
    }

    match Band::try_from(qso.band) {
        Ok(Band::Unspecified) => {
            return Err(LogbookError::Validation("band is required.".into()));
        }
        Ok(_) => {}
        Err(_) => {
            return Err(LogbookError::Validation("band is invalid.".into()));
        }
    }

    match Mode::try_from(qso.mode) {
        Ok(Mode::Unspecified) => {
            return Err(LogbookError::Validation("mode is required.".into()));
        }
        Ok(_) => {}
        Err(_) => {
            return Err(LogbookError::Validation("mode is invalid.".into()));
        }
    }

    Ok(())
}

fn invalid_import_reason(qso: &QsoRecord) -> Option<String> {
    if qso.station_callsign.trim().is_empty() {
        Some("station_callsign is required.".to_string())
    } else if qso.worked_callsign.trim().is_empty() {
        Some("worked_callsign is required.".to_string())
    } else if qso.utc_timestamp.is_none() {
        if let Some(raw_date) = qso.extra_fields.get("QSO_DATE") {
            Some(format!(
                "invalid ADIF date/time '{raw_date}{}'.",
                qso.extra_fields
                    .get("TIME_ON")
                    .map_or(String::new(), |time| format!("/{time}"))
            ))
        } else {
            Some("utc_timestamp is required.".to_string())
        }
    } else {
        match Band::try_from(qso.band) {
            Ok(Band::Unspecified) => Some(match qso.extra_fields.get("BAND") {
                Some(raw_band) => format!("unrecognized ADIF band '{raw_band}'."),
                None => "band is required.".to_string(),
            }),
            Ok(_) => match Mode::try_from(qso.mode) {
                Ok(Mode::Unspecified) => Some(match qso.extra_fields.get("MODE") {
                    Some(raw_mode) => format!("unrecognized ADIF mode '{raw_mode}'."),
                    None => "mode is required.".to_string(),
                }),
                Ok(_) => None,
                Err(_) => Some("mode is invalid.".to_string()),
            },
            Err(_) => Some("band is invalid.".to_string()),
        }
    }
}

fn qso_has_station_context(qso: &QsoRecord) -> bool {
    !qso.station_callsign.trim().is_empty()
        || qso
            .station_snapshot
            .as_ref()
            .is_some_and(station_snapshot_has_values)
}

fn station_profile_label(profile: &StationProfile) -> String {
    non_empty_trimmed_option(profile.profile_name.as_deref())
        .or_else(|| non_empty_trimmed_option(Some(profile.station_callsign.as_str())))
        .unwrap_or_else(|| "active station profile".to_string())
}

fn qsos_match_for_duplicate(existing: &QsoRecord, candidate: &QsoRecord) -> bool {
    timestamps_match(
        existing.utc_timestamp.as_ref(),
        candidate.utc_timestamp.as_ref(),
    ) && existing.band == candidate.band
        && existing.mode == candidate.mode
        && strings_equal_ignore_ascii_case(
            existing.station_callsign.as_str(),
            candidate.station_callsign.as_str(),
        )
        && strings_equal_ignore_ascii_case(
            existing.worked_callsign.as_str(),
            candidate.worked_callsign.as_str(),
        )
        && optional_strings_compatible(existing.submode.as_deref(), candidate.submode.as_deref())
        && optional_u64_compatible(existing.frequency_khz, candidate.frequency_khz)
}

fn timestamps_match(left: Option<&Timestamp>, right: Option<&Timestamp>) -> bool {
    left == right
}

fn strings_equal_ignore_ascii_case(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn optional_strings_compatible(left: Option<&str>, right: Option<&str>) -> bool {
    match (
        non_empty_trimmed_option(left),
        non_empty_trimmed_option(right),
    ) {
        (Some(left), Some(right)) => left.eq_ignore_ascii_case(&right),
        _ => true,
    }
}

fn optional_u64_compatible(left: Option<u64>, right: Option<u64>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left == right,
        _ => true,
    }
}

fn non_empty_trimmed(value: &str) -> Option<String> {
    non_empty_trimmed_option(Some(value))
}

fn non_empty_trimmed_option(value: Option<&str>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
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

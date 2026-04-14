//! Bidirectional QRZ logbook sync workflow.
//!
//! **Phase 1** — Download QSOs from QRZ and merge into local storage.
//! **Phase 2** — Upload local-only and modified QSOs to QRZ.
//! **Phase 3** — Update sync metadata with the current timestamp.

use std::collections::HashMap;

use qsoripper_core::domain::qso::new_local_id;
use qsoripper_core::proto::qsoripper::domain::{ConflictPolicy, QsoRecord, SyncStatus};
use qsoripper_core::proto::qsoripper::services::SyncWithQrzResponse;
use qsoripper_core::qrz_logbook::{QrzLogbookClient, QrzLogbookError, QrzUploadResult};
use qsoripper_core::storage::{LogbookStore, QsoListQuery, SyncMetadata};
use tokio::sync::mpsc;
use tonic::Status;

// ---------------------------------------------------------------------------
// Type aliases (avoids clippy::type_complexity)
// ---------------------------------------------------------------------------

/// Index from QRZ logbook-record-id → local QSO.
type LogidIndex = HashMap<String, QsoRecord>;

/// Index from (callsign, band, mode) → local QSOs with that key.
type FuzzyIndex = HashMap<(String, i32, i32), Vec<QsoRecord>>;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Extra-field key that QRZ ADIF responses use for the logbook record ID.
const QRZ_LOGID_EXTRA_FIELD: &str = "APP_QRZ_LOGID";

/// Maximum time difference (seconds) for fuzzy timestamp matching.
const TIMESTAMP_TOLERANCE_SECONDS: i64 = 60;

// ---------------------------------------------------------------------------
// Trait — testable abstraction over the real QRZ HTTP client
// ---------------------------------------------------------------------------

/// QRZ logbook HTTP operations required by the sync workflow.
///
/// Extracted into a trait so unit tests can substitute a mock without hitting
/// the network.
#[tonic::async_trait]
pub(crate) trait QrzLogbookApi: Send + Sync {
    /// Fetch QSOs from the remote logbook, optionally since a date.
    async fn fetch_qsos(&self, since: Option<&str>) -> Result<Vec<QsoRecord>, QrzLogbookError>;

    /// Upload a single QSO and return its QRZ-assigned log ID.
    async fn upload_qso(&self, qso: &QsoRecord) -> Result<QrzUploadResult, QrzLogbookError>;
}

#[tonic::async_trait]
impl QrzLogbookApi for QrzLogbookClient {
    async fn fetch_qsos(&self, since: Option<&str>) -> Result<Vec<QsoRecord>, QrzLogbookError> {
        QrzLogbookClient::fetch_qsos(self, since).await
    }

    async fn upload_qso(&self, qso: &QsoRecord) -> Result<QrzUploadResult, QrzLogbookError> {
        QrzLogbookClient::upload_qso(self, qso).await
    }
}

// ---------------------------------------------------------------------------
// Accumulated sync counters
// ---------------------------------------------------------------------------

struct SyncCounters {
    downloaded: u32,
    uploaded: u32,
    conflicts: u32,
    errors: Vec<String>,
}

impl SyncCounters {
    fn new() -> Self {
        Self {
            downloaded: 0,
            uploaded: 0,
            conflicts: 0,
            errors: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Core sync orchestrator
// ---------------------------------------------------------------------------

/// Execute a bidirectional sync between the local logbook and QRZ.
///
/// Sends streaming progress updates through `progress_tx`. The final message
/// will have `complete = true`.
pub(crate) async fn execute_sync(
    client: &dyn QrzLogbookApi,
    store: &dyn LogbookStore,
    full_sync: bool,
    conflict_policy: ConflictPolicy,
    progress_tx: &mpsc::Sender<Result<SyncWithQrzResponse, Status>>,
) {
    let mut counters = SyncCounters::new();

    let metadata = download_phase(
        client,
        store,
        full_sync,
        conflict_policy,
        progress_tx,
        &mut counters,
    )
    .await;

    upload_phase(client, store, progress_tx, &mut counters).await;

    update_metadata(store, &metadata, &mut counters).await;

    let error_summary = if counters.errors.is_empty() {
        None
    } else {
        Some(counters.errors.join("; "))
    };

    eprintln!(
        "[sync] Sync completed: downloaded={} uploaded={} conflicts={} errors={}",
        counters.downloaded,
        counters.uploaded,
        counters.conflicts,
        counters.errors.len(),
    );

    send_complete(
        progress_tx,
        counters.downloaded,
        counters.uploaded,
        counters.conflicts,
        error_summary,
    )
    .await;
}

// ---------------------------------------------------------------------------
// Phase 1 — Download from QRZ
// ---------------------------------------------------------------------------

async fn download_phase(
    client: &dyn QrzLogbookApi,
    store: &dyn LogbookStore,
    full_sync: bool,
    conflict_policy: ConflictPolicy,
    progress_tx: &mpsc::Sender<Result<SyncWithQrzResponse, Status>>,
    counters: &mut SyncCounters,
) -> SyncMetadata {
    send_progress(progress_tx, "Fetching QSOs from QRZ…", 0, 0, 0).await;

    let metadata = match store.get_sync_metadata().await {
        Ok(m) => m,
        Err(err) => {
            eprintln!("[sync] Failed to read sync metadata: {err}");
            SyncMetadata::default()
        }
    };

    // Load all local QSOs for matching and sync-policy decisions.
    let local_qsos = match store.list_qsos(&QsoListQuery::default()).await {
        Ok(qsos) => qsos,
        Err(err) => {
            send_complete(
                progress_tx,
                0,
                0,
                0,
                Some(format!("Failed to load local QSOs: {err}")),
            )
            .await;
            return metadata;
        }
    };

    // If the local logbook is empty, force a full remote fetch even for
    // incremental sync requests. This recovers from stale metadata and gives
    // first-time users the expected "download everything" behavior.
    let since_date = if full_sync || local_qsos.is_empty() {
        None
    } else {
        metadata
            .last_sync
            .as_ref()
            .and_then(|ts| chrono::DateTime::from_timestamp(ts.seconds, 0))
            .map(|dt| dt.format("%Y-%m-%d").to_string())
    };

    let remote_qsos = match client.fetch_qsos(since_date.as_deref()).await {
        Ok(qsos) => qsos,
        Err(err) => {
            send_complete(
                progress_tx,
                0,
                0,
                0,
                Some(format!("Failed to fetch QSOs from QRZ: {err}")),
            )
            .await;
            return metadata;
        }
    };

    eprintln!(
        "[sync] Fetched {} QSOs from QRZ (full_sync={full_sync})",
        remote_qsos.len()
    );

    send_progress(
        progress_tx,
        &format!("Processing {} downloaded QSOs…", remote_qsos.len()),
        counters.downloaded,
        counters.uploaded,
        counters.conflicts,
    )
    .await;

    // Build lookup indexes.
    let (mut by_qrz_logid, mut by_key) = build_local_indexes(&local_qsos);

    // Process each remote QSO.
    for remote in &remote_qsos {
        let remote_logid = extract_qrz_logid(remote);

        let local_match = remote_logid
            .as_deref()
            .and_then(|logid| by_qrz_logid.get(logid).cloned())
            .or_else(|| fuzzy_match(remote, &by_key));

        match local_match {
            None => {
                insert_new_remote_qso(
                    remote,
                    remote_logid.as_deref(),
                    store,
                    &mut by_qrz_logid,
                    &mut by_key,
                    counters,
                )
                .await;
            }
            Some(local) => {
                merge_with_local(
                    &local,
                    remote_logid.as_deref(),
                    remote,
                    store,
                    conflict_policy,
                    counters,
                )
                .await;
            }
        }
    }

    metadata
}

/// Insert a QSO downloaded from QRZ that has no local match.
async fn insert_new_remote_qso(
    remote: &QsoRecord,
    remote_logid: Option<&str>,
    store: &dyn LogbookStore,
    by_qrz_logid: &mut LogidIndex,
    by_key: &mut FuzzyIndex,
    counters: &mut SyncCounters,
) {
    let mut new_qso = remote.clone();
    if new_qso.local_id.is_empty() {
        new_qso.local_id = new_local_id();
    }
    new_qso.sync_status = SyncStatus::Synced as i32;
    if let Some(logid) = remote_logid {
        new_qso.qrz_logid = Some(logid.to_string());
    }
    match store.insert_qso(&new_qso).await {
        Ok(()) => {
            counters.downloaded += 1;
            // Keep indexes up to date for subsequent matches.
            if let Some(logid) = remote_logid {
                by_qrz_logid.insert(logid.to_string(), new_qso.clone());
            }
            let key = (
                new_qso.worked_callsign.to_ascii_uppercase(),
                new_qso.band,
                new_qso.mode,
            );
            by_key.entry(key).or_default().push(new_qso);
        }
        Err(err) => {
            eprintln!(
                "[sync] Failed to insert downloaded QSO for {}: {err}",
                remote.worked_callsign
            );
            counters.errors.push(format!(
                "Insert failed for {}: {err}",
                remote.worked_callsign
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 2 — Upload pending local QSOs to QRZ
// ---------------------------------------------------------------------------

async fn upload_phase(
    client: &dyn QrzLogbookApi,
    store: &dyn LogbookStore,
    progress_tx: &mpsc::Sender<Result<SyncWithQrzResponse, Status>>,
    counters: &mut SyncCounters,
) {
    send_progress(
        progress_tx,
        "Uploading local QSOs to QRZ…",
        counters.downloaded,
        counters.uploaded,
        counters.conflicts,
    )
    .await;

    let pending_qsos: Vec<QsoRecord> = match store.list_qsos(&QsoListQuery::default()).await {
        Ok(qsos) => qsos
            .into_iter()
            .filter(|q| {
                q.sync_status == SyncStatus::LocalOnly as i32
                    || q.sync_status == SyncStatus::Modified as i32
            })
            .collect(),
        Err(err) => {
            eprintln!("[sync] Failed to list pending QSOs for upload: {err}");
            counters
                .errors
                .push(format!("Failed to list pending QSOs: {err}"));
            Vec::new()
        }
    };

    eprintln!(
        "[sync] Uploading {} pending QSOs to QRZ",
        pending_qsos.len()
    );

    for qso in &pending_qsos {
        match client.upload_qso(qso).await {
            Ok(result) => {
                let mut synced = qso.clone();
                synced.qrz_logid = Some(result.logid);
                synced.sync_status = SyncStatus::Synced as i32;
                match store.update_qso(&synced).await {
                    Ok(_) => counters.uploaded += 1,
                    Err(err) => {
                        eprintln!(
                            "[sync] Uploaded but failed to update local record {}: {err}",
                            qso.local_id
                        );
                        counters.errors.push(format!(
                            "Upload succeeded for {} but local update failed: {err}",
                            qso.worked_callsign,
                        ));
                    }
                }
            }
            Err(err) => {
                eprintln!(
                    "[sync] Failed to upload QSO {} ({}): {err}",
                    qso.local_id, qso.worked_callsign
                );
                counters
                    .errors
                    .push(format!("Upload failed for {}: {err}", qso.worked_callsign));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 3 — Persist updated metadata
// ---------------------------------------------------------------------------

async fn update_metadata(
    store: &dyn LogbookStore,
    prev_metadata: &SyncMetadata,
    counters: &mut SyncCounters,
) {
    let now = chrono::Utc::now();
    let updated = SyncMetadata {
        qrz_qso_count: prev_metadata.qrz_qso_count,
        last_sync: Some(prost_types::Timestamp {
            seconds: now.timestamp(),
            nanos: 0,
        }),
        qrz_logbook_owner: prev_metadata.qrz_logbook_owner.clone(),
    };

    if let Err(err) = store.upsert_sync_metadata(&updated).await {
        eprintln!("[sync] Failed to update sync metadata: {err}");
        counters
            .errors
            .push(format!("Failed to update sync metadata: {err}"));
    }
}

// ---------------------------------------------------------------------------
// Download merge helpers
// ---------------------------------------------------------------------------

/// Merge a remote QSO with a matched local QSO based on sync status.
async fn merge_with_local(
    local: &QsoRecord,
    remote_logid: Option<&str>,
    remote: &QsoRecord,
    store: &dyn LogbookStore,
    conflict_policy: ConflictPolicy,
    counters: &mut SyncCounters,
) {
    match SyncStatus::try_from(local.sync_status) {
        Ok(SyncStatus::Synced) => {
            // Remote wins — update local with remote data.
            let mut updated = remote.clone();
            updated.local_id.clone_from(&local.local_id);
            updated.sync_status = SyncStatus::Synced as i32;
            updated.qrz_logid = remote_logid
                .map(String::from)
                .or_else(|| local.qrz_logid.clone());

            match store.update_qso(&updated).await {
                Ok(_) => counters.downloaded += 1,
                Err(err) => {
                    eprintln!(
                        "[sync] Failed to update QSO {} from remote: {err}",
                        local.local_id
                    );
                    counters.errors.push(format!(
                        "Update failed for {}: {err}",
                        local.worked_callsign
                    ));
                }
            }
        }
        Ok(SyncStatus::LocalOnly) => {
            // The QSO was already on QRZ (e.g. uploaded outside QsoRipper).
            // Link it and mark synced to avoid a duplicate upload in Phase 2.
            let mut linked = local.clone();
            linked.sync_status = SyncStatus::Synced as i32;
            if let Some(logid) = remote_logid {
                linked.qrz_logid = Some(logid.to_string());
            }
            match store.update_qso(&linked).await {
                Ok(_) => counters.downloaded += 1,
                Err(err) => {
                    eprintln!(
                        "[sync] Failed to link local-only QSO {} to remote: {err}",
                        local.local_id
                    );
                    counters
                        .errors
                        .push(format!("Link failed for {}: {err}", local.worked_callsign));
                }
            }
        }
        Ok(SyncStatus::Modified) => {
            resolve_modified_conflict(local, remote, store, conflict_policy, counters).await;
        }
        _ => {
            // Conflict or unknown — leave untouched.
        }
    }
}

/// Resolve a modified-vs-remote conflict according to the configured policy.
///
/// - **`LastWriteWins`**: compare `updated_at` timestamps. If remote is newer
///   (or timestamps tie), overwrite local and set `SYNCED`. If local is newer,
///   leave it as `MODIFIED` so the upload phase pushes it.
/// - **`FlagForReview`**: set local to `CONFLICT` without overwriting.
async fn resolve_modified_conflict(
    local: &QsoRecord,
    remote: &QsoRecord,
    store: &dyn LogbookStore,
    policy: ConflictPolicy,
    counters: &mut SyncCounters,
) {
    match policy {
        ConflictPolicy::LastWriteWins => {
            let local_ts = local.updated_at.as_ref().map_or(0, |ts| ts.seconds);
            let remote_ts = remote.updated_at.as_ref().map_or(0, |ts| ts.seconds);

            if remote_ts >= local_ts {
                // Remote is newer (or equal) — overwrite local.
                let mut updated = remote.clone();
                updated.local_id.clone_from(&local.local_id);
                updated.sync_status = SyncStatus::Synced as i32;
                updated.qrz_logid.clone_from(&local.qrz_logid);
                match store.update_qso(&updated).await {
                    Ok(_) => counters.downloaded += 1,
                    Err(err) => {
                        eprintln!(
                            "[sync] Failed to overwrite QSO {} with newer remote: {err}",
                            local.local_id
                        );
                        counters.errors.push(format!(
                            "Overwrite failed for {}: {err}",
                            local.worked_callsign,
                        ));
                    }
                }
            } else {
                // Local is newer — keep MODIFIED; upload phase will push it.
                eprintln!(
                    "[sync] Local QSO {} is newer than remote; keeping MODIFIED for upload",
                    local.local_id
                );
            }
        }
        ConflictPolicy::FlagForReview => {
            let mut conflicted = local.clone();
            conflicted.sync_status = SyncStatus::Conflict as i32;
            match store.update_qso(&conflicted).await {
                Ok(_) => counters.conflicts += 1,
                Err(err) => {
                    eprintln!(
                        "[sync] Failed to mark QSO {} as conflict: {err}",
                        local.local_id
                    );
                    counters.errors.push(format!(
                        "Conflict mark failed for {}: {err}",
                        local.worked_callsign
                    ));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Matching helpers
// ---------------------------------------------------------------------------

fn build_local_indexes(local_qsos: &[QsoRecord]) -> (LogidIndex, FuzzyIndex) {
    let mut by_qrz_logid: LogidIndex = HashMap::new();
    let mut by_key: FuzzyIndex = HashMap::new();

    for qso in local_qsos {
        if let Some(logid) = qso.qrz_logid.as_deref() {
            if !logid.is_empty() {
                by_qrz_logid.insert(logid.to_string(), qso.clone());
            }
        }
        let key = (qso.worked_callsign.to_ascii_uppercase(), qso.band, qso.mode);
        by_key.entry(key).or_default().push(qso.clone());
    }

    (by_qrz_logid, by_key)
}

/// Extract the QRZ logbook record ID from a QSO.
///
/// Checks the dedicated `qrz_logid` field first, then falls back to
/// `extra_fields["APP_QRZ_LOGID"]`.
fn extract_qrz_logid(qso: &QsoRecord) -> Option<String> {
    if let Some(logid) = qso.qrz_logid.as_deref() {
        if !logid.is_empty() {
            return Some(logid.to_string());
        }
    }
    qso.extra_fields
        .get(QRZ_LOGID_EXTRA_FIELD)
        .filter(|v| !v.is_empty())
        .cloned()
}

/// Find a local QSO matching by worked callsign, band, mode, and timestamp
/// (within [`TIMESTAMP_TOLERANCE_SECONDS`]).
fn fuzzy_match(remote: &QsoRecord, by_key: &FuzzyIndex) -> Option<QsoRecord> {
    let key = (
        remote.worked_callsign.to_ascii_uppercase(),
        remote.band,
        remote.mode,
    );
    let candidates = by_key.get(&key)?;
    let remote_ts = remote.utc_timestamp.as_ref()?.seconds;

    candidates
        .iter()
        .find(|local| {
            local
                .utc_timestamp
                .as_ref()
                .is_some_and(|ts| (ts.seconds - remote_ts).abs() <= TIMESTAMP_TOLERANCE_SECONDS)
        })
        .cloned()
}

// ---------------------------------------------------------------------------
// Progress reporting helpers
// ---------------------------------------------------------------------------

async fn send_progress(
    tx: &mpsc::Sender<Result<SyncWithQrzResponse, Status>>,
    action: &str,
    downloaded: u32,
    uploaded: u32,
    conflicts: u32,
) {
    drop(
        tx.send(Ok(SyncWithQrzResponse {
            total_records: downloaded + uploaded,
            processed_records: downloaded + uploaded,
            uploaded_records: uploaded,
            downloaded_records: downloaded,
            conflict_records: conflicts,
            current_action: Some(action.to_string()),
            complete: false,
            error: None,
        }))
        .await,
    );
}

async fn send_complete(
    tx: &mpsc::Sender<Result<SyncWithQrzResponse, Status>>,
    downloaded: u32,
    uploaded: u32,
    conflicts: u32,
    error: Option<String>,
) {
    drop(
        tx.send(Ok(SyncWithQrzResponse {
            total_records: downloaded + uploaded,
            processed_records: downloaded + uploaded,
            uploaded_records: uploaded,
            downloaded_records: downloaded,
            conflict_records: conflicts,
            current_action: Some("Sync complete".to_string()),
            complete: true,
            error,
        }))
        .await,
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::indexing_slicing
)]
mod tests {
    use std::sync::{Arc, Mutex};

    use prost_types::Timestamp;
    use qsoripper_core::domain::qso::QsoRecordBuilder;
    use qsoripper_core::proto::qsoripper::domain::{
        Band, ConflictPolicy, Mode, QsoRecord, SyncStatus,
    };
    use qsoripper_core::qrz_logbook::{QrzLogbookError, QrzUploadResult};
    use qsoripper_core::storage::{LogbookStore, QsoListQuery, SyncMetadata};
    use qsoripper_storage_memory::MemoryStorage;
    use tokio::sync::mpsc;

    use super::{execute_sync, QrzLogbookApi};

    // -- Mock API -----------------------------------------------------------

    struct MockQrzApi {
        fetch_result: Mutex<Option<Result<Vec<QsoRecord>, QrzLogbookError>>>,
        upload_results: Mutex<Vec<Result<QrzUploadResult, QrzLogbookError>>>,
    }

    impl MockQrzApi {
        fn new(
            fetch: Result<Vec<QsoRecord>, QrzLogbookError>,
            uploads: Vec<Result<QrzUploadResult, QrzLogbookError>>,
        ) -> Self {
            Self {
                fetch_result: Mutex::new(Some(fetch)),
                upload_results: Mutex::new(uploads),
            }
        }
    }

    #[tonic::async_trait]
    impl QrzLogbookApi for MockQrzApi {
        async fn fetch_qsos(
            &self,
            _since: Option<&str>,
        ) -> Result<Vec<QsoRecord>, QrzLogbookError> {
            self.fetch_result
                .lock()
                .unwrap()
                .take()
                .unwrap_or_else(|| Ok(Vec::new()))
        }

        async fn upload_qso(&self, _qso: &QsoRecord) -> Result<QrzUploadResult, QrzLogbookError> {
            let mut results = self.upload_results.lock().unwrap();
            if results.is_empty() {
                Err(QrzLogbookError::ApiError(
                    "no more mock upload results".into(),
                ))
            } else {
                results.remove(0)
            }
        }
    }

    /// Capturing mock that records what `since` value was passed to `fetch_qsos`.
    struct CapturingApi {
        since: Arc<Mutex<Option<String>>>,
    }

    #[tonic::async_trait]
    impl QrzLogbookApi for CapturingApi {
        async fn fetch_qsos(&self, since: Option<&str>) -> Result<Vec<QsoRecord>, QrzLogbookError> {
            *self.since.lock().unwrap() = since.map(String::from);
            Ok(vec![])
        }

        async fn upload_qso(&self, _qso: &QsoRecord) -> Result<QrzUploadResult, QrzLogbookError> {
            Ok(QrzUploadResult {
                logid: "ignored".into(),
            })
        }
    }

    // -- Helpers ------------------------------------------------------------

    fn make_qso(station: &str, worked: &str, band: Band, mode: Mode, ts_seconds: i64) -> QsoRecord {
        QsoRecordBuilder::new(station, worked)
            .band(band)
            .mode(mode)
            .timestamp(Timestamp {
                seconds: ts_seconds,
                nanos: 0,
            })
            .build()
    }

    /// Collect the final streamed response.
    async fn collect_final(
        mut rx: mpsc::Receiver<Result<super::SyncWithQrzResponse, tonic::Status>>,
    ) -> super::SyncWithQrzResponse {
        let mut last = None;
        while let Some(Ok(msg)) = rx.recv().await {
            last = Some(msg);
        }
        last.expect("expected at least one streamed response")
    }

    // -- Test cases ---------------------------------------------------------

    #[tokio::test]
    async fn download_inserts_new_qsos_from_remote() {
        let store = MemoryStorage::new();

        let remote1 = {
            let mut q = make_qso("W1AW", "K7ABC", Band::Band20m, Mode::Ft8, 1_700_000_000);
            q.qrz_logid = Some("QRZ001".into());
            q
        };
        let remote2 = {
            let mut q = make_qso("W1AW", "JA1ZZZ", Band::Band40m, Mode::Cw, 1_700_000_100);
            q.qrz_logid = Some("QRZ002".into());
            q
        };
        let api = MockQrzApi::new(Ok(vec![remote1, remote2]), vec![]);

        let (tx, rx) = mpsc::channel(16);
        execute_sync(&api, &store, true, ConflictPolicy::LastWriteWins, &tx).await;
        drop(tx);

        let final_msg = collect_final(rx).await;
        assert!(final_msg.complete);
        assert_eq!(final_msg.downloaded_records, 2);
        assert_eq!(final_msg.uploaded_records, 0);
        assert!(final_msg.error.is_none());

        let all = store.list_qsos(&QsoListQuery::default()).await.unwrap();
        assert_eq!(all.len(), 2);
        for qso in &all {
            assert_eq!(qso.sync_status, SyncStatus::Synced as i32);
            assert!(qso.qrz_logid.is_some());
        }
    }

    #[tokio::test]
    async fn upload_sends_local_only_qsos() {
        let store = MemoryStorage::new();

        let local1 = make_qso("W1AW", "K7ABC", Band::Band20m, Mode::Ft8, 1_700_000_000);
        let local2 = make_qso("W1AW", "DL1ABC", Band::Band40m, Mode::Ssb, 1_700_000_100);
        store.insert_qso(&local1).await.unwrap();
        store.insert_qso(&local2).await.unwrap();

        let api = MockQrzApi::new(
            Ok(vec![]),
            vec![
                Ok(QrzUploadResult {
                    logid: "QRZ100".into(),
                }),
                Ok(QrzUploadResult {
                    logid: "QRZ101".into(),
                }),
            ],
        );

        let (tx, rx) = mpsc::channel(16);
        execute_sync(&api, &store, true, ConflictPolicy::LastWriteWins, &tx).await;
        drop(tx);

        let final_msg = collect_final(rx).await;
        assert!(final_msg.complete);
        assert_eq!(final_msg.uploaded_records, 2);
        assert_eq!(final_msg.downloaded_records, 0);
        assert!(final_msg.error.is_none());

        let all = store.list_qsos(&QsoListQuery::default()).await.unwrap();
        for qso in &all {
            assert_eq!(qso.sync_status, SyncStatus::Synced as i32);
            assert!(qso.qrz_logid.is_some());
        }
    }

    #[tokio::test]
    async fn mixed_sync_downloads_and_uploads() {
        let store = MemoryStorage::new();

        // One local QSO pending upload.
        let local = make_qso("W1AW", "K7LOCAL", Band::Band10m, Mode::Ft8, 1_700_000_500);
        store.insert_qso(&local).await.unwrap();

        // One remote QSO not yet in local store.
        let remote = {
            let mut q = make_qso("W1AW", "VK3REMOTE", Band::Band20m, Mode::Cw, 1_700_000_600);
            q.qrz_logid = Some("QRZ200".into());
            q
        };

        let api = MockQrzApi::new(
            Ok(vec![remote]),
            vec![Ok(QrzUploadResult {
                logid: "QRZ201".into(),
            })],
        );

        let (tx, rx) = mpsc::channel(16);
        execute_sync(&api, &store, true, ConflictPolicy::LastWriteWins, &tx).await;
        drop(tx);

        let final_msg = collect_final(rx).await;
        assert!(final_msg.complete);
        assert_eq!(final_msg.downloaded_records, 1);
        assert_eq!(final_msg.uploaded_records, 1);
        assert!(final_msg.error.is_none());

        let all = store.list_qsos(&QsoListQuery::default()).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn upload_errors_do_not_abort_sync() {
        let store = MemoryStorage::new();

        let local1 = make_qso("W1AW", "K7FAIL", Band::Band20m, Mode::Ft8, 1_700_000_000);
        let local2 = make_qso("W1AW", "K7PASS", Band::Band40m, Mode::Ssb, 1_700_000_100);
        store.insert_qso(&local1).await.unwrap();
        store.insert_qso(&local2).await.unwrap();

        let api = MockQrzApi::new(
            Ok(vec![]),
            vec![
                Err(QrzLogbookError::ApiError("rejected".into())),
                Ok(QrzUploadResult {
                    logid: "QRZ300".into(),
                }),
            ],
        );

        let (tx, rx) = mpsc::channel(16);
        execute_sync(&api, &store, true, ConflictPolicy::LastWriteWins, &tx).await;
        drop(tx);

        let final_msg = collect_final(rx).await;
        assert!(final_msg.complete);
        // One upload succeeded, one failed.
        assert_eq!(final_msg.uploaded_records, 1);
        assert!(final_msg.error.is_some());
    }

    #[tokio::test]
    async fn metadata_persisted_after_sync() {
        let store = MemoryStorage::new();

        let api = MockQrzApi::new(Ok(vec![]), vec![]);

        let (tx, rx) = mpsc::channel(16);
        execute_sync(&api, &store, true, ConflictPolicy::LastWriteWins, &tx).await;
        drop(tx);

        drop(collect_final(rx).await);

        let metadata = store.get_sync_metadata().await.unwrap();
        assert!(
            metadata.last_sync.is_some(),
            "last_sync should be set after sync"
        );
    }

    #[tokio::test]
    async fn fuzzy_match_links_by_callsign_time_band_mode() {
        let store = MemoryStorage::new();

        // Insert a local QSO.
        let local = make_qso("W1AW", "K7ABC", Band::Band20m, Mode::Ft8, 1_700_000_000);
        store.insert_qso(&local).await.unwrap();

        // Remote QSO: same callsign+band+mode, timestamp differs by 30 seconds.
        // No qrz_logid on local, so the match must be fuzzy.
        let remote = {
            let mut q = make_qso("W1AW", "K7ABC", Band::Band20m, Mode::Ft8, 1_700_000_030);
            q.qrz_logid = Some("QRZ400".into());
            // Give it a fresh local_id to make it clearly different from the local one.
            q.local_id = "remote-temp-id".into();
            q
        };

        let api = MockQrzApi::new(Ok(vec![remote]), vec![]);

        let (tx, rx) = mpsc::channel(16);
        execute_sync(&api, &store, true, ConflictPolicy::LastWriteWins, &tx).await;
        drop(tx);

        let final_msg = collect_final(rx).await;
        assert!(final_msg.complete);
        // The remote matched the local, so it counts as a download update,
        // not a new insert. No new QSOs created.
        assert_eq!(final_msg.downloaded_records, 1);

        let all = store.list_qsos(&QsoListQuery::default()).await.unwrap();
        assert_eq!(all.len(), 1, "fuzzy match should not insert a duplicate");
        assert_eq!(all[0].sync_status, SyncStatus::Synced as i32);
    }

    #[tokio::test]
    async fn modified_local_marked_as_conflict_on_remote_match() {
        let store = MemoryStorage::new();

        // Local QSO with MODIFIED status.
        let mut local = make_qso("W1AW", "K7MOD", Band::Band20m, Mode::Ft8, 1_700_000_000);
        local.sync_status = SyncStatus::Modified as i32;
        local.qrz_logid = Some("QRZ500".into());
        store.insert_qso(&local).await.unwrap();

        // Remote QSO with same logid.
        let remote = {
            let mut q = make_qso("W1AW", "K7MOD", Band::Band20m, Mode::Ft8, 1_700_000_000);
            q.qrz_logid = Some("QRZ500".into());
            q
        };

        let api = MockQrzApi::new(Ok(vec![remote]), vec![]);

        let (tx, rx) = mpsc::channel(16);
        execute_sync(&api, &store, true, ConflictPolicy::FlagForReview, &tx).await;
        drop(tx);

        let final_msg = collect_final(rx).await;
        assert!(final_msg.complete);
        assert_eq!(final_msg.conflict_records, 1);
        assert_eq!(final_msg.uploaded_records, 0);

        let all = store.list_qsos(&QsoListQuery::default()).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].sync_status, SyncStatus::Conflict as i32);
    }

    #[tokio::test]
    async fn local_only_linked_to_remote_not_reuploaded() {
        let store = MemoryStorage::new();

        // Local QSO with LOCAL_ONLY status that happens to match a remote QSO.
        let local = make_qso("W1AW", "K7LINK", Band::Band20m, Mode::Ft8, 1_700_000_000);
        assert_eq!(local.sync_status, SyncStatus::LocalOnly as i32);
        store.insert_qso(&local).await.unwrap();

        // Remote QSO matches by callsign+band+mode+timestamp.
        let remote = {
            let mut q = make_qso("W1AW", "K7LINK", Band::Band20m, Mode::Ft8, 1_700_000_010);
            q.qrz_logid = Some("QRZ600".into());
            q.local_id = "remote-temp".into();
            q
        };

        // No upload results needed — the QSO should be linked, not uploaded.
        let api = MockQrzApi::new(Ok(vec![remote]), vec![]);

        let (tx, rx) = mpsc::channel(16);
        execute_sync(&api, &store, true, ConflictPolicy::LastWriteWins, &tx).await;
        drop(tx);

        let final_msg = collect_final(rx).await;
        assert!(final_msg.complete);
        assert_eq!(final_msg.downloaded_records, 1);
        assert_eq!(
            final_msg.uploaded_records, 0,
            "linked QSO should not be re-uploaded"
        );
        assert!(final_msg.error.is_none());

        let all = store.list_qsos(&QsoListQuery::default()).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].sync_status, SyncStatus::Synced as i32);
        assert_eq!(all[0].qrz_logid.as_deref(), Some("QRZ600"));
    }

    #[tokio::test]
    async fn incremental_sync_uses_last_sync_date() {
        let store = MemoryStorage::new();

        // Add at least one local record so incremental sync uses last_sync.
        let local = make_qso("W1AW", "K7LOCAL", Band::Band20m, Mode::Cw, 1_700_000_050);
        store.insert_qso(&local).await.unwrap();

        // Seed metadata with a previous sync timestamp.
        let metadata = SyncMetadata {
            last_sync: Some(Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            }),
            ..SyncMetadata::default()
        };
        store.upsert_sync_metadata(&metadata).await.unwrap();

        let since_capture: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let api = CapturingApi {
            since: since_capture.clone(),
        };

        let (tx, rx) = mpsc::channel(16);
        // full_sync = false → should use last_sync date.
        execute_sync(&api, &store, false, ConflictPolicy::LastWriteWins, &tx).await;
        drop(tx);

        drop(collect_final(rx).await);

        let captured = since_capture.lock().unwrap().clone();
        assert_eq!(captured.as_deref(), Some("2023-11-14"));
    }

    #[tokio::test]
    async fn incremental_sync_with_empty_local_log_uses_full_fetch() {
        let store = MemoryStorage::new();

        // Metadata exists, but there are no local QSOs yet.
        let metadata = SyncMetadata {
            last_sync: Some(Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            }),
            ..SyncMetadata::default()
        };
        store.upsert_sync_metadata(&metadata).await.unwrap();

        let since_capture: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let api = CapturingApi {
            since: since_capture.clone(),
        };

        let (tx, rx) = mpsc::channel(16);
        execute_sync(&api, &store, false, ConflictPolicy::LastWriteWins, &tx).await;
        drop(tx);

        drop(collect_final(rx).await);

        let captured = since_capture.lock().unwrap().clone();
        assert!(
            captured.is_none(),
            "empty local log should force a full remote fetch"
        );
    }

    #[tokio::test]
    async fn full_sync_ignores_last_sync_date() {
        let store = MemoryStorage::new();

        let metadata = SyncMetadata {
            last_sync: Some(Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            }),
            ..SyncMetadata::default()
        };
        store.upsert_sync_metadata(&metadata).await.unwrap();

        let since_capture: Arc<Mutex<Option<String>>> =
            Arc::new(Mutex::new(Some("should-be-cleared".into())));
        let api = CapturingApi {
            since: since_capture.clone(),
        };

        let (tx, rx) = mpsc::channel(16);
        // full_sync = true → should NOT pass a since date.
        execute_sync(&api, &store, true, ConflictPolicy::LastWriteWins, &tx).await;
        drop(tx);

        drop(collect_final(rx).await);

        let captured = since_capture.lock().unwrap().clone();
        assert!(
            captured.is_none(),
            "full_sync should not pass a since parameter"
        );
    }

    #[tokio::test]
    async fn extract_logid_from_extra_fields() {
        let qso = QsoRecord {
            extra_fields: [(super::QRZ_LOGID_EXTRA_FIELD.into(), "EX123".into())]
                .into_iter()
                .collect(),
            ..QsoRecord::default()
        };

        let logid = super::extract_qrz_logid(&qso);
        assert_eq!(logid.as_deref(), Some("EX123"));
    }

    #[tokio::test]
    async fn extract_logid_prefers_dedicated_field() {
        let qso = QsoRecord {
            qrz_logid: Some("DIRECT".into()),
            extra_fields: [(super::QRZ_LOGID_EXTRA_FIELD.into(), "EXTRA".into())]
                .into_iter()
                .collect(),
            ..QsoRecord::default()
        };

        let logid = super::extract_qrz_logid(&qso);
        assert_eq!(logid.as_deref(), Some("DIRECT"));
    }

    // -- Conflict-policy tests -----------------------------------------------

    #[tokio::test]
    async fn last_write_wins_remote_newer_overwrites_local() {
        let store = MemoryStorage::new();

        // Local QSO: MODIFIED, updated_at = 1000.
        let mut local = make_qso("W1AW", "K7LWW", Band::Band20m, Mode::Ft8, 1_700_000_000);
        local.sync_status = SyncStatus::Modified as i32;
        local.qrz_logid = Some("QRZ700".into());
        local.updated_at = Some(Timestamp {
            seconds: 1000,
            nanos: 0,
        });
        local.notes = Some("local edit".into());
        store.insert_qso(&local).await.unwrap();

        // Remote QSO: same logid, updated_at = 2000 (newer).
        let remote = {
            let mut q = make_qso("W1AW", "K7LWW", Band::Band20m, Mode::Ft8, 1_700_000_000);
            q.qrz_logid = Some("QRZ700".into());
            q.updated_at = Some(Timestamp {
                seconds: 2000,
                nanos: 0,
            });
            q.notes = Some("remote edit".into());
            q
        };

        let api = MockQrzApi::new(Ok(vec![remote]), vec![]);

        let (tx, rx) = mpsc::channel(16);
        execute_sync(&api, &store, true, ConflictPolicy::LastWriteWins, &tx).await;
        drop(tx);

        let final_msg = collect_final(rx).await;
        assert!(final_msg.complete);
        assert_eq!(final_msg.downloaded_records, 1, "remote should overwrite");
        assert_eq!(final_msg.conflict_records, 0);

        let all = store.list_qsos(&QsoListQuery::default()).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].sync_status, SyncStatus::Synced as i32);
        assert_eq!(all[0].notes.as_deref(), Some("remote edit"));
    }

    #[tokio::test]
    async fn last_write_wins_local_newer_keeps_modified() {
        let store = MemoryStorage::new();

        // Local QSO: MODIFIED, updated_at = 3000 (newer).
        let mut local = make_qso("W1AW", "K7LWW2", Band::Band20m, Mode::Ft8, 1_700_000_000);
        local.sync_status = SyncStatus::Modified as i32;
        local.qrz_logid = Some("QRZ701".into());
        local.updated_at = Some(Timestamp {
            seconds: 3000,
            nanos: 0,
        });
        local.notes = Some("local newer".into());
        store.insert_qso(&local).await.unwrap();

        // Remote QSO: updated_at = 1000 (older).
        let remote = {
            let mut q = make_qso("W1AW", "K7LWW2", Band::Band20m, Mode::Ft8, 1_700_000_000);
            q.qrz_logid = Some("QRZ701".into());
            q.updated_at = Some(Timestamp {
                seconds: 1000,
                nanos: 0,
            });
            q.notes = Some("remote older".into());
            q
        };

        // Provide an upload result because the upload phase will push the local QSO.
        let api = MockQrzApi::new(
            Ok(vec![remote]),
            vec![Ok(QrzUploadResult {
                logid: "QRZ701".into(),
            })],
        );

        let (tx, rx) = mpsc::channel(16);
        execute_sync(&api, &store, true, ConflictPolicy::LastWriteWins, &tx).await;
        drop(tx);

        let final_msg = collect_final(rx).await;
        assert!(final_msg.complete);
        assert_eq!(
            final_msg.downloaded_records, 0,
            "local is newer, no download"
        );
        assert_eq!(final_msg.conflict_records, 0);
        // Upload phase should push the locally modified QSO.
        assert_eq!(final_msg.uploaded_records, 1);

        let all = store.list_qsos(&QsoListQuery::default()).await.unwrap();
        assert_eq!(all.len(), 1);
        // After upload, status is SYNCED.
        assert_eq!(all[0].sync_status, SyncStatus::Synced as i32);
    }

    #[tokio::test]
    async fn flag_for_review_does_not_overwrite_and_skips_upload() {
        let store = MemoryStorage::new();

        // Local QSO: MODIFIED.
        let mut local = make_qso("W1AW", "K7FFR", Band::Band20m, Mode::Ft8, 1_700_000_000);
        local.sync_status = SyncStatus::Modified as i32;
        local.qrz_logid = Some("QRZ800".into());
        local.updated_at = Some(Timestamp {
            seconds: 1000,
            nanos: 0,
        });
        local.notes = Some("local version".into());
        store.insert_qso(&local).await.unwrap();

        // Remote QSO: newer updated_at but with FLAG_FOR_REVIEW we don't compare timestamps.
        let remote = {
            let mut q = make_qso("W1AW", "K7FFR", Band::Band20m, Mode::Ft8, 1_700_000_000);
            q.qrz_logid = Some("QRZ800".into());
            q.updated_at = Some(Timestamp {
                seconds: 5000,
                nanos: 0,
            });
            q.notes = Some("remote version".into());
            q
        };

        // No upload results — CONFLICT QSOs should not be uploaded.
        let api = MockQrzApi::new(Ok(vec![remote]), vec![]);

        let (tx, rx) = mpsc::channel(16);
        execute_sync(&api, &store, true, ConflictPolicy::FlagForReview, &tx).await;
        drop(tx);

        let final_msg = collect_final(rx).await;
        assert!(final_msg.complete);
        assert_eq!(final_msg.conflict_records, 1);
        assert_eq!(final_msg.downloaded_records, 0);
        assert_eq!(final_msg.uploaded_records, 0);

        let all = store.list_qsos(&QsoListQuery::default()).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].sync_status, SyncStatus::Conflict as i32);
        // Local data preserved, not overwritten by remote.
        assert_eq!(all[0].notes.as_deref(), Some("local version"));
    }

    #[tokio::test]
    async fn last_write_wins_equal_timestamps_remote_wins() {
        let store = MemoryStorage::new();

        // Local QSO: MODIFIED, updated_at = 2000.
        let mut local = make_qso("W1AW", "K7EQ", Band::Band20m, Mode::Ft8, 1_700_000_000);
        local.sync_status = SyncStatus::Modified as i32;
        local.qrz_logid = Some("QRZ900".into());
        local.updated_at = Some(Timestamp {
            seconds: 2000,
            nanos: 0,
        });
        local.notes = Some("local tie".into());
        store.insert_qso(&local).await.unwrap();

        // Remote QSO: same updated_at → tie, remote wins.
        let remote = {
            let mut q = make_qso("W1AW", "K7EQ", Band::Band20m, Mode::Ft8, 1_700_000_000);
            q.qrz_logid = Some("QRZ900".into());
            q.updated_at = Some(Timestamp {
                seconds: 2000,
                nanos: 0,
            });
            q.notes = Some("remote tie".into());
            q
        };

        let api = MockQrzApi::new(Ok(vec![remote]), vec![]);

        let (tx, rx) = mpsc::channel(16);
        execute_sync(&api, &store, true, ConflictPolicy::LastWriteWins, &tx).await;
        drop(tx);

        let final_msg = collect_final(rx).await;
        assert!(final_msg.complete);
        assert_eq!(final_msg.downloaded_records, 1, "tie goes to remote");
        assert_eq!(final_msg.conflict_records, 0);

        let all = store.list_qsos(&QsoListQuery::default()).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].sync_status, SyncStatus::Synced as i32);
        assert_eq!(all[0].notes.as_deref(), Some("remote tie"));
    }
}

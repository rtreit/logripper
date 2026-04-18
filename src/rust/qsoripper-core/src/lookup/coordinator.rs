//! Lookup orchestration with cache policy and in-flight request deduplication.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use futures::future::{BoxFuture, FutureExt, Shared};
use tokio::sync::{Mutex, RwLock};

use crate::{
    domain::{
        callsign_parser::{annotate_record, parse_callsign},
        lookup::normalize_callsign,
    },
    proto::qsoripper::domain::{CallsignRecord, LookupResult, LookupState},
    storage::{EngineStorage, LookupSnapshot},
};

use super::provider::{
    CallsignProvider, ProviderLookup, ProviderLookupError, ProviderLookupOutcome,
};

type ProviderLookupResult = Result<ProviderLookup, ProviderLookupError>;
type SharedProviderLookup = Shared<BoxFuture<'static, ProviderLookupResult>>;

const DEFAULT_POSITIVE_CACHE_TTL: Duration = Duration::from_secs(15 * 60);
const DEFAULT_NEGATIVE_CACHE_TTL: Duration = Duration::from_secs(2 * 60);

/// Lookup coordinator configuration.
#[derive(Debug, Clone, Copy)]
pub struct LookupCoordinatorConfig {
    positive_ttl: Duration,
    negative_ttl: Duration,
}

impl LookupCoordinatorConfig {
    /// Create an explicit cache configuration.
    #[must_use]
    pub fn new(positive_ttl: Duration, negative_ttl: Duration) -> Self {
        Self {
            positive_ttl,
            negative_ttl,
        }
    }

    /// Positive (found-record) cache TTL.
    #[must_use]
    pub fn positive_ttl(self) -> Duration {
        self.positive_ttl
    }

    /// Negative (not-found) cache TTL.
    #[must_use]
    pub fn negative_ttl(self) -> Duration {
        self.negative_ttl
    }
}

impl Default for LookupCoordinatorConfig {
    fn default() -> Self {
        Self {
            positive_ttl: DEFAULT_POSITIVE_CACHE_TTL,
            negative_ttl: DEFAULT_NEGATIVE_CACHE_TTL,
        }
    }
}

#[derive(Debug, Clone)]
enum CachedLookup {
    Found(Box<CallsignRecord>),
    NotFound,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    lookup: CachedLookup,
    cached_at: Instant,
}

/// Coordinates lookup policy over an underlying callsign provider.
pub struct LookupCoordinator {
    provider: Arc<dyn CallsignProvider>,
    config: LookupCoordinatorConfig,
    cache: RwLock<HashMap<String, CacheEntry>>,
    in_flight: Mutex<HashMap<String, SharedProviderLookup>>,
    snapshot_storage: Option<Arc<dyn EngineStorage>>,
}

impl LookupCoordinator {
    /// Create a lookup coordinator around a provider and cache policy.
    #[must_use]
    pub fn new(provider: Arc<dyn CallsignProvider>, config: LookupCoordinatorConfig) -> Self {
        Self {
            provider,
            config,
            cache: RwLock::new(HashMap::new()),
            in_flight: Mutex::new(HashMap::new()),
            snapshot_storage: None,
        }
    }

    /// Create a lookup coordinator backed by a persistent snapshot store.
    ///
    /// Lookup results are persisted to the store and loaded when the
    /// in-memory cache does not contain a fresh entry.
    #[must_use]
    pub fn with_snapshot_store(
        provider: Arc<dyn CallsignProvider>,
        config: LookupCoordinatorConfig,
        storage: Arc<dyn EngineStorage>,
    ) -> Self {
        Self {
            provider,
            config,
            cache: RwLock::new(HashMap::new()),
            in_flight: Mutex::new(HashMap::new()),
            snapshot_storage: Some(storage),
        }
    }

    /// Perform a unary lookup with cache and provider orchestration.
    pub async fn lookup(&self, callsign: &str, skip_cache: bool) -> LookupResult {
        let normalized_callsign = normalize_callsign(callsign);
        if !skip_cache {
            if let Some(entry) = self.get_cache_entry(&normalized_callsign).await {
                if self.is_fresh(&entry) {
                    return Self::cache_entry_to_result(&entry, &normalized_callsign, None, true);
                }
            }
        }

        let parsed = parse_callsign(&normalized_callsign);
        let base = if parsed.modifier_kind.is_some() {
            Some(parsed.base_callsign.as_str())
        } else {
            None
        };

        let started_at = Instant::now();
        let provider_result = self
            .run_provider_lookup_with_fallback(&normalized_callsign, base)
            .await;
        let latency_ms = duration_to_millis_u32(started_at.elapsed());
        self.provider_result_to_lookup(provider_result, &normalized_callsign, latency_ms, &parsed)
            .await
    }

    /// Perform a streaming lookup state transition.
    ///
    /// The returned vector is intended to be emitted in-order by a transport layer.
    pub async fn stream_lookup(&self, callsign: &str, skip_cache: bool) -> Vec<LookupResult> {
        let normalized_callsign = normalize_callsign(callsign);
        let mut updates = vec![LookupResult {
            state: LookupState::Loading as i32,
            record: None,
            error_message: None,
            cache_hit: false,
            lookup_latency_ms: 0,
            queried_callsign: normalized_callsign.clone(),
            debug_http_exchanges: Vec::new(),
        }];

        if !skip_cache {
            if let Some(entry) = self.get_cache_entry(&normalized_callsign).await {
                if self.is_fresh(&entry) {
                    updates.push(Self::cache_entry_to_result(
                        &entry,
                        &normalized_callsign,
                        None,
                        true,
                    ));
                    return updates;
                }

                if let CachedLookup::Found(_) = entry.lookup {
                    updates.push(Self::cache_entry_to_result(
                        &entry,
                        &normalized_callsign,
                        Some(LookupState::Stale),
                        true,
                    ));
                }
            }
        }

        let parsed = parse_callsign(&normalized_callsign);
        let base = if parsed.modifier_kind.is_some() {
            Some(parsed.base_callsign.as_str())
        } else {
            None
        };

        let started_at = Instant::now();
        let provider_result = self
            .run_provider_lookup_with_fallback(&normalized_callsign, base)
            .await;
        let latency_ms = duration_to_millis_u32(started_at.elapsed());
        updates.push(
            self.provider_result_to_lookup(
                provider_result,
                &normalized_callsign,
                latency_ms,
                &parsed,
            )
            .await,
        );

        updates
    }

    /// Return a cache-only lookup result.
    pub async fn get_cached_callsign(&self, callsign: &str) -> LookupResult {
        let normalized_callsign = normalize_callsign(callsign);
        if let Some(entry) = self.get_cache_entry(&normalized_callsign).await {
            if self.is_fresh(&entry) {
                return Self::cache_entry_to_result(&entry, &normalized_callsign, None, true);
            }
        }

        LookupResult {
            state: LookupState::NotFound as i32,
            record: None,
            error_message: None,
            cache_hit: false,
            lookup_latency_ms: 0,
            queried_callsign: normalized_callsign,
            debug_http_exchanges: Vec::new(),
        }
    }

    async fn get_cache_entry(&self, normalized_callsign: &str) -> Option<CacheEntry> {
        if let Some(entry) = self.cache.read().await.get(normalized_callsign).cloned() {
            return Some(entry);
        }
        // Fall back to persistent snapshot store.
        self.load_snapshot_entry(normalized_callsign).await
    }

    fn is_fresh(&self, entry: &CacheEntry) -> bool {
        entry.cached_at.elapsed() <= self.ttl_for(entry)
    }

    fn ttl_for(&self, entry: &CacheEntry) -> Duration {
        match entry.lookup {
            CachedLookup::Found(_) => self.config.positive_ttl,
            CachedLookup::NotFound => self.config.negative_ttl,
        }
    }

    fn cache_entry_to_result(
        entry: &CacheEntry,
        normalized_callsign: &str,
        state_override: Option<LookupState>,
        cache_hit: bool,
    ) -> LookupResult {
        match &entry.lookup {
            CachedLookup::Found(record) => LookupResult {
                state: state_override.unwrap_or(LookupState::Found) as i32,
                record: Some((**record).clone()),
                error_message: None,
                cache_hit,
                lookup_latency_ms: 0,
                queried_callsign: normalized_callsign.to_string(),
                debug_http_exchanges: Vec::new(),
            },
            CachedLookup::NotFound => LookupResult {
                state: LookupState::NotFound as i32,
                record: None,
                error_message: None,
                cache_hit,
                lookup_latency_ms: 0,
                queried_callsign: normalized_callsign.to_string(),
                debug_http_exchanges: Vec::new(),
            },
        }
    }

    async fn provider_result_to_lookup(
        &self,
        provider_result: ProviderLookupResult,
        normalized_callsign: &str,
        lookup_latency_ms: u32,
        parsed: &crate::domain::callsign_parser::ParsedCallsign,
    ) -> LookupResult {
        match provider_result {
            Ok(ProviderLookup {
                outcome: ProviderLookupOutcome::Found(mut record),
                debug_http_exchanges,
            }) => {
                annotate_record(&mut record, parsed);
                self.store_cache_entry(
                    normalized_callsign,
                    CacheEntry {
                        lookup: CachedLookup::Found(record.clone()),
                        cached_at: Instant::now(),
                    },
                )
                .await;

                LookupResult {
                    state: LookupState::Found as i32,
                    record: Some(*record),
                    error_message: None,
                    cache_hit: false,
                    lookup_latency_ms,
                    queried_callsign: normalized_callsign.to_string(),
                    debug_http_exchanges,
                }
            }
            Ok(ProviderLookup {
                outcome: ProviderLookupOutcome::NotFound,
                debug_http_exchanges,
            }) => {
                self.store_cache_entry(
                    normalized_callsign,
                    CacheEntry {
                        lookup: CachedLookup::NotFound,
                        cached_at: Instant::now(),
                    },
                )
                .await;

                LookupResult {
                    state: LookupState::NotFound as i32,
                    record: None,
                    error_message: None,
                    cache_hit: false,
                    lookup_latency_ms,
                    queried_callsign: normalized_callsign.to_string(),
                    debug_http_exchanges,
                }
            }
            Err(error) => LookupResult {
                state: LookupState::Error as i32,
                record: None,
                error_message: Some(error.to_string()),
                cache_hit: false,
                lookup_latency_ms,
                queried_callsign: normalized_callsign.to_string(),
                debug_http_exchanges: error.debug_http_exchanges().to_vec(),
            },
        }
    }

    async fn store_cache_entry(&self, normalized_callsign: &str, entry: CacheEntry) {
        self.cache
            .write()
            .await
            .insert(normalized_callsign.to_string(), entry.clone());

        // Persist to the snapshot store if available.
        if let Some(ref storage) = self.snapshot_storage {
            let ttl = self.ttl_for(&entry);
            let now_utc = chrono::Utc::now();
            let stored_at = prost_types::Timestamp {
                seconds: now_utc.timestamp(),
                nanos: 0,
            };
            let ttl_secs = i64::try_from(ttl.as_secs()).unwrap_or(i64::MAX);
            let expires_at = prost_types::Timestamp {
                seconds: now_utc.timestamp().saturating_add(ttl_secs),
                nanos: 0,
            };
            let result = match &entry.lookup {
                CachedLookup::Found(record) => LookupResult {
                    state: LookupState::Found as i32,
                    record: Some((**record).clone()),
                    ..Default::default()
                },
                CachedLookup::NotFound => LookupResult {
                    state: LookupState::NotFound as i32,
                    ..Default::default()
                },
            };
            let snapshot = LookupSnapshot {
                callsign: normalized_callsign.to_string(),
                result,
                stored_at,
                expires_at: Some(expires_at),
            };
            if let Err(err) = storage
                .lookup_snapshots()
                .upsert_lookup_snapshot(&snapshot)
                .await
            {
                eprintln!(
                    "[lookup] Failed to persist lookup snapshot for {normalized_callsign}: {err}"
                );
            }
        }
    }

    /// Try to load a lookup entry from persistent storage, returning a fresh
    /// in-memory cache entry when the stored snapshot has not yet expired.
    async fn load_snapshot_entry(&self, normalized_callsign: &str) -> Option<CacheEntry> {
        let storage = self.snapshot_storage.as_ref()?;
        let snapshot = match storage
            .lookup_snapshots()
            .get_lookup_snapshot(normalized_callsign)
            .await
        {
            Ok(Some(s)) => s,
            Ok(None) => return None,
            Err(err) => {
                eprintln!(
                    "[lookup] Failed to load lookup snapshot for {normalized_callsign}: {err}"
                );
                return None;
            }
        };

        // Check expiry — if expired, don't use.
        if let Some(ref expires_at) = snapshot.expires_at {
            let now_secs = chrono::Utc::now().timestamp();
            if now_secs >= expires_at.seconds {
                return None;
            }
        }

        let lookup = if snapshot.result.state == LookupState::Found as i32 {
            if let Some(record) = snapshot.result.record {
                CachedLookup::Found(Box::new(record))
            } else {
                return None;
            }
        } else {
            CachedLookup::NotFound
        };

        // Reconstruct a cache entry with its Instant set so TTL checks pass.
        // Use the remaining lifetime to back-date `cached_at`.
        let remaining = snapshot.expires_at.map_or(Duration::ZERO, |e| {
            let now_secs = chrono::Utc::now().timestamp();
            let remaining_secs =
                u64::try_from(e.seconds.saturating_sub(now_secs).max(0)).unwrap_or(0);
            Duration::from_secs(remaining_secs)
        });
        let ttl = match &lookup {
            CachedLookup::Found(_) => self.config.positive_ttl,
            CachedLookup::NotFound => self.config.negative_ttl,
        };
        let elapsed = ttl.saturating_sub(remaining);
        let cached_at = Instant::now()
            .checked_sub(elapsed)
            .unwrap_or_else(Instant::now);

        let entry = CacheEntry { lookup, cached_at };

        // Promote into in-memory cache so subsequent reads are fast.
        self.cache
            .write()
            .await
            .insert(normalized_callsign.to_string(), entry.clone());

        Some(entry)
    }

    /// Run provider lookup with exact-first / base-callsign-fallback behavior.
    ///
    /// If the exact callsign (e.g. `AE7XI/P`) returns `NotFound`, and a
    /// `base_callsign` is supplied (e.g. `AE7XI`), a second attempt is made
    /// with the base callsign so that providers that do not know slash forms
    /// can still enrich the record.  The cache is always keyed on the original
    /// exact callsign regardless of which attempt succeeded.
    async fn run_provider_lookup_with_fallback(
        &self,
        exact_callsign: &str,
        base_callsign: Option<&str>,
    ) -> ProviderLookupResult {
        let first = self.run_provider_lookup_deduped(exact_callsign).await;

        if let Some(base) = base_callsign {
            if base != exact_callsign {
                if let Ok(ProviderLookup {
                    outcome: ProviderLookupOutcome::NotFound,
                    debug_http_exchanges: first_exchanges,
                }) = first
                {
                    let second = self.run_provider_lookup_deduped(base).await;
                    return second.map(|mut lookup| {
                        lookup.debug_http_exchanges.extend(first_exchanges);
                        lookup
                    });
                }
            }
        }

        first
    }

    async fn run_provider_lookup_deduped(&self, normalized_callsign: &str) -> ProviderLookupResult {
        let normalized_callsign = normalized_callsign.to_string();
        let (lookup_future, owner) = {
            let mut in_flight = self.in_flight.lock().await;
            if let Some(existing) = in_flight.get(&normalized_callsign) {
                (existing.clone(), false)
            } else {
                let provider = Arc::clone(&self.provider);
                let request_callsign = normalized_callsign.clone();
                let future = async move { provider.lookup_callsign(&request_callsign).await }
                    .boxed()
                    .shared();
                in_flight.insert(normalized_callsign.clone(), future.clone());
                (future, true)
            }
        };

        let result = lookup_future.await;
        if owner {
            self.in_flight.lock().await.remove(&normalized_callsign);
        }

        result
    }
}

fn duration_to_millis_u32(duration: Duration) -> u32 {
    match u32::try_from(duration.as_millis()) {
        Ok(value) => value,
        Err(_) => u32::MAX,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };

    use tokio::{sync::Mutex, time::sleep};

    use crate::proto::qsoripper::domain::LookupState;

    use super::*;

    #[derive(Debug, Clone)]
    struct QueueProvider {
        responses: Arc<Mutex<VecDeque<ProviderLookupResult>>>,
        calls: Arc<AtomicUsize>,
        delay: Duration,
    }

    impl QueueProvider {
        fn new(responses: Vec<ProviderLookupResult>, delay: Duration) -> Self {
            Self {
                responses: Arc::new(Mutex::new(VecDeque::from(responses))),
                calls: Arc::new(AtomicUsize::new(0)),
                delay,
            }
        }

        fn call_count(&self) -> usize {
            self.calls.load(Ordering::Relaxed)
        }
    }

    #[tonic::async_trait]
    impl CallsignProvider for QueueProvider {
        async fn lookup_callsign(
            &self,
            _callsign: &str,
        ) -> Result<ProviderLookup, ProviderLookupError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if !self.delay.is_zero() {
                sleep(self.delay).await;
            }

            match self.responses.lock().await.pop_front() {
                Some(result) => result,
                None => Ok(ProviderLookup::not_found(Vec::new())),
            }
        }
    }

    fn found_record(callsign: &str, first_name: &str) -> CallsignRecord {
        CallsignRecord {
            callsign: callsign.to_string(),
            cross_ref: callsign.to_string(),
            first_name: first_name.to_string(),
            last_name: "Operator".to_string(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn lookup_returns_cache_hit_on_second_call() {
        let provider = QueueProvider::new(
            vec![Ok(ProviderLookup::found(
                found_record("W1AW", "Initial"),
                Vec::new(),
            ))],
            Duration::ZERO,
        );
        let coordinator = LookupCoordinator::new(
            Arc::new(provider.clone()),
            LookupCoordinatorConfig::new(Duration::from_secs(60), Duration::from_secs(60)),
        );

        let first = coordinator.lookup("w1aw", false).await;
        let second = coordinator.lookup("w1aw", false).await;

        assert_eq!(first.state, LookupState::Found as i32);
        assert!(!first.cache_hit);
        assert_eq!(second.state, LookupState::Found as i32);
        assert!(second.cache_hit);
        assert_eq!(provider.call_count(), 1);
    }

    #[tokio::test]
    async fn skip_cache_forces_provider_lookup() {
        let provider = QueueProvider::new(
            vec![
                Ok(ProviderLookup::found(
                    found_record("W1AW", "First"),
                    Vec::new(),
                )),
                Ok(ProviderLookup::found(
                    found_record("W1AW", "Second"),
                    Vec::new(),
                )),
            ],
            Duration::ZERO,
        );
        let coordinator = LookupCoordinator::new(
            Arc::new(provider.clone()),
            LookupCoordinatorConfig::new(Duration::from_secs(60), Duration::from_secs(60)),
        );

        let _ = coordinator.lookup("w1aw", false).await;
        let second = coordinator.lookup("w1aw", true).await;

        assert_eq!(second.state, LookupState::Found as i32);
        assert!(!second.cache_hit);
        assert_eq!(provider.call_count(), 2);
    }

    #[tokio::test]
    async fn stream_lookup_emits_loading_stale_and_refreshed_found() {
        let provider = QueueProvider::new(
            vec![
                Ok(ProviderLookup::found(
                    found_record("W1AW", "Cached"),
                    Vec::new(),
                )),
                Ok(ProviderLookup::found(
                    found_record("W1AW", "Fresh"),
                    Vec::new(),
                )),
            ],
            Duration::ZERO,
        );
        let coordinator = LookupCoordinator::new(
            Arc::new(provider.clone()),
            LookupCoordinatorConfig::new(Duration::from_millis(1), Duration::from_secs(60)),
        );

        let _ = coordinator.lookup("w1aw", false).await;
        sleep(Duration::from_millis(5)).await;
        let updates = coordinator.stream_lookup("w1aw", false).await;

        assert_eq!(updates.len(), 3);
        let first = updates.first().expect("loading update expected");
        let second = updates.get(1).expect("stale update expected");
        let third = updates.get(2).expect("found update expected");
        assert_eq!(first.state, LookupState::Loading as i32);
        assert_eq!(second.state, LookupState::Stale as i32);
        assert_eq!(third.state, LookupState::Found as i32);

        let stale_record = second.record.as_ref().expect("stale record expected");
        let fresh_record = third.record.as_ref().expect("fresh result record expected");
        assert_eq!(stale_record.first_name, "Cached");
        assert_eq!(fresh_record.first_name, "Fresh");
        assert_eq!(provider.call_count(), 2);
    }

    #[tokio::test]
    async fn concurrent_identical_lookups_share_inflight_request() {
        let provider = QueueProvider::new(
            vec![Ok(ProviderLookup::found(
                found_record("W1AW", "Shared"),
                Vec::new(),
            ))],
            Duration::from_millis(30),
        );
        let coordinator = Arc::new(LookupCoordinator::new(
            Arc::new(provider.clone()),
            LookupCoordinatorConfig::new(Duration::from_secs(60), Duration::from_secs(60)),
        ));

        let first_coordinator = Arc::clone(&coordinator);
        let second_coordinator = Arc::clone(&coordinator);
        let (first, second) = tokio::join!(
            first_coordinator.lookup("w1aw", true),
            second_coordinator.lookup("W1AW", true)
        );

        assert_eq!(first.state, LookupState::Found as i32);
        assert_eq!(second.state, LookupState::Found as i32);
        assert_eq!(provider.call_count(), 1);
    }

    // -- Snapshot persistence tests -----------------------------------------

    use std::collections::BTreeMap;

    use crate::storage::{
        EngineStorage, LogbookCounts, LogbookStore, LookupSnapshot, LookupSnapshotStore,
        QsoListQuery, StorageError, SyncMetadata,
    };

    /// Minimal in-memory implementation of [`EngineStorage`] for snapshot tests.
    /// Only the [`LookupSnapshotStore`] methods are exercised; the logbook
    /// surface is left unimplemented.
    struct MockSnapshotStorage {
        snapshots: tokio::sync::RwLock<BTreeMap<String, LookupSnapshot>>,
    }

    impl MockSnapshotStorage {
        fn new() -> Self {
            Self {
                snapshots: tokio::sync::RwLock::new(BTreeMap::new()),
            }
        }
    }

    impl EngineStorage for MockSnapshotStorage {
        fn logbook(&self) -> &dyn LogbookStore {
            unimplemented!("logbook not used in snapshot tests")
        }
        fn lookup_snapshots(&self) -> &dyn LookupSnapshotStore {
            self
        }
        fn backend_name(&self) -> &'static str {
            "mock-snapshot"
        }
    }

    #[tonic::async_trait]
    impl LogbookStore for MockSnapshotStorage {
        async fn insert_qso(
            &self,
            _qso: &crate::proto::qsoripper::domain::QsoRecord,
        ) -> Result<(), StorageError> {
            unimplemented!()
        }
        async fn update_qso(
            &self,
            _qso: &crate::proto::qsoripper::domain::QsoRecord,
        ) -> Result<bool, StorageError> {
            unimplemented!()
        }
        async fn delete_qso(&self, _local_id: &str) -> Result<bool, StorageError> {
            unimplemented!()
        }
        async fn get_qso(
            &self,
            _local_id: &str,
        ) -> Result<Option<crate::proto::qsoripper::domain::QsoRecord>, StorageError> {
            unimplemented!()
        }
        async fn list_qsos(
            &self,
            _query: &QsoListQuery,
        ) -> Result<Vec<crate::proto::qsoripper::domain::QsoRecord>, StorageError> {
            unimplemented!()
        }
        async fn qso_counts(&self) -> Result<LogbookCounts, StorageError> {
            unimplemented!()
        }
        async fn get_sync_metadata(&self) -> Result<SyncMetadata, StorageError> {
            unimplemented!()
        }
        async fn upsert_sync_metadata(&self, _metadata: &SyncMetadata) -> Result<(), StorageError> {
            unimplemented!()
        }
    }

    #[tonic::async_trait]
    impl LookupSnapshotStore for MockSnapshotStorage {
        async fn get_lookup_snapshot(
            &self,
            callsign: &str,
        ) -> Result<Option<LookupSnapshot>, StorageError> {
            Ok(self.snapshots.read().await.get(callsign).cloned())
        }
        async fn upsert_lookup_snapshot(
            &self,
            snapshot: &LookupSnapshot,
        ) -> Result<(), StorageError> {
            self.snapshots
                .write()
                .await
                .insert(snapshot.callsign.clone(), snapshot.clone());
            Ok(())
        }
        async fn delete_lookup_snapshot(&self, callsign: &str) -> Result<bool, StorageError> {
            Ok(self.snapshots.write().await.remove(callsign).is_some())
        }
    }

    #[tokio::test]
    async fn lookup_persists_snapshot_to_storage() {
        let storage = Arc::new(MockSnapshotStorage::new());
        let provider = QueueProvider::new(
            vec![Ok(ProviderLookup::found(
                found_record("W1AW", "Persisted"),
                Vec::new(),
            ))],
            Duration::ZERO,
        );
        let coordinator = LookupCoordinator::with_snapshot_store(
            Arc::new(provider),
            LookupCoordinatorConfig::new(Duration::from_secs(300), Duration::from_secs(60)),
            storage.clone() as Arc<dyn EngineStorage>,
        );

        let result = coordinator.lookup("W1AW", false).await;
        assert_eq!(result.state, LookupState::Found as i32);

        // The snapshot should now be persisted in storage.
        let snapshot = storage
            .lookup_snapshots()
            .get_lookup_snapshot("W1AW")
            .await
            .expect("storage read")
            .expect("snapshot should exist after lookup");
        assert_eq!(snapshot.callsign, "W1AW");
        assert_eq!(snapshot.result.state, LookupState::Found as i32);
        assert!(snapshot.expires_at.is_some());
    }

    #[tokio::test]
    async fn lookup_loads_fresh_snapshot_from_storage_on_cold_start() {
        let storage = Arc::new(MockSnapshotStorage::new());

        // Pre-populate a fresh snapshot in storage (expires far in the future).
        let now_secs = chrono::Utc::now().timestamp();
        let snapshot = LookupSnapshot {
            callsign: "W1AW".to_string(),
            result: LookupResult {
                state: LookupState::Found as i32,
                record: Some(CallsignRecord {
                    callsign: "W1AW".to_string(),
                    first_name: "Stored".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            },
            stored_at: prost_types::Timestamp {
                seconds: now_secs,
                nanos: 0,
            },
            expires_at: Some(prost_types::Timestamp {
                seconds: now_secs + 600,
                nanos: 0,
            }),
        };
        storage
            .lookup_snapshots()
            .upsert_lookup_snapshot(&snapshot)
            .await
            .unwrap();

        // Create a coordinator with an empty in-memory cache. The provider
        // should NOT be called — the persisted snapshot should be used.
        let provider = QueueProvider::new(Vec::new(), Duration::ZERO);
        let coordinator = LookupCoordinator::with_snapshot_store(
            Arc::new(provider.clone()),
            LookupCoordinatorConfig::new(Duration::from_secs(600), Duration::from_secs(60)),
            storage as Arc<dyn EngineStorage>,
        );

        let result = coordinator.lookup("W1AW", false).await;
        assert_eq!(result.state, LookupState::Found as i32);
        assert!(
            result.cache_hit,
            "result should be a cache hit from storage"
        );
        let record = result.record.expect("record should be present");
        assert_eq!(record.first_name, "Stored");
        assert_eq!(provider.call_count(), 0, "provider should not be called");
    }

    #[tokio::test]
    async fn lookup_ignores_expired_snapshot_from_storage() {
        let storage = Arc::new(MockSnapshotStorage::new());

        // Pre-populate an expired snapshot in storage.
        let now_secs = chrono::Utc::now().timestamp();
        let snapshot = LookupSnapshot {
            callsign: "W1AW".to_string(),
            result: LookupResult {
                state: LookupState::Found as i32,
                record: Some(CallsignRecord {
                    callsign: "W1AW".to_string(),
                    first_name: "Expired".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            },
            stored_at: prost_types::Timestamp {
                seconds: now_secs - 1000,
                nanos: 0,
            },
            expires_at: Some(prost_types::Timestamp {
                seconds: now_secs - 1, // already expired
                nanos: 0,
            }),
        };
        storage
            .lookup_snapshots()
            .upsert_lookup_snapshot(&snapshot)
            .await
            .unwrap();

        // The coordinator should ignore the expired snapshot and call the provider.
        let provider = QueueProvider::new(
            vec![Ok(ProviderLookup::found(
                found_record("W1AW", "Fresh"),
                Vec::new(),
            ))],
            Duration::ZERO,
        );
        let coordinator = LookupCoordinator::with_snapshot_store(
            Arc::new(provider.clone()),
            LookupCoordinatorConfig::new(Duration::from_secs(600), Duration::from_secs(60)),
            storage as Arc<dyn EngineStorage>,
        );

        let result = coordinator.lookup("W1AW", false).await;
        assert_eq!(result.state, LookupState::Found as i32);
        assert!(!result.cache_hit);
        let record = result.record.expect("record should be present");
        assert_eq!(record.first_name, "Fresh");
        assert_eq!(provider.call_count(), 1, "provider should be called");
    }
}

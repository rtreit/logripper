//! Lookup orchestration with cache policy and in-flight request deduplication.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use futures::future::{BoxFuture, FutureExt, Shared};
use tokio::sync::{Mutex, RwLock};

use crate::{
    domain::lookup::normalize_callsign,
    proto::logripper::domain::{CallsignRecord, LookupResult, LookupState},
};

use super::provider::{CallsignProvider, ProviderLookup, ProviderLookupError};

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

        let started_at = Instant::now();
        let provider_result = self.run_provider_lookup_deduped(&normalized_callsign).await;
        let latency_ms = duration_to_millis_u32(started_at.elapsed());
        self.provider_result_to_lookup(provider_result, &normalized_callsign, latency_ms)
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

        let started_at = Instant::now();
        let provider_result = self.run_provider_lookup_deduped(&normalized_callsign).await;
        let latency_ms = duration_to_millis_u32(started_at.elapsed());
        updates.push(
            self.provider_result_to_lookup(provider_result, &normalized_callsign, latency_ms)
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
        }
    }

    async fn get_cache_entry(&self, normalized_callsign: &str) -> Option<CacheEntry> {
        self.cache.read().await.get(normalized_callsign).cloned()
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
            },
            CachedLookup::NotFound => LookupResult {
                state: LookupState::NotFound as i32,
                record: None,
                error_message: None,
                cache_hit,
                lookup_latency_ms: 0,
                queried_callsign: normalized_callsign.to_string(),
            },
        }
    }

    async fn provider_result_to_lookup(
        &self,
        provider_result: ProviderLookupResult,
        normalized_callsign: &str,
        lookup_latency_ms: u32,
    ) -> LookupResult {
        match provider_result {
            Ok(ProviderLookup::Found(record)) => {
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
                }
            }
            Ok(ProviderLookup::NotFound) => {
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
                }
            }
            Err(error) => LookupResult {
                state: LookupState::Error as i32,
                record: None,
                error_message: Some(error.to_string()),
                cache_hit: false,
                lookup_latency_ms,
                queried_callsign: normalized_callsign.to_string(),
            },
        }
    }

    async fn store_cache_entry(&self, normalized_callsign: &str, entry: CacheEntry) {
        self.cache
            .write()
            .await
            .insert(normalized_callsign.to_string(), entry);
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

    use crate::proto::logripper::domain::LookupState;

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
                None => Ok(ProviderLookup::NotFound),
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
            vec![Ok(ProviderLookup::Found(Box::new(found_record(
                "W1AW", "Initial",
            ))))],
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
                Ok(ProviderLookup::Found(Box::new(found_record(
                    "W1AW", "First",
                )))),
                Ok(ProviderLookup::Found(Box::new(found_record(
                    "W1AW", "Second",
                )))),
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
                Ok(ProviderLookup::Found(Box::new(found_record(
                    "W1AW", "Cached",
                )))),
                Ok(ProviderLookup::Found(Box::new(found_record(
                    "W1AW", "Fresh",
                )))),
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
            vec![Ok(ProviderLookup::Found(Box::new(found_record(
                "W1AW", "Shared",
            ))))],
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
}

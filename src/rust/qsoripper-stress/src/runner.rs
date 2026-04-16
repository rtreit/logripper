#![expect(
    clippy::indexing_slicing,
    reason = "Stress vectors iterate fixed non-empty catalogs with modulo-based indexing."
)]
#![expect(
    clippy::semicolon_if_nothing_returned,
    reason = "Async match arms stay easier to scan without extra unit statements."
)]

use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use difa::Record;
use prost_types::Timestamp;
use qsoripper_core::adif::{parse_adi_qsos, serialize_adi_qsos, AdifMapper};
use qsoripper_core::domain::band::{band_from_adif, band_from_frequency_mhz};
use qsoripper_core::domain::mode::mode_from_adif;
use qsoripper_core::ffi;
use qsoripper_core::proto::qsoripper::domain::{Band, Mode, QsoRecord};
use qsoripper_core::proto::qsoripper::services::{
    logbook_service_client::LogbookServiceClient, lookup_service_client::LookupServiceClient,
    AdifChunk, DeleteQsoRequest, ExportAdifRequest, ImportAdifRequest, ListQsosRequest,
    LogQsoRequest, LookupRequest, QsoSortOrder, StreamLookupRequest, StressLogEntry,
    StressLogLevel, StressProcessMetrics, StressRunConfiguration, StressRunSnapshot,
    StressRunState, StressVectorState, StressVectorStatus, UpdateQsoRequest,
};
use sysinfo::{Pid, ProcessesToUpdate, System};
use tokio::process::{Child, Command};
use tokio::sync::{watch, Mutex};
use tokio::task::JoinSet;
use tokio_stream::iter;
use tokio_util::sync::CancellationToken;
use tonic::{transport::Endpoint, Code};
use uuid::Uuid;

const CORE_VECTORS: [(&str, &str); 5] = [
    ("core-adif-parse", "ADIF Parse Fuzzing"),
    ("core-adif-mapper", "ADIF Mapper Fuzzing"),
    ("core-qso-roundtrip", "QSO Roundtrip Chaos"),
    ("core-band-mode", "Band and Mode Parsing"),
    ("core-ffi", "FFI Abuse"),
];

const GRPC_VECTORS: [(&str, &str); 12] = [
    ("grpc-logqso-adversarial", "LogQso Adversarial"),
    ("grpc-logqso-oversized", "LogQso Oversized Payload"),
    ("grpc-logqso-missing", "LogQso Missing Fields"),
    ("grpc-updateqso-garbage", "UpdateQso Garbage"),
    ("grpc-deleteqso-garbage", "DeleteQso Garbage"),
    ("grpc-getqso-garbage", "GetQso Garbage"),
    ("grpc-listqsos-chaos", "ListQsos Chaos"),
    ("grpc-lookup-adversarial", "Lookup Adversarial"),
    ("grpc-streamlookup-adversarial", "StreamLookup Adversarial"),
    ("grpc-streamlookup-cancel", "StreamLookup Cancel Storm"),
    ("grpc-importadif-garbage", "ImportAdif Garbage"),
    ("grpc-exportadif-chaos", "ExportAdif Chaos"),
];

pub(crate) struct SharedHarness {
    state: Mutex<HarnessState>,
    updates: watch::Sender<StressRunSnapshot>,
}

impl SharedHarness {
    pub(crate) fn new() -> Self {
        let state = HarnessState::new();
        let (updates, _) = watch::channel(state.snapshot.clone());
        Self {
            state: Mutex::new(state),
            updates,
        }
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<StressRunSnapshot> {
        self.updates.subscribe()
    }

    pub(crate) fn current_snapshot(&self) -> StressRunSnapshot {
        self.updates.borrow().clone()
    }

    pub(crate) async fn with_state<F>(&self, update: F)
    where
        F: FnOnce(&mut HarnessState),
    {
        let mut guard = self.state.lock().await;
        update(&mut guard);
    }

    pub(crate) async fn publish(&self) -> StressRunSnapshot {
        let mut guard = self.state.lock().await;
        let snapshot = guard.publish_snapshot();
        let _ = self.updates.send(snapshot.clone());
        snapshot
    }
}

pub(crate) struct HarnessState {
    snapshot: StressRunSnapshot,
    vectors: BTreeMap<String, VectorEntry>,
    processes: BTreeMap<String, StressProcessMetrics>,
    events: VecDeque<StressLogEntry>,
    event_log_path: Option<PathBuf>,
    recent_event_limit: usize,
    last_publish_instant: Instant,
    last_published_total_operations: u64,
}

struct VectorEntry {
    status: StressVectorStatus,
    last_published_total_operations: u64,
}

impl HarnessState {
    fn new() -> Self {
        Self {
            snapshot: StressRunSnapshot {
                state: StressRunState::Idle as i32,
                status_message: "Idle".to_string(),
                ..StressRunSnapshot::default()
            },
            vectors: BTreeMap::new(),
            processes: BTreeMap::new(),
            events: VecDeque::new(),
            event_log_path: None,
            recent_event_limit: 50,
            last_publish_instant: Instant::now(),
            last_published_total_operations: 0,
        }
    }

    fn reset_run(&mut self, profile_name: &str, configuration: &StressRunConfiguration) {
        self.snapshot = StressRunSnapshot {
            run_id: Uuid::new_v4().to_string(),
            state: StressRunState::Starting as i32,
            active_profile_name: profile_name.to_string(),
            configuration: Some(configuration.clone()),
            started_at_utc: timestamp_now(),
            status_message: "Starting stress run.".to_string(),
            ..StressRunSnapshot::default()
        };
        self.vectors.clear();
        self.processes.clear();
        self.events.clear();
        self.event_log_path = create_event_log(profile_name, self.snapshot.run_id.as_str()).ok();
        self.recent_event_limit = usize::try_from(configuration.recent_event_limit).unwrap_or(50);
        if self.recent_event_limit == 0 {
            self.recent_event_limit = 50;
        }
        self.last_publish_instant = Instant::now();
        self.last_published_total_operations = 0;
    }

    fn register_vector(&mut self, vector_id: &str, display_name: &str) {
        self.vectors.insert(
            vector_id.to_string(),
            VectorEntry {
                status: StressVectorStatus {
                    vector_id: vector_id.to_string(),
                    display_name: display_name.to_string(),
                    state: StressVectorState::Idle as i32,
                    ..StressVectorStatus::default()
                },
                last_published_total_operations: 0,
            },
        );
    }

    pub(crate) fn set_state(&mut self, state: StressRunState, message: impl Into<String>) {
        self.snapshot.state = state as i32;
        self.snapshot.status_message = message.into();
        if matches!(
            state,
            StressRunState::Completed | StressRunState::Failed | StressRunState::Stopped
        ) {
            self.snapshot.ended_at_utc = timestamp_now();
        }
    }

    fn set_vector_state(&mut self, vector_id: &str, state: StressVectorState) {
        if let Some(entry) = self.vectors.get_mut(vector_id) {
            entry.status.state = state as i32;
        }
    }

    fn record_operation(&mut self, vector_id: &str, sample: String) {
        self.record_attempt(vector_id, sample);
    }

    fn record_attempt(&mut self, vector_id: &str, sample: String) {
        self.snapshot.total_operations = self.snapshot.total_operations.saturating_add(1);
        if let Some(entry) = self.vectors.get_mut(vector_id) {
            entry.status.total_operations = entry.status.total_operations.saturating_add(1);
            entry.status.last_sample_input = Some(sample);
            entry.status.last_activity_utc = timestamp_now();
            if entry.status.state != StressVectorState::Failed as i32 {
                entry.status.state = StressVectorState::Running as i32;
            }
        }
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "Callers already own the error text produced by async task joins and tonic failures."
    )]
    fn record_error(&mut self, vector_id: &str, sample: String, message: String, internal: bool) {
        let sample_summary = preview_string(sample.as_str());
        let noteworthy = is_noteworthy_failure(message.as_str(), internal);
        self.record_attempt(vector_id, sample);
        self.snapshot.error_count = self.snapshot.error_count.saturating_add(1);
        if internal {
            self.snapshot.internal_error_count =
                self.snapshot.internal_error_count.saturating_add(1);
        }

        if let Some(entry) = self.vectors.get_mut(vector_id) {
            entry.status.error_count = entry.status.error_count.saturating_add(1);
            if internal {
                entry.status.internal_error_count =
                    entry.status.internal_error_count.saturating_add(1);
            }
            entry.status.last_error_message = Some(message.clone());
        }

        if noteworthy {
            self.push_event(
                StressLogLevel::Error,
                format!("Vector {vector_id} failure on {sample_summary}: {message}"),
                Some(vector_id),
            );
        }
    }

    fn update_process(&mut self, process_name: &str, metrics: StressProcessMetrics) {
        self.processes.insert(process_name.to_string(), metrics);
    }

    pub(crate) fn push_event(
        &mut self,
        level: StressLogLevel,
        message: impl Into<String>,
        vector_id: Option<&str>,
    ) {
        let entry = StressLogEntry {
            occurred_at_utc: timestamp_now(),
            level: level as i32,
            message: message.into(),
            vector_id: vector_id.map(ToString::to_string),
        };
        self.events.push_back(entry.clone());
        while self.events.len() > self.recent_event_limit {
            self.events.pop_front();
        }
        if let Some(path) = &self.event_log_path {
            let _ = append_event_log(path, &entry);
        }
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "The dashboard shows approximate rates; integer exactness is not meaningful for this telemetry."
    )]
    fn publish_snapshot(&mut self) -> StressRunSnapshot {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_publish_instant).as_secs_f64();
        if elapsed > 0.0 {
            let delta_total = self
                .snapshot
                .total_operations
                .saturating_sub(self.last_published_total_operations);
            self.snapshot.operations_per_second = delta_total as f64 / elapsed;
            self.last_published_total_operations = self.snapshot.total_operations;

            for entry in self.vectors.values_mut() {
                let delta = entry
                    .status
                    .total_operations
                    .saturating_sub(entry.last_published_total_operations);
                entry.status.operations_per_second = delta as f64 / elapsed;
                entry.last_published_total_operations = entry.status.total_operations;
            }

            self.last_publish_instant = now;
        }

        self.snapshot.vector_statuses = self
            .vectors
            .values()
            .map(|entry| entry.status.clone())
            .collect();
        self.snapshot.processes = self.processes.values().cloned().collect();
        self.snapshot.recent_events = self.events.iter().cloned().collect();
        self.snapshot.clone()
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "The run lifecycle is easier to audit while startup, worker fan-out, and cleanup remain in one routine."
)]
pub(crate) async fn run_session(
    shared: Arc<SharedHarness>,
    cancellation: CancellationToken,
    profile_name: String,
    configuration: StressRunConfiguration,
) {
    shared
        .with_state(|state| {
            state.reset_run(profile_name.as_str(), &configuration);
            if configuration.include_core_vectors {
                for (vector_id, display_name) in CORE_VECTORS {
                    state.register_vector(vector_id, display_name);
                }
            }
            if configuration.include_grpc_vectors {
                for (vector_id, display_name) in GRPC_VECTORS {
                    state.register_vector(vector_id, display_name);
                }
            }
            if let Some(path) = &state.event_log_path {
                state.push_event(
                    StressLogLevel::Info,
                    format!("Persistent stress event log: {}.", path.display()),
                    None,
                );
            }
            state.push_event(
                StressLogLevel::Info,
                format!("Profile '{profile_name}' selected."),
                None,
            );
        })
        .await;
    shared.publish().await;

    let mut managed_engine = if configuration.auto_start_engine {
        match start_engine(configuration.engine_endpoint.as_str()) {
            Ok(engine) => {
                let sqlite_path = engine.sqlite_path.display().to_string();
                shared
                    .with_state(|state| {
                        state.push_event(
                            StressLogLevel::Info,
                            format!(
                                "Started isolated engine on {} with stress log {}.",
                                configuration.engine_endpoint, sqlite_path
                            ),
                            None,
                        );
                    })
                    .await;
                Some(engine)
            }
            Err(error) => {
                shared
                    .with_state(|state| {
                        state.set_state(StressRunState::Failed, "Engine startup failed.");
                        state.push_event(StressLogLevel::Error, error.clone(), None);
                    })
                    .await;
                shared.publish().await;
                return;
            }
        }
    } else {
        None
    };

    if configuration.auto_start_engine {
        if let Err(error) = wait_for_endpoint(configuration.engine_endpoint.as_str()).await {
            shared
                .with_state(|state| {
                    state.set_state(StressRunState::Failed, "Engine did not become ready.");
                    state.push_event(StressLogLevel::Error, error.clone(), None);
                })
                .await;
            if let Some(engine) = managed_engine.as_mut() {
                stop_engine(engine).await;
            }
            shared.publish().await;
            return;
        }
    }

    shared
        .with_state(|state| {
            state.set_state(StressRunState::Running, "Stress run active.");
            state.push_event(StressLogLevel::Info, "Stress run is active.", None);
            for (vector_id, _) in CORE_VECTORS {
                state.set_vector_state(vector_id, StressVectorState::Running);
            }
            for (vector_id, _) in GRPC_VECTORS {
                state.set_vector_state(vector_id, StressVectorState::Running);
            }
        })
        .await;
    shared.publish().await;

    let completion_state = Arc::new(Mutex::new(StressRunState::Stopped));
    let mut tasks = JoinSet::new();

    if configuration.include_core_vectors {
        tasks.spawn(run_core_adif_parse(
            Arc::clone(&shared),
            cancellation.clone(),
        ));
        tasks.spawn(run_core_adif_mapper(
            Arc::clone(&shared),
            cancellation.clone(),
        ));
        tasks.spawn(run_core_qso_roundtrip(
            Arc::clone(&shared),
            cancellation.clone(),
        ));
        tasks.spawn(run_core_band_mode(
            Arc::clone(&shared),
            cancellation.clone(),
        ));
        tasks.spawn(run_core_ffi(Arc::clone(&shared), cancellation.clone()));
    }

    if configuration.include_grpc_vectors {
        spawn_grpc_vector_tasks(
            &mut tasks,
            &shared,
            &cancellation,
            configuration.engine_endpoint.as_str(),
            configuration.grpc_parallelism,
        );
    }

    tasks.spawn(run_process_sampler(
        Arc::clone(&shared),
        cancellation.clone(),
        configuration.metrics_interval_ms,
        managed_engine.as_ref().and_then(|engine| engine.process_id),
    ));

    if configuration.duration_seconds > 0 {
        let duration = Duration::from_secs(u64::from(configuration.duration_seconds));
        let timer_shared = Arc::clone(&shared);
        let timer_cancel = cancellation.clone();
        let timer_state = Arc::clone(&completion_state);
        tasks.spawn(async move {
            tokio::time::sleep(duration).await;
            {
                let mut guard = timer_state.lock().await;
                *guard = StressRunState::Completed;
            }
            timer_shared
                .with_state(|state| {
                    state.push_event(
                        StressLogLevel::Info,
                        "Configured duration elapsed. Stopping run.",
                        None,
                    );
                })
                .await;
            timer_cancel.cancel();
        });
    }

    cancellation.cancelled().await;
    while tasks.join_next().await.is_some() {}

    if let Some(engine) = managed_engine.as_mut() {
        stop_engine(engine).await;
    }

    let final_state = *completion_state.lock().await;
    shared
        .with_state(|state| {
            state.set_state(final_state, final_status_message(final_state));
            state.push_event(
                StressLogLevel::Info,
                final_status_message(final_state),
                None,
            );
            for entry in state.vectors.values_mut() {
                if entry.status.state != StressVectorState::Failed as i32 {
                    entry.status.state = StressVectorState::Completed as i32;
                }
            }
        })
        .await;
    shared.publish().await;
}

fn final_status_message(state: StressRunState) -> &'static str {
    match state {
        StressRunState::Completed => "Stress run completed.",
        StressRunState::Failed => "Stress run failed.",
        _ => "Stress run stopped.",
    }
}

fn spawn_grpc_vector_tasks(
    tasks: &mut JoinSet<()>,
    shared: &Arc<SharedHarness>,
    cancellation: &CancellationToken,
    engine_endpoint: &str,
    grpc_parallelism: u32,
) {
    let counts = distribute_workers(grpc_parallelism, GRPC_VECTORS.len());
    for ((vector_id, display_name), worker_count) in GRPC_VECTORS.into_iter().zip(counts) {
        for worker_index in 0..worker_count {
            let shared = Arc::clone(shared);
            let cancellation = cancellation.clone();
            let engine_endpoint = engine_endpoint.to_string();
            let vector_id = vector_id.to_string();
            let display_name = display_name.to_string();
            let vector_kind = GrpcVectorKind::from_id(vector_id.as_str());
            tasks.spawn(async move {
                run_grpc_vector(
                    &shared,
                    &cancellation,
                    engine_endpoint.as_str(),
                    vector_id,
                    display_name,
                    vector_kind,
                    worker_index,
                )
                .await;
            });
        }
    }
}

fn distribute_workers(total_workers: u32, vector_count: usize) -> Vec<usize> {
    let total_workers = usize::try_from(total_workers).unwrap_or(0);
    let mut counts = vec![0; vector_count];
    if total_workers == 0 || vector_count == 0 {
        return counts;
    }

    let base = total_workers / vector_count;
    let remainder = total_workers % vector_count;
    for (index, count) in counts.iter_mut().enumerate() {
        *count = base + usize::from(index < remainder);
    }

    counts
}

async fn run_core_adif_parse(shared: Arc<SharedHarness>, cancellation: CancellationToken) {
    let payloads = adversarial_adif_payloads();
    let mut index = 0usize;
    while !cancellation.is_cancelled() {
        let payload = payloads[index % payloads.len()].clone();
        let sample = preview_bytes(&payload);
        let handle = tokio::spawn(async move {
            let _ = parse_adi_qsos(&payload).await;
        });
        match handle.await {
            Ok(()) => {
                shared
                    .with_state(|state| state.record_operation("core-adif-parse", sample))
                    .await
            }
            Err(error) => {
                shared
                    .with_state(|state| {
                        state.record_error(
                            "core-adif-parse",
                            sample,
                            extract_panic_message(error),
                            false,
                        )
                    })
                    .await
            }
        }
        index = index.saturating_add(1);
        tokio::task::yield_now().await;
    }
}

async fn run_core_adif_mapper(shared: Arc<SharedHarness>, cancellation: CancellationToken) {
    let cases = mapper_field_cases();
    let mut index = 0usize;
    while !cancellation.is_cancelled() {
        let (sample, fields) = cases[index % cases.len()].clone();
        let handle = tokio::task::spawn_blocking(move || {
            let mut record = Record::new();
            for (key, value) in &fields {
                let _ = record.insert(key.as_str(), value.as_str());
            }
            let _ = AdifMapper::record_to_qso(&record);
        });
        match handle.await {
            Ok(()) => {
                shared
                    .with_state(|state| state.record_operation("core-adif-mapper", sample))
                    .await
            }
            Err(error) => {
                shared
                    .with_state(|state| {
                        state.record_error(
                            "core-adif-mapper",
                            sample,
                            extract_panic_message(error),
                            false,
                        )
                    })
                    .await
            }
        }
        index = index.saturating_add(1);
        tokio::task::yield_now().await;
    }
}

async fn run_core_qso_roundtrip(shared: Arc<SharedHarness>, cancellation: CancellationToken) {
    let cases = qso_roundtrip_cases();
    let mut index = 0usize;
    while !cancellation.is_cancelled() {
        let (sample, qso) = cases[index % cases.len()].clone();
        let handle = tokio::task::spawn_blocking(move || {
            let fields = AdifMapper::qso_to_adif_fields(&qso);
            let _ = AdifMapper::fields_to_adi(&fields);
            let _ = serialize_adi_qsos(&[qso], true);
        });
        match handle.await {
            Ok(()) => {
                shared
                    .with_state(|state| state.record_operation("core-qso-roundtrip", sample))
                    .await
            }
            Err(error) => {
                shared
                    .with_state(|state| {
                        state.record_error(
                            "core-qso-roundtrip",
                            sample,
                            extract_panic_message(error),
                            false,
                        )
                    })
                    .await
            }
        }
        index = index.saturating_add(1);
        tokio::task::yield_now().await;
    }
}

async fn run_core_band_mode(shared: Arc<SharedHarness>, cancellation: CancellationToken) {
    let inputs = adversarial_strings();
    let frequencies = [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        0.0,
        -14.074,
        f64::MAX,
        f64::MIN,
        14.074,
    ];
    let mut index = 0usize;
    while !cancellation.is_cancelled() {
        let sample = inputs[index % inputs.len()].clone();
        let frequency = frequencies[index % frequencies.len()];
        let preview = format!("{} | freq={frequency}", preview_string(sample.as_str()));
        let handle = tokio::task::spawn_blocking(move || {
            let _ = band_from_frequency_mhz(frequency);
            let _ = band_from_adif(sample.as_str());
            let _ = mode_from_adif(sample.as_str());
        });
        match handle.await {
            Ok(()) => {
                shared
                    .with_state(|state| state.record_operation("core-band-mode", preview))
                    .await
            }
            Err(error) => {
                shared
                    .with_state(|state| {
                        state.record_error(
                            "core-band-mode",
                            preview,
                            extract_panic_message(error),
                            false,
                        )
                    })
                    .await
            }
        }
        index = index.saturating_add(1);
        tokio::task::yield_now().await;
    }
}

async fn run_core_ffi(shared: Arc<SharedHarness>, cancellation: CancellationToken) {
    let mut index = 0usize;
    while !cancellation.is_cancelled() {
        let sample = format!("ffi-case-{}", index % 4);
        let handle = tokio::task::spawn_blocking({
            let case = index % 4;
            move || match case {
                0 => {
                    let _ = ffi::hz_to_khz(0);
                }
                1 => {
                    let _ = ffi::hz_to_khz(u64::MAX);
                }
                2 => {
                    let _ = ffi::moving_average(&[]);
                }
                _ => {
                    let _ = ffi::moving_average(&[f64::INFINITY, f64::NEG_INFINITY]);
                }
            }
        });
        match handle.await {
            Ok(()) => {
                shared
                    .with_state(|state| state.record_operation("core-ffi", sample))
                    .await
            }
            Err(error) => {
                shared
                    .with_state(|state| {
                        state.record_error("core-ffi", sample, extract_panic_message(error), false)
                    })
                    .await
            }
        }
        index = index.saturating_add(1);
        tokio::task::yield_now().await;
    }
}

async fn run_grpc_vector(
    shared: &Arc<SharedHarness>,
    cancellation: &CancellationToken,
    engine_endpoint: &str,
    vector_id: String,
    _display_name: String,
    vector_kind: GrpcVectorKind,
    worker_index: usize,
) {
    let endpoint = match Endpoint::from_shared(engine_endpoint.to_string()) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            shared
                .with_state(|state| {
                    state.record_error(
                        vector_id.as_str(),
                        engine_endpoint.to_string(),
                        format!("Invalid engine endpoint: {error}"),
                        false,
                    )
                })
                .await;
            return;
        }
    };

    let channel = match endpoint.connect().await {
        Ok(channel) => channel,
        Err(error) => {
            shared
                .with_state(|state| {
                    state.record_error(
                        vector_id.as_str(),
                        engine_endpoint.to_string(),
                        format!("Failed to connect to engine: {error}"),
                        false,
                    )
                })
                .await;
            return;
        }
    };

    let mut logbook = LogbookServiceClient::new(channel.clone());
    let mut lookup = LookupServiceClient::new(channel);
    let mut iteration = worker_index;

    while !cancellation.is_cancelled() {
        let sample = vector_kind.sample(iteration);
        match vector_kind
            .execute(&mut logbook, &mut lookup, iteration)
            .await
        {
            Ok(()) => {
                shared
                    .with_state(|state| state.record_operation(vector_id.as_str(), sample))
                    .await
            }
            Err(error) => {
                shared
                    .with_state(|state| {
                        state.record_error(
                            vector_id.as_str(),
                            sample,
                            error.message,
                            error.internal,
                        );
                    })
                    .await
            }
        }

        iteration = iteration.saturating_add(1);
        tokio::task::yield_now().await;
    }
}

async fn run_process_sampler(
    shared: Arc<SharedHarness>,
    cancellation: CancellationToken,
    metrics_interval_ms: u32,
    engine_process_id: Option<u32>,
) {
    let interval = Duration::from_millis(u64::from(metrics_interval_ms.max(250)));
    let host_pid = std::process::id();
    let mut system = System::new_all();

    while !cancellation.is_cancelled() {
        system.refresh_processes(ProcessesToUpdate::All, true);

        if let Some(metrics) = process_metrics("stress-host", host_pid, &system) {
            shared
                .with_state(|state| state.update_process("stress-host", metrics))
                .await;
        }

        if let Some(engine_process_id) = engine_process_id {
            if let Some(metrics) = process_metrics("qsoripper-server", engine_process_id, &system) {
                shared
                    .with_state(|state| state.update_process("qsoripper-server", metrics))
                    .await;
            }
        }

        shared.publish().await;
        tokio::time::sleep(interval).await;
    }
}

fn process_metrics(
    process_name: &str,
    process_id: u32,
    system: &System,
) -> Option<StressProcessMetrics> {
    let process = system.process(Pid::from_u32(process_id))?;
    Some(StressProcessMetrics {
        process_name: process_name.to_string(),
        process_id: Some(process_id),
        cpu_usage_percent: f64::from(process.cpu_usage()),
        working_set_bytes: process.memory(),
        virtual_memory_bytes: process.virtual_memory(),
    })
}

struct ManagedEngine {
    child: Child,
    process_id: Option<u32>,
    sqlite_path: PathBuf,
}

fn start_engine(engine_endpoint: &str) -> Result<ManagedEngine, String> {
    let current_executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let target_directory = current_executable
        .parent()
        .ok_or_else(|| "Unable to resolve the stress host target directory.".to_string())?;
    let server_executable = {
        let mut path = target_directory.to_path_buf();
        path.push(executable_name("qsoripper-server"));
        path
    };

    let listen = endpoint_to_listen_argument(engine_endpoint)?;
    let sqlite_path = stress_storage_path()?;
    let mut command = if server_executable.exists() {
        let mut command = Command::new(server_executable);
        command.args([
            "--storage",
            "sqlite",
            "--sqlite-path",
            sqlite_path
                .to_str()
                .ok_or_else(|| "Stress sqlite path is not valid UTF-8.".to_string())?,
            "--listen",
            listen.as_str(),
        ]);
        command
    } else {
        let workspace_manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| "Unable to resolve Rust workspace root.".to_string())?
            .join("Cargo.toml");
        let mut command = Command::new("cargo");
        command.args([
            "run",
            "--manifest-path",
            workspace_manifest
                .to_str()
                .ok_or_else(|| "Workspace manifest path is not valid UTF-8.".to_string())?,
            "-p",
            "qsoripper-server",
            "--",
            "--storage",
            "sqlite",
            "--sqlite-path",
            sqlite_path
                .to_str()
                .ok_or_else(|| "Stress sqlite path is not valid UTF-8.".to_string())?,
            "--listen",
            listen.as_str(),
        ]);
        command
    };

    command.stdout(Stdio::null());
    command.stderr(Stdio::null());
    command.stdin(Stdio::null());
    let child = command.spawn().map_err(|error| error.to_string())?;
    let process_id = child.id();
    Ok(ManagedEngine {
        child,
        process_id,
        sqlite_path,
    })
}

async fn stop_engine(engine: &mut ManagedEngine) {
    let _ = engine.child.kill().await;
    let _ = engine.child.wait().await;
}

async fn wait_for_endpoint(engine_endpoint: &str) -> Result<(), String> {
    let mut attempts = 0u8;
    while attempts < 20 {
        let endpoint = Endpoint::from_shared(engine_endpoint.to_string())
            .map_err(|error| format!("Invalid endpoint '{engine_endpoint}': {error}"))?;
        if endpoint.connect().await.is_ok() {
            return Ok(());
        }

        attempts = attempts.saturating_add(1);
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    Err(format!(
        "Timed out waiting for engine endpoint '{engine_endpoint}'."
    ))
}

fn endpoint_to_listen_argument(engine_endpoint: &str) -> Result<String, String> {
    let trimmed = engine_endpoint
        .trim()
        .trim_end_matches('/')
        .strip_prefix("http://")
        .or_else(|| {
            engine_endpoint
                .trim()
                .trim_end_matches('/')
                .strip_prefix("https://")
        })
        .unwrap_or(engine_endpoint);
    if trimmed.is_empty() {
        return Err("Engine endpoint is empty.".to_string());
    }

    Ok(trimmed.to_string())
}

fn stress_storage_path() -> Result<PathBuf, String> {
    let mut directory = workspace_root().unwrap_or_else(std::env::temp_dir);
    directory.push("artifacts");
    directory.push("stress");
    directory.push("storage");
    std::fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "Unable to create stress storage directory '{}': {error}",
            directory.display()
        )
    })?;

    let file_name = format!("stress-{}.db", Uuid::new_v4());
    Ok(directory.join(file_name))
}

fn create_event_log(profile_name: &str, run_id: &str) -> Result<PathBuf, String> {
    let mut directory = workspace_root().unwrap_or_else(std::env::temp_dir);
    directory.push("artifacts");
    directory.push("stress");
    directory.push("logs");
    std::fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "Unable to create stress log directory '{}': {error}",
            directory.display()
        )
    })?;

    let path = directory.join(format!("stress-run-{run_id}.log"));
    let header = format!(
        "QsoRipper stress run log\nrun_id={run_id}\nprofile={profile_name}\nstarted_at={}\n\n",
        format_timestamp(timestamp_now().as_ref())
    );
    std::fs::write(&path, header).map_err(|error| {
        format!(
            "Unable to initialize stress event log '{}': {error}",
            path.display()
        )
    })?;

    Ok(path)
}

fn append_event_log(path: &PathBuf, entry: &StressLogEntry) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| {
            format!(
                "Unable to open stress event log '{}': {error}",
                path.display()
            )
        })?;
    let vector = entry.vector_id.as_deref().unwrap_or("-");
    let line = format!(
        "{} [{}] [{}] {}\n",
        format_timestamp(entry.occurred_at_utc.as_ref()),
        format_log_level(entry.level),
        vector,
        entry.message
    );
    file.write_all(line.as_bytes()).map_err(|error| {
        format!(
            "Unable to append to stress event log '{}': {error}",
            path.display()
        )
    })
}

fn workspace_root() -> Option<PathBuf> {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .map(std::path::Path::to_path_buf)
}

fn format_timestamp(timestamp: Option<&Timestamp>) -> String {
    timestamp.map_or_else(
        || "unknown-time".to_string(),
        |timestamp| format!("{}.{:09}Z", timestamp.seconds, timestamp.nanos.max(0)),
    )
}

fn format_log_level(level: i32) -> &'static str {
    match StressLogLevel::try_from(level).unwrap_or(StressLogLevel::Unspecified) {
        StressLogLevel::Info => "INFO",
        StressLogLevel::Warning => "WARN",
        StressLogLevel::Error => "ERROR",
        StressLogLevel::Unspecified => "UNKNOWN",
    }
}

fn is_noteworthy_failure(message: &str, internal: bool) -> bool {
    internal
        || message.contains("panic")
        || message.contains("task join error")
        || message.contains("Failed to connect")
}

fn executable_name(base_name: &str) -> OsString {
    if cfg!(windows) {
        OsString::from(format!("{base_name}.exe"))
    } else {
        OsString::from(base_name)
    }
}

fn adversarial_strings() -> Vec<String> {
    vec![
        String::new(),
        " ".to_string(),
        "\0".to_string(),
        "\0\0\0\0\0\0\0\0".to_string(),
        "\t\n\r".to_string(),
        "\u{FFFD}\u{FFFD}".to_string(),
        "\u{200F}\u{202E}".to_string(),
        "\u{FEFF}".to_string(),
        "\u{1F4A9}\u{1F4A9}".to_string(),
        "A\u{0301}".to_string(),
        "X".repeat(2048),
        "\u{00FC}".repeat(512),
        "W1AW".to_string(),
        "K7DBG/P".to_string(),
        "VE3/W1AW/QRP".to_string(),
        "-1".to_string(),
        "NaN".to_string(),
        "Infinity".to_string(),
        "-Infinity".to_string(),
        "20250230".to_string(),
        "99991399".to_string(),
    ]
}

fn adversarial_adif_payloads() -> Vec<Vec<u8>> {
    vec![
        Vec::new(),
        vec![0xFF, 0xFF, 0xFF],
        (0..=255).collect(),
        b"<CALL:4>W1AW<BAND:3>20M<MODE:2>CW<QSO_DATE:8>20250115<TIME_ON:4>1200<STATION_CALLSIGN:4>TEST<eor>".to_vec(),
        "<CALL:4>W1AW<QSO_DATE:8>202\u{00FC}123<TIME_ON:4>1200<BAND:3>20M<MODE:2>CW<STATION_CALLSIGN:4>TEST<eor>"
            .as_bytes()
            .to_vec(),
        b"<CALL:-1>W1AW<eor>".to_vec(),
        b"<CALL:999999999>W1AW<eor>".to_vec(),
        b"<EOH><CALL:4>W1AW<CALL:999999999>W1AW<EOR>".to_vec(),
        b"<CALL:12>TRUNCATED<EOR><QSO_DATE:999999999>20250115".to_vec(),
        vec![b'A'; 16_384],
        vec![b'Z'; 262_144],
        "<CALL:4>\u{1F4A9}<eor>".as_bytes().to_vec(),
    ]
}

fn stream_lookup_churn_callsigns() -> [&'static str; 6] {
    [
        "W1AW",
        "K7DBG/P",
        "VE3/W1AW/QRP",
        "",
        "99991399",
        "\u{200F}\u{202E}",
    ]
}

fn mapper_field_cases() -> Vec<(String, Vec<(String, String)>)> {
    vec![
        ("empty record".to_string(), Vec::new()),
        (
            "call only".to_string(),
            vec![("CALL".to_string(), "W1AW".to_string())],
        ),
        (
            "non-ascii date".to_string(),
            vec![
                ("CALL".to_string(), "W1AW".to_string()),
                ("QSO_DATE".to_string(), "202\u{00FC}123".to_string()),
            ],
        ),
        (
            "adversarial fields".to_string(),
            vec![
                ("CALL".to_string(), "\u{200F}\u{202E}".to_string()),
                ("TIME_ON".to_string(), "ZZZZ".to_string()),
                ("BAND".to_string(), "999ZZZ".to_string()),
                ("MODE".to_string(), "\t\n\r".to_string()),
                ("FREQ".to_string(), "-0.0".to_string()),
            ],
        ),
    ]
}

fn qso_roundtrip_cases() -> Vec<(String, QsoRecord)> {
    vec![
        ("default qso".to_string(), QsoRecord::default()),
        (
            "negative nanos".to_string(),
            QsoRecord {
                worked_callsign: "W1AW".to_string(),
                utc_timestamp: Some(Timestamp {
                    seconds: 1_700_000_000,
                    nanos: -1,
                }),
                ..QsoRecord::default()
            },
        ),
        (
            "extreme band mode".to_string(),
            QsoRecord {
                worked_callsign: "W1AW".to_string(),
                band: i32::MAX,
                mode: i32::MIN,
                ..QsoRecord::default()
            },
        ),
        (
            "null byte strings".to_string(),
            QsoRecord {
                worked_callsign: "\0\0\0".to_string(),
                station_callsign: "\0".to_string(),
                comment: Some("\0".to_string()),
                ..QsoRecord::default()
            },
        ),
        ("huge extra fields".to_string(), {
            let mut qso = QsoRecord {
                worked_callsign: "W1AW".to_string(),
                ..QsoRecord::default()
            };
            for index in 0..64 {
                qso.extra_fields
                    .insert(format!("FIELD_{index}"), "\u{1F4A9}".repeat(32));
            }
            qso
        }),
    ]
}

fn make_adversarial_qso(step: usize) -> QsoRecord {
    let strings = adversarial_strings();
    let value = |offset: usize| strings[(step + offset) % strings.len()].clone();
    let seconds = [1_731_600_000_i64, -1, i64::MIN / 2, i64::MAX / 2, 0][step % 5];
    let nanos = [0, -1, i32::MIN / 2, 999_999_999, 42][step % 5];
    let mut qso = QsoRecord {
        station_callsign: value(0),
        worked_callsign: value(1),
        band: match step % 4 {
            0 => Band::Band20m as i32,
            1 => Band::Band40m as i32,
            2 => -5,
            _ => 99,
        },
        mode: match step % 4 {
            0 => Mode::Ssb as i32,
            1 => Mode::Cw as i32,
            2 => -10,
            _ => 88,
        },
        utc_timestamp: Some(Timestamp { seconds, nanos }),
        comment: Some(value(2)),
        notes: Some(value(3)),
        ..QsoRecord::default()
    };

    for index in 0..(step % 6) {
        qso.extra_fields
            .insert(format!("FIELD_{index}"), value(index + 4));
    }

    qso
}

fn make_oversized_qso(step: usize) -> QsoRecord {
    let mut qso = make_adversarial_qso(step);
    qso.comment = Some("X".repeat(131_072));
    qso.notes = Some("\u{1F4A9}".repeat(65_536));
    for index in 0..32 {
        qso.extra_fields
            .insert(format!("OVERSIZED_{index}"), "Q".repeat(8_192));
    }

    qso
}

fn preview_string(value: &str) -> String {
    let sanitized = value.replace('\0', "\\0").replace('\n', "\\n");
    if sanitized.len() > 80 {
        format!("{}...", &sanitized[..80])
    } else {
        sanitized
    }
}

fn preview_bytes(value: &[u8]) -> String {
    match String::from_utf8(value.to_vec()) {
        Ok(text) => preview_string(text.as_str()),
        Err(_) => format!("{} bytes of binary ADIF input", value.len()),
    }
}

fn extract_panic_message(error: tokio::task::JoinError) -> String {
    if error.is_panic() {
        let payload = error.into_panic();
        if let Some(message) = payload.downcast_ref::<&str>() {
            (*message).to_string()
        } else if let Some(message) = payload.downcast_ref::<String>() {
            message.clone()
        } else {
            "unknown panic payload".to_string()
        }
    } else {
        format!("task join error: {error}")
    }
}

fn timestamp_now() -> Option<Timestamp> {
    system_time_to_timestamp(SystemTime::now())
}

fn system_time_to_timestamp(time: SystemTime) -> Option<Timestamp> {
    let duration = time.duration_since(UNIX_EPOCH).ok()?;
    Some(Timestamp {
        seconds: i64::try_from(duration.as_secs()).ok()?,
        nanos: i32::try_from(duration.subsec_nanos()).ok()?,
    })
}

#[derive(Clone, Copy)]
enum GrpcVectorKind {
    LogQsoAdversarial,
    LogQsoOversized,
    LogQsoMissingFields,
    UpdateQsoGarbage,
    DeleteQsoGarbage,
    GetQsoGarbage,
    ListQsosChaos,
    LookupAdversarial,
    StreamLookupAdversarial,
    StreamLookupCancelStorm,
    ImportAdifGarbage,
    ExportAdifChaos,
}

impl GrpcVectorKind {
    fn from_id(vector_id: &str) -> Self {
        match vector_id {
            "grpc-logqso-adversarial" => Self::LogQsoAdversarial,
            "grpc-logqso-oversized" => Self::LogQsoOversized,
            "grpc-logqso-missing" => Self::LogQsoMissingFields,
            "grpc-updateqso-garbage" => Self::UpdateQsoGarbage,
            "grpc-deleteqso-garbage" => Self::DeleteQsoGarbage,
            "grpc-getqso-garbage" => Self::GetQsoGarbage,
            "grpc-listqsos-chaos" => Self::ListQsosChaos,
            "grpc-lookup-adversarial" => Self::LookupAdversarial,
            "grpc-streamlookup-adversarial" => Self::StreamLookupAdversarial,
            "grpc-streamlookup-cancel" => Self::StreamLookupCancelStorm,
            "grpc-importadif-garbage" => Self::ImportAdifGarbage,
            _ => Self::ExportAdifChaos,
        }
    }

    fn sample(self, iteration: usize) -> String {
        let strings = adversarial_strings();
        let value = strings[iteration % strings.len()].clone();
        match self {
            Self::LogQsoAdversarial => format!("QSO {}", preview_string(value.as_str())),
            Self::LogQsoOversized => "comment=128KiB notes=64KiB extras=32x8KiB".to_string(),
            Self::LogQsoMissingFields => "default QsoRecord".to_string(),
            Self::UpdateQsoGarbage
            | Self::DeleteQsoGarbage
            | Self::GetQsoGarbage
            | Self::ListQsosChaos
            | Self::LookupAdversarial
            | Self::StreamLookupAdversarial => preview_string(value.as_str()),
            Self::StreamLookupCancelStorm => {
                let callsigns = stream_lookup_churn_callsigns();
                format!(
                    "burst={} callsign={}",
                    (iteration % 4) + 1,
                    preview_string(callsigns[iteration % callsigns.len()])
                )
            }
            Self::ImportAdifGarbage => preview_bytes(
                adversarial_adif_payloads()[iteration % adversarial_adif_payloads().len()]
                    .as_slice(),
            ),
            Self::ExportAdifChaos => format!("after-seconds={}", iteration.saturating_mul(97)),
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Keeping each gRPC vector together makes the supported stress surface easy to review."
    )]
    async fn execute(
        self,
        logbook: &mut LogbookServiceClient<tonic::transport::Channel>,
        lookup: &mut LookupServiceClient<tonic::transport::Channel>,
        iteration: usize,
    ) -> Result<(), VectorError> {
        match self {
            Self::LogQsoAdversarial => logbook
                .log_qso(LogQsoRequest {
                    qso: Some(make_adversarial_qso(iteration)),
                    sync_to_qrz: false,
                })
                .await
                .map(|_| ())
                .map_err(|status| VectorError::from_status(&status)),
            Self::LogQsoOversized => logbook
                .log_qso(LogQsoRequest {
                    qso: Some(make_oversized_qso(iteration)),
                    sync_to_qrz: false,
                })
                .await
                .map(|_| ())
                .map_err(|status| VectorError::from_status(&status)),
            Self::LogQsoMissingFields => logbook
                .log_qso(LogQsoRequest {
                    qso: Some(QsoRecord::default()),
                    sync_to_qrz: false,
                })
                .await
                .map(|_| ())
                .map_err(|status| VectorError::from_status(&status)),
            Self::UpdateQsoGarbage => {
                let mut qso = make_adversarial_qso(iteration);
                let garbage_local_id =
                    adversarial_strings()[iteration % adversarial_strings().len()].clone();
                qso.local_id = garbage_local_id;
                logbook
                    .update_qso(UpdateQsoRequest {
                        qso: Some(qso),
                        sync_to_qrz: false,
                    })
                    .await
                    .map(|_| ())
                    .map_err(|status| VectorError::from_status(&status))
            }
            Self::DeleteQsoGarbage => logbook
                .delete_qso(DeleteQsoRequest {
                    local_id: adversarial_strings()[iteration % adversarial_strings().len()]
                        .clone(),
                    delete_from_qrz: false,
                })
                .await
                .map(|_| ())
                .map_err(|status| VectorError::from_status(&status)),
            Self::GetQsoGarbage => logbook
                .get_qso(qsoripper_core::proto::qsoripper::services::GetQsoRequest {
                    local_id: adversarial_strings()[iteration % adversarial_strings().len()]
                        .clone(),
                })
                .await
                .map(|_| ())
                .map_err(|status| VectorError::from_status(&status)),
            Self::ListQsosChaos => {
                let mut response = logbook
                    .list_qsos(ListQsosRequest {
                        callsign_filter: Some(
                            adversarial_strings()[iteration % adversarial_strings().len()].clone(),
                        ),
                        band_filter: Some(match iteration % 3 {
                            0 => Band::Band20m as i32,
                            1 => Band::Band40m as i32,
                            _ => -3,
                        }),
                        mode_filter: Some(match iteration % 3 {
                            0 => Mode::Ssb as i32,
                            1 => Mode::Cw as i32,
                            _ => 77,
                        }),
                        limit: u32::try_from((iteration % 250) + 1).unwrap_or(1),
                        offset: u32::try_from(iteration % 100).unwrap_or(0),
                        sort: QsoSortOrder::NewestFirst as i32,
                        ..ListQsosRequest::default()
                    })
                    .await
                    .map_err(|status| VectorError::from_status(&status))?
                    .into_inner();
                while response
                    .message()
                    .await
                    .map_err(|status| VectorError::from_status(&status))?
                    .is_some()
                {}
                Ok(())
            }
            Self::LookupAdversarial => lookup
                .lookup(LookupRequest {
                    callsign: adversarial_strings()[iteration % adversarial_strings().len()]
                        .clone(),
                    skip_cache: iteration.is_multiple_of(2),
                })
                .await
                .map(|_| ())
                .map_err(|status| VectorError::from_status(&status)),
            Self::StreamLookupAdversarial => {
                let mut response = lookup
                    .stream_lookup(StreamLookupRequest {
                        callsign: adversarial_strings()[iteration % adversarial_strings().len()]
                            .clone(),
                        skip_cache: iteration.is_multiple_of(2),
                    })
                    .await
                    .map_err(|status| VectorError::from_status(&status))?
                    .into_inner();
                while response
                    .message()
                    .await
                    .map_err(|status| VectorError::from_status(&status))?
                    .is_some()
                {}
                Ok(())
            }
            Self::StreamLookupCancelStorm => {
                let callsigns = stream_lookup_churn_callsigns();
                let callsign = callsigns[iteration % callsigns.len()].to_string();
                for burst_index in 0..=(iteration % 4) {
                    let mut response = lookup
                        .stream_lookup(StreamLookupRequest {
                            callsign: callsign.clone(),
                            skip_cache: true,
                        })
                        .await
                        .map_err(|status| VectorError::from_status(&status))?
                        .into_inner();
                    if (iteration + burst_index).is_multiple_of(2) {
                        let _ = response
                            .message()
                            .await
                            .map_err(|status| VectorError::from_status(&status))?;
                    }
                    drop(response);
                    tokio::task::yield_now().await;
                }
                Ok(())
            }
            Self::ImportAdifGarbage => {
                let payloads = adversarial_adif_payloads();
                let payload = payloads[iteration % payloads.len()].clone();
                let request_stream = iter(vec![ImportAdifRequest {
                    chunk: Some(AdifChunk { data: payload }),
                    refresh: false,
                }]);
                logbook
                    .import_adif(request_stream)
                    .await
                    .map(|_| ())
                    .map_err(|status| VectorError::from_status(&status))
            }
            Self::ExportAdifChaos => {
                let after = Timestamp {
                    seconds: i64::try_from(iteration.saturating_mul(97)).unwrap_or(0),
                    nanos: 0,
                };
                let mut response = logbook
                    .export_adif(ExportAdifRequest {
                        after: Some(after),
                        include_header: iteration.is_multiple_of(2),
                        ..ExportAdifRequest::default()
                    })
                    .await
                    .map_err(|status| VectorError::from_status(&status))?
                    .into_inner();
                while response
                    .message()
                    .await
                    .map_err(|status| VectorError::from_status(&status))?
                    .is_some()
                {}
                Ok(())
            }
        }
    }
}

struct VectorError {
    message: String,
    internal: bool,
}

impl VectorError {
    fn from_status(status: &tonic::Status) -> Self {
        Self {
            internal: matches!(status.code(), Code::Internal | Code::Unknown),
            message: format!("{}: {}", status.code(), status.message()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        create_event_log, distribute_workers, endpoint_to_listen_argument, executable_name,
        workspace_root, HarnessState, SharedHarness,
    };
    use qsoripper_core::proto::qsoripper::services::StressRunState;

    #[test]
    fn distribute_workers_spreads_parallelism_across_vectors() {
        let counts = distribute_workers(11, 4);

        assert_eq!(vec![3, 3, 3, 2], counts);
    }

    #[expect(
        clippy::panic,
        reason = "These unit tests intentionally panic on broken helper behavior."
    )]
    #[test]
    fn endpoint_to_listen_argument_strips_scheme_and_trailing_slash() {
        let listen = match endpoint_to_listen_argument("http://127.0.0.1:50051/") {
            Ok(listen) => listen,
            Err(error) => panic!("listen argument should parse: {error}"),
        };

        assert_eq!("127.0.0.1:50051", listen);
    }

    #[test]
    fn executable_name_adds_windows_suffix_only_when_needed() {
        let value = executable_name("qsoripper-server");

        if cfg!(windows) {
            assert_eq!("qsoripper-server.exe", value.to_string_lossy());
        } else {
            assert_eq!("qsoripper-server", value.to_string_lossy());
        }
    }

    #[expect(
        clippy::panic,
        reason = "This unit test intentionally panics if repository root resolution regresses."
    )]
    #[test]
    fn workspace_root_resolves_repository_root() {
        let root = workspace_root()
            .unwrap_or_else(|| panic!("workspace root should resolve from cargo manifest dir"));
        assert!(root.join("README.md").exists());
        assert!(root.join("src").join("rust").join("Cargo.toml").exists());
    }

    #[expect(
        clippy::panic,
        reason = "This unit test intentionally panics if stress event log creation regresses."
    )]
    #[test]
    fn create_event_log_writes_file_under_artifacts_stress_logs() {
        let path = create_event_log("long-haul", "unit-test-run")
            .unwrap_or_else(|error| panic!("event log should be created: {error}"));
        let contents = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("event log should be readable: {error}"));

        assert!(path.to_string_lossy().contains("artifacts"));
        assert!(path.to_string_lossy().contains("stress"));
        assert!(path.to_string_lossy().contains("logs"));
        assert!(contents.contains("run_id=unit-test-run"));
        assert!(contents.contains("profile=long-haul"));

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn shared_harness_starts_in_idle_state() {
        let harness = SharedHarness::new();

        let snapshot = harness.current_snapshot();

        assert_eq!(StressRunState::Idle as i32, snapshot.state);
        assert_eq!("Idle", snapshot.status_message);
    }

    #[test]
    #[expect(
        clippy::panic,
        reason = "This unit test intentionally panics if the built-in vector registry regresses."
    )]
    fn record_error_counts_failed_attempt_as_operation() {
        let mut state = HarnessState::new();
        state.register_vector("grpc-deleteqso-garbage", "DeleteQso Garbage");

        state.record_error(
            "grpc-deleteqso-garbage",
            "NaN".to_string(),
            "invalid request".to_string(),
            false,
        );

        assert_eq!(1, state.snapshot.total_operations);
        assert_eq!(1, state.snapshot.error_count);

        let vector = state
            .vectors
            .get("grpc-deleteqso-garbage")
            .unwrap_or_else(|| panic!("grpc-deleteqso-garbage vector should exist"));
        assert_eq!(1, vector.status.total_operations);
        assert_eq!(1, vector.status.error_count);
        assert_eq!(Some("NaN".to_string()), vector.status.last_sample_input);
    }
}

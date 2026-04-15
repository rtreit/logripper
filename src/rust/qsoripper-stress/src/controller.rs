use std::sync::Arc;

use qsoripper_core::proto::qsoripper::services::{
    stress_control_service_server::StressControlService, GetStressRunStatusRequest,
    GetStressRunStatusResponse, ListStressProfilesRequest, ListStressProfilesResponse,
    StartStressRunRequest, StartStressRunResponse, StopStressRunRequest, StopStressRunResponse,
    StreamStressRunEventsRequest, StreamStressRunEventsResponse, StressLogLevel, StressProfile,
    StressRunConfiguration, StressRunState,
};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status};

use crate::runner::{run_session, SharedHarness};

const DEFAULT_STRESS_ENGINE_ENDPOINT: &str = "http://127.0.0.1:55051";

#[derive(Clone)]
pub(crate) struct StressController {
    shared: Arc<SharedHarness>,
    active_run: Arc<Mutex<Option<ActiveRun>>>,
    profiles: Arc<Vec<StressProfile>>,
}

struct ActiveRun {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl StressController {
    pub(crate) fn new() -> Self {
        Self {
            shared: Arc::new(SharedHarness::new()),
            active_run: Arc::new(Mutex::new(None)),
            profiles: Arc::new(default_profiles()),
        }
    }

    pub(crate) fn subscribe(
        &self,
    ) -> tokio::sync::watch::Receiver<qsoripper_core::proto::qsoripper::services::StressRunSnapshot>
    {
        self.shared.subscribe()
    }

    pub(crate) fn current_snapshot(
        &self,
    ) -> qsoripper_core::proto::qsoripper::services::StressRunSnapshot {
        self.shared.current_snapshot()
    }

    pub(crate) fn list_profiles(&self) -> Vec<StressProfile> {
        self.profiles.as_ref().clone()
    }

    pub(crate) async fn start_run(
        &self,
        request: StartStressRunRequest,
    ) -> Result<qsoripper_core::proto::qsoripper::services::StressRunSnapshot, Status> {
        let mut guard = self.active_run.lock().await;
        if let Some(active_run) = guard.as_ref() {
            if active_run.handle.is_finished() {
                guard.take();
            } else {
                return Err(Status::failed_precondition(
                    "A stress run is already active.",
                ));
            }
        }

        let profile = resolve_profile(self.profiles.as_ref(), request.profile_name.as_str())?;
        let configuration = merge_configuration(&profile, request.configuration);
        let cancel = CancellationToken::new();
        let shared = Arc::clone(&self.shared);
        let active_run = Arc::clone(&self.active_run);
        let profile_name = profile.name.clone();
        let task_cancel = cancel.clone();

        let handle = tokio::spawn(async move {
            run_session(shared, task_cancel, profile_name, configuration).await;
            let mut active_guard = active_run.lock().await;
            active_guard.take();
        });

        *guard = Some(ActiveRun { cancel, handle });
        drop(guard);

        self.shared.publish().await;
        Ok(self.shared.current_snapshot())
    }

    pub(crate) async fn stop_run(
        &self,
        request: StopStressRunRequest,
    ) -> qsoripper_core::proto::qsoripper::services::StressRunSnapshot {
        let guard = self.active_run.lock().await;
        if let Some(active_run) = guard.as_ref() {
            active_run.cancel.cancel();
            let message = if request.force {
                "Force stop requested."
            } else {
                "Stop requested."
            };
            self.shared
                .with_state(|state| {
                    state.set_state(StressRunState::Stopping, message);
                    state.push_event(StressLogLevel::Info, message, None);
                })
                .await;
            self.shared.publish().await
        } else {
            self.shared.current_snapshot()
        }
    }
}

#[derive(Clone)]
pub(crate) struct StressControlSurface {
    controller: StressController,
}

impl StressControlSurface {
    pub(crate) fn new(controller: StressController) -> Self {
        Self { controller }
    }
}

#[tonic::async_trait]
impl StressControlService for StressControlSurface {
    type StreamStressRunEventsStream =
        ReceiverStream<Result<StreamStressRunEventsResponse, Status>>;

    async fn start_stress_run(
        &self,
        request: Request<StartStressRunRequest>,
    ) -> Result<Response<StartStressRunResponse>, Status> {
        Ok(Response::new(StartStressRunResponse {
            snapshot: Some(self.controller.start_run(request.into_inner()).await?),
        }))
    }

    async fn stop_stress_run(
        &self,
        request: Request<StopStressRunRequest>,
    ) -> Result<Response<StopStressRunResponse>, Status> {
        Ok(Response::new(StopStressRunResponse {
            snapshot: Some(self.controller.stop_run(request.into_inner()).await),
        }))
    }

    async fn get_stress_run_status(
        &self,
        _request: Request<GetStressRunStatusRequest>,
    ) -> Result<Response<GetStressRunStatusResponse>, Status> {
        Ok(Response::new(GetStressRunStatusResponse {
            snapshot: Some(self.controller.current_snapshot()),
        }))
    }

    async fn stream_stress_run_events(
        &self,
        request: Request<StreamStressRunEventsRequest>,
    ) -> Result<Response<Self::StreamStressRunEventsStream>, Status> {
        let include_current_snapshot = request.into_inner().include_current_snapshot;
        let mut subscription = self.controller.subscribe();
        let initial_snapshot = if include_current_snapshot {
            Some(self.controller.current_snapshot())
        } else {
            None
        };
        let (sender, receiver) = mpsc::channel(32);

        tokio::spawn(async move {
            if let Some(snapshot) = initial_snapshot {
                if sender
                    .send(Ok(StreamStressRunEventsResponse {
                        snapshot: Some(snapshot),
                    }))
                    .await
                    .is_err()
                {
                    return;
                }
            }

            loop {
                if subscription.changed().await.is_err() {
                    break;
                }

                let snapshot = subscription.borrow().clone();
                if sender
                    .send(Ok(StreamStressRunEventsResponse {
                        snapshot: Some(snapshot),
                    }))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(receiver)))
    }

    async fn list_stress_profiles(
        &self,
        _request: Request<ListStressProfilesRequest>,
    ) -> Result<Response<ListStressProfilesResponse>, Status> {
        Ok(Response::new(ListStressProfilesResponse {
            profiles: self.controller.list_profiles(),
        }))
    }
}

fn default_profiles() -> Vec<StressProfile> {
    vec![
        StressProfile {
            name: "long-haul".to_string(),
            description: "Runs core and gRPC vectors until stopped.".to_string(),
            configuration: Some(StressRunConfiguration {
                engine_endpoint: DEFAULT_STRESS_ENGINE_ENDPOINT.to_string(),
                duration_seconds: 0,
                grpc_parallelism: 24,
                metrics_interval_ms: 1000,
                recent_event_limit: 50,
                include_core_vectors: true,
                include_grpc_vectors: true,
                auto_start_engine: true,
            }),
        },
        StressProfile {
            name: "quick-smoke".to_string(),
            description: "Runs a shorter smoke pass with lighter gRPC pressure.".to_string(),
            configuration: Some(StressRunConfiguration {
                engine_endpoint: DEFAULT_STRESS_ENGINE_ENDPOINT.to_string(),
                duration_seconds: 30,
                grpc_parallelism: 8,
                metrics_interval_ms: 1000,
                recent_event_limit: 30,
                include_core_vectors: true,
                include_grpc_vectors: true,
                auto_start_engine: true,
            }),
        },
    ]
}

#[expect(
    clippy::result_large_err,
    reason = "The tonic service surface already uses `tonic::Status`, so keeping this helper aligned avoids extra conversions."
)]
fn resolve_profile(
    profiles: &[StressProfile],
    requested_name: &str,
) -> Result<StressProfile, Status> {
    if requested_name.is_empty() {
        return profiles
            .first()
            .cloned()
            .ok_or_else(|| Status::internal("No built-in stress profiles are available."));
    }

    profiles
        .iter()
        .find(|profile| profile.name == requested_name)
        .cloned()
        .ok_or_else(|| Status::not_found(format!("Unknown stress profile '{requested_name}'.")))
}

fn merge_configuration(
    profile: &StressProfile,
    request_configuration: Option<StressRunConfiguration>,
) -> StressRunConfiguration {
    let mut configuration = profile.configuration.clone().unwrap_or_default();
    if let Some(request_configuration) = request_configuration {
        if !request_configuration.engine_endpoint.is_empty() {
            configuration.engine_endpoint = request_configuration.engine_endpoint;
        }
        if request_configuration.duration_seconds > 0 {
            configuration.duration_seconds = request_configuration.duration_seconds;
        }
        if request_configuration.grpc_parallelism > 0 {
            configuration.grpc_parallelism = request_configuration.grpc_parallelism;
        }
        if request_configuration.metrics_interval_ms > 0 {
            configuration.metrics_interval_ms = request_configuration.metrics_interval_ms;
        }
        if request_configuration.recent_event_limit > 0 {
            configuration.recent_event_limit = request_configuration.recent_event_limit;
        }
        if request_configuration.include_core_vectors {
            configuration.include_core_vectors = true;
        }
        if request_configuration.include_grpc_vectors {
            configuration.include_grpc_vectors = true;
        }
        if request_configuration.auto_start_engine {
            configuration.auto_start_engine = true;
        }
    }

    configuration
}

#[cfg(test)]
mod tests {
    use super::{
        default_profiles, merge_configuration, resolve_profile, DEFAULT_STRESS_ENGINE_ENDPOINT,
    };
    use qsoripper_core::proto::qsoripper::services::StressRunConfiguration;
    use tonic::Code;

    #[expect(
        clippy::panic,
        reason = "These unit tests intentionally panic on broken profile fixture behavior."
    )]
    #[test]
    fn resolve_profile_defaults_to_first_profile_when_name_is_blank() {
        let profiles = default_profiles();

        let resolved = match resolve_profile(&profiles, "") {
            Ok(resolved) => resolved,
            Err(error) => panic!("default profile lookup failed: {error}"),
        };

        assert_eq!("long-haul", resolved.name);
    }

    #[expect(
        clippy::panic,
        reason = "These unit tests intentionally panic on broken profile fixture behavior."
    )]
    #[test]
    fn resolve_profile_returns_not_found_for_unknown_name() {
        let profiles = default_profiles();

        let error = match resolve_profile(&profiles, "missing") {
            Ok(profile) => panic!("unexpected profile: {}", profile.name),
            Err(error) => error,
        };

        assert_eq!(Code::NotFound, error.code());
    }

    #[expect(
        clippy::panic,
        reason = "These unit tests intentionally panic on broken profile fixture behavior."
    )]
    #[test]
    fn merge_configuration_prefers_non_default_request_values() {
        let Some(profile) = default_profiles()
            .into_iter()
            .find(|candidate| candidate.name == "quick-smoke")
        else {
            panic!("quick-smoke profile missing");
        };

        let merged = merge_configuration(
            &profile,
            Some(StressRunConfiguration {
                engine_endpoint: "http://127.0.0.1:55001".to_string(),
                duration_seconds: 90,
                grpc_parallelism: 16,
                metrics_interval_ms: 500,
                recent_event_limit: 99,
                include_core_vectors: true,
                include_grpc_vectors: true,
                auto_start_engine: true,
            }),
        );

        assert_eq!("http://127.0.0.1:55001", merged.engine_endpoint);
        assert_eq!(90, merged.duration_seconds);
        assert_eq!(16, merged.grpc_parallelism);
        assert_eq!(500, merged.metrics_interval_ms);
        assert_eq!(99, merged.recent_event_limit);
        assert!(merged.include_core_vectors);
        assert!(merged.include_grpc_vectors);
        assert!(merged.auto_start_engine);
    }

    #[test]
    fn default_profiles_use_isolated_engine_endpoint() {
        let profiles = default_profiles();

        assert!(profiles.iter().all(|profile| {
            profile.configuration.as_ref().is_some_and(|configuration| {
                configuration.engine_endpoint == DEFAULT_STRESS_ENGINE_ENDPOINT
            })
        }));
    }
}

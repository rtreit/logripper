//! Runnable tonic gRPC host for the `QsoRipper` Rust engine.

mod runtime_config;
mod setup;
mod station_profile_support;

use std::{fs, future::Future, io, net::SocketAddr, path::PathBuf, sync::Arc};

use qsoripper_core::adif::{parse_adi_qsos, serialize_adi_qsos};
use qsoripper_core::application::logbook::LogbookError;
use qsoripper_core::lookup::QRZ_USER_AGENT_ENV_VAR;
use qsoripper_core::storage::{EngineStorage, QsoListQuery, QsoSortOrder, StorageError};
use qsoripper_storage_memory::MemoryStorage;
use qsoripper_storage_sqlite::SqliteStorageBuilder;
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use qsoripper_core::proto::qsoripper::domain::{Band, Mode};
use qsoripper_core::proto::qsoripper::services::{
    developer_control_service_server::{DeveloperControlService, DeveloperControlServiceServer},
    logbook_service_server::{LogbookService, LogbookServiceServer},
    lookup_service_server::{LookupService, LookupServiceServer},
    setup_service_server::SetupServiceServer,
    station_profile_service_server::StationProfileServiceServer,
    AdifChunk, ApplyRuntimeConfigRequest, ApplyRuntimeConfigResponse, BatchLookupRequest,
    BatchLookupResponse, DeleteQsoRequest, DeleteQsoResponse, ExportAdifRequest,
    ExportAdifResponse, GetCachedCallsignRequest, GetCachedCallsignResponse, GetDxccEntityRequest,
    GetDxccEntityResponse, GetQsoRequest, GetQsoResponse, GetRuntimeConfigRequest,
    GetRuntimeConfigResponse, GetSyncStatusRequest, GetSyncStatusResponse, ImportAdifRequest,
    ImportAdifResponse, ListQsosRequest, ListQsosResponse, LogQsoRequest, LogQsoResponse,
    LookupRequest, LookupResponse, QsoSortOrder as ProtoQsoSortOrder, ResetRuntimeConfigRequest,
    ResetRuntimeConfigResponse, StreamLookupRequest, StreamLookupResponse, SyncWithQrzRequest,
    SyncWithQrzResponse, UpdateQsoRequest, UpdateQsoResponse,
};
use runtime_config::RuntimeConfigManager;
use setup::{
    default_config_path, SetupControlSurface, SetupState, StationProfileControlSurface,
    CONFIG_PATH_ENV_VAR,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_dotenv_if_present();
    let options = ServerOptions::from_env_and_args(std::env::args().skip(1))?;
    run_server(options, tokio::signal::ctrl_c()).await
}

async fn run_server<ShutdownSignal>(
    options: ServerOptions,
    shutdown_signal: ShutdownSignal,
) -> Result<(), Box<dyn std::error::Error>>
where
    ShutdownSignal: Future<Output = std::io::Result<()>>,
{
    let address = options.listen_address;
    let setup_state = Arc::new(SetupState::load(options.config_path.clone())?);
    let config_file_values = setup_state.runtime_config_values().await;
    let runtime_config = Arc::new(
        RuntimeConfigManager::new_with_config_file_values_and_cli_storage_overrides(
            config_file_values,
            &options.runtime_config_cli_storage_overrides(),
        )?,
    );
    let logbook_service = DeveloperLogbookService::new(runtime_config.clone());
    let lookup_service = DeveloperLookupService::new(runtime_config.clone());
    let developer_control_service = DeveloperControlSurface::new(runtime_config.clone());
    let setup_service = SetupControlSurface::new(setup_state.clone(), runtime_config.clone());
    let station_profile_service =
        StationProfileControlSurface::new(setup_state.clone(), runtime_config.clone());
    let active_storage_backend = runtime_config.active_storage_backend().await;
    let setup_status = setup_state.status().await;
    let setup_completion = setup_completion_label(setup_status.setup_complete);
    let config_path = setup_status.config_path.clone();

    println!(
        "{}",
        server_starting_message(
            address,
            active_storage_backend.as_str(),
            setup_completion,
            config_path.as_str(),
        )
    );

    let listener = tokio::net::TcpListener::bind(address).await?;
    let bound_address = listener.local_addr()?;

    println!(
        "{}",
        server_ready_message(
            bound_address,
            active_storage_backend.as_str(),
            setup_completion,
            config_path.as_str(),
        )
    );

    let (shutdown_sender, shutdown_receiver) = tokio::sync::oneshot::channel::<()>();
    let server = Server::builder()
        .add_service(LogbookServiceServer::new(logbook_service))
        .add_service(LookupServiceServer::new(lookup_service))
        .add_service(SetupServiceServer::new(setup_service))
        .add_service(StationProfileServiceServer::new(station_profile_service))
        .add_service(DeveloperControlServiceServer::new(
            developer_control_service,
        ))
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
            let _ = shutdown_receiver.await;
        });
    tokio::pin!(server);
    tokio::pin!(shutdown_signal);

    tokio::select! {
        result = &mut server => {
            result?;
        }
        signal_result = &mut shutdown_signal => {
            signal_result?;
            println!("Shutting down.");
            let _ = shutdown_sender.send(());
            server.await?;
        }
    }

    Ok(())
}

fn setup_completion_label(setup_complete: bool) -> &'static str {
    if setup_complete {
        "complete"
    } else {
        "incomplete"
    }
}

fn server_starting_message(
    address: SocketAddr,
    active_storage_backend: &str,
    setup_completion: &str,
    config_path: &str,
) -> String {
    format!(
        "Starting QsoRipper gRPC server on {address} using {active_storage_backend} storage (setup: {setup_completion}, config: {config_path})"
    )
}

fn server_ready_message(
    address: SocketAddr,
    active_storage_backend: &str,
    setup_completion: &str,
    config_path: &str,
) -> String {
    format!(
        "QsoRipper gRPC server ready on {address} using {active_storage_backend} storage (setup: {setup_completion}, config: {config_path})"
    )
}

fn load_dotenv_if_present() {
    match dotenvy::dotenv() {
        Ok(path) => println!("Loaded config from {}", path.display()),
        Err(dotenvy::Error::Io(error)) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            if let Some(path) = load_dotenv_with_legacy_qrz_user_agent_compatibility(&error) {
                eprintln!(
                    "Warning: loaded {} after auto-correcting a legacy unquoted {} value; quote that value in .env to remove this warning.",
                    path.display(),
                    QRZ_USER_AGENT_ENV_VAR
                );
                println!("Loaded config from {}", path.display());
            } else {
                eprintln!("Warning: failed to parse .env file: {error}");
            }
        }
    }
}

fn load_dotenv_with_legacy_qrz_user_agent_compatibility(error: &dotenvy::Error) -> Option<PathBuf> {
    let dotenvy::Error::LineParse(_, _) = error else {
        return None;
    };
    let path = find_dotenv_path().ok()??;
    let contents = fs::read_to_string(&path).ok()?;
    let compatible_contents = sanitize_legacy_qrz_user_agent_contents(&contents)?;
    dotenvy::from_read(std::io::Cursor::new(compatible_contents)).ok()?;
    Some(path)
}

fn legacy_qrz_user_agent_line_compatibility(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let (key, raw_value) = trimmed.split_once('=')?;

    if key.trim() != QRZ_USER_AGENT_ENV_VAR {
        return None;
    }

    let value = raw_value.trim();
    if value.is_empty()
        || value.contains('#')
        || matches!(value.chars().next(), Some('"' | '\''))
        || !(value.chars().any(char::is_whitespace) || value.contains('(') || value.contains(')'))
    {
        return None;
    }

    let leading_whitespace = &line[..line.len() - trimmed.len()];
    Some(format!(
        r#"{leading_whitespace}{QRZ_USER_AGENT_ENV_VAR}="{}""#,
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

fn sanitize_legacy_qrz_user_agent_contents(contents: &str) -> Option<String> {
    let mut changed = false;
    let mut compatible_contents = String::with_capacity(contents.len());

    for segment in contents.split_inclusive('\n') {
        let (line, newline) = if let Some(line) = segment.strip_suffix("\r\n") {
            (line, "\r\n")
        } else if let Some(line) = segment.strip_suffix('\n') {
            (line, "\n")
        } else {
            (segment, "")
        };

        if let Some(compatible_line) = legacy_qrz_user_agent_line_compatibility(line) {
            compatible_contents.push_str(&compatible_line);
            changed = true;
        } else {
            compatible_contents.push_str(line);
        }

        compatible_contents.push_str(newline);
    }

    changed.then_some(compatible_contents)
}

fn find_dotenv_path() -> io::Result<Option<PathBuf>> {
    let current_dir = std::env::current_dir()?;
    Ok(current_dir
        .ancestors()
        .map(|directory| directory.join(".env"))
        .find(|candidate| candidate.is_file()))
}

#[derive(Clone)]
struct DeveloperLogbookService {
    runtime_config: Arc<RuntimeConfigManager>,
}

impl DeveloperLogbookService {
    fn new(runtime_config: Arc<RuntimeConfigManager>) -> Self {
        Self { runtime_config }
    }
}

#[tonic::async_trait]
impl LogbookService for DeveloperLogbookService {
    type ListQsosStream = ReceiverStream<Result<ListQsosResponse, Status>>;
    type SyncWithQrzStream = ReceiverStream<Result<SyncWithQrzResponse, Status>>;
    type ExportAdifStream = ReceiverStream<Result<ExportAdifResponse, Status>>;

    async fn log_qso(
        &self,
        request: Request<LogQsoRequest>,
    ) -> Result<Response<LogQsoResponse>, Status> {
        let (engine, active_station_profile) = self.runtime_config.logbook_context().await;
        let request = request.into_inner();
        let qso = request
            .qso
            .ok_or_else(|| Status::invalid_argument("LogQso requires a qso payload."))?;
        let stored = engine
            .log_qso_with_station_profile(qso, active_station_profile.as_ref())
            .await
            .map_err(map_logbook_error)?;
        let (sync_success, sync_error) = sync_result(request.sync_to_qrz, "QRZ sync");

        Ok(Response::new(LogQsoResponse {
            local_id: stored.local_id,
            qrz_logid: stored.qrz_logid,
            sync_success,
            sync_error,
        }))
    }

    async fn update_qso(
        &self,
        request: Request<UpdateQsoRequest>,
    ) -> Result<Response<UpdateQsoResponse>, Status> {
        let engine = self.runtime_config.logbook_engine().await;
        let request = request.into_inner();
        let qso = request
            .qso
            .ok_or_else(|| Status::invalid_argument("UpdateQso requires a qso payload."))?;
        let _ = engine.update_qso(qso).await.map_err(map_logbook_error)?;
        let (sync_success, sync_error) = sync_result(request.sync_to_qrz, "QRZ sync");

        Ok(Response::new(UpdateQsoResponse {
            success: true,
            error: None,
            sync_success,
            sync_error,
        }))
    }

    async fn delete_qso(
        &self,
        request: Request<DeleteQsoRequest>,
    ) -> Result<Response<DeleteQsoResponse>, Status> {
        let engine = self.runtime_config.logbook_engine().await;
        let request = request.into_inner();
        engine
            .delete_qso(&request.local_id)
            .await
            .map_err(map_logbook_error)?;
        let (qrz_delete_success, qrz_delete_error) =
            sync_result(request.delete_from_qrz, "QRZ delete");

        Ok(Response::new(DeleteQsoResponse {
            success: true,
            error: None,
            qrz_delete_success,
            qrz_delete_error,
        }))
    }

    async fn get_qso(
        &self,
        request: Request<GetQsoRequest>,
    ) -> Result<Response<GetQsoResponse>, Status> {
        let engine = self.runtime_config.logbook_engine().await;
        let request = request.into_inner();
        let qso = engine
            .get_qso(&request.local_id)
            .await
            .map_err(map_logbook_error)?;

        Ok(Response::new(GetQsoResponse { qso: Some(qso) }))
    }

    async fn list_qsos(
        &self,
        request: Request<ListQsosRequest>,
    ) -> Result<Response<Self::ListQsosStream>, Status> {
        let engine = self.runtime_config.logbook_engine().await;
        let request = request.into_inner();
        let query = qso_list_query_from_request(&request).map_err(|status| *status)?;
        let records = engine.list_qsos(&query).await.map_err(map_logbook_error)?;
        let (sender, receiver) = tokio::sync::mpsc::channel(records.len().max(1));

        for record in records {
            if sender
                .send(Ok(ListQsosResponse { qso: Some(record) }))
                .await
                .is_err()
            {
                break;
            }
        }

        Ok(Response::new(ReceiverStream::new(receiver)))
    }

    async fn sync_with_qrz(
        &self,
        _request: Request<SyncWithQrzRequest>,
    ) -> Result<Response<Self::SyncWithQrzStream>, Status> {
        Err(Status::unimplemented("SyncWithQrz is not implemented yet."))
    }

    async fn get_sync_status(
        &self,
        _request: Request<GetSyncStatusRequest>,
    ) -> Result<Response<GetSyncStatusResponse>, Status> {
        let sync_status = self
            .runtime_config
            .logbook_engine()
            .await
            .get_sync_status()
            .await
            .map_err(map_logbook_error)?;

        Ok(Response::new(GetSyncStatusResponse {
            local_qso_count: sync_status.local_qso_count,
            qrz_qso_count: sync_status.qrz_qso_count,
            pending_upload: sync_status.pending_upload,
            last_sync: sync_status.last_sync,
            qrz_logbook_owner: sync_status.qrz_logbook_owner,
        }))
    }

    async fn import_adif(
        &self,
        request: Request<tonic::Streaming<ImportAdifRequest>>,
    ) -> Result<Response<ImportAdifResponse>, Status> {
        let (engine, active_station_profile) = self.runtime_config.logbook_context().await;
        let mut stream = request.into_inner();
        let mut adif_bytes = Vec::new();

        while let Some(chunk) = stream.message().await? {
            let chunk = chunk
                .chunk
                .ok_or_else(|| Status::invalid_argument("ImportAdifRequest requires a chunk."))?;
            adif_bytes.extend_from_slice(&chunk.data);
        }

        let qsos = parse_adi_qsos(&adif_bytes)
            .await
            .map_err(Status::invalid_argument)?;
        let summary = engine
            .import_adif_qsos(qsos, active_station_profile.as_ref())
            .await
            .map_err(map_logbook_error)?;

        Ok(Response::new(ImportAdifResponse {
            records_imported: summary.records_imported,
            records_skipped: summary.records_skipped,
            warnings: summary.warnings,
        }))
    }

    async fn export_adif(
        &self,
        request: Request<ExportAdifRequest>,
    ) -> Result<Response<Self::ExportAdifStream>, Status> {
        let engine = self.runtime_config.logbook_engine().await;
        let request = request.into_inner();
        let query = export_qso_list_query_from_request(&request);
        let qsos = engine.list_qsos(&query).await.map_err(map_logbook_error)?;
        let adif_bytes = serialize_adi_qsos(&qsos, request.include_header);
        let chunk_count = adif_bytes.len().div_ceil(ADIF_CHUNK_SIZE).max(1);
        let (sender, receiver) = tokio::sync::mpsc::channel(chunk_count);

        for chunk in adif_bytes.chunks(ADIF_CHUNK_SIZE) {
            if sender
                .send(Ok(ExportAdifResponse {
                    chunk: Some(AdifChunk {
                        data: chunk.to_vec(),
                    }),
                }))
                .await
                .is_err()
            {
                break;
            }
        }

        Ok(Response::new(ReceiverStream::new(receiver)))
    }
}

#[derive(Clone)]
struct DeveloperLookupService {
    runtime_config: Arc<RuntimeConfigManager>,
}

impl DeveloperLookupService {
    fn new(runtime_config: Arc<RuntimeConfigManager>) -> Self {
        Self { runtime_config }
    }
}

#[tonic::async_trait]
impl LookupService for DeveloperLookupService {
    type StreamLookupStream = ReceiverStream<Result<StreamLookupResponse, Status>>;

    async fn lookup(
        &self,
        request: Request<LookupRequest>,
    ) -> Result<Response<LookupResponse>, Status> {
        let coordinator = self.runtime_config.lookup_coordinator().await;
        let request = request.into_inner();
        let result = coordinator
            .lookup(&request.callsign, request.skip_cache)
            .await;
        Ok(Response::new(LookupResponse {
            result: Some(result),
        }))
    }

    async fn stream_lookup(
        &self,
        request: Request<StreamLookupRequest>,
    ) -> Result<Response<Self::StreamLookupStream>, Status> {
        let coordinator = self.runtime_config.lookup_coordinator().await;
        let request = request.into_inner();
        let updates = coordinator
            .stream_lookup(&request.callsign, request.skip_cache)
            .await;
        let (sender, receiver) = tokio::sync::mpsc::channel(8);

        for update in updates {
            if sender
                .send(Ok(StreamLookupResponse {
                    result: Some(update),
                }))
                .await
                .is_err()
            {
                break;
            }
        }

        Ok(Response::new(ReceiverStream::new(receiver)))
    }

    async fn get_cached_callsign(
        &self,
        request: Request<GetCachedCallsignRequest>,
    ) -> Result<Response<GetCachedCallsignResponse>, Status> {
        let coordinator = self.runtime_config.lookup_coordinator().await;
        let request = request.into_inner();
        let result = coordinator.get_cached_callsign(&request.callsign).await;
        Ok(Response::new(GetCachedCallsignResponse {
            result: Some(result),
        }))
    }

    async fn get_dxcc_entity(
        &self,
        _request: Request<GetDxccEntityRequest>,
    ) -> Result<Response<GetDxccEntityResponse>, Status> {
        Err(Status::unimplemented(
            "GetDxccEntity is out of scope for the first lookup slice.",
        ))
    }

    async fn batch_lookup(
        &self,
        _request: Request<BatchLookupRequest>,
    ) -> Result<Response<BatchLookupResponse>, Status> {
        Err(Status::unimplemented(
            "BatchLookup is out of scope for the first lookup slice.",
        ))
    }
}

#[derive(Clone)]
struct DeveloperControlSurface {
    runtime_config: Arc<RuntimeConfigManager>,
}

impl DeveloperControlSurface {
    fn new(runtime_config: Arc<RuntimeConfigManager>) -> Self {
        Self { runtime_config }
    }
}

#[tonic::async_trait]
impl DeveloperControlService for DeveloperControlSurface {
    async fn get_runtime_config(
        &self,
        _request: Request<GetRuntimeConfigRequest>,
    ) -> Result<Response<GetRuntimeConfigResponse>, Status> {
        Ok(Response::new(GetRuntimeConfigResponse {
            snapshot: Some(self.runtime_config.snapshot().await),
        }))
    }

    async fn apply_runtime_config(
        &self,
        request: Request<ApplyRuntimeConfigRequest>,
    ) -> Result<Response<ApplyRuntimeConfigResponse>, Status> {
        let snapshot = self
            .runtime_config
            .apply_request(request.into_inner())
            .await
            .map_err(Status::invalid_argument)?;
        Ok(Response::new(ApplyRuntimeConfigResponse {
            snapshot: Some(snapshot),
        }))
    }

    async fn reset_runtime_config(
        &self,
        request: Request<ResetRuntimeConfigRequest>,
    ) -> Result<Response<ResetRuntimeConfigResponse>, Status> {
        let snapshot = self
            .runtime_config
            .reset_request(request.into_inner())
            .await
            .map_err(Status::invalid_argument)?;
        Ok(Response::new(ResetRuntimeConfigResponse {
            snapshot: Some(snapshot),
        }))
    }
}

#[derive(Debug, Clone)]
struct ServerOptions {
    listen_address: SocketAddr,
    config_path: PathBuf,
    #[cfg(test)]
    storage: StorageOptions,
    storage_cli_overrides: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StorageOptions {
    backend: StorageBackendKind,
    sqlite_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StorageBackendKind {
    Memory,
    Sqlite,
}

impl ServerOptions {
    fn from_env_and_args<I>(args: I) -> Result<Self, Box<dyn std::error::Error>>
    where
        I: IntoIterator<Item = String>,
    {
        let mut listen = std::env::var("QSORIPPER_SERVER_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:50051".to_string());
        let mut config_path = std::env::var(CONFIG_PATH_ENV_VAR)
            .map(PathBuf::from)
            .unwrap_or(default_config_path()?);
        let mut storage_backend = parse_storage_backend(
            &std::env::var("QSORIPPER_STORAGE_BACKEND").unwrap_or_else(|_| "memory".to_string()),
        )?;
        let mut sqlite_path = PathBuf::from(
            std::env::var("QSORIPPER_SQLITE_PATH").unwrap_or_else(|_| "qsoripper.db".to_string()),
        );
        let mut storage_cli_overrides = std::collections::BTreeMap::new();
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--listen" => {
                    let value = args.next().ok_or("Missing value for --listen")?;
                    listen = value;
                }
                "--config" => {
                    let value = args.next().ok_or("Missing value for --config")?;
                    config_path = PathBuf::from(value);
                }
                "--storage" => {
                    let value = args.next().ok_or("Missing value for --storage")?;
                    storage_backend = parse_storage_backend(&value)?;
                    storage_cli_overrides.insert(
                        runtime_config::STORAGE_BACKEND_ENV_VAR.to_string(),
                        match storage_backend {
                            StorageBackendKind::Memory => "memory".to_string(),
                            StorageBackendKind::Sqlite => "sqlite".to_string(),
                        },
                    );
                }
                "--sqlite-path" => {
                    let value = args.next().ok_or("Missing value for --sqlite-path")?;
                    sqlite_path = PathBuf::from(value);
                    storage_cli_overrides.insert(
                        runtime_config::SQLITE_PATH_ENV_VAR.to_string(),
                        sqlite_path.display().to_string(),
                    );
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                _ => return Err(format!("Unknown argument: {arg}").into()),
            }
        }

        if storage_backend == StorageBackendKind::Sqlite
            && storage_cli_overrides.contains_key(runtime_config::STORAGE_BACKEND_ENV_VAR)
            && !storage_cli_overrides.contains_key(runtime_config::SQLITE_PATH_ENV_VAR)
        {
            storage_cli_overrides.insert(
                runtime_config::SQLITE_PATH_ENV_VAR.to_string(),
                sqlite_path.display().to_string(),
            );
        }

        Ok(Self {
            listen_address: listen.parse()?,
            config_path,
            #[cfg(test)]
            storage: StorageOptions {
                backend: storage_backend,
                sqlite_path,
            },
            storage_cli_overrides,
        })
    }

    fn runtime_config_cli_storage_overrides(&self) -> std::collections::BTreeMap<String, String> {
        self.storage_cli_overrides.clone()
    }
}

fn print_help() {
    println!(
        "QsoRipper gRPC server\n\nUsage:\n  cargo run -p qsoripper-server -- [--listen 127.0.0.1:50051] [--config path\\to\\config.toml] [--storage memory|sqlite] [--sqlite-path path\\to\\qsoripper.db]\n\nEnvironment:\n  QSORIPPER_SERVER_ADDR       Overrides the bind address\n  QSORIPPER_CONFIG_PATH       Overrides the persisted setup config path\n  QSORIPPER_STORAGE_BACKEND   Selects memory or sqlite storage (default: memory)\n  QSORIPPER_SQLITE_PATH       SQLite path when sqlite storage is selected (default: qsoripper.db)"
    );
}

fn build_storage(
    options: &StorageOptions,
) -> Result<Arc<dyn EngineStorage>, Box<dyn std::error::Error>> {
    let storage: Arc<dyn EngineStorage> = match options.backend {
        StorageBackendKind::Memory => Arc::new(MemoryStorage::new()),
        StorageBackendKind::Sqlite => Arc::new(
            SqliteStorageBuilder::new()
                .path(options.sqlite_path.clone())
                .build()?,
        ),
    };

    Ok(storage)
}

fn parse_storage_backend(value: &str) -> Result<StorageBackendKind, Box<dyn std::error::Error>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "memory" => Ok(StorageBackendKind::Memory),
        "sqlite" => Ok(StorageBackendKind::Sqlite),
        other => Err(format!("Unsupported storage backend: {other}").into()),
    }
}

fn map_logbook_error(error: LogbookError) -> Status {
    match error {
        LogbookError::Validation(message) => Status::invalid_argument(message),
        LogbookError::NotFound(local_id) => {
            Status::not_found(format!("QSO '{local_id}' was not found."))
        }
        LogbookError::Storage(StorageError::Duplicate { entity, key }) => {
            Status::already_exists(format!("{entity} '{key}' already exists."))
        }
        LogbookError::Storage(other) => Status::internal(other.to_string()),
    }
}

fn qso_list_query_from_request(request: &ListQsosRequest) -> Result<QsoListQuery, Box<Status>> {
    let band_filter = request
        .band_filter
        .map(|value| {
            Band::try_from(value)
                .map_err(|_| Box::new(Status::invalid_argument("Invalid band_filter value.")))
        })
        .transpose()?;
    let mode_filter = request
        .mode_filter
        .map(|value| {
            Mode::try_from(value)
                .map_err(|_| Box::new(Status::invalid_argument("Invalid mode_filter value.")))
        })
        .transpose()?;
    let sort = match ProtoQsoSortOrder::try_from(request.sort) {
        Ok(ProtoQsoSortOrder::NewestFirst) => QsoSortOrder::NewestFirst,
        Ok(ProtoQsoSortOrder::OldestFirst) => QsoSortOrder::OldestFirst,
        Err(_) => return Err(Box::new(Status::invalid_argument("Invalid sort order."))),
    };

    Ok(QsoListQuery {
        after: request.after,
        before: request.before,
        callsign_filter: request
            .callsign_filter
            .as_deref()
            .and_then(non_empty_string),
        band_filter,
        mode_filter,
        contest_id: request.contest_id.as_deref().and_then(non_empty_string),
        limit: (request.limit > 0).then_some(request.limit),
        offset: request.offset,
        sort,
    })
}

const ADIF_CHUNK_SIZE: usize = 16 * 1024;

fn export_qso_list_query_from_request(request: &ExportAdifRequest) -> QsoListQuery {
    QsoListQuery {
        after: request.after,
        before: request.before,
        contest_id: request.contest_id.as_deref().and_then(non_empty_string),
        sort: QsoSortOrder::OldestFirst,
        ..QsoListQuery::default()
    }
}

fn non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn sync_result(requested: bool, label: &str) -> (bool, Option<String>) {
    if requested {
        (false, Some(format!("{label} is not implemented yet.")))
    } else {
        (true, None)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Duration;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        build_storage, legacy_qrz_user_agent_line_compatibility, load_dotenv_if_present,
        parse_storage_backend, run_server, sanitize_legacy_qrz_user_agent_contents,
        server_ready_message, server_starting_message, DeveloperLogbookService,
        DeveloperLookupService, Server, ServerOptions, StorageBackendKind, StorageOptions,
    };
    use crate::runtime_config::{
        RuntimeConfigManager, SQLITE_PATH_ENV_VAR, STATION_CALLSIGN_ENV_VAR, STATION_GRID_ENV_VAR,
        STATION_OPERATOR_CALLSIGN_ENV_VAR, STATION_PROFILE_NAME_ENV_VAR, STORAGE_BACKEND_ENV_VAR,
    };
    use prost_types::Timestamp;
    use qsoripper_core::lookup::{
        QRZ_USER_AGENT_ENV_VAR, QRZ_XML_PASSWORD_ENV_VAR, QRZ_XML_USERNAME_ENV_VAR,
    };
    use qsoripper_core::proto::qsoripper::domain::{
        Band, LookupResult, LookupState, Mode, QsoRecord,
    };
    use qsoripper_core::proto::qsoripper::services::{
        get_dxcc_entity_request,
        logbook_service_client::LogbookServiceClient,
        logbook_service_server::{LogbookService, LogbookServiceServer},
        lookup_service_server::LookupService,
        AdifChunk, BatchLookupRequest, DeleteQsoRequest, ExportAdifRequest,
        GetCachedCallsignRequest, GetDxccEntityRequest, GetQsoRequest, GetSyncStatusRequest,
        ImportAdifRequest, ImportAdifResponse, ListQsosRequest, LogQsoRequest, LookupRequest,
        QsoSortOrder, StreamLookupRequest, UpdateQsoRequest,
    };
    use tokio_stream::StreamExt;
    use tonic::transport::Channel;
    use tonic::{Code, Request};

    static PROCESS_STATE_LOCK: Mutex<()> = Mutex::new(());

    const PROCESS_ENV_KEYS: [&str; 4] = [
        "QSORIPPER_SERVER_ADDR",
        "QSORIPPER_CONFIG_PATH",
        "QSORIPPER_STORAGE_BACKEND",
        "QSORIPPER_SQLITE_PATH",
    ];

    struct ProcessStateGuard {
        original_dir: PathBuf,
        original_env: Vec<(&'static str, Option<String>)>,
    }

    impl ProcessStateGuard {
        fn capture() -> Self {
            Self {
                original_dir: std::env::current_dir().expect("current working directory"),
                original_env: PROCESS_ENV_KEYS
                    .into_iter()
                    .map(|key| (key, std::env::var(key).ok()))
                    .collect(),
            }
        }

        fn restore_current_dir(&self) {
            std::env::set_current_dir(&self.original_dir).expect("restore current directory");
        }
    }

    impl Drop for ProcessStateGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original_dir);
            for (key, value) in &self.original_env {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    fn capture_clean_process_state() -> (std::sync::MutexGuard<'static, ()>, ProcessStateGuard) {
        let process_state_lock = PROCESS_STATE_LOCK.lock().expect("lock process state");
        let process_state = ProcessStateGuard::capture();

        for key in PROCESS_ENV_KEYS {
            std::env::remove_var(key);
        }

        (process_state_lock, process_state)
    }

    fn test_lookup_service() -> DeveloperLookupService {
        DeveloperLookupService::new(test_runtime_config())
    }

    fn test_runtime_config() -> Arc<RuntimeConfigManager> {
        Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime config"))
    }

    fn test_runtime_config_with_logbook(
        backend: StorageBackendKind,
        sqlite_path: Option<&std::path::Path>,
        include_active_station: bool,
    ) -> Arc<RuntimeConfigManager> {
        let mut startup_values = BTreeMap::new();
        startup_values.insert(
            STORAGE_BACKEND_ENV_VAR.to_string(),
            match backend {
                StorageBackendKind::Memory => "memory".to_string(),
                StorageBackendKind::Sqlite => "sqlite".to_string(),
            },
        );

        if let Some(path) = sqlite_path {
            startup_values.insert(
                SQLITE_PATH_ENV_VAR.to_string(),
                path.to_string_lossy().into_owned(),
            );
        }

        if include_active_station {
            startup_values.insert(STATION_PROFILE_NAME_ENV_VAR.to_string(), "Home".to_string());
            startup_values.insert(STATION_CALLSIGN_ENV_VAR.to_string(), "K7RND".to_string());
            startup_values.insert(
                STATION_OPERATOR_CALLSIGN_ENV_VAR.to_string(),
                "K7RND".to_string(),
            );
            startup_values.insert(STATION_GRID_ENV_VAR.to_string(), "CN87".to_string());
        }

        Arc::new(RuntimeConfigManager::new(startup_values).expect("runtime config"))
    }

    fn unique_sqlite_test_path(label: &str) -> PathBuf {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "qsoripper-{label}-{}-{unique_suffix}.db",
            std::process::id()
        ))
    }

    fn sample_qso_without_station_callsign(worked_callsign: &str) -> QsoRecord {
        QsoRecord {
            worked_callsign: worked_callsign.to_string(),
            utc_timestamp: Some(Timestamp {
                seconds: 1_731_600_000,
                nanos: 0,
            }),
            band: Band::Band20m as i32,
            mode: Mode::Ssb as i32,
            notes: Some("Logged from gRPC test".to_string()),
            ..QsoRecord::default()
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "This test helper exercises the full CRUD flow end-to-end in one place."
    )]
    async fn exercise_logbook_crud_flow(service: &DeveloperLogbookService) {
        let log_response = LogbookService::log_qso(
            service,
            Request::new(LogQsoRequest {
                qso: Some(sample_qso_without_station_callsign("W1AW")),
                sync_to_qrz: false,
            }),
        )
        .await
        .expect("log response")
        .into_inner();

        assert!(log_response.sync_success);
        assert!(log_response.sync_error.is_none());

        let loaded = LogbookService::get_qso(
            service,
            Request::new(GetQsoRequest {
                local_id: log_response.local_id.clone(),
            }),
        )
        .await
        .expect("get response")
        .into_inner()
        .qso
        .expect("loaded qso");

        assert_eq!("K7RND", loaded.station_callsign);
        let loaded_snapshot = loaded
            .station_snapshot
            .as_ref()
            .expect("station snapshot should be materialized");
        assert_eq!("K7RND", loaded_snapshot.station_callsign);
        assert_eq!(Some("Home"), loaded_snapshot.profile_name.as_deref());
        assert_eq!(Some("CN87"), loaded_snapshot.grid.as_deref());

        let listed = LogbookService::list_qsos(
            service,
            Request::new(ListQsosRequest {
                callsign_filter: Some("W1AW".to_string()),
                limit: 10,
                sort: QsoSortOrder::NewestFirst as i32,
                ..ListQsosRequest::default()
            }),
        )
        .await
        .expect("list response")
        .into_inner()
        .map(|result| result.expect("list item").qso.expect("listed qso payload"))
        .collect::<Vec<_>>()
        .await;

        assert_eq!(1, listed.len());
        assert_eq!(
            log_response.local_id,
            listed.first().expect("expected listed QSO").local_id
        );

        let mut updated = loaded.clone();
        updated.station_callsign.clear();
        updated.station_snapshot = None;
        updated.notes = Some("Updated through gRPC".to_string());
        let update_response = LogbookService::update_qso(
            service,
            Request::new(UpdateQsoRequest {
                qso: Some(updated),
                sync_to_qrz: false,
            }),
        )
        .await
        .expect("update response")
        .into_inner();

        assert!(update_response.success);
        assert!(update_response.sync_success);
        assert!(update_response.sync_error.is_none());

        let reloaded = LogbookService::get_qso(
            service,
            Request::new(GetQsoRequest {
                local_id: log_response.local_id.clone(),
            }),
        )
        .await
        .expect("reload response")
        .into_inner()
        .qso
        .expect("reloaded qso");

        assert_eq!("K7RND", reloaded.station_callsign);
        assert_eq!(Some("Updated through gRPC"), reloaded.notes.as_deref());
        assert_eq!(
            Some("Home"),
            reloaded
                .station_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.profile_name.as_deref())
        );

        let sync_status =
            LogbookService::get_sync_status(service, Request::new(GetSyncStatusRequest {}))
                .await
                .expect("sync status response")
                .into_inner();

        assert_eq!(1, sync_status.local_qso_count);
        assert_eq!(1, sync_status.pending_upload);
        assert_eq!(0, sync_status.qrz_qso_count);
        assert!(sync_status.last_sync.is_none());
        assert!(sync_status.qrz_logbook_owner.is_none());

        let delete_response = LogbookService::delete_qso(
            service,
            Request::new(DeleteQsoRequest {
                local_id: log_response.local_id.clone(),
                delete_from_qrz: false,
            }),
        )
        .await
        .expect("delete response")
        .into_inner();

        assert!(delete_response.success);
        assert!(delete_response.qrz_delete_success);
        assert!(delete_response.qrz_delete_error.is_none());

        let get_error = LogbookService::get_qso(
            service,
            Request::new(GetQsoRequest {
                local_id: log_response.local_id,
            }),
        )
        .await
        .expect_err("deleted record should not load");
        assert_eq!(Code::NotFound, get_error.code());
    }

    async fn grpc_logbook_client(
        service: DeveloperLogbookService,
    ) -> (LogbookServiceClient<Channel>, tokio::task::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("listener address");
        drop(listener);

        let handle = tokio::spawn(async move {
            Server::builder()
                .add_service(LogbookServiceServer::new(service))
                .serve(address)
                .await
                .expect("serve test gRPC server");
        });

        let endpoint = format!("http://{address}");
        for attempt in 0..20 {
            match LogbookServiceClient::connect(endpoint.clone()).await {
                Ok(client) => return (client, handle),
                Err(error) => {
                    assert!(
                        attempt < 19,
                        "failed to connect to test gRPC server at {endpoint}: {error}"
                    );
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        }
        unreachable!("connection loop should have returned or asserted");
    }

    async fn import_adif_payload(
        client: &mut LogbookServiceClient<Channel>,
        chunks: Vec<Vec<u8>>,
    ) -> ImportAdifResponse {
        let stream = tokio_stream::iter(chunks.into_iter().map(|chunk| ImportAdifRequest {
            chunk: Some(AdifChunk { data: chunk }),
        }));

        client
            .import_adif(Request::new(stream))
            .await
            .expect("import response")
            .into_inner()
    }

    #[test]
    fn load_dotenv_if_present_reads_env_from_current_directory() {
        let (_process_state_lock, process_state) = capture_clean_process_state();

        let temp_dir = std::env::temp_dir().join(format!(
            "qsoripper-dotenv-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let env_path = temp_dir.join(".env");
        fs::write(
            &env_path,
            concat!(
                "QSORIPPER_SERVER_ADDR=127.0.0.1:61051\n",
                "QSORIPPER_STORAGE_BACKEND=sqlite\n",
                "QSORIPPER_SQLITE_PATH=data/test-qsoripper.db\n"
            ),
        )
        .expect("write temp .env");

        std::env::set_current_dir(&temp_dir).expect("switch to temp dir");
        load_dotenv_if_present();

        let options = ServerOptions::from_env_and_args(Vec::<String>::new()).unwrap();

        assert_eq!("127.0.0.1:61051", options.listen_address.to_string());
        assert_eq!(StorageBackendKind::Sqlite, options.storage.backend);
        assert_eq!(
            PathBuf::from("data/test-qsoripper.db"),
            options.storage.sqlite_path
        );

        process_state.restore_current_dir();
        fs::remove_file(env_path).expect("remove temp .env");
        fs::remove_dir_all(temp_dir).expect("remove temp dir");
    }

    #[test]
    fn legacy_qrz_user_agent_compatibility_rewrites_and_preserves_later_values() {
        let legacy_line = "QSORIPPER_QRZ_USER_AGENT=QsoRipper/0.1.0 (AE7XI)";
        assert_eq!(
            Some("QSORIPPER_QRZ_USER_AGENT=\"QsoRipper/0.1.0 (AE7XI)\"".to_string()),
            legacy_qrz_user_agent_line_compatibility(legacy_line)
        );

        let sanitized = sanitize_legacy_qrz_user_agent_contents(concat!(
            "QSORIPPER_QRZ_USER_AGENT=QsoRipper/0.1.0 (AE7XI)\n",
            "QSORIPPER_QRZ_XML_USERNAME=KC7AVA\n",
            "QSORIPPER_QRZ_XML_PASSWORD=test-password\n"
        ))
        .expect("compatibility rewrite");

        let entries: Vec<(String, String)> =
            dotenvy::from_read_iter(std::io::Cursor::new(sanitized))
                .collect::<Result<_, _>>()
                .expect("sanitized dotenv entries");

        assert_eq!(
            vec![
                (
                    QRZ_USER_AGENT_ENV_VAR.to_string(),
                    "QsoRipper/0.1.0 (AE7XI)".to_string()
                ),
                (QRZ_XML_USERNAME_ENV_VAR.to_string(), "KC7AVA".to_string()),
                (
                    QRZ_XML_PASSWORD_ENV_VAR.to_string(),
                    "test-password".to_string()
                ),
            ],
            entries
        );
    }

    #[test]
    fn server_starting_message_reports_pending_startup() {
        let message = server_starting_message(
            "127.0.0.1:50051".parse().expect("address"),
            "memory",
            "complete",
            "config.toml",
        );

        assert_eq!(
            "Starting QsoRipper gRPC server on 127.0.0.1:50051 using memory storage (setup: complete, config: config.toml)",
            message
        );
    }

    #[test]
    fn server_ready_message_confirms_bound_listener() {
        let message = server_ready_message(
            "127.0.0.1:50051".parse().expect("address"),
            "sqlite",
            "incomplete",
            "config.toml",
        );

        assert_eq!(
            "QsoRipper gRPC server ready on 127.0.0.1:50051 using sqlite storage (setup: incomplete, config: config.toml)",
            message
        );
    }

    #[test]
    fn server_options_default_to_localhost_port_50051() {
        let (_process_state_lock, _process_state) = capture_clean_process_state();

        let options = ServerOptions::from_env_and_args(Vec::<String>::new()).unwrap();

        assert_eq!("127.0.0.1:50051", options.listen_address.to_string());
        assert_eq!(options.storage.backend, StorageBackendKind::Memory);
        assert!(options.runtime_config_cli_storage_overrides().is_empty());
    }

    #[test]
    fn server_options_allow_explicit_listen_override() {
        let (_process_state_lock, _process_state) = capture_clean_process_state();

        let options = ServerOptions::from_env_and_args([
            "--listen".to_string(),
            "127.0.0.1:60051".to_string(),
        ])
        .unwrap();

        assert_eq!("127.0.0.1:60051", options.listen_address.to_string());
    }

    #[test]
    fn lookup_result_defaults_to_unspecified_state() {
        let result = LookupResult::default();

        assert_eq!(LookupState::Unspecified as i32, result.state);
    }

    #[test]
    fn server_options_allow_sqlite_storage_override() {
        let (_process_state_lock, _process_state) = capture_clean_process_state();

        let options = ServerOptions::from_env_and_args([
            "--storage".to_string(),
            "sqlite".to_string(),
            "--sqlite-path".to_string(),
            "data\\qsoripper.db".to_string(),
        ])
        .unwrap();

        assert_eq!(options.storage.backend, StorageBackendKind::Sqlite);
        assert_eq!(
            options.storage.sqlite_path,
            std::path::PathBuf::from("data\\qsoripper.db")
        );
        assert_eq!(
            Some("sqlite"),
            options
                .runtime_config_cli_storage_overrides()
                .get(STORAGE_BACKEND_ENV_VAR)
                .map(String::as_str)
        );
        assert_eq!(
            Some("data\\qsoripper.db"),
            options
                .runtime_config_cli_storage_overrides()
                .get(SQLITE_PATH_ENV_VAR)
                .map(String::as_str)
        );
    }

    #[test]
    fn server_options_emit_default_sqlite_path_when_cli_selects_sqlite_without_path() {
        let (_process_state_lock, _process_state) = capture_clean_process_state();

        let options =
            ServerOptions::from_env_and_args(["--storage".to_string(), "sqlite".to_string()])
                .unwrap();

        assert_eq!(options.storage.backend, StorageBackendKind::Sqlite);
        assert_eq!(
            Some("sqlite"),
            options
                .runtime_config_cli_storage_overrides()
                .get(STORAGE_BACKEND_ENV_VAR)
                .map(String::as_str)
        );
        assert_eq!(
            Some("qsoripper.db"),
            options
                .runtime_config_cli_storage_overrides()
                .get(SQLITE_PATH_ENV_VAR)
                .map(String::as_str)
        );
    }

    #[test]
    fn parse_storage_backend_rejects_unknown_values() {
        let error = parse_storage_backend("rocksdb").unwrap_err();

        assert!(error.to_string().contains("Unsupported storage backend"));
    }

    #[test]
    fn build_storage_uses_requested_backend() {
        let memory_storage = build_storage(&StorageOptions {
            backend: StorageBackendKind::Memory,
            sqlite_path: std::path::PathBuf::from("ignored.db"),
        })
        .expect("memory storage");
        assert_eq!(memory_storage.backend_name(), "memory");

        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let sqlite_path = std::env::temp_dir().join(format!(
            "qsoripper-storage-{}-{unique_suffix}.db",
            std::process::id()
        ));
        let sqlite_storage = build_storage(&StorageOptions {
            backend: StorageBackendKind::Sqlite,
            sqlite_path: sqlite_path.clone(),
        })
        .expect("sqlite storage");
        assert_eq!(sqlite_storage.backend_name(), "sqlite");
        drop(sqlite_storage);

        if sqlite_path.exists() {
            fs::remove_file(sqlite_path).expect("remove sqlite test database");
        }
    }

    #[tokio::test]
    async fn server_exits_cleanly_when_shutdown_signal_resolves() {
        let config_path = unique_sqlite_test_path("ctrl-c-config").with_extension("toml");
        let options = ServerOptions {
            listen_address: "127.0.0.1:0".parse().expect("listen address"),
            config_path: config_path.clone(),
            storage: StorageOptions {
                backend: StorageBackendKind::Memory,
                sqlite_path: PathBuf::from("ignored.db"),
            },
            storage_cli_overrides: BTreeMap::from([(
                STORAGE_BACKEND_ENV_VAR.to_string(),
                "memory".to_string(),
            )]),
        };

        tokio::time::timeout(
            Duration::from_secs(1),
            run_server(options, std::future::ready(Ok(()))),
        )
        .await
        .expect("shutdown timeout")
        .expect("clean server shutdown");

        if config_path.exists() {
            fs::remove_file(config_path).expect("remove config test file");
        }
    }

    #[tokio::test]
    async fn lookup_service_lookup_returns_error_state_when_provider_is_disabled() {
        let service = test_lookup_service();

        let response = LookupService::lookup(
            &service,
            Request::new(LookupRequest {
                callsign: "W1AW".to_string(),
                skip_cache: false,
            }),
        )
        .await
        .expect("lookup response")
        .into_inner()
        .result
        .expect("lookup result payload");

        assert_eq!(LookupState::Error as i32, response.state);
        assert_eq!(
            Some(
                "Provider configuration error: Required environment variable 'QSORIPPER_QRZ_XML_USERNAME' is missing or blank."
            ),
            response.error_message.as_deref()
        );
    }

    #[tokio::test]
    async fn stream_lookup_emits_loading_then_error_when_provider_is_disabled() {
        let service = test_lookup_service();

        let response = LookupService::stream_lookup(
            &service,
            Request::new(StreamLookupRequest {
                callsign: "W1AW".to_string(),
                skip_cache: false,
            }),
        )
        .await
        .expect("stream response")
        .into_inner();
        let updates = response
            .map(|result| {
                result
                    .expect("stream item")
                    .result
                    .expect("stream result payload")
            })
            .collect::<Vec<_>>()
            .await;

        assert_eq!(2, updates.len());
        assert_eq!(
            LookupState::Loading as i32,
            updates.first().expect("loading update").state
        );
        assert_eq!(
            LookupState::Error as i32,
            updates.get(1).expect("error update").state
        );
    }

    #[tokio::test]
    async fn cache_lookup_defaults_to_unspecified_without_cached_value() {
        let service = test_lookup_service();

        let response = LookupService::get_cached_callsign(
            &service,
            Request::new(GetCachedCallsignRequest {
                callsign: "W1AW".to_string(),
            }),
        )
        .await
        .expect("cache response")
        .into_inner()
        .result
        .expect("cached lookup result payload");

        assert_eq!(LookupState::NotFound as i32, response.state);
        assert!(!response.cache_hit);
    }

    #[tokio::test]
    async fn dxcc_and_batch_lookup_remain_unimplemented_for_first_slice() {
        let service = test_lookup_service();

        let dxcc_error = LookupService::get_dxcc_entity(
            &service,
            Request::new(GetDxccEntityRequest {
                query: Some(get_dxcc_entity_request::Query::Prefix("W1AW".to_string())),
            }),
        )
        .await
        .expect_err("dxcc should be unimplemented");
        let batch_error = LookupService::batch_lookup(
            &service,
            Request::new(BatchLookupRequest { callsigns: vec![] }),
        )
        .await
        .expect_err("batch should be unimplemented");

        assert_eq!(Code::Unimplemented, dxcc_error.code());
        assert_eq!(Code::Unimplemented, batch_error.code());
    }

    #[tokio::test]
    async fn logbook_crud_flow_works_through_memory_grpc_surface() {
        let service = DeveloperLogbookService::new(test_runtime_config_with_logbook(
            StorageBackendKind::Memory,
            None,
            true,
        ));

        exercise_logbook_crud_flow(&service).await;
    }

    #[tokio::test]
    async fn logbook_crud_flow_works_through_sqlite_grpc_surface() {
        let sqlite_path = unique_sqlite_test_path("logbook-grpc");
        let service = DeveloperLogbookService::new(test_runtime_config_with_logbook(
            StorageBackendKind::Sqlite,
            Some(&sqlite_path),
            true,
        ));

        exercise_logbook_crud_flow(&service).await;

        drop(service);
        if sqlite_path.exists() {
            fs::remove_file(sqlite_path).expect("remove sqlite test database");
        }
    }

    #[tokio::test]
    async fn logbook_sync_status_reports_live_local_counts() {
        let service = DeveloperLogbookService::new(test_runtime_config());

        let logged = LogbookService::log_qso(
            &service,
            Request::new(LogQsoRequest {
                qso: Some(QsoRecord {
                    station_callsign: "K7RND".to_string(),
                    worked_callsign: "W1AW".to_string(),
                    utc_timestamp: Some(Timestamp {
                        seconds: 1_731_600_000,
                        nanos: 0,
                    }),
                    band: Band::Band20m as i32,
                    mode: Mode::Ssb as i32,
                    ..QsoRecord::default()
                }),
                sync_to_qrz: false,
            }),
        )
        .await
        .expect("log response")
        .into_inner();

        let mut second_qso = QsoRecord {
            station_callsign: "K7RND".to_string(),
            worked_callsign: "K7XYZ".to_string(),
            utc_timestamp: Some(Timestamp {
                seconds: 1_731_600_100,
                nanos: 0,
            }),
            band: Band::Band40m as i32,
            mode: Mode::Cw as i32,
            ..QsoRecord::default()
        };
        second_qso.sync_status =
            qsoripper_core::proto::qsoripper::domain::SyncStatus::Synced as i32;
        let _ = LogbookService::log_qso(
            &service,
            Request::new(LogQsoRequest {
                qso: Some(second_qso),
                sync_to_qrz: false,
            }),
        )
        .await
        .expect("second log response");

        let response =
            LogbookService::get_sync_status(&service, Request::new(GetSyncStatusRequest {}))
                .await
                .expect("sync status")
                .into_inner();

        assert_eq!(2, response.local_qso_count);
        assert_eq!(0, response.qrz_qso_count);
        assert_eq!(1, response.pending_upload);
        assert!(response.last_sync.is_none());
        assert!(response.qrz_logbook_owner.is_none());

        let _ = LogbookService::delete_qso(
            &service,
            Request::new(DeleteQsoRequest {
                local_id: logged.local_id,
                delete_from_qrz: false,
            }),
        )
        .await
        .expect("delete first qso");
    }

    #[tokio::test]
    async fn adif_import_preserves_station_history_and_reports_duplicates() {
        let service = DeveloperLogbookService::new(test_runtime_config_with_logbook(
            StorageBackendKind::Memory,
            None,
            true,
        ));
        let (mut client, server_handle) = grpc_logbook_client(service).await;
        let payload = concat!(
            "<ADIF_VER:5>3.1.7\n<EOH>\n",
            "<CALL:4>W1AW<STATION_CALLSIGN:5>K1ABC<OPERATOR:5>N1OPS<MY_GRIDSQUARE:6>FN42aa<QSO_DATE:8>20250102<TIME_ON:6>010203<BAND:3>20M<MODE:3>SSB<EOR>\n",
            "<CALL:4>W1AW<STATION_CALLSIGN:5>K1ABC<OPERATOR:5>N1OPS<MY_GRIDSQUARE:6>FN42aa<QSO_DATE:8>20250102<TIME_ON:6>010203<BAND:3>20M<MODE:3>SSB<EOR>\n"
        )
        .as_bytes();

        let (first_chunk, second_chunk) = payload.split_at(40);
        let result = import_adif_payload(
            &mut client,
            vec![first_chunk.to_vec(), second_chunk.to_vec()],
        )
        .await;

        assert_eq!(1, result.records_imported);
        assert_eq!(1, result.records_skipped);
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("duplicate skipped")));

        let imported = client
            .list_qsos(Request::new(ListQsosRequest {
                limit: 10,
                sort: QsoSortOrder::OldestFirst as i32,
                ..ListQsosRequest::default()
            }))
            .await
            .expect("list response")
            .into_inner()
            .map(|result| result.expect("list item").qso.expect("listed qso payload"))
            .collect::<Vec<_>>()
            .await;

        assert_eq!(1, imported.len());
        let imported = imported.first().expect("imported qso");
        assert_eq!("K1ABC", imported.station_callsign);
        let snapshot = imported
            .station_snapshot
            .as_ref()
            .expect("station snapshot");
        assert_eq!("K1ABC", snapshot.station_callsign);
        assert_eq!(Some("N1OPS"), snapshot.operator_callsign.as_deref());
        assert_eq!(Some("FN42aa"), snapshot.grid.as_deref());
        assert_eq!(
            None,
            snapshot.profile_name.as_deref(),
            "imported station history should not be overwritten by the active profile"
        );

        server_handle.abort();
    }

    #[tokio::test]
    async fn adif_import_uses_active_station_profile_only_as_explicit_fallback() {
        let service = DeveloperLogbookService::new(test_runtime_config_with_logbook(
            StorageBackendKind::Memory,
            None,
            true,
        ));
        let (mut client, server_handle) = grpc_logbook_client(service).await;
        let payload =
            b"<CALL:5>DL1AA<QSO_DATE:8>20250103<TIME_ON:6>030405<BAND:3>20M<MODE:2>CW<EOR>\n";

        let result = import_adif_payload(&mut client, vec![payload.to_vec()]).await;

        assert_eq!(1, result.records_imported);
        assert_eq!(0, result.records_skipped);
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("applied active station profile")));

        let imported = client
            .list_qsos(Request::new(ListQsosRequest {
                limit: 1,
                sort: QsoSortOrder::OldestFirst as i32,
                ..ListQsosRequest::default()
            }))
            .await
            .expect("list response")
            .into_inner()
            .next()
            .await
            .expect("list item")
            .expect("qso")
            .qso
            .expect("listed qso payload");

        assert_eq!("K7RND", imported.station_callsign);
        assert_eq!(
            Some("Home"),
            imported
                .station_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.profile_name.as_deref())
        );

        server_handle.abort();
    }

    #[tokio::test]
    async fn adif_import_reports_invalid_records_without_active_station_fallback() {
        let service = DeveloperLogbookService::new(test_runtime_config());
        let (mut client, server_handle) = grpc_logbook_client(service).await;
        let payload = b"<CALL:5>DL1AA<STATION_CALLSIGN:5>K1ABC<QSO_DATE:8>20250103<TIME_ON:6>030405<BAND:3>11M<MODE:2>CW<EOR>\n";

        let result = import_adif_payload(&mut client, vec![payload.to_vec()]).await;

        assert_eq!(0, result.records_imported);
        assert_eq!(1, result.records_skipped);
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("unrecognized ADIF band '11M'")));

        server_handle.abort();
    }

    #[tokio::test]
    async fn adif_export_streams_filtered_qsos_in_adif_order() {
        let service = DeveloperLogbookService::new(test_runtime_config_with_logbook(
            StorageBackendKind::Memory,
            None,
            true,
        ));
        let (mut client, server_handle) = grpc_logbook_client(service.clone()).await;

        let first = QsoRecord {
            station_callsign: "K7RND".to_string(),
            worked_callsign: "W1AW".to_string(),
            utc_timestamp: Some(Timestamp {
                seconds: 1_735_689_600,
                nanos: 0,
            }),
            band: Band::Band20m as i32,
            mode: Mode::Ssb as i32,
            contest_id: Some("FIELD-DAY".to_string()),
            ..QsoRecord::default()
        };
        let second = QsoRecord {
            station_callsign: "K7RND".to_string(),
            worked_callsign: "K3LR".to_string(),
            utc_timestamp: Some(Timestamp {
                seconds: 1_735_689_900,
                nanos: 0,
            }),
            band: Band::Band40m as i32,
            mode: Mode::Cw as i32,
            ..QsoRecord::default()
        };

        let _ = client
            .log_qso(Request::new(LogQsoRequest {
                qso: Some(first),
                sync_to_qrz: false,
            }))
            .await
            .expect("log first qso");
        let _ = client
            .log_qso(Request::new(LogQsoRequest {
                qso: Some(second),
                sync_to_qrz: false,
            }))
            .await
            .expect("log second qso");

        let response = client
            .export_adif(Request::new(ExportAdifRequest {
                contest_id: Some("FIELD-DAY".to_string()),
                include_header: true,
                ..ExportAdifRequest::default()
            }))
            .await
            .expect("export response")
            .into_inner();
        let chunks = response
            .map(|result| result.expect("chunk"))
            .collect::<Vec<_>>()
            .await;
        let bytes = chunks.into_iter().fold(Vec::new(), |mut output, chunk| {
            output.extend_from_slice(&chunk.chunk.expect("export chunk payload").data);
            output
        });
        let text = String::from_utf8(bytes).expect("utf8 payload");

        assert!(text.contains("<ADIF_VER:5>3.1.7"));
        assert!(text.contains("<PROGRAMID:9>QsoRipper"));
        assert!(text.contains("<CALL:4>W1AW"));
        assert!(!text.contains("<CALL:4>K3LR"));
        assert!(text.contains("<CONTEST_ID:9>FIELD-DAY"));

        server_handle.abort();
    }

    #[tokio::test]
    async fn logbook_requires_station_context_when_station_callsign_is_missing() {
        let service = DeveloperLogbookService::new(test_runtime_config());

        let error = LogbookService::log_qso(
            &service,
            Request::new(LogQsoRequest {
                qso: Some(sample_qso_without_station_callsign("W1AW")),
                sync_to_qrz: false,
            }),
        )
        .await
        .expect_err("missing station context should fail");

        assert_eq!(Code::InvalidArgument, error.code());
        assert_eq!("station_callsign is required.", error.message());
    }

    #[tokio::test]
    async fn logbook_requires_timestamp_band_and_mode() {
        let service = DeveloperLogbookService::new(test_runtime_config_with_logbook(
            StorageBackendKind::Memory,
            None,
            true,
        ));

        let error = LogbookService::log_qso(
            &service,
            Request::new(LogQsoRequest {
                qso: Some(QsoRecord {
                    worked_callsign: "W1AW".to_string(),
                    ..QsoRecord::default()
                }),
                sync_to_qrz: false,
            }),
        )
        .await
        .expect_err("missing timestamp/band/mode should fail");

        assert_eq!(Code::InvalidArgument, error.code());
        assert_eq!("utc_timestamp is required.", error.message());

        let band_error = LogbookService::log_qso(
            &service,
            Request::new(LogQsoRequest {
                qso: Some(QsoRecord {
                    worked_callsign: "W1AW".to_string(),
                    utc_timestamp: Some(Timestamp {
                        seconds: 1_731_600_000,
                        nanos: 0,
                    }),
                    ..QsoRecord::default()
                }),
                sync_to_qrz: false,
            }),
        )
        .await
        .expect_err("missing band should fail");

        assert_eq!(Code::InvalidArgument, band_error.code());
        assert_eq!("band is required.", band_error.message());

        let mode_error = LogbookService::log_qso(
            &service,
            Request::new(LogQsoRequest {
                qso: Some(QsoRecord {
                    worked_callsign: "W1AW".to_string(),
                    utc_timestamp: Some(Timestamp {
                        seconds: 1_731_600_000,
                        nanos: 0,
                    }),
                    band: Band::Band20m as i32,
                    ..QsoRecord::default()
                }),
                sync_to_qrz: false,
            }),
        )
        .await
        .expect_err("missing mode should fail");

        assert_eq!(Code::InvalidArgument, mode_error.code());
        assert_eq!("mode is required.", mode_error.message());
    }
}

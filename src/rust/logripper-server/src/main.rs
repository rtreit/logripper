//! Runnable tonic gRPC host for the `LogRipper` Rust engine.

mod runtime_config;

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use logripper_core::application::logbook::LogbookError;
use logripper_core::storage::{EngineStorage, QsoListQuery, QsoSortOrder, StorageError};
use logripper_storage_memory::MemoryStorage;
use logripper_storage_sqlite::SqliteStorageBuilder;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use logripper_core::proto::logripper::domain::{
    Band, BatchLookupRequest, BatchLookupResponse, CachedCallsignRequest, DxccEntity, DxccRequest,
    LookupRequest, LookupResult, Mode, QsoRecord,
};
use logripper_core::proto::logripper::services::{
    developer_control_service_server::{DeveloperControlService, DeveloperControlServiceServer},
    logbook_service_server::{LogbookService, LogbookServiceServer},
    lookup_service_server::{LookupService, LookupServiceServer},
    AdifChunk, ApplyRuntimeConfigRequest, DeleteQsoRequest, DeleteQsoResponse, ExportRequest,
    GetQsoRequest, GetQsoResponse, GetRuntimeConfigRequest, ImportResult, ListQsosRequest,
    LogQsoRequest, LogQsoResponse, ResetRuntimeConfigRequest, RuntimeConfigSnapshot, SortOrder,
    SyncProgress, SyncRequest, SyncStatusRequest, SyncStatusResponse, UpdateQsoRequest,
    UpdateQsoResponse,
};
use runtime_config::RuntimeConfigManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_dotenv_if_present();
    let options = ServerOptions::from_env_and_args(std::env::args().skip(1))?;
    let address = options.listen_address;
    let runtime_config = Arc::new(RuntimeConfigManager::new_from_storage_options(
        &options.storage,
    )?);
    let logbook_service = DeveloperLogbookService::new(runtime_config.clone());
    let lookup_service = DeveloperLookupService::new(runtime_config.clone());
    let developer_control_service = DeveloperControlSurface::new(runtime_config.clone());
    let active_storage_backend = runtime_config.active_storage_backend().await;

    println!("Starting LogRipper gRPC server on {address} using {active_storage_backend} storage");

    Server::builder()
        .add_service(LogbookServiceServer::new(logbook_service))
        .add_service(LookupServiceServer::new(lookup_service))
        .add_service(DeveloperControlServiceServer::new(
            developer_control_service,
        ))
        .serve(address)
        .await?;

    Ok(())
}

fn load_dotenv_if_present() {
    dotenvy::dotenv().ok();
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
    type ListQsosStream = ReceiverStream<Result<QsoRecord, Status>>;
    type SyncWithQrzStream = ReceiverStream<Result<SyncProgress, Status>>;
    type ExportAdifStream = ReceiverStream<Result<AdifChunk, Status>>;

    async fn log_qso(
        &self,
        request: Request<LogQsoRequest>,
    ) -> Result<Response<LogQsoResponse>, Status> {
        let engine = self.runtime_config.logbook_engine().await;
        let request = request.into_inner();
        let qso = request
            .qso
            .ok_or_else(|| Status::invalid_argument("LogQso requires a qso payload."))?;
        let stored = engine.log_qso(qso).await.map_err(map_logbook_error)?;
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
            if sender.send(Ok(record)).await.is_err() {
                break;
            }
        }

        Ok(Response::new(ReceiverStream::new(receiver)))
    }

    async fn sync_with_qrz(
        &self,
        _request: Request<SyncRequest>,
    ) -> Result<Response<Self::SyncWithQrzStream>, Status> {
        Err(Status::unimplemented("SyncWithQrz is not implemented yet."))
    }

    async fn get_sync_status(
        &self,
        _request: Request<SyncStatusRequest>,
    ) -> Result<Response<SyncStatusResponse>, Status> {
        let sync_status = self
            .runtime_config
            .logbook_engine()
            .await
            .get_sync_status()
            .await
            .map_err(map_logbook_error)?;

        Ok(Response::new(SyncStatusResponse {
            local_qso_count: sync_status.local_qso_count,
            qrz_qso_count: sync_status.qrz_qso_count,
            pending_upload: sync_status.pending_upload,
            last_sync: sync_status.last_sync,
            qrz_logbook_owner: sync_status.qrz_logbook_owner,
        }))
    }

    async fn import_adif(
        &self,
        _request: Request<tonic::Streaming<AdifChunk>>,
    ) -> Result<Response<ImportResult>, Status> {
        Err(Status::unimplemented("ImportAdif is not implemented yet."))
    }

    async fn export_adif(
        &self,
        _request: Request<ExportRequest>,
    ) -> Result<Response<Self::ExportAdifStream>, Status> {
        Err(Status::unimplemented("ExportAdif is not implemented yet."))
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
    type StreamLookupStream = ReceiverStream<Result<LookupResult, Status>>;

    async fn lookup(
        &self,
        request: Request<LookupRequest>,
    ) -> Result<Response<LookupResult>, Status> {
        let coordinator = self.runtime_config.lookup_coordinator().await;
        let request = request.into_inner();
        Ok(Response::new(
            coordinator
                .lookup(&request.callsign, request.skip_cache)
                .await,
        ))
    }

    async fn stream_lookup(
        &self,
        request: Request<LookupRequest>,
    ) -> Result<Response<Self::StreamLookupStream>, Status> {
        let coordinator = self.runtime_config.lookup_coordinator().await;
        let request = request.into_inner();
        let updates = coordinator
            .stream_lookup(&request.callsign, request.skip_cache)
            .await;
        let (sender, receiver) = tokio::sync::mpsc::channel(8);

        for update in updates {
            if sender.send(Ok(update)).await.is_err() {
                break;
            }
        }

        Ok(Response::new(ReceiverStream::new(receiver)))
    }

    async fn get_cached_callsign(
        &self,
        request: Request<CachedCallsignRequest>,
    ) -> Result<Response<LookupResult>, Status> {
        let coordinator = self.runtime_config.lookup_coordinator().await;
        let request = request.into_inner();
        Ok(Response::new(
            coordinator.get_cached_callsign(&request.callsign).await,
        ))
    }

    async fn get_dxcc_entity(
        &self,
        _request: Request<DxccRequest>,
    ) -> Result<Response<DxccEntity>, Status> {
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
    ) -> Result<Response<RuntimeConfigSnapshot>, Status> {
        Ok(Response::new(self.runtime_config.snapshot().await))
    }

    async fn apply_runtime_config(
        &self,
        request: Request<ApplyRuntimeConfigRequest>,
    ) -> Result<Response<RuntimeConfigSnapshot>, Status> {
        let snapshot = self
            .runtime_config
            .apply_request(request.into_inner())
            .await
            .map_err(Status::invalid_argument)?;
        Ok(Response::new(snapshot))
    }

    async fn reset_runtime_config(
        &self,
        request: Request<ResetRuntimeConfigRequest>,
    ) -> Result<Response<RuntimeConfigSnapshot>, Status> {
        let snapshot = self
            .runtime_config
            .reset_request(request.into_inner())
            .await
            .map_err(Status::invalid_argument)?;
        Ok(Response::new(snapshot))
    }
}

#[derive(Debug, Clone)]
struct ServerOptions {
    listen_address: SocketAddr,
    storage: StorageOptions,
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
        let mut listen = std::env::var("LOGRIPPER_SERVER_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:50051".to_string());
        let mut storage_backend = parse_storage_backend(
            &std::env::var("LOGRIPPER_STORAGE_BACKEND").unwrap_or_else(|_| "memory".to_string()),
        )?;
        let mut sqlite_path = PathBuf::from(
            std::env::var("LOGRIPPER_SQLITE_PATH").unwrap_or_else(|_| "logripper.db".to_string()),
        );
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--listen" => {
                    let value = args.next().ok_or("Missing value for --listen")?;
                    listen = value;
                }
                "--storage" => {
                    let value = args.next().ok_or("Missing value for --storage")?;
                    storage_backend = parse_storage_backend(&value)?;
                }
                "--sqlite-path" => {
                    let value = args.next().ok_or("Missing value for --sqlite-path")?;
                    sqlite_path = PathBuf::from(value);
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                _ => return Err(format!("Unknown argument: {arg}").into()),
            }
        }

        Ok(Self {
            listen_address: listen.parse()?,
            storage: StorageOptions {
                backend: storage_backend,
                sqlite_path,
            },
        })
    }
}

fn print_help() {
    println!(
        "LogRipper gRPC server\n\nUsage:\n  cargo run -p logripper-server -- [--listen 127.0.0.1:50051] [--storage memory|sqlite] [--sqlite-path path\\to\\logripper.db]\n\nEnvironment:\n  LOGRIPPER_SERVER_ADDR       Overrides the bind address\n  LOGRIPPER_STORAGE_BACKEND   Selects memory or sqlite storage (default: memory)\n  LOGRIPPER_SQLITE_PATH       SQLite path when sqlite storage is selected (default: logripper.db)"
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
    let sort = match SortOrder::try_from(request.sort) {
        Ok(SortOrder::NewestFirst) => QsoSortOrder::NewestFirst,
        Ok(SortOrder::OldestFirst) => QsoSortOrder::OldestFirst,
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
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        build_storage, load_dotenv_if_present, parse_storage_backend, DeveloperLogbookService,
        DeveloperLookupService, ServerOptions, StorageBackendKind, StorageOptions,
    };
    use crate::runtime_config::RuntimeConfigManager;
    use logripper_core::proto::logripper::domain::{
        dxcc_request, BatchLookupRequest, CachedCallsignRequest, DxccRequest, LookupRequest,
        LookupResult, LookupState,
    };
    use logripper_core::proto::logripper::services::{
        logbook_service_server::LogbookService, lookup_service_server::LookupService,
        SyncStatusRequest,
    };
    use tokio_stream::StreamExt;
    use tonic::{Code, Request};

    static PROCESS_STATE_LOCK: Mutex<()> = Mutex::new(());

    const SERVER_ENV_KEYS: [&str; 3] = [
        "LOGRIPPER_SERVER_ADDR",
        "LOGRIPPER_STORAGE_BACKEND",
        "LOGRIPPER_SQLITE_PATH",
    ];

    struct ProcessStateGuard {
        original_dir: PathBuf,
        original_env: Vec<(&'static str, Option<String>)>,
    }

    impl ProcessStateGuard {
        fn capture() -> Self {
            Self {
                original_dir: std::env::current_dir().expect("current working directory"),
                original_env: SERVER_ENV_KEYS
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

    fn test_lookup_service() -> DeveloperLookupService {
        DeveloperLookupService::new(test_runtime_config())
    }

    fn test_runtime_config() -> Arc<RuntimeConfigManager> {
        Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime config"))
    }

    #[test]
    fn load_dotenv_if_present_reads_env_from_current_directory() {
        let _process_state_lock = PROCESS_STATE_LOCK.lock().expect("lock process state");
        let process_state = ProcessStateGuard::capture();

        for key in SERVER_ENV_KEYS {
            std::env::remove_var(key);
        }

        let temp_dir = std::env::temp_dir().join(format!(
            "logripper-dotenv-{}-{}",
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
                "LOGRIPPER_SERVER_ADDR=127.0.0.1:61051\n",
                "LOGRIPPER_STORAGE_BACKEND=sqlite\n",
                "LOGRIPPER_SQLITE_PATH=data/test-logripper.db\n"
            ),
        )
        .expect("write temp .env");

        std::env::set_current_dir(&temp_dir).expect("switch to temp dir");
        load_dotenv_if_present();

        let options = ServerOptions::from_env_and_args(Vec::<String>::new()).unwrap();

        assert_eq!("127.0.0.1:61051", options.listen_address.to_string());
        assert_eq!(StorageBackendKind::Sqlite, options.storage.backend);
        assert_eq!(
            PathBuf::from("data/test-logripper.db"),
            options.storage.sqlite_path
        );

        process_state.restore_current_dir();
        fs::remove_file(env_path).expect("remove temp .env");
        fs::remove_dir(temp_dir).expect("remove temp dir");
    }

    #[test]
    fn server_options_default_to_localhost_port_50051() {
        std::env::remove_var("LOGRIPPER_SERVER_ADDR");
        std::env::remove_var("LOGRIPPER_STORAGE_BACKEND");
        std::env::remove_var("LOGRIPPER_SQLITE_PATH");

        let options = ServerOptions::from_env_and_args(Vec::<String>::new()).unwrap();

        assert_eq!("127.0.0.1:50051", options.listen_address.to_string());
        assert_eq!(options.storage.backend, StorageBackendKind::Memory);
    }

    #[test]
    fn server_options_allow_explicit_listen_override() {
        std::env::remove_var("LOGRIPPER_SERVER_ADDR");

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
        let options = ServerOptions::from_env_and_args([
            "--storage".to_string(),
            "sqlite".to_string(),
            "--sqlite-path".to_string(),
            "data\\logripper.db".to_string(),
        ])
        .unwrap();

        assert_eq!(options.storage.backend, StorageBackendKind::Sqlite);
        assert_eq!(
            options.storage.sqlite_path,
            std::path::PathBuf::from("data\\logripper.db")
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
            "logripper-storage-{}-{unique_suffix}.db",
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
        .into_inner();

        assert_eq!(LookupState::Error as i32, response.state);
        assert_eq!(
            Some(
                "Provider configuration error: Required environment variable 'LOGRIPPER_QRZ_XML_USERNAME' is missing or blank."
            ),
            response.error_message.as_deref()
        );
    }

    #[tokio::test]
    async fn stream_lookup_emits_loading_then_error_when_provider_is_disabled() {
        let service = test_lookup_service();

        let response = LookupService::stream_lookup(
            &service,
            Request::new(LookupRequest {
                callsign: "W1AW".to_string(),
                skip_cache: false,
            }),
        )
        .await
        .expect("stream response")
        .into_inner();
        let updates = response
            .map(|result| result.expect("stream item"))
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
            Request::new(CachedCallsignRequest {
                callsign: "W1AW".to_string(),
            }),
        )
        .await
        .expect("cache response")
        .into_inner();

        assert_eq!(LookupState::NotFound as i32, response.state);
        assert!(!response.cache_hit);
    }

    #[tokio::test]
    async fn dxcc_and_batch_lookup_remain_unimplemented_for_first_slice() {
        let service = test_lookup_service();

        let dxcc_error = LookupService::get_dxcc_entity(
            &service,
            Request::new(DxccRequest {
                query: Some(dxcc_request::Query::Prefix("W1AW".to_string())),
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
    async fn logbook_sync_status_returns_zeroed_placeholder_values() {
        let service = DeveloperLogbookService::new(test_runtime_config());

        let response =
            LogbookService::get_sync_status(&service, Request::new(SyncStatusRequest {}))
                .await
                .expect("sync status")
                .into_inner();

        assert_eq!(0, response.local_qso_count);
        assert_eq!(0, response.qrz_qso_count);
        assert_eq!(0, response.pending_upload);
        assert!(response.last_sync.is_none());
        assert!(response.qrz_logbook_owner.is_none());
    }
}

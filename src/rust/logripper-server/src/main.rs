//! Runnable tonic gRPC host for the `LogRipper` Rust engine.

use std::{net::SocketAddr, sync::Arc};

use logripper_core::lookup::{
    CallsignProvider, DisabledCallsignProvider, LookupCoordinator, LookupCoordinatorConfig,
    QrzXmlConfig, QrzXmlProvider,
};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use logripper_core::proto::logripper::domain::{
    BatchLookupRequest, BatchLookupResponse, CachedCallsignRequest, DxccEntity, DxccRequest,
    LookupRequest, LookupResult, QsoRecord,
};
use logripper_core::proto::logripper::services::{
    logbook_service_server::{LogbookService, LogbookServiceServer},
    lookup_service_server::{LookupService, LookupServiceServer},
    AdifChunk, DeleteQsoRequest, DeleteQsoResponse, ExportRequest, GetQsoRequest, GetQsoResponse,
    ImportResult, ListQsosRequest, LogQsoRequest, LogQsoResponse, SyncProgress, SyncRequest,
    SyncStatusRequest, SyncStatusResponse, UpdateQsoRequest, UpdateQsoResponse,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = ServerOptions::from_env_and_args(std::env::args().skip(1))?;
    let address = options.listen_address;
    let lookup_service = DeveloperLookupService::new(create_lookup_coordinator());

    println!("Starting LogRipper gRPC server on {address}");

    Server::builder()
        .add_service(LogbookServiceServer::new(DeveloperLogbookService))
        .add_service(LookupServiceServer::new(lookup_service))
        .serve(address)
        .await?;

    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct DeveloperLogbookService;

#[tonic::async_trait]
impl LogbookService for DeveloperLogbookService {
    type ListQsosStream = ReceiverStream<Result<QsoRecord, Status>>;
    type SyncWithQrzStream = ReceiverStream<Result<SyncProgress, Status>>;
    type ExportAdifStream = ReceiverStream<Result<AdifChunk, Status>>;

    async fn log_qso(
        &self,
        _request: Request<LogQsoRequest>,
    ) -> Result<Response<LogQsoResponse>, Status> {
        Err(Status::unimplemented("LogQso is not implemented yet."))
    }

    async fn update_qso(
        &self,
        _request: Request<UpdateQsoRequest>,
    ) -> Result<Response<UpdateQsoResponse>, Status> {
        Err(Status::unimplemented("UpdateQso is not implemented yet."))
    }

    async fn delete_qso(
        &self,
        _request: Request<DeleteQsoRequest>,
    ) -> Result<Response<DeleteQsoResponse>, Status> {
        Err(Status::unimplemented("DeleteQso is not implemented yet."))
    }

    async fn get_qso(
        &self,
        _request: Request<GetQsoRequest>,
    ) -> Result<Response<GetQsoResponse>, Status> {
        Err(Status::unimplemented("GetQso is not implemented yet."))
    }

    async fn list_qsos(
        &self,
        _request: Request<ListQsosRequest>,
    ) -> Result<Response<Self::ListQsosStream>, Status> {
        Err(Status::unimplemented("ListQsos is not implemented yet."))
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
        Ok(Response::new(SyncStatusResponse {
            local_qso_count: 0,
            qrz_qso_count: 0,
            pending_upload: 0,
            last_sync: None,
            qrz_logbook_owner: None,
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
    coordinator: Arc<LookupCoordinator>,
}

impl DeveloperLookupService {
    fn new(coordinator: Arc<LookupCoordinator>) -> Self {
        Self { coordinator }
    }
}

#[tonic::async_trait]
impl LookupService for DeveloperLookupService {
    type StreamLookupStream = ReceiverStream<Result<LookupResult, Status>>;

    async fn lookup(
        &self,
        request: Request<LookupRequest>,
    ) -> Result<Response<LookupResult>, Status> {
        let request = request.into_inner();
        Ok(Response::new(
            self.coordinator
                .lookup(&request.callsign, request.skip_cache)
                .await,
        ))
    }

    async fn stream_lookup(
        &self,
        request: Request<LookupRequest>,
    ) -> Result<Response<Self::StreamLookupStream>, Status> {
        let request = request.into_inner();
        let updates = self
            .coordinator
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
        let request = request.into_inner();
        Ok(Response::new(
            self.coordinator
                .get_cached_callsign(&request.callsign)
                .await,
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

fn create_lookup_coordinator() -> Arc<LookupCoordinator> {
    Arc::new(LookupCoordinator::new(
        build_provider_from_env(),
        LookupCoordinatorConfig::default(),
    ))
}

fn build_provider_from_env() -> Arc<dyn CallsignProvider> {
    match QrzXmlConfig::from_env() {
        Ok(config) => match QrzXmlProvider::new(config) {
            Ok(provider) => Arc::new(provider),
            Err(error) => {
                eprintln!("QRZ XML lookup disabled: {error}");
                Arc::new(DisabledCallsignProvider::new(error.to_string()))
            }
        },
        Err(error) => {
            eprintln!("QRZ XML lookup disabled: {error}");
            Arc::new(DisabledCallsignProvider::new(error.to_string()))
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ServerOptions {
    listen_address: SocketAddr,
}

impl ServerOptions {
    fn from_env_and_args<I>(args: I) -> Result<Self, Box<dyn std::error::Error>>
    where
        I: IntoIterator<Item = String>,
    {
        let mut listen = std::env::var("LOGRIPPER_SERVER_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:50051".to_string());
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--listen" => {
                    let value = args.next().ok_or("Missing value for --listen")?;
                    listen = value;
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
        })
    }
}

fn print_help() {
    println!(
        "LogRipper gRPC server\n\nUsage:\n  cargo run -p logripper-server -- [--listen 127.0.0.1:50051]\n\nEnvironment:\n  LOGRIPPER_SERVER_ADDR   Overrides the bind address"
    );
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use super::{DeveloperLogbookService, DeveloperLookupService, ServerOptions};
    use logripper_core::lookup::{
        DisabledCallsignProvider, LookupCoordinator, LookupCoordinatorConfig,
    };
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

    fn test_lookup_service() -> DeveloperLookupService {
        let coordinator = Arc::new(LookupCoordinator::new(
            Arc::new(DisabledCallsignProvider::new("lookup disabled")),
            LookupCoordinatorConfig::default(),
        ));
        DeveloperLookupService::new(coordinator)
    }

    #[test]
    fn server_options_default_to_localhost_port_50051() {
        std::env::remove_var("LOGRIPPER_SERVER_ADDR");

        let options = ServerOptions::from_env_and_args(Vec::<String>::new()).unwrap();

        assert_eq!("127.0.0.1:50051", options.listen_address.to_string());
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
            Some("Provider configuration error: lookup disabled"),
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
        let response = LogbookService::get_sync_status(
            &DeveloperLogbookService,
            Request::new(SyncStatusRequest {}),
        )
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

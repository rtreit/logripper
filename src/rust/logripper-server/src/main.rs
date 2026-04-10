//! Runnable tonic gRPC host for the `LogRipper` Rust engine.

use logripper_core::domain::lookup::{normalize_callsign, placeholder_lookup_error};
use std::net::SocketAddr;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use logripper_core::proto::logripper::domain::{
    BatchLookupRequest, BatchLookupResponse, CachedCallsignRequest, DxccEntity, DxccRequest,
    LookupRequest, LookupResult, LookupState, QsoRecord,
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

    println!("Starting LogRipper gRPC server on {address}");

    Server::builder()
        .add_service(LogbookServiceServer::new(DeveloperLogbookService))
        .add_service(LookupServiceServer::new(DeveloperLookupService))
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

#[derive(Debug, Clone, Copy)]
struct DeveloperLookupService;

#[tonic::async_trait]
impl LookupService for DeveloperLookupService {
    type StreamLookupStream = ReceiverStream<Result<LookupResult, Status>>;

    async fn lookup(
        &self,
        request: Request<LookupRequest>,
    ) -> Result<Response<LookupResult>, Status> {
        let request = request.into_inner();
        Ok(Response::new(placeholder_lookup_error(&request.callsign)))
    }

    async fn stream_lookup(
        &self,
        request: Request<LookupRequest>,
    ) -> Result<Response<Self::StreamLookupStream>, Status> {
        let request = request.into_inner();
        let (sender, receiver) = tokio::sync::mpsc::channel(4);
        let queried_callsign = normalize_callsign(&request.callsign);

        let _ = sender
            .send(Ok(LookupResult {
                state: LookupState::Loading as i32,
                record: None,
                error_message: None,
                cache_hit: false,
                lookup_latency_ms: 0,
                queried_callsign: queried_callsign.clone(),
            }))
            .await;

        let _ = sender
            .send(Ok(placeholder_lookup_error(&queried_callsign)))
            .await;

        Ok(Response::new(ReceiverStream::new(receiver)))
    }

    async fn get_cached_callsign(
        &self,
        request: Request<CachedCallsignRequest>,
    ) -> Result<Response<LookupResult>, Status> {
        let request = request.into_inner();
        Ok(Response::new(LookupResult {
            state: LookupState::NotFound as i32,
            record: None,
            error_message: None,
            cache_hit: false,
            lookup_latency_ms: 0,
            queried_callsign: normalize_callsign(&request.callsign),
        }))
    }

    async fn get_dxcc_entity(
        &self,
        _request: Request<DxccRequest>,
    ) -> Result<Response<DxccEntity>, Status> {
        Err(Status::unimplemented(
            "GetDxccEntity is not implemented yet.",
        ))
    }

    async fn batch_lookup(
        &self,
        request: Request<BatchLookupRequest>,
    ) -> Result<Response<BatchLookupResponse>, Status> {
        let request = request.into_inner();
        Ok(Response::new(BatchLookupResponse {
            results: request
                .callsigns
                .into_iter()
                .map(|callsign| placeholder_lookup_error(&callsign))
                .collect(),
        }))
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::ServerOptions;
    use logripper_core::proto::logripper::domain::{LookupResult, LookupState};

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
}

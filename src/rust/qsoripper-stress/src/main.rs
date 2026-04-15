//! Runnable stress harness host for `QsoRipper`.

mod controller;
mod runner;

use std::net::SocketAddr;

use controller::{StressControlSurface, StressController};
use qsoripper_core::proto::qsoripper::services::stress_control_service_server::StressControlServiceServer;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listen_address = parse_listen_address(std::env::args().skip(1))?;
    let controller = StressController::new();

    println!("Starting QsoRipper stress host on {listen_address}");

    Server::builder()
        .add_service(StressControlServiceServer::new(StressControlSurface::new(
            controller,
        )))
        .serve_with_shutdown(listen_address, async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;

    Ok(())
}

fn parse_listen_address<I>(mut args: I) -> Result<SocketAddr, String>
where
    I: Iterator<Item = String>,
{
    let mut listen = "127.0.0.1:50061".to_string();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--listen" => {
                let Some(value) = args.next() else {
                    return Err("Missing value for --listen.".to_string());
                };
                listen = value;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => {
                return Err(format!("Unknown argument '{arg}'. Use --help for usage."));
            }
        }
    }

    listen
        .parse()
        .map_err(|error| format!("Invalid listen address '{listen}': {error}"))
}

fn print_help() {
    println!(
        "QsoRipper stress host\n\nUsage:\n  cargo run -p qsoripper-stress -- [--listen 127.0.0.1:50061]"
    );
}

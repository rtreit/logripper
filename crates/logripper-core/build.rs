use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = PathBuf::from("../../proto");

    let protos = &[
        proto_root.join("domain/callsign.proto"),
        proto_root.join("domain/qso.proto"),
        proto_root.join("domain/lookup.proto"),
        proto_root.join("services/lookup_service.proto"),
        proto_root.join("services/logbook_service.proto"),
    ];

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(protos, &[&proto_root])?;

    Ok(())
}

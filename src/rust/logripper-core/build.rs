//! Build script for generating protobuf bindings and compiling the DSP C library.

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = PathBuf::from("../../../proto");
    println!("cargo::rerun-if-changed={}", proto_root.display());

    let protos = &[
        proto_root.join("domain/callsign.proto"),
        proto_root.join("domain/station.proto"),
        proto_root.join("domain/qso.proto"),
        proto_root.join("domain/lookup.proto"),
        proto_root.join("services/debug_control_service.proto"),
        proto_root.join("services/lookup_service.proto"),
        proto_root.join("services/logbook_service.proto"),
        proto_root.join("services/setup_service.proto"),
        proto_root.join("services/station_profile_service.proto"),
    ];

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(protos, &[&proto_root])?;

    let dsp_root = PathBuf::from("../../../src/c/logripper-dsp");
    println!("cargo::rerun-if-changed={}", dsp_root.display());

    cc::Build::new()
        .file(dsp_root.join("src/dsp.c"))
        .include(dsp_root.join("include"))
        .warnings(true)
        .compile("logripper_dsp");

    Ok(())
}

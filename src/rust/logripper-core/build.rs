//! Build script for generating protobuf bindings and compiling the DSP C library.

use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = PathBuf::from("../../../proto");
    println!("cargo::rerun-if-changed={}", proto_root.display());

    let mut protos = Vec::new();
    collect_proto_files(&proto_root.join("domain"), &mut protos)?;
    collect_proto_files(&proto_root.join("services"), &mut protos)?;
    protos.sort();

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&protos, &[&proto_root])?;

    let dsp_root = PathBuf::from("../../../src/c/logripper-dsp");
    println!("cargo::rerun-if-changed={}", dsp_root.display());

    cc::Build::new()
        .file(dsp_root.join("src/dsp.c"))
        .include(dsp_root.join("include"))
        .warnings(true)
        .compile("logripper_dsp");

    Ok(())
}

fn collect_proto_files(
    directory: &Path,
    protos: &mut Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_proto_files(path.as_path(), protos)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("proto") {
            protos.push(path);
        }
    }

    Ok(())
}

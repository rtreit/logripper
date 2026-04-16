//! Build script for generating protobuf bindings, staging shared assets, and compiling the DSP C library.

use std::{
    fs,
    path::{Path, PathBuf},
};

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

    let dsp_root = PathBuf::from("../../../src/c/qsoripper-dsp");
    println!("cargo::rerun-if-changed={}", dsp_root.display());

    cc::Build::new()
        .file(dsp_root.join("src/dsp.c"))
        .include(dsp_root.join("include"))
        .warnings(true)
        .compile("qsoripper_dsp");

    let shared_adif_root = PathBuf::from("../../../shared/adif-data");
    println!("cargo::rerun-if-changed={}", shared_adif_root.display());
    stage_shared_asset(&shared_adif_root, "dxcc_entities.tsv")?;
    stage_shared_asset(&shared_adif_root, "submode_aliases.tsv")?;

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

fn stage_shared_asset(
    source_root: &Path,
    file_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    fs::copy(source_root.join(file_name), out_dir.join(file_name))?;
    Ok(())
}

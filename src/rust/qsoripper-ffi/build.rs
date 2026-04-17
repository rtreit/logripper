//! Build script that generates the C header via cbindgen.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR")?;

    let config = cbindgen::Config::from_file("cbindgen.toml")?;

    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()?
        .write_to_file("qsoripper_ffi.h");

    Ok(())
}

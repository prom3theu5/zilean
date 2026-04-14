//! Phase 4: build script switched from tonic-build (gRPC service stubs)
//! to prost-build only. Zilean no longer exposes a gRPC transport; the
//! `proto` module now just holds the generated message types which the
//! DMM page parser and IMDb ingestor consume as domain-ish structs.

use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let descriptor_path = out_dir.join("descriptor.bin");

    let mut config = prost_build::Config::new();
    config.file_descriptor_set_path(&descriptor_path);

    config.compile_protos(&["../Protos/zilean_rust.proto"], &["../Protos"])?;

    Ok(())
}

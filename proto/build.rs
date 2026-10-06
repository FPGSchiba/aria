use std::path::PathBuf;

/// Schemas to compile, relative to the include root. Layout per D87:
/// `aria/<service>/v1/<service>.proto`, matching `package aria.<service>.v1;`.
const PROTOS: &[&str] = &[
    "aria/gateway/v1/gateway.proto",
    "aria/agent_core/v1/agent_core.proto",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Absolute, so the include root never depends on the build script's working directory.
    // prost-build silently drops an include path that does not exist.
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let protos: Vec<PathBuf> = PROTOS.iter().map(|p| root.join(p)).collect();

    let mut config = prost_build::Config::new();
    config.protoc_executable(protoc_bin_vendored::protoc_bin_path()?);

    // The encoded descriptors, for gRPC server reflection; `include_imports` is on by default.
    let descriptor_path = PathBuf::from(std::env::var("OUT_DIR")?).join("aria_descriptor.bin");
    tonic_prost_build::configure()
        .file_descriptor_set_path(descriptor_path)
        .compile_with_config(config, &protos, std::slice::from_ref(&root))?;

    for proto in &protos {
        println!("cargo:rerun-if-changed={}", proto.display());
    }

    Ok(())
}

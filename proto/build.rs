use std::env;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // SAFETY: build.rs is single-threaded at this stage, so mutating the environment is safe.
    unsafe { env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?) };

    tonic_prost_build::configure()
        .compile_protos(
            &["gateway/v1/gateway.proto", "agent-core/v1/agent_core.proto"],
            &["proto"],
        )
        .expect("Failed to compile protobuf schemas");

    println!("cargo:rerun-if-changed=gateway/v1/gateway.proto");
    println!("cargo:rerun-if-changed=agent-core/v1/agent_core.proto");

    Ok(())
}

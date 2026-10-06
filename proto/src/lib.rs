//! Generated gRPC stubs and Prost message types for ARIA services.

/// The encoded file descriptor set of every ARIA proto and its imports, for gRPC server
/// reflection.
pub const FILE_DESCRIPTOR_SET: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/aria_descriptor.bin"));

pub mod gateway {
    pub mod v1 {
        tonic::include_proto!("aria.gateway.v1");
    }
}

pub mod agent_core {
    pub mod v1 {
        tonic::include_proto!("aria.agent_core.v1");
    }
}

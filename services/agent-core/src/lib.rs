//! The ARIA Agent Core: serves the streaming `Decide` call, keeps per-session
//! conversation history, and delegates text generation to a pluggable LLM backend.

/// A boxed error cause, so a failure can carry whatever error produced it.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// This crate's name as the tracing target of its spans. The binary passes it to the telemetry
/// setup, so the export filter keeps this crate's spans.
pub const CRATE_NAME: &str = env!("CARGO_CRATE_NAME");

pub mod backend;
pub mod config;
pub mod conversation;
pub mod health;
pub mod history;
pub mod service;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_of_this_crate_carry_the_exported_target() {
        // `decide` and the other spans live in modules of this crate, so their target starts with
        // the crate's name; the export filter keeps exactly that prefix.
        assert!(module_path!().starts_with(CRATE_NAME));
    }
}

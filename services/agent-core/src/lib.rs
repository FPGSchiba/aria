//! The ARIA Agent Core: serves the streaming `Decide` call, keeps per-session
//! conversation history, and delegates text generation to a pluggable LLM backend.

/// A boxed error cause, so a failure can carry whatever error produced it.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub mod backend;
pub mod config;
pub mod conversation;
pub mod history;
pub mod service;
pub mod telemetry;

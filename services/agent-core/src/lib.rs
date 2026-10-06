//! The ARIA Agent Core: serves the streaming `Decide` call, keeps per-session
//! conversation history, and delegates text generation to a pluggable LLM backend.

pub mod backend;
pub mod config;
pub mod conversation;
pub mod history;
pub mod service;
pub mod telemetry;

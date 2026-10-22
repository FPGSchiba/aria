//! The openai backend: streams a conversation through an OpenAI-compatible Chat Completions
//! provider (D85) and turns its events into ARIA chunks, enforcing the idle timeout.

use crate::backend::{Backend, BackendError, Chunk};
use crate::config::Config;
use crate::conversation::ConversationPart;
use crate::health::BackendHealth;
use async_openai::config::{OpenAIConfig};
use futures_core::Stream;
use std::pin::Pin;
use std::time::Duration;
use async_openai::Client;
use async_openai::middleware::ReqwestService;
use secrecy::ExposeSecret;
use tokio::sync::watch;
use tokio::sync::watch::Sender;
use tower::retry::RetryLayer;
use crate::backend::openai::retry::RetryPolicy;

pub mod error;
#[cfg(feature = "test-util")]
pub mod fake_provider;
pub(crate) mod request;
pub(crate) mod retry;
pub(crate) mod spans;

/// An OpenAI-compatible backend that streams a conversation through a Chat Completions provider
/// and turns its events into ARIA chunks, enforcing the idle timeout.
#[derive(Debug, Clone)]
pub struct OpenAIBackend {
    client: async_openai::Client<OpenAIConfig>,
    model: String,
    idle_timeout: Duration,
    health_sender: Sender<BackendHealth>,
}

/// An error that can occur while creating an OpenAIBackend.
#[derive(Debug, thiserror::Error)]
pub enum OpenAIBackendError {
    /// The HTTP client could not be built, for example because the TLS backend failed to initialise.
    #[error("failed to build the HTTP client: {0}")]
    ClientCreationFailed(#[from] reqwest::Error),
}

impl OpenAIBackend {
    const NAME: &'static str = "openai";

    /// Creates a new OpenAIBackend from the given configuration.
    /// Fails with [`OpenAIBackendError::ClientCreationFailed`] if the HTTP client cannot be built.
    pub fn new(config: &Config) -> Result<Self, OpenAIBackendError> {
        Self::build(config, retry::BACKOFF_BASE)
    }

    /// For tests: like [`OpenAIBackend::new`] but with an explicit backoff base, so retry tests
    /// run without real backoff delays.
    #[cfg(any(test, feature = "test-util"))]
    pub fn with_backoff_base(
        config: &Config,
        backoff_base: Duration,
    ) -> Result<Self, OpenAIBackendError> {
        Self::build(config, backoff_base)
    }

    /// The one build path: assembles the HTTP client and tower stack with the given backoff base.
    fn build(config: &Config, backoff_base: Duration) -> Result<Self, OpenAIBackendError> {
        let openai_config = OpenAIConfig::new()
            .with_api_base(config.llm_url().as_str())
            .with_api_key(config.llm_api_key().expose_secret());

        let http_client = reqwest::Client::builder().build()?;

        let health = BackendHealth::new(Self::NAME, true, false);
        let health_sender = watch::channel(health).0;

        let stack = tower::ServiceBuilder::new()
            .layer(spans::StatusCaptureLayer)                       // outermost
            .layer(RetryLayer::new(RetryPolicy::new(config.llm_max_retries(), backoff_base)))
            .layer(spans::AttemptTracingLayer)
            .service(ReqwestService::new(http_client));             // innermost

        let client = Client::with_config(openai_config).with_http_service(stack);

        Ok(Self {
            client,
            model: config.llm_model().to_owned(),
            idle_timeout: config.llm_idle_timeout(),
            health_sender,
        })
    }
}

impl Backend for OpenAIBackend {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn subscribe_health(&self) -> watch::Receiver<BackendHealth> {
        self.health_sender.subscribe()
    }

    /// Streams a conversation to the OpenAI-compatible backend, returning a stream of chunks.
    /// The stream ends cleanly when the backend finishes, or with an error if the backend fails.
    /// The idle timeout covers opening the stream (including retries) and every gap between
    /// provider events. Any provider event resets it, including reasoning deltas that are never
    /// emitted as chunks. A stall with no events within the window ends the stream with a timeout
    /// error.
    fn stream_conversation(
        &self,
        history: Vec<ConversationPart>,
        input: Vec<ConversationPart>,
    ) -> Pin<Box<dyn Stream<Item = Result<Chunk, BackendError>> + Send + 'static>> {
        todo!()
    }
}

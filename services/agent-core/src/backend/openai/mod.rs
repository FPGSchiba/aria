//! The openai backend: streams a conversation through an OpenAI-compatible Chat Completions
//! provider (D85) and turns its events into ARIA chunks, enforcing the idle timeout.

use crate::backend::openai::error::{
    CompletionError, ErrorObject, GenerationFailure, connection_failed,
    map_completion_error_to_availability, parse_error_object,
};
use crate::backend::openai::provider_config::ProviderConfig;
use crate::backend::openai::request::map_conversation_to_request;
use crate::backend::openai::retry::RetryPolicy;
use crate::backend::{Backend, BackendError, Chunk};
use crate::config::Config;
use crate::conversation::ConversationPart;
use crate::health::{BackendHealth, HostStatus};
use async_openai::Client;
use async_openai::error::OpenAIError;
use async_openai::middleware::ReqwestService;
use async_openai::types::chat::{CreateChatCompletionStreamResponse, FinishReason};
use futures_core::Stream;
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::watch;
use tokio::sync::watch::Sender;
use tokio_stream::StreamExt;
use tower::retry::RetryLayer;
use tracing::{Instrument, Span};

pub mod error;
#[cfg(feature = "test-util")]
pub mod fake_provider;
pub(crate) mod provider_config;
pub(crate) mod request;
pub(crate) mod retry;
pub(crate) mod spans;
pub(crate) mod status;
#[cfg(test)]
pub(crate) mod test_support;

/// An OpenAI-compatible backend that streams a conversation through a Chat Completions provider
/// and turns its events into ARIA chunks, enforcing the idle timeout.
#[derive(Debug, Clone)]
pub struct OpenAIBackend {
    client: Client<ProviderConfig>,
    model: String,
    idle_timeout: Duration,
    /// The provider's host and port, recorded on every call span as `server.address` and
    /// `server.port`.
    server_address: String,
    server_port: Option<u16>,
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
        // Built from the ARIA_LLM_* settings alone, never from the OPENAI_* environment. The root
        // slash of the base URL is dropped too (a bare host is `http://host/`), or the route
        // would be requested as `//chat/completions`.
        let provider_config = ProviderConfig::new(
            config.llm_url().as_str().trim_end_matches('/'),
            config.llm_api_key().clone(),
        );

        let http_client = reqwest::Client::builder().build()?;

        let health = BackendHealth::new(Self::NAME, true, false);
        let health_sender = watch::channel(health).0;

        let stack = tower::ServiceBuilder::new()
            .layer(status::StatusCaptureLayer) // outermost
            .layer(RetryLayer::new(RetryPolicy::new(
                config.llm_max_retries(),
                backoff_base,
            )))
            .layer(spans::AttemptTracingLayer)
            .service(ReqwestService::new(http_client)); // innermost

        let client = Client::with_config(provider_config).with_http_service(stack);

        Ok(Self {
            client,
            model: config.llm_model().to_owned(),
            idle_timeout: config.llm_idle_timeout(),
            server_address: config.llm_host().to_string(),
            server_port: config.llm_port(),
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
    /// provider events. Every parsed event resets it, including reasoning deltas that are never
    /// emitted as chunks; SSE keep-alive comments do not, because the crate swallows them. A stall
    /// with no parsed event within the window ends the stream with a timeout error. Once the
    /// provider has said it finished, a timeout or a later error (such as an unreadable usage
    /// chunk) no longer fails the reply: it is complete.
    fn stream_conversation(
        &self,
        history: Vec<ConversationPart>,
        input: Vec<ConversationPart>,
    ) -> Pin<Box<dyn Stream<Item = Result<Chunk, BackendError>> + Send + 'static>> {
        let client = self.client.clone();
        let model = self.model.clone();
        let idle_timeout_dur = self.idle_timeout;
        let health_sender = self.health_sender.clone();
        let server_address = self.server_address.clone();
        let server_port = self.server_port;

        Box::pin(async_stream::stream! {
            // Created on the first poll, which the service makes inside the `decide` span, so the
            // call span is its child. It lives as long as this stream, and closes with it.
            let call_span = tracing::info_span!(
                "chat",
                "otel.name" = %format!("chat {model}"),
                "gen_ai.operation.name" = "chat",
                "gen_ai.provider.name" = "openai",
                "server.address" = %server_address,
                "server.port" = tracing::field::Empty,
                "gen_ai.request.model" = %model,
                "gen_ai.response.model" = tracing::field::Empty,
                "gen_ai.response.finish_reasons" = tracing::field::Empty,
                "gen_ai.usage.input_tokens" = tracing::field::Empty,
                "gen_ai.usage.output_tokens" = tracing::field::Empty,
                "error.type" = tracing::field::Empty,
                "otel.status_code" = tracing::field::Empty,
            );
            if let Some(port) = server_port {
                call_span.record("server.port", u64::from(port));
            }
            let call = Call { span: call_span.clone(), health: health_sender };
            let request = map_conversation_to_request(&history, &input, &model);

            // One idle window: it starts with the call, so opening (with its retries) counts as
            // waiting for the first event, and every event after that restarts it.
            let idle_timeout = tokio::time::sleep(idle_timeout_dur);
            tokio::pin!(idle_timeout);

            let chat = client.chat();
            let opened = tokio::select! {
                opened = chat.create_stream(request).instrument(call_span.clone()) => Some(opened),
                _ = &mut idle_timeout => None,
            };
            let mut stream = match opened {
                Some(Ok(stream)) => stream,
                Some(Err(e)) => {
                    yield Err(call.fail(classify(e, false)));
                    return;
                }
                None => {
                    yield Err(call.fail(CompletionError::Timeout));
                    return;
                }
            };

            let mut finished = false;
            let mut produced_text = false;
            let mut first_event = true;

            loop {
                let next = tokio::select! {
                    maybe_event = stream.next() => Some(maybe_event),
                    _ = &mut idle_timeout => None,
                };
                match next {
                    Some(Some(Ok(event))) => {
                        idle_timeout.as_mut().reset(tokio::time::Instant::now() + idle_timeout_dur);
                        record_event(&call_span, &event, first_event);
                        first_event = false;
                        for choice in event.choices {
                            if choice.delta.tool_calls.is_some_and(|calls| !calls.is_empty()) {
                                yield Err(call.fail(CompletionError::UnexpectedToolCall));
                                return;
                            }
                            if let Some(content) = choice.delta.content.filter(|c| !c.is_empty()) {
                                if !produced_text {
                                    produced_text = true;
                                    call_span.in_scope(|| tracing::info!("first text chunk"));
                                }
                                yield Ok(Chunk::Text { text: content });
                            }
                            let Some(reason) = choice.finish_reason else { continue };
                            call_span.record("gen_ai.response.finish_reasons", finish_reason_name(&reason));
                            match reason {
                                FinishReason::Stop => {
                                    finished = true;
                                }
                                // Length: an error only if no text came, else the reply ends here.
                                FinishReason::Length => {
                                    if produced_text {
                                        finished = true;
                                    } else {
                                        yield Err(call.fail(CompletionError::GenerationFailed(
                                            GenerationFailure::LengthLimit,
                                        )));
                                        return;
                                    }
                                }
                                // The content filter always makes the reply an error, even after text.
                                FinishReason::ContentFilter => {
                                    yield Err(call.fail(CompletionError::GenerationFailed(
                                        GenerationFailure::ContentFilter,
                                    )));
                                    return;
                                }
                                FinishReason::ToolCalls | FinishReason::FunctionCall => {
                                    yield Err(call.fail(CompletionError::UnexpectedToolCall));
                                    return;
                                }
                            }
                        }
                    }
                    Some(Some(Err(e))) => {
                        // The reply is complete once the provider finished; what breaks after that
                        // (a transport error, an unreadable usage chunk) does not undo it.
                        if finished {
                            call.succeed();
                        } else {
                            yield Err(call.fail(classify(e, true)));
                        }
                        return;
                    }
                    // The stream ended: complete only if the provider said it finished.
                    Some(None) => {
                        if finished {
                            call.succeed();
                        } else {
                            yield Err(call.fail(CompletionError::StreamError {
                                description: "ended without a finish reason".to_string(),
                                cause: None,
                                provider_code: None,
                            }));
                        }
                        return;
                    }
                    // Nothing within the window; a reply that had finished is still complete.
                    None => {
                        if finished {
                            call.succeed();
                        } else {
                            yield Err(call.fail(CompletionError::Timeout));
                        }
                        return;
                    }
                }
            }
        })
    }
}

/// What one call shares between its exits: the span to record on and the health to publish to.
struct Call {
    span: Span,
    health: Sender<BackendHealth>,
}

impl Call {
    /// The single error exit: records the error's kind on the span, publishes the availability it
    /// implies, and returns the error the stream yields (its cause stays downcastable).
    fn fail(&self, error: CompletionError) -> BackendError {
        self.span.record("error.type", error.error_type());
        self.span.record("otel.status_code", "ERROR");
        self.publish(Some(&error));
        BackendError::unexpected(error)
    }

    /// The success exit: the provider answered and the reply completed.
    fn succeed(&self) {
        self.publish(None);
    }

    /// Updates the host status once for this call. `send_if_modified` changes it in place, so no
    /// borrow is held across the write.
    fn publish(&self, error: Option<&CompletionError>) {
        self.health.send_if_modified(|health| {
            let update = match error {
                Some(error) => map_completion_error_to_availability(error, health.host_status()),
                None if *health.host_status() == HostStatus::Available => None,
                None => Some(HostStatus::Available),
            };
            match update {
                Some(status) => {
                    *health = health.with_host_status(status);
                    true
                }
                None => false,
            }
        });
    }
}

/// Maps an error from the open (`streaming == false`) or from a later event to our own.
fn classify(error: OpenAIError, streaming: bool) -> CompletionError {
    // The crate reports an error event in the stream as an event it could not read, with the raw
    // data; if that data is an error object, it is the provider's own error.
    if streaming
        && let OpenAIError::JSONDeserialize(_, content) = &error
        && let Some(ErrorObject {
            message: Some(message),
            code,
        }) = parse_error_object(content)
    {
        return CompletionError::StreamError {
            description: format!("provider sent an error event: {message}"),
            cause: None,
            provider_code: code,
        };
    }
    match error {
        // The status-capture layer boxes our own error; give it back.
        OpenAIError::Boxed(boxed) => match boxed.downcast::<CompletionError>() {
            Ok(error) => *error,
            Err(other) => CompletionError::UnexpectedError(other),
        },
        // The request got no response: refused, reset, or closed without an answer.
        OpenAIError::Reqwest(e) if connection_failed(&e) => {
            CompletionError::ConnectionError(Box::new(e))
        }
        other if streaming => CompletionError::StreamError {
            description: "the stream broke".to_string(),
            cause: Some(Box::new(other)),
            provider_code: None,
        },
        other => CompletionError::UnexpectedError(Box::new(other)),
    }
}

fn finish_reason_name(reason: &FinishReason) -> &'static str {
    match reason {
        FinishReason::Stop => "stop",
        FinishReason::Length => "length",
        FinishReason::ContentFilter => "content_filter",
        FinishReason::ToolCalls => "tool_calls",
        FinishReason::FunctionCall => "function_call",
    }
}

/// Records what an event reports on the call span: the response model (from the first event) and
/// the token counts (from the usage event). Never any content.
fn record_event(span: &Span, event: &CreateChatCompletionStreamResponse, first_event: bool) {
    if first_event {
        span.record("gen_ai.response.model", event.model.as_str());
    }
    if let Some(usage) = &event.usage {
        span.record("gen_ai.usage.input_tokens", u64::from(usage.prompt_tokens));
        span.record(
            "gen_ai.usage.output_tokens",
            u64::from(usage.completion_tokens),
        );
    }
}

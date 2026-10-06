//! The `AgentCore` gRPC service.
//! Validates the request, reserves the session, streams backend chunks to the caller,
//! commits the exchange on clean finish, and maps failures to gRPC statuses and telemetry.

use crate::backend::{Backend, BackendError, Chunk};
use crate::history::{Actor, Content, ConversationPart, HistoryError, HistoryStore};
use async_stream::try_stream;
use proto::agent_core::v1::agent_core_server::AgentCore;
use proto::agent_core::v1::{DecideRequest, DecideResponse};
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status};
use tracing::{Instrument, Span};

/// Why a `Decide` call failed. Converts to a gRPC `Status` with generic client texts.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    /// The backend failed while producing the reply.
    #[error("streaming failed in the backend: {0}")]
    StreamError(#[from] BackendError),
    /// The conversation store refused or failed the operation.
    #[error("conversation store error: {0}")]
    Store(#[from] HistoryError),
    /// A request field was missing or empty.
    #[error("invalid input in field: {field}")]
    InvalidInput { field: &'static str },
}

impl From<ServiceError> for Status {
    fn from(err: ServiceError) -> Self {
        match err {
            ServiceError::StreamError(e) => match e {
                BackendError::UnexpectedError => Status::unavailable("Unexpected error in backend"),
            },
            ServiceError::Store(e) => match e {
                HistoryError::TurnInFlight => {
                    Status::aborted("Session is busy with another request")
                }
                HistoryError::StoreUnavailable => {
                    Status::unavailable("Session store is unavailable")
                }
                HistoryError::InvalidTurnOwner => {
                    Status::unavailable("Session store is unavailable")
                }
            },
            ServiceError::InvalidInput { field } => {
                Status::invalid_argument(format!("Invalid input in field: {}", field))
            }
        }
    }
}

/// The Agent Core service, generic over its LLM backend and conversation store.
#[allow(dead_code)] // Phase 4: remove once decide reads backend and history
pub struct Service<B: Backend, H: HistoryStore> {
    backend: B,
    history: H,
}

impl<B: Backend, H: HistoryStore> Service<B, H> {
    /// Creates a service serving replies from `backend` and keeping conversations in `history`.
    ///
    /// To be served, `history` must be a cheaply cloneable handle (`Clone`): a clone moves into
    /// the response stream.
    pub fn new(backend: B, history: H) -> Self {
        Self { backend, history }
    }
}

/// Builds a service from the default backend and the default (empty) store.
impl<B, H> Default for Service<B, H>
where
    B: Backend + Default,
    H: HistoryStore + Default,
{
    fn default() -> Self {
        Self {
            backend: B::default(),
            history: H::default(),
        }
    }
}

#[tonic::async_trait]
impl<B: Backend + 'static, H: HistoryStore + Clone + 'static> AgentCore for Service<B, H> {
    type DecideStream = std::pin::Pin<
        Box<dyn futures_core::Stream<Item = Result<DecideResponse, Status>> + Send + 'static>,
    >;

    async fn decide(
        &self,
        request: Request<DecideRequest>,
    ) -> Result<Response<Self::DecideStream>, Status> {
        let id = request.get_ref().session_id.clone();
        let span = tracing::info_span!("decide", session_id = %id, "otel.status_code" = tracing::field::Empty);
        let request = request.into_inner();
        span.in_scope(|| {
            tracing::info!(
                "Processing decide request for session: {}",
                request.session_id
            )
        });

        if request.session_id.is_empty() {
            report_failure(&span, &id, "invalid_input", &"session_id is empty");
            return Err(ServiceError::InvalidInput {
                field: "session_id",
            }
            .into());
        }
        if request.text.is_empty() {
            report_failure(&span, &id, "invalid_input", &"text is empty");
            return Err(ServiceError::InvalidInput { field: "text" }.into());
        }

        span.in_scope(
            || tracing::info!(outcome = "valid_input", session_id = %id, "Valid input received"),
        );
        let (token, conversation) = self
            .history
            .start_turn(&request.session_id)
            .instrument(span.clone())
            .await
            .map_err(ServiceError::Store)?;
        let input = vec![ConversationPart {
            content: Content::Text { text: request.text },
            actor: Actor::User,
        }];
        let mut stream = self
            .backend
            .stream_conversation(conversation, input.clone());

        let session_id = request.session_id.clone();
        // Owned handles for the `'static` response stream: no `self` inside the block.
        let history = self.history.clone();
        let stream_span = span.clone();

        let s = try_stream! {
            let mut cancel_guard = CancelGuard::new(stream_span.clone(), session_id.clone());
            let mut chunks: Vec<Chunk> = Vec::new();
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(chunk) => {
                        chunks.push(chunk.clone());
                        match chunk {
                            Chunk::Text { text } => {
                                tracing::debug!(outcome = "streaming", session_id = %session_id, "Streaming text chunk to client");
                                yield DecideResponse {
                                    payload: Some(proto::agent_core::v1::decide_response::Payload::TextDelta(text)),
                                };
                            }
                        }
                    }
                    Err(e) => {
                        cancel_guard.disarm();
                        report_failure(&stream_span, &session_id, "backend_error", &e);
                        Err(ServiceError::StreamError(e))?;
                    }
                }
            }
            tracing::info!(outcome = "committing", session_id = %session_id, "Committing conversation");
            let mut conv = input.clone();
            for chunk in chunks {
                push_conversation(&mut conv, chunk);
            }
            if let Err(e) = history.commit(token, conv).await {
                cancel_guard.disarm();
                report_failure(&stream_span, &session_id, "commit_failed", &e);
                Err(ServiceError::Store(e))?;
            }
            cancel_guard.disarm();
            yield DecideResponse { payload: Some(proto::agent_core::v1::decide_response::Payload::TurnComplete(Default::default())) };
        };

        let stream: Self::DecideStream = Box::pin(s);
        Ok(Response::new(Box::pin(InSpan {
            span,
            inner: stream,
        })))
    }
}

/// Appends a backend chunk to the conversation as assistant output.
///
/// Text extends the last part when that is assistant text, so a streamed reply coalesces into one
/// part; otherwise (including right after the user's part) it starts a new assistant part.
fn push_conversation(conv: &mut Vec<ConversationPart>, chunk: Chunk) {
    match chunk {
        Chunk::Text { text } => match conv.last_mut() {
            Some(ConversationPart {
                content: Content::Text { text: last },
                actor: Actor::Assistant,
            }) => last.push_str(&text),
            _ => conv.push(ConversationPart {
                content: Content::Text { text },
                actor: Actor::Assistant,
            }),
        },
    }
}

/// Marks the `decide` span as failed and logs the failure inside it, naming the outcome and
/// the session.
fn report_failure(
    span: &Span,
    session_id: &str,
    outcome: &'static str,
    detail: &dyn std::fmt::Display,
) {
    span.record("otel.status_code", "ERROR");
    span.in_scope(|| {
        tracing::error!(outcome, session_id = %session_id, error = %detail, "Decide turn failed");
    });
}

/// Reports a turn that is dropped before it finishes (the client cancelled the stream).
/// Owned by the response stream, so dropping the stream drops it; `disarm` it once the turn has
/// ended on its own, whether cleanly or with an already-reported error.
struct CancelGuard {
    span: Span,
    session_id: String,
    armed: bool,
}

impl CancelGuard {
    fn new(span: Span, session_id: String) -> Self {
        Self {
            span,
            session_id,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for CancelGuard {
    fn drop(&mut self) {
        if self.armed {
            report_failure(
                &self.span,
                &self.session_id,
                "cancelled",
                &"client cancelled the stream",
            );
        }
    }
}

/// Polls the inner stream inside `span`, so everything a poll does belongs to it.
struct InSpan {
    span: Span,
    inner: std::pin::Pin<
        Box<dyn futures_core::Stream<Item = Result<DecideResponse, Status>> + Send + 'static>,
    >,
}

impl futures_core::Stream for InSpan {
    type Item = Result<DecideResponse, Status>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let _entered = this.span.enter();
        this.inner.as_mut().poll_next(cx)
    }
}

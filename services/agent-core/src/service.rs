//! The `AgentCore` gRPC service.
//! Validates the request, reserves the session, streams backend chunks to the caller,
//! commits the exchange on clean finish, and maps failures to gRPC statuses and telemetry.

use crate::backend::{Backend, BackendError, Chunk};
use crate::history::{Actor, Content, ConversationPart, HistoryError, HistoryStore};
use async_stream::try_stream;
use futures_core::Stream;
use proto::agent_core::v1::agent_core_server::AgentCore;
use proto::agent_core::v1::{DecideRequest, DecideResponse};
use std::pin::Pin;
use std::task::{Context, Poll};
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
                BackendError::UnexpectedError => Status::internal("Unexpected error in backend"),
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
    type DecideStream =
        Pin<Box<dyn Stream<Item = Result<DecideResponse, Status>> + Send + 'static>>;

    async fn decide(
        &self,
        request: Request<DecideRequest>,
    ) -> Result<Response<Self::DecideStream>, Status> {
        let DecideRequest {
            session_id, text, ..
        } = request.into_inner();
        let span = tracing::info_span!("decide", session_id = %session_id, "otel.status_code" = tracing::field::Empty);
        span.in_scope(|| tracing::info!("Processing decide request for session: {}", session_id));

        if session_id.is_empty() {
            report_failure(&span, &session_id, "invalid_input", &"session_id is empty");
            return Err(ServiceError::InvalidInput {
                field: "session_id",
            }
            .into());
        }
        if text.is_empty() {
            report_failure(&span, &session_id, "invalid_input", &"text is empty");
            return Err(ServiceError::InvalidInput { field: "text" }.into());
        }

        span.in_scope(
            || tracing::info!(stage = "valid_input", session_id = %session_id, "Valid input received"),
        );

        let (token, history_so_far) = match self
            .history
            .start_turn(&session_id)
            .instrument(span.clone())
            .await
        {
            Ok(started) => started,
            Err(e) => {
                report_failure(&span, &session_id, "store_error", &e);
                return Err(ServiceError::Store(e).into());
            }
        };

        // The conversation as it will be committed: the user's part now, the reply as it arrives.
        let mut conversation = vec![ConversationPart {
            content: Content::Text { text },
            actor: Actor::User,
        }];
        let mut backend_stream = self
            .backend
            .stream_conversation(history_so_far, conversation.clone());

        // Owned handles for the `'static` response stream: no `self` inside the block.
        let history = self.history.clone();
        let stream_span = span.clone();
        // Created here, not in the block, so a stream dropped before its first poll still reports.
        let mut cancel_guard = CancelGuard::new(span.clone(), session_id.clone());

        let reply = try_stream! {
            while let Some(chunk) = backend_stream.next().await {
                match chunk {
                    Ok(chunk) => {
                        push_conversation(&mut conversation, &chunk);
                        match chunk {
                            Chunk::Text { text } => {
                                tracing::trace!(stage = "streaming", session_id = %session_id, "Streaming text chunk to client");
                                yield DecideResponse {
                                    payload: Some(proto::agent_core::v1::decide_response::Payload::TextDelta(text)),
                                };
                            }
                        }
                    }
                    Err(e) => {
                        cancel_guard.finish();
                        report_failure(&stream_span, &session_id, "backend_error", &e);
                        Err(ServiceError::StreamError(e))?;
                    }
                }
            }
            tracing::info!(stage = "committing", session_id = %session_id, "Committing conversation");
            cancel_guard.committing();
            if let Err(e) = history.commit(token, conversation).await {
                cancel_guard.finish();
                report_failure(&stream_span, &session_id, "commit_failed", &e);
                Err(ServiceError::Store(e))?;
            }
            cancel_guard.finish();
            yield DecideResponse { payload: Some(proto::agent_core::v1::decide_response::Payload::TurnComplete(Default::default())) };
        };

        Ok(Response::new(Box::pin(InSpan {
            span,
            inner: Box::pin(reply),
        })))
    }
}

/// Appends a backend chunk to the conversation as assistant output.
///
/// Text extends the last part when that is assistant text, so a streamed reply coalesces into one
/// part; otherwise (including right after the user's part) it starts a new assistant part.
fn push_conversation(conv: &mut Vec<ConversationPart>, chunk: &Chunk) {
    match chunk {
        Chunk::Text { text } => match conv.last_mut() {
            Some(ConversationPart {
                content: Content::Text { text: last },
                actor: Actor::Assistant,
            }) => last.push_str(text),
            _ => conv.push(ConversationPart {
                content: Content::Text { text: text.clone() },
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

/// Where a turn is when its `CancelGuard` is dropped.
enum TurnState {
    /// Still streaming the reply; a drop here is a client cancel.
    Streaming,
    /// Waiting on the store's commit; a drop here leaves the outcome of the commit unknown.
    Committing,
    /// The turn ended on its own, cleanly or with an already-reported error.
    Finished,
}

/// Reports a turn that is dropped before it finishes (the client cancelled the stream).
/// Owned by the response stream, so dropping the stream drops it, even before its first poll.
/// Call `committing` before awaiting the commit and `finish` once the turn has ended on its own.
struct CancelGuard {
    span: Span,
    session_id: String,
    state: TurnState,
}

impl CancelGuard {
    fn new(span: Span, session_id: String) -> Self {
        Self {
            span,
            session_id,
            state: TurnState::Streaming,
        }
    }

    fn committing(&mut self) {
        self.state = TurnState::Committing;
    }

    fn finish(&mut self) {
        self.state = TurnState::Finished;
    }
}

impl Drop for CancelGuard {
    fn drop(&mut self) {
        let (outcome, detail) = match self.state {
            TurnState::Streaming => ("cancelled", "client cancelled the stream"),
            // The exchange may or may not have been stored.
            TurnState::Committing => ("commit_interrupted", "client cancelled during the commit"),
            TurnState::Finished => return,
        };
        report_failure(&self.span, &self.session_id, outcome, &detail);
    }
}

/// Polls the inner stream inside `span`, so everything a poll does belongs to it.
struct InSpan<S> {
    span: Span,
    inner: Pin<Box<S>>,
}

impl<S: Stream> Stream for InSpan<S> {
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let _entered = this.span.enter();
        this.inner.as_mut().poll_next(cx)
    }
}

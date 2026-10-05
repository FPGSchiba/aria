//! The `AgentCore` gRPC service.
//! Validates the request, reserves the session, streams backend chunks to the caller,
//! commits the exchange on clean finish, and maps failures to gRPC statuses and telemetry.

use crate::backend::{Backend, BackendError};
use crate::history::{HistoryError, HistoryStore};
use proto::agent_core::v1::agent_core_server::AgentCore;
use proto::agent_core::v1::{DecideRequest, DecideResponse};
use tonic::{Request, Response, Status};

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
        _request: Request<DecideRequest>,
    ) -> Result<Response<Self::DecideStream>, Status> {
        todo!()
    }
}

//! The openai backend's own classified error, and how each outcome maps to an availability
//! update (or to no update).

use crate::BoxError;
use crate::health::{HostStatus, UnavailabilityReason};
use std::cmp::PartialEq;
use std::fmt::Display;

/// Why the provider finished a reply without producing text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationFailure {
    /// The provider's content filter stopped the reply (`content_filter`).
    ContentFilter,
    /// The reply hit the length limit before any text arrived (`length`).
    LengthLimit,
}

impl Display for GenerationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenerationFailure::ContentFilter => write!(f, "Content was filtered"),
            GenerationFailure::LengthLimit => write!(f, "Length limit reached"),
        }
    }
}

/// The OpenAI-compatible backend's classified failure. It records what happened, such as the raw
/// HTTP status and provider code, not what it means; interpretation is the job of
/// [`map_completion_error_to_availability`]. It is boxed as the cause of a `BackendError`.
#[derive(Debug, thiserror::Error)]
pub enum CompletionError {
    /// Nothing arrived within the idle timeout, while opening (including retries) or mid-stream.
    #[error("request timed out")]
    Timeout,
    /// The provider answered with a non-2xx status after retries.
    #[error("HTTP error: [http_code: {status}, code: {code:?}] {message}")]
    HttpError {
        /// The HTTP status of the answer.
        status: reqwest::StatusCode,
        /// The provider's error message.
        message: String,
        /// The provider's own error code (a stable string), if it sent one.
        code: Option<String>,
    },
    /// No connection could be made (DNS, refused, TLS); the cause is in the source chain.
    #[error("Failed to connect to the host")]
    ConnectionError(#[source] BoxError),
    /// The stream started and then failed to complete. Three situations:
    /// - the connection was cut or the data was unreadable: `cause` holds the underlying error;
    /// - the provider sent an error event mid-stream: `provider_code` holds its code and
    ///   `description` its message;
    /// - the stream ended without a finish reason (truncated): no cause and no code.
    #[error("Stream failed: {description}")]
    StreamError {
        /// What happened, in a short sentence, e.g. "ended without a finish reason".
        description: String,
        /// The underlying error, if the stream broke because of one.
        #[source]
        cause: Option<BoxError>,
        /// The provider's own error code (a stable string), if it sent an error event.
        provider_code: Option<String>,
    },
    /// The reply finished with `length` or `content_filter` and no text.
    #[error("Failed to generate a response: {0}")]
    GenerationFailed(GenerationFailure),
    /// A tool-call delta arrived although no tools are sent. Once tools are sent (ARIA-61) this
    /// becomes a call to a tool that was not in the request.
    #[error("Unexpected tool call")]
    UnexpectedToolCall,
    /// Anything else, mostly local (e.g. a request the client could not build); the provider was
    /// not reached.
    #[error("Unexpected error: {0}")]
    UnexpectedError(#[source] BoxError),
}

/// A result whose error is a [`CompletionError`].
pub type CompletionResult<T> = Result<T, CompletionError>;

/// Maps a failure to the availability update it implies, or `None` for no update.
///
/// Any answer from the provider means it is available, including ordinary 4xx statuses and
/// `GenerationFailed`. 401 and 403 mean the key was rejected, 402 payment is required, 429 the
/// provider rate-limited us and 5xx a server error. Timeout, connection and stream failures map to
/// their own reasons. There is no update when the provider was never reached (`UnexpectedError`)
/// and when the result equals `current`.
pub fn map_completion_error_to_availability(
    error: &CompletionError,
    current: &HostStatus,
) -> Option<HostStatus> {
    let new_status: HostStatus;
    match error {
        CompletionError::Timeout => {
            new_status = HostStatus::Unavailable(UnavailabilityReason::TimedOut)
        }
        CompletionError::HttpError {
            status,
            message: _,
            code,
        } => {
            match status.as_u16() {
                400 => new_status = HostStatus::Available,
                401 => {
                    new_status = HostStatus::Unavailable(UnavailabilityReason::KeyRejected {
                        host_code: code.to_owned(),
                    })
                }
                402 => {
                    new_status = HostStatus::Unavailable(UnavailabilityReason::PaymentRequired {
                        host_code: code.to_owned(),
                    })
                }
                403 => {
                    new_status = HostStatus::Unavailable(UnavailabilityReason::KeyRejected {
                        host_code: code.to_owned(),
                    })
                }
                404 => new_status = HostStatus::Available,
                413 => new_status = HostStatus::Available,
                422 => new_status = HostStatus::Available,
                429 => {
                    new_status = HostStatus::Unavailable(UnavailabilityReason::RateLimited {
                        host_code: code.to_owned(),
                    })
                }
                500..=599 => {
                    new_status = HostStatus::Unavailable(UnavailabilityReason::ServerError {
                        http_code: status.as_u16(),
                        host_code: code.to_owned(),
                    })
                }
                _ => {
                    // Any other answer from the provider counts as available
                    new_status = HostStatus::Available;
                }
            }
        }
        CompletionError::ConnectionError(_) => {
            new_status = HostStatus::Unavailable(UnavailabilityReason::Unreachable)
        }
        CompletionError::StreamError { provider_code, .. } => {
            new_status = HostStatus::Unavailable(UnavailabilityReason::StreamInterrupted {
                host_code: provider_code.to_owned(),
            })
        }
        CompletionError::GenerationFailed(_) => new_status = HostStatus::Available,
        CompletionError::UnexpectedToolCall => new_status = HostStatus::Available,
        CompletionError::UnexpectedError(_) => return None, // For unexpected errors, we don't change the status
    }

    if &new_status == current {
        None
    } else {
        Some(new_status)
    }
}

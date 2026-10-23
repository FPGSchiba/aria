//! The openai backend's own classified error, and how each outcome maps to an availability
//! update (or to no update).

use crate::BoxError;
use crate::health::{HostStatus, UnavailabilityReason};
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
    /// The provider answered with a non-2xx status after retries. The provider's message is
    /// provider text, so it is not part of this error: it goes to the debug log once, where the
    /// answer is parsed (see [`log_provider_message`]).
    #[error("HTTP error: [http_code: {status}, code: {code:?}]")]
    HttpError {
        /// The HTTP status of the answer.
        status: reqwest::StatusCode,
        /// The provider's own error code (a stable string), if it sent one.
        code: Option<String>,
    },
    /// The provider host could not be reached or did not answer: no connection could be made (DNS,
    /// refused, TLS), or the connection failed before a response arrived (reset, or closed without
    /// answering). The cause is in the source chain.
    #[error("Failed to connect to the host")]
    ConnectionError(#[source] BoxError),
    /// The stream started and then failed to complete. Three situations:
    /// - the connection was cut or the data was unreadable: `cause` holds the underlying error;
    /// - the provider sent an error event mid-stream: `provider_code` holds its code (its message
    ///   goes to the debug log, not into this error);
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
    /// A local failure (e.g. a request the client could not build), or an answer that says nothing
    /// about whether the provider can serve (e.g. a redirect loop). Either way availability is
    /// unchanged. A transport error after the request was sent is a `ConnectionError`, not this.
    #[error("Unexpected error: {0}")]
    UnexpectedError(#[source] BoxError),
}

/// Whether sending the request failed without any response: the connection could not be made,
/// was reset, or was closed before an answer. Redirect, decode, body, status and builder errors
/// are not this: the provider answered, or the request was never sent.
pub(crate) fn connection_failed(error: &reqwest::Error) -> bool {
    error.is_connect() || error.is_request()
}

/// The log target of the provider's own error messages. They are provider text, so they are kept
/// out of errors and spans: this target is only for stdout at debug level, and the telemetry
/// crate keeps it out of the span export.
pub(crate) const PROVIDER_MESSAGE_TARGET: &str = "agent_core::provider_message";

/// How many characters of a provider message are logged.
const PROVIDER_MESSAGE_MAX_CHARS: usize = 200;

/// Logs a provider's error message once, truncated, at debug level on [`PROVIDER_MESSAGE_TARGET`].
/// This is the only place provider text is rendered; errors carry status, code and kind only.
pub(crate) fn log_provider_message(message: &str, status: Option<u16>, code: Option<&str>) {
    tracing::debug!(
        target: PROVIDER_MESSAGE_TARGET,
        status,
        code,
        "provider message: {}",
        truncated(message, PROVIDER_MESSAGE_MAX_CHARS)
    );
}

/// `text` cut to `max_chars` characters, on a character boundary, with an ellipsis if it was cut.
fn truncated(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

/// The `error.type` of a failed call, as the name of the `CompletionError` case. One vocabulary
/// for the call span and the attempt spans.
pub(crate) mod error_type {
    pub(crate) const TIMEOUT: &str = "timeout";
    pub(crate) const HTTP_ERROR: &str = "http_error";
    pub(crate) const CONNECTION_ERROR: &str = "connection_error";
    pub(crate) const STREAM_ERROR: &str = "stream_error";
    pub(crate) const GENERATION_FAILED: &str = "generation_failed";
    pub(crate) const UNEXPECTED_TOOL_CALL: &str = "unexpected_tool_call";
    pub(crate) const UNEXPECTED_ERROR: &str = "unexpected_error";
}

impl CompletionError {
    /// The span's `error.type`: the name of this case.
    pub(crate) fn error_type(&self) -> &'static str {
        match self {
            CompletionError::Timeout => error_type::TIMEOUT,
            CompletionError::HttpError { .. } => error_type::HTTP_ERROR,
            CompletionError::ConnectionError(_) => error_type::CONNECTION_ERROR,
            CompletionError::StreamError { .. } => error_type::STREAM_ERROR,
            CompletionError::GenerationFailed(_) => error_type::GENERATION_FAILED,
            CompletionError::UnexpectedToolCall => error_type::UNEXPECTED_TOOL_CALL,
            CompletionError::UnexpectedError(_) => error_type::UNEXPECTED_ERROR,
        }
    }
}

/// The message and code of a provider's error object.
pub(crate) struct ErrorObject {
    /// The provider's message, if the object has one.
    pub(crate) message: Option<String>,
    /// The provider's own error code (a stable string), if it sent one.
    pub(crate) code: Option<String>,
}

/// Parses a provider error out of `data`: Cerebras's flat `{"message","type","param","code"}` or
/// OpenAI's wrapped `{"error": {..}}`. `None` if `data` is not JSON; a JSON value without a
/// message gives an `ErrorObject` with none, so each caller decides its own fallback.
pub(crate) fn parse_error_object(data: &str) -> Option<ErrorObject> {
    let value: serde_json::Value = serde_json::from_str(data).ok()?;
    let fields = value
        .get("error")
        .filter(|e| e.is_object())
        .unwrap_or(&value);
    let text = |name: &str| fields.get(name)?.as_str().map(str::to_string);
    Some(ErrorObject {
        message: text("message"),
        code: text("code"),
    })
}

/// Maps a failure to the availability update it implies, or `None` for no update.
///
/// Any answer from the provider means it is available, including ordinary 4xx statuses and
/// `GenerationFailed`. 401 and 403 mean the key was rejected, 402 payment is required, 429 the
/// provider rate-limited us and 5xx a server error. Timeout, connection and stream failures map to
/// their own reasons. There is no update for an `UnexpectedError` (a local failure, or an answer
/// that says nothing about the provider) and when the result equals `current`.
pub fn map_completion_error_to_availability(
    error: &CompletionError,
    current: &HostStatus,
) -> Option<HostStatus> {
    let new_status = match error {
        CompletionError::Timeout => HostStatus::Unavailable(UnavailabilityReason::TimedOut),
        CompletionError::HttpError { status, code, .. } => match status.as_u16() {
            401 | 403 => HostStatus::Unavailable(UnavailabilityReason::KeyRejected {
                host_code: code.to_owned(),
            }),
            402 => HostStatus::Unavailable(UnavailabilityReason::PaymentRequired {
                host_code: code.to_owned(),
            }),
            429 => HostStatus::Unavailable(UnavailabilityReason::RateLimited {
                host_code: code.to_owned(),
            }),
            500..=599 => HostStatus::Unavailable(UnavailabilityReason::ServerError {
                http_code: status.as_u16(),
                host_code: code.to_owned(),
            }),
            // Any other answer from the provider counts as available.
            _ => HostStatus::Available,
        },
        CompletionError::ConnectionError(_) => {
            HostStatus::Unavailable(UnavailabilityReason::Unreachable)
        }
        CompletionError::StreamError { provider_code, .. } => {
            HostStatus::Unavailable(UnavailabilityReason::StreamInterrupted {
                host_code: provider_code.to_owned(),
            })
        }
        CompletionError::GenerationFailed(_) | CompletionError::UnexpectedToolCall => {
            HostStatus::Available
        }
        // A local failure, or an answer that says nothing about the provider.
        CompletionError::UnexpectedError(_) => return None,
    };

    if &new_status == current {
        None
    } else {
        Some(new_status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;
    use std::error::Error;

    fn http(status: u16, code: Option<&str>) -> CompletionError {
        CompletionError::HttpError {
            status: StatusCode::from_u16(status).expect("a valid status"),
            code: code.map(str::to_string),
        }
    }

    fn stream_error(provider_code: Option<&str>) -> CompletionError {
        CompletionError::StreamError {
            description: "broke".to_string(),
            cause: None,
            provider_code: provider_code.map(str::to_string),
        }
    }

    fn unavailable(reason: UnavailabilityReason) -> HostStatus {
        HostStatus::Unavailable(reason)
    }

    /// Every starting status a classification can be asked about.
    fn every_current() -> Vec<HostStatus> {
        vec![
            HostStatus::Unknown,
            HostStatus::Available,
            unavailable(UnavailabilityReason::TimedOut),
            unavailable(UnavailabilityReason::Unreachable),
            unavailable(UnavailabilityReason::KeyRejected { host_code: None }),
        ]
    }

    #[test]
    fn a_timeout_makes_the_provider_unavailable_as_timed_out() {
        assert_eq!(
            map_completion_error_to_availability(&CompletionError::Timeout, &HostStatus::Unknown),
            Some(unavailable(UnavailabilityReason::TimedOut))
        );
    }

    #[test]
    fn a_connection_failure_makes_the_provider_unavailable_as_unreachable() {
        let error = CompletionError::ConnectionError("refused".into());

        assert_eq!(
            map_completion_error_to_availability(&error, &HostStatus::Available),
            Some(unavailable(UnavailabilityReason::Unreachable))
        );
    }

    #[test]
    fn a_401_and_a_403_are_a_rejected_key_carrying_the_provider_code() {
        for status in [401, 403] {
            assert_eq!(
                map_completion_error_to_availability(
                    &http(status, Some("wrong_api_key")),
                    &HostStatus::Unknown
                ),
                Some(unavailable(UnavailabilityReason::KeyRejected {
                    host_code: Some("wrong_api_key".to_string())
                })),
                "{status}"
            );
        }
    }

    #[test]
    fn a_rejected_key_without_a_provider_code_has_no_host_code() {
        assert_eq!(
            map_completion_error_to_availability(&http(401, None), &HostStatus::Available),
            Some(unavailable(UnavailabilityReason::KeyRejected {
                host_code: None
            }))
        );
    }

    #[test]
    fn a_402_is_payment_required_not_a_rejected_key() {
        assert_eq!(
            map_completion_error_to_availability(
                &http(402, Some("insufficient_credit")),
                &HostStatus::Available
            ),
            Some(unavailable(UnavailabilityReason::PaymentRequired {
                host_code: Some("insufficient_credit".to_string())
            }))
        );
    }

    #[test]
    fn a_429_is_rate_limited_carrying_the_provider_code() {
        assert_eq!(
            map_completion_error_to_availability(
                &http(429, Some("rate_limit_exceeded")),
                &HostStatus::Available
            ),
            Some(unavailable(UnavailabilityReason::RateLimited {
                host_code: Some("rate_limit_exceeded".to_string())
            }))
        );
    }

    #[test]
    fn every_5xx_is_a_server_error_carrying_its_status_and_provider_code() {
        for status in [500, 502, 503, 504, 599] {
            assert_eq!(
                map_completion_error_to_availability(
                    &http(status, Some("boom")),
                    &HostStatus::Available
                ),
                Some(unavailable(UnavailabilityReason::ServerError {
                    http_code: status,
                    host_code: Some("boom".to_string())
                })),
                "{status}"
            );
        }
    }

    #[test]
    fn any_other_answer_from_the_provider_counts_as_available() {
        for status in [400, 404, 405, 409, 413, 415, 418, 422] {
            assert_eq!(
                map_completion_error_to_availability(&http(status, None), &HostStatus::Unknown),
                Some(HostStatus::Available),
                "{status}"
            );
        }
    }

    #[test]
    fn a_409_while_available_is_no_update() {
        assert_eq!(
            map_completion_error_to_availability(&http(409, None), &HostStatus::Available),
            None
        );
    }

    #[test]
    fn a_409_while_unavailable_restores_available() {
        for reason in [
            UnavailabilityReason::TimedOut,
            UnavailabilityReason::KeyRejected { host_code: None },
            UnavailabilityReason::ServerError {
                http_code: 503,
                host_code: None,
            },
        ] {
            assert_eq!(
                map_completion_error_to_availability(&http(409, None), &unavailable(reason)),
                Some(HostStatus::Available)
            );
        }
    }

    #[test]
    fn a_repeated_outcome_is_no_update() {
        let cases = [
            (CompletionError::Timeout, UnavailabilityReason::TimedOut),
            (
                http(429, Some("c")),
                UnavailabilityReason::RateLimited {
                    host_code: Some("c".to_string()),
                },
            ),
            (
                http(503, None),
                UnavailabilityReason::ServerError {
                    http_code: 503,
                    host_code: None,
                },
            ),
            (
                http(401, Some("c")),
                UnavailabilityReason::KeyRejected {
                    host_code: Some("c".to_string()),
                },
            ),
            (
                stream_error(Some("c")),
                UnavailabilityReason::StreamInterrupted {
                    host_code: Some("c".to_string()),
                },
            ),
        ];
        for (error, reason) in cases {
            assert_eq!(
                map_completion_error_to_availability(&error, &unavailable(reason)),
                None,
                "{error:?}"
            );
        }
    }

    #[test]
    fn a_change_of_kind_while_unavailable_is_an_update() {
        assert_eq!(
            map_completion_error_to_availability(
                &http(503, None),
                &unavailable(UnavailabilityReason::TimedOut)
            ),
            Some(unavailable(UnavailabilityReason::ServerError {
                http_code: 503,
                host_code: None
            }))
        );
    }

    #[test]
    fn a_different_status_or_provider_code_of_the_same_kind_is_an_update() {
        let on_500 = unavailable(UnavailabilityReason::ServerError {
            http_code: 500,
            host_code: None,
        });
        assert!(map_completion_error_to_availability(&http(503, None), &on_500).is_some());

        let rejected = unavailable(UnavailabilityReason::KeyRejected {
            host_code: Some("a".to_string()),
        });
        assert_eq!(
            map_completion_error_to_availability(&http(401, Some("b")), &rejected),
            Some(unavailable(UnavailabilityReason::KeyRejected {
                host_code: Some("b".to_string())
            }))
        );
    }

    #[test]
    fn a_stream_error_carries_the_provider_code_into_stream_interrupted() {
        assert_eq!(
            map_completion_error_to_availability(
                &stream_error(Some("queue_overflow")),
                &HostStatus::Available
            ),
            Some(unavailable(UnavailabilityReason::StreamInterrupted {
                host_code: Some("queue_overflow".to_string())
            }))
        );
    }

    #[test]
    fn a_stream_error_without_a_provider_code_has_no_host_code() {
        assert_eq!(
            map_completion_error_to_availability(&stream_error(None), &HostStatus::Unknown),
            Some(unavailable(UnavailabilityReason::StreamInterrupted {
                host_code: None
            }))
        );
    }

    #[test]
    fn a_stream_error_with_a_cause_is_classified_like_one_without() {
        let with_cause = CompletionError::StreamError {
            description: "reset".to_string(),
            cause: Some("connection reset".into()),
            provider_code: None,
        };

        assert_eq!(
            map_completion_error_to_availability(&with_cause, &HostStatus::Available),
            Some(unavailable(UnavailabilityReason::StreamInterrupted {
                host_code: None
            }))
        );
    }

    #[test]
    fn an_empty_generation_means_the_provider_answered_so_it_is_available() {
        for failure in [
            GenerationFailure::ContentFilter,
            GenerationFailure::LengthLimit,
        ] {
            let error = CompletionError::GenerationFailed(failure);
            assert_eq!(
                map_completion_error_to_availability(&error, &HostStatus::Unknown),
                Some(HostStatus::Available),
                "{failure:?}"
            );
            assert_eq!(
                map_completion_error_to_availability(&error, &HostStatus::Available),
                None,
                "{failure:?}"
            );
        }
    }

    #[test]
    fn an_unexpected_tool_call_means_the_provider_answered_so_it_is_available() {
        assert_eq!(
            map_completion_error_to_availability(
                &CompletionError::UnexpectedToolCall,
                &unavailable(UnavailabilityReason::TimedOut)
            ),
            Some(HostStatus::Available)
        );
        assert_eq!(
            map_completion_error_to_availability(
                &CompletionError::UnexpectedToolCall,
                &HostStatus::Available
            ),
            None
        );
    }

    #[test]
    fn an_unexpected_error_is_no_update_whatever_the_current_status() {
        for current in every_current() {
            let error = CompletionError::UnexpectedError("local failure".into());

            assert_eq!(
                map_completion_error_to_availability(&error, &current),
                None,
                "{current:?}"
            );
        }
    }

    #[test]
    fn available_after_available_is_no_update_for_every_available_outcome() {
        for error in [
            http(400, None),
            http(404, None),
            http(409, None),
            http(422, None),
            CompletionError::GenerationFailed(GenerationFailure::LengthLimit),
            CompletionError::UnexpectedToolCall,
        ] {
            assert_eq!(
                map_completion_error_to_availability(&error, &HostStatus::Available),
                None,
                "{error:?}"
            );
        }
    }

    #[test]
    fn the_http_error_display_names_the_status_and_code_but_not_the_provider_message() {
        let text = http(429, Some("rate_limit_exceeded")).to_string();

        assert!(text.contains("429"), "{text}");
        assert!(text.contains("rate_limit_exceeded"), "{text}");
        assert!(!text.contains("provider message"), "{text}");
    }

    #[test]
    fn a_short_message_is_logged_whole() {
        assert_eq!(truncated("short", 200), "short");
        assert_eq!(truncated("", 200), "");
    }

    #[test]
    fn a_message_of_exactly_the_limit_is_not_cut() {
        let message = "x".repeat(200);

        assert_eq!(truncated(&message, 200), message);
    }

    #[test]
    fn a_longer_message_is_cut_at_the_limit_with_an_ellipsis() {
        let message = "x".repeat(201);

        let cut = truncated(&message, 200);

        assert_eq!(cut, format!("{}…", "x".repeat(200)));
    }

    #[test]
    fn truncation_never_splits_a_multibyte_character() {
        // Each "é" is two bytes, so a cut by bytes would land inside one.
        let message = "é".repeat(150);

        let cut = truncated(&message, 100);

        assert_eq!(cut, format!("{}…", "é".repeat(100)));
    }

    #[test]
    fn a_stream_error_displays_its_description_without_repeating_its_cause() {
        let error = CompletionError::StreamError {
            description: "the connection was cut".to_string(),
            cause: Some("socket closed by peer".into()),
            provider_code: None,
        };

        let text = error.to_string();

        assert!(text.contains("the connection was cut"), "{text}");
        assert!(!text.contains("socket closed by peer"), "{text}");
        assert_eq!(
            error.source().map(ToString::to_string).as_deref(),
            Some("socket closed by peer")
        );
    }

    #[test]
    fn a_truncated_stream_error_has_no_source() {
        let error = CompletionError::StreamError {
            description: "ended without a finish reason".to_string(),
            cause: None,
            provider_code: None,
        };

        assert!(error.source().is_none());
    }

    #[test]
    fn the_cause_of_a_connection_or_unexpected_error_is_its_source() {
        let connection = CompletionError::ConnectionError("dns failure".into());
        let unexpected = CompletionError::UnexpectedError("cannot build request".into());

        assert_eq!(
            connection.source().map(ToString::to_string).as_deref(),
            Some("dns failure")
        );
        assert_eq!(
            unexpected.source().map(ToString::to_string).as_deref(),
            Some("cannot build request")
        );
    }

    #[test]
    fn generation_failures_name_their_reason() {
        assert_eq!(
            GenerationFailure::ContentFilter.to_string(),
            "Content was filtered"
        );
        assert_eq!(
            GenerationFailure::LengthLimit.to_string(),
            "Length limit reached"
        );
    }

    #[test]
    fn a_flat_error_object_gives_its_message_and_code() {
        let object = parse_error_object(
            r#"{"message":"Wrong API Key","type":"invalid_request_error","param":"api_key","code":"wrong_api_key"}"#,
        )
        .expect("an object");

        assert_eq!(object.message.as_deref(), Some("Wrong API Key"));
        assert_eq!(object.code.as_deref(), Some("wrong_api_key"));
    }

    #[test]
    fn a_wrapped_error_object_gives_its_message_and_code() {
        let object =
            parse_error_object(r#"{"error":{"message":"m","code":"c"}}"#).expect("an object");

        assert_eq!(object.message.as_deref(), Some("m"));
        assert_eq!(object.code.as_deref(), Some("c"));
    }

    #[test]
    fn json_without_a_message_or_code_gives_an_object_with_neither() {
        let object = parse_error_object(r#"{"id":"x"}"#).expect("json");

        assert!(object.message.is_none() && object.code.is_none());
    }

    #[test]
    fn a_numeric_code_is_not_a_code() {
        let object = parse_error_object(r#"{"message":"m","code":429}"#).expect("json");

        assert_eq!(object.message.as_deref(), Some("m"));
        assert!(object.code.is_none());
    }

    #[test]
    fn data_that_is_not_json_is_not_an_error_object() {
        assert!(parse_error_object("<html>").is_none());
        assert!(parse_error_object("").is_none());
    }
}

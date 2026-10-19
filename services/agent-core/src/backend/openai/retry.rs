//! The retry policy: reuses async-openai's retry classification and gives up when the provider
//! asks to wait longer than the `Retry-After` cap.

use async_openai::error::OpenAIError;
use async_openai::middleware::HttpRequestFactory;
use reqwest::Response;
use std::time::Duration;
use tokio::time::Sleep;
use tower::retry::Policy;

/// The base of the exponential backoff between retries.
pub(crate) const BACKOFF_BASE: Duration = Duration::from_millis(100);

/// The longest `Retry-After` the policy will wait out. Honouring a longer wait would let the idle
/// timeout end the turn first, which would then be reported as a timeout instead of a rate limit.
const RETRY_AFTER_CAP: Duration = Duration::from_secs(10);

/// The C3 retry policy: retries 429, 5xx and connect errors (async-openai's `should_retry`
/// classification) up to the configured max with jittered exponential backoff. Honours a
/// `Retry-After` up to the cap and gives up immediately above it (an HTTP-date `Retry-After`
/// counts as above the cap), passing the response through unchanged. tower clones it per request,
/// so the attempt counter is per request.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    max_retries: u16,
    backoff_base: Duration,
    attempts: u16,
}

impl RetryPolicy {
    /// Creates a new retry policy with the given maximum number of retries and backoff base duration.
    pub fn new(max_retries: u16, backoff_base: Duration) -> Self {
        Self {
            max_retries,
            backoff_base,
            attempts: 0,
        }
    }
}

impl Policy<HttpRequestFactory, Response, OpenAIError> for RetryPolicy {
    type Future = Sleep;

    /// `Some(sleep)` retries the request after the sleep; `None` stops and returns `result` as is.
    fn retry(
        &mut self,
        req: &mut HttpRequestFactory,
        result: &mut Result<Response, OpenAIError>,
    ) -> Option<Self::Future> {
        todo!()
    }

    /// Clones the request for a possible retry; `None` means it cannot be replayed, so no retry.
    fn clone_request(&mut self, req: &HttpRequestFactory) -> Option<HttpRequestFactory> {
        todo!()
    }
}

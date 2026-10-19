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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::openai::fake_provider::{ErrorScript, FakeProvider, StreamScript};
    use axum::http::StatusCode;
    use tokio::time::Instant;

    const BASE: Duration = Duration::from_millis(10);

    /// A request factory the policy never builds from; it only needs something to hold.
    fn factory() -> HttpRequestFactory {
        HttpRequestFactory::new(|| async {
            Err(OpenAIError::InvalidArgument("never built".to_string()))
        })
    }

    /// A real response with the scripted status and optional `Retry-After`, fetched from a fake
    /// provider so the policy sees exactly what a client would.
    async fn response(status: u16, retry_after: Option<&str>) -> Response {
        let provider = FakeProvider::start().await.expect("fake provider");
        if status == 200 {
            provider.enqueue(StreamScript::new(Vec::new()));
        } else {
            let mut script = ErrorScript::new(StatusCode::from_u16(status).expect("status"));
            if let Some(value) = retry_after {
                script = script.retry_after(value);
            }
            provider.enqueue(script);
        }
        reqwest::Client::new()
            .post(format!("{}/chat/completions", provider.base_url()))
            .send()
            .await
            .expect("a response")
    }

    /// The delay of a retry decision, read off the sleep it returned.
    fn delay_of(sleep: &Sleep) -> Duration {
        sleep.deadline().saturating_duration_since(Instant::now())
    }

    async fn decision(
        policy: &mut RetryPolicy,
        status: u16,
        retry_after: Option<&str>,
    ) -> Option<Sleep> {
        let mut result = Ok(response(status, retry_after).await);
        policy.retry(&mut factory(), &mut result)
    }

    #[tokio::test]
    async fn a_503_is_retried() {
        let mut policy = RetryPolicy::new(3, BASE);

        assert!(decision(&mut policy, 503, None).await.is_some());
    }

    #[tokio::test]
    async fn every_5xx_and_a_429_are_retried() {
        for status in [429, 500, 502, 503, 504] {
            let mut policy = RetryPolicy::new(3, BASE);

            assert!(
                decision(&mut policy, status, None).await.is_some(),
                "{status}"
            );
        }
    }

    #[tokio::test]
    async fn a_success_is_not_retried() {
        let mut policy = RetryPolicy::new(3, BASE);

        assert!(decision(&mut policy, 200, None).await.is_none());
    }

    #[tokio::test]
    async fn a_503_then_a_success_means_exactly_one_retry() {
        let mut policy = RetryPolicy::new(3, BASE);

        assert!(decision(&mut policy, 503, None).await.is_some());
        assert!(decision(&mut policy, 200, None).await.is_none());
    }

    #[tokio::test]
    async fn persistent_503s_stop_after_the_configured_number_of_retries() {
        let mut policy = RetryPolicy::new(2, BASE);

        assert!(decision(&mut policy, 503, None).await.is_some());
        assert!(decision(&mut policy, 503, None).await.is_some());
        assert!(decision(&mut policy, 503, None).await.is_none());
    }

    #[tokio::test]
    async fn zero_retries_never_retries() {
        let mut policy = RetryPolicy::new(0, BASE);

        assert!(decision(&mut policy, 503, None).await.is_none());
    }

    #[tokio::test]
    async fn other_statuses_are_not_retried() {
        for status in [400, 401, 402, 403, 404, 409, 422] {
            let mut policy = RetryPolicy::new(3, BASE);

            assert!(
                decision(&mut policy, status, None).await.is_none(),
                "{status}"
            );
        }
    }

    #[tokio::test]
    async fn a_429_with_a_short_retry_after_waits_about_that_long() {
        let mut policy = RetryPolicy::new(3, BASE);

        let sleep = decision(&mut policy, 429, Some("1"))
            .await
            .expect("a short Retry-After is honoured");

        let delay = delay_of(&sleep);
        assert!(
            delay > Duration::from_millis(800) && delay <= Duration::from_millis(1100),
            "waits {delay:?} for Retry-After: 1"
        );
    }

    #[tokio::test]
    async fn a_retry_after_replaces_the_backoff_rather_than_adding_to_it() {
        let mut policy = RetryPolicy::new(3, Duration::from_secs(5));

        let sleep = decision(&mut policy, 503, Some("1"))
            .await
            .expect("retried");

        assert!(delay_of(&sleep) <= Duration::from_millis(1100));
    }

    #[tokio::test]
    async fn a_retry_after_above_the_cap_ends_retrying_at_once() {
        for value in ["11", "30", "3600"] {
            let mut policy = RetryPolicy::new(3, BASE);

            assert!(
                decision(&mut policy, 429, Some(value)).await.is_none(),
                "Retry-After: {value}"
            );
        }
    }

    #[tokio::test]
    async fn an_http_date_retry_after_counts_as_above_the_cap() {
        let mut policy = RetryPolicy::new(3, BASE);

        let retry = decision(&mut policy, 429, Some("Wed, 21 Oct 2037 07:28:00 GMT")).await;

        assert!(retry.is_none());
    }

    #[tokio::test]
    async fn giving_up_passes_the_429_through_with_its_status_and_headers_intact() {
        let mut policy = RetryPolicy::new(3, BASE);
        let mut result = Ok(response(429, Some("3600")).await);

        let retry = policy.retry(&mut factory(), &mut result);

        assert!(retry.is_none());
        let kept = result.expect("the response is passed through, not turned into an error");
        assert_eq!(kept.status(), 429);
        assert_eq!(kept.headers()["retry-after"], "3600");
    }

    #[tokio::test]
    async fn without_a_retry_after_the_backoff_grows_between_attempts() {
        let mut policy = RetryPolicy::new(4, Duration::from_millis(50));

        let first = delay_of(&decision(&mut policy, 503, None).await.expect("retry 1"));
        assert!(decision(&mut policy, 503, None).await.is_some(), "retry 2");
        let third = delay_of(&decision(&mut policy, 503, None).await.expect("retry 3"));

        assert!(third > first, "first {first:?}, third {third:?}");
        assert!(first < Duration::from_secs(1), "first backoff {first:?}");
    }

    #[tokio::test]
    async fn a_connect_error_is_retried() {
        // A port nothing listens on: bind to learn a free one, then close it.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        let error = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{port}/"))
            .send()
            .await
            .expect_err("nothing listens there");
        assert!(error.is_connect(), "{error:?}");
        let mut policy = RetryPolicy::new(3, BASE);

        let retry = policy.retry(&mut factory(), &mut Err(OpenAIError::Reqwest(error)));

        assert!(retry.is_some());
    }

    #[tokio::test]
    async fn an_error_that_is_not_a_connect_error_is_not_retried() {
        let mut policy = RetryPolicy::new(3, BASE);

        let retry = policy.retry(
            &mut factory(),
            &mut Err(OpenAIError::InvalidArgument("bad".to_string())),
        );

        assert!(retry.is_none());
    }

    #[tokio::test]
    async fn the_request_can_always_be_replayed() {
        let mut policy = RetryPolicy::new(3, BASE);

        assert!(policy.clone_request(&factory()).is_some());
    }

    #[tokio::test]
    async fn each_clone_counts_its_own_attempts() {
        let fresh = RetryPolicy::new(1, BASE);
        let mut first_request = fresh.clone();
        let mut second_request = fresh.clone();

        assert!(decision(&mut first_request, 503, None).await.is_some());
        assert!(decision(&mut first_request, 503, None).await.is_none());

        assert!(decision(&mut second_request, 503, None).await.is_some());
    }
}

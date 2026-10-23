//! The per-attempt tracing layer (inside retry, one span per HTTP attempt). Never records prompt
//! or reply content, or the key.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::backend::openai::error::host_not_reached;
use async_openai::error::OpenAIError;
use async_openai::middleware::HttpRequestFactory;
use reqwest::Response;
use tower::{Layer, Service};
use tracing::Instrument;

/// Layer for the per-attempt tracing service. Sits inside the retry layer, so it wraps every HTTP
/// attempt.
#[derive(Debug, Clone, Copy, Default)]
pub struct AttemptTracingLayer;

/// Service that records one span per HTTP attempt: status or connection error, never body or key.
#[derive(Debug, Clone)]
pub struct AttemptTracing<S> {
    inner: S,
}

impl<S> Layer<S> for AttemptTracingLayer {
    type Service = AttemptTracing<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AttemptTracing { inner }
    }
}

impl<S> Service<HttpRequestFactory> for AttemptTracing<S>
where
    S: Service<HttpRequestFactory, Response = Response, Error = OpenAIError>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = OpenAIError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    /// Creates one span per attempt, runs the inner call instrumented with it, and records the
    /// status or the connection error. Never records the body or key-bearing headers. Uses the
    /// `std::mem::replace` swap so the ready inner service is the one called.
    fn call(&mut self, req: HttpRequestFactory) -> Self::Future {
        // Created here, so it is a child of whatever span is current: the call span.
        let span = tracing::info_span!(
            "http_attempt",
            "http.response.status_code" = tracing::field::Empty,
            "error.type" = tracing::field::Empty,
        );
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let recorder = span.clone();
        Box::pin(
            async move {
                let result = inner.call(req).await;
                // Only the status or the kind of failure: never the body or any header.
                match &result {
                    Ok(response) => {
                        recorder.record("http.response.status_code", response.status().as_u16());
                    }
                    // The same names as the call span's `error.type`.
                    Err(OpenAIError::Reqwest(error)) if host_not_reached(error) => {
                        recorder.record("error.type", "connection_error");
                    }
                    Err(_) => {
                        recorder.record("error.type", "unexpected_error");
                    }
                }
                result
            }
            .instrument(span),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::openai::fake_provider::{ErrorScript, FakeProvider, Step, StreamScript};
    use axum::http::StatusCode;
    use tower::{ServiceExt, service_fn};

    /// A request factory the layers never build from; the inner service below ignores it.
    fn factory() -> HttpRequestFactory {
        HttpRequestFactory::new(|| async {
            Err(OpenAIError::InvalidArgument("never built".to_string()))
        })
    }

    /// An inner service that posts to the fake provider with a plain reqwest client, standing in
    /// for the bottom of the real stack.
    fn transport(
        base_url: String,
    ) -> impl Service<
        HttpRequestFactory,
        Response = Response,
        Error = OpenAIError,
        Future = impl Future<Output = Result<Response, OpenAIError>> + Send,
    > + Clone
    + Send
    + 'static {
        let client = reqwest::Client::new();
        let url = format!("{base_url}/chat/completions");
        service_fn(move |_request: HttpRequestFactory| {
            let client = client.clone();
            let url = url.clone();
            async move { client.post(url).send().await.map_err(OpenAIError::Reqwest) }
        })
    }

    #[tokio::test]
    async fn attempt_tracing_passes_a_success_through_untouched() {
        let provider = FakeProvider::start().await.expect("provider");
        provider.enqueue(StreamScript::new(vec![Step::data("payload")]));
        let service = AttemptTracingLayer.layer(transport(provider.base_url()));

        let response = service.oneshot(factory()).await.expect("a success");

        assert_eq!(response.status(), 200);
        assert_eq!(response.text().await.expect("body"), "data: payload\n\n");
    }

    #[tokio::test]
    async fn attempt_tracing_leaves_an_error_status_as_a_response_for_the_retry_policy() {
        let provider = FakeProvider::start().await.expect("provider");
        provider.enqueue(ErrorScript::new(StatusCode::SERVICE_UNAVAILABLE).retry_after("2"));
        let service = AttemptTracingLayer.layer(transport(provider.base_url()));

        let response = service
            .oneshot(factory())
            .await
            .expect("the retry policy needs the response, not an error");

        assert_eq!(response.status(), 503);
        assert_eq!(response.headers()["retry-after"], "2");
    }

    #[tokio::test]
    async fn attempt_tracing_passes_a_transport_error_through_unchanged() {
        let provider = FakeProvider::start().await.expect("provider");
        let inner = transport(provider.base_url());
        drop(provider);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let service = AttemptTracingLayer.layer(inner);

        let error = service
            .oneshot(factory())
            .await
            .expect_err("nothing listens");

        match error {
            OpenAIError::Reqwest(error) => assert!(error.is_connect(), "{error:?}"),
            other => panic!("expected the transport error, got {other:?}"),
        }
    }
}

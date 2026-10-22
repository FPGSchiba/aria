//! Tower layers around the HTTP call: status capture (outside retry, turns a final non-2xx
//! response into a typed error) and per-attempt tracing (inside retry, one span per HTTP attempt).
//! Never records prompt or reply content, or the key.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::backend::openai::error::CompletionError;
use async_openai::error::OpenAIError;
use async_openai::middleware::HttpRequestFactory;
use reqwest::{Response, StatusCode};
use tower::{Layer, Service};
use tracing::Instrument;

/// Layer for the status-capture service. Sits outside the retry layer, so it only sees the final
/// response of a call.
#[derive(Debug, Clone, Copy, Default)]
pub struct StatusCaptureLayer;

/// Service that turns the final non-2xx response into a typed `CompletionError::HttpError`.
#[derive(Debug, Clone)]
pub struct StatusCapture<S> {
    inner: S,
}

impl<S> Layer<S> for StatusCaptureLayer {
    type Service = StatusCapture<S>;

    fn layer(&self, inner: S) -> Self::Service {
        StatusCapture { inner }
    }
}

impl<S> Service<HttpRequestFactory> for StatusCapture<S>
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

    /// Awaits the inner call. A 2xx response passes through untouched. For a non-2xx response it
    /// reads the body, parses the flat provider error `{message, type, param, code}` and returns
    /// `OpenAIError::Boxed(CompletionError::HttpError { .. })`. Uses the `std::mem::replace` swap so
    /// the ready inner service is the one called.
    fn call(&mut self, req: HttpRequestFactory) -> Self::Future {
        // `poll_ready` readied `self.inner`, so that one makes the call; the clone left behind is
        // readied again by the next `poll_ready`.
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        Box::pin(async move {
            let response = inner.call(req).await?;
            let status = response.status();
            if status.is_success() {
                return Ok(response);
            }
            // A body that cannot be read leaves an empty one: the status is what matters.
            let body = response.text().await.unwrap_or_default();
            Err(OpenAIError::Boxed(Box::new(http_error(status, &body))))
        })
    }
}

/// Builds the HTTP error for a non-2xx answer from its body: Cerebras's flat
/// `{"message","type","param","code"}`, OpenAI's wrapped `{"error": {..}}`, or anything else (then
/// the message is the status text and there is no code).
fn http_error(status: StatusCode, body: &str) -> CompletionError {
    let json: Option<serde_json::Value> = serde_json::from_str(body).ok();
    let fields = json.as_ref().map(|value| {
        value
            .get("error")
            .filter(|e| e.is_object())
            .unwrap_or(value)
    });
    let text = |name: &str| {
        fields
            .and_then(|f| f.get(name))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    CompletionError::HttpError {
        status,
        message: text("message").unwrap_or_else(|| {
            status
                .canonical_reason()
                .unwrap_or("unknown status")
                .to_string()
        }),
        code: text("code"),
    }
}

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
                    Err(OpenAIError::Reqwest(error)) if error.is_connect() => {
                        recorder.record("error.type", "connect_error");
                    }
                    Err(_) => {
                        recorder.record("error.type", "request_error");
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
    use crate::backend::openai::error::CompletionError;
    use crate::backend::openai::fake_provider::{
        ErrorScript, FakeProvider, Step, StreamScript, flat_error_body,
    };
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

    /// The classified error a failed call carries.
    fn completion_error(error: OpenAIError) -> CompletionError {
        match error {
            OpenAIError::Boxed(boxed) => *boxed
                .downcast::<CompletionError>()
                .unwrap_or_else(|other| panic!("not a CompletionError: {other:?}")),
            other => panic!("expected OpenAIError::Boxed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn status_capture_passes_a_success_through_untouched() {
        let provider = FakeProvider::start().await.expect("provider");
        provider.enqueue(StreamScript::new(vec![Step::data("payload")]));
        let service = StatusCaptureLayer.layer(transport(provider.base_url()));

        let response = service.oneshot(factory()).await.expect("a success");

        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["content-type"], "text/event-stream");
        assert_eq!(response.text().await.expect("body"), "data: payload\n\n");
    }

    #[tokio::test]
    async fn status_capture_turns_a_flat_provider_error_into_an_http_error_with_status_and_code() {
        let provider = FakeProvider::start().await.expect("provider");
        provider.enqueue(
            ErrorScript::new(StatusCode::UNAUTHORIZED).body(flat_error_body(
                "Wrong API Key",
                "invalid_request_error",
                Some("api_key"),
                "wrong_api_key",
            )),
        );
        let service = StatusCaptureLayer.layer(transport(provider.base_url()));

        let error = service.oneshot(factory()).await.expect_err("a non-2xx");

        match completion_error(error) {
            CompletionError::HttpError {
                status,
                message,
                code,
            } => {
                assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
                assert_eq!(message, "Wrong API Key");
                assert_eq!(code.as_deref(), Some("wrong_api_key"));
            }
            other => panic!("expected an HTTP error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn status_capture_keeps_the_status_of_a_429_and_a_5xx() {
        for status in [StatusCode::TOO_MANY_REQUESTS, StatusCode::BAD_GATEWAY] {
            let provider = FakeProvider::start().await.expect("provider");
            provider.enqueue(ErrorScript::new(status).body(flat_error_body(
                "nope",
                "server_error",
                None,
                "some_code",
            )));
            let service = StatusCaptureLayer.layer(transport(provider.base_url()));

            let error = service.oneshot(factory()).await.expect_err("a non-2xx");

            match completion_error(error) {
                CompletionError::HttpError { status: got, .. } => {
                    assert_eq!(got.as_u16(), status.as_u16());
                }
                other => panic!("expected an HTTP error, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn status_capture_keeps_the_status_when_the_body_is_not_the_flat_form() {
        for body in [
            Some("<html>bad gateway</html>"),
            Some("{\"unexpected\":true}"),
            None,
        ] {
            let provider = FakeProvider::start().await.expect("provider");
            let mut script = ErrorScript::new(StatusCode::BAD_GATEWAY);
            if let Some(body) = body {
                script = script.body(body);
            }
            provider.enqueue(script);
            let service = StatusCaptureLayer.layer(transport(provider.base_url()));

            let error = service.oneshot(factory()).await.expect_err("a non-2xx");

            match completion_error(error) {
                CompletionError::HttpError { status, code, .. } => {
                    assert_eq!(status, reqwest::StatusCode::BAD_GATEWAY, "{body:?}");
                    assert_eq!(code, None, "{body:?}");
                }
                other => panic!("expected an HTTP error for {body:?}, got {other:?}"),
            }
        }
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

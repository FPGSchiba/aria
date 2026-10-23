//! The status-capture layer: turns a final non-2xx response into a typed error, because the crate
//! cannot parse Cerebras's flat error body and would lose the status. It sits outside the retry
//! layer, so it only sees the final response of a call.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::backend::openai::error::{CompletionError, log_provider_message, parse_error_object};
use async_openai::error::OpenAIError;
use async_openai::middleware::HttpRequestFactory;
use reqwest::{Response, StatusCode};
use tower::{Layer, Service};

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
/// there is no code). The error carries status and code only: the provider's message is logged
/// once, here, at debug level, and goes no further.
fn http_error(status: StatusCode, body: &str) -> CompletionError {
    let parsed = parse_error_object(body);
    if let Some(message) = parsed.as_ref().and_then(|object| object.message.as_deref()) {
        log_provider_message(
            message,
            Some(status.as_u16()),
            parsed.as_ref().and_then(|object| object.code.as_deref()),
        );
    }
    CompletionError::HttpError {
        status,
        code: parsed.and_then(|object| object.code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::openai::fake_provider::{
        ErrorScript, FakeProvider, Step, StreamScript, flat_error_body,
    };
    use crate::backend::openai::test_support::{factory, transport};
    use axum::http::StatusCode;
    use tower::ServiceExt;

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
            CompletionError::HttpError { status, code } => {
                assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
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

    /// What `http_error` logs for `body`, at debug level, and the error it returns.
    fn logged_http_error(status: StatusCode, body: &str) -> (String, CompletionError) {
        use std::sync::{Arc, Mutex};
        #[derive(Clone, Default)]
        struct Buffer(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Buffer {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().expect("buffer").extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Buffer {
            type Writer = Buffer;
            fn make_writer(&'a self) -> Buffer {
                self.clone()
            }
        }
        let buffer = Buffer::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(buffer.clone())
            .with_ansi(false)
            .with_max_level(tracing::Level::DEBUG)
            .finish();
        let error = tracing::subscriber::with_default(subscriber, || http_error(status, body));
        let output = String::from_utf8_lossy(&buffer.0.lock().expect("buffer")).into_owned();
        (output, error)
    }

    #[test]
    fn the_parsed_message_is_logged_once_at_debug_and_the_error_has_only_status_and_code() {
        let body = flat_error_body(
            "Wrong API Key",
            "invalid_request_error",
            None,
            "wrong_api_key",
        );

        let (log, error) = logged_http_error(reqwest::StatusCode::UNAUTHORIZED, &body);

        assert_eq!(log.lines().count(), 1, "{log}");
        assert!(log.contains("DEBUG"), "{log}");
        assert!(log.contains("agent_core::provider_message"), "{log}");
        assert!(log.contains("Wrong API Key"), "{log}");
        assert!(log.contains("wrong_api_key"), "{log}");
        assert!(log.contains("401"), "{log}");
        assert!(!error.to_string().contains("Wrong API Key"), "{error}");
        assert!(!format!("{error:?}").contains("Wrong API Key"), "{error:?}");
    }

    #[test]
    fn the_wrapped_form_is_parsed_and_logged_too() {
        let body = r#"{"error":{"message":"wrapped message","code":"wrapped_code"}}"#;

        let (log, error) = logged_http_error(reqwest::StatusCode::BAD_REQUEST, body);

        assert!(log.contains("wrapped message"), "{log}");
        assert!(matches!(
            error,
            CompletionError::HttpError { code: Some(ref code), .. } if code == "wrapped_code"
        ));
    }

    #[test]
    fn a_body_without_a_message_logs_nothing_and_keeps_status_and_any_code() {
        let (log, error) = logged_http_error(
            reqwest::StatusCode::BAD_GATEWAY,
            r#"{"code":"only_a_code"}"#,
        );

        assert_eq!(log, "");
        assert!(matches!(
            error,
            CompletionError::HttpError { status, code: Some(ref code) }
                if status == reqwest::StatusCode::BAD_GATEWAY && code == "only_a_code"
        ));
    }

    #[test]
    fn a_body_that_is_not_json_logs_nothing_and_has_no_code() {
        let (log, error) =
            logged_http_error(reqwest::StatusCode::BAD_GATEWAY, "<html>bad gateway</html>");

        assert_eq!(log, "");
        assert!(matches!(
            error,
            CompletionError::HttpError { code: None, .. }
        ));
    }
}

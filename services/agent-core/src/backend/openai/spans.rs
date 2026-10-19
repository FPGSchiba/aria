//! Tower layers around the HTTP call: status capture (outside retry, turns a final non-2xx
//! response into a typed error) and per-attempt tracing (inside retry, one span per HTTP attempt).
//! Never records prompt or reply content, or the key.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_openai::error::OpenAIError;
use async_openai::middleware::HttpRequestFactory;
use reqwest::Response;
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
    fn call(&mut self, _req: HttpRequestFactory) -> Self::Future {
        todo!()
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
    fn call(&mut self, _req: HttpRequestFactory) -> Self::Future {
        todo!()
    }
}

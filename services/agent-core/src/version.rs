//! The build's version, and the response header that carries it.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tonic::codegen::http::{HeaderValue, Request, Response};
use tower::{Layer, Service};

/// The package version, from `CARGO_PKG_VERSION`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The git commit the build came from, if `ARIA_GIT_SHA` was set to a non-empty value when it was
/// built. Absent in a local build.
pub const GIT_SHA: Option<&str> = non_empty(option_env!("ARIA_GIT_SHA"));

/// `None` for an unset or empty value, so a blank build argument counts as no SHA.
const fn non_empty(value: Option<&'static str>) -> Option<&'static str> {
    match value {
        Some(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

/// The name of the header every gRPC response carries.
pub const VERSION_HEADER: &str = "x-aria-version";

/// Adds the [`VERSION_HEADER`] header, set to [`VERSION`], to every response of the server it
/// wraps, whichever service answers.
#[derive(Debug, Clone, Default)]
pub struct VersionLayer;

impl<S> Layer<S> for VersionLayer {
    type Service = VersionHeader<S>;

    fn layer(&self, inner: S) -> Self::Service {
        VersionHeader { inner }
    }
}

/// The service [`VersionLayer`] builds.
#[derive(Debug, Clone)]
pub struct VersionHeader<S> {
    inner: S,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for VersionHeader<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        // The clone is the one that was polled ready; swap so the ready one makes the call.
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        Box::pin(async move {
            let mut response = inner.call(request).await?;
            response
                .headers_mut()
                .insert(VERSION_HEADER, HeaderValue::from_static(VERSION));
            Ok(response)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::non_empty;

    #[test]
    fn an_empty_sha_counts_as_none() {
        assert_eq!(non_empty(None), None);
        assert_eq!(non_empty(Some("")), None);
        assert_eq!(non_empty(Some("abc1234")), Some("abc1234"));
    }
}

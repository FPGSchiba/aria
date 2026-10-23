//! Helpers the unit tests of the tower layers and the retry policy share.

use async_openai::error::OpenAIError;
use async_openai::middleware::HttpRequestFactory;
use reqwest::Response;
use tower::{Service, service_fn};

/// A request factory nothing builds from: the policy only holds it, and `transport` ignores it.
pub(crate) fn factory() -> HttpRequestFactory {
    HttpRequestFactory::new(|| async {
        Err(OpenAIError::InvalidArgument("never built".to_string()))
    })
}

/// An inner service that posts to the fake provider with a plain reqwest client, standing in for
/// the bottom of the real stack.
pub(crate) fn transport(
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

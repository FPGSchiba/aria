//! The retry policy as seen through the whole backend: how many requests reach the provider, how
//! long the backend waits, and what the caller finally sees.

mod common;

use agent_core::backend::openai::OpenAIBackend;
use agent_core::backend::openai::error::CompletionError;
use agent_core::backend::openai::fake_provider::{
    ErrorScript, FakeProvider, Step, StreamScript, finish_chunk, flat_error_body, role_chunk,
    text_delta_chunk, usage_chunk,
};
use agent_core::backend::{Backend, BackendError, Chunk};
use agent_core::conversation::{Actor, Content, ConversationPart};
use axum::http::StatusCode;
use common::backend_for;
use std::error::Error;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tokio_stream::StreamExt;

/// Upper bound for anything that could hang, so a failure reports instead of blocking.
const LIMIT: Duration = Duration::from_secs(15);

async fn turn(backend: &OpenAIBackend) -> (Vec<Result<Chunk, BackendError>>, Duration) {
    let started = Instant::now();
    let input = vec![ConversationPart {
        content: Content::Text {
            text: "hello".to_string(),
        },
        actor: Actor::User,
    }];
    let items = timeout(
        LIMIT,
        backend
            .stream_conversation(Vec::new(), input)
            .collect::<Vec<_>>(),
    )
    .await
    .expect("the stream did not end in time");
    (items, started.elapsed())
}

fn texts(items: &[Result<Chunk, BackendError>]) -> Vec<String> {
    items
        .iter()
        .filter_map(|i| match i {
            Ok(Chunk::Text { text }) => Some(text.clone()),
            Err(_) => None,
        })
        .collect()
}

/// The HTTP status of the one error that ended the turn.
fn http_status(items: &[Result<Chunk, BackendError>]) -> reqwest::StatusCode {
    let error = items
        .iter()
        .find_map(|i| i.as_ref().err())
        .unwrap_or_else(|| panic!("expected an error: {items:?}"));
    match error
        .source()
        .and_then(|s| s.downcast_ref::<CompletionError>())
    {
        Some(CompletionError::HttpError { status, .. }) => *status,
        other => panic!("expected an HTTP error, got {other:?}"),
    }
}

fn reply() -> StreamScript {
    StreamScript::new(vec![
        Step::data(role_chunk()),
        Step::data(text_delta_chunk("recovered")),
        Step::data(finish_chunk("stop")),
        Step::data(usage_chunk(1, 1)),
        Step::done(),
    ])
}

fn error(status: StatusCode) -> ErrorScript {
    ErrorScript::new(status)
}

#[tokio::test]
async fn a_503_then_success_streams_the_reply_after_exactly_one_retry() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(error(StatusCode::SERVICE_UNAVAILABLE));
    provider.enqueue(reply());
    let backend = backend_for(&provider, "5s", 3);

    let (items, _) = turn(&backend).await;

    assert_eq!(texts(&items), vec!["recovered"]);
    assert!(items.iter().all(Result::is_ok), "{items:?}");
    assert_eq!(provider.request_count(), 2);
}

#[tokio::test]
async fn persistent_503s_stop_after_the_configured_retries_and_report_the_503() {
    let provider = FakeProvider::start().await.expect("provider");
    for _ in 0..5 {
        provider.enqueue(error(StatusCode::SERVICE_UNAVAILABLE));
    }
    let backend = backend_for(&provider, "5s", 2);

    let (items, _) = turn(&backend).await;

    assert_eq!(
        http_status(&items),
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(provider.request_count(), 3, "one attempt plus two retries");
}

#[tokio::test]
async fn a_429_is_retried() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(error(StatusCode::TOO_MANY_REQUESTS));
    provider.enqueue(reply());
    let backend = backend_for(&provider, "5s", 3);

    let (items, _) = turn(&backend).await;

    assert_eq!(texts(&items), vec!["recovered"]);
    assert_eq!(provider.request_count(), 2);
}

#[tokio::test]
async fn a_429_with_a_short_retry_after_is_retried_after_waiting_about_that_long() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(error(StatusCode::TOO_MANY_REQUESTS).retry_after("1"));
    provider.enqueue(reply());
    let backend = backend_for(&provider, "10s", 3);

    let (items, elapsed) = turn(&backend).await;

    assert_eq!(texts(&items), vec!["recovered"]);
    assert_eq!(provider.request_count(), 2);
    assert!(
        elapsed >= Duration::from_millis(900),
        "the retry came after {elapsed:?}, not after the requested second"
    );
    assert!(elapsed < Duration::from_millis(2500), "{elapsed:?}");
}

#[tokio::test]
async fn a_429_with_a_long_retry_after_is_not_retried_and_the_caller_sees_the_429() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(
        error(StatusCode::TOO_MANY_REQUESTS)
            .retry_after("3600")
            .body(flat_error_body(
                "Too many requests",
                "too_many_requests_error",
                None,
                "rate_limit_exceeded",
            )),
    );
    provider.enqueue(reply());
    let backend = backend_for(&provider, "30s", 3);

    let (items, elapsed) = turn(&backend).await;

    assert_eq!(http_status(&items), reqwest::StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(provider.request_count(), 1);
    assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
}

#[tokio::test]
async fn a_retry_after_just_above_the_cap_is_not_waited_out() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(error(StatusCode::TOO_MANY_REQUESTS).retry_after("11"));
    provider.enqueue(reply());
    let backend = backend_for(&provider, "30s", 3);

    let (items, elapsed) = turn(&backend).await;

    assert_eq!(http_status(&items), reqwest::StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(provider.request_count(), 1);
    assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
}

#[tokio::test]
async fn an_http_date_retry_after_is_treated_as_above_the_cap() {
    let provider = FakeProvider::start().await.expect("provider");
    provider
        .enqueue(error(StatusCode::TOO_MANY_REQUESTS).retry_after("Wed, 21 Oct 2037 07:28:00 GMT"));
    provider.enqueue(reply());
    let backend = backend_for(&provider, "30s", 3);

    let (items, _) = turn(&backend).await;

    assert_eq!(http_status(&items), reqwest::StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(provider.request_count(), 1);
}

#[tokio::test]
async fn a_400_is_not_retried() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(error(StatusCode::BAD_REQUEST).body(flat_error_body(
        "Bad request",
        "invalid_request_error",
        None,
        "bad_request",
    )));
    provider.enqueue(reply());
    let backend = backend_for(&provider, "5s", 3);

    let (items, _) = turn(&backend).await;

    assert_eq!(http_status(&items), reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(provider.request_count(), 1);
}

#[tokio::test]
async fn a_401_is_not_retried() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(error(StatusCode::UNAUTHORIZED).body(flat_error_body(
        "Wrong API Key",
        "invalid_request_error",
        Some("api_key"),
        "wrong_api_key",
    )));
    provider.enqueue(reply());
    let backend = backend_for(&provider, "5s", 3);

    let (items, _) = turn(&backend).await;

    assert_eq!(http_status(&items), reqwest::StatusCode::UNAUTHORIZED);
    assert_eq!(provider.request_count(), 1);
}

#[tokio::test]
async fn a_retry_count_of_zero_makes_exactly_one_request() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(error(StatusCode::SERVICE_UNAVAILABLE));
    provider.enqueue(reply());
    let backend = backend_for(&provider, "5s", 0);

    let (items, _) = turn(&backend).await;

    assert_eq!(
        http_status(&items),
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(provider.request_count(), 1);
}

#[tokio::test]
async fn a_successful_call_makes_exactly_one_request() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(reply());
    let backend = backend_for(&provider, "5s", 3);

    let (items, _) = turn(&backend).await;

    assert_eq!(texts(&items), vec!["recovered"]);
    assert_eq!(provider.request_count(), 1);
}

#[tokio::test]
async fn every_turn_gets_its_own_retry_budget() {
    let provider = FakeProvider::start().await.expect("provider");
    // Two turns, each: one 503, then success. With a budget of one retry per turn, a shared
    // counter would leave the second turn without its retry.
    provider.enqueue(error(StatusCode::SERVICE_UNAVAILABLE));
    provider.enqueue(reply());
    provider.enqueue(error(StatusCode::SERVICE_UNAVAILABLE));
    provider.enqueue(reply());
    let backend = backend_for(&provider, "5s", 1);

    let (first, _) = turn(&backend).await;
    let (second, _) = turn(&backend).await;

    assert_eq!(texts(&first), vec!["recovered"]);
    assert_eq!(texts(&second), vec!["recovered"]);
    assert_eq!(provider.request_count(), 4);
}

#[tokio::test]
async fn a_stream_that_breaks_after_it_started_is_not_retried() {
    let provider = FakeProvider::start().await.expect("provider");
    // Text, then the connection closes with no finish reason; a second reply is waiting.
    provider.enqueue(StreamScript::new(vec![
        Step::data(role_chunk()),
        Step::data(text_delta_chunk("started")),
    ]));
    provider.enqueue(reply());
    let backend = backend_for(&provider, "5s", 3);

    let (items, _) = turn(&backend).await;

    assert_eq!(texts(&items), vec!["started"]);
    assert!(items.iter().any(Result::is_err), "{items:?}");
    assert_eq!(provider.request_count(), 1);
}

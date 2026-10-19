//! The OpenAI-compatible backend streaming from a fake provider: chunks, the idle timeout, every
//! kind of failure, and what the request looked like on the wire.

mod common;

use agent_core::backend::openai::OpenAIBackend;
use agent_core::backend::openai::error::{
    CompletionError, GenerationFailure, map_completion_error_to_availability,
};
use agent_core::backend::openai::fake_provider::{
    ErrorScript, FakeProvider, Step, StreamScript, default_happy_path, finish_chunk,
    flat_error_body, reasoning_chunk, role_chunk, text_delta_chunk, tool_call_delta_chunk,
    usage_chunk,
};
use agent_core::backend::{Backend, BackendError, Chunk};
use agent_core::conversation::{Actor, Content, ConversationPart};
use agent_core::health::{HostStatus, UnavailabilityReason};
use axum::http::StatusCode;
use common::{TEST_KEY, TEST_MODEL, backend_for};
use std::error::Error;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tokio_stream::StreamExt;

/// Upper bound for anything that could hang, so a failure reports instead of blocking.
const LIMIT: Duration = Duration::from_secs(15);

// ---------------------------------------------------------------- harness

fn part(actor: Actor, text: &str) -> ConversationPart {
    ConversationPart {
        content: Content::Text {
            text: text.to_string(),
        },
        actor,
    }
}

fn user(text: &str) -> Vec<ConversationPart> {
    vec![part(Actor::User, text)]
}

/// Everything one call produced, in order.
struct Run {
    items: Vec<Result<Chunk, BackendError>>,
    elapsed: Duration,
}

impl Run {
    fn texts(&self) -> Vec<String> {
        self.items
            .iter()
            .filter_map(|item| match item {
                Ok(Chunk::Text { text }) => Some(text.clone()),
                Err(_) => None,
            })
            .collect()
    }

    fn errors(&self) -> Vec<&BackendError> {
        self.items.iter().filter_map(|i| i.as_ref().err()).collect()
    }

    /// The one error that ended the call; fails if there is none or more than one.
    fn error(&self) -> &BackendError {
        let errors = self.errors();
        assert_eq!(
            errors.len(),
            1,
            "expected exactly one error: {:?}",
            self.items
        );
        errors[0]
    }

    fn cause(&self) -> &CompletionError {
        completion_error(self.error())
    }

    fn assert_clean(&self) {
        assert!(
            self.errors().is_empty(),
            "unexpected error: {:?}",
            self.items
        );
    }
}

/// The classified cause of a backend error.
fn completion_error(error: &BackendError) -> &CompletionError {
    error
        .source()
        .and_then(|source| source.downcast_ref::<CompletionError>())
        .unwrap_or_else(|| panic!("the error's source is not a CompletionError: {error:?}"))
}

async fn run_with(
    backend: &OpenAIBackend,
    history: Vec<ConversationPart>,
    input: Vec<ConversationPart>,
) -> Run {
    let started = Instant::now();
    let items = timeout(
        LIMIT,
        backend
            .stream_conversation(history, input)
            .collect::<Vec<_>>(),
    )
    .await
    .expect("the stream did not end in time");
    Run {
        items,
        elapsed: started.elapsed(),
    }
}

async fn run(backend: &OpenAIBackend) -> Run {
    run_with(backend, Vec::new(), user("hello")).await
}

fn script(steps: Vec<Step>) -> StreamScript {
    StreamScript::new(steps)
}

/// role, one text chunk per entry, a "stop" finish, usage and `[DONE]`.
fn replying(texts: &[&str]) -> StreamScript {
    let mut steps = vec![Step::data(role_chunk())];
    steps.extend(texts.iter().map(|t| Step::data(text_delta_chunk(t))));
    steps.extend([
        Step::data(finish_chunk("stop")),
        Step::data(usage_chunk(3, 4)),
        Step::done(),
    ]);
    script(steps)
}

fn key_rejected_body() -> String {
    flat_error_body(
        "Wrong API Key",
        "invalid_request_error",
        Some("api_key"),
        "wrong_api_key",
    )
}

// ---------------------------------------------------------------- identity

#[tokio::test]
async fn the_backend_reports_its_name() {
    let provider = FakeProvider::start().await.expect("provider");
    let backend = backend_for(&provider, "5s", 0);

    assert_eq!(backend.name(), "openai");
}

#[tokio::test]
async fn a_fresh_backend_has_not_observed_the_provider_yet() {
    let provider = FakeProvider::start().await.expect("provider");
    let backend = backend_for(&provider, "5s", 0);

    assert_eq!(
        *backend.subscribe_health().borrow().host_status(),
        HostStatus::Unknown
    );
}

// ---------------------------------------------------------------- text

#[tokio::test]
async fn a_three_delta_stream_yields_three_chunks_in_order() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(replying(&["one ", "two ", "three"]));
    let backend = backend_for(&provider, "5s", 0);

    let run = run(&backend).await;

    run.assert_clean();
    assert_eq!(run.texts(), vec!["one ", "two ", "three"]);
}

#[tokio::test]
async fn the_default_happy_path_yields_only_its_text_deltas() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(default_happy_path());
    let backend = backend_for(&provider, "5s", 0);

    let run = run(&backend).await;

    run.assert_clean();
    assert_eq!(run.texts(), vec!["Hello", ", ", "world"]);
}

#[tokio::test]
async fn the_first_chunk_is_delivered_before_a_delayed_final_event_is_sent() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(script(vec![
        Step::data(role_chunk()),
        Step::data(text_delta_chunk("first")),
        Step::data(finish_chunk("stop")).after(Duration::from_millis(900)),
        Step::data(usage_chunk(1, 1)),
        Step::done(),
    ]));
    let backend = backend_for(&provider, "5s", 0);
    let started = Instant::now();
    let mut stream = backend.stream_conversation(Vec::new(), user("hello"));

    let first = timeout(LIMIT, stream.next())
        .await
        .expect("no chunk in time")
        .expect("the stream ended early")
        .expect("a chunk");
    let first_after = started.elapsed();
    let rest = timeout(LIMIT, stream.collect::<Vec<_>>())
        .await
        .expect("the rest did not arrive");

    assert_eq!(
        first,
        Chunk::Text {
            text: "first".to_string()
        }
    );
    assert!(
        first_after < Duration::from_millis(600),
        "the first chunk took {first_after:?}, so it was held until the end"
    );
    assert!(rest.is_empty(), "{rest:?}");
    assert!(started.elapsed() >= Duration::from_millis(850));
}

#[tokio::test]
async fn reasoning_only_deltas_yield_no_chunks() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(script(vec![
        Step::data(role_chunk()),
        Step::data(reasoning_chunk("thinking about it")),
        Step::data(reasoning_chunk("still thinking")),
        Step::data(finish_chunk("stop")),
        Step::data(usage_chunk(2, 2)),
        Step::done(),
    ]));
    let backend = backend_for(&provider, "5s", 0);

    let run = run(&backend).await;

    assert!(run.texts().is_empty(), "{:?}", run.items);
}

#[tokio::test]
async fn reasoning_text_is_never_emitted_between_text_deltas() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(script(vec![
        Step::data(role_chunk()),
        Step::data(reasoning_chunk("secret reasoning")),
        Step::data(text_delta_chunk("answer")),
        Step::data(finish_chunk("stop")),
        Step::data(usage_chunk(2, 2)),
        Step::done(),
    ]));
    let backend = backend_for(&provider, "5s", 0);

    let run = run(&backend).await;

    run.assert_clean();
    assert_eq!(run.texts(), vec!["answer"]);
}

#[tokio::test]
async fn a_length_finish_after_text_ends_cleanly() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(script(vec![
        Step::data(role_chunk()),
        Step::data(text_delta_chunk("partial answer")),
        Step::data(finish_chunk("length")),
        Step::data(usage_chunk(2, 2)),
        Step::done(),
    ]));
    let backend = backend_for(&provider, "5s", 0);

    let run = run(&backend).await;

    run.assert_clean();
    assert_eq!(run.texts(), vec!["partial answer"]);
}

// ---------------------------------------------------------------- idle timeout

#[tokio::test]
async fn a_stall_after_the_first_event_times_out_after_the_first_chunk_was_delivered() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(
        script(vec![
            Step::data(role_chunk()),
            Step::data(text_delta_chunk("hi")),
        ])
        .stall(),
    );
    let backend = backend_for(&provider, "300ms", 0);

    let run = run(&backend).await;

    assert_eq!(run.texts(), vec!["hi"]);
    assert!(
        matches!(run.cause(), CompletionError::Timeout),
        "{:?}",
        run.cause()
    );
    assert!(
        run.elapsed >= Duration::from_millis(250),
        "{:?}",
        run.elapsed
    );
    assert!(run.elapsed < Duration::from_secs(3), "{:?}", run.elapsed);
}

#[tokio::test]
async fn a_stream_that_sends_no_event_at_all_times_out() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(script(Vec::new()).stall());
    let backend = backend_for(&provider, "300ms", 0);

    let run = run(&backend).await;

    assert!(run.texts().is_empty());
    assert!(
        matches!(run.cause(), CompletionError::Timeout),
        "{:?}",
        run.cause()
    );
    assert!(run.elapsed < Duration::from_secs(3), "{:?}", run.elapsed);
}

#[tokio::test]
async fn a_first_event_later_than_the_idle_window_times_out_without_waiting_for_it() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(script(vec![
        Step::data(role_chunk()).after(Duration::from_millis(1500)),
    ]));
    let backend = backend_for(&provider, "300ms", 0);

    let run = run(&backend).await;

    assert!(
        matches!(run.cause(), CompletionError::Timeout),
        "{:?}",
        run.cause()
    );
    assert!(
        run.elapsed < Duration::from_millis(1200),
        "{:?}",
        run.elapsed
    );
}

#[tokio::test]
async fn a_response_that_does_not_open_within_the_idle_window_times_out() {
    let provider = FakeProvider::start().await.expect("provider");
    // Retries are not pinned by the plan for a timeout while opening: queue enough delayed
    // responses that either behaviour ends in a timeout, and assert only on the outcome.
    for _ in 0..4 {
        provider.enqueue(replying(&["never"]).delay_headers(Duration::from_secs(5)));
    }
    let backend = backend_for(&provider, "200ms", 1);

    let run = run(&backend).await;

    assert!(run.texts().is_empty(), "{:?}", run.items);
    assert!(
        matches!(run.cause(), CompletionError::Timeout),
        "{:?}",
        run.cause()
    );
    assert_eq!(
        map_completion_error_to_availability(run.cause(), &HostStatus::Unknown),
        Some(HostStatus::Unavailable(UnavailabilityReason::TimedOut))
    );
    assert!(run.elapsed < Duration::from_secs(3), "{:?}", run.elapsed);
}

#[tokio::test]
async fn reasoning_deltas_spaced_just_under_the_timeout_do_not_trip_it() {
    let provider = FakeProvider::start().await.expect("provider");
    let gap = Duration::from_millis(400);
    provider.enqueue(script(vec![
        Step::data(role_chunk()),
        Step::data(reasoning_chunk("one")).after(gap),
        Step::data(reasoning_chunk("two")).after(gap),
        Step::data(reasoning_chunk("three")).after(gap),
        Step::data(reasoning_chunk("four")).after(gap),
        Step::data(text_delta_chunk("done")).after(gap),
        Step::data(finish_chunk("stop")),
        Step::data(usage_chunk(1, 1)),
        Step::done(),
    ]));
    // The whole reply takes two seconds; no single gap exceeds the window.
    let backend = backend_for(&provider, "600ms", 0);

    let run = run(&backend).await;

    run.assert_clean();
    assert_eq!(run.texts(), vec!["done"]);
    assert!(
        run.elapsed >= Duration::from_millis(1800),
        "{:?}",
        run.elapsed
    );
}

#[tokio::test]
async fn keep_alive_comments_do_not_reset_the_idle_timeout() {
    let provider = FakeProvider::start().await.expect("provider");
    let gap = Duration::from_millis(200);
    let mut steps = vec![Step::data(role_chunk()), Step::data(text_delta_chunk("a"))];
    steps.extend((0..8).map(|_| Step::comment("keep-alive").after(gap)));
    provider.enqueue(script(steps).stall());
    let backend = backend_for(&provider, "500ms", 0);

    let run = run(&backend).await;

    assert_eq!(run.texts(), vec!["a"]);
    assert!(
        matches!(run.cause(), CompletionError::Timeout),
        "{:?}",
        run.cause()
    );
    assert!(
        run.elapsed < Duration::from_millis(1300),
        "{:?}",
        run.elapsed
    );
}

// ---------------------------------------------------------------- provider errors

#[tokio::test]
async fn a_401_surfaces_as_a_rejected_key_with_the_provider_code() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(ErrorScript::new(StatusCode::UNAUTHORIZED).body(key_rejected_body()));
    let backend = backend_for(&provider, "5s", 2);

    let run = run(&backend).await;

    match run.cause() {
        CompletionError::HttpError {
            status,
            message,
            code,
        } => {
            assert_eq!(*status, reqwest::StatusCode::UNAUTHORIZED);
            assert_eq!(message, "Wrong API Key");
            assert_eq!(code.as_deref(), Some("wrong_api_key"));
        }
        other => panic!("expected an HTTP error, got {other:?}"),
    }
    assert_eq!(
        map_completion_error_to_availability(run.cause(), &HostStatus::Unknown),
        Some(HostStatus::Unavailable(UnavailabilityReason::KeyRejected {
            host_code: Some("wrong_api_key".to_string())
        }))
    );
    assert_eq!(provider.request_count(), 1, "a 401 is not retried");
}

#[tokio::test]
async fn a_500_after_retries_is_a_provider_error_carrying_the_status() {
    let provider = FakeProvider::start().await.expect("provider");
    for _ in 0..3 {
        provider
            .enqueue(ErrorScript::new(StatusCode::INTERNAL_SERVER_ERROR).body("upstream broke"));
    }
    let backend = backend_for(&provider, "5s", 2);

    let run = run(&backend).await;

    match run.cause() {
        CompletionError::HttpError { status, .. } => {
            assert_eq!(*status, reqwest::StatusCode::INTERNAL_SERVER_ERROR);
        }
        other => panic!("expected an HTTP error, got {other:?}"),
    }
    assert_eq!(provider.request_count(), 3, "one attempt and two retries");
}

#[tokio::test]
async fn a_429_with_a_flat_error_body_keeps_its_status_and_code() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(
        ErrorScript::new(StatusCode::TOO_MANY_REQUESTS)
            .retry_after("3600")
            .body(flat_error_body(
                "Too many requests",
                "too_many_requests_error",
                None,
                "rate_limit_exceeded",
            )),
    );
    let backend = backend_for(&provider, "5s", 2);

    let run = run(&backend).await;

    match run.cause() {
        CompletionError::HttpError { status, code, .. } => {
            assert_eq!(*status, reqwest::StatusCode::TOO_MANY_REQUESTS);
            assert_eq!(code.as_deref(), Some("rate_limit_exceeded"));
        }
        other => panic!("expected an HTTP error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_402_keeps_its_status_so_it_can_be_told_from_a_rejected_key() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(
        ErrorScript::new(StatusCode::PAYMENT_REQUIRED).body(flat_error_body(
            "Out of credit",
            "payment_required",
            None,
            "insufficient_credit",
        )),
    );
    let backend = backend_for(&provider, "5s", 2);

    let run = run(&backend).await;

    match run.cause() {
        CompletionError::HttpError { status, code, .. } => {
            assert_eq!(*status, reqwest::StatusCode::PAYMENT_REQUIRED);
            assert_eq!(code.as_deref(), Some("insufficient_credit"));
        }
        other => panic!("expected an HTTP error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_non_json_error_body_still_keeps_the_status() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(ErrorScript::new(StatusCode::BAD_GATEWAY).body("<html>bad gateway</html>"));
    let backend = backend_for(&provider, "5s", 0);

    let run = run(&backend).await;

    match run.cause() {
        CompletionError::HttpError { status, code, .. } => {
            assert_eq!(*status, reqwest::StatusCode::BAD_GATEWAY);
            assert_eq!(*code, None);
        }
        other => panic!("expected an HTTP error, got {other:?}"),
    }
}

#[tokio::test]
async fn an_error_response_with_no_body_still_keeps_the_status() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(ErrorScript::new(StatusCode::SERVICE_UNAVAILABLE));
    let backend = backend_for(&provider, "5s", 0);

    let run = run(&backend).await;

    match run.cause() {
        CompletionError::HttpError { status, .. } => {
            assert_eq!(*status, reqwest::StatusCode::SERVICE_UNAVAILABLE);
        }
        other => panic!("expected an HTTP error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_connection_failure_is_a_connection_error_with_its_cause() {
    // A port nothing listens on: the provider's own port, once it is shut down.
    let provider = FakeProvider::start().await.expect("provider");
    let backend = backend_for(&provider, "5s", 1);
    drop(provider);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let run = run(&backend).await;

    match run.cause() {
        CompletionError::ConnectionError(_) => {}
        other => panic!("expected a connection error, got {other:?}"),
    }
    assert!(run.cause().source().is_some(), "the cause is attached");
}

// ---------------------------------------------------------------- finish reasons

#[tokio::test]
async fn a_content_filter_finish_with_no_text_errors_carrying_the_reason() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(script(vec![
        Step::data(role_chunk()),
        Step::data(finish_chunk("content_filter")),
        Step::data(usage_chunk(2, 0)),
        Step::done(),
    ]));
    let backend = backend_for(&provider, "5s", 0);

    let run = run(&backend).await;

    assert!(
        matches!(
            run.cause(),
            CompletionError::GenerationFailed(GenerationFailure::ContentFilter)
        ),
        "{:?}",
        run.cause()
    );
    assert!(run.texts().is_empty());
}

#[tokio::test]
async fn a_length_finish_with_no_text_errors_carrying_the_reason() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(script(vec![
        Step::data(role_chunk()),
        Step::data(reasoning_chunk("thought until the limit")),
        Step::data(finish_chunk("length")),
        Step::data(usage_chunk(2, 0)),
        Step::done(),
    ]));
    let backend = backend_for(&provider, "5s", 0);

    let run = run(&backend).await;

    assert!(
        matches!(
            run.cause(),
            CompletionError::GenerationFailed(GenerationFailure::LengthLimit)
        ),
        "{:?}",
        run.cause()
    );
}

#[tokio::test]
async fn a_tool_call_delta_is_an_error() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(script(vec![
        Step::data(role_chunk()),
        Step::data(tool_call_delta_chunk(0, "call_1", "get_weather", "{}")),
        Step::data(finish_chunk("tool_calls")),
        Step::done(),
    ]));
    let backend = backend_for(&provider, "5s", 0);

    let run = run(&backend).await;

    assert!(
        matches!(run.cause(), CompletionError::UnexpectedToolCall),
        "{:?}",
        run.cause()
    );
}

#[tokio::test]
async fn a_tool_call_after_text_still_ends_the_stream_with_the_error() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(script(vec![
        Step::data(role_chunk()),
        Step::data(text_delta_chunk("let me check")),
        Step::data(tool_call_delta_chunk(0, "call_1", "get_weather", "{}")),
        Step::done(),
    ]));
    let backend = backend_for(&provider, "5s", 0);

    let run = run(&backend).await;

    assert_eq!(run.texts(), vec!["let me check"]);
    assert!(matches!(run.cause(), CompletionError::UnexpectedToolCall));
}

// ---------------------------------------------------------------- broken streams

#[tokio::test]
async fn a_stream_that_ends_without_a_finish_reason_is_an_error_not_a_clean_end() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(script(vec![
        Step::data(role_chunk()),
        Step::data(text_delta_chunk("half a reply")),
    ]));
    let backend = backend_for(&provider, "5s", 0);

    let run = run(&backend).await;

    assert_eq!(run.texts(), vec!["half a reply"]);
    match run.cause() {
        CompletionError::StreamError {
            cause,
            provider_code,
            ..
        } => {
            assert!(cause.is_none(), "a truncation has no cause: {cause:?}");
            assert_eq!(*provider_code, None);
        }
        other => panic!("expected a stream error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_done_marker_without_a_finish_reason_is_also_a_truncation() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(script(vec![
        Step::data(role_chunk()),
        Step::data(text_delta_chunk("half a reply")),
        Step::done(),
    ]));
    let backend = backend_for(&provider, "5s", 0);

    let run = run(&backend).await;

    assert_eq!(run.texts(), vec!["half a reply"]);
    assert!(
        matches!(run.cause(), CompletionError::StreamError { .. }),
        "{:?}",
        run.cause()
    );
}

#[tokio::test]
async fn a_provider_error_event_mid_stream_carries_the_provider_code() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(script(vec![
        Step::data(role_chunk()),
        Step::data(text_delta_chunk("starting")),
        Step::data(
            r#"{"error":{"message":"queue overflow","type":"server_error","param":null,"code":"queue_overflow"}}"#,
        ),
    ]));
    let backend = backend_for(&provider, "5s", 0);

    let run = run(&backend).await;

    assert_eq!(run.texts(), vec!["starting"]);
    match run.cause() {
        CompletionError::StreamError { provider_code, .. } => {
            assert_eq!(provider_code.as_deref(), Some("queue_overflow"));
        }
        other => panic!("expected a stream error, got {other:?}"),
    }
}

#[tokio::test]
async fn an_unreadable_event_is_a_stream_error_that_keeps_its_cause() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(script(vec![
        Step::data(role_chunk()),
        Step::data(text_delta_chunk("before")),
        Step::data("this is not json"),
        Step::data(text_delta_chunk("after")),
    ]));
    let backend = backend_for(&provider, "5s", 0);

    let run = run(&backend).await;

    assert_eq!(run.texts(), vec!["before"]);
    match run.cause() {
        CompletionError::StreamError { cause, .. } => {
            assert!(cause.is_some(), "the underlying error is attached");
        }
        other => panic!("expected a stream error, got {other:?}"),
    }
}

#[tokio::test]
async fn nothing_follows_an_error() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(script(vec![
        Step::data(role_chunk()),
        Step::data(text_delta_chunk("a")),
        Step::data(tool_call_delta_chunk(0, "call_1", "f", "{}")),
        Step::data(text_delta_chunk("late")),
        Step::data(finish_chunk("stop")),
        Step::done(),
    ]));
    let backend = backend_for(&provider, "5s", 0);

    let run = run(&backend).await;

    assert!(run.items.last().expect("items").is_err(), "{:?}", run.items);
    assert_eq!(run.errors().len(), 1);
    assert_eq!(run.texts(), vec!["a"]);
}

// ---------------------------------------------------------------- the request

fn message_roles(body: &serde_json::Value) -> Vec<(String, String)> {
    body["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .map(|m| {
            (
                m["role"].as_str().expect("role").to_string(),
                m["content"].as_str().expect("text content").to_string(),
            )
        })
        .collect()
}

fn pair(role: &str, text: &str) -> (String, String) {
    (role.to_string(), text.to_string())
}

async fn recorded_after_one_reply(
    history: Vec<ConversationPart>,
    input: Vec<ConversationPart>,
) -> agent_core::backend::openai::fake_provider::RecordedRequest {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(replying(&["ok"]));
    let backend = backend_for(&provider, "5s", 0);
    run_with(&backend, history, input).await.assert_clean();
    assert_eq!(provider.request_count(), 1);
    provider.requests().remove(0)
}

#[tokio::test]
async fn the_request_carries_the_configured_model() {
    let request = recorded_after_one_reply(Vec::new(), user("hi")).await;

    assert_eq!(request.body["model"], TEST_MODEL);
}

#[tokio::test]
async fn the_request_streams_and_asks_for_a_usage_summary() {
    let request = recorded_after_one_reply(Vec::new(), user("hi")).await;

    assert_eq!(request.body["stream"], true);
    assert_eq!(request.body["stream_options"]["include_usage"], true);
}

#[tokio::test]
async fn the_request_sends_no_tools() {
    let request = recorded_after_one_reply(Vec::new(), user("hi")).await;

    let tools = request.body.get("tools");
    assert!(
        tools.is_none_or(|t| t.is_null() || t.as_array().is_some_and(Vec::is_empty)),
        "{tools:?}"
    );
    assert!(request.body.get("tool_choice").is_none_or(|v| v.is_null()));
}

#[tokio::test]
async fn the_request_sends_no_traceparent_header() {
    let request = recorded_after_one_reply(Vec::new(), user("hi")).await;

    assert_eq!(request.header("traceparent"), None);
    assert_eq!(request.header("tracestate"), None);
}

#[tokio::test]
async fn the_request_authenticates_with_the_configured_key() {
    let request = recorded_after_one_reply(Vec::new(), user("hi")).await;

    assert_eq!(
        request.header("authorization"),
        Some(format!("Bearer {TEST_KEY}").as_str())
    );
}

#[tokio::test]
async fn user_and_assistant_parts_keep_their_roles_and_history_comes_first() {
    let history = vec![
        part(Actor::User, "first question"),
        part(Actor::Assistant, "first answer"),
    ];

    let request = recorded_after_one_reply(history, user("second question")).await;

    assert_eq!(
        message_roles(&request.body),
        vec![
            pair("user", "first question"),
            pair("assistant", "first answer"),
            pair("user", "second question"),
        ]
    );
}

#[tokio::test]
async fn a_stored_system_entry_in_history_is_left_out() {
    let history = vec![
        part(Actor::System, "STORED-SYSTEM-ENTRY"),
        part(Actor::User, "earlier"),
        part(Actor::Assistant, "reply"),
    ];

    let request = recorded_after_one_reply(history, user("now")).await;

    let roles = message_roles(&request.body);
    assert!(roles.iter().all(|(role, _)| role != "system"), "{roles:?}");
    assert!(
        !request.body.to_string().contains("STORED-SYSTEM-ENTRY"),
        "the stored system entry was replayed"
    );
}

#[tokio::test]
async fn a_system_part_in_the_input_is_sent_as_system() {
    let input = vec![
        part(Actor::System, "CURRENT-INSTRUCTION"),
        part(Actor::User, "question"),
    ];

    let request = recorded_after_one_reply(Vec::new(), input).await;

    assert_eq!(
        message_roles(&request.body),
        vec![
            pair("system", "CURRENT-INSTRUCTION"),
            pair("user", "question")
        ]
    );
}

// ---------------------------------------------------------------- cancellation

#[tokio::test]
async fn dropping_the_stream_mid_reply_closes_the_provider_connection() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(
        script(vec![
            Step::data(role_chunk()),
            Step::data(text_delta_chunk("first")),
        ])
        .stall(),
    );
    let backend = backend_for(&provider, "30s", 0);
    let mut stream = backend.stream_conversation(Vec::new(), user("hello"));
    let first = timeout(LIMIT, stream.next())
        .await
        .expect("no chunk in time")
        .expect("the stream ended early")
        .expect("a chunk");
    assert_eq!(
        first,
        Chunk::Text {
            text: "first".to_string()
        }
    );
    assert_eq!(provider.disconnect_count(), 0);

    drop(stream);

    assert!(
        provider.wait_for_disconnect(Duration::from_secs(5)).await,
        "the provider never saw the client hang up"
    );
}

#[tokio::test]
async fn a_stream_dropped_before_its_first_poll_sends_no_request() {
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(replying(&["never"]));
    let backend = backend_for(&provider, "5s", 0);

    let stream = backend.stream_conversation(Vec::new(), user("hello"));
    drop(stream);
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert_eq!(provider.request_count(), 0);
}

#[tokio::test]
async fn dropping_the_stream_while_the_request_is_still_opening_cancels_it() {
    // The provider answers 503 twice before the reply; the stream is dropped during the retries.
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(ErrorScript::new(StatusCode::TOO_MANY_REQUESTS).retry_after("1"));
    provider.enqueue(replying(&["never delivered"]));
    let backend = backend_for(&provider, "30s", 3);
    let mut stream = backend.stream_conversation(Vec::new(), user("hello"));

    // Poll once, long enough for the first attempt to have been made, then hang up.
    let polled = timeout(Duration::from_millis(300), stream.next()).await;
    assert!(polled.is_err(), "nothing can arrive while the retry waits");
    drop(stream);
    tokio::time::sleep(Duration::from_millis(1500)).await;

    assert_eq!(
        provider.request_count(),
        1,
        "the retry must not outlive the dropped stream"
    );
}

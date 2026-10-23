//! The spans the OpenAI-compatible backend emits around the provider call, captured in-process:
//! one call span per call, a first-text event, one child span per HTTP attempt, and nothing that
//! carries the prompt, the reply or the key.
//!
//! The plan fixes the attribute names to the OpenTelemetry GenAI semantic conventions and the
//! `error.type` to the `CompletionError` case, but not the names of the spans or of the first-text
//! event. So the call span is found by its `gen_ai.operation.name` field, attempt spans are the
//! call span's children, and the first-text event is an event of the call span whose name or
//! message mentions "first".

mod common;

use agent_core::backend::Backend;
use agent_core::backend::openai::OpenAIBackend;
use agent_core::backend::openai::fake_provider::{
    ErrorScript, FAKE_MODEL, FakeProvider, Step, StreamScript, default_happy_path, finish_chunk,
    flat_error_body, reasoning_chunk, role_chunk, text_delta_chunk, tool_call_delta_chunk,
    usage_chunk,
};
use agent_core::health::HostStatus;
use agent_core::history::InMemoryHistory;
use agent_core::service::Service;
use axum::http::StatusCode;
use common::{TEST_KEY, TEST_MODEL, backend_for, normalize};
use proto::agent_core::v1::DecideRequest;
use proto::agent_core::v1::agent_core_client::AgentCoreClient;
use proto::agent_core::v1::agent_core_server::AgentCoreServer;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tokio::time::timeout;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Server};
use tracing::field::{Field, Visit};
use tracing::{Event as TracingEvent, Subscriber, span};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;

/// Upper bound for anything that could hang, so a failure reports instead of blocking.
const LIMIT: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------- capture

#[derive(Debug, Default, Clone)]
struct SpanRecord {
    id: u64,
    name: String,
    target: String,
    parent: Option<u64>,
    fields: HashMap<String, String>,
    closed: bool,
}

impl SpanRecord {
    fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }
}

#[derive(Debug, Clone)]
struct EventRecord {
    name: String,
    target: String,
    parent: Option<u64>,
    fields: HashMap<String, String>,
}

#[derive(Default)]
struct Captured {
    spans: Vec<SpanRecord>,
    events: Vec<EventRecord>,
}

/// Records spans (with the last value recorded per field, their parent and whether they have
/// closed) and events with their parent span.
#[derive(Clone, Default)]
struct Capture {
    state: Arc<Mutex<Captured>>,
    changed: Arc<Notify>,
}

struct FieldMap<'a>(&'a mut HashMap<String, String>);

impl Visit for FieldMap<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_string(), value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().to_string(), format!("{value:?}"));
    }
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Capture {
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let mut record = SpanRecord {
            id: id.into_u64(),
            name: attrs.metadata().name().to_string(),
            target: attrs.metadata().target().to_string(),
            parent: ctx
                .span(id)
                .and_then(|span| span.parent())
                .map(|parent| parent.id().into_u64()),
            ..SpanRecord::default()
        };
        attrs.record(&mut FieldMap(&mut record.fields));
        self.state.lock().expect("capture").spans.push(record);
        self.changed.notify_waiters();
    }

    fn on_record(&self, id: &span::Id, values: &span::Record<'_>, _ctx: Context<'_, S>) {
        let mut state = self.state.lock().expect("capture");
        if let Some(record) = state.spans.iter_mut().find(|s| s.id == id.into_u64()) {
            values.record(&mut FieldMap(&mut record.fields));
        }
        drop(state);
        self.changed.notify_waiters();
    }

    fn on_event(&self, event: &TracingEvent<'_>, ctx: Context<'_, S>) {
        let mut fields = HashMap::new();
        event.record(&mut FieldMap(&mut fields));
        self.state
            .lock()
            .expect("capture")
            .events
            .push(EventRecord {
                name: event.metadata().name().to_string(),
                target: event.metadata().target().to_string(),
                parent: ctx.event_span(event).map(|span| span.id().into_u64()),
                fields,
            });
        self.changed.notify_waiters();
    }

    fn on_close(&self, id: span::Id, _ctx: Context<'_, S>) {
        let mut state = self.state.lock().expect("capture");
        if let Some(record) = state.spans.iter_mut().find(|s| s.id == id.into_u64()) {
            record.closed = true;
        }
        drop(state);
        self.changed.notify_waiters();
    }
}

impl Capture {
    fn install() -> (Self, tracing::subscriber::DefaultGuard) {
        common::keep_tracing_interest_global();
        let capture = Self::default();
        let subscriber = tracing_subscriber::registry().with(capture.clone());
        (capture, tracing::subscriber::set_default(subscriber))
    }

    fn spans(&self) -> Vec<SpanRecord> {
        self.state.lock().expect("capture").spans.clone()
    }

    fn events(&self) -> Vec<EventRecord> {
        self.state.lock().expect("capture").events.clone()
    }

    /// The backend's call spans, oldest first.
    fn call_spans(&self) -> Vec<SpanRecord> {
        self.spans()
            .into_iter()
            .filter(|s| s.field("gen_ai.operation.name").is_some())
            .collect()
    }

    fn decide_spans(&self) -> Vec<SpanRecord> {
        self.spans()
            .into_iter()
            .filter(|s| s.name == "decide")
            .collect()
    }

    /// The child spans of `parent` (one per HTTP attempt, under a call span).
    fn children_of(&self, parent: &SpanRecord) -> Vec<SpanRecord> {
        self.spans()
            .into_iter()
            .filter(|s| s.parent == Some(parent.id))
            .collect()
    }

    /// The events whose span is `span`.
    fn events_of(&self, span: &SpanRecord) -> Vec<EventRecord> {
        self.events()
            .into_iter()
            .filter(|e| e.parent == Some(span.id))
            .collect()
    }

    /// Waits (woken by every recorded change, bounded by `LIMIT`) until `ready` holds.
    async fn wait_until(&self, what: &str, ready: impl Fn(&Self) -> bool) {
        let waited = timeout(LIMIT, async {
            loop {
                let notified = self.changed.notified();
                if ready(self) {
                    return;
                }
                // Re-check on a timer as well, so a change between the check and the wait is
                // never missed.
                let _ = timeout(Duration::from_millis(20), notified).await;
            }
        })
        .await;
        assert!(waited.is_ok(), "timed out waiting for {what}");
    }

    /// Waits for `count` call spans that have all closed.
    async fn wait_for_closed_calls(&self, count: usize) -> Vec<SpanRecord> {
        self.wait_until(&format!("{count} closed call span(s)"), |c| {
            let calls = c.call_spans();
            calls.len() >= count && calls.iter().all(|s| s.closed)
        })
        .await;
        self.call_spans()
    }
}

fn is_first_text_event(event: &EventRecord) -> bool {
    let message = event.fields.get("message").cloned().unwrap_or_default();
    normalize(&format!("{} {message}", event.name)).contains("first")
}

// ---------------------------------------------------------------- harness

async fn serve(backend: OpenAIBackend) -> AgentCoreClient<Channel> {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let service = Service::new(backend, InMemoryHistory::default());
    tokio::spawn(async move {
        Server::builder()
            .add_service(AgentCoreServer::new(service))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .expect("server");
    });
    timeout(LIMIT, AgentCoreClient::connect(format!("http://{addr}")))
        .await
        .expect("connect timed out")
        .expect("connect")
}

/// Runs one `Decide` turn to the end of its stream, ignoring how it ended.
async fn decide(client: &mut AgentCoreClient<Channel>, session: &str, text: &str) {
    let request = DecideRequest {
        session_id: session.to_string(),
        speaker_name: None,
        text: text.to_string(),
    };
    timeout(LIMIT, async {
        if let Ok(response) = client.decide(request).await {
            let mut stream = response.into_inner();
            while stream.next().await.is_some() {}
        }
    })
    .await
    .expect("the turn did not finish");
}

fn reply() -> StreamScript {
    default_happy_path()
}

// ---------------------------------------------------------------- the call span

#[tokio::test]
async fn a_successful_call_has_one_call_span_with_model_finish_reason_and_token_counts() {
    let (capture, _guard) = Capture::install();
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(reply());
    let mut client = serve(backend_for(&provider, "5s", 0)).await;

    decide(&mut client, "s1", "hello").await;

    let calls = capture.wait_for_closed_calls(1).await;
    assert_eq!(calls.len(), 1, "{calls:?}");
    let call = &calls[0];
    assert_eq!(
        call.field("gen_ai.operation.name"),
        Some("chat"),
        "{call:?}"
    );
    assert!(
        call.field("gen_ai.provider.name").is_some() || call.field("gen_ai.system").is_some(),
        "no provider attribute: {call:?}"
    );
    assert!(
        call.field("server.address")
            .is_some_and(|a| a.contains("127.0.0.1")),
        "{call:?}"
    );
    assert_eq!(
        call.field("gen_ai.request.model"),
        Some(TEST_MODEL),
        "{call:?}"
    );
    assert_eq!(
        call.field("gen_ai.response.model"),
        Some(FAKE_MODEL),
        "{call:?}"
    );
    assert!(
        call.field("gen_ai.response.finish_reasons")
            .is_some_and(|r| r.contains("stop")),
        "{call:?}"
    );
    assert_eq!(
        call.field("gen_ai.usage.input_tokens"),
        Some("12"),
        "{call:?}"
    );
    assert_eq!(
        call.field("gen_ai.usage.output_tokens"),
        Some("9"),
        "{call:?}"
    );
    assert_eq!(call.field("error.type"), None, "{call:?}");
}

#[tokio::test]
async fn a_length_finish_after_text_records_its_finish_reason_without_an_error() {
    let (capture, _guard) = Capture::install();
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(StreamScript::new(vec![
        Step::data(role_chunk()),
        Step::data(text_delta_chunk("cut off")),
        Step::data(finish_chunk("length")),
        Step::data(usage_chunk(5, 7)),
        Step::done(),
    ]));
    let mut client = serve(backend_for(&provider, "5s", 0)).await;

    decide(&mut client, "s1", "hello").await;

    let calls = capture.wait_for_closed_calls(1).await;
    assert!(
        calls[0]
            .field("gen_ai.response.finish_reasons")
            .is_some_and(|r| r.contains("length")),
        "{:?}",
        calls[0]
    );
    assert_eq!(calls[0].field("error.type"), None, "{:?}", calls[0]);
}

#[tokio::test]
async fn the_call_span_is_a_child_of_the_decide_span() {
    let (capture, _guard) = Capture::install();
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(reply());
    let mut client = serve(backend_for(&provider, "5s", 0)).await;

    decide(&mut client, "s1", "hello").await;

    let calls = capture.wait_for_closed_calls(1).await;
    let decides = capture.decide_spans();
    assert_eq!(decides.len(), 1, "{decides:?}");
    assert_eq!(calls[0].parent, Some(decides[0].id), "{:?}", calls[0]);
}

#[tokio::test]
async fn the_call_span_closes_only_after_the_last_chunk() {
    let (capture, _guard) = Capture::install();
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(StreamScript::new(vec![
        Step::data(role_chunk()),
        Step::data(text_delta_chunk("first")),
        Step::data(text_delta_chunk("second")).after(Duration::from_millis(600)),
        Step::data(finish_chunk("stop")),
        Step::data(usage_chunk(1, 1)),
        Step::done(),
    ]));
    let mut client = serve(backend_for(&provider, "5s", 0)).await;
    let request = DecideRequest {
        session_id: "s1".to_string(),
        speaker_name: None,
        text: "hello".to_string(),
    };
    let mut stream = client.decide(request).await.expect("decide").into_inner();

    timeout(LIMIT, stream.next())
        .await
        .expect("no first chunk")
        .expect("stream ended")
        .expect("status");

    let calls = capture.call_spans();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(
        !calls[0].closed,
        "the call span closed after the first chunk, before the stream ended"
    );
    while timeout(LIMIT, stream.next())
        .await
        .expect("rest timed out")
        .is_some()
    {}
    capture.wait_for_closed_calls(1).await;
}

// ---------------------------------------------------------------- the first-text event

#[tokio::test]
async fn exactly_one_first_text_event_marks_the_first_text_chunk() {
    let (capture, _guard) = Capture::install();
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(reply());
    let mut client = serve(backend_for(&provider, "5s", 0)).await;

    decide(&mut client, "s1", "hello").await;

    let calls = capture.wait_for_closed_calls(1).await;
    let firsts: Vec<_> = capture
        .events_of(&calls[0])
        .into_iter()
        .filter(is_first_text_event)
        .collect();
    assert_eq!(firsts.len(), 1, "{:#?}", capture.events_of(&calls[0]));
}

#[tokio::test]
async fn a_call_that_produced_no_text_has_no_first_text_event() {
    let (capture, _guard) = Capture::install();
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(StreamScript::new(vec![
        Step::data(role_chunk()),
        Step::data(reasoning_chunk("only thinking")),
        Step::data(finish_chunk("content_filter")),
        Step::data(usage_chunk(1, 0)),
        Step::done(),
    ]));
    let mut client = serve(backend_for(&provider, "5s", 0)).await;

    decide(&mut client, "s1", "hello").await;

    let calls = capture.wait_for_closed_calls(1).await;
    assert!(
        capture
            .events_of(&calls[0])
            .iter()
            .all(|e| !is_first_text_event(e)),
        "{:#?}",
        capture.events_of(&calls[0])
    );
}

// ---------------------------------------------------------------- attempt spans

#[tokio::test]
async fn a_call_without_retries_has_one_attempt_span_recording_the_status() {
    let (capture, _guard) = Capture::install();
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(reply());
    let mut client = serve(backend_for(&provider, "5s", 3)).await;

    decide(&mut client, "s1", "hello").await;

    let calls = capture.wait_for_closed_calls(1).await;
    let attempts = capture.children_of(&calls[0]);
    assert_eq!(attempts.len(), 1, "{attempts:?}");
    assert!(
        attempts[0].fields.values().any(|v| v == "200"),
        "the attempt span does not record the status: {:?}",
        attempts[0]
    );
}

#[tokio::test]
async fn a_call_that_retried_once_has_two_attempt_spans() {
    let (capture, _guard) = Capture::install();
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(ErrorScript::new(StatusCode::SERVICE_UNAVAILABLE));
    provider.enqueue(reply());
    let mut client = serve(backend_for(&provider, "5s", 3)).await;

    decide(&mut client, "s1", "hello").await;

    let calls = capture.wait_for_closed_calls(1).await;
    let mut attempts = capture.children_of(&calls[0]);
    attempts.sort_by_key(|s| s.id);
    assert_eq!(attempts.len(), 2, "{attempts:?}");
    assert!(
        attempts[0].fields.values().any(|v| v == "503"),
        "the first attempt span does not record the 503: {:?}",
        attempts[0]
    );
    assert!(
        attempts[1].fields.values().any(|v| v == "200"),
        "the second attempt span does not record the 200: {:?}",
        attempts[1]
    );
    assert_eq!(provider.request_count(), 2);
}

#[tokio::test]
async fn every_retry_adds_an_attempt_span() {
    let (capture, _guard) = Capture::install();
    let provider = FakeProvider::start().await.expect("provider");
    for _ in 0..3 {
        provider.enqueue(ErrorScript::new(StatusCode::BAD_GATEWAY));
    }
    let mut client = serve(backend_for(&provider, "5s", 2)).await;

    decide(&mut client, "s1", "hello").await;

    let calls = capture.wait_for_closed_calls(1).await;
    assert_eq!(capture.children_of(&calls[0]).len(), 3);
}

#[tokio::test]
async fn an_attempt_that_cannot_connect_records_the_connection_failure() {
    let (capture, _guard) = Capture::install();
    let provider = FakeProvider::start().await.expect("provider");
    let backend = backend_for(&provider, "5s", 0);
    drop(provider);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut client = serve(backend).await;

    decide(&mut client, "s1", "hello").await;

    let calls = capture.wait_for_closed_calls(1).await;
    let attempts = capture.children_of(&calls[0]);
    assert_eq!(attempts.len(), 1, "{attempts:?}");
    let recorded: String = attempts[0]
        .fields
        .values()
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        normalize(&recorded).contains("connect") || normalize(&recorded).contains("error"),
        "the attempt span records no failure: {:?}",
        attempts[0]
    );
}

// ---------------------------------------------------------------- error.type

#[tokio::test]
async fn a_timed_out_call_carries_the_timeout_error_type_and_so_does_a_second_one() {
    let (capture, _guard) = Capture::install();
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(StreamScript::new(vec![Step::data(role_chunk())]).stall());
    provider.enqueue(StreamScript::new(vec![Step::data(role_chunk())]).stall());
    let backend = backend_for(&provider, "300ms", 0);
    let health = backend.subscribe_health();
    let mut client = serve(backend).await;

    decide(&mut client, "first", "hello").await;
    decide(&mut client, "second", "hello").await;

    let calls = capture.wait_for_closed_calls(2).await;
    assert_eq!(calls.len(), 2, "{calls:?}");
    for call in &calls {
        let error_type = call
            .field("error.type")
            .unwrap_or_else(|| panic!("no error.type: {call:?}"));
        assert!(normalize(error_type).contains("timeout"), "{error_type}");
    }
    // The second timeout changed nothing about availability, yet its span still names the error.
    timeout(LIMIT, async {
        while !matches!(health.borrow().host_status(), HostStatus::Unavailable(_)) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the provider never became unavailable");
}

#[tokio::test]
async fn a_content_filter_failure_carries_its_own_error_type_while_availability_stays_available() {
    let (capture, _guard) = Capture::install();
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(StreamScript::new(vec![
        Step::data(role_chunk()),
        Step::data(finish_chunk("content_filter")),
        Step::data(usage_chunk(1, 0)),
        Step::done(),
    ]));
    let backend = backend_for(&provider, "5s", 0);
    let health = backend.subscribe_health();
    let mut client = serve(backend).await;

    decide(&mut client, "s1", "hello").await;

    let calls = capture.wait_for_closed_calls(1).await;
    let error_type = calls[0]
        .field("error.type")
        .unwrap_or_else(|| panic!("no error.type: {:?}", calls[0]));
    assert!(normalize(error_type).contains("generation"), "{error_type}");
    assert_eq!(*health.borrow().host_status(), HostStatus::Available);
}

#[tokio::test]
async fn every_failure_case_names_itself_in_error_type() {
    let cases: Vec<(&str, StreamOrError)> = vec![
        (
            "http",
            StreamOrError::Error(
                ErrorScript::new(StatusCode::UNAUTHORIZED).body(flat_error_body(
                    "Wrong API Key",
                    "invalid_request_error",
                    Some("api_key"),
                    "wrong_api_key",
                )),
            ),
        ),
        (
            "stream",
            StreamOrError::Stream(StreamScript::new(vec![
                Step::data(role_chunk()),
                Step::data(text_delta_chunk("truncated")),
            ])),
        ),
        (
            "toolcall",
            StreamOrError::Stream(StreamScript::new(vec![
                Step::data(role_chunk()),
                Step::data(tool_call_delta_chunk(0, "call_1", "f", "{}")),
                Step::done(),
            ])),
        ),
    ];
    for (expected, case) in cases {
        let (capture, _guard) = Capture::install();
        let provider = FakeProvider::start().await.expect("provider");
        match case {
            StreamOrError::Error(script) => provider.enqueue(script),
            StreamOrError::Stream(script) => provider.enqueue(script),
        }
        let mut client = serve(backend_for(&provider, "5s", 0)).await;

        decide(&mut client, "s1", "hello").await;

        let calls = capture.wait_for_closed_calls(1).await;
        let error_type = calls[0]
            .field("error.type")
            .unwrap_or_else(|| panic!("{expected}: no error.type: {:?}", calls[0]));
        assert!(
            normalize(error_type).contains(expected),
            "expected {expected:?} in error.type, got {error_type:?}"
        );
    }
}

enum StreamOrError {
    Error(ErrorScript),
    Stream(StreamScript),
}

#[tokio::test]
async fn a_connection_failure_carries_the_connection_error_type() {
    let (capture, _guard) = Capture::install();
    let provider = FakeProvider::start().await.expect("provider");
    let backend = backend_for(&provider, "5s", 0);
    drop(provider);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut client = serve(backend).await;

    decide(&mut client, "s1", "hello").await;

    let calls = capture.wait_for_closed_calls(1).await;
    let error_type = calls[0]
        .field("error.type")
        .unwrap_or_else(|| panic!("no error.type: {:?}", calls[0]));
    assert!(normalize(error_type).contains("connection"), "{error_type}");
}

#[tokio::test]
async fn the_attempt_span_and_the_call_span_name_a_connection_failure_the_same() {
    let (capture, _guard) = Capture::install();
    let provider = FakeProvider::start().await.expect("provider");
    let backend = backend_for(&provider, "5s", 0);
    drop(provider);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut client = serve(backend).await;

    decide(&mut client, "s1", "hello").await;

    let calls = capture.wait_for_closed_calls(1).await;
    let attempts = capture.children_of(&calls[0]);
    assert_eq!(attempts.len(), 1, "{attempts:?}");
    assert_eq!(
        attempts[0].field("error.type"),
        calls[0].field("error.type"),
        "one vocabulary: {attempts:?} / {calls:?}"
    );
    assert_eq!(calls[0].field("error.type"), Some("connection_error"));
}

// ---------------------------------------------------------------- no secrets

#[tokio::test]
async fn no_captured_field_contains_the_prompt_the_reply_or_the_key() {
    let (capture, _guard) = Capture::install();
    let provider = FakeProvider::start().await.expect("provider");
    provider.enqueue(StreamScript::new(vec![
        Step::data(role_chunk()),
        Step::data(reasoning_chunk("REASONING-CANARY-1177")),
        Step::data(text_delta_chunk("REPLY-CANARY-5530 ")),
        Step::data(text_delta_chunk("and more of the reply")),
        Step::data(finish_chunk("stop")),
        Step::data(usage_chunk(4, 6)),
        Step::done(),
    ]));
    provider.enqueue(reply());
    let mut client = serve(backend_for(&provider, "5s", 1)).await;

    // The second turn carries the first exchange as history.
    decide(&mut client, "s1", "PROMPT-CANARY-8841").await;
    decide(&mut client, "s1", "SECOND-PROMPT-CANARY-2290").await;

    capture.wait_for_closed_calls(2).await;
    let canaries = [
        "PROMPT-CANARY-8841",
        "SECOND-PROMPT-CANARY-2290",
        "REPLY-CANARY-5530",
        "and more of the reply",
        "REASONING-CANARY-1177",
        TEST_KEY,
        "0123456789",
    ];
    let ours = |target: &str| target.starts_with("agent_core");
    let mut seen = 0;
    for span in capture.spans().into_iter().filter(|s| ours(&s.target)) {
        for (name, value) in span
            .fields
            .iter()
            .chain([(&"name".to_string(), &span.name)])
        {
            seen += 1;
            for canary in canaries {
                assert!(
                    !value.contains(canary),
                    "span {:?} field {name:?} contains {canary:?}: {value:?}",
                    span.name
                );
            }
        }
    }
    for event in capture.events().into_iter().filter(|e| ours(&e.target)) {
        for (name, value) in &event.fields {
            seen += 1;
            for canary in canaries {
                assert!(
                    !value.contains(canary),
                    "event {:?} field {name:?} contains {canary:?}: {value:?}",
                    event.name
                );
            }
        }
    }
    assert!(seen > 0, "the capture saw nothing of this crate");
}

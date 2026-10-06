//! `Decide` over a real in-process gRPC server and client (tonic over loopback TCP).
//! The handler is never called directly: every assertion is about what the caller receives.

use agent_core::backend::scripted::{ScriptStep, ScriptedBackend};
use agent_core::backend::{Backend, BackendError, Chunk};
use agent_core::history::{
    Actor, Content, ConversationPart, HistoryError, HistoryResult, HistoryStore, InMemoryHistory,
    Token,
};
use agent_core::service::Service;
use futures_core::Stream;
use proto::agent_core::v1::agent_core_client::AgentCoreClient;
use proto::agent_core::v1::agent_core_server::{AgentCore, AgentCoreServer};
use proto::agent_core::v1::{DecideRequest, decide_response};
use std::collections::HashMap;
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{Notify, Semaphore};
use tokio::time::timeout;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Server};
use tonic::{Code, Status};
use tracing::field::{Field, Visit};
use tracing::{Event as TracingEvent, Subscriber, span};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;

/// Upper bound for anything that could hang, so a failure reports instead of blocking.
const LIMIT: Duration = Duration::from_secs(10);

type Client = AgentCoreClient<Channel>;
type BackendStream = Pin<Box<dyn Stream<Item = Result<Chunk, BackendError>> + Send + 'static>>;

// ---------------------------------------------------------------- harness

async fn serve<B, H>(backend: B, store: H) -> Client
where
    B: Backend + 'static,
    H: HistoryStore + Clone + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let service = Service::new(backend, store);
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

fn request(session_id: &str, text: &str) -> DecideRequest {
    DecideRequest {
        session_id: session_id.to_string(),
        speaker_name: None,
        text: text.to_string(),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Delta(String),
    Complete,
}

fn delta(s: &str) -> Event {
    Event::Delta(s.to_string())
}

/// What the caller saw: events in arrival order, and the error status that ended the call, if any.
#[derive(Debug)]
struct Outcome {
    events: Vec<Event>,
    status: Option<Status>,
}

impl Outcome {
    fn completes(&self) -> usize {
        self.events
            .iter()
            .filter(|e| matches!(e, Event::Complete))
            .count()
    }

    fn code(&self) -> Option<Code> {
        self.status.as_ref().map(Status::code)
    }
}

fn to_event(response: proto::agent_core::v1::DecideResponse) -> Event {
    match response.payload.expect("payload set") {
        decide_response::Payload::TextDelta(text) => Event::Delta(text),
        decide_response::Payload::TurnComplete(_) => Event::Complete,
    }
}

/// Runs one `Decide` to the end of its stream. An error may arrive at call time or mid-stream.
async fn decide(client: &mut Client, session_id: &str, text: &str) -> Outcome {
    timeout(LIMIT, async {
        let mut events = Vec::new();
        let mut stream = match client.decide(request(session_id, text)).await {
            Ok(response) => response.into_inner(),
            Err(status) => {
                return Outcome {
                    events,
                    status: Some(status),
                };
            }
        };
        while let Some(item) = stream.next().await {
            match item {
                Ok(response) => events.push(to_event(response)),
                Err(status) => {
                    return Outcome {
                        events,
                        status: Some(status),
                    };
                }
            }
        }
        Outcome {
            events,
            status: None,
        }
    })
    .await
    .expect("Decide did not finish in time")
}

fn chunk(s: &str) -> ScriptStep {
    ScriptStep::Chunk(text(s))
}

fn text(s: &str) -> Chunk {
    Chunk::Text {
        text: s.to_string(),
    }
}

fn part(actor: Actor, s: &str) -> ConversationPart {
    ConversationPart {
        content: Content::Text {
            text: s.to_string(),
        },
        actor,
    }
}

fn fail() -> ScriptStep {
    ScriptStep::Error(BackendError::UnexpectedError)
}

// ---------------------------------------------------------------- test doubles

/// Emits "a", waits for a permit on the gate, then emits "b" and ends cleanly.
#[derive(Clone)]
struct GatedBackend {
    gate: Arc<Semaphore>,
    calls: Arc<AtomicUsize>,
}

impl GatedBackend {
    fn new() -> Self {
        Self {
            gate: Arc::new(Semaphore::new(0)),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl Backend for GatedBackend {
    fn stream_conversation(
        &self,
        _history: Vec<ConversationPart>,
        _input: Vec<ConversationPart>,
    ) -> BackendStream {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let gate = self.gate.clone();
        let first = tokio_stream::once::<Result<Chunk, BackendError>>(Ok(text("a")));
        let second = tokio_stream::once(()).then(move |()| {
            let gate = gate.clone();
            async move {
                gate.acquire().await.expect("gate open").forget();
                Ok(text("b"))
            }
        });
        Box::pin(first.chain(second))
    }
}

/// First call: emits "a" and then never finishes. Later calls: emit "ok" and end cleanly.
/// Records the history each call received.
#[derive(Clone, Default)]
struct StallFirstCallBackend {
    histories: Arc<Mutex<Vec<Vec<ConversationPart>>>>,
}

impl Backend for StallFirstCallBackend {
    fn stream_conversation(
        &self,
        history: Vec<ConversationPart>,
        _input: Vec<ConversationPart>,
    ) -> BackendStream {
        let mut histories = self.histories.lock().unwrap();
        let first_call = histories.is_empty();
        histories.push(history);
        let a = tokio_stream::once::<Result<Chunk, BackendError>>(Ok(text("a")));
        if first_call {
            Box::pin(a.chain(tokio_stream::pending()))
        } else {
            Box::pin(tokio_stream::once(Ok(text("ok"))))
        }
    }
}

/// An in-memory store whose commit always fails (the turn is consumed, so the session is freed).
#[derive(Clone, Default)]
struct FailingCommitStore {
    inner: InMemoryHistory,
}

impl HistoryStore for FailingCommitStore {
    type Turn = Token;

    async fn start_turn(&self, session_id: &str) -> HistoryResult<(Token, Vec<ConversationPart>)> {
        self.inner.start_turn(session_id).await
    }

    async fn commit(&self, _token: Token, _new_parts: Vec<ConversationPart>) -> HistoryResult<()> {
        Err(HistoryError::StoreUnavailable)
    }

    async fn abort(&self, token: Token) -> HistoryResult<()> {
        self.inner.abort(token).await
    }
}

/// An in-memory store whose `start_turn` always fails.
#[derive(Clone, Default)]
struct FailingStartStore {
    inner: InMemoryHistory,
}

impl HistoryStore for FailingStartStore {
    type Turn = Token;

    async fn start_turn(&self, _session_id: &str) -> HistoryResult<(Token, Vec<ConversationPart>)> {
        Err(HistoryError::StoreUnavailable)
    }

    async fn commit(&self, token: Token, new_parts: Vec<ConversationPart>) -> HistoryResult<()> {
        self.inner.commit(token, new_parts).await
    }

    async fn abort(&self, token: Token) -> HistoryResult<()> {
        self.inner.abort(token).await
    }
}

/// An in-memory store whose commit never finishes, so a turn can be interrupted mid-commit.
#[derive(Clone, Default)]
struct StalledCommitStore {
    inner: InMemoryHistory,
}

impl HistoryStore for StalledCommitStore {
    type Turn = Token;

    async fn start_turn(&self, session_id: &str) -> HistoryResult<(Token, Vec<ConversationPart>)> {
        self.inner.start_turn(session_id).await
    }

    async fn commit(&self, _token: Token, _new_parts: Vec<ConversationPart>) -> HistoryResult<()> {
        std::future::pending().await
    }

    async fn abort(&self, token: Token) -> HistoryResult<()> {
        self.inner.abort(token).await
    }
}

/// Captures log output of the current thread. Tests using it run on the default current-thread
/// runtime, so the server tasks log on the same thread.
#[derive(Clone, Default)]
struct LogBuffer(Arc<Mutex<Vec<u8>>>);

impl io::Write for LogBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogBuffer {
    type Writer = LogBuffer;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

impl LogBuffer {
    fn lines(&self) -> Vec<String> {
        String::from_utf8_lossy(&self.0.lock().unwrap())
            .lines()
            .map(str::to_string)
            .collect()
    }
}

// ---------------------------------------------------------------- AC1, AC2

#[tokio::test]
async fn three_chunks_arrive_in_order_then_exactly_one_turn_complete() {
    let backend = ScriptedBackend::new(vec![chunk("one "), chunk("two "), chunk("three")]);
    let mut client = serve(backend, InMemoryHistory::default()).await;

    let outcome = decide(&mut client, "s1", "hello").await;

    assert!(outcome.status.is_none(), "{outcome:?}");
    assert_eq!(
        outcome.events,
        vec![
            delta("one "),
            delta("two "),
            delta("three"),
            Event::Complete
        ]
    );
}

#[tokio::test]
async fn first_chunk_reaches_the_caller_before_the_backend_finishes() {
    let backend = GatedBackend::new();
    let gate = backend.gate.clone();
    let mut client = serve(backend, InMemoryHistory::default()).await;

    let mut stream = client
        .decide(request("s1", "hello"))
        .await
        .expect("decide")
        .into_inner();
    let first = timeout(LIMIT, stream.next())
        .await
        .expect("no chunk while the backend was still running")
        .expect("stream ended early")
        .expect("status");
    assert_eq!(to_event(first), delta("a"));

    gate.add_permits(1);
    let rest: Vec<_> = timeout(LIMIT, stream.collect::<Vec<_>>())
        .await
        .expect("rest timed out");
    let rest: Vec<Event> = rest.into_iter().map(|r| to_event(r.unwrap())).collect();
    assert_eq!(rest, vec![delta("b"), Event::Complete]);
}

#[tokio::test]
async fn second_decide_on_same_session_passes_first_exchange_to_backend() {
    let backend = ScriptedBackend::new(vec![chunk("one "), chunk("two")]);
    let mut client = serve(backend.clone(), InMemoryHistory::default()).await;

    decide(&mut client, "s1", "hello").await;
    let outcome = decide(&mut client, "s1", "again").await;

    assert!(outcome.status.is_none(), "{outcome:?}");
    let calls = backend.calls();
    assert_eq!(calls.len(), 2);
    assert!(calls[0].history.is_empty());
    assert_eq!(
        calls[1].history,
        vec![
            part(Actor::User, "hello"),
            part(Actor::Assistant, "one two")
        ]
    );
    assert_eq!(calls[1].input, vec![part(Actor::User, "again")]);
}

#[tokio::test]
async fn sessions_do_not_share_history() {
    let backend = ScriptedBackend::new(vec![chunk("reply")]);
    let mut client = serve(backend.clone(), InMemoryHistory::default()).await;

    decide(&mut client, "s1", "hello").await;
    decide(&mut client, "s2", "other").await;

    assert!(backend.calls()[1].history.is_empty());
}

// ---------------------------------------------------------------- AC3 failures

#[tokio::test]
async fn backend_failing_after_one_chunk_delivers_chunk_then_error_and_is_called_once() {
    let backend = ScriptedBackend::new(vec![chunk("partial"), fail()]);
    let mut client = serve(backend.clone(), InMemoryHistory::default()).await;

    let outcome = decide(&mut client, "s1", "hello").await;

    assert_eq!(outcome.events, vec![delta("partial")]);
    let code = outcome.code().expect("an error status ends the stream");
    assert_eq!(code, Code::Internal);
    assert_ne!(code, Code::Aborted, "must not look like busy");
    assert_ne!(code, Code::InvalidArgument, "must not look like bad input");
    assert_eq!(backend.calls().len(), 1, "no retry");
}

#[tokio::test]
async fn decide_after_a_failed_turn_sees_no_history() {
    let backend = ScriptedBackend::new(vec![chunk("partial"), fail()]);
    let mut client = serve(backend.clone(), InMemoryHistory::default()).await;

    decide(&mut client, "s1", "hello").await;
    decide(&mut client, "s1", "again").await;

    let calls = backend.calls();
    assert_eq!(calls.len(), 2);
    assert!(calls[1].history.is_empty(), "failed turn must not be kept");
}

#[tokio::test]
async fn backend_failing_before_any_chunk_yields_error_status_and_no_chunks() {
    let backend = ScriptedBackend::new(vec![fail()]);
    let mut client = serve(backend.clone(), InMemoryHistory::default()).await;

    let outcome = decide(&mut client, "s1", "hello").await;

    assert!(outcome.events.is_empty(), "{outcome:?}");
    assert!(outcome.status.is_some());
    assert_eq!(backend.calls().len(), 1);
}

// ---------------------------------------------------------------- busy, cancel

#[tokio::test]
async fn concurrent_decide_on_busy_session_is_aborted_while_first_completes() {
    let backend = GatedBackend::new();
    let gate = backend.gate.clone();
    let calls = backend.calls.clone();
    let mut client = serve(backend, InMemoryHistory::default()).await;

    // Hold the first turn in flight: once its first chunk is here, the session is reserved.
    let mut first = client
        .decide(request("s1", "hello"))
        .await
        .expect("decide")
        .into_inner();
    let first_chunk = timeout(LIMIT, first.next())
        .await
        .expect("first chunk timed out")
        .expect("stream ended early")
        .expect("status");
    assert_eq!(to_event(first_chunk), delta("a"));

    let second = decide(&mut client, "s1", "interrupting").await;
    assert_eq!(second.code(), Some(Code::Aborted), "{second:?}");
    assert!(second.events.is_empty());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "busy must not call the backend"
    );

    gate.add_permits(1);
    let rest = timeout(LIMIT, first.collect::<Vec<_>>())
        .await
        .expect("first turn did not finish");
    let rest: Vec<Event> = rest.into_iter().map(|r| to_event(r.unwrap())).collect();
    assert_eq!(rest, vec![delta("b"), Event::Complete]);
}

#[tokio::test]
async fn busy_session_does_not_block_a_different_session() {
    let backend = GatedBackend::new();
    let gate = backend.gate.clone();
    let mut client = serve(backend, InMemoryHistory::default()).await;

    let mut first = client
        .decide(request("s1", "hello"))
        .await
        .expect("decide")
        .into_inner();
    timeout(LIMIT, first.next())
        .await
        .expect("first chunk timed out")
        .expect("stream ended early")
        .expect("status");

    gate.add_permits(2);
    let other = decide(&mut client, "s2", "hello").await;

    assert!(other.status.is_none(), "{other:?}");
    assert_eq!(other.completes(), 1);
}

#[tokio::test]
async fn cancelled_client_stream_leaves_no_history_and_frees_the_session() {
    let backend = StallFirstCallBackend::default();
    let histories = backend.histories.clone();
    let mut client = serve(backend, InMemoryHistory::default()).await;

    // Start a turn that never finishes, see its first chunk, then walk away.
    let mut stalled = client
        .decide(request("s1", "hello"))
        .await
        .expect("decide")
        .into_inner();
    timeout(LIMIT, stalled.next())
        .await
        .expect("first chunk timed out")
        .expect("stream ended early")
        .expect("status");
    drop(stalled);

    // The server notices the cancel asynchronously; poll until the session is free again.
    let outcome = timeout(LIMIT, async {
        loop {
            let outcome = decide(&mut client, "s1", "again").await;
            if outcome.code() != Some(Code::Aborted) {
                return outcome;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("session was never freed after the client cancelled");

    assert!(outcome.status.is_none(), "{outcome:?}");
    assert_eq!(outcome.completes(), 1);
    let histories = histories.lock().unwrap();
    assert_eq!(histories.len(), 2);
    assert!(
        histories[1].is_empty(),
        "cancelled turn must leave no history"
    );
}

// ---------------------------------------------------------------- invalid input

#[tokio::test]
async fn empty_session_id_is_invalid_argument_without_calling_the_backend() {
    let backend = ScriptedBackend::new(vec![chunk("a")]);
    let mut client = serve(backend.clone(), InMemoryHistory::default()).await;

    let outcome = decide(&mut client, "", "hello").await;

    assert_eq!(outcome.code(), Some(Code::InvalidArgument), "{outcome:?}");
    assert!(outcome.events.is_empty());
    assert!(backend.calls().is_empty());
}

#[tokio::test]
async fn empty_text_is_invalid_argument_without_calling_the_backend() {
    let backend = ScriptedBackend::new(vec![chunk("a")]);
    let mut client = serve(backend.clone(), InMemoryHistory::default()).await;

    let outcome = decide(&mut client, "s1", "").await;

    assert_eq!(outcome.code(), Some(Code::InvalidArgument), "{outcome:?}");
    assert!(outcome.events.is_empty());
    assert!(backend.calls().is_empty());
}

// ---------------------------------------------------------------- telemetry

#[tokio::test]
async fn backend_failure_emits_an_error_level_log_record_carrying_the_session_id() {
    let logs = LogBuffer::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let backend = ScriptedBackend::new(vec![chunk("partial"), fail()]);
    let mut client = serve(backend, InMemoryHistory::default()).await;

    decide(&mut client, "session-under-test", "hello").await;

    // The record may be written just before or just after the client sees the error.
    timeout(LIMIT, async {
        loop {
            let found = logs
                .lines()
                .iter()
                .any(|l| l.contains("ERROR") && l.contains("session-under-test"));
            if found {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "no ERROR record carrying the session id; captured: {:?}",
            logs.lines()
        )
    });
}

// ---------------------------------------------------------------- store failure

#[tokio::test]
async fn failing_commit_delivers_chunks_then_unavailable_and_no_turn_complete() {
    let backend = ScriptedBackend::new(vec![chunk("one "), chunk("two")]);
    let mut client = serve(backend, FailingCommitStore::default()).await;

    let outcome = decide(&mut client, "s1", "hello").await;

    assert_eq!(outcome.events, vec![delta("one "), delta("two")]);
    assert_eq!(outcome.code(), Some(Code::Unavailable), "{outcome:?}");
}

// ---------------------------------------------------------------- span capture

#[derive(Debug, Default, Clone)]
struct SpanRecord {
    name: String,
    fields: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct EventRecord {
    parent_span: Option<String>,
    fields: HashMap<String, String>,
}

#[derive(Default)]
struct Captured {
    spans: HashMap<span::Id, SpanRecord>,
    events: Vec<EventRecord>,
}

/// Records spans (with the last value recorded per field) and events with their parent span.
#[derive(Clone, Default)]
struct SpanCapture {
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

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for SpanCapture {
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, _ctx: Context<'_, S>) {
        let mut record = SpanRecord {
            name: attrs.metadata().name().to_string(),
            fields: HashMap::new(),
        };
        attrs.record(&mut FieldMap(&mut record.fields));
        self.state.lock().unwrap().spans.insert(id.clone(), record);
        self.changed.notify_one();
    }

    fn on_record(&self, id: &span::Id, values: &span::Record<'_>, _ctx: Context<'_, S>) {
        if let Some(record) = self.state.lock().unwrap().spans.get_mut(id) {
            values.record(&mut FieldMap(&mut record.fields));
        }
        self.changed.notify_one();
    }

    fn on_event(&self, event: &TracingEvent<'_>, ctx: Context<'_, S>) {
        let parent_span = ctx.event_span(event).map(|s| s.name().to_string());
        let mut fields = HashMap::new();
        event.record(&mut FieldMap(&mut fields));
        self.state.lock().unwrap().events.push(EventRecord {
            parent_span,
            fields,
        });
        self.changed.notify_one();
    }
}

impl SpanCapture {
    fn install() -> (Self, tracing::subscriber::DefaultGuard) {
        let capture = Self::default();
        let subscriber = tracing_subscriber::registry().with(capture.clone());
        (capture, tracing::subscriber::set_default(subscriber))
    }

    fn decide_spans(&self) -> Vec<SpanRecord> {
        let state = self.state.lock().unwrap();
        state
            .spans
            .values()
            .filter(|s| s.name == "decide")
            .cloned()
            .collect()
    }

    fn decide_events(&self) -> Vec<EventRecord> {
        let state = self.state.lock().unwrap();
        state
            .events
            .iter()
            .filter(|e| e.parent_span.as_deref() == Some("decide"))
            .cloned()
            .collect()
    }

    /// Waits (woken by each recorded span/event change, bounded by `LIMIT`) until `ready` holds.
    async fn wait_until(&self, what: &str, ready: impl Fn(&Self) -> bool) {
        let waited = timeout(LIMIT, async {
            while !ready(self) {
                self.changed.notified().await;
            }
        })
        .await;
        assert!(waited.is_ok(), "timed out waiting for {what}");
    }
}

fn is_error_status(span: &SpanRecord) -> bool {
    span.fields.get("otel.status_code").map(String::as_str) == Some("ERROR")
}

fn has_outcome(capture: &SpanCapture, outcome: &str, session_id: &str) -> bool {
    capture.decide_events().iter().any(|e| {
        e.fields.get("outcome").map(String::as_str) == Some(outcome)
            && e.fields.get("session_id").map(String::as_str) == Some(session_id)
    })
}

#[tokio::test]
async fn backend_failure_marks_decide_span_error_with_backend_error_event() {
    let (capture, _guard) = SpanCapture::install();
    let backend = ScriptedBackend::new(vec![chunk("partial"), fail()]);
    let mut client = serve(backend, InMemoryHistory::default()).await;

    decide(&mut client, "span-session", "hello").await;

    capture
        .wait_until(
            "the decide span to be marked ERROR with a backend_error event",
            |c| {
                c.decide_spans().iter().any(is_error_status)
                    && has_outcome(c, "backend_error", "span-session")
            },
        )
        .await;
}

#[tokio::test]
async fn cancelled_stream_marks_decide_span_error_with_cancelled_event() {
    let (capture, _guard) = SpanCapture::install();
    let mut client = serve(StallFirstCallBackend::default(), InMemoryHistory::default()).await;

    let mut stalled = client
        .decide(request("cancel-session", "hello"))
        .await
        .expect("decide")
        .into_inner();
    timeout(LIMIT, stalled.next())
        .await
        .expect("first chunk timed out")
        .expect("stream ended early")
        .expect("status");
    drop(stalled);

    capture
        .wait_until(
            "the decide span to be marked ERROR with a cancelled event",
            |c| {
                c.decide_spans().iter().any(is_error_status)
                    && has_outcome(c, "cancelled", "cancel-session")
            },
        )
        .await;
}

#[tokio::test]
async fn successful_turn_has_a_decide_span_that_is_not_marked_error() {
    let (capture, _guard) = SpanCapture::install();
    let backend = ScriptedBackend::new(vec![chunk("one "), chunk("two")]);
    let mut client = serve(backend, InMemoryHistory::default()).await;

    let outcome = decide(&mut client, "ok-session", "hello").await;
    assert_eq!(outcome.completes(), 1, "{outcome:?}");

    let spans = capture.decide_spans();
    assert_eq!(spans.len(), 1, "expected one decide span, got {spans:?}");
    assert!(!is_error_status(&spans[0]), "{:?}", spans[0]);
    let outcome_events: Vec<_> = capture
        .decide_events()
        .into_iter()
        .filter(|e| e.fields.contains_key("outcome"))
        .collect();
    assert!(
        outcome_events.is_empty(),
        "a clean turn records no outcome: {outcome_events:?}"
    );
}

#[tokio::test]
async fn start_turn_store_failure_is_unavailable_and_marks_the_span_error() {
    let (capture, _guard) = SpanCapture::install();
    let backend = ScriptedBackend::new(vec![chunk("a")]);
    let mut client = serve(backend.clone(), FailingStartStore::default()).await;

    let outcome = decide(&mut client, "start-fail", "hello").await;

    assert_eq!(outcome.code(), Some(Code::Unavailable), "{outcome:?}");
    assert!(backend.calls().is_empty());
    capture
        .wait_until(
            "the decide span to be marked ERROR with a store_error event",
            |c| {
                c.decide_spans().iter().any(is_error_status)
                    && has_outcome(c, "store_error", "start-fail")
            },
        )
        .await;
}

#[tokio::test]
async fn busy_rejection_marks_the_span_error_with_store_error_outcome() {
    let (capture, _guard) = SpanCapture::install();
    let backend = GatedBackend::new();
    let gate = backend.gate.clone();
    let mut client = serve(backend, InMemoryHistory::default()).await;

    let mut first = client
        .decide(request("busy-span", "hello"))
        .await
        .expect("decide")
        .into_inner();
    timeout(LIMIT, first.next())
        .await
        .expect("first chunk timed out")
        .expect("stream ended early")
        .expect("status");

    let second = decide(&mut client, "busy-span", "interrupting").await;
    assert_eq!(second.code(), Some(Code::Aborted), "{second:?}");

    capture
        .wait_until(
            "the rejected decide span to be marked ERROR with a store_error event",
            |c| {
                c.decide_spans().iter().any(is_error_status)
                    && has_outcome(c, "store_error", "busy-span")
            },
        )
        .await;

    gate.add_permits(1);
    let rest = timeout(LIMIT, first.collect::<Vec<_>>())
        .await
        .expect("first turn did not finish");
    assert_eq!(rest.len(), 2, "the first turn still completes");
}

#[tokio::test]
async fn dropping_an_unpolled_response_stream_reports_cancelled() {
    let (capture, _guard) = SpanCapture::install();
    let service = Service::new(
        ScriptedBackend::new(vec![chunk("a")]),
        InMemoryHistory::default(),
    );

    // Called directly, not over the wire: tonic would poll the stream at once, and this contract
    // is about a stream that was never polled.
    let response = service
        .decide(tonic::Request::new(request("unpolled", "hello")))
        .await
        .expect("decide");
    drop(response);

    capture
        .wait_until(
            "the decide span to be marked ERROR with a cancelled event",
            |c| {
                c.decide_spans().iter().any(is_error_status)
                    && has_outcome(c, "cancelled", "unpolled")
            },
        )
        .await;
}

#[tokio::test]
async fn cancel_while_commit_is_in_flight_reports_commit_interrupted() {
    let (capture, _guard) = SpanCapture::install();
    let backend = ScriptedBackend::new(vec![chunk("one "), chunk("two")]);
    let mut client = serve(backend, StalledCommitStore::default()).await;

    let mut stream = client
        .decide(request("mid-commit", "hello"))
        .await
        .expect("decide")
        .into_inner();
    for _ in 0..2 {
        timeout(LIMIT, stream.next())
            .await
            .expect("chunk timed out")
            .expect("stream ended early")
            .expect("status");
    }
    drop(stream);

    capture
        .wait_until(
            "the decide span to be marked ERROR with a commit_interrupted event",
            |c| {
                c.decide_spans().iter().any(is_error_status)
                    && has_outcome(c, "commit_interrupted", "mid-commit")
            },
        )
        .await;
}

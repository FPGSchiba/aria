//! `Decide` over a real in-process gRPC server and client (tonic over loopback TCP).
//! The handler is never called directly: every assertion is about what the caller receives.

use agent_core::backend::scripted::{ScriptStep, ScriptedBackend};
use agent_core::backend::{Backend, BackendError, Chunk};
use agent_core::history::{
    Actor, Content, ConversationPart, History, HistoryError, HistoryResult, HistoryStore, Token,
};
use agent_core::service::Service;
use futures_core::Stream;
use proto::agent_core::v1::agent_core_client::AgentCoreClient;
use proto::agent_core::v1::agent_core_server::AgentCoreServer;
use proto::agent_core::v1::{DecideRequest, decide_response};
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Server};
use tonic::{Code, Status};

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
    inner: History,
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
    let mut client = serve(backend, History::default()).await;

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
    let mut client = serve(backend, History::default()).await;

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
    let mut client = serve(backend.clone(), History::default()).await;

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
    let mut client = serve(backend.clone(), History::default()).await;

    decide(&mut client, "s1", "hello").await;
    decide(&mut client, "s2", "other").await;

    assert!(backend.calls()[1].history.is_empty());
}

// ---------------------------------------------------------------- AC3 failures

#[tokio::test]
async fn backend_failing_after_one_chunk_delivers_chunk_then_error_and_is_called_once() {
    let backend = ScriptedBackend::new(vec![chunk("partial"), fail()]);
    let mut client = serve(backend.clone(), History::default()).await;

    let outcome = decide(&mut client, "s1", "hello").await;

    assert_eq!(outcome.events, vec![delta("partial")]);
    let code = outcome.code().expect("an error status ends the stream");
    assert_ne!(code, Code::Aborted, "must not look like busy");
    assert_ne!(code, Code::InvalidArgument, "must not look like bad input");
    assert_eq!(backend.calls().len(), 1, "no retry");
}

#[tokio::test]
async fn decide_after_a_failed_turn_sees_no_history() {
    let backend = ScriptedBackend::new(vec![chunk("partial"), fail()]);
    let mut client = serve(backend.clone(), History::default()).await;

    decide(&mut client, "s1", "hello").await;
    decide(&mut client, "s1", "again").await;

    let calls = backend.calls();
    assert_eq!(calls.len(), 2);
    assert!(calls[1].history.is_empty(), "failed turn must not be kept");
}

#[tokio::test]
async fn backend_failing_before_any_chunk_yields_error_status_and_no_chunks() {
    let backend = ScriptedBackend::new(vec![fail()]);
    let mut client = serve(backend.clone(), History::default()).await;

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
    let mut client = serve(backend, History::default()).await;

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
    let mut client = serve(backend, History::default()).await;

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
    let mut client = serve(backend, History::default()).await;

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
    let mut client = serve(backend.clone(), History::default()).await;

    let outcome = decide(&mut client, "", "hello").await;

    assert_eq!(outcome.code(), Some(Code::InvalidArgument), "{outcome:?}");
    assert!(outcome.events.is_empty());
    assert!(backend.calls().is_empty());
}

#[tokio::test]
async fn empty_text_is_invalid_argument_without_calling_the_backend() {
    let backend = ScriptedBackend::new(vec![chunk("a")]);
    let mut client = serve(backend.clone(), History::default()).await;

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
    let mut client = serve(backend, History::default()).await;

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

//! gRPC health over a real in-process server: the statuses follow the backend and the store, and
//! shutdown reports not-serving before the drain, while a turn is still in flight.

mod common;

use agent_core::backend::scripted::{ScriptStep, ScriptedBackend};
use agent_core::backend::{Backend, BackendError, Chunk};
use agent_core::conversation::ConversationPart;
use agent_core::health::{
    self, BACKEND_SERVICE_NAME, BackendHealth, HISTORY_SERVICE_NAME, HistoryHealth, HostStatus,
    UnavailabilityReason,
};
use agent_core::history::{HistoryResult, HistoryStore, InMemoryHistory, InMemoryTurn};
use agent_core::service::Service;
use futures_core::Stream;
use proto::agent_core::v1::agent_core_client::AgentCoreClient;
use proto::agent_core::v1::agent_core_server::{AgentCoreServer, SERVICE_NAME};
use proto::agent_core::v1::{DecideRequest, decide_response};
use proto::grpc::health::v1::health_check_response::ServingStatus;
use proto::grpc::health::v1::health_client::HealthClient;
use proto::grpc::health::v1::{HealthCheckRequest, HealthCheckResponse, HealthListRequest};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{Semaphore, oneshot, watch};
use tokio::time::timeout;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::Streaming;
use tonic::transport::{Channel, Server};
use tracing::span;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, SubscriberExt};

/// Upper bound for anything that could hang, so a failure reports instead of blocking.
const LIMIT: Duration = Duration::from_secs(10);

type BackendStream = Pin<Box<dyn Stream<Item = Result<Chunk, BackendError>> + Send + 'static>>;

// ---------------------------------------------------------------- doubles

/// An in-memory store whose health a test can set.
#[derive(Clone)]
struct ToggleStore {
    inner: InMemoryHistory,
    health: Arc<watch::Sender<HistoryHealth>>,
}

impl ToggleStore {
    fn new() -> Self {
        Self {
            inner: InMemoryHistory::default(),
            health: Arc::new(watch::channel(HistoryHealth::new("toggle", true)).0),
        }
    }

    fn set_healthy(&self, healthy: bool) {
        self.health
            .send_replace(HistoryHealth::new("toggle", healthy));
    }
}

impl HistoryStore for ToggleStore {
    type Turn = InMemoryTurn;

    fn subscribe_health(&self) -> watch::Receiver<HistoryHealth> {
        self.health.subscribe()
    }

    async fn start_turn(
        &self,
        session_id: &str,
    ) -> HistoryResult<(InMemoryTurn, Vec<ConversationPart>)> {
        self.inner.start_turn(session_id).await
    }

    async fn commit(
        &self,
        turn: InMemoryTurn,
        new_parts: Vec<ConversationPart>,
    ) -> HistoryResult<()> {
        self.inner.commit(turn, new_parts).await
    }

    async fn abort(&self, turn: InMemoryTurn) -> HistoryResult<()> {
        self.inner.abort(turn).await
    }
}

/// Emits "a", waits for a permit on the gate, then emits "b" and ends cleanly.
#[derive(Clone)]
struct GatedBackend {
    gate: Arc<Semaphore>,
}

impl Backend for GatedBackend {
    fn name(&self) -> &'static str {
        "gated"
    }

    fn subscribe_health(&self) -> watch::Receiver<BackendHealth> {
        watch::channel(BackendHealth::new(self.name(), true, false)).1
    }

    fn stream_conversation(
        &self,
        _history: Vec<ConversationPart>,
        _input: Vec<ConversationPart>,
    ) -> BackendStream {
        let gate = self.gate.clone();
        let first = tokio_stream::once::<Result<Chunk, BackendError>>(Ok(Chunk::Text {
            text: "a".to_string(),
        }));
        let second = tokio_stream::once(()).then(move |()| {
            let gate = gate.clone();
            async move {
                gate.acquire().await.expect("gate open").forget();
                Ok(Chunk::Text {
                    text: "b".to_string(),
                })
            }
        });
        Box::pin(first.chain(second))
    }
}

/// Counts spans named `decide` and events at info or above, to show health checks create none.
/// Installed with `set_default`, which is per thread: the tests using it run on tokio's default
/// current-thread runtime, so the server tasks run on the same thread and are seen. Each test calls
/// `common::keep_tracing_interest_global` first, or a parallel test can hide events from it.
#[derive(Clone, Default)]
struct Counts {
    decide_spans: Arc<AtomicUsize>,
    info_events: Arc<AtomicUsize>,
}

impl<S: tracing::Subscriber> Layer<S> for Counts {
    fn on_new_span(&self, attrs: &span::Attributes<'_>, _id: &span::Id, _ctx: Context<'_, S>) {
        if attrs.metadata().name() == "decide" {
            self.decide_spans.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        if *event.metadata().level() <= tracing::Level::INFO {
            self.info_events.fetch_add(1, Ordering::SeqCst);
        }
    }
}

// ---------------------------------------------------------------- harness

struct Harness {
    channel: Channel,
    shutdown: oneshot::Sender<()>,
    /// Finishes when the server has drained and stopped.
    server: tokio::task::JoinHandle<()>,
}

/// Serves health and the Agent Core like the binary does: the shutdown future reports
/// not-serving first, and only then lets the server drain.
async fn serve<B, H>(backend: B, store: H) -> Harness
where
    B: Backend + 'static,
    H: HistoryStore + Clone + 'static,
{
    let (health, health_server) =
        health::start(backend.subscribe_health(), store.subscribe_health()).await;
    let service = Service::new(backend, store);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let (shutdown, shutdown_rx) = oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        Server::builder()
            .add_service(health_server)
            .add_service(AgentCoreServer::new(service))
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
                let _ = shutdown_rx.await;
                health.begin_shutdown().await;
            })
            .await
            .expect("server");
    });
    let channel = timeout(
        LIMIT,
        Channel::from_shared(format!("http://{addr}"))
            .expect("uri")
            .connect(),
    )
    .await
    .expect("connect timed out")
    .expect("connect");
    Harness {
        channel,
        shutdown,
        server,
    }
}

fn request(service: &str) -> HealthCheckRequest {
    HealthCheckRequest {
        service: service.to_string(),
    }
}

async fn check(client: &mut HealthClient<Channel>, service: &str) -> ServingStatus {
    let response = timeout(LIMIT, client.check(request(service)))
        .await
        .expect("check timed out")
        .expect("check");
    response.into_inner().status()
}

/// Opens a watch stream and returns it with the status it starts with.
async fn watch_status(
    client: &mut HealthClient<Channel>,
    service: &str,
) -> (Streaming<HealthCheckResponse>, ServingStatus) {
    let mut stream = client
        .watch(request(service))
        .await
        .expect("watch")
        .into_inner();
    let first = next_status(&mut stream).await;
    (stream, first)
}

/// The next status the stream reports; fails if none arrives in time.
async fn next_status(stream: &mut Streaming<HealthCheckResponse>) -> ServingStatus {
    timeout(LIMIT, stream.next())
        .await
        .expect("no status change in time")
        .expect("stream ended")
        .expect("status")
        .status()
}

/// Asserts the stream reports nothing for a short while.
async fn assert_no_change(stream: &mut Streaming<HealthCheckResponse>) {
    let waited = timeout(Duration::from_millis(150), stream.next()).await;
    assert!(waited.is_err(), "unexpected status change: {waited:?}");
}

// ---------------------------------------------------------------- tests

#[tokio::test]
async fn started_server_reports_serving_overall_and_for_agent_core() {
    let harness = serve(ScriptedBackend::new(vec![]), ToggleStore::new()).await;
    let mut health = HealthClient::new(harness.channel.clone());

    assert_eq!(check(&mut health, "").await, ServingStatus::Serving);
    assert_eq!(
        check(&mut health, SERVICE_NAME).await,
        ServingStatus::Serving
    );
}

#[tokio::test]
async fn health_checks_create_no_decide_spans_and_no_info_log_lines() {
    common::keep_tracing_interest_global();
    let counts = Counts::default();
    let _guard =
        tracing::subscriber::set_default(tracing_subscriber::registry().with(counts.clone()));
    let backend = ScriptedBackend::new(vec![ScriptStep::Chunk(Chunk::Text {
        text: "hi".to_string(),
    })]);
    let harness = serve(backend, ToggleStore::new()).await;
    let mut health = HealthClient::new(harness.channel.clone());
    let mut agent_core = AgentCoreClient::new(harness.channel.clone());

    // A Decide first, so the counters are shown to count.
    let turn = agent_core
        .decide(DecideRequest {
            session_id: "counted".to_string(),
            speaker_name: None,
            text: "hello".to_string(),
        })
        .await
        .expect("decide")
        .into_inner();
    timeout(LIMIT, turn.collect::<Vec<_>>())
        .await
        .expect("turn timed out");
    let spans_after_decide = counts.decide_spans.load(Ordering::SeqCst);
    let events_after_decide = counts.info_events.load(Ordering::SeqCst);
    assert!(spans_after_decide >= 1, "the counter must count a Decide");
    assert!(
        events_after_decide >= 1,
        "the counter must count info events"
    );

    for _ in 0..3 {
        check(&mut health, "").await;
        check(&mut health, SERVICE_NAME).await;
    }

    assert_eq!(
        counts.decide_spans.load(Ordering::SeqCst),
        spans_after_decide
    );
    assert_eq!(
        counts.info_events.load(Ordering::SeqCst),
        events_after_decide
    );
}

#[tokio::test]
async fn unhealthy_backend_makes_agent_core_not_serving_and_recovery_restores_it() {
    let backend = ScriptedBackend::new(vec![]);
    let harness = serve(backend.clone(), ToggleStore::new()).await;
    let mut health = HealthClient::new(harness.channel.clone());
    let (mut agent_core, first) = watch_status(&mut health, SERVICE_NAME).await;
    assert_eq!(first, ServingStatus::Serving);

    backend.set_healthy(false);
    assert_eq!(
        next_status(&mut agent_core).await,
        ServingStatus::NotServing
    );
    assert_eq!(check(&mut health, "").await, ServingStatus::Serving);

    backend.set_healthy(true);
    assert_eq!(next_status(&mut agent_core).await, ServingStatus::Serving);
}

#[tokio::test]
async fn unhealthy_store_makes_agent_core_not_serving_and_recovery_restores_it() {
    let store = ToggleStore::new();
    let harness = serve(ScriptedBackend::new(vec![]), store.clone()).await;
    let mut health = HealthClient::new(harness.channel.clone());
    let (mut agent_core, first) = watch_status(&mut health, SERVICE_NAME).await;
    assert_eq!(first, ServingStatus::Serving);

    store.set_healthy(false);
    assert_eq!(
        next_status(&mut agent_core).await,
        ServingStatus::NotServing
    );
    assert_eq!(check(&mut health, "").await, ServingStatus::Serving);

    store.set_healthy(true);
    assert_eq!(next_status(&mut agent_core).await, ServingStatus::Serving);
}

#[tokio::test]
async fn with_both_unhealthy_fixing_one_is_not_enough() {
    let backend = ScriptedBackend::new(vec![]);
    let store = ToggleStore::new();
    let harness = serve(backend.clone(), store.clone()).await;
    let mut health = HealthClient::new(harness.channel.clone());
    let (mut agent_core, _) = watch_status(&mut health, SERVICE_NAME).await;

    backend.set_healthy(false);
    store.set_healthy(false);
    assert_eq!(
        next_status(&mut agent_core).await,
        ServingStatus::NotServing
    );

    backend.set_healthy(true);
    assert_no_change(&mut agent_core).await;
    assert_eq!(
        check(&mut health, SERVICE_NAME).await,
        ServingStatus::NotServing
    );

    store.set_healthy(true);
    assert_eq!(next_status(&mut agent_core).await, ServingStatus::Serving);
}

#[tokio::test]
async fn shutdown_reports_not_serving_while_an_in_flight_turn_is_still_draining() {
    let gate = Arc::new(Semaphore::new(0));
    let backend = GatedBackend { gate: gate.clone() };
    let harness = serve(backend, ToggleStore::new()).await;
    let mut health = HealthClient::new(harness.channel.clone());
    let mut agent_core_client = AgentCoreClient::new(harness.channel.clone());

    let (mut overall, first) = watch_status(&mut health, "").await;
    assert_eq!(first, ServingStatus::Serving);
    let (mut agent_core, first) = watch_status(&mut health, SERVICE_NAME).await;
    assert_eq!(first, ServingStatus::Serving);

    // A turn in flight: its first chunk is here, the rest waits on the gate.
    let mut turn = agent_core_client
        .decide(DecideRequest {
            session_id: "drain".to_string(),
            speaker_name: None,
            text: "hello".to_string(),
        })
        .await
        .expect("decide")
        .into_inner();
    let first_chunk = timeout(LIMIT, turn.next())
        .await
        .expect("first chunk timed out")
        .expect("stream ended")
        .expect("status");
    assert!(matches!(
        first_chunk.payload,
        Some(decide_response::Payload::TextDelta(_))
    ));

    harness.shutdown.send(()).expect("server is running");

    assert_eq!(
        next_status(&mut agent_core).await,
        ServingStatus::NotServing
    );
    assert_eq!(next_status(&mut overall).await, ServingStatus::NotServing);

    // The turn was still in flight the whole time, and still completes.
    gate.add_permits(1);
    let rest = timeout(LIMIT, turn.collect::<Vec<_>>())
        .await
        .expect("turn did not finish");
    let payloads: Vec<_> = rest
        .into_iter()
        .map(|r| r.expect("status").payload.expect("payload"))
        .collect();
    assert!(matches!(
        payloads.as_slice(),
        [
            decide_response::Payload::TextDelta(text),
            decide_response::Payload::TurnComplete(_)
        ] if text == "b"
    ));
}

#[tokio::test]
async fn shutdown_ends_open_watch_streams_so_the_drain_is_not_held() {
    let harness = serve(ScriptedBackend::new(vec![]), ToggleStore::new()).await;
    let mut health = HealthClient::new(harness.channel.clone());
    let (mut overall, first) = watch_status(&mut health, "").await;
    assert_eq!(first, ServingStatus::Serving);
    let (mut agent_core, first) = watch_status(&mut health, SERVICE_NAME).await;
    assert_eq!(first, ServingStatus::Serving);

    harness.shutdown.send(()).expect("server is running");

    // Each watcher sees not-serving first, and then its stream ends.
    for stream in [&mut overall, &mut agent_core] {
        assert_eq!(next_status(stream).await, ServingStatus::NotServing);
        let end = timeout(LIMIT, stream.next())
            .await
            .expect("stream did not end");
        assert!(
            end.is_none(),
            "the stream must end after not-serving: {end:?}"
        );
    }
    // No turn is in flight, so the server finishes well inside any drain bound.
    timeout(Duration::from_secs(2), harness.server)
        .await
        .expect("the server was held open by a watch stream")
        .expect("server task");
}

#[tokio::test]
async fn started_server_reports_serving_for_history_and_unknown_for_a_provider_not_yet_observed() {
    let harness = serve(ScriptedBackend::new(vec![]), ToggleStore::new()).await;
    let mut health = HealthClient::new(harness.channel.clone());

    assert_eq!(
        check(&mut health, HISTORY_SERVICE_NAME).await,
        ServingStatus::Serving
    );
    assert_eq!(
        check(&mut health, BACKEND_SERVICE_NAME).await,
        ServingStatus::Unknown
    );
}

#[tokio::test]
async fn an_unavailable_provider_shows_in_the_backend_status_and_not_the_stores() {
    let backend = ScriptedBackend::new(vec![]);
    let harness = serve(backend.clone(), ToggleStore::new()).await;
    let mut health = HealthClient::new(harness.channel.clone());
    let (mut backend_status, _) = watch_status(&mut health, BACKEND_SERVICE_NAME).await;

    backend.set_host_status(HostStatus::Unavailable(UnavailabilityReason::TimedOut));

    assert_eq!(
        next_status(&mut backend_status).await,
        ServingStatus::NotServing
    );
    assert_eq!(
        check(&mut health, HISTORY_SERVICE_NAME).await,
        ServingStatus::Serving
    );
}

#[tokio::test]
async fn unhealthy_store_shows_in_its_own_status_and_not_the_backends() {
    let store = ToggleStore::new();
    let harness = serve(ScriptedBackend::new(vec![]), store.clone()).await;
    let mut health = HealthClient::new(harness.channel.clone());
    let (mut history_status, _) = watch_status(&mut health, HISTORY_SERVICE_NAME).await;

    store.set_healthy(false);

    assert_eq!(
        next_status(&mut history_status).await,
        ServingStatus::NotServing
    );
    assert_eq!(
        check(&mut health, BACKEND_SERVICE_NAME).await,
        ServingStatus::Unknown
    );
    assert_eq!(
        check(&mut health, SERVICE_NAME).await,
        ServingStatus::NotServing
    );
}

#[tokio::test]
async fn shutdown_sets_all_four_statuses_to_not_serving() {
    let harness = serve(ScriptedBackend::new(vec![]), ToggleStore::new()).await;
    let mut health = HealthClient::new(harness.channel.clone());
    let names = ["", SERVICE_NAME, HISTORY_SERVICE_NAME, BACKEND_SERVICE_NAME];
    let mut streams = Vec::new();
    for name in names {
        let (stream, first) = watch_status(&mut health, name).await;
        // The provider has not been observed yet, so its entry starts unknown.
        let expected = if name == BACKEND_SERVICE_NAME {
            ServingStatus::Unknown
        } else {
            ServingStatus::Serving
        };
        assert_eq!(first, expected, "{name:?}");
        streams.push(stream);
    }

    harness.shutdown.send(()).expect("server is running");

    for (name, stream) in names.iter().zip(streams.iter_mut()) {
        assert_eq!(
            next_status(stream).await,
            ServingStatus::NotServing,
            "{name:?}"
        );
        let end = timeout(LIMIT, stream.next())
            .await
            .expect("stream did not end");
        assert!(end.is_none(), "{name:?}: {end:?}");
    }
}

/// Calls `List` and returns each name with its status.
async fn list(
    client: &mut HealthClient<Channel>,
) -> std::collections::BTreeMap<String, ServingStatus> {
    let response = timeout(LIMIT, client.list(HealthListRequest {}))
        .await
        .expect("list timed out")
        .expect("list");
    response
        .into_inner()
        .statuses
        .into_iter()
        .map(|(name, response)| (name, response.status()))
        .collect()
}

#[tokio::test]
async fn list_returns_all_four_statuses() {
    let harness = serve(ScriptedBackend::new(vec![]), ToggleStore::new()).await;
    let mut health = HealthClient::new(harness.channel.clone());

    let statuses = list(&mut health).await;

    let expected: std::collections::BTreeMap<String, ServingStatus> = [
        ("", ServingStatus::Serving),
        (SERVICE_NAME, ServingStatus::Serving),
        (HISTORY_SERVICE_NAME, ServingStatus::Serving),
        (BACKEND_SERVICE_NAME, ServingStatus::Unknown),
    ]
    .into_iter()
    .map(|(name, status)| (name.to_string(), status))
    .collect();
    assert_eq!(statuses, expected);
}

#[tokio::test]
async fn list_reflects_a_toggled_dependency() {
    let backend = ScriptedBackend::new(vec![]);
    let store = ToggleStore::new();
    let harness = serve(backend.clone(), store.clone()).await;
    let mut health = HealthClient::new(harness.channel.clone());
    let (mut backend_status, _) = watch_status(&mut health, BACKEND_SERVICE_NAME).await;
    let (mut history_status, _) = watch_status(&mut health, HISTORY_SERVICE_NAME).await;

    backend.set_host_status(HostStatus::Unavailable(UnavailabilityReason::Unreachable));
    store.set_healthy(false);
    assert_eq!(
        next_status(&mut backend_status).await,
        ServingStatus::NotServing
    );
    assert_eq!(
        next_status(&mut history_status).await,
        ServingStatus::NotServing
    );

    let statuses = list(&mut health).await;
    assert_eq!(statuses[""], ServingStatus::Serving);
    assert_eq!(statuses[SERVICE_NAME], ServingStatus::NotServing);
    assert_eq!(statuses[HISTORY_SERVICE_NAME], ServingStatus::NotServing);
    assert_eq!(statuses[BACKEND_SERVICE_NAME], ServingStatus::NotServing);
}

#[tokio::test]
async fn list_after_shutdown_reads_all_not_serving() {
    // The server is not told to stop here, so List can still be called once shutdown is reported.
    let backend = ScriptedBackend::new(vec![]);
    let store = ToggleStore::new();
    let (monitor, health_server) =
        health::start(backend.subscribe_health(), store.subscribe_health()).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        Server::builder()
            .add_service(health_server)
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .expect("server");
    });
    let channel = Channel::from_shared(format!("http://{addr}"))
        .expect("uri")
        .connect()
        .await
        .expect("connect");
    let mut health = HealthClient::new(channel);
    let before = list(&mut health).await;
    assert_eq!(before.len(), 4);
    assert!(
        before
            .iter()
            .all(|(name, s)| *s == ServingStatus::Serving || name == BACKEND_SERVICE_NAME),
        "{before:?}"
    );

    timeout(LIMIT, monitor.begin_shutdown())
        .await
        .expect("begin_shutdown timed out");

    let statuses = list(&mut health).await;
    assert_eq!(statuses.len(), 4);
    assert!(
        statuses.values().all(|s| *s == ServingStatus::NotServing),
        "{statuses:?}"
    );
}

#[tokio::test]
async fn unknown_service_is_not_found_on_check_and_service_unknown_on_watch() {
    let harness = serve(ScriptedBackend::new(vec![]), ToggleStore::new()).await;
    let mut health = HealthClient::new(harness.channel.clone());

    let status = health
        .check(request("nope"))
        .await
        .expect_err("unknown service");
    assert_eq!(status.code(), tonic::Code::NotFound);
    let (_, first) = watch_status(&mut health, "nope").await;
    assert_eq!(first, ServingStatus::ServiceUnknown);
}

// ---------------------------------------------------------------- provider availability (C5)

/// Polls `Check` until `service` reports `expected`; fails after `LIMIT` with the last value.
async fn eventually(client: &mut HealthClient<Channel>, service: &str, expected: ServingStatus) {
    let waited = timeout(LIMIT, async {
        loop {
            if check(client, service).await == expected {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(
        waited.is_ok(),
        "{service:?} never became {expected:?}; it is {:?}",
        check(client, service).await
    );
}

fn timed_out() -> HostStatus {
    HostStatus::Unavailable(UnavailabilityReason::TimedOut)
}

#[tokio::test]
async fn a_provider_not_yet_observed_shows_unknown_while_the_server_is_ready() {
    let harness = serve(ScriptedBackend::new(vec![]), ToggleStore::new()).await;
    let mut health = HealthClient::new(harness.channel.clone());

    assert_eq!(
        check(&mut health, BACKEND_SERVICE_NAME).await,
        ServingStatus::Unknown
    );
    assert_eq!(
        check(&mut health, SERVICE_NAME).await,
        ServingStatus::Serving
    );
    assert_eq!(check(&mut health, "").await, ServingStatus::Serving);
}

#[tokio::test]
async fn an_available_provider_shows_serving_on_the_backend_entry() {
    let backend = ScriptedBackend::new(vec![]);
    let harness = serve(backend.clone(), ToggleStore::new()).await;
    let mut health = HealthClient::new(harness.channel.clone());
    let (mut backend_status, first) = watch_status(&mut health, BACKEND_SERVICE_NAME).await;
    assert_eq!(first, ServingStatus::Unknown);

    backend.set_host_status(HostStatus::Available);

    assert_eq!(
        next_status(&mut backend_status).await,
        ServingStatus::Serving
    );
}

#[tokio::test]
async fn every_kind_of_unavailability_shows_not_serving_while_readiness_stays_serving() {
    let reasons = [
        UnavailabilityReason::KeyRejected {
            host_code: Some("wrong_api_key".to_string()),
        },
        UnavailabilityReason::Unreachable,
        UnavailabilityReason::TimedOut,
        UnavailabilityReason::RateLimited { host_code: None },
        UnavailabilityReason::PaymentRequired { host_code: None },
        UnavailabilityReason::ServerError {
            http_code: 503,
            host_code: None,
        },
        UnavailabilityReason::StreamInterrupted { host_code: None },
    ];
    for reason in reasons {
        let backend = ScriptedBackend::new(vec![]);
        let harness = serve(backend.clone(), ToggleStore::new()).await;
        let mut health = HealthClient::new(harness.channel.clone());

        backend.set_host_status(HostStatus::Unavailable(reason.clone()));

        eventually(&mut health, BACKEND_SERVICE_NAME, ServingStatus::NotServing).await;
        assert_eq!(
            check(&mut health, SERVICE_NAME).await,
            ServingStatus::Serving,
            "{reason:?} must not make the server unready"
        );
        assert_eq!(check(&mut health, "").await, ServingStatus::Serving);
    }
}

#[tokio::test]
async fn a_recovered_provider_returns_the_backend_entry_to_serving() {
    let backend = ScriptedBackend::new(vec![]);
    let harness = serve(backend.clone(), ToggleStore::new()).await;
    let mut health = HealthClient::new(harness.channel.clone());
    backend.set_host_status(timed_out());
    eventually(&mut health, BACKEND_SERVICE_NAME, ServingStatus::NotServing).await;

    backend.set_host_status(HostStatus::Available);

    eventually(&mut health, BACKEND_SERVICE_NAME, ServingStatus::Serving).await;
}

#[tokio::test]
async fn a_host_status_back_to_unknown_shows_unknown_again() {
    let backend = ScriptedBackend::new(vec![]);
    let harness = serve(backend.clone(), ToggleStore::new()).await;
    let mut health = HealthClient::new(harness.channel.clone());
    backend.set_host_status(HostStatus::Available);
    eventually(&mut health, BACKEND_SERVICE_NAME, ServingStatus::Serving).await;

    backend.set_host_status(HostStatus::Unknown);

    eventually(&mut health, BACKEND_SERVICE_NAME, ServingStatus::Unknown).await;
}

#[tokio::test]
async fn readiness_follows_the_configured_flag_whatever_availability_says() {
    let backend = ScriptedBackend::new(vec![]);
    let harness = serve(backend.clone(), ToggleStore::new()).await;
    let mut health = HealthClient::new(harness.channel.clone());
    backend.set_host_status(HostStatus::Available);
    eventually(&mut health, BACKEND_SERVICE_NAME, ServingStatus::Serving).await;

    backend.set_healthy(false);

    eventually(&mut health, SERVICE_NAME, ServingStatus::NotServing).await;
    assert_eq!(
        check(&mut health, BACKEND_SERVICE_NAME).await,
        ServingStatus::Serving,
        "the diagnostic entry reports availability, not the configured flag"
    );
}

#[tokio::test]
async fn shutdown_makes_the_backend_entry_not_serving_whatever_availability_was() {
    // Each availability with the backend entry it shows before shutdown. The timed-out entry is
    // already not-serving, so for it the readiness and overall entries are what prove shutdown had
    // an effect: they are serving before and not-serving after.
    let cases = [
        (HostStatus::Unknown, ServingStatus::Unknown),
        (HostStatus::Available, ServingStatus::Serving),
        (timed_out(), ServingStatus::NotServing),
    ];
    for (status, before) in cases {
        // The server is not told to stop here, so the entry can be read once shutdown is reported;
        // a status that is already not-serving gets no further watch item.
        let backend = ScriptedBackend::new(vec![]);
        let store = ToggleStore::new();
        let (monitor, health_server) =
            health::start(backend.subscribe_health(), store.subscribe_health()).await;
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            Server::builder()
                .add_service(health_server)
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .expect("server");
        });
        let channel = Channel::from_shared(format!("http://{addr}"))
            .expect("uri")
            .connect()
            .await
            .expect("connect");
        let mut health = HealthClient::new(channel);
        backend.set_host_status(status.clone());
        eventually(&mut health, BACKEND_SERVICE_NAME, before).await;
        assert_eq!(
            check(&mut health, SERVICE_NAME).await,
            ServingStatus::Serving
        );
        assert_eq!(check(&mut health, "").await, ServingStatus::Serving);

        timeout(LIMIT, monitor.begin_shutdown())
            .await
            .expect("begin_shutdown timed out");

        for name in [BACKEND_SERVICE_NAME, SERVICE_NAME, ""] {
            assert_eq!(
                check(&mut health, name).await,
                ServingStatus::NotServing,
                "{name:?} after shutdown, from {status:?}"
            );
        }
    }
}

#[tokio::test]
async fn each_availability_transition_is_logged_once_with_its_kind() {
    let (logs, _guard) = common::capture_logs();
    let backend = ScriptedBackend::new(vec![]);
    let harness = serve(backend.clone(), ToggleStore::new()).await;
    let mut health = HealthClient::new(harness.channel.clone());
    let rejected = HostStatus::Unavailable(UnavailabilityReason::KeyRejected {
        host_code: Some("wrong_api_key".to_string()),
    });

    backend.set_host_status(rejected.clone());
    eventually(&mut health, BACKEND_SERVICE_NAME, ServingStatus::NotServing).await;
    timeout(LIMIT, async {
        while logs.count_mentioning("keyrejected") == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the transition was never logged: {:?}", logs.lines()));

    // The same state reported again is not a transition.
    backend.set_host_status(rejected);
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert_eq!(
        logs.count_mentioning("keyrejected"),
        1,
        "{:#?}",
        logs.lines()
    );
}

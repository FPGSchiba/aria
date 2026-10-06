//! gRPC health over a real in-process server: the statuses follow the backend and the store, and
//! shutdown reports not-serving before the drain, while a turn is still in flight.

use agent_core::backend::scripted::ScriptedBackend;
use agent_core::backend::{Backend, BackendError, Chunk};
use agent_core::conversation::ConversationPart;
use agent_core::health::{self, BackendHealth, HistoryHealth};
use agent_core::history::{HistoryResult, HistoryStore, InMemoryHistory, InMemoryTurn};
use agent_core::service::Service;
use futures_core::Stream;
use proto::agent_core::v1::agent_core_client::AgentCoreClient;
use proto::agent_core::v1::agent_core_server::{AgentCoreServer, SERVICE_NAME};
use proto::agent_core::v1::{DecideRequest, decide_response};
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
use tonic_health::pb::health_check_response::ServingStatus;
use tonic_health::pb::health_client::HealthClient;
use tonic_health::pb::{HealthCheckRequest, HealthCheckResponse};
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
        watch::channel(BackendHealth::new("gated", true, false)).1
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

/// Counts spans named `decide`, to show health checks create none.
#[derive(Clone, Default)]
struct DecideSpanCount(Arc<AtomicUsize>);

impl<S: tracing::Subscriber> Layer<S> for DecideSpanCount {
    fn on_new_span(&self, attrs: &span::Attributes<'_>, _id: &span::Id, _ctx: Context<'_, S>) {
        if attrs.metadata().name() == "decide" {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

// ---------------------------------------------------------------- harness

struct Harness {
    channel: Channel,
    shutdown: oneshot::Sender<()>,
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
    tokio::spawn(async move {
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
    Harness { channel, shutdown }
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
async fn started_server_reports_serving_overall_and_for_agent_core_without_decide_spans() {
    let count = DecideSpanCount::default();
    let _guard =
        tracing::subscriber::set_default(tracing_subscriber::registry().with(count.clone()));
    let harness = serve(ScriptedBackend::new(vec![]), ToggleStore::new()).await;
    let mut health = HealthClient::new(harness.channel.clone());

    assert_eq!(check(&mut health, "").await, ServingStatus::Serving);
    assert_eq!(
        check(&mut health, SERVICE_NAME).await,
        ServingStatus::Serving
    );
    assert_eq!(
        count.0.load(Ordering::SeqCst),
        0,
        "health checks must not create decide spans"
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

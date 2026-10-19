//! Provider availability end to end: the OpenAI-compatible backend serves `Decide` turns against a
//! fake provider, and the health service shows what the backend observed, while readiness stays
//! tied to the configured state.

mod common;

use agent_core::backend::Backend;
use agent_core::backend::openai::OpenAIBackend;
use agent_core::backend::openai::fake_provider::{
    ErrorScript, FakeProvider, Step, StreamScript, default_happy_path, finish_chunk,
    flat_error_body, role_chunk, text_delta_chunk, usage_chunk,
};
use agent_core::health::{
    self, BACKEND_SERVICE_NAME, HealthMonitor, HostStatus, UnavailabilityReason,
};
use agent_core::history::{HistoryStore, InMemoryHistory};
use agent_core::service::Service;
use axum::http::StatusCode;
use common::backend_for;
use proto::agent_core::v1::DecideRequest;
use proto::agent_core::v1::agent_core_client::AgentCoreClient;
use proto::agent_core::v1::agent_core_server::{AgentCoreServer, SERVICE_NAME};
use proto::grpc::health::v1::HealthCheckRequest;
use proto::grpc::health::v1::health_check_response::ServingStatus;
use proto::grpc::health::v1::health_client::HealthClient;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::Code;
use tonic::transport::{Channel, Server};

/// Upper bound for anything that could hang, so a failure reports instead of blocking.
const LIMIT: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------- harness

struct Harness {
    provider: FakeProvider,
    backend: OpenAIBackend,
    monitor: HealthMonitor,
    agent_core: AgentCoreClient<Channel>,
    health: HealthClient<Channel>,
}

/// Serves health and `Decide` over the OpenAI-compatible backend, like the binary does.
async fn serve(idle_timeout: &str, max_retries: u16) -> Harness {
    let provider = FakeProvider::start().await.expect("provider");
    let backend = backend_for(&provider, idle_timeout, max_retries);
    let store = InMemoryHistory::default();
    let (monitor, health_server) =
        health::start(backend.subscribe_health(), store.subscribe_health()).await;
    let service = Service::new(backend.clone(), store);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        Server::builder()
            .add_service(health_server)
            .add_service(AgentCoreServer::new(service))
            .serve_with_incoming(TcpListenerStream::new(listener))
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
        provider,
        backend,
        monitor,
        agent_core: AgentCoreClient::new(channel.clone()),
        health: HealthClient::new(channel),
    }
}

impl Harness {
    /// Runs one `Decide` turn to the end and returns the call's final status code, if it failed.
    async fn turn(&mut self) -> Option<Code> {
        self.turn_for("session").await
    }

    async fn turn_for(&mut self, session: &str) -> Option<Code> {
        let request = DecideRequest {
            session_id: session.to_string(),
            speaker_name: None,
            text: "hello".to_string(),
        };
        timeout(LIMIT, async {
            let mut stream = match self.agent_core.decide(request).await {
                Ok(response) => response.into_inner(),
                Err(status) => return Some(status.code()),
            };
            while let Some(item) = stream.next().await {
                if let Err(status) = item {
                    return Some(status.code());
                }
            }
            None
        })
        .await
        .expect("the turn did not finish")
    }

    async fn check(&mut self, service: &str) -> ServingStatus {
        self.health
            .check(HealthCheckRequest {
                service: service.to_string(),
            })
            .await
            .expect("check")
            .into_inner()
            .status()
    }

    /// Waits until the health service shows `expected` for `service`.
    async fn eventually(&mut self, service: &str, expected: ServingStatus) {
        let waited = timeout(LIMIT, async {
            loop {
                if self.check(service).await == expected {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(
            waited.is_ok(),
            "{service:?} never became {expected:?}; it is {:?}",
            self.check(service).await
        );
    }

    /// Waits until the backend's own host status equals `expected`.
    async fn eventually_host_status(&self, expected: HostStatus) {
        let health = self.backend.subscribe_health();
        let waited = timeout(LIMIT, async {
            loop {
                if *health.borrow().host_status() == expected {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(
            waited.is_ok(),
            "the host status never became {expected:?}; it is {:?}",
            health.borrow().host_status()
        );
    }

    fn host_status(&self) -> HostStatus {
        self.backend
            .subscribe_health()
            .borrow()
            .host_status()
            .clone()
    }
}

fn reply() -> StreamScript {
    default_happy_path()
}

fn rejected_key() -> ErrorScript {
    ErrorScript::new(StatusCode::UNAUTHORIZED).body(flat_error_body(
        "Wrong API Key",
        "invalid_request_error",
        Some("api_key"),
        "wrong_api_key",
    ))
}

// ---------------------------------------------------------------- tests

#[tokio::test]
async fn a_fresh_backend_shows_unknown_on_its_entry_and_the_server_is_ready() {
    let mut harness = serve("5s", 0).await;

    assert_eq!(
        harness.check(BACKEND_SERVICE_NAME).await,
        ServingStatus::Unknown
    );
    assert_eq!(harness.check(SERVICE_NAME).await, ServingStatus::Serving);
    assert_eq!(harness.host_status(), HostStatus::Unknown);
}

#[tokio::test]
async fn after_a_successful_turn_the_entry_is_serving() {
    let mut harness = serve("5s", 0).await;
    harness.provider.enqueue(reply());

    assert_eq!(harness.turn().await, None);

    harness
        .eventually(BACKEND_SERVICE_NAME, ServingStatus::Serving)
        .await;
    assert_eq!(harness.host_status(), HostStatus::Available);
}

#[tokio::test]
async fn after_retries_are_exhausted_on_503s_the_entry_is_not_serving_but_readiness_is() {
    let mut harness = serve("5s", 1).await;
    for _ in 0..2 {
        harness
            .provider
            .enqueue(ErrorScript::new(StatusCode::SERVICE_UNAVAILABLE));
    }

    assert_eq!(harness.turn().await, Some(Code::Internal));

    harness
        .eventually(BACKEND_SERVICE_NAME, ServingStatus::NotServing)
        .await;
    assert_eq!(harness.check(SERVICE_NAME).await, ServingStatus::Serving);
    assert_eq!(harness.check("").await, ServingStatus::Serving);
    assert_eq!(
        harness.host_status(),
        HostStatus::Unavailable(UnavailabilityReason::ServerError {
            http_code: 503,
            host_code: None
        })
    );
}

#[tokio::test]
async fn availability_is_decided_once_per_turn_after_retries_not_per_attempt() {
    let mut harness = serve("5s", 2).await;
    harness
        .provider
        .enqueue(ErrorScript::new(StatusCode::SERVICE_UNAVAILABLE));
    harness.provider.enqueue(reply());

    assert_eq!(harness.turn().await, None);

    harness
        .eventually(BACKEND_SERVICE_NAME, ServingStatus::Serving)
        .await;
    assert_eq!(harness.host_status(), HostStatus::Available);
}

#[tokio::test]
async fn after_a_401_the_entry_is_not_serving_with_the_key_rejected_kind() {
    let mut harness = serve("5s", 2).await;
    harness.provider.enqueue(rejected_key());

    assert_eq!(harness.turn().await, Some(Code::Internal));

    harness
        .eventually(BACKEND_SERVICE_NAME, ServingStatus::NotServing)
        .await;
    assert_eq!(harness.check(SERVICE_NAME).await, ServingStatus::Serving);
    assert_eq!(
        harness.host_status(),
        HostStatus::Unavailable(UnavailabilityReason::KeyRejected {
            host_code: Some("wrong_api_key".to_string())
        })
    );
}

#[tokio::test]
async fn a_402_marks_payment_required_not_a_rejected_key() {
    let mut harness = serve("5s", 2).await;
    harness.provider.enqueue(
        ErrorScript::new(StatusCode::PAYMENT_REQUIRED).body(flat_error_body(
            "Out of credit",
            "payment_required",
            None,
            "insufficient_credit",
        )),
    );

    harness.turn().await;

    harness
        .eventually_host_status(HostStatus::Unavailable(
            UnavailabilityReason::PaymentRequired {
                host_code: Some("insufficient_credit".to_string()),
            },
        ))
        .await;
}

#[tokio::test]
async fn a_400_counts_as_the_provider_answering_so_the_entry_is_serving() {
    let mut harness = serve("5s", 2).await;
    harness.provider.enqueue(
        ErrorScript::new(StatusCode::BAD_REQUEST).body(flat_error_body(
            "Bad request",
            "invalid_request_error",
            None,
            "bad_request",
        )),
    );

    assert_eq!(harness.turn().await, Some(Code::Internal));

    harness
        .eventually(BACKEND_SERVICE_NAME, ServingStatus::Serving)
        .await;
    assert_eq!(harness.host_status(), HostStatus::Available);
}

#[tokio::test]
async fn a_400_after_an_outage_restores_available_because_the_provider_answered() {
    let mut harness = serve("5s", 0).await;
    harness.provider.enqueue(rejected_key());
    harness.turn().await;
    harness
        .eventually(BACKEND_SERVICE_NAME, ServingStatus::NotServing)
        .await;
    harness
        .provider
        .enqueue(ErrorScript::new(StatusCode::BAD_REQUEST));

    harness.turn().await;

    harness
        .eventually(BACKEND_SERVICE_NAME, ServingStatus::Serving)
        .await;
}

#[tokio::test]
async fn a_success_after_an_outage_returns_the_entry_to_serving() {
    let mut harness = serve("5s", 0).await;
    harness
        .provider
        .enqueue(ErrorScript::new(StatusCode::SERVICE_UNAVAILABLE));
    harness.turn().await;
    harness
        .eventually(BACKEND_SERVICE_NAME, ServingStatus::NotServing)
        .await;
    harness.provider.enqueue(reply());

    assert_eq!(harness.turn().await, None);

    harness
        .eventually(BACKEND_SERVICE_NAME, ServingStatus::Serving)
        .await;
}

#[tokio::test]
async fn a_turn_that_times_out_marks_the_provider_unavailable_as_timed_out() {
    let mut harness = serve("300ms", 0).await;
    harness
        .provider
        .enqueue(StreamScript::new(vec![Step::data(role_chunk())]).stall());

    assert_eq!(harness.turn().await, Some(Code::Internal));

    harness
        .eventually_host_status(HostStatus::Unavailable(UnavailabilityReason::TimedOut))
        .await;
    harness
        .eventually(BACKEND_SERVICE_NAME, ServingStatus::NotServing)
        .await;
}

#[tokio::test]
async fn a_response_that_does_not_open_in_time_marks_the_provider_timed_out() {
    let mut harness = serve("200ms", 1).await;
    for _ in 0..4 {
        harness
            .provider
            .enqueue(StreamScript::new(Vec::new()).delay_headers(Duration::from_secs(5)));
    }

    assert_eq!(harness.turn().await, Some(Code::Internal));

    harness
        .eventually_host_status(HostStatus::Unavailable(UnavailabilityReason::TimedOut))
        .await;
}

#[tokio::test]
async fn a_reply_that_breaks_mid_stream_marks_the_provider_unavailable_as_stream_interrupted() {
    let mut harness = serve("5s", 0).await;
    harness.provider.enqueue(StreamScript::new(vec![
        Step::data(role_chunk()),
        Step::data(text_delta_chunk("half a reply")),
    ]));

    assert_eq!(harness.turn().await, Some(Code::Internal));

    harness
        .eventually_host_status(HostStatus::Unavailable(
            UnavailabilityReason::StreamInterrupted { host_code: None },
        ))
        .await;
}

#[tokio::test]
async fn an_unreachable_provider_marks_it_unavailable_as_unreachable() {
    let mut harness = serve("5s", 0).await;
    // Take the provider away, keeping the backend pointed at its old address.
    let replacement = FakeProvider::start().await.expect("another provider");
    drop(std::mem::replace(&mut harness.provider, replacement));
    tokio::time::sleep(Duration::from_millis(50)).await;

    harness.turn().await;

    harness
        .eventually_host_status(HostStatus::Unavailable(UnavailabilityReason::Unreachable))
        .await;
}

#[tokio::test]
async fn a_content_filter_failure_leaves_the_provider_available() {
    let mut harness = serve("5s", 0).await;
    harness.provider.enqueue(StreamScript::new(vec![
        Step::data(role_chunk()),
        Step::data(finish_chunk("content_filter")),
        Step::data(usage_chunk(1, 0)),
        Step::done(),
    ]));

    assert_eq!(harness.turn().await, Some(Code::Internal));

    harness
        .eventually(BACKEND_SERVICE_NAME, ServingStatus::Serving)
        .await;
    assert_eq!(harness.host_status(), HostStatus::Available);
}

#[tokio::test]
async fn a_turn_the_client_cancels_leaves_availability_unchanged() {
    let mut harness = serve("30s", 0).await;
    harness.provider.enqueue(
        StreamScript::new(vec![
            Step::data(role_chunk()),
            Step::data(text_delta_chunk("first")),
        ])
        .stall(),
    );
    let request = DecideRequest {
        session_id: "cancelled".to_string(),
        speaker_name: None,
        text: "hello".to_string(),
    };
    let mut stream = harness
        .agent_core
        .decide(request)
        .await
        .expect("decide")
        .into_inner();
    timeout(LIMIT, stream.next())
        .await
        .expect("no first chunk")
        .expect("stream ended")
        .expect("status");

    drop(stream);

    assert!(
        harness
            .provider
            .wait_for_disconnect(Duration::from_secs(5))
            .await
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(harness.host_status(), HostStatus::Unknown);
    assert_eq!(
        harness.check(BACKEND_SERVICE_NAME).await,
        ServingStatus::Unknown
    );
}

#[tokio::test]
async fn shutdown_makes_the_backend_entry_not_serving_whatever_availability_was() {
    let mut harness = serve("5s", 0).await;
    harness.provider.enqueue(reply());
    harness.turn().await;
    harness
        .eventually(BACKEND_SERVICE_NAME, ServingStatus::Serving)
        .await;

    timeout(LIMIT, harness.monitor.begin_shutdown())
        .await
        .expect("begin_shutdown timed out");

    assert_eq!(
        harness.check(BACKEND_SERVICE_NAME).await,
        ServingStatus::NotServing
    );
    assert_eq!(harness.check(SERVICE_NAME).await, ServingStatus::NotServing);
}

#[tokio::test]
async fn each_transition_is_logged_once_and_a_repeat_logs_nothing() {
    let (logs, _guard) = common::capture_logs();
    let mut harness = serve("5s", 0).await;
    harness.provider.enqueue(rejected_key());
    harness.provider.enqueue(rejected_key());

    harness.turn_for("first").await;
    harness
        .eventually(BACKEND_SERVICE_NAME, ServingStatus::NotServing)
        .await;
    harness.turn_for("second").await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert_eq!(
        logs.count_mentioning("keyrejected"),
        1,
        "{:#?}",
        logs.lines()
    );
}

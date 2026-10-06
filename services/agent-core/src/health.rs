//! gRPC health for the Agent Core.
//!
//! Serves the standard gRPC health service. The history store and the backend push their own
//! health through watch channels, and a [`HealthMonitor`] task turns the two into the Agent Core
//! service's status: serving only while both are healthy and shutdown has not begun. The overall
//! server status (the empty service name) stays serving until shutdown begins, so liveness probes
//! point at it and readiness probes at the Agent Core service. The monitor never probes a
//! dependency; a dependency reports a change itself.

use proto::agent_core::v1::agent_core_server::SERVICE_NAME;
use tokio::sync::watch;
use tonic_health::ServingStatus;
use tonic_health::pb::health_server::{Health, HealthServer};
use tonic_health::server::{HealthReporter, health_reporter};

/// Health of an LLM backend, as the backend itself reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendHealth {
    healthy: bool,
    is_backup: bool,
    service: String,
}

impl BackendHealth {
    /// A health value for the backend named `service` (used in log lines). `is_backup` marks a
    /// fallback backend; it is carried and logged, and does not change the reported status.
    pub fn new(service: impl Into<String>, healthy: bool, is_backup: bool) -> Self {
        Self {
            healthy,
            is_backup,
            service: service.into(),
        }
    }

    /// Whether the backend can serve a turn.
    pub fn is_healthy(&self) -> bool {
        self.healthy
    }

    /// Whether this is a fallback backend.
    pub fn is_backup(&self) -> bool {
        self.is_backup
    }

    /// The backend's name, used in log lines.
    pub fn service(&self) -> &str {
        &self.service
    }
}

/// Health of a history store, as the store itself reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryHealth {
    healthy: bool,
    service: String,
}

impl HistoryHealth {
    /// A health value for the store named `service` (used in log lines).
    pub fn new(service: impl Into<String>, healthy: bool) -> Self {
        Self {
            healthy,
            service: service.into(),
        }
    }

    /// Whether the store can serve a turn.
    pub fn is_healthy(&self) -> bool {
        self.healthy
    }

    /// The store's name, used in log lines.
    pub fn service(&self) -> &str {
        &self.service
    }
}

/// The handle to the running health task. Dropping it does not stop the task; the task ends after
/// shutdown has been reported.
#[derive(Debug)]
pub struct HealthMonitor {
    shutdown: watch::Sender<bool>,
    reported: watch::Receiver<bool>,
}

impl HealthMonitor {
    /// Marks the Agent Core service and the overall server not-serving, and returns only once the
    /// health service shows it. Call it when the shutdown signal arrives and before the server
    /// starts draining, so new traffic stops while in-flight turns finish.
    pub async fn begin_shutdown(&self) {
        self.shutdown.send_replace(true);
        // The task is the only writer of statuses, so waiting for its acknowledgement means the
        // reporter is updated. An error means the task is gone; there is nothing left to wait for.
        let _ = self.reported.clone().wait_for(|done| *done).await;
    }
}

/// Starts the health task and returns its handle with the health service to add to the server.
///
/// `backend` and `history` are the subscriptions the backend and the store hand out. The statuses
/// are set before this returns, so the first check already sees them. Must be called inside a
/// tokio runtime.
pub async fn start(
    backend: watch::Receiver<BackendHealth>,
    history: watch::Receiver<HistoryHealth>,
) -> (HealthMonitor, HealthServer<impl Health>) {
    let (reporter, server) = health_reporter();
    let (shutdown, shutdown_rx) = watch::channel(false);
    let (reported_tx, reported) = watch::channel(false);

    let mut state = MonitorState {
        reporter,
        backend,
        history,
        shutdown: shutdown_rx,
        last_status: None,
        last_backend: None,
        last_history: None,
    };
    // The first evaluation happens here, so a check right after startup finds the status.
    let first_done = state.evaluate().await;
    if first_done {
        reported_tx.send_replace(true);
    } else {
        tokio::spawn(async move {
            state.run().await;
            reported_tx.send_replace(true);
        });
    }
    (HealthMonitor { shutdown, reported }, server)
}

/// What the health task knows between evaluations.
struct MonitorState {
    reporter: HealthReporter,
    backend: watch::Receiver<BackendHealth>,
    history: watch::Receiver<HistoryHealth>,
    shutdown: watch::Receiver<bool>,
    last_status: Option<bool>,
    last_backend: Option<BackendHealth>,
    last_history: Option<HistoryHealth>,
}

impl MonitorState {
    /// Re-evaluates every input, updates the reporter, and logs changes. Returns true once
    /// shutdown has been reported, after which nothing else is written.
    async fn evaluate(&mut self) -> bool {
        let shutting_down = *self.shutdown.borrow_and_update();
        let backend = self.backend.borrow_and_update().clone();
        let history = self.history.borrow_and_update().clone();

        if self.last_backend.as_ref() != Some(&backend) {
            log_backend(&backend);
            self.last_backend = Some(backend.clone());
        }
        if self.last_history.as_ref() != Some(&history) {
            log_history(&history);
            self.last_history = Some(history.clone());
        }

        let serving = backend.is_healthy() && history.is_healthy() && !shutting_down;
        if self.last_status != Some(serving) {
            tracing::info!(serving, shutting_down, "agent core health status changed");
            self.last_status = Some(serving);
            self.reporter
                .set_service_status(SERVICE_NAME, status(serving))
                .await;
        }
        if shutting_down {
            self.reporter
                .set_service_status("", ServingStatus::NotServing)
                .await;
        }
        shutting_down
    }

    /// Re-evaluates on every change until shutdown has been reported.
    async fn run(&mut self) {
        loop {
            tokio::select! {
                () = changed(&mut self.backend) => {}
                () = changed(&mut self.history) => {}
                () = changed(&mut self.shutdown) => {}
            }
            if self.evaluate().await {
                return;
            }
        }
    }
}

/// Resolves when the channel has a new value. If its sender is gone, nothing more will change, so
/// it never resolves and the last value stays in force.
async fn changed<T>(receiver: &mut watch::Receiver<T>) {
    if receiver.changed().await.is_err() {
        std::future::pending::<()>().await;
    }
}

fn status(serving: bool) -> ServingStatus {
    if serving {
        ServingStatus::Serving
    } else {
        ServingStatus::NotServing
    }
}

fn log_backend(health: &BackendHealth) {
    if health.is_healthy() {
        tracing::info!(
            service = health.service(),
            is_backup = health.is_backup(),
            "backend is healthy"
        );
    } else {
        tracing::warn!(
            service = health.service(),
            is_backup = health.is_backup(),
            "backend is unhealthy"
        );
    }
}

fn log_history(health: &HistoryHealth) {
    if health.is_healthy() {
        tracing::info!(service = health.service(), "history store is healthy");
    } else {
        tracing::warn!(service = health.service(), "history store is unhealthy");
    }
}

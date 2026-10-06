//! gRPC health for the Agent Core.
//!
//! Serves the standard gRPC health service (`grpc.health.v1.Health`: Check, List and Watch). The
//! history store and the backend push their own
//! health through watch channels, and a [`HealthMonitor`] task turns the two into the Agent Core
//! service's status: serving only while both are healthy and shutdown has not begun. The overall
//! server status (the empty service name) stays serving until shutdown begins, so liveness probes
//! point at it and readiness probes at the Agent Core service. The monitor never probes a
//! dependency; a dependency reports a change itself.
//!
//! The statuses, by service name:
//! - `""`: the process, serving until shutdown begins.
//! - [`SERVICE_NAME`]: serving only while both dependencies are healthy and shutdown has not begun.
//! - [`HISTORY_SERVICE_NAME`] and [`BACKEND_SERVICE_NAME`]: whether that one dependency is
//!   healthy, so an operator can see which one makes the Agent Core unready. `is_backup` does not
//!   change them.
//!
//! Shutdown sets all four to not-serving, and every `Watch` stream ends after its last update.

use proto::agent_core::v1::agent_core_server::SERVICE_NAME;
use proto::grpc::health::v1::health_check_response::ServingStatus;
use proto::grpc::health::v1::health_server::{Health, HealthServer};
use proto::grpc::health::v1::{
    HealthCheckRequest, HealthCheckResponse, HealthListRequest, HealthListResponse,
};
use std::collections::HashMap;
use std::pin::Pin;
use tokio::sync::watch;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

/// The history store's own status: `aria.agent_core.v1.AgentCore` plus `.history`. A test keeps it
/// in step with the generated service name.
pub const HISTORY_SERVICE_NAME: &str = "aria.agent_core.v1.AgentCore.history";

/// The backend's own status: `aria.agent_core.v1.AgentCore` plus `.backend`.
pub const BACKEND_SERVICE_NAME: &str = "aria.agent_core.v1.AgentCore.backend";

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

/// The handle to the running health task. Dropping it ends the task, and with it every open
/// `Watch` stream; the task also ends once shutdown has been reported.
#[derive(Debug)]
pub struct HealthMonitor {
    shutdown: watch::Sender<bool>,
    reported: watch::Receiver<bool>,
}

impl HealthMonitor {
    /// Marks the Agent Core service, the overall server and both dependency statuses not-serving,
    /// and returns only once the health service shows it. Call it when the shutdown signal arrives
    /// and before the server starts draining, so new traffic stops while in-flight turns finish.
    pub async fn begin_shutdown(&self) {
        self.shutdown.send_replace(true);
        // The task is the only writer of statuses, so waiting for its acknowledgement means they
        // are published. An error means the task is gone; there is nothing left to wait for.
        let _ = self.reported.clone().wait_for(|done| *done).await;
    }
}

/// The four published statuses. The health task is their only writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Statuses {
    overall: ServingStatus,
    agent_core: ServingStatus,
    history: ServingStatus,
    backend: ServingStatus,
}

impl Statuses {
    /// The status of the named service, or `None` if the name is not one of the four.
    fn get(&self, name: &str) -> Option<ServingStatus> {
        match name {
            "" => Some(self.overall),
            SERVICE_NAME => Some(self.agent_core),
            HISTORY_SERVICE_NAME => Some(self.history),
            BACKEND_SERVICE_NAME => Some(self.backend),
            _ => None,
        }
    }

    fn all(&self) -> [(&'static str, ServingStatus); 4] {
        [
            ("", self.overall),
            (SERVICE_NAME, self.agent_core),
            (HISTORY_SERVICE_NAME, self.history),
            (BACKEND_SERVICE_NAME, self.backend),
        ]
    }
}

/// Starts the health task and returns its handle with the health service to add to the server.
///
/// `backend` and `history` are the subscriptions the backend and the store hand out. The statuses
/// are published before this returns, so the first check already sees them. Must be called inside
/// a tokio runtime.
pub async fn start(
    backend: watch::Receiver<BackendHealth>,
    history: watch::Receiver<HistoryHealth>,
) -> (HealthMonitor, HealthServer<impl Health>) {
    let (published, statuses) = watch::channel(Statuses {
        overall: ServingStatus::Serving,
        agent_core: ServingStatus::NotServing,
        history: ServingStatus::NotServing,
        backend: ServingStatus::NotServing,
    });
    let (shutdown, shutdown_rx) = watch::channel(false);
    let (reported_tx, reported) = watch::channel(false);
    let server = HealthServer::new(HealthImpl {
        statuses,
        reported: reported.clone(),
    });

    let mut state = MonitorState {
        published,
        backend,
        history,
        shutdown: shutdown_rx,
        last_serving: None,
        last_backend: None,
        last_history: None,
    };
    // The first evaluation happens here, so a check right after startup finds the statuses.
    state.evaluate();
    tokio::spawn(async move {
        state.run().await;
        reported_tx.send_replace(true);
    });
    (HealthMonitor { shutdown, reported }, server)
}

/// What the health task knows between evaluations.
struct MonitorState {
    published: watch::Sender<Statuses>,
    backend: watch::Receiver<BackendHealth>,
    history: watch::Receiver<HistoryHealth>,
    shutdown: watch::Receiver<bool>,
    last_serving: Option<bool>,
    last_backend: Option<BackendHealth>,
    last_history: Option<HistoryHealth>,
}

impl MonitorState {
    /// Re-evaluates every input, publishes the statuses and logs changes. Returns true once
    /// shutdown has been published, after which nothing else is written.
    fn evaluate(&mut self) -> bool {
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
        if self.last_serving != Some(serving) {
            tracing::info!(serving, shutting_down, "agent core health status changed");
            self.last_serving = Some(serving);
        }
        // Each dependency's own status reflects only its `healthy`; every status is not-serving
        // at shutdown.
        self.published.send_replace(Statuses {
            overall: status(!shutting_down),
            agent_core: status(serving),
            history: status(history.is_healthy() && !shutting_down),
            backend: status(backend.is_healthy() && !shutting_down),
        });
        shutting_down
    }

    /// Re-evaluates on every change until shutdown has been reported or the monitor is dropped.
    async fn run(&mut self) {
        loop {
            tokio::select! {
                () = changed(&mut self.backend) => {}
                () = changed(&mut self.history) => {}
                result = self.shutdown.changed() => {
                    if result.is_err() {
                        return;
                    }
                }
            }
            if self.evaluate() {
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

fn response(status: ServingStatus) -> HealthCheckResponse {
    HealthCheckResponse {
        status: status.into(),
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

/// The `grpc.health.v1.Health` service over the published statuses. `Watch` streams end once
/// shutdown has been reported: tonic's graceful shutdown waits for open streams, so a watcher left
/// open would hold the drain for its whole bound.
struct HealthImpl {
    statuses: watch::Receiver<Statuses>,
    reported: watch::Receiver<bool>,
}

type WatchStream = Pin<Box<dyn Stream<Item = Result<HealthCheckResponse, Status>> + Send>>;

#[tonic::async_trait]
impl Health for HealthImpl {
    async fn check(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let name = request.into_inner().service;
        match self.statuses.borrow().get(&name) {
            Some(status) => Ok(Response::new(response(status))),
            None => Err(Status::not_found("service not registered")),
        }
    }

    async fn list(
        &self,
        _request: Request<HealthListRequest>,
    ) -> Result<Response<HealthListResponse>, Status> {
        let statuses: HashMap<String, HealthCheckResponse> = self
            .statuses
            .borrow()
            .all()
            .into_iter()
            .map(|(name, status)| (name.to_string(), response(status)))
            .collect();
        Ok(Response::new(HealthListResponse { statuses }))
    }

    type WatchStream = WatchStream;

    async fn watch(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        let name = request.into_inner().service;
        let mut statuses = self.statuses.clone();
        let mut reported = self.reported.clone();
        let stream = async_stream::stream! {
            let known = statuses.borrow().get(&name).is_some();
            if !known {
                // An unknown name is reported as such and the call stays open, as the protocol
                // says; it ends with the others at shutdown.
                yield Ok(response(ServingStatus::ServiceUnknown));
                let _ = reported.wait_for(|done| *done).await;
                return;
            }
            let mut last = None;
            loop {
                let current = statuses.borrow_and_update().get(&name);
                if let Some(current) = current && last != Some(current) {
                    last = Some(current);
                    yield Ok(response(current));
                }
                let shutdown_reported = async {
                    // A dropped sender means the task is gone, which ends the watchers too.
                    let _ = reported.wait_for(|done| *done).await;
                };
                tokio::select! {
                    changed = statuses.changed() => {
                        if changed.is_err() {
                            return;
                        }
                    }
                    () = shutdown_reported => {
                        // Shutdown is reported: the last update must be not-serving, then end.
                        let current = statuses.borrow().get(&name);
                        if current != last && let Some(current) = current {
                            yield Ok(response(current));
                            last = Some(current);
                        }
                        if last != Some(ServingStatus::NotServing) {
                            yield Ok(response(ServingStatus::NotServing));
                        }
                        return;
                    }
                }
            }
        };
        Ok(Response::new(Box::pin(stream)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependency_names_extend_the_generated_service_name() {
        assert_eq!(HISTORY_SERVICE_NAME, format!("{SERVICE_NAME}.history"));
        assert_eq!(BACKEND_SERVICE_NAME, format!("{SERVICE_NAME}.backend"));
    }
}

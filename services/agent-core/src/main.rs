use agent_core::backend::Backend;
use agent_core::backend::echo::EchoBackend;
use agent_core::config::Config;
use agent_core::history::{HistoryStore, InMemoryHistory};
use agent_core::service::Service;
use proto::agent_core::v1::agent_core_server::AgentCoreServer;
use std::error::Error;
use std::process::ExitCode;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;

/// Resolves when the process is asked to stop: SIGTERM or Ctrl-C on Unix, Ctrl-C, Ctrl-Break or
/// console close on Windows. Logs which signal arrived. A handler that cannot be installed is
/// logged and skipped rather than panicking.
async fn shutdown_signal() {
    let signal = wait_for_signal().await;
    tracing::info!(signal, "shutdown signal received, shutting down");
}

/// Waits for Ctrl-C and returns its name. If the handler cannot be installed, logs it and never
/// resolves.
async fn ctrl_c() -> &'static str {
    if let Err(e) = tokio::signal::ctrl_c().await {
        tracing::error!(error = %e, "cannot listen for Ctrl-C");
        std::future::pending::<()>().await;
    }
    "Ctrl-C"
}

#[cfg(unix)]
async fn wait_for_signal() -> &'static str {
    use tokio::signal::unix::{SignalKind, signal};

    match signal(SignalKind::terminate()) {
        Ok(mut term) => tokio::select! {
            _ = term.recv() => "SIGTERM",
            name = ctrl_c() => name,
        },
        Err(e) => {
            tracing::error!(error = %e, "cannot install SIGTERM handler, falling back to Ctrl-C only");
            ctrl_c().await
        }
    }
}

#[cfg(not(unix))]
async fn wait_for_signal() -> &'static str {
    use tokio::signal::windows::{ctrl_break, ctrl_close};

    match (ctrl_break(), ctrl_close()) {
        (Ok(mut brk), Ok(mut close)) => tokio::select! {
            _ = brk.recv() => 42,
            _ = close.recv() => "console close",
            name = ctrl_c() => name,
        },
        (brk, close) => {
            for e in [brk.err(), close.err()].into_iter().flatten() {
                tracing::error!(error = %e, "cannot install console signal handler, falling back to Ctrl-C only");
            }
            ctrl_c().await
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // Printed with `Display`: returning the error from `main` would print its `Debug`.
            eprintln!("agent-core: {}", error_chain(e.as_ref()));
            ExitCode::FAILURE
        }
    }
}

/// The reflection services, v1 and v1alpha (grpcurl and Postman differ in which they ask for),
/// describing the Agent Core and the standard health service.
fn reflection_services() -> Result<
    (
        tonic_reflection::server::v1::ServerReflectionServer<
            impl tonic_reflection::server::v1::ServerReflection,
        >,
        tonic_reflection::server::v1alpha::ServerReflectionServer<
            impl tonic_reflection::server::v1alpha::ServerReflection,
        >,
    ),
    tonic_reflection::server::Error,
> {
    let builder = || {
        tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(proto::FILE_DESCRIPTOR_SET)
            .with_service_name(proto::agent_core::v1::agent_core_server::SERVICE_NAME)
            .with_service_name("grpc.health.v1.Health")
    };
    Ok((builder().build_v1()?, builder().build_v1alpha()?))
}

/// How long in-flight requests get to finish after the shutdown signal. Kept below Kubernetes'
/// default 30 s termination grace period, so the process exits on its own before the SIGKILL.
const DRAIN_BOUND: Duration = Duration::from_secs(20);

/// The error and its sources, joined with ": ", so the cause is not lost.
fn error_chain(error: &(dyn Error + 'static)) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

/// Reads the configuration, sets up tracing and serves until a shutdown signal arrives.
async fn run() -> Result<(), Box<dyn Error>> {
    let config = Config::from_env()?;

    let telemetry = telemetry::init!(
        "aria-agent-core",
        config.otlp_endpoint(),
        target = agent_core::CRATE_NAME
    )?;

    let backend = EchoBackend::new();
    let history = InMemoryHistory::default();
    let backend_name = backend.name();
    let (health, health_server) =
        agent_core::health::start(backend.subscribe_health(), history.subscribe_health()).await;
    let service = Service::new(backend, history);
    tracing::info!(
        reflection = config.reflection(),
        "gRPC server reflection is {}",
        if config.reflection() { "on" } else { "off" }
    );

    // Bound here, not by tonic, so the log names the real address (even for port 0).
    let listener = TcpListener::bind(config.listen_address()).await?;
    tracing::info!(
        version = agent_core::version::VERSION,
        git_sha = agent_core::version::GIT_SHA.unwrap_or("unknown"),
        "Starting agent-core service with '{backend_name}' backend on '{}'",
        listener.local_addr()?
    );

    let (signalled_tx, signalled_rx) = oneshot::channel();
    let mut router = tonic::transport::Server::builder()
        .layer(agent_core::version::VersionLayer)
        .add_service(health_server)
        .add_service(AgentCoreServer::new(service));
    if config.reflection() {
        let (v1, v1alpha) = reflection_services()?;
        router = router.add_service(v1).add_service(v1alpha);
    }
    let serve = router.serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
        shutdown_signal().await;
        // Not-serving is reported before tonic starts draining, so new traffic stops first.
        health.begin_shutdown().await;
        // The receiver is gone only when serving already ended.
        let _ = signalled_tx.send(());
    });
    let served = serve_with_bounded_drain(serve, signalled_rx, DRAIN_BOUND).await;

    if let Err(e) = telemetry.shutdown() {
        tracing::error!(error = %error_chain(&e), "failed to shut down telemetry");
    }

    Ok(served.unwrap_or(Ok(()))?)
}

/// Runs `serve` to completion, but once `shutdown_started` fires gives it only `bound` more time.
/// Returns `None`, after logging a warning, when the bound was hit and `serve` was dropped.
async fn serve_with_bounded_drain<R>(
    serve: impl Future<Output = R>,
    shutdown_started: oneshot::Receiver<()>,
    bound: Duration,
) -> Option<R> {
    tokio::select! {
        result = serve => Some(result),
        () = async {
            // A dropped sender means serving ended on its own, which the other branch reports.
            if shutdown_started.await.is_ok() {
                tokio::time::sleep(bound).await;
            } else {
                std::future::pending::<()>().await;
            }
        } => {
            tracing::warn!(?bound, "in-flight requests did not finish in time, stopping anyway");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn drain_gives_up_after_the_bound_once_shutdown_started() {
        let (tx, rx) = oneshot::channel();
        tx.send(()).unwrap();
        let started = std::time::Instant::now();

        let result = serve_with_bounded_drain(
            std::future::pending::<&str>(),
            rx,
            Duration::from_millis(100),
        )
        .await;

        assert_eq!(result, None);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn drain_returns_the_result_of_a_serve_that_finishes_in_time() {
        let (tx, rx) = oneshot::channel();
        let serve = async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            "done"
        };
        tx.send(()).unwrap();

        let result = serve_with_bounded_drain(serve, rx, Duration::from_secs(5)).await;

        assert_eq!(result, Some("done"));
    }

    #[tokio::test]
    async fn drain_waits_indefinitely_while_no_shutdown_has_started() {
        let (_tx, rx) = oneshot::channel::<()>();
        let serve = async {
            tokio::time::sleep(Duration::from_millis(150)).await;
            "done"
        };

        let result = serve_with_bounded_drain(serve, rx, Duration::from_millis(10)).await;

        assert_eq!(result, Some("done"));
    }
}

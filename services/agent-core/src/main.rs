use agent_core::backend::echo::EchoBackend;
use agent_core::config::Config;
use agent_core::history::InMemoryHistory;
use agent_core::service::Service;
use agent_core::telemetry::configure_tracing;
use proto::agent_core::v1::agent_core_server::AgentCoreServer;
use std::error::Error;

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
            _ = brk.recv() => "Ctrl-Break",
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
async fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::from_env()?;

    let provider = configure_tracing(config.otlp_endpoint().map(str::to_owned))?;

    let service: Service<EchoBackend, InMemoryHistory> = Service::default();
    tracing::info!(
        "Starting agent-core service with 'echo' backend on '{}'",
        config.listen_address()
    );

    let served = tonic::transport::Server::builder()
        .add_service(AgentCoreServer::new(service))
        .serve_with_shutdown(config.listen_address(), shutdown_signal())
        .await;
    if let Err(e) = &served {
        tracing::error!(error = %e, "server failed");
    }

    if let Some(Err(e)) = provider.map(|p| p.shutdown()) {
        tracing::error!(error = %e, "failed to shut down OTLP tracer provider");
    }

    Ok(served?)
}

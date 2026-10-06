//! Tracing setup.
//! Logs always go to stdout, filtered by `RUST_LOG` (default `info`); OTLP span export is enabled
//! only when an endpoint is configured.
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;
use std::io::IsTerminal;
use tracing::Level;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Layer};

/// Installs the global tracing subscriber and returns the span exporter's provider, if any.
///
/// Logs go to stdout, filtered by `RUST_LOG`. With an OTLP endpoint, spans are also exported to
/// it, filtered separately. The caller must `shutdown` the returned provider on exit to flush.
pub fn configure_tracing(
    otlp_endpoint: Option<String>,
) -> Result<Option<SdkTracerProvider>, Box<dyn std::error::Error>> {
    // `RUST_LOG` filters the stdout logs only; an unset, blank or invalid value falls back to `info`.
    // Span export has its own filter, so a quiet log level never thins the traces.
    // A blank `RUST_LOG` counts as unset.
    let rust_log = std::env::var("RUST_LOG")
        .ok()
        .filter(|v| !v.trim().is_empty());
    let (filter, invalid_filter) = match rust_log {
        None => (EnvFilter::new("info"), None),
        Some(value) => match EnvFilter::try_new(&value) {
            Ok(filter) => (filter, None),
            Err(_) => (EnvFilter::new("info"), Some(value)),
        },
    };
    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stdout)
        .with_ansi(std::io::stdout().is_terminal())
        .with_target(false)
        .with_level(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_filter(filter);

    let subscriber = tracing_subscriber::registry().with(stdout_layer);

    let provider = if let Some(endpoint) = otlp_endpoint {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()?;
        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(
                opentelemetry_sdk::Resource::builder()
                    .with_service_name("aria-agent-core")
                    .build(),
            )
            .build();
        let otel_layer = tracing_opentelemetry::layer()
            .with_tracer(provider.tracer("aria-agent-core"))
            .with_filter(export_filter());
        tracing::subscriber::set_global_default(subscriber.with(otel_layer))?;

        tracing::info!("OTLP tracing enabled");
        Some(provider)
    } else {
        tracing::subscriber::set_global_default(subscriber)?;

        tracing::info!("OTLP tracing disabled");
        None
    };

    if let Some(value) = invalid_filter {
        tracing::warn!(rust_log = %value, "invalid RUST_LOG, falling back to 'info'");
    }
    Ok(provider)
}

/// The filter on span export: this crate's spans down to `debug`; dependencies only at `warn`, so
/// their per-request spans do not flood the collector.
fn export_filter() -> Targets {
    Targets::new()
        .with_default(Level::WARN)
        .with_target(env!("CARGO_CRATE_NAME"), Level::DEBUG)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_filter_passes_this_crates_decide_span() {
        let filter = export_filter();

        assert!(filter.would_enable("agent_core::service", &Level::INFO));
        assert!(filter.would_enable("agent_core::service", &Level::DEBUG));
    }

    #[test]
    fn export_filter_drops_dependency_debug_but_keeps_their_warnings() {
        let filter = export_filter();

        for target in [
            "h2::proto::connection",
            "hyper::proto::h2",
            "tonic::transport::server",
        ] {
            assert!(!filter.would_enable(target, &Level::DEBUG), "{target}");
            assert!(filter.would_enable(target, &Level::WARN), "{target}");
        }
    }
}

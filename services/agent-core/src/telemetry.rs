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
    // `RUST_LOG` filters the stdout logs only; an unset or invalid value falls back to `info`.
    // Span export has its own filter, so a quiet log level never thins the traces.
    let (filter, invalid_filter) = match EnvFilter::try_from_default_env() {
        Ok(filter) => (filter, None),
        Err(_) => (EnvFilter::new("info"), std::env::var("RUST_LOG").ok()),
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
                    .with_service_name("agent-core")
                    .build(),
            )
            .build();
        // Export this crate's spans down to `debug`; dependencies only at `warn`, so their
        // per-request spans do not flood the collector.
        let export_filter = Targets::new()
            .with_default(Level::WARN)
            .with_target(env!("CARGO_CRATE_NAME"), Level::DEBUG);
        let otel_layer = tracing_opentelemetry::layer()
            .with_tracer(provider.tracer("agent-core"))
            .with_filter(export_filter);
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

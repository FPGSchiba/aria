//! Tracing setup.
//! Logs always go to stdout, filtered by `RUST_LOG` (default `info`); OTLP span export is enabled
//! only when an endpoint is configured.
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::{EnvFilter, Layer};

pub fn configure_tracing(
    otlp_endpoint: Option<String>,
) -> Result<Option<SdkTracerProvider>, Box<dyn std::error::Error>> {
    // `RUST_LOG` filters the stdout logs only; an unset or invalid value falls back to `info`.
    // The filter is not applied to span export, so a quiet log level never thins the traces.
    let (filter, invalid_filter) = match EnvFilter::try_from_default_env() {
        Ok(filter) => (filter, None),
        Err(_) => (EnvFilter::new("info"), std::env::var("RUST_LOG").ok()),
    };
    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stdout)
        .with_ansi(true)
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
            .with_batch_exporter(exporter) // no runtime argument any more
            .with_resource(
                opentelemetry_sdk::Resource::builder()
                    .with_service_name("agent-core")
                    .build(),
            )
            .build();
        let otel_layer = tracing_opentelemetry::layer().with_tracer(provider.tracer("agent-core"));
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

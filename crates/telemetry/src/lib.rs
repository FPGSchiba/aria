//! Shared telemetry setup for ARIA services.
//!
//! One call sets up stdout logging, optional OTLP span export, the export filter and the tracer
//! provider shutdown, so every service does it the same way. Use the [`init!`] macro: it takes the
//! service name and the OTLP endpoint, and captures the calling crate's name so the export filter
//! knows which spans are the service's own. A service that does not depend on this crate gets none
//! of its dependencies.
//!
//! Span export needs a running multi-thread tokio runtime: call [`init!`] from inside it, and
//! call [`TelemetryGuard::shutdown`] inside it too. Without an OTLP endpoint no runtime is needed.

use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::{ExporterBuildError, WithExportConfig};
use opentelemetry_sdk::error::OTelSdkError;
use opentelemetry_sdk::trace::SdkTracerProvider;
use std::io::IsTerminal;
use tracing::Level;
use tracing::subscriber::SetGlobalDefaultError;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Layer};

/// Why telemetry setup or shutdown failed.
#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    /// The OTLP span exporter could not be built, for example from a malformed endpoint.
    #[error("failed to build the OTLP span exporter")]
    ExporterBuild(#[source] ExporterBuildError),
    /// A global tracing subscriber was already installed, so this one could not be.
    #[error("a global tracing subscriber is already installed")]
    SubscriberAlreadyInstalled(#[source] SetGlobalDefaultError),
    /// The tracer provider failed to flush and shut down.
    #[error("failed to shut down the tracer provider")]
    Shutdown(#[source] OTelSdkError),
    /// An OTLP endpoint is set but there is no current tokio runtime to export spans on.
    #[error("span export needs a running tokio runtime, and there is none")]
    NoRuntime,
}

/// Flushes span export on the way out.
///
/// Hold it for as long as the service runs, then call [`shutdown`](Self::shutdown) inside the
/// runtime to flush the spans still buffered, stop the exporter and see whether that worked.
/// Dropping it without that call, for example by an early `?` return, only flushes once and logs a
/// failure: export keeps running afterwards, because the installed subscriber still holds the
/// provider. When span export is off the guard does nothing.
#[must_use = "dropping the guard early only flushes once and export keeps running; hold it until the service exits and call `shutdown()` for a clean stop"]
#[derive(Debug)]
pub struct TelemetryGuard {
    provider: Option<SdkTracerProvider>,
}

impl TelemetryGuard {
    /// Flushes buffered spans and shuts the exporter down. Call it inside the tokio runtime.
    ///
    /// # Errors
    /// `Shutdown` if the provider could not flush or shut down. Always `Ok` when export is off.
    pub fn shutdown(mut self) -> Result<(), TelemetryError> {
        match self.provider.take() {
            Some(provider) => provider.shutdown().map_err(TelemetryError::Shutdown),
            None => Ok(()),
        }
    }
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take()
            && let Err(e) = provider.force_flush()
        {
            tracing::error!(error = %e, "failed to flush the tracer provider");
        }
    }
}

/// Installs the global tracing subscriber for a service and returns the guard to hold until exit.
///
/// `$service_name` is the OpenTelemetry `service.name` and tracer name, for example
/// `"aria-agent-core"`. The macro also records the calling crate's version (`CARGO_PKG_VERSION`,
/// read where the macro expands) as `service.version`. `$otlp_endpoint` is an `Option<&str>`: with an endpoint, spans are also
/// exported to it; with `None`, or a blank value (empty or only whitespace), nothing is exported
/// and no collector is contacted.
///
/// Span export needs a running multi-thread tokio runtime, so call this from inside one; with a
/// current-thread runtime a warning is logged, because the exporter's flush can then block it.
///
/// The macro expands in the calling crate and records that crate's name, so the span export filter
/// keeps the caller's own spans down to `debug` and everything else only at `warn`. That filter is
/// fixed and separate from the log filter, so a quiet `RUST_LOG` never thins the traces. Pass
/// `target = "crate_name"` as a third argument to override the captured name; this is needed when
/// the binary's crate name differs from the library crate holding the spans. A trailing comma is
/// allowed.
///
/// Logs always go to stdout, filtered by `RUST_LOG`. An unset, blank or invalid value falls back
/// to `info`, and an invalid one is named in a warning. Colour is used only when stdout is a
/// terminal.
///
/// # Errors
/// [`TelemetryError::NoRuntime`] if an endpoint is set but there is no current tokio runtime
/// (checked before anything is installed), [`TelemetryError::ExporterBuild`] if the exporter
/// cannot be built, and [`TelemetryError::SubscriberAlreadyInstalled`] if a global subscriber is
/// already set.
#[macro_export]
macro_rules! init {
    ($service_name:expr, $otlp_endpoint:expr $(,)?) => {
        $crate::__init(
            $service_name,
            $otlp_endpoint,
            ::core::env!("CARGO_CRATE_NAME"),
            ::core::env!("CARGO_PKG_VERSION"),
        )
    };
    ($service_name:expr, $otlp_endpoint:expr, target = $target:expr $(,)?) => {
        $crate::__init(
            $service_name,
            $otlp_endpoint,
            $target,
            ::core::env!("CARGO_PKG_VERSION"),
        )
    };
}

/// The work behind [`init!`]; call the macro, which supplies `own_target`.
#[doc(hidden)]
pub fn __init(
    service_name: &str,
    otlp_endpoint: Option<&str>,
    own_target: &str,
    service_version: &str,
) -> Result<TelemetryGuard, TelemetryError> {
    // Checked before anything is installed, so a failure leaves the process untouched.
    let endpoint = resolve_endpoint(otlp_endpoint);
    let runtime_flavor = match endpoint {
        Some(_) => Some(
            tokio::runtime::Handle::try_current()
                .map_err(|_| TelemetryError::NoRuntime)?
                .runtime_flavor(),
        ),
        None => None,
    };

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

    let provider = if let Some(endpoint) = endpoint {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()
            .map_err(TelemetryError::ExporterBuild)?;
        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource(service_name, service_version))
            .build();
        let otel_layer = tracing_opentelemetry::layer()
            .with_tracer(provider.tracer(service_name.to_owned()))
            .with_filter(export_filter(own_target));
        tracing::subscriber::set_global_default(subscriber.with(otel_layer))
            .map_err(TelemetryError::SubscriberAlreadyInstalled)?;

        tracing::info!("OTLP tracing enabled");
        Some(provider)
    } else {
        tracing::subscriber::set_global_default(subscriber)
            .map_err(TelemetryError::SubscriberAlreadyInstalled)?;

        tracing::info!("OTLP tracing disabled");
        None
    };

    if runtime_flavor == Some(tokio::runtime::RuntimeFlavor::CurrentThread) {
        tracing::warn!(
            "span export is running on a current-thread runtime; use a multi-thread one"
        );
    }
    if let Some(value) = invalid_filter {
        tracing::warn!(rust_log = %value, "invalid RUST_LOG, falling back to 'info'");
    }
    Ok(TelemetryGuard { provider })
}

/// The resource attached to every exported span: the service's name and version.
fn resource(service_name: &str, service_version: &str) -> opentelemetry_sdk::Resource {
    opentelemetry_sdk::Resource::builder()
        .with_service_name(service_name.to_owned())
        .with_attribute(opentelemetry::KeyValue::new(
            "service.version",
            service_version.to_owned(),
        ))
        .build()
}

/// The endpoint to export to: `None` when unset or blank, otherwise the trimmed value.
fn resolve_endpoint(endpoint: Option<&str>) -> Option<&str> {
    endpoint.map(str::trim).filter(|e| !e.is_empty())
}

/// The filter on span export: spans from `own_target` down to `debug`, everything else only at
/// `warn`, so dependencies' per-request spans do not flood the collector.
fn export_filter(own_target: &str) -> Targets {
    Targets::new()
        .with_default(Level::WARN)
        .with_target(own_target.to_owned(), Level::DEBUG)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_filter_keeps_the_given_targets_spans_down_to_debug() {
        let filter = export_filter("agent_core");

        assert!(filter.would_enable("agent_core::service", &Level::INFO));
        assert!(filter.would_enable("agent_core::service", &Level::DEBUG));
    }

    #[test]
    fn export_filter_drops_dependency_debug_but_keeps_their_warnings() {
        let filter = export_filter("agent_core");

        for target in [
            "h2::proto::connection",
            "hyper::proto::h2",
            "tonic::transport::server",
        ] {
            assert!(!filter.would_enable(target, &Level::DEBUG), "{target}");
            assert!(filter.would_enable(target, &Level::WARN), "{target}");
        }
    }

    #[test]
    fn export_filter_for_another_target_keeps_its_spans_and_not_agent_cores() {
        let filter = export_filter("other_service");

        assert!(filter.would_enable("other_service::handler", &Level::DEBUG));
        assert!(!filter.would_enable("agent_core::service", &Level::INFO));
        assert!(!filter.would_enable("agent_core::service", &Level::DEBUG));
    }

    #[test]
    fn unset_or_blank_endpoint_builds_no_exporter() {
        assert_eq!(resolve_endpoint(None), None);
        assert_eq!(resolve_endpoint(Some("")), None);
        assert_eq!(resolve_endpoint(Some("   \t")), None);
    }

    #[test]
    fn endpoint_is_used_trimmed() {
        assert_eq!(
            resolve_endpoint(Some(" http://collector:4317 ")),
            Some("http://collector:4317")
        );
    }

    #[test]
    fn guard_without_export_shuts_down_cleanly() {
        let guard = TelemetryGuard { provider: None };

        assert!(guard.shutdown().is_ok());
    }

    #[test]
    fn export_filter_uses_the_override_target() {
        let filter = export_filter("custom_bin");

        assert!(filter.would_enable("custom_bin::main", &Level::DEBUG));
        assert!(!filter.would_enable("agent_core::service", &Level::DEBUG));
    }

    #[test]
    fn init_macro_accepts_a_target_override_and_trailing_commas() {
        // Never called: it only has to compile, since a call would install a global subscriber.
        let _compiles = || {
            let _ = init!("svc", None);
            let _ = init!("svc", None,);
            let _ = init!("svc", None, target = "custom_bin");
            init!("svc", None, target = "custom_bin",)
        };
    }

    #[test]
    fn endpoint_without_a_runtime_is_no_runtime_and_installs_nothing() {
        let result = __init("svc", Some("http://localhost:4317"), "svc", "1.2.3");

        assert!(matches!(result, Err(TelemetryError::NoRuntime)));
        assert!(
            tracing::subscriber::set_global_default(tracing_subscriber::registry()).is_ok(),
            "NoRuntime must be reported before any subscriber is installed"
        );
    }

    #[test]
    fn resource_carries_the_service_name_and_version() {
        let resource = resource("aria-svc", "1.2.3");

        let get = |key: &str| {
            resource
                .get(&opentelemetry::Key::new(key.to_owned()))
                .map(|v| v.to_string())
        };
        assert_eq!(get("service.name").as_deref(), Some("aria-svc"));
        assert_eq!(get("service.version").as_deref(), Some("1.2.3"));
    }

    #[test]
    fn telemetry_error_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync + 'static>() {}
        assert_send_sync::<TelemetryError>();
    }
}

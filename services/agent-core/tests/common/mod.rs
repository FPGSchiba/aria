//! Helpers shared by the integration test binaries.

use std::sync::Once;

/// Call before every `tracing::subscriber::set_default` in a test, or the capture can silently
/// miss events when tests run in parallel.
///
/// tracing-core caches each callsite's interest globally. While only one dispatcher is live, it
/// computes that interest from the default of whichever thread hits the callsite first. A
/// parallel test with no subscriber then caches `never`, and every other thread's capture loses
/// that event for good. A permanent global dispatcher keeps at least two live, so interest is
/// computed from all of them instead. The registry has no layers, so it records nothing.
pub fn keep_tracing_interest_global() {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        tracing::subscriber::set_global_default(tracing_subscriber::registry())
            .expect("no other global tracing subscriber in the test binary");
    });
}

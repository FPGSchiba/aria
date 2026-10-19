//! Helpers shared by the integration test binaries.
#![allow(dead_code)]

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

use agent_core::backend::openai::OpenAIBackend;
use agent_core::backend::openai::fake_provider::FakeProvider;
use agent_core::config::{
    ARIA_LLM_API_KEY_VAR, ARIA_LLM_BASE_URL_VAR, ARIA_LLM_IDLE_TIMEOUT_VAR,
    ARIA_LLM_MAX_RETRIES_VAR, ARIA_LLM_MODEL_VAR, Config,
};
use std::time::Duration;

/// The API key every test configuration carries. Tests assert it never leaks.
pub const TEST_KEY: &str = "sk-test-key-0123456789";

/// The model every test configuration requests.
pub const TEST_MODEL: &str = "test-model";

/// The backoff base tests give the backend, so retries do not slow them down.
pub const TEST_BACKOFF: Duration = Duration::from_millis(1);

/// A configuration pointing at `base_url`, with the given idle timeout (a duration string such
/// as `"300ms"`) and retry count.
pub fn llm_config(base_url: &str, idle_timeout: &str, max_retries: u16) -> Config {
    let base_url = base_url.to_string();
    let idle_timeout = idle_timeout.to_string();
    Config::from_lookup(move |name| {
        Ok(match name {
            ARIA_LLM_API_KEY_VAR => Some(TEST_KEY.to_string()),
            ARIA_LLM_BASE_URL_VAR => Some(base_url.clone()),
            ARIA_LLM_MODEL_VAR => Some(TEST_MODEL.to_string()),
            ARIA_LLM_IDLE_TIMEOUT_VAR => Some(idle_timeout.clone()),
            ARIA_LLM_MAX_RETRIES_VAR => Some(max_retries.to_string()),
            _ => None,
        })
    })
    .expect("a valid test configuration")
}

/// A backend talking to `provider`, with a tiny backoff base.
pub fn backend_for(provider: &FakeProvider, idle_timeout: &str, max_retries: u16) -> OpenAIBackend {
    let config = llm_config(&provider.base_url(), idle_timeout, max_retries);
    OpenAIBackend::with_backoff_base(&config, TEST_BACKOFF).expect("the backend builds")
}

use std::io;
use std::sync::{Arc, Mutex};

/// Captures log output written on the current thread's subscriber.
#[derive(Clone, Default)]
pub struct LogBuffer(Arc<Mutex<Vec<u8>>>);

impl io::Write for LogBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .expect("log buffer mutex")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogBuffer {
    type Writer = LogBuffer;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

impl LogBuffer {
    /// Everything captured so far, one entry per line.
    pub fn lines(&self) -> Vec<String> {
        String::from_utf8_lossy(&self.0.lock().expect("log buffer mutex"))
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// How many captured lines mention `needle`, ignoring case and every non-alphanumeric
    /// character, so `KeyRejected`, `key_rejected` and `key rejected` all match `keyrejected`.
    pub fn count_mentioning(&self, needle: &str) -> usize {
        let needle = normalize(needle);
        self.lines()
            .iter()
            .filter(|line| normalize(line).contains(&needle))
            .count()
    }
}

/// Lowercases and drops everything that is not a letter or digit.
pub fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Starts capturing logs of the current thread at every level. Keep the guard alive for the test;
/// the tests using it run on tokio's default current-thread runtime, so spawned tasks log on the
/// same thread.
pub fn capture_logs() -> (LogBuffer, tracing::subscriber::DefaultGuard) {
    keep_tracing_interest_global();
    let logs = LogBuffer::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .finish();
    (logs, tracing::subscriber::set_default(subscriber))
}

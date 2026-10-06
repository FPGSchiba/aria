//! Scripted backend for tests, behind the `test-util` feature.
//! Replays configured chunks, can fail at a configured point, and records
//! what it was called with and how many times.

use crate::backend::{Backend, BackendError, Chunk};
use crate::conversation::ConversationPart;
use futures_core::Stream;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

/// One step of a script: a chunk to emit or an error that ends the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptStep {
    /// Emit this chunk.
    Chunk(Chunk),
    /// Emit an error with this message; nothing after it is emitted.
    Error(String),
}

/// What the backend was called with on one call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedConversation {
    /// The prior conversation passed in.
    pub history: Vec<ConversationPart>,
    /// The new input passed in.
    pub input: Vec<ConversationPart>,
}

/// A backend that replays a fixed script and records every call. Clones share the recording.
#[derive(Debug, Clone)]
pub struct ScriptedBackend {
    script: Vec<ScriptStep>,
    calls: Arc<Mutex<Vec<ScriptedConversation>>>,
}

impl Backend for ScriptedBackend {
    fn name(&self) -> &'static str {
        "scripted"
    }

    fn stream_conversation(
        &self,
        history: Vec<ConversationPart>,
        input: Vec<ConversationPart>,
    ) -> Pin<Box<dyn Stream<Item = Result<Chunk, BackendError>> + Send + 'static>> {
        let calls = self.calls.clone();
        let script = self.script.clone();
        let conversation = ScriptedConversation { history, input };

        // Record the call
        calls.lock().unwrap().push(conversation);

        // An error is final: emit steps up to and including the first one, nothing after.
        let mut items = Vec::new();
        for step in script {
            match step {
                ScriptStep::Chunk(chunk) => items.push(Ok(chunk)),
                ScriptStep::Error(message) => {
                    items.push(Err(BackendError::unexpected(message)));
                    break;
                }
            }
        }
        Box::pin(tokio_stream::iter(items))
    }
}

impl ScriptedBackend {
    /// Creates a backend that replays `script` on every call.
    pub fn new(script: Vec<ScriptStep>) -> Self {
        Self {
            script,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// The recorded calls in order; the call count is its length.
    pub fn calls(&self) -> Vec<ScriptedConversation> {
        self.calls.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::{Actor, Content};
    use tokio_stream::StreamExt;

    fn text(s: &str) -> Chunk {
        Chunk::Text {
            text: s.to_string(),
        }
    }

    fn chunk(s: &str) -> ScriptStep {
        ScriptStep::Chunk(text(s))
    }

    fn part(actor: Actor, s: &str) -> ConversationPart {
        ConversationPart {
            content: Content::Text {
                text: s.to_string(),
            },
            actor,
        }
    }

    /// Drains one call, reducing each error to its cause's message (errors are not `Eq`).
    async fn drain(backend: &ScriptedBackend) -> Vec<Result<Chunk, String>> {
        backend
            .stream_conversation(Vec::new(), vec![part(Actor::User, "hi")])
            .map(|item| {
                item.map_err(|e| match e {
                    BackendError::UnexpectedError(cause) => cause.to_string(),
                })
            })
            .collect()
            .await
    }

    #[tokio::test]
    async fn yields_configured_chunks_in_order_then_ends_cleanly() {
        let backend = ScriptedBackend::new(vec![chunk("a"), chunk("b"), chunk("c")]);

        let items = drain(&backend).await;

        assert_eq!(items, vec![Ok(text("a")), Ok(text("b")), Ok(text("c"))]);
    }

    #[tokio::test]
    async fn fails_after_n_chunks_yields_n_chunks_then_error() {
        let backend = ScriptedBackend::new(vec![
            chunk("a"),
            chunk("b"),
            ScriptStep::Error("boom".to_string()),
        ]);

        let items = drain(&backend).await;

        assert_eq!(
            items,
            vec![Ok(text("a")), Ok(text("b")), Err("boom".to_string())]
        );
    }

    #[tokio::test]
    async fn failing_immediately_yields_only_the_error() {
        let backend = ScriptedBackend::new(vec![ScriptStep::Error("boom".to_string())]);

        let items = drain(&backend).await;

        assert_eq!(items, vec![Err("boom".to_string())]);
    }

    #[tokio::test]
    async fn nothing_is_emitted_after_an_error() {
        let backend =
            ScriptedBackend::new(vec![ScriptStep::Error("boom".to_string()), chunk("late")]);

        let items = drain(&backend).await;

        assert_eq!(items, vec![Err("boom".to_string())]);
    }

    #[tokio::test]
    async fn records_history_and_input_received_on_each_call() {
        let backend = ScriptedBackend::new(vec![chunk("a")]);
        let history = vec![part(Actor::User, "old"), part(Actor::Assistant, "older")];
        let input = vec![part(Actor::User, "new")];

        let _ = backend
            .stream_conversation(history.clone(), input.clone())
            .collect::<Vec<_>>()
            .await;
        let _ = backend
            .stream_conversation(Vec::new(), vec![part(Actor::User, "second")])
            .collect::<Vec<_>>()
            .await;

        let calls = backend.calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], ScriptedConversation { history, input });
        assert_eq!(calls[1].input, vec![part(Actor::User, "second")]);
    }

    #[tokio::test]
    async fn call_count_is_zero_before_any_call() {
        let backend = ScriptedBackend::new(vec![chunk("a")]);

        assert!(backend.calls().is_empty());
    }

    #[tokio::test]
    async fn clones_share_the_recording() {
        let backend = ScriptedBackend::new(vec![chunk("a")]);
        let clone = backend.clone();

        let _ = drain(&clone).await;

        assert_eq!(backend.calls().len(), 1);
    }
}

//! Scripted backend for tests, behind the `test-util` feature.
//! Replays configured chunks, can fail at a configured point, and records
//! what it was called with and how many times.

use crate::backend::{Backend, BackendError, Chunk};
use crate::history::ConversationPart;
use futures_core::Stream;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

/// One step of a script: a chunk to emit or an error that ends the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptStep {
    /// Emit this chunk.
    Chunk(Chunk),
    /// Emit this error; nothing after it is emitted.
    Error(BackendError),
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
                ScriptStep::Error(err) => {
                    items.push(Err(err));
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

impl Default for ScriptedBackend {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

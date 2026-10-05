//! LLM backend contract.
//! Defines what the Agent Core asks of any backend: prior exchanges plus new user
//! text in, a stream of text chunks out, with a clean end distinguishable from an error.

use crate::history::ConversationPart;
use futures_core::Stream;
use std::pin::Pin;

pub mod echo;
#[cfg(feature = "test-util")]
pub mod scripted;

/// One item of a backend's reply stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chunk {
    /// A chunk of text from the backend.
    Text { text: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
/// A backend failure. It ends the turn; its text is for logs and telemetry, not the client.
pub enum BackendError {
    /// The backend failed in a way not covered by a more specific variant.
    #[error("an unexpected error occurred in the backend")]
    UnexpectedError,
}

/// An LLM backend the Agent Core streams a reply from. One shared instance serves all
/// turns, so implementations hold no per-conversation state.
pub trait Backend: Send + Sync {
    /// Streams a conversation to the backend, returning a stream of chunks.
    ///
    /// # Arguments
    /// * `history` - The history of the conversation.
    /// * `input` - The new user input.
    /// # Returns
    /// A stream of chunks.
    ///
    /// # Contract for implementors
    /// * The returned stream is lazy: no work happens until it is first polled, and
    ///   dropping it cancels the in-flight work.
    /// * The stream ending (`None`) means the reply is complete. A truncated or
    ///   interrupted reply must be reported as an `Err` item, never as a clean end.
    /// * An `Err` item is final: nothing may follow it.
    /// * Do not retry: a failed call fails the turn.
    fn stream_conversation(
        &self,
        history: Vec<ConversationPart>,
        input: Vec<ConversationPart>,
    ) -> Pin<Box<dyn Stream<Item = Result<Chunk, BackendError>> + Send + 'static>>;
}

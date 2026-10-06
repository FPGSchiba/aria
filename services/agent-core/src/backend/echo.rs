//! Trivial echo backend the binary serves with.
//! Streams the user's text back in more than one chunk and ends cleanly.
//! Not a product backend.

use crate::backend::{Backend, BackendError, Chunk};
use crate::conversation::{Content, ConversationPart};
use crate::health::BackendHealth;
use futures_core::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::watch;

/// Echoes the user's text back in at least two chunks (for text of two or more chars), ignores
/// history, and ends cleanly. The chunks are produced when the stream is first polled.
#[derive(Debug, Clone)]
pub struct EchoBackend {
    /// The sender of this backend's health; it starts healthy and never changes.
    health: Arc<watch::Sender<BackendHealth>>,
}

impl EchoBackend {
    /// The name this backend goes by, in the startup log and in health lines.
    const NAME: &'static str = "echo";

    /// Creates an echo backend, always healthy.
    pub fn new() -> Self {
        Self {
            health: Arc::new(watch::channel(BackendHealth::new(Self::NAME, true, false)).0),
        }
    }
}

impl Default for EchoBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for EchoBackend {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn subscribe_health(&self) -> watch::Receiver<BackendHealth> {
        self.health.subscribe()
    }

    fn stream_conversation(
        &self,
        _history: Vec<ConversationPart>,
        input: Vec<ConversationPart>,
    ) -> Pin<Box<dyn Stream<Item = Result<Chunk, BackendError>> + Send + 'static>> {
        Box::pin(async_stream::stream! {
            // Split each text part by chars into pieces of ceil(n / 2), at most 8, so any text of
            // two or more chars streams as at least two chunks and the pieces concatenate exactly.
            for part in input {
                match part.content {
                    Content::Text { text } => {
                        let chars: Vec<char> = text.chars().collect();
                        let size = chars.len().div_ceil(2).clamp(1, 8);
                        for piece in chars.chunks(size) {
                            yield Ok(Chunk::Text { text: piece.iter().collect() });
                        }
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::Actor;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn echoes_input_in_at_least_two_chunks_that_concatenate_to_the_input() {
        let input = "hello from the echo backend";
        let items: Vec<_> = EchoBackend::new()
            .stream_conversation(
                Vec::new(),
                vec![ConversationPart {
                    content: Content::Text {
                        text: input.to_string(),
                    },
                    actor: Actor::User,
                }],
            )
            .collect()
            .await;

        assert!(
            items.len() >= 2,
            "expected at least two chunks, got {items:?}"
        );
        let mut joined = String::new();
        for item in items {
            let Chunk::Text { text } = item.expect("echo never errors");
            joined.push_str(&text);
        }
        assert_eq!(joined, input);
    }
}

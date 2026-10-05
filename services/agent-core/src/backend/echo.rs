//! Trivial echo backend the binary serves with.
//! Streams the user's text back in more than one chunk and ends cleanly.
//! Not a product backend.

use crate::backend::{Backend, BackendError, Chunk};
use crate::history::{Content, ConversationPart};
use futures_core::Stream;
use std::pin::Pin;

/// Echoes the user's text back in at least two chunks (for text of two or more chars), ignores
/// history, and ends cleanly.
#[derive(Debug, Clone, Default)]
pub struct EchoBackend;

impl Backend for EchoBackend {
    fn stream_conversation(
        &self,
        _history: Vec<ConversationPart>,
        input: Vec<ConversationPart>,
    ) -> Pin<Box<dyn Stream<Item = Result<Chunk, BackendError>> + Send + 'static>> {
        // Split each text part by chars into pieces of ceil(n / 2), at most 8, so any text of
        // two or more chars streams as at least two chunks and the pieces concatenate exactly.
        let mut chunks = Vec::new();
        for part in input {
            match part.content {
                Content::Text { text } => {
                    let chars: Vec<char> = text.chars().collect();
                    let size = chars.len().div_ceil(2).clamp(1, 8);
                    for piece in chars.chunks(size) {
                        chunks.push(Ok(Chunk::Text {
                            text: piece.iter().collect(),
                        }));
                    }
                }
            }
        }
        Box::pin(tokio_stream::iter(chunks))
    }
}

//! The conversation model: who said what, as a list of authored parts.
//! Shared by the history store, the backends and the service.

/// Who produced a part of the conversation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Actor {
    /// The end user.
    User,
    /// The agent's reply.
    Assistant,
    /// A system instruction.
    System,
}

/// The content of a conversation part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// Plain text.
    Text { text: String },
}

/// One part of a conversation: who said it and what was said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationPart {
    /// The content of this part (text today, extensible).
    pub content: Content,
    /// Whether this part is from the user, the assistant or the system.
    pub actor: Actor,
}

//! Maps ARIA's conversation onto the provider's streaming request: roles, the current input's
//! system part, no replayed system entries, no tools, and a usage summary.

use crate::conversation::ConversationPart;
use async_openai::types::chat::{
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestAssistantMessageContent,
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessage,
    ChatCompletionRequestUserMessageContent, ChatCompletionStreamOptions,
    CreateChatCompletionRequest,
};

/// Maps ARIA's conversation onto the provider's streaming request: roles, the current input's
/// system part, no replayed system entries, no tools, and a usage summary.
fn map_conversation_to_request(
    history: &[ConversationPart],
    input: &[ConversationPart],
    model_name: &str,
) -> CreateChatCompletionRequest {
    let mut messages: Vec<ChatCompletionRequestMessage> = input.iter().map(|p| p.into()).collect();
    messages.extend(history.iter().map(|p| p.into()));

    CreateChatCompletionRequest {
        model: model_name.to_string(),
        messages,
        stream: Some(true),
        stream_options: Some(ChatCompletionStreamOptions {
            include_usage: Some(true),
            include_obfuscation: Some(false),
        }),
        ..Default::default()
    }
}

impl From<&ConversationPart> for ChatCompletionRequestMessage {
    fn from(part: &ConversationPart) -> Self {
        match part.actor {
            crate::conversation::Actor::User => {
                ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                    content: match &part.content {
                        crate::conversation::Content::Text { text } => {
                            ChatCompletionRequestUserMessageContent::Text(text.clone())
                        }
                    },
                    name: None,
                })
            }
            crate::conversation::Actor::System => {
                ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
                    content: match &part.content {
                        crate::conversation::Content::Text { text } => {
                            ChatCompletionRequestSystemMessageContent::Text(text.clone())
                        }
                    },
                    name: None,
                })
            }
            crate::conversation::Actor::Assistant => {
                ChatCompletionRequestMessage::Assistant(ChatCompletionRequestAssistantMessage {
                    content: match &part.content {
                        crate::conversation::Content::Text { text } => Some(
                            ChatCompletionRequestAssistantMessageContent::Text(text.clone()),
                        ),
                    },
                    ..Default::default()
                })
            }
        }
    }
}

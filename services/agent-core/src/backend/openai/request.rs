//! Maps ARIA's conversation onto the provider's streaming request: roles, the current input's
//! system part, no replayed system entries, no tools, and a usage summary.

use crate::conversation::{Actor, ConversationPart};
use async_openai::types::chat::{
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestAssistantMessageContent,
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessage,
    ChatCompletionRequestUserMessageContent, ChatCompletionStreamOptions,
    CreateChatCompletionRequest,
};

/// Maps ARIA's conversation onto the provider's streaming request: roles, the current input's
/// system part, no replayed system entries, no tools, and a usage summary.
pub(crate) fn map_conversation_to_request(
    history: &[ConversationPart],
    input: &[ConversationPart],
    model_name: &str,
) -> CreateChatCompletionRequest {
    let mut messages: Vec<ChatCompletionRequestMessage> = history
        .iter()
        .filter(|p| p.actor != Actor::System)
        .map(|p| p.into())
        .collect();
    messages.extend(input.iter().map(|p| p.into()));

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::{Actor, Content};

    fn part(actor: Actor, text: &str) -> ConversationPart {
        ConversationPart {
            content: Content::Text {
                text: text.to_string(),
            },
            actor,
        }
    }

    /// The request's messages as `(role, text)` pairs, so the order and roles read at a glance.
    fn roles_and_texts(request: &CreateChatCompletionRequest) -> Vec<(&'static str, String)> {
        request
            .messages
            .iter()
            .map(|message| match message {
                ChatCompletionRequestMessage::User(m) => match &m.content {
                    ChatCompletionRequestUserMessageContent::Text(t) => ("user", t.clone()),
                    other => panic!("unexpected user content {other:?}"),
                },
                ChatCompletionRequestMessage::Assistant(m) => match &m.content {
                    Some(ChatCompletionRequestAssistantMessageContent::Text(t)) => {
                        ("assistant", t.clone())
                    }
                    other => panic!("unexpected assistant content {other:?}"),
                },
                ChatCompletionRequestMessage::System(m) => match &m.content {
                    ChatCompletionRequestSystemMessageContent::Text(t) => ("system", t.clone()),
                    other => panic!("unexpected system content {other:?}"),
                },
                other => panic!("unexpected message {other:?}"),
            })
            .collect()
    }

    fn pair(role: &'static str, text: &str) -> (&'static str, String) {
        (role, text.to_string())
    }

    #[test]
    fn user_and_assistant_parts_keep_their_roles() {
        let history = vec![
            part(Actor::User, "question"),
            part(Actor::Assistant, "answer"),
        ];
        let input = vec![part(Actor::User, "follow-up")];

        let request = map_conversation_to_request(&history, &input, "m");

        assert_eq!(
            roles_and_texts(&request),
            vec![
                pair("user", "question"),
                pair("assistant", "answer"),
                pair("user", "follow-up"),
            ]
        );
    }

    #[test]
    fn history_comes_before_the_input() {
        let history = vec![part(Actor::User, "old"), part(Actor::Assistant, "older")];
        let input = vec![part(Actor::User, "new")];

        let request = map_conversation_to_request(&history, &input, "m");

        let texts: Vec<String> = roles_and_texts(&request)
            .into_iter()
            .map(|(_, text)| text)
            .collect();
        assert_eq!(texts, vec!["old", "older", "new"]);
    }

    #[test]
    fn the_inputs_own_order_is_kept() {
        let input = vec![part(Actor::System, "be brief"), part(Actor::User, "hi")];

        let request = map_conversation_to_request(&[], &input, "m");

        assert_eq!(
            roles_and_texts(&request),
            vec![pair("system", "be brief"), pair("user", "hi")]
        );
    }

    #[test]
    fn a_system_part_in_the_input_is_sent_as_system() {
        let input = vec![part(Actor::System, "current instruction")];

        let request = map_conversation_to_request(&[], &input, "m");

        assert_eq!(
            roles_and_texts(&request),
            vec![pair("system", "current instruction")]
        );
    }

    #[test]
    fn stored_system_parts_in_the_history_are_not_replayed() {
        let history = vec![
            part(Actor::System, "stored instruction"),
            part(Actor::User, "hello"),
            part(Actor::System, "another stored one"),
            part(Actor::Assistant, "hi"),
        ];
        let input = vec![part(Actor::User, "next")];

        let request = map_conversation_to_request(&history, &input, "m");

        assert_eq!(
            roles_and_texts(&request),
            vec![
                pair("user", "hello"),
                pair("assistant", "hi"),
                pair("user", "next"),
            ]
        );
    }

    #[test]
    fn the_configured_model_is_requested() {
        let request =
            map_conversation_to_request(&[], &[part(Actor::User, "hi")], "configured-model");

        assert_eq!(request.model, "configured-model");
    }

    #[test]
    fn the_request_streams_and_asks_for_a_usage_summary() {
        let request = map_conversation_to_request(&[], &[part(Actor::User, "hi")], "m");

        assert_eq!(request.stream, Some(true));
        let options = request.stream_options.expect("stream options are set");
        assert_eq!(options.include_usage, Some(true));
    }

    #[test]
    fn no_tools_are_sent() {
        let request = map_conversation_to_request(&[], &[part(Actor::User, "hi")], "m");

        assert!(request.tools.is_none(), "{:?}", request.tools);
    }

    #[test]
    fn an_empty_conversation_maps_to_no_messages() {
        let request = map_conversation_to_request(&[], &[], "m");

        assert!(request.messages.is_empty());
    }
}

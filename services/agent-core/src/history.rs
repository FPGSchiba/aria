//! Session history store.
//! Owns every session's conversation so far and whether a turn is in flight on it.
//! Decides whether a turn may start and what a finished turn adds.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

#[derive(Clone, Debug)]
struct RequestCounter {
    next: Arc<AtomicU64>,
}

impl RequestCounter {
    fn next_id(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }
}

/// Why a history store operation failed.
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    /// Another turn is already in flight on the session.
    #[error("turn already in flight")]
    TurnInFlight,
    /// The store could not be reached or used.
    #[error("store not available")]
    StoreUnavailable,
    /// The turn does not hold the session it was handed back for.
    #[error("owner of the turn is not the same as the token")]
    InvalidTurnOwner,
}

/// Result of a history store operation.
pub type HistoryResult<T> = Result<T, HistoryError>;

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

#[derive(Debug, Clone)]
struct SessionHistory {
    /// The conversation history for this session.
    conversation: Vec<ConversationPart>,
    /// Who currently holds the session in turn. None if no turn is in flight.
    turn_holder: Option<u64>,
}

impl SessionHistory {
    /// Clears the holder if it is this token. Works on locked data: std `Mutex` is not re-entrant.
    fn release_if_held(&mut self, token_id: u64) {
        if self.turn_holder == Some(token_id) {
            self.turn_holder = None;
        }
    }
}

/// The in-memory history store. Cloning shares the same sessions.
#[derive(Debug, Clone)]
pub struct History {
    /// The session history store.
    sessions: Arc<Mutex<HashMap<String, SessionHistory>>>,
    /// A counter for generating unique token IDs.
    request_counter: RequestCounter,
}

/// The in-memory store's turn handle. Dropping it releases the session if neither commit nor
/// abort ran.
#[must_use = "dropping the token immediately frees the session; hold it for the whole turn"]
#[derive(Debug)]
pub struct Token {
    store: History,
    session_id: String,
    token_id: u64,
}

impl Default for History {
    fn default() -> Self {
        Self {
            request_counter: RequestCounter {
                next: Arc::new(AtomicU64::new(1)),
            },
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

/// A history store: one turn per session at a time; commit appends atomically; abort and drop
/// leave history unchanged.
///
/// To be served, a store must be a cheaply cloneable handle (`Clone`), because a clone moves
/// into the response stream.
pub trait HistoryStore: Send + Sync {
    /// The store's receipt for one in-flight turn on a session. It is obtained only from
    /// `start_turn` and must be handed back to exactly one of `commit` or `abort`, both of
    /// which consume it. While it exists the session refuses new turns (busy). Dropping it
    /// without commit or abort (error, cancelled stream, panic) frees the session eventually
    /// without changing history; the mechanism is the store's choice (the in-memory store
    /// releases synchronously in `Drop`; a persisted store may use a spawned release or a
    /// lease). Bounded `Send + 'static` because it lives inside the response stream.
    type Turn: Send + 'static;

    /// Starts a turn on the session and returns it with an owned snapshot of the prior
    /// conversation (empty for an unseen session).
    ///
    /// The turn must be held for the whole turn: dropping it frees the session. Callers generic
    /// over the store get no `#[must_use]` lint, so this is a documented obligation.
    ///
    /// # Errors
    /// `TurnInFlight` if the session is busy, `StoreUnavailable` if the store cannot be used.
    fn start_turn(
        &self,
        session_id: &str,
    ) -> impl Future<Output = HistoryResult<(Self::Turn, Vec<ConversationPart>)>> + Send;

    /// Consumes the turn, appends `new_parts` to the session atomically and frees the session.
    ///
    /// On any error the turn is still consumed and the session freed.
    ///
    /// # Errors
    /// `StoreUnavailable` if the store cannot be used, `InvalidTurnOwner` if the turn does not
    /// hold the session or was issued by another store.
    fn commit(
        &self,
        token: Self::Turn,
        new_parts: Vec<ConversationPart>,
    ) -> impl Future<Output = HistoryResult<()>> + Send;

    /// Consumes the turn, leaves history unchanged and frees the session.
    ///
    /// On any error the turn is still consumed and the session freed.
    ///
    /// # Errors
    /// `StoreUnavailable` if the store cannot be used, `InvalidTurnOwner` if the turn was
    /// issued by another store.
    fn abort(&self, token: Self::Turn) -> impl Future<Output = HistoryResult<()>> + Send;
}

impl HistoryStore for History {
    type Turn = Token;

    async fn start_turn(&self, session_id: &str) -> HistoryResult<(Token, Vec<ConversationPart>)> {
        let mut sessions = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
        let token_id = self.request_counter.next_id();
        let session_history = sessions
            .entry(session_id.to_string())
            .or_insert(SessionHistory {
                conversation: Vec::new(),
                turn_holder: None,
            });

        if session_history.turn_holder.is_some() {
            return Err(HistoryError::TurnInFlight);
        }

        session_history.turn_holder = Some(token_id);
        let token = Token {
            store: self.clone(),
            session_id: session_id.to_string(),
            token_id,
        };
        Ok((token, session_history.conversation.clone()))
    }

    async fn commit(
        &self,
        token: Self::Turn,
        new_parts: Vec<ConversationPart>,
    ) -> HistoryResult<()> {
        if !Arc::ptr_eq(&self.sessions, &token.store.sessions) {
            tracing::error!(
                session_id = %token.session_id,
                token_id = token.token_id,
                "commit discarded: the turn was issued by a different store"
            );
            return Err(HistoryError::InvalidTurnOwner);
        }
        let mut sessions = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
        let held = sessions
            .get_mut(&token.session_id)
            .filter(|session_history| session_history.turn_holder == Some(token.token_id));
        match held {
            Some(session_history) => {
                session_history.conversation.extend(new_parts);
                session_history.release_if_held(token.token_id);
                Ok(())
            }
            None => {
                tracing::error!(
                    session_id = %token.session_id,
                    token_id = token.token_id,
                    "commit discarded: the turn no longer holds the session"
                );
                Err(HistoryError::InvalidTurnOwner)
            }
        }
    }

    async fn abort(&self, token: Self::Turn) -> HistoryResult<()> {
        if !Arc::ptr_eq(&self.sessions, &token.store.sessions) {
            return Err(HistoryError::InvalidTurnOwner);
        }
        token.release_turn();
        Ok(())
    }
}

impl Drop for Token {
    fn drop(&mut self) {
        self.release_turn()
    }
}

impl Token {
    /// Releases the turn for this token, allowing another turn to start.
    fn release_turn(&self) {
        let mut sessions = self
            .store
            .sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(session_history) = sessions.get_mut(&self.session_id) {
            session_history.release_if_held(self.token_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exchange(user: &str, assistant: &str) -> Vec<ConversationPart> {
        vec![
            ConversationPart {
                content: Content::Text {
                    text: user.to_string(),
                },
                actor: Actor::User,
            },
            ConversationPart {
                content: Content::Text {
                    text: assistant.to_string(),
                },
                actor: Actor::Assistant,
            },
        ]
    }

    async fn commit_exchange<S: HistoryStore>(store: &S, session: &str, user: &str, reply: &str) {
        let (turn, _) = store.start_turn(session).await.expect("start_turn");
        store
            .commit(turn, exchange(user, reply))
            .await
            .expect("commit");
    }

    async fn history_of<S: HistoryStore>(store: &S, session: &str) -> Vec<ConversationPart> {
        let (turn, history) = store.start_turn(session).await.expect("start_turn");
        store.abort(turn).await.expect("abort");
        history
    }

    #[tokio::test]
    async fn new_session_has_empty_history() {
        let store = History::default();
        let (_turn, history) = store.start_turn("s1").await.expect("start_turn");
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn committed_turn_is_visible_to_next_turn() {
        let store = History::default();
        commit_exchange(&store, "s1", "hello", "hi there").await;

        assert_eq!(
            history_of(&store, "s1").await,
            exchange("hello", "hi there")
        );
    }

    #[tokio::test]
    async fn two_committed_turns_are_visible_oldest_first() {
        let store = History::default();
        commit_exchange(&store, "s1", "first", "one").await;
        commit_exchange(&store, "s1", "second", "two").await;

        let mut expected = exchange("first", "one");
        expected.extend(exchange("second", "two"));
        assert_eq!(history_of(&store, "s1").await, expected);
    }

    #[tokio::test]
    async fn aborted_turn_leaves_history_unchanged_and_frees_session() {
        let store = History::default();
        commit_exchange(&store, "s1", "first", "one").await;

        let (turn, _) = store.start_turn("s1").await.expect("start_turn");
        store.abort(turn).await.expect("abort");

        let (turn, history) = store
            .start_turn("s1")
            .await
            .expect("session is free after abort");
        assert_eq!(history, exchange("first", "one"));
        store.abort(turn).await.expect("abort");
    }

    #[tokio::test]
    async fn dropped_turn_frees_session_and_leaves_history_unchanged() {
        let store = History::default();
        commit_exchange(&store, "s1", "first", "one").await;

        let (turn, _) = store.start_turn("s1").await.expect("start_turn");
        drop(turn);

        let (_turn, history) = store
            .start_turn("s1")
            .await
            .expect("session is free after drop");
        assert_eq!(history, exchange("first", "one"));
    }

    #[tokio::test]
    async fn second_turn_on_busy_session_is_refused_as_turn_in_flight() {
        let store = History::default();
        let (_turn, _) = store.start_turn("s1").await.expect("start_turn");

        let second = store.start_turn("s1").await;

        assert!(
            matches!(second, Err(HistoryError::TurnInFlight)),
            "expected TurnInFlight, got {:?}",
            second.map(|(_, history)| history)
        );
    }

    #[tokio::test]
    async fn busy_session_does_not_block_a_different_session() {
        let store = History::default();
        let (_busy, _) = store.start_turn("s1").await.expect("start_turn");

        let other = store.start_turn("s2").await;

        assert!(other.is_ok(), "a different session must not be blocked");
    }

    #[tokio::test]
    async fn commit_on_one_session_does_not_change_another() {
        let store = History::default();
        commit_exchange(&store, "s1", "first", "one").await;

        assert!(history_of(&store, "s2").await.is_empty());
    }

    #[tokio::test]
    async fn committing_a_turn_from_another_store_is_refused_as_invalid_turn_owner() {
        let store_a = History::default();
        let store_b = History::default();
        let (foreign_turn, _) = store_a.start_turn("s1").await.expect("start_turn");

        let result = store_b.commit(foreign_turn, exchange("x", "y")).await;

        assert!(
            matches!(result, Err(HistoryError::InvalidTurnOwner)),
            "expected InvalidTurnOwner, got {result:?}"
        );
    }

    #[tokio::test]
    async fn aborting_a_turn_from_another_store_is_refused_as_invalid_turn_owner() {
        let store_a = History::default();
        let store_b = History::default();
        let (foreign_turn, _) = store_a.start_turn("s1").await.expect("start_turn");

        let result = store_b.abort(foreign_turn).await;

        assert!(
            matches!(result, Err(HistoryError::InvalidTurnOwner)),
            "expected InvalidTurnOwner, got {result:?}"
        );
    }
}

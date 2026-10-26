//! The one test that calls the real provider: the first chunk of a long reply must arrive
//! before the stream ends, so streaming is real and not a buffered reply cut into pieces.
//!
//! It is `#[ignore]`d, so the CI gate skips it. Run it with the key set:
//!
//! ```text
//! ARIA_LLM_API_KEY=<key> cargo test -p agent-core --features test-util \
//!     --test openai_live -- --ignored --nocapture
//! ```
//!
//! `ARIA_LLM_BASE_URL`, `ARIA_LLM_MODEL`, `ARIA_LLM_IDLE_TIMEOUT` and `ARIA_LLM_MAX_RETRIES` apply
//! as for the binary. Without `ARIA_LLM_API_KEY` the test fails, it does not skip, so a green
//! result always means it really ran.

use agent_core::backend::openai::OpenAIBackend;
use agent_core::backend::{Backend, Chunk};
use agent_core::config::Config;
use agent_core::conversation::{Actor, Content, ConversationPart};
use std::time::{Duration, Instant};
use tokio_stream::StreamExt;

/// Upper bound for the whole reply, so a hung provider fails the test instead of blocking it.
const LIMIT: Duration = Duration::from_secs(120);

/// How far apart the first chunk and the end of the stream must be, so "before" is not an
/// accident of timer resolution.
const MIN_GAP: Duration = Duration::from_millis(20);

#[tokio::test]
#[ignore = "calls the real provider; run with ARIA_LLM_API_KEY set and --ignored"]
async fn the_first_chunk_arrives_strictly_before_the_end_of_a_long_reply() {
    let config = Config::from_env().unwrap_or_else(|error| {
        panic!("the live test needs a valid configuration, at least ARIA_LLM_API_KEY: {error}")
    });
    let backend = OpenAIBackend::new(&config).expect("the backend builds");
    let input = vec![ConversationPart {
        content: Content::Text {
            text: "Write a story of about six hundred words about a lighthouse keeper who \
                   finds a message in a bottle. Use plain prose, no headings."
                .to_string(),
        },
        actor: Actor::User,
    }];

    let mut stream = backend.stream_conversation(Vec::new(), input);
    let mut first_chunk_at = None;
    let mut chunks = 0usize;
    let mut text = String::new();
    let ended_at = tokio::time::timeout(LIMIT, async {
        while let Some(item) = stream.next().await {
            match item.expect("the reply streams without an error") {
                Chunk::Text { text: delta } => {
                    first_chunk_at.get_or_insert_with(Instant::now);
                    chunks += 1;
                    text.push_str(&delta);
                }
            }
        }
        Instant::now()
    })
    .await
    .expect("the reply did not finish in time");

    let first_chunk_at = first_chunk_at.expect("the reply produced no text at all");
    assert!(
        chunks > 1,
        "a long reply arrived as {chunks} chunk(s): {text:?}"
    );
    assert!(
        first_chunk_at < ended_at,
        "the first chunk did not arrive before the stream ended"
    );
    assert!(
        ended_at.duration_since(first_chunk_at) >= MIN_GAP,
        "first chunk and end are only {:?} apart, so the reply was probably buffered",
        ended_at.duration_since(first_chunk_at)
    );
}

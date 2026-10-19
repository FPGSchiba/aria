//! Test-only fake provider: a controllable SSE server that scripts events, delays, stalls and
//! error statuses, and records every request it receives.
//!
//! Start one with [`FakeProvider::start`], queue one [`Script`] per expected request, point the
//! backend at [`FakeProvider::base_url`], then read back what arrived with
//! [`FakeProvider::requests`]. The server stops when the handle is dropped.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use axum::routing::post;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;

/// The model name carried by every chunk the helpers build.
pub const FAKE_MODEL: &str = "fake-model";

const CHUNK_ID: &str = "chatcmpl-fake";
const CHUNK_CREATED: u64 = 1_700_000_000;
const MUTEX_POISONED: &str = "fake provider state mutex poisoned";

/// One event of a scripted SSE stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A `data: <payload>` event; the payload is sent verbatim (normally a JSON string).
    Data(String),
    /// The explicit `data: [DONE]` terminator.
    Done,
    /// An SSE comment line (`: <text>`), the usual keep-alive.
    Comment(String),
}

impl Event {
    fn to_wire(&self) -> String {
        match self {
            Event::Data(payload) => format!("data: {payload}\n\n"),
            Event::Done => "data: [DONE]\n\n".to_string(),
            Event::Comment(text) => format!(": {text}\n\n"),
        }
    }
}

/// An [`Event`] and the delay before it is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// How long to wait, after the previous step, before sending the event.
    pub delay: Duration,
    /// The event to send.
    pub event: Event,
}

impl Step {
    /// A data event sent immediately.
    pub fn data(payload: impl Into<String>) -> Self {
        Self::now(Event::Data(payload.into()))
    }

    /// The `[DONE]` terminator, sent immediately.
    pub fn done() -> Self {
        Self::now(Event::Done)
    }

    /// A keep-alive comment, sent immediately.
    pub fn comment(text: impl Into<String>) -> Self {
        Self::now(Event::Comment(text.into()))
    }

    /// The same step, but sent `delay` after the previous one.
    pub fn after(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    fn now(event: Event) -> Self {
        Self {
            delay: Duration::ZERO,
            event,
        }
    }
}

/// What the server does once every step of a stream has been sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ending {
    /// End the response body. Without a scripted [`Event::Done`] this is a stream that closes
    /// without `[DONE]`.
    #[default]
    Close,
    /// Keep the connection open and send nothing more, until the client hangs up.
    Stall,
}

/// A scripted SSE response: steps in order, then an [`Ending`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamScript {
    /// The events, each with its delay.
    pub steps: Vec<Step>,
    /// What happens after the last step.
    pub ending: Ending,
}

impl StreamScript {
    /// A stream of `steps` that closes after the last one.
    pub fn new(steps: Vec<Step>) -> Self {
        Self {
            steps,
            ending: Ending::Close,
        }
    }

    /// The same stream, but stalling forever after the last step instead of closing.
    pub fn stall(mut self) -> Self {
        self.ending = Ending::Stall;
        self
    }
}

/// A scripted error response: a status, an optional `Retry-After` and an optional body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorScript {
    /// The HTTP status to return.
    pub status: StatusCode,
    /// The raw `Retry-After` header value, so tests can send seconds or an HTTP date.
    pub retry_after: Option<String>,
    /// The response body, sent verbatim; see [`flat_error_body`].
    pub body: Option<String>,
}

impl ErrorScript {
    /// An error response with `status`, no `Retry-After` and an empty body.
    pub fn new(status: StatusCode) -> Self {
        Self {
            status,
            retry_after: None,
            body: None,
        }
    }

    /// The same response with a `Retry-After` header carrying `value` verbatim.
    pub fn retry_after(mut self, value: impl Into<String>) -> Self {
        self.retry_after = Some(value.into());
        self
    }

    /// The same response with `body` as its body.
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }
}

/// What the fake provider answers to one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Script {
    /// An error status.
    Error(ErrorScript),
    /// An SSE stream.
    Stream(StreamScript),
}

impl From<ErrorScript> for Script {
    fn from(script: ErrorScript) -> Self {
        Script::Error(script)
    }
}

impl From<StreamScript> for Script {
    fn from(script: StreamScript) -> Self {
        Script::Stream(script)
    }
}

/// One request the fake provider received.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedRequest {
    /// The request headers as `(lowercased name, value)` pairs; non-UTF-8 values are lossy.
    pub headers: Vec<(String, String)>,
    /// The JSON body, or the raw text as a JSON string if it did not parse.
    pub body: Value,
}

impl RecordedRequest {
    /// The first value of the header `name`, matched case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }
}

struct Shared {
    scripts: Mutex<VecDeque<Script>>,
    requests: Mutex<Vec<RecordedRequest>>,
    disconnects: watch::Sender<usize>,
    shutdown: watch::Receiver<()>,
}

impl Shared {
    fn record(&self, headers: &HeaderMap, body: &[u8]) {
        let headers = headers
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_ascii_lowercase(),
                    String::from_utf8_lossy(value.as_bytes()).into_owned(),
                )
            })
            .collect();
        let body = serde_json::from_slice(body)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(body).into_owned()));
        self.requests
            .lock()
            .expect(MUTEX_POISONED)
            .push(RecordedRequest { headers, body });
    }

    fn note_disconnect(&self) {
        self.disconnects.send_modify(|count| *count += 1);
    }
}

/// A running fake provider. Dropping it stops the server and ends any open streams.
pub struct FakeProvider {
    addr: SocketAddr,
    shared: Arc<Shared>,
    server: JoinHandle<()>,
    // Dropped with the handle; open streams see the channel close and stop.
    _shutdown: watch::Sender<()>,
}

impl FakeProvider {
    /// Starts the server on 127.0.0.1 with an OS-assigned port.
    pub async fn start() -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let (shutdown_tx, shutdown_rx) = watch::channel(());
        let shared = Arc::new(Shared {
            scripts: Mutex::new(VecDeque::new()),
            requests: Mutex::new(Vec::new()),
            disconnects: watch::channel(0).0,
            shutdown: shutdown_rx,
        });
        let app = Router::new()
            .route("/v1/chat/completions", post(chat_completions))
            .with_state(Arc::clone(&shared));
        let server = tokio::spawn(async move {
            // The only way this ends is the task being aborted on drop.
            let _ = axum::serve(listener, app).await;
        });
        Ok(Self {
            addr,
            shared,
            server,
            _shutdown: shutdown_tx,
        })
    }

    /// The base URL to configure the backend with, ending in `/v1`.
    pub fn base_url(&self) -> String {
        format!("http://{}/v1", self.addr)
    }

    /// Queues the answer to the next unanswered request. Scripts are consumed in order.
    pub fn enqueue(&self, script: impl Into<Script>) {
        self.shared
            .scripts
            .lock()
            .expect(MUTEX_POISONED)
            .push_back(script.into());
    }

    /// How many requests have arrived.
    pub fn request_count(&self) -> usize {
        self.shared.requests.lock().expect(MUTEX_POISONED).len()
    }

    /// A snapshot of every request received so far, oldest first.
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.shared.requests.lock().expect(MUTEX_POISONED).clone()
    }

    /// How many streams the client hung up on, whether mid-stream or during a stall.
    pub fn disconnect_count(&self) -> usize {
        *self.shared.disconnects.borrow()
    }

    /// Waits until at least one client hang-up has been seen; false if `timeout` passes first.
    pub async fn wait_for_disconnect(&self, timeout: Duration) -> bool {
        let mut count = self.shared.disconnects.subscribe();
        matches!(
            tokio::time::timeout(timeout, count.wait_for(|n| *n >= 1)).await,
            Ok(Ok(_))
        )
    }
}

impl Drop for FakeProvider {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn chat_completions(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    shared.record(&headers, &body);
    let script = shared.scripts.lock().expect(MUTEX_POISONED).pop_front();
    match script {
        None => plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "fake provider: no script queued for this request".to_string(),
        ),
        Some(Script::Error(error)) => error_response(error),
        Some(Script::Stream(stream)) => stream_response(stream, shared),
    }
}

fn plain_response(status: StatusCode, body: String) -> Response {
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    response
}

fn error_response(error: ErrorScript) -> Response {
    let mut response = plain_response(error.status, error.body.unwrap_or_default());
    if let Some(value) = error
        .retry_after
        .as_deref()
        .and_then(|v| HeaderValue::from_str(v).ok())
    {
        response.headers_mut().insert(header::RETRY_AFTER, value);
    }
    response
}

fn stream_response(script: StreamScript, shared: Arc<Shared>) -> Response {
    let (tx, rx) = mpsc::channel::<Result<Bytes, Infallible>>(16);
    tokio::spawn(play(script, tx, shared));
    let mut response = Response::new(Body::from_stream(ReceiverStream::new(rx)));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    response
}

/// Plays a script into the response body, noticing a client hang-up at any point.
async fn play(
    script: StreamScript,
    tx: mpsc::Sender<Result<Bytes, Infallible>>,
    shared: Arc<Shared>,
) {
    let mut shutdown = shared.shutdown.clone();
    for step in script.steps {
        tokio::select! {
            () = tokio::time::sleep(step.delay) => {}
            () = tx.closed() => return shared.note_disconnect(),
            _ = shutdown.changed() => return,
        }
        if tx
            .send(Ok(Bytes::from(step.event.to_wire())))
            .await
            .is_err()
        {
            return shared.note_disconnect();
        }
    }
    if script.ending == Ending::Stall {
        tokio::select! {
            () = tx.closed() => shared.note_disconnect(),
            _ = shutdown.changed() => {}
        }
    }
}

/// Cerebras's flat error body, `{"message","type","param","code"}`, as a JSON string. Unlike
/// OpenAI's wrapped `{"error": {..}}` form, the fields sit at the top level.
pub fn flat_error_body(message: &str, error_type: &str, param: Option<&str>, code: &str) -> String {
    json!({ "message": message, "type": error_type, "param": param, "code": code }).to_string()
}

fn chunk(delta: Value, finish_reason: Value) -> String {
    json!({
        "id": CHUNK_ID,
        "object": "chat.completion.chunk",
        "created": CHUNK_CREATED,
        "model": FAKE_MODEL,
        "choices": [{ "index": 0, "delta": delta, "finish_reason": finish_reason }],
    })
    .to_string()
}

/// The opening chunk, which only announces the assistant role.
pub fn role_chunk() -> String {
    chunk(json!({ "role": "assistant" }), Value::Null)
}

/// A chunk carrying a `reasoning` delta and no content, as reasoning models stream.
pub fn reasoning_chunk(text: &str) -> String {
    chunk(json!({ "reasoning": text }), Value::Null)
}

/// A chunk carrying a text `content` delta.
pub fn text_delta_chunk(text: &str) -> String {
    chunk(json!({ "content": text }), Value::Null)
}

/// A chunk with an empty delta and the given `finish_reason`, such as "stop", "length" or
/// "content_filter".
pub fn finish_chunk(finish_reason: &str) -> String {
    chunk(json!({}), json!(finish_reason))
}

/// A chunk carrying a tool-call delta: the call at `index`, with its id, function name and a
/// fragment of the JSON arguments.
pub fn tool_call_delta_chunk(index: u32, id: &str, name: &str, arguments: &str) -> String {
    chunk(
        json!({ "tool_calls": [{
            "index": index,
            "id": id,
            "type": "function",
            "function": { "name": name, "arguments": arguments },
        }] }),
        Value::Null,
    )
}

/// The final usage-only chunk: empty `choices` and a `usage` object.
pub fn usage_chunk(prompt_tokens: u32, completion_tokens: u32) -> String {
    json!({
        "id": CHUNK_ID,
        "object": "chat.completion.chunk",
        "created": CHUNK_CREATED,
        "model": FAKE_MODEL,
        "choices": [],
        "usage": {
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
            "total_tokens": prompt_tokens + completion_tokens,
        },
    })
    .to_string()
}

/// A happy-path stream: role chunk, two reasoning deltas, three text deltas ("Hello", ", ",
/// "world"), a "stop" chunk, a usage-only chunk, then `[DONE]`.
///
/// Modelled on the OpenAI-compatible streaming format, NOT a captured Cerebras stream (plan Risk:
/// the fake can drift from the real provider). Replace with a capture when one is available.
pub fn default_happy_path() -> StreamScript {
    StreamScript::new(vec![
        Step::data(role_chunk()),
        Step::data(reasoning_chunk("The user greets me. ")),
        Step::data(reasoning_chunk("I should greet back.")),
        Step::data(text_delta_chunk("Hello")),
        Step::data(text_delta_chunk(", ")),
        Step::data(text_delta_chunk("world")),
        Step::data(finish_chunk("stop")),
        Step::data(usage_chunk(12, 9)),
        Step::done(),
    ])
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    const DELAY: Duration = Duration::from_millis(200);

    async fn post(provider: &FakeProvider, body: &str) -> reqwest::Response {
        reqwest::Client::new()
            .post(format!("{}/chat/completions", provider.base_url()))
            .header("Authorization", "Bearer test-key")
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .expect("request to fake provider")
    }

    /// Reads chunks until the accumulated text contains `needle`, returning when that happened.
    async fn read_until(
        response: &mut reqwest::Response,
        seen: &mut String,
        needle: &str,
    ) -> Instant {
        while !seen.contains(needle) {
            let chunk = response
                .chunk()
                .await
                .expect("read chunk")
                .expect("stream ended early");
            seen.push_str(&String::from_utf8_lossy(&chunk));
        }
        Instant::now()
    }

    #[tokio::test]
    async fn streams_events_in_order_with_the_scripted_delay() {
        let provider = FakeProvider::start().await.expect("start");
        provider.enqueue(StreamScript::new(vec![
            Step::data(r#"{"n":1}"#),
            Step::data(r#"{"n":2}"#).after(DELAY),
        ]));

        let mut response = post(&provider, "{}").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "text/event-stream");

        let mut seen = String::new();
        let first = read_until(&mut response, &mut seen, r#"data: {"n":1}"#).await;
        let second = read_until(&mut response, &mut seen, r#"data: {"n":2}"#).await;
        assert!(seen.find(r#""n":1"#) < seen.find(r#""n":2"#));
        assert!(
            second - first >= DELAY / 2,
            "second event arrived {:?} after the first",
            second - first
        );
    }

    #[tokio::test]
    async fn error_script_returns_status_retry_after_and_flat_body() {
        let provider = FakeProvider::start().await.expect("start");
        let body = flat_error_body(
            "Slow down",
            "too_many_requests_error",
            None,
            "queue_exceeded",
        );
        provider.enqueue(
            ErrorScript::new(StatusCode::TOO_MANY_REQUESTS)
                .retry_after("7")
                .body(body.clone()),
        );

        let response = post(&provider, "{}").await;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()["retry-after"], "7");
        assert_eq!(response.text().await.expect("body"), body);
    }

    #[tokio::test]
    async fn records_headers_and_json_body_and_fails_loudly_when_unscripted() {
        let provider = FakeProvider::start().await.expect("start");

        let response = post(&provider, r#"{"model":"m"}"#).await;
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        assert_eq!(provider.request_count(), 1);
        let recorded = &provider.requests()[0];
        assert_eq!(recorded.header("authorization"), Some("Bearer test-key"));
        assert_eq!(recorded.body, json!({ "model": "m" }));
    }

    #[tokio::test]
    async fn a_client_hanging_up_during_a_stall_is_signalled() {
        let provider = FakeProvider::start().await.expect("start");
        provider.enqueue(StreamScript::new(vec![Step::data(role_chunk())]).stall());

        let mut response = post(&provider, "{}").await;
        response.chunk().await.expect("read chunk");
        assert_eq!(provider.disconnect_count(), 0);
        drop(response);

        assert!(provider.wait_for_disconnect(Duration::from_secs(2)).await);
    }
}

# `aria-agent-core` — Agent Core (DEC)

[Library index](../README.md) · [Architecture](../03-architecture.md) · [Decisions](../decisions/README.md) · [Open questions](../open-questions/README.md)

**Deployed service · `services/agent-core/`**

---

## Amendments since 2026-08-12

- **[D49](../decisions/0009-measurement-session.md)** — the canonical internal tool-call shape is
  **ARIA's own type**, deliberately close to OpenAI's; both backends adapt into it rather than one
  vendor's format becoming canonical. Driven by a measurement: Ollama's OpenAI-compatible endpoint
  cannot disable thinking (`think: false` is native-API only), so the Ollama backend needs the
  native endpoint and neither adapter is free.
  **New requirement on ARIA-54:** set the thinking parameter explicitly rather than inheriting the
  model's default. **New requirement on ARIA-61:** the tool-loop latency budget must allow for a
  reasoning phase the hosted path does not have.
- **[ARIA-121](../backlog/README.md)** — new story, blocks ARIA-61. Both tool-capable local models
  fabricated a required argument on **100%** of runs where the user supplied none. The Agent Core
  cannot rely on the model to decline; something structural must catch it.


## Amendments from 2026-09-23 (D61, D83)

- **[D61](../decisions/0012-clients-gateway-surface.md)** — the **turn log is product data**, not
  an implementation detail of this service's conversation history. The Agent Core still owns and
  persists history (D36); what changed is that part of it is now a **user-facing surface** with a
  client reading it over the Gateway's HTTP door, which constrains what may be dropped or
  reshaped later.
- **[D83](../decisions/0016-observability-v2.md)** — the **user-facing action timeline is built
  from the turn log, never from traces.** Traces are sampled and retained on an operational
  schedule; the turn log is not. Building the timeline on traces would either force trace retention
  to match product retention or show the user a history with sampled-out holes in it.
- **[D82](../decisions/0016-observability-v2.md)** — the span wrapping the **LLM call** is one of
  D83's four propagation points. Whether `traceparent` rides the outbound **hosted** call is
  **still open** — propagating discloses a correlation identifier to a third party; not propagating
  breaks the trace at the most interesting hop. The Ollama fallback has no such objection.

## Amendments from 2026-10-02 (D85)

- **[D85](../decisions/0018-hosted-llm-backend.md)** — the default hosted backend is **Qwen 3.8 27B on
  Cerebras** (`https://api.cerebras.ai/v1`), behind an OpenAI-compatible adapter whose host is
  configuration. Reasoning stays at the provider default (`high`); switching it off was measured to cost
  accuracy. Streamed tool calls arrive as one complete chunk; prose streams.
  **New work recorded with it, not decided:** inject the current date, time and time zone into every
  request (measured to fix dates); a clarification path for ARIA-121 (every leading model used a
  `request_information` tool when given one); and, if strict schemas are used, dropping null optional
  arguments before forwarding.

## Interim behaviour from ARIA-131 (2026-10-06)

`Decide` now runs. None of the following is an architecture decision; each is what the walking
skeleton does until the named story replaces it.

- **History lives in process memory** and is lost on restart, with no eviction. D36 rejected this as
  an end state. It sits behind a history-store trait so that a Knowledge-Core-backed store can replace
  it without changing the `Decide` handler. *Replaced by:* the story that persists history (D36).
- **A second `Decide` on a session with a turn in flight is refused** with `ABORTED`, not queued, and
  the backend is not called. This assumes the caller cancels an in-flight turn on barge-in, which is
  **open**: D27 only has the Gateway cancel the outbound TTS stream. A cancelled turn is released
  only when the server drops the cancelled stream, so a `Decide` sent immediately after cancelling
  can briefly still see `ABORTED`. That is transient and safe to retry after a short delay.
  *Revisit if:* the Gateway's barge-in design needs the Agent Core to cancel the turn itself.
- **An exchange is committed only when the turn finishes cleanly.** The commit happens before
  turn-complete is sent. A backend error, an empty reply, a failed commit, or a client cancel before
  the commit starts leaves history unchanged. After a cancelled turn there is no memory of the half
  answer the user heard. Two edge cases are accepted: a cancel *during* the commit may or may not
  have stored the exchange (reported as `commit_interrupted`), and a client that disconnects after
  the commit but before turn-complete has its exchange stored without having seen turn-complete.
  A failed commit ends the stream with `INTERNAL` in place of turn-complete. *Revisit with:* D61's
  turn log, which is where a turn status such as "interrupted" belongs.
- ~~**The binary serves an `echo` backend.**~~ Replaced by ARIA-143: the binary now serves the hosted
  backend (below), and `echo` is gone.
- **Spans are exported only when an OTLP endpoint is configured** (`OTEL_EXPORTER_OTLP_ENDPOINT`).
  Without one, no exporter is created and logs still go to stdout. `RUST_LOG` filters stdout only, so
  a quiet log level never thins exported spans. Export has its own fixed filter: this service at
  `debug`, dependencies at `warn`. *Replaced by:* the in-cluster collector (D82), at which point the
  endpoint is always set.

## Interim behaviour from ARIA-143 (2026-10-22)

The binary serves the **hosted backend** ([D85](../decisions/0018-hosted-llm-backend.md)): an
OpenAI-compatible Chat Completions client (`openai` in logs and code, named for the protocol, not the
provider) pointed at Cerebras by default. Startup logs the backend, model and host, never the key. A
missing key or an invalid setting stops the binary at startup with the variable named. None of the
following is an architecture decision.

- **Retries.** A 429, a 5xx or a failure to connect is retried on the same provider, up to
  `ARIA_LLM_MAX_RETRIES` times, with exponential backoff and jitter. A provider `Retry-After` of up to
  10 s is honoured exactly; a longer one, or one that is not a number of seconds (such as a date),
  ends retrying at once and that response, a 429 or a 5xx, is reported as is. Nothing is retried once a reply has started streaming. This is not D35's failover,
  which will sit above it. A host that accepts the connection and then closes it without answering
  is not retried, because the request may already have reached it; the turn fails and the provider is
  marked unreachable.
- **Idle timeout.** A turn fails as a timeout when the provider sends nothing for
  `ARIA_LLM_IDLE_TIMEOUT`, counted from the request (opening and its retries share the first window)
  and restarted by every event, reasoning included. Keep-alive comments do not restart it. A long
  reply that keeps streaming never trips it.
- **How a reply can end.** A normal finish commits the turn. A reply cut short by the length limit
  after some text ends cleanly; with no text it is an error. A content-filter finish is **always** an
  error, because part of the answer was withheld. A tool call is an error until tool calling lands
  (ARIA-61). A stream that ends without a finish reason is treated as cut off, an error, never a
  complete reply. Once a normal finish has arrived, anything that goes wrong afterwards (a stall, a
  dropped connection, an unreadable usage event) still commits the turn: the whole answer was
  already delivered.
- **Base URL.** All trailing slashes on `ARIA_LLM_BASE_URL` are removed, so `…/v1/` and `…/v1` are
  the same. A URL carrying a username or password is refused at startup, and no error message ever
  prints URL credentials back.
- **Provider error messages stay local.** A failed turn's log line and span carry the HTTP status,
  the provider's error code and the kind of failure, never the provider's message text, which some
  servers fill with the request's input. The message is logged once, cut to 200 characters, at
  `debug` under its own target, which the OTLP export drops. **This is a deliberate, narrow exception
  to "conversation content never lands in a log line" ([D88](../decisions/0021-provider-message-debug-log.md)):** stdout is the cluster's log surface (D82), so
  enabling `debug` for this service on a pod (`RUST_LOG=agent_core=debug`) puts up to 200 characters
  of provider text into that pod's logs. It stays off by default (`RUST_LOG` defaults to `info`).
- **Conversation mapping.** History goes first, then the new input. Stored system entries are not
  replayed; whether they ever are is open (below). The current date and time are not injected yet
  (ARIA-222).
- **Availability is observed, not probed.** The backend records whether the provider was reachable
  from real turns, once per turn after retries: any answer from the provider, even a 400, means
  available (the one exception is an answer that says nothing about serving, such as a redirect loop,
  which changes nothing); a rejected key, a rate limit, payment required, a 5xx, a timeout, an unreachable host or
  a broken stream means unavailable, with that reason. A cancelled turn changes nothing. Each change
  is logged once. It starts as unknown until the first turn.
- **Spans.** Each provider call has a `chat {model}` span under the `Decide` span, covering the whole
  stream, with a first-text event and an `error.type` on failure; each HTTP attempt, retries included,
  is a child span. No message content and no key is recorded. Whether `traceparent` is sent to the
  provider is still open; it is not sent.

**Running it (interim values, not settled):**

| Setting | Variable | Default |
|---|---|---|
| Listen address | `ARIA_AGENT_CORE_LISTEN_ADDRESS` | `0.0.0.0:6517`. Port `6517` is a skeleton default, not an allocation. |
| Span export | `OTEL_EXPORTER_OTLP_ENDPOINT` | unset, so export is off |
| Stdout log level | `RUST_LOG` | `info` |
| gRPC server reflection (v1 and v1alpha, for grpcurl/Postman) | `ARIA_AGENT_CORE_REFLECTION` | off. `true`/`1` turns it on; blank means off. |
| Hosted backend API key | `ARIA_LLM_API_KEY` | **required**; no default. Held as a secret and never logged. Surrounding whitespace, such as the trailing newline of a secret made from a file, is trimmed; a key that still cannot be sent as a header stops startup. |
| Hosted backend base URL (including `/v1`; `http` or `https` with a host, no credentials, query or fragment; trailing slashes are dropped) | `ARIA_LLM_BASE_URL` | `https://api.cerebras.ai/v1` |
| Hosted model (surrounding whitespace is trimmed) | `ARIA_LLM_MODEL` | `qwen-3.8-27b` |
| Idle timeout (a duration such as `30s`; zero and anything above `1h` are rejected) | `ARIA_LLM_IDLE_TIMEOUT` | `30s` |
| Retries per request (`0` turns retrying off) | `ARIA_LLM_MAX_RETRIES` | `3` |

The five `ARIA_LLM_*` settings are the hosted client's only ARIA configuration: `OPENAI_*` variables in
the environment (`OPENAI_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_ORG_ID`, `OPENAI_PROJECT_ID`, …) are
ignored and never sent to the provider. The standard proxy variables (`HTTPS_PROXY`, `ALL_PROXY`,
`NO_PROXY`) and the host's system proxy settings still apply to the provider call.

**Health (`grpc.health.v1`, same port):**

| Health service name | Serving when | Kubernetes probe |
|---|---|---|
| `""` (the server overall) | the process is up and shutdown has not begun | **liveness** |
| `aria.agent_core.v1.AgentCore` | the history store **and** the backend both report healthy in their *configured* state (observed provider availability is not counted), and shutdown has not begun | **readiness** |
| `aria.agent_core.v1.AgentCore.history` | the history store reports healthy, and shutdown has not begun | none; for diagnosis |
| `aria.agent_core.v1.AgentCore.backend` | the provider was last observed **available**; `NOT_SERVING` once observed unavailable (the log names the reason) or once shutdown begins; `UNKNOWN` until the first turn | none; for diagnosis |

**Changed by ARIA-143:** the backend entry used to follow the backend's configured health, like
readiness. It now reports the provider's **observed availability**, including the `UNKNOWN` state.
Readiness still uses the configured state only, so a provider outage or a wrong key does **not** make
pods unready: turns fail with `INTERNAL`, and the backend entry and the span say why.

`List` (from the current upstream `health.proto`, vendored in the `proto` crate) returns all four
statuses in one call. The store and the backend each publish their own health. The health service only listens, and never
calls a dependency to check it. On SIGTERM all four statuses become not-serving **before** the drain
starts. Watchers of the health status see the change, and then their streams end, so they do not
hold up the drain. New connections stop because the listener closes; in-flight turns get up to 20 s
to finish. A dependency that reports itself as a
backup (for later failover, D35) still counts as healthy; the flag is only logged. **Accepted cost
(ARIA-131):** a backend that reports itself unhealthy in its configured state makes every pod unready
at once, so callers see "no endpoints" rather than a `Decide` error naming the backend. The hosted
backend never does this: its configured state is fixed, and since ARIA-143 a provider outage shows
only on the backend entry, with pods staying ready and turns failing with `INTERNAL`. *Revisit if*
either proves worse in practice than the other.

**Calling the real provider (manual, key-gated):** one test calls Cerebras for real, to show the
first chunk arrives before the reply ends. It is ignored by the CI gate and fails, rather than
skipping, when the key is missing:

```
ARIA_LLM_API_KEY=<key> cargo test -p agent-core --features test-util \
    --test openai_live -- --ignored --nocapture
```

**Version:** every response carries an `x-aria-version` header with the build version. The
startup log and the OTel `service.version` attribute carry it too. CI images carry the git SHA, passed as the
`ARIA_GIT_SHA` Docker build argument; a local build without it reports `unknown`. No call checks compatibility yet; that is an open question.

**Status codes `Decide` returns:**

| Status | When |
|---|---|
| `INVALID_ARGUMENT` | Empty `session_id` or `text`. The backend is not called. |
| `ABORTED` | The session already has a turn in flight. Right after the caller cancels its own turn this can be transient; retry after a short delay. |
| `UNAVAILABLE` | The history store failed before the turn started. Nothing has happened, so a retry is safe. |
| `INTERNAL` | The backend failed or replied with nothing, or the commit failed after the reply was streamed. A fault, not a transient outage. Because it arrives mid-stream, after the response headers, no gRPC retry policy would retry it whatever the code. |

---

## Purpose

The orchestrator. Retrieves context, calls the LLM, executes the chosen tool call via the Registry, streams the response back.

## Replaces

The 2021 hand-built `NLU` / `NLG` boxes and the `Unknown` fallback box.

## Owns

Per request:
1. Retrieve relevant context from the Knowledge Core
2. Call the pluggable LLM (hosted API default, Ollama fallback) with the current MCP toolset
3. Execute whichever MCP tool call the model chooses **via the MCP Registry** — it holds no MCP
   client of its own (D38)
4. Stream the response back — `Decide` is **server-streaming**, emitting text chunks and tool-call
   events so synthesis can start on the first sentence (D37)

Also owns per-`session_id` conversation history, persisted to the Knowledge Core (D36), and the
**consent-prompt path**: turning the Registry's `consent required` into a question in the
conversation and retrying the suspended call on approval (D42).

## Binding decisions

D35 (failover + override, no automatic routing) · D36 (`session_id`, history) · D37 (server-streaming `Decide`) · D38 (no MCP SDK) · D40 ("unrecognized" defined structurally) · D42 (consent prompt path) · D46 (may see ungranted tools) · D9 (never trust an identity-shaped argument)

See [the Decision Log](../decisions/README.md) for the full reasoning and rejected alternatives.

## Contract

`Decide` (server-streaming) — `proto/aria/agent_core/v1/agent_core.proto`

## Open items

- ~~Awaiting measurement: hosted provider/model (ARIA-40)~~ — answered by [D85](../decisions/0018-hosted-llm-backend.md).
- **Awaiting measurement:** which Ollama model (ARIA-109) —
  this gates whether the backend-agnostic conformance suite passes unmodified, and gates reopening
  the automatic-routing rule
- Whether the serving backend is exposed in the `Decide` response body or kept to traces only
- Prompt formatting, and whether retrieval re-runs mid-loop
- The numeric tool-loop iteration bound (deferred — C-4)
- Who cancels an in-flight `Decide` on barge-in ([needs-decision](../open-questions/needs-decision.md#surfaced-by-aria-131-2026-10-06))
- Where D49's conversation type lives once it crosses a service boundary (same section)
- Whether stored system entries are replayed to the backend; a direction is agreed but not decided (same section)
- Date, time and time zone injection into every request (ARIA-222)
- **Recovery once failover routes away on availability.** Today availability recovers passively,
  because readiness ignores it and real turns keep arriving. Once D35's failover sends turns
  elsewhere while the hosted backend is unavailable, no turn arrives to observe recovery, so an
  active check is needed while unavailable (lazy while available, active while unavailable). Belongs
  with D35's failover.
- Whether `traceparent` rides the hosted call (not sent today)

## Jira stories

ARIA-40, 48, 50, 53, 54, 56, 58, 61, 63, 109, 131, 143

---

*Derived from `03-architecture.md` and the Decision Log. Last updated 2026-10-22.*

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
- **The binary serves an `echo` backend.** It streams the user's text back and is not a product
  backend; startup logs which backend is serving. *Replaced by:* ARIA-143 (hosted backend, D85).
- **Spans are exported only when an OTLP endpoint is configured** (`OTEL_EXPORTER_OTLP_ENDPOINT`).
  Without one, no exporter is created and logs still go to stdout. `RUST_LOG` filters stdout only, so
  a quiet log level never thins exported spans. Export has its own fixed filter: this service at
  `debug`, dependencies at `warn`. *Replaced by:* the in-cluster collector (D82), at which point the
  endpoint is always set.

**Running it (interim values, not settled):**

| Setting | Variable | Default |
|---|---|---|
| Listen address | `ARIA_AGENT_CORE_LISTEN_ADDRESS` | `0.0.0.0:6517`. Port `6517` is a skeleton default, not an allocation. |
| Span export | `OTEL_EXPORTER_OTLP_ENDPOINT` | unset, so export is off |
| Stdout log level | `RUST_LOG` | `info` |
| gRPC server reflection (v1 and v1alpha, for grpcurl/Postman) | `ARIA_AGENT_CORE_REFLECTION` | off. `true`/`1` turns it on; blank means off. |

**Health (`grpc.health.v1`, same port):**

| Health service name | Serving when | Kubernetes probe |
|---|---|---|
| `""` (the server overall) | the process is up and shutdown has not begun | **liveness** |
| `aria.agent_core.v1.AgentCore` | the history store **and** the backend both report healthy, and shutdown has not begun | **readiness** |
| `aria.agent_core.v1.AgentCore.history` | the history store reports healthy, and shutdown has not begun | none; for diagnosis |
| `aria.agent_core.v1.AgentCore.backend` | the backend reports healthy, and shutdown has not begun | none; for diagnosis |

`List` (from the current upstream `health.proto`, vendored in the `proto` crate) returns all four
statuses in one call. The store and the backend each publish their own health. The health service only listens, and never
calls a dependency to check it. On SIGTERM both statuses become not-serving **before** the drain
starts. Watchers of the health status see the change, and then their streams end, so they do not
hold up the drain. New connections stop because the listener closes; in-flight turns get up to 20 s
to finish. A dependency that reports itself as a
backup (for later failover, D35) still counts as healthy; the flag is only logged. **Accepted cost:**
a backend outage makes every pod unready at once, so callers see "no endpoints" rather than a
`Decide` error naming the backend. *Revisit if* that proves worse in practice than serving and
failing turns.

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

## Jira stories

ARIA-40, 48, 50, 53, 54, 56, 58, 61, 63, 109, 131, 143

---

*Derived from `03-architecture.md` and the Decision Log. Last updated 2026-10-06.*

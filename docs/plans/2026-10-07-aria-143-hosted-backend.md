---
title: Add the openai LLM backend to the Agent Core (ARIA-143)
slug: aria-143-openai-backend
tier: technical
status: active
created: 2026-10-07
branch: aria-143-openai-backend
owns_branch: true
---

## Goal

The Agent Core serves a hosted backend that calls Qwen 3.8 27B on Cerebras (D85) through an
OpenAI-compatible Chat Completions client and streams its text into `Decide` as it arrives. The
`echo` backend no longer ships in the binary. A missing key stops startup with an error naming the
setting. A provider that stops sending ends the turn with an error instead of hanging. The backend
retries transient provider failures before giving up, and it reports what it has observed about the
provider's availability through the diagnostic health entry, without that observation ever affecting
readiness. Spans around the provider call make time to first token, retries and the failure kind
visible. Story: [ARIA-143](https://firephoenixgames.atlassian.net/browse/ARIA-143); subtasks
ARIA-145 (streaming client), ARIA-147 (configuration), ARIA-149 (key-gated test), ARIA-150 (spans).

## Non-goals

- The Ollama fallback and any failover between backends (D35's story).
- Tool calls. No tools are sent to the provider.
- Injecting the date, time or time zone, or any system prompt. Tracked as
  [ARIA-222](https://firephoenixgames.atlassian.net/browse/ARIA-222); interim: no system prompt, so
  the model may get dates wrong.
- Strict tool schemas and null stripping (no tools, so the question doesn't arise yet).
- Probing the provider for health. Availability is observed from real turns only.
- A backend selector setting. There is one served backend.
- The Helm chart. It doesn't exist yet; ARIA-177 creates it and picks the settings up from the Agent
  Core page.
- Moving the Cerebras account to pay-as-you-go, and the egress allow-list (deployment work recorded
  under D85).

## Constraints

- **The backend contract from ARIA-131.** A backend has a name, publishes its health on a watch
  channel, and turns history plus input into a stream of text chunks or a backend error that carries a
  boxed cause. The contract may grow (a variant, or a health field) but its existing users keep
  working: the `Decide` service, the scripted test backend and the health monitor.
- **Turn semantics from ARIA-131 are unchanged.** A clean end with no chunks already fails the turn
  as `INTERNAL`/`generation_failed`; the hosted backend relies on that rather than reimplementing it.
- **`ARIA_LLM_API_KEY` is the credential's name** (D85, sealed secret `llm-api-key`).
- **No `traceparent` on the outbound call** (open question, interim: do not send).
- **The CI gate (ARIA-140) runs without a key** and requires 80 % line coverage. Everything except
  the key-gated test must be exercised against a local fake provider.
- **`grpc.health.v1` carries only a serving status per entry**, so richer availability detail can
  only travel in logs and spans.
- **Prompt and reply content never enters telemetry**, and the key never appears in logs, spans or
  debug output.
- **rustls, not OpenSSL**, so the container image keeps no system TLS dependency.

## Decisions

- **Client: `async-openai` (0.42.x) over a hand-built `reqwest` + SSE client.** First recommendation
  was hand-built, on the belief that the crate's retries would hide failures, duplicate streamed text
  and flatten HTTP status. Reading the 0.42.1 source disproved all three: retry runs at the HTTP
  response before any body is read (a 2xx returns immediately, so streamed text is never retried),
  attempts are counted, and API errors carry the status code. The middleware is a tower stack, so the
  retry policy and a tracing layer can be swapped in. Rejected: hand-built `reqwest` + an SSE parser
  (rebuilds what exists); `reqwest-eventsource` (reconnects by design, wrong for a one-shot
  completion). Pin the minor version: the crate is pre-1.0 and changes shape between minors.
- **Retry the same provider before giving up; this is not failover.** Transient failures (429, 5xx,
  connect errors) are retried a configurable number of times, default 3, with the crate's backoff.
  D35's failover will sit above this layer: retries exhausted → fail over. Retrying never happens
  once any body has been read.
- **Our own retry policy, reusing the crate's retry classification but capping `Retry-After`.** The
  crate honours `Retry-After` without a cap. On the trial tier (5 requests/minute) a 429 can say
  "wait 60 s"; the idle timeout would end the wait, but the turn would then be reported as a timeout
  instead of a rate limit. When the provider asks for longer than a fixed cap (5 s, a constant rather
  than a setting), the policy gives up at once and the 429 is reported as is. Rejected: the default
  layer (a rate limit shows up as a timeout); no retry on 429 (loses the common short-wait case).
- **Timeout is an idle timeout, one setting, default 30 s.** It covers the wait for the first event
  and every gap between events after that, and any event resets it, including reasoning deltas that
  are never emitted as text. Rejected: a total deadline (cuts off long healthy replies, or must be set
  so high that a stall hangs the user); time-to-first-byte only (misses a mid-stream stall). A total
  ceiling as a backstop was considered and left out: it guards against a trickle failure nobody has
  observed. The retry wait counts against the idle timeout, because nothing has arrived yet.
- **`echo` leaves the binary; hosted is the only served backend, and a missing key is always
  fatal.** Rejected: a backend selector (pre-empts D35's configuration surface; a selector is hard to
  remove once relied on); falling back to `echo` without a key (a pod missing its secret would serve
  parrot replies). For local runs without a provider, point the base URL at Ollama's OpenAI-compatible
  endpoint with any non-empty key.
- **The base URL is a setting**, as D85's "the host is configuration" requires. Settings, all with
  interim names and values: `ARIA_LLM_API_KEY` (required; blank counts as missing),
  `ARIA_LLM_BASE_URL` (default `https://api.cerebras.ai/v1`), `ARIA_LLM_MODEL` (default
  `qwen-3.8-27b`), `ARIA_LLM_IDLE_TIMEOUT` (default 30 s), `ARIA_LLM_MAX_RETRIES` (default 3).
- **Health: readiness uses "configured"; the diagnostic backend entry reports observed provider
  availability.** Readiness keeps using the backend's configured state, which is always true at
  runtime because a configuration error is fatal at startup, so a provider outage never pulls every
  pod at once. The `aria.agent_core.v1.AgentCore.backend` entry now reports availability:
  available → `SERVING`, unavailable → `NOT_SERVING`, key rejected → `NOT_SERVING`, no call yet →
  gRPC's `UNKNOWN`. Shutdown still forces `NOT_SERVING` first. **This changes ARIA-131's documented
  health table:** the backend entry is no longer an input to readiness. Chosen by Jann as the honest
  option, which is what the per-dependency entries exist for. Rejected: keeping availability out of
  the health entries (hides what the entries are for); an active probe (spends the trial rate limit,
  sends traffic with no turn behind it, and pulls every pod on a blip); flipping readiness after N
  failures (half a circuit breaker that belongs to D35, and an unready pod gets no turns, so it never
  recovers).
- **Availability is observed per turn, after retries are exhausted**, so one transient failure does
  not flip it. Classification: completed stream → available; connect failure, idle timeout, 5xx or
  429 → unavailable, with the kind recorded; 401 or 403 → key rejected (the provider is up but
  refuses us); any other 4xx → no change (that's our bug, not their outage). Each transition is logged
  once.
- **Conversation mapping.** User and assistant parts map to their roles. A system part in the
  **current input** is sent with the system role; **stored** system entries in history are not
  replayed. That follows the direction agreed while planning ARIA-131, and nothing writes system
  entries yet.
- **Finish reasons.** `length` or `content_filter` with no text becomes an error that carries the
  reason (ARIA-131's review). `length` with text ends cleanly, and the reason is recorded on the span.
- **A tool-call delta is an error.** No tools are sent, so it is a protocol violation, and dropping it
  silently would commit a reply with a hole in it. Rejected: ignoring it.
- **Cancelling `Decide` aborts the HTTP request.** Dropping the stream drops the request, so tokens
  nobody will see stop being generated.
- **Spans follow the OpenTelemetry GenAI semantic conventions.** One span per backend call
  (`chat {model}`) carrying operation, provider, server address, requested and response model, finish
  reasons, input and output token counts (the stream is asked for a usage summary), and `error.type`
  using the availability classification. An event marks the first text chunk, so time to first token
  is read off the trace. A tracing layer in the tower stack adds a child span per HTTP attempt, so
  retries show. Rejected: home-made attribute names.
- **The fake provider is a hand-built, controllable SSE test server, not wiremock.** wiremock delays
  a whole response, but two cases need a stream that sends one event and then stalls, or sends a
  chunk well before the end. It lives behind the `test-util` feature as a fixture, not product code,
  and also records what it received, so tests can assert on the request.
- **The key-gated test is `#[ignore]` and fails without a key.** It is run with the key set plus
  `--ignored`. Failing rather than skipping when the key is absent means a green result always means
  it really ran. Rejected: an early return when the key is missing (passes silently).
- **No Helm change in this PR.** The Definition of Done's Helm clause has nothing to update until
  ARIA-177 creates the chart; the settings table on the Agent Core page is its input.

### Amendment 2026-10-19: the span's `error.type` comes from the error, not from availability

The Spans decision above said the call span's `error.type` uses the availability classification.
That did not survive two later choices: "any answer from the provider counts as available"
(GenerationFailed, UnexpectedToolCall, ordinary 4xx classify as available, so they have no kind to
record) and the classification's built-in deduplication (a repeated outcome returns "no update", so a
second consecutive timeout gives the span nothing). The API review found this as a Bug. Availability
answers "is the provider up?"; the span needs "what went wrong in this call?". **Chosen (Jann):** the
span's `error.type` names the `CompletionError` case itself. Rejected: making the classification also
return an error kind (it would answer two questions in one function and tie the span to the
dedup rule).

### Settled in Phase 1 (brainstorm, 2026-10-07)

- **`BackendError` stays thin; the hosted backend owns its classification.** The `Decide` service
  maps every backend failure to `INTERNAL`, so only availability (C5) and the span (C6) need the
  cases, and both live inside the hosted backend. It classifies into its own error type, then boxes
  that as the cause. Rejected: a variant per case on `BackendError` (HTTP status, key rejected and
  tool-call delta are provider concepts in a backend-neutral trait, and every other backend inherits
  them). Deferred: a provider-neutral error *kind* on `BackendError`, which is D35 failover's call
  if it needs to branch without downcasting. Whether the hosted error type is public (tests downcast
  the cause) or private (tests observe classification through availability and spans) is settled
  in Phase 2.
- **Availability update semantics.** The returned stream is `'static`, so it carries what it needs
  to publish health. Only a real end (success, error, timeout) updates availability; a stream that
  is dropped (client cancel) leaves it unchanged. Overlapping turns: last writer wins (accepted, the
  next turn corrects it). A change of *kind* while unavailable (timeout → 503) counts as a transition
  and is logged. A mid-stream failure after text has arrived counts as unavailable. Some outcomes
  (other 4xx) mean "no update", and the classification must be able to say so. Availability is a
  **new field** beside the configured flag in `BackendHealth`, not a replacement for it: readiness
  and the diagnostic entry read two different facts. The scripted backend defaults it to unknown and
  can set it for C5's tests.
- **One idle window across both phases of a call.** Opening (send, retries, wait for a 2xx) counts as
  waiting for the first event; every event after that restarts the window. A timeout while opening
  must drop the in-flight retry future, so no request outlives the failed turn. Rejected: a separate
  opening deadline (a second setting, and the rejected total deadline applied to a smaller slice).
- **Investigate first in Phase 2 (code-agent):** does `async-openai`'s streaming call return only
  once the response status is known, or does it return a stream immediately and deliver a 401/503
  as the first item? That decides where classification reads the HTTP status. In the second case,
  an error in the stream cannot be assumed to be a mid-stream failure. *(Answered; see Deviations.)*
- **The idle timeout is a duration string** (`ARIA_LLM_IDLE_TIMEOUT=30s`, parsed with
  `humantime`). A bare number has no unit and is a configuration error naming variable and value,
  not a guess at seconds. Zero is rejected. Rejected: integer seconds with `_SECS` in the name.
- **The key is a `secrecy` secret, and this is now a repo rule for every secret**
  (`docs/05-conventions.md`, Jann's call). `async-openai` already depends on `secrecy`, so it costs
  no new dependency. Rejected: a hand-written `Debug` that skips the field (the next derive on a
  containing type leaks it). Existing code is brought into line as it is touched, not in this
  story. The code-agent reports any secret it sees held unwrapped, so the gap is known.
- **Tower layer order: tracing goes *inside* retry** (retry → tracing → reqwest), or C6 gets one
  span for all attempts instead of one per attempt. Easy to invert unnoticed; C6's retry test is
  what catches it.

## Open questions

None block this plan. Carried, not decided here:

- Whether `traceparent` rides the hosted call (interim: not sent; a test pins it).
- Date, time and time zone injection, and whether stored system entries are ever replayed
  (ARIA-222).
- **Recovery once something routes away on availability.** Here availability recovers passively,
  because readiness ignores it and real turns keep arriving. Once D35's failover sends turns
  elsewhere while the hosted backend is unavailable, no turn arrives to observe success, so recovery
  needs an active check while unavailable (lazy while available, active while unavailable; Jann's
  proposal, Phase 1). Belongs with D35's failover; C9 records it on the Agent Core page.

## Risks

- **Cerebras's stream may not match what `async-openai`'s chunk types expect** (reasoning fields,
  a usage-only final chunk with no choices, error events mid-stream). Early signal: the key-gated test
  or a manual `grpcurl` run fails to deserialize while the fake provider passes. Mitigation: the
  `byot` feature or tolerant types; record a Deviation.
- **The fake provider can drift from the real one**, so CI is green while production breaks. Early
  signal: the key-gated test fails on a case the fake passes. Mitigation: make the fake's default
  events a captured real Cerebras stream rather than invented ones.
- **Trial-tier rate limits make the key-gated test flaky.** Early signal: intermittent 429s in manual
  runs. Accepted while it is a manual test.
- **A key that's wrong (rather than missing) leaves pods ready but failing every turn.** Accepted:
  readiness deliberately ignores availability. Mitigated by the key-rejected state on the diagnostic
  entry and in the span.
- **Rate limits reported as timeouts.** With a 10 s `Retry-After` cap and 3 retries, honoured waits
  can fill the 30 s idle window. Early signal: spans or availability show `TimedOut` while the
  attempt spans show 429s. Remedy: lower the cap, or keep cap × max retries well under the idle
  timeout.
- **Coverage.** The retry and timeout paths are where untested lines would hide. Early signal: the
  coverage job drops below 80 % on this branch.

## Components

### C1 — Hosted backend configuration

status: done

**Responsibility.** Read and validate the hosted backend's settings from the environment, following
the existing configuration module's lookup pattern, so the binary either has a complete, valid
configuration or refuses to start with a message saying which setting is wrong. (ARIA-147.)

**Contracts.** A missing or blank key is an error that names `ARIA_LLM_API_KEY`. Base URL, model,
idle timeout and retry count fall back to their defaults when unset or blank. An unparsable value is
an error naming both the variable and the offending value, except for the key, whose value is never
echoed. The key is never revealed by debug formatting, logs or error messages. Callers can read every
setting back as configured.

**Tests.** A set key with everything else unset yields the documented defaults. An unset key, and a
blank key, each fail and name the variable. Each optional setting, when set, is used. An unparsable
timeout and an unparsable retry count each fail, naming variable and value. A negative or zero
timeout is rejected. Debug-formatting a configuration does not contain the key's value.

**Done when.** Those tests pass and lint is clean.

### C2 — Controllable fake provider

status: done

**Responsibility.** A test fixture, behind `test-util`, that speaks enough of the Chat Completions
streaming endpoint for the backend to be exercised without a network: scripted SSE events with
delays between them, stalls, error statuses with optional `Retry-After`, and a record of every
request it received.

**Contracts.** A test can script, per request, either a status (with an optional `Retry-After`) or a
sequence of events with a delay before each, optionally ending with a stall that never closes. A test
can read back how many requests arrived and each one's headers and body. A test can tell that the
connection was closed by the client. The default happy-path script reproduces a real Cerebras stream
(reasoning deltas, text deltas, finish reason, usage chunk), not an invented shape.

**Tests.** Exercised through C4–C6 rather than tested on its own. One sanity case: a scripted
two-event stream is received by a plain HTTP client in order, with the delay observable.

**Done when.** C4's tests can be written against it and the sanity case passes.

### C3 — Retry policy

status: done
depends-on: 1

**Responsibility.** Decide, for each failed attempt, whether the client tries again and after how
long, reusing the crate's retry classification and adding the `Retry-After` cap.

**Contracts.** 429, 5xx and connect errors are retried; any other status and any success are not.
At most the configured number of retries happen. A `Retry-After` within the cap is honoured; one
above the cap ends retrying immediately and the original 429 reaches the caller with its status
intact. Without `Retry-After`, backoff grows between attempts. Nothing is retried once a successful
response's body has started.

**Tests.** A 503 then success yields success after one retry. Persistent 503s stop after the
configured count and report the 503. A 429 with a short `Retry-After` is retried after waiting about
that long. A 429 with a long `Retry-After` is not retried, and the caller sees a 429. A 400 and a 401
are not retried. A retry count of zero makes exactly one request.

**Done when.** Those tests pass against the fake provider, and lint is clean.

### C4 — Hosted backend streaming

status: done
depends-on: 1, 2, 3

**Responsibility.** The backend itself (ARIA-145): turn history and input into a streaming Chat
Completions request, and the provider's event stream into ARIA's text chunks and backend errors,
enforcing the idle timeout and classifying every outcome.

**Contracts.** Text deltas become text chunks in order, each emitted as soon as it arrives. Reasoning
deltas are never emitted but reset the idle timeout. No tools and no `traceparent` are sent; the
configured model is, and the stream is asked for a usage summary. User and assistant parts keep their
roles; a system part in the input is sent as system; stored system entries in history are not.
Callers can tell apart an idle timeout, a provider HTTP error (status available), a rejected key, a
connection failure, a mid-stream stream error, a `length` or `content_filter` finish with no text
(the reason available), and an unexpected tool-call delta, each with its cause attached. A clean end
with text, including a `length` finish, ends the stream without error. Dropping the stream closes the
provider connection. The backend reports its name.

**Tests.** A three-delta stream yields three chunks in order. The first chunk is observable before a
delayed final event is sent. Reasoning-only deltas yield no chunks. A stall after the first event ends
with a timeout error within the idle window, after the first chunk was delivered. No event at all ends
with a timeout error. Reasoning deltas spaced just under the timeout do not trip it. A 401 surfaces as
key rejected; a 500 after retries as a provider error carrying the status. A `content_filter` finish
with no text errors carrying the reason; a `length` finish after text ends cleanly. A tool-call delta
errors. The recorded request has the configured model, no tools, no `traceparent`, the right roles,
and leaves out a stored system entry. Dropping the stream mid-reply closes the fake provider's
connection.

**Done when.** Those tests pass and lint is clean.

### C5 — Provider availability in backend health

status: done
depends-on: 4

**Responsibility.** Extend the backend's health with observed provider availability, and change the
health service so readiness uses the configured state while the diagnostic backend entry reports
availability.

**Contracts.** Availability starts as unknown and is updated once per turn, after retries are
exhausted, according to the classification in Decisions; other 4xx leave it unchanged. Readiness
(`aria.agent_core.v1.AgentCore`) is unaffected by availability. The `…AgentCore.backend` entry
maps available → `SERVING`, unavailable and key rejected → `NOT_SERVING`, unknown → `UNKNOWN`.
Shutdown still sets every entry to not serving before the drain. Each availability transition is
logged once with its kind; repeating the same state logs nothing. The scripted backend and existing
health tests keep working.

**Tests.** A fresh backend's entry is `UNKNOWN` and the server is ready. After a successful turn the
entry is `SERVING`. After a turn that exhausts retries on 503s the entry is `NOT_SERVING` and
readiness is still `SERVING`. After a 401 the entry is `NOT_SERVING` with the key-rejected kind
logged. A 400 leaves the previous state. A success after an outage returns the entry to `SERVING`.
Shutdown makes the backend entry not serving, whatever availability was.

**Done when.** Those tests and the existing health tests pass, and lint is clean.

### C6 — Spans around the provider call

status: done
depends-on: 4

**Responsibility.** Instrument the backend call (ARIA-150) per the GenAI semantic conventions: one
span per call, an event on the first text chunk, and a child span per HTTP attempt.

**Contracts.** The call span carries operation, provider, server address, requested and response
model, finish reasons, and input and output token counts when the provider reports them. On failure it
carries `error.type` naming the failure's own case (the `CompletionError` case), independent of the
availability classification and of deduplication (amended 2026-10-19). The first-text event occurs exactly
once per call, and only when text arrived. Each retry adds a child attempt span. No span or event
contains prompt text, reply text or the key.

**Tests.** Using the existing tracing-capture helper: a successful call produces one call span with
model, finish reason and token counts, and one first-text event. A call that retried once has two
attempt spans. A timed-out call carries the timeout error type, and so does a second consecutive
timed-out call (availability unchanged). A `content_filter` failure carries its own error type
although availability stays available. No captured field contains the prompt's or the reply's text,
or the key.

**Done when.** Those tests pass reliably under parallel test execution, and lint is clean.

### C7 — Serve the hosted backend from the binary

status: done
depends-on: 1, 4, 5, 6

**Responsibility.** Wire the configuration and the hosted backend into the binary in place of `echo`,
so startup either serves the hosted backend or exits with the configuration error.

**Contracts.** With a valid configuration the server starts, logs that the hosted backend is serving
and which model and host it uses, and never logs the key. With an invalid or incomplete configuration
the process exits non-zero before listening, and its error names the setting. `echo` is no longer
reachable from the binary; it stays behind `test-util` only if a test still uses it, otherwise it is
deleted.

**Tests.** Starting the binary without the key exits non-zero with `ARIA_LLM_API_KEY` in its error
output and never opens the port. Starting it against the fake provider serves a `Decide` turn whose
chunks come from the fake.

**Done when.** Those tests pass, lint is clean, and a manual `grpcurl` turn against Cerebras streams a
reply (story criterion 1).

### C8 — Key-gated integration test

status: done
kind: chore
depends-on: 7

**Responsibility.** The one test that calls the real provider (ARIA-149), proving the first chunk
arrives before the provider finishes.

**Contracts.** Ignored by default. When run, it fails, rather than passing, if `ARIA_LLM_API_KEY` is
unset. It asks for a reply long enough that the first chunk and the end of the stream are measurably
apart, and passes only if the first chunk arrived strictly before the stream ended, with more than one
chunk. The command to run it is documented on the Agent Core page.

**Tests.** It is the test. It was run once with a real key before shipping, and the result is
recorded in the PR.

**Done when.** It passes with a key, fails without one, and is skipped by the CI gate.

### C9 — Agent Core page and surfaces

status: done
kind: chore
depends-on: 5, 7

**Responsibility.** Bring `docs/services/agent-core.md` in line with what now runs, and keep Jira
and Confluence consistent with it.

**Contracts.** The interim-behaviour section says the hosted backend replaced `echo`, and states
retries, the idle timeout, availability observation and conversation mapping as interim behaviour.
The settings table gains the five `ARIA_LLM_*` settings with their defaults. The health table states
that readiness uses the configured state and that the backend entry reports availability, including
the `UNKNOWN` state. That change to ARIA-131's table is called out as such. The key-gated test's run
command is documented. The date question points at ARIA-222. Nothing in `docs/decisions/` is edited.
The recovery open question (active checks once failover routes away) is listed in the page's open
items. The Confluence mirror is updated through the existing mirror process, including the new
secrets rule on the conventions page.

**Tests.** Reviewed, not scaffolded: every setting and health state in the page matches what C1, C5
and C7 implement.

**Done when.** The page is merged with the code, and the mirror matches it.

## Deviations

- Phase 2 investigation (`async-openai` 0.42.1 source): **the status is known before the stream
  exists.** A non-2xx is an `Err` from awaiting the streaming call, never a first stream item, so
  opening and streaming classify separately, as hoped.
- **C4 gains a contract case: a stream that ends without a finish reason is an error (truncated).**
  The crate treats a stream ending without `[DONE]` as a clean end, so without this a cut connection
  would commit half a reply as complete. Test: a fake stream that sends text and then closes without
  a finish reason ends with an error.
- **Keep-alives do not reset the idle timeout**, contrary to the plan's "any SSE event, including
  keep-alives". The crate swallows `keepalive` events, and the reasoning field is dropped from the
  typed delta (visible only with `byot`). Reasoning chunks still arrive as stream items, so they
  still reset the timer. Accepted: it only matters if the provider sends nothing but keep-alives for
  longer than the timeout.
- **A non-JSON error body loses the HTTP status** inside the crate (a deserialize error carries the
  body only). Classification that needs the status must read it in our own tower layer, below the
  crate's parsing.
- **Every Cerebras 4xx loses its status and code inside the crate (measured 2026-10-07).** Cerebras
  returns a *flat* error body, e.g. a 401 is
  `{"message":"Wrong API Key","type":"invalid_request_error","param":"api_key","code":"wrong_api_key"}`,
  while `async-openai` only parses OpenAI's wrapped `{"error": {…}}` form. So 401, 402, 429 and other
  4xx reach us as a deserialize error with no status. 5xx keep their status (the crate does not parse
  them). Reading the status in our own tower layer is therefore the main path, not an edge case.
  Provider error codes are stable **strings**, not numbers. Cerebras also documents **402 Payment
  Required** (out of credit), which the classification must place: it is neither our bug nor their
  outage.
- **The base URL prints in full in `Debug` output** (accepted, Jann's call): it never carries
  credentials, so it is not wrapped as a secret. If a URL ever needs userinfo, the secrets rule
  applies to it.
- **C5 availability carries the provider's error code** (`host_code`, a string; Jann's call). It
  is safe for the transition rule because provider codes are stable identifiers, but the value is
  no longer `Copy`, and because of the flat error body the code must be parsed in our own tower
  layer. Changing availability returns a new `BackendHealth` with the status replaced (Jann chose
  the immutable form over in-place mutation), and the backend publishes it on its watch channel. "Starts at unknown" is enforced by construction (the constructor takes no
  status). The catch-all reason is `Other`, and 402 gets its own reason.
- **The backend's module and types are named for the protocol, not the role** (Jann's call): the
  code says *OpenAI-compatible*, not *hosted*. "Hosted" is where the provider runs, which is a
  deployment fact; an Anthropic-compatible client would be hosted too and would collide, while
  Groq or OpenRouter would use this same adapter. "The hosted backend" stays the name of the
  *role* in docs and Jira (D85, ARIA-143). The startup log prints host and model next to
  `Backend::name()`, so the protocol name does not hide which provider is serving.
- **Any answer from the provider counts as evidence it is available** (Jann's call, replacing the
  plan's "other 4xx → no change"): 400/404/413/422, any other non-5xx/429/401/402/403 status, and a
  `GenerationFailed` all classify as available, because the provider answered. "No update" is left
  for outcomes where the provider was never reached. The classification takes the current status
  and also deduplicates (Jann's call): it returns "no update" both when an outcome does not affect
  availability and when the new status equals the current one. Tests therefore supply a current
  status per case. With no path left to it, the `Other` reason was removed.
- **C3 is a tower retry `Policy`** (Jann's call), not a hand-written layer: tower owns the loop,
  readiness and re-sending, and clones the policy per request, so the attempt count is per request.
- **C3 details:** a `Retry-After` given as an HTTP date counts as above the cap (give up, pass the
  429 through), since a date nearly always means "much later". The backoff gets a small random
  jitter so retries across pods don't run in lockstep after a provider blip.
- **The `Retry-After` cap is 10 s, not the planned 5 s** (Jann's call). Consequence, also under
  Risks: 3 retries × 10 s equals the 30 s idle window, so a provider asking for long waits can end
  the turn as a timeout rather than a rate limit.
- **Tower stack order, outside in: status capture → retry → per-attempt tracing → reqwest.** Because
  Cerebras's flat error body makes the crate lose the status, a layer must turn a non-2xx raw
  response into the HTTP error (status, message, provider code) before the crate parses it. It must
  sit **outside** retry: inside, retry would see an error instead of a 429/5xx response, and
  `should_retry` would not retry it, so retries would silently stop working.
- **C6 adds two contracts (Phase 2):** the call span must be a **child of the `Decide` span** and
  stay attached while the `'static` stream is polled (create it inside the call, instrument the
  stream with it). Its duration must cover the stream from open to end, because tower layers end at
  the response headers and cannot measure stream lifetime. Tests: parent is the `Decide` span; the
  span closes only after the last chunk. Per-attempt spans live in `spans.rs`; the call span lives
  with the stream in the backend module. Status capture and per-attempt tracing are hand-written
  `Layer` + `Service` pairs (Jann's call): tower's sync helpers can't read a body, and `tower-http`'s
  `TraceLayer` does not speak the crate's request factory.
- **Metrics stay out of this story:** cross-turn questions (in-flight streams, p95 time to first
  token) need an OTel metrics pipeline in `crates/telemetry`, which touches every service. Candidate
  follow-up under `theme-observability`.
- **C2 (the fake provider) was written in full by the code-agent** at Jann's request, as test
  infrastructure, not by Jann in Phases 2 and 4. Its default stream is modelled on the
  OpenAI-compatible format, not a captured Cerebras stream; the Risk about drift stays open until
  a capture replaces it.
- **API-review follow-ups (2026-10-19).** The code-agent fixed the Smells and the `Backend` trait's
  "do not retry" doc. Jann fixed the classification's catch-all arm (Bug 2). At Jann's request the
  code-agent also applied the remaining Improvements (too provider-specific to be worth hand-shaping):
  `StreamError` carries an optional cause, an optional provider code and a description, so it covers
  cut streams, provider error events and truncation; tests get a test-only constructor with an
  explicit backoff base; `request`, `retry` and `spans` became crate-private, so only the constructor
  assembles the stack; the idle-timeout getter returns `std::time::Duration`.
- **API gate passed (2026-10-19)** on the second review: 0 Bugs, 0 Improvements. The last Smells
  were fixed before the API commit, including the `stream_conversation` doc, which had contradicted
  C4 on reasoning deltas resetting the idle timeout. Config getters follow one rule: `llm_url`,
  `llm_model`, `llm_idle_timeout`, `llm_max_retries`, with `llm_api_key` as the recorded exception.
- **C1 field naming:** the key's field keeps `llm_api_key` while the others are `llm_*` (Jann's
  call: it mirrors the variable `ARIA_LLM_API_KEY`).
- Secrets rule gap found outside this story: `crates/identity` reads the private key file into a
  plain `String`. Left for whoever next touches it (likely ARIA-181), per the rule.
- **Phase 4 delegation (2026-10-22).** Jann wrote C1, request mapping, the C3 structure and `build()`.
  At Jann's request the code-agent finished the C3 details (three-way `Retry-After`, overflow-safe
  jittered backoff), wrote both layer `call` bodies, completed `stream_conversation`'s error
  mapping, availability and spans on top of Jann's loop, and wrote C5 and C7.
- **C3 backoff:** the first retry waits the base, doubling after; equal jitter (half to full
  nominal); bounded by `MAX_BACKOFF` (30 s), which only matters at a high `max_retries`. A
  `Retry-After` that is present but unparsable gives up, like one above the cap; only a missing
  header falls back to backoff. Whether a timeout while *opening* is retried is not pinned: the
  idle window covers opening and its retries together, so it ends as a timeout either way.
- **Status capture keeps no raw body.** A non-2xx body is parsed (wrapped or flat form) for message
  and string code only; a non-JSON body falls back to the status text, so a provider's HTML error
  page never reaches a log. Attempt spans are `http_attempt` with `http.response.status_code` or
  `error.type` (`connect_error` / `request_error`).
- **C4 ending rules (Jann's call on content filter):** `stop` is a clean end; `length` after text is
  a clean (shortened) end, `length` with no text is `GenerationFailed(LengthLimit)`; a
  `content_filter` finish is **always** `GenerationFailed(ContentFilter)`, even after text, because
  part of the answer was withheld. Tool-call deltas or a tool-call finish are `UnexpectedToolCall`.
  A timeout after `stop` counts as complete. Empty text deltas are skipped.
- **Availability is published once per turn**, at its real end (completed reply → available; error
  → the classification), not on the first event, so a turn the client cancels leaves it unchanged.
- **The call span opens on the stream's first poll**, not in `stream_conversation`, because the
  service calls `stream_conversation` outside the `Decide` span but polls inside it. Span name
  `chat` with `otel.name` = `chat {model}`; `gen_ai.provider.name` = `openai` (the protocol) and
  `server.address` = the configured host; `gen_ai.response.finish_reasons` is a lowercase string,
  since `tracing` cannot record arrays.
- **A mid-stream provider error is recognised by its error-object shape** (wrapped or flat); the
  real Cerebras shape is unverified until the C8 live run. A non-error unreadable event stays
  attached as the `StreamError` cause, so a full error chain can contain reply text (accepted,
  Jann's call: log handling stays controllable for data regulation).
- **First live run (2026-10-22) found a fake-versus-real gap: Cerebras rejects
  `stream_options.include_obfuscation`** with `400 wrong_api_format` ("property is unsupported"). The
  request now leaves it unset, so it is omitted from the JSON; a test pins that the request carries
  only `include_usage`. The run also confirmed, against the real provider, that the flat 4xx body
  survives status capture with its status and code, that a 400 is not retried, and that it counts as
  the provider being available.
- **C8 result (2026-10-22):** the key-gated test passed against Cerebras (`qwen-3.8-27b`) in 17.2 s:
  the first chunk arrived strictly before the end of a long reply, with more than one chunk. Without
  a key it failed, naming `ARIA_LLM_API_KEY`, rather than skipping. The key was read from the macOS
  Keychain, never typed on a command line. A manual `Decide` through the binary also streamed a
  multi-turn reply, with about 2.3 s to the first text (the reasoning phase, D85) and the usage
  event parsing without error.
- **Milestone review (2026-10-23), fixed before shipping:**
  - A provider that accepts the request and then hangs up counts as unreachable, not "never
    reached".
  - async-openai's own `async_openai::error` log line, which prints an unparsable event raw (reply
    text included), is switched off in both the stdout and the export filter, beyond `RUST_LOG`'s
    reach. The muted target list lives in the shared `crates/telemetry`, so every service gets
    it; there is no per-service switch.
  - An error after a `stop` finish completes the turn, like a stall does.
  - A trailing slash on the base URL is stripped.
  - The flat/wrapped error-object parser exists once, in a new `status.rs` that holds status
    capture.
  - Connection failures use one `error.type` vocabulary on both span levels.
- **`health.rs` speaks HTTP** (accepted, recorded at review): `UnavailabilityReason` names
  401/403, 402, 429 and 5xx and carries `http_code`, although Phase 1 kept `BackendError` thin to keep
  provider concepts out of backend-neutral code. Accepted for now because the reasons are what an
  operator needs on the diagnostic entry. *Revisit when* a second backend (Ollama, ARIA-54) needs
  reasons that are not HTTP-shaped.
- **Workspace-wide OpenTelemetry bump** (in the API-shape commit, 4e4abdd): `tracing-opentelemetry`
  0.33 → 0.34 and `opentelemetry*` 0.32 → 0.33, kept in step with each other. It touches every
  service's telemetry, and every service's tests passed with it.


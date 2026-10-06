---
title: Stream Decide from the Agent Core with in-memory session history (ARIA-131)
slug: aria-131-decide-streaming
tier: technical
status: active
created: 2026-10-05
branch: aria-131-decide-streaming
owns_branch: true
---

## Goal

`aria-agent-core` serves `Decide` over gRPC. For each call it takes the session's history so far,
sends that plus the user's new text to an LLM backend, and streams the reply back to the caller as
text chunks followed by a turn-complete marker. A turn that finishes cleanly is remembered, so the
next `Decide` on the same `session_id` passes the earlier exchange to the backend. A backend failure
ends the stream with an error status, is not retried, and is recorded in telemetry. The binary runs
with a deliberately trivial `echo` backend, so the image ARIA-123 publishes is a live Agent Core for
the walking skeleton. Story: [ARIA-131](https://firephoenixgames.atlassian.net/browse/ARIA-131).

## Non-goals

- A real LLM provider. The hosted backend is ARIA-143, and it builds on the backend contract set here.
- Persisting history to the Knowledge Core (D36). History lives in process memory and is lost on restart.
- Evicting or expiring sessions. Memory grows with the number of sessions.
- Tools, tool-call events, retrieval and consent prompts.
- Using `speaker_name`. It is accepted and ignored.
- Queueing or cancelling a competing turn on the same session. Barge-in cancellation belongs to the
  caller (D29/D30 side).
- Authenticating the caller or verifying end-user context.

## Constraints

- The wire contract is the existing `proto/aria/agent_core/v1/agent_core.proto` (ARIA-130, D87).
  This story does not change it.
- `Decide` is server-streaming (D37). Text reaches the caller as it is produced, not after the
  backend finishes.
- **In-memory history is an interim, not the design.** D36 rejected "session ID with in-memory
  history only" as the end state. This story ships it only because persistence is out of scope. The
  history component must sit behind a boundary that a Knowledge-Core-backed store can replace
  without touching the `Decide` handler.
- No retry of the backend call (D39's posture, applied here because a backend call will later carry
  tool calls).
- Telemetry follows D82: spans exported over OTLP to an in-cluster collector, logs on stdout. No
  collector exists yet, so **spans are exported only when an OTLP endpoint is configured** (the
  story's interim). Without one the service still runs and still logs to stdout.
- Definition of done includes OTel spans (D84). The Helm chart and migration parts of D84 do not
  apply here: there is no chart work in this story and no datastore.
- Workspace lints apply (`[lints] workspace = true`). `cargo clippy --workspace --all-targets -D
  warnings` stays clean.

## Decisions

- **Commit an exchange only when the turn finishes cleanly.** The user's text and the full reply are
  appended together, atomically, and only after the backend signals completion. A backend error or
  a cancelled client stream leaves history exactly as it was. Chosen because it is the only option
  that keeps history a strict user/assistant alternation, which every chat backend, including
  ARIA-143's OpenAI-compatible one, expects.
  Rejected: *append the user's text first, the reply on finish* (a failed turn leaves a dangling
  user message and the next turn sends two user messages in a row); *record partial replies as
  interrupted* (most faithful to what the user heard, but it introduces a turn-status concept that
  D61's turn log should own). Accepted cost: after a cancelled turn, ARIA has no memory of the half
  answer the user heard.
- **Failed turns are recorded in telemetry, not in history.** A backend error or client cancel marks
  the `Decide` span as an error, adds an event naming the outcome and the session, and emits an
  error-level log line on stdout. Chosen per Jann's direction to make the failure observable through
  OTel. Logs stay on stdout rather than OTLP, per D82.
- **A second `Decide` on a busy session is rejected, not queued.** While a turn is in flight on a
  session, another `Decide` for that session fails immediately with a distinguishable "busy"
  status, and the backend is not called. Chosen because barge-in cancellation is the caller's job,
  and a clear status is something the caller can act on.
  Rejected *for now*, not for good: *serialize per session* (correct, but an abandoned slow turn
  blocks the user's next one); *snapshot at start and commit on finish* (no blocking, but history
  can interleave and break the alternation the first decision bought). Revisit if the Gateway's
  barge-in design needs the Agent Core to cancel the in-flight turn itself.
- **Invalid input is rejected before any backend call.** An empty `session_id` or empty `text` returns
  an invalid-argument status. This is a cheap guard rather than a policy question.
- **The binary ships with an `echo` backend.** It streams the user's text back in a few chunks.
  Chosen over a library-only story because the image is already published (ARIA-123), the walking
  skeleton needs a live Agent Core, and the OTLP-or-not switch only has meaning at process startup.
  It is named so nobody mistakes it for a product backend, and startup logs which backend is
  serving. Rejected: *library only, stub `main`* (the OTel interim would drift into ARIA-143).
- **The history store is a trait now, not a concrete type** (amended after the API review on
  2026-10-05). The review found that the concrete in-memory store could not be replaced by a persisted
  one without touching the `Decide` handler, which breaks the constraint above. Jann chose to meet the
  constraint now rather than relax it. The store trait has asynchronous, fallible start, commit and
  abort. Each store defines its own turn handle as an associated type. The service is generic over the
  store, as it already is over the backend. The in-memory store is its first implementation.
  Rejected: *amend the constraint and let the persistence story change the signatures* (cheaper now,
  but pushes a handler change onto the next milestone).
- **Dropping a turn handle frees the session "eventually"; the mechanism is the store's choice.**
  `Drop` cannot await, so the trait states only the contract. The in-memory store releases
  synchronously. A persisted store chooses between a spawned release task and a lease with expiry.
  The lease also covers a pod crash, where no `Drop` runs; that is the persistence story's concern.
  Rejected for this story: *prescribe a spawned task* (couples the trait to a runtime); *prescribe a
  lease* (designs the Knowledge Core's locking before its API exists).
- **Commit before `TurnComplete`, and store failure is `UNAVAILABLE`.** Once commit can fail, the
  turn-complete marker is sent only after the commit succeeds. A failed commit ends the stream with an
  error in place of `TurnComplete`, even though the chunks were already delivered. That is better than
  telling the client "done" for an exchange that the next turn will not see. A store failure maps to
  `UNAVAILABLE` with a generic message, distinct from busy (`ABORTED`) and invalid input
  (`INVALID_ARGUMENT`).
- **The acceptance tests exercise the real gRPC surface.** They run against an in-process server and
  client, not by calling the handler directly, because AC3 is a statement about the status the
  caller receives, which only the wire proves.

## Open questions

None blocking. The busy-rejection and in-memory interim are recorded on the Agent Core page (C5) as
interim behaviour, not as decisions.

- **Where D49's canonical conversation and tool-call type lives once it crosses a service
  boundary** (persistence to the Knowledge Core under D36, the turn log under D61): a type in
  `crates/shared`, or a proto message plus a domain type inside each service that converts at the
  boundary. *Interim:* a small, typed conversation model internal to `agent-core`, designed to be
  extended with tool calls additively and moved later. No proto change in this story. The LLM probe
  defines no ARIA-owned shape (it is single-turn and speaks each vendor's wire format), so D49's
  "close to OpenAI's" is the only reference. To be settled by the story that first persists history.
- **System entries: stored, but not replayed.** The conversation model is a list of authored
  entries (user, assistant, system, and later tool call and tool result), not user/assistant pairs.
  Direction agreed for when the first system prompt arrives (D85's date/time injection, still open in
  `needs-decision.md`): system entries are **stored** for debugging, but stored ones are **not
  sent** to the backend. Only the fresh system entry for the current call is sent. Replaying stored
  entries would hand the model several contradictory "today is" statements. Rejected: *not storing
  them* (loses what the model was actually given); *storing only the static part* (splits one
  prompt into two mechanisms). Not implemented here, because this story sends no system prompt; the
  type only has to be able to represent one.

## Risks

- **The backend contract is ARIA-143's foundation.** If it cannot express what a streaming
  OpenAI-compatible client needs (chunks, a clean end and an error at any point, with the full
  ordered history as input), ARIA-143 reopens it. Early signal: the API review cannot say how a
  network error after the third chunk would be reported.
- **A cancelled stream that does not release the session.** If the busy marker is tied to the
  handler running to completion rather than to the turn being dropped, a client disconnect locks the
  session permanently. Early signal: the cancellation test hangs or reports busy.
- **Unbounded memory.** Accepted as a non-goal. The signal that it matters is a long-running
  skeleton pod whose memory climbs. Persistence replaces this store before that becomes real.
- **Telemetry setup that fails without a collector.** An exporter configured unconditionally would
  retry against nothing and spam stdout. Early signal: noisy startup with no endpoint set.

## Components

### C1 — Session history store

status: done

**Responsibility.** Own every session's conversation so far, and whether a turn is currently in
flight on it. It is the one place that decides whether a turn may start and what a finished turn
adds.

**Contracts.** A session never seen before reads as empty history. A turn that commits appends the
user's text and the reply as one exchange, after any earlier exchanges, in order. A turn that ends
without committing, including by being dropped mid-way, leaves the history unchanged **and frees
the session**. Starting a turn on a session that already has one in flight is refused in a way
callers can tell apart from every other failure, including a store failure. Sessions are independent:
a busy or failed turn on one has no effect on another. The store is shareable across concurrent
requests. Its boundary is a trait with asynchronous, fallible start, commit and abort, and a
per-store turn handle, so a persisted store can replace the in-memory one without changing callers.
Dropping a turn handle frees the session eventually, by whatever mechanism the store chooses.

**Tests.** A new session has empty history. After one committed turn, the next turn sees exactly
that exchange. After two, it sees both, oldest first. A turn ended without committing leaves the
history unchanged and lets a new turn start. A turn that is simply dropped also frees the session.
Starting a second turn while one is in flight is refused as busy. A busy session does not block a
different session.

**Done when.** Those tests pass and the linter is clean.

### C2 — LLM backend contract, scripted backend and echo backend

status: done

**Responsibility.** Define what the Agent Core asks of any LLM backend, and provide two backends: a
scripted one for tests and the trivial `echo` one the binary serves with.

**Contracts.** A backend receives the session's prior exchanges in order plus the new user text,
and produces a stream of text chunks. The caller must be able to tell a clean end from an error,
and an error can occur before the first chunk or after any number of chunks. The contract must be
implementable by a network-backed streaming client (ARIA-143) without changing it, which means it is
asynchronous and usable across tasks. The scripted backend replays a configured sequence of chunks,
can be configured to fail at a given point, and lets a test inspect exactly what it was called with
and how many times. The echo backend streams the user's text back in more than one chunk and ends
cleanly.

**Tests.** The scripted backend yields its configured chunks in order and then ends cleanly. One
configured to fail after N chunks yields N chunks and then an error. One configured to fail
immediately yields only the error. It records the history and text it received on each call, and its
call count. The echo backend yields at least two chunks whose concatenation is the input text, then
ends cleanly.

**Done when.** Those tests pass and the linter is clean.

### C3 — `Decide` service

status: done
depends-on: 1, 2

**Responsibility.** Implement the `AgentCore` gRPC service. Validate the request, reserve the
session, run the backend with the session's history, stream chunks to the caller as they arrive,
and on clean finish commit the exchange and then send the turn-complete marker. Map every failure to a
gRPC status, and record failures in telemetry.

**Contracts.** Chunks reach the caller as the backend produces them, followed by exactly one
turn-complete marker on success. A backend error ends the stream with an error status, the backend
is called exactly once (no retry), and history is unchanged. A client that cancels mid-stream
leaves history unchanged and the session free. A busy session yields a status that callers can tell
apart from a backend error and from invalid input. A commit that fails ends the stream with an
unavailable status in place of the turn-complete marker. Invalid input yields an invalid-argument status
without calling the backend. Each `Decide` runs inside a span. A failed turn marks that span as an
error, attaches an event naming the outcome (backend error or cancelled) and the session id, and
emits an error-level log record.

**Tests.** *(AC1)* One `Decide` against a scripted backend with three chunks delivers all three, in
order, then turn-complete. *(AC2)* A second `Decide` on the same session passes the first exchange
(user text plus the full concatenated reply) to the backend. *(AC3)* A backend failing after one
chunk delivers that chunk, then an error status. The backend was called once, and a following
`Decide` on the session sees no history. A backend failing before any chunk ends with an error
status and no chunks. A second concurrent `Decide` on a session with a turn in flight is refused as
busy, and the first completes normally. A cancelled client stream leaves the session with no history
and free for a new turn. An empty `session_id`, and separately empty `text`, return invalid argument
with zero backend calls. A backend failure emits an error-level record carrying the session id. A store
whose commit fails delivers the chunks, then an unavailable status, and no turn-complete marker.

**Done when.** Those tests pass over an in-process gRPC server and client, and the linter is clean.

### C4 — Service binary and telemetry setup

status: done
depends-on: 2, 3

**Responsibility.** Turn the crate into a runnable service: read configuration from the environment,
set up tracing (stdout always, OTLP span export only when an endpoint is configured), and serve
`Decide` with the echo backend.

**Contracts.** Configuration covers at least the listen address and the optional OTLP endpoint,
with a sensible default listen address. An invalid listen address fails at startup with a clear
message rather than at first request. With no OTLP endpoint, no exporter is created and nothing
tries to reach a collector. With one, spans are exported to it. Startup logs which backend is
serving and whether span export is on. The process shuts down cleanly on SIGTERM, which pods depend
on.

**Tests.** Config with no OTLP endpoint resolves to export-off. Config with an endpoint resolves to
export-on at that endpoint. An unparsable listen address is a configuration error. An unset listen
address uses the default. *(Smoke, by hand, recorded in the commit body:)* `cargo run -p
agent-core` serves, and one `Decide` via `grpcurl` streams echoed chunks.

**Done when.** Those tests pass, the smoke run works, and the linter is clean.

### C5 — Agent Core page: interim behaviour

kind: chore
status: done
depends-on: 3, 4

**Responsibility.** Update `docs/services/agent-core.md` so the library matches what runs. Record
the in-memory history as an interim pending D36's persistence, busy-rejection for concurrent turns
on one session (interim, with the barge-in caveat), commit-on-clean-finish, and the `echo` backend
plus the OTLP-only-when-configured switch. These are interim behaviour, not decisions, and the
page says so. Then check whether Confluence or Jira (ARIA-143) need the same note, per rule 6.

**Done when.** The page states each interim with its replacement trigger, and nothing on it reads as
a new architecture decision.

## Deviations

- C1: `start_turn` returns the owned history snapshot alongside the turn, so a handler that is
  generic over the store can pass history to the backend without naming the in-memory token. It
  also removes a second clone per turn. Found in API review round 2.
- C1: the in-memory store checks that a turn was issued by the same store (identity of its shared
  map) before committing or aborting. A foreign turn is refused as `InvalidTurnOwner`, which maps to
  `UNAVAILABLE` like every commit failure.
- C1: generic callers get no `#[must_use]` lint on the associated turn type, so holding the turn for
  the whole turn is a documented obligation on `start_turn`. `#[must_use]` sits on the in-memory
  `Token`.
- C3: serving requires the store to be cheaply `Clone`, because a clone moves into the `'static`
  response stream to commit after the last chunk. `InMemoryHistory` satisfies this through its shared `Arc`.
- C4: the pure constructor takes a lookup by variable name in place of two positional values, so the
  arguments cannot be swapped. A non-UTF-8 value is an error rather than "unset", and every error
  names the variable and the offending value.
- C3: telemetry names are pinned so tests can observe them at the `tracing` layer, without an
  in-test OTel exporter. The span is `decide`. A failed turn records `otel.status_code = "ERROR"` on
  it, and its failure event carries `outcome` (`backend_error` or `cancelled`) and `session_id`.
  Exporter wiring is C4's concern. The smoke run after the milestone review covered it with export
  on: export was attempted against an unused endpoint, and the service kept serving.
- C3: failure outcomes are `backend_error`, `cancelled`, `commit_failed`, `commit_interrupted`,
  `store_error` (which includes a busy rejection) and `invalid_input`. Progress events use `stage`,
  so `outcome` only ever names a failure. (Amended after the milestone review.) The
  last two go beyond the plan's list: any `Decide` that does not end in turn-complete marks its span.
  Cancellation is reported by a drop guard owned by the response stream, because a cancelled stream
  is dropped rather than polled to an end.
- C3: the response stream enters the span on every poll through a small in-crate adapter, because
  `tracing::Instrument` covers futures only and the alternative was the `tracing-futures` crate.
- C3: a committed reply is one assistant entry whose adjacent text chunks coalesce into one part,
  so ARIA-143's tool calls can interleave later without breaking order.
- C4: `RUST_LOG` filters the stdout layer only, defaulting to `info`, so a quiet log level never
  thins exported spans. An invalid value warns and falls back. Not in the contract; added after the
  smoke run showed TRACE output from tonic, h2 and hyper by default.
- C4: the shutdown signal also handles Windows (Ctrl-C, Ctrl-Break, console close). That branch is
  not compiled in CI or locally yet, because no Windows target is installed.
- C1: the in-memory store is `InMemoryHistory`, renamed from `History` during implementation.
- C3 (after milestone review): a backend error maps to `INTERNAL`, not `UNAVAILABLE`, because default
  gRPC retry policies retry `UNAVAILABLE` and would re-run a failed turn, against D39's posture.
  A store or commit failure stays `UNAVAILABLE`, as decided above. A busy rejection is marked as a
  failed span (`store_error`), so error traces include ordinary barge-in races.
- C3 (after milestone review): a drop while a commit is in flight reports `commit_interrupted`,
  because the history may or may not hold the exchange. The store trait now asks for commit to be
  atomic under cancellation, which the persistence story must honour.
- C4 (after milestone review): exported spans have their own filter (this crate at `debug`,
  dependencies at `warn`), separate from `RUST_LOG`. ANSI colour is on only when stdout is a
  terminal. A serve error still flushes the tracer provider.

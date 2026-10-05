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
- **The acceptance tests exercise the real gRPC surface.** They run against an in-process server and
  client, not by calling the handler directly, because AC3 is a statement about the status the
  caller receives, which only the wire proves.

## Open questions

None blocking. The busy-rejection and in-memory interim are recorded on the Agent Core page (C5) as
interim behaviour, not as decisions.

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

status: pending

**Responsibility.** Own every session's conversation so far, and whether a turn is currently in
flight on it. It is the one place that decides whether a turn may start and what a finished turn
adds.

**Contracts.** A session never seen before reads as empty history. A turn that commits appends the
user's text and the reply as one exchange, after any earlier exchanges, in order. A turn that ends
without committing, including by being dropped mid-way, leaves the history unchanged **and frees
the session**. Starting a turn on a session that already has one in flight is refused in a way
callers can tell apart from every other failure. Sessions are independent: a busy or failed turn on
one has no effect on another. The store is shareable across concurrent requests. Its boundary must
be replaceable by a persisted store later without changing callers.

**Tests.** A new session has empty history. After one committed turn, the next turn sees exactly
that exchange. After two, it sees both, oldest first. A turn ended without committing leaves the
history unchanged and lets a new turn start. A turn that is simply dropped also frees the session.
Starting a second turn while one is in flight is refused as busy. A busy session does not block a
different session.

**Done when.** Those tests pass and the linter is clean.

### C2 — LLM backend contract, scripted backend and echo backend

status: pending

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

status: pending
depends-on: 1, 2

**Responsibility.** Implement the `AgentCore` gRPC service. Validate the request, reserve the
session, run the backend with the session's history, stream chunks to the caller as they arrive,
and on clean finish send the turn-complete marker and commit the exchange. Map every failure to a
gRPC status, and record failures in telemetry.

**Contracts.** Chunks reach the caller as the backend produces them, followed by exactly one
turn-complete marker on success. A backend error ends the stream with an error status, the backend
is called exactly once (no retry), and history is unchanged. A client that cancels mid-stream
leaves history unchanged and the session free. A busy session yields a status that callers can tell
apart from a backend error and from invalid input. Invalid input yields an invalid-argument status
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
with zero backend calls. A backend failure emits an error-level record carrying the session id.

**Done when.** Those tests pass over an in-process gRPC server and client, and the linter is clean.

### C4 — Service binary and telemetry setup

status: pending
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
status: pending
depends-on: 3, 4

**Responsibility.** Update `docs/services/agent-core.md` so the library matches what runs. Record
the in-memory history as an interim pending D36's persistence, busy-rejection for concurrent turns
on one session (interim, with the barge-in caveat), commit-on-clean-finish, and the `echo` backend
plus the OTLP-only-when-configured switch. These are interim behaviour, not decisions, and the
page says so. Then check whether Confluence or Jira (ARIA-143) need the same note, per rule 6.

**Done when.** The page states each interim with its replacement trigger, and nothing on it reads as
a new architecture decision.

## Deviations

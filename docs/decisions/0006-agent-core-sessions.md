# ARIA — Decision Log · Agent Core, sessions & conversation

**Decisions D35–D40** · taken 2026-08-12 (session 1)

One entry per decision: what was decided, why, what was rejected and why, and which stories it
affects. Part of [the decision index](README.md). Several entries carry an explicit **revisit
trigger** — those are not settled forever, but must not be reopened until the trigger fires.

---

### D35 · Q12 — Failover plus an explicit override; automatic routing deferred (closes ARIA-44)

**Decided.** The hosted API remains the default. Ollama takes over on error or timeout. A per-request
or config-level override can force local. No automatic task-class routing yet.

**Why.** Task-class routing needs a quality threshold, and the measurement that would set it does
not exist — the Ollama model's actual tool-calling ability is the missing spike identified in triage
(B11). A routing policy resting on an unmeasured quality assumption is exactly the invented
certainty this backlog was built to avoid. The explicit override still provides a lever without
pretending to a policy.

**Rejected.**

- *Task-class routing now* — real cost and privacy gains for everyday turns, but rests on an
  unmeasured assumption.
- *Failover only* — nothing to get wrong, but the GPU capacity §2 wanted to use sits idle outside
  outages, with no lever at all.

**Revisit trigger.** Once B11 measures local tool-calling reliability, automatic routing becomes
decidable. This decision should be reopened then, not inherited.

**Affects.** ARIA-44 (closed by this), ARIA-56, ARIA-54, ARIA-53.

---

### D36 · X5a — The Gateway mints a `session_id`; the Agent Core owns history and persists it to the Knowledge Core

**Decided.** A session ID is minted at stream start and carried on every downstream call. The Agent
Core holds the message history for that session and persists it to the Knowledge Core.

**Why.** Persisted conversation history is exactly "what ARIA has learned from past interactions",
which the vision already places in the Knowledge Core. It gives D29's reconnect something to resume
into, and makes mid-conversation re-retrieval possible.

**Rejected.**

- *Stateless, Gateway sends full context each turn* — simplest scaling and failure story, but puts
  knowledge-shaped data in the transport layer and re-sends the conversation every turn.
- *Session ID with in-memory history only* — fastest on the turn path, but a pod restart silently
  drops every conversation and nothing reaches the Knowledge Core.

**Affects.** ARIA-48, ARIA-50, ARIA-58, ARIA-34, ARIA-36, ARIA-39, ARIA-108. Newly revealed:
conversation-history schema and accessors in the Knowledge Core are not covered by any story, and
X10's retention question now applies to conversation history too.

---

### D37 · X5b — `Decide` is server-streaming

**Decided.** `Decide` streams: it emits text chunks and tool-call events rather than returning one
complete response. The `LlmBackend` trait signature follows.

**Why.** Speech can begin synthesising the first sentence while the model is still generating — the
largest latency win available on a voice path, and latency is what the product is judged on.

**Accepted cost.** A more complex trait and tool loop, since tool calls interleave with partial
text, and the backend-agnostic conformance suite (ARIA-50) must exercise streaming.

**Rejected.**

- *Unary* — much simpler to build, test and reason about, but the user hears silence for the entire
  duration including every tool round-trip.
- *Unary now, streaming later* — the proto, trait, tool loop and Speech contract all change when it
  flips; cheap now, expensive to retrofit.

**Affects.** ARIA-48, ARIA-50, ARIA-53, ARIA-54, ARIA-56, ARIA-58, ARIA-61, ARIA-51.

---

### D38 · A5 — No MCP SDK in the Agent Core

**Contradiction resolved.** The Confluence Agent Core page's `rmcp` dependency is an error. The
Agent Core speaks only gRPC to the MCP Registry; tool schemas cross that boundary as JSON Schema
carried in the proto contract.

**Why.** Keeps exactly one path to MCP servers, which is what makes the consent enforcement claim
true rather than aspirational.

**Rejected.**

- *Direct connections for hot-path tools* — lower latency, but creates a second route that bypasses
  consent enforcement and audit.
- *SDK types only, no transport* — avoids hand-modelling schemas, but couples the Agent Core's build
  to an MCP SDK version for type definitions the proto can carry itself.

**Affects.** ARIA-61, ARIA-48, ARIA-92, ARIA-69. Requires an edit to the Confluence Agent Core page.

---

### D39 · G2 — Read-only calls retry; tool calls never auto-retry

**Decided.** Retrieval, `ListTools` and other read-only calls retry with backoff. Anything that can
execute a tool fails to the user instead.

**Why.** A double-executed side-effecting tool call is a far worse failure than one that didn't
happen and said so — and the user can simply ask again.

**Rejected.**

- *Idempotency keys with Registry deduplication* — the correct end state and best resilience, but
  real work needing a dedup window and storage decided first. Explicitly deferred, not dismissed.
- *Retry everything with backoff* — most resilient to transient faults, and the exact failure this
  question exists to prevent.

**Affects.** ARIA-34, ARIA-95, ARIA-61.

---

### D40 · A1 — "Unrecognized" is defined structurally

**Decided.** An interaction is logged as unrecognized when **no successful tool call was made AND
retrieval returned nothing above the relevance threshold**. Separately, every tool-loop iteration-
bound exhaustion is logged regardless.

**Why.** Two measurable conditions, neither depending on the model self-reporting confidence — the
least reliable signal available, and one that would differ between the hosted and Ollama backends
(D35). Gives ARIA-63 a testable definition and produces a log that is directly actionable: these are
the gaps where a new MCP server would help, which is the whole point of the vision's self-extension
loop.

**Rejected.**

- *The model says it couldn't help* — closest to the user's experience of being failed, but backend-
  dependent and unreliable.
- *Tool-loop exhaustion only* — narrowest and least noisy, but misses the most interesting case: a
  request where no tool was even plausible.

**Affects.** ARIA-63, ARIA-108, ARIA-61.

---

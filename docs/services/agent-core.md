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

`Decide` (server-streaming) — `proto/agent-core/v1/agent_core.proto`

## Open items

- **Awaiting measurement:** hosted provider/model (ARIA-40); which Ollama model (ARIA-109) —
  this gates whether the backend-agnostic conformance suite passes unmodified, and gates reopening
  the automatic-routing rule
- Whether the serving backend is exposed in the `Decide` response body or kept to traces only
- Prompt formatting, and whether retrieval re-runs mid-loop
- The numeric tool-loop iteration bound (deferred — C-4)

## Jira stories

ARIA-40, 48, 50, 53, 54, 56, 58, 61, 63, 109

---

*Derived from `03-architecture.md` and the Decision Log. Last updated 2026-08-19.*

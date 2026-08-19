# ARIA — Architecture (target state)

[Library index](README.md) · [Decisions](decisions/README.md) · [Open questions](open-questions/README.md) · [Sprint 1](sprints/sprint-01.md)

The services ARIA is composed of, what each owns, and what it replaces. One page per service
lives under [services/](services/) with the proposed internal structure.

**Source:** CLAUDE.md §3 as of 2026-08-12, moved here verbatim.

---

- **Gateway / Interface service** (`aria-gateway`) — client-facing gRPC endpoint (voice
  streaming + text/command input). Replaces the "Interface" box and the 3-socket client
  protocol. Client-agnostic by design. Owns: JWT validation, minting the signed end-user context
  (section 2), minting the **`session_id`** at stream start, Opus→PCM decoding, and stream
  lifecycle. A `Converse` stream is a **session carrying many turns**, not one turn. **Barge-in is
  supported.** On SIGTERM the Gateway stops accepting new streams, sends a "session ending,
  reconnect" control frame on active ones, and closes after a short grace period (~30 s).
  Read-only downstream calls retry; **anything that can execute a tool is never auto-retried** —
  a double-executed tool call is a worse failure than one that didn't happen and said so.
- **Speech service** (`aria-speech`) — STT/TTS only, **English only**. Scheduled on the same GPU
  node already running Ollama; mind GPU memory budget between whatever model Ollama has loaded
  and the STT/TTS models. **Owns VAD and utterance endpointing**, emitting utterance-boundary
  events upstream — the Gateway stays pure transport. Also owns **TTS text normalisation**
  (numbers, dates, units), starting from whatever the chosen TTS library provides and adding
  rules only where it demonstrably fails. Consumes mono s16le PCM. Replaces `Speech2Text` /
  `Text2Speech`.
- **Agent Core (DEC)** (`aria-agent-core`) — the orchestrator. Per request: (1) retrieve
  relevant context from the Knowledge Core, (2) call the pluggable LLM (hosted API default,
  Ollama fallback) with the current MCP toolset, (3) execute whichever MCP tool call the model
  chooses **via the MCP Registry** (it holds no MCP client of its own), (4) stream the response
  back. Owns per-`session_id` conversation history and persists it to the Knowledge Core. Also
  owns the **consent-prompt path**: turning the Registry's `consent required` into a question in
  the conversation, and retrying the suspended call on approval. Replaces the separate hand-built
  `NLU`/`NLG` boxes. An interaction is logged to the Knowledge Core as **unrecognized** when no
  successful tool call was made *and* retrieval returned nothing above the relevance threshold;
  tool-loop iteration-bound exhaustion is always logged (replaces the old `Unknown` fallback box).
- **MCP Registry** (`aria-mcp-registry`) — holds the list of connected MCP servers (each its own
  pod/service in the cluster, or a remote HTTP endpoint on the LAN), aggregates their
  tools/resources, and exposes the dynamic registration API ("tell ARIA to connect to this").
  The **only** component that speaks MCP. Also the **enforcement point** for both gates: the
  approval check against `approved-servers.yaml` before registration, and, before forwarding any
  tool call, the Keycloak-level consent read (30 s TTL, fail-closed) and the per-tool grant in
  the Knowledge Core. Injects the reserved `_aria_user_id` argument. Filters `ListTools` by
  server. Writes every invocation attempt to `consent_audit_log`.
- **Knowledge Core** (`aria-knowledge-core`) — a **deployed service** that owns Postgres and
  Qdrant. Replaces `DB-ARIA`, `DB-ARIA-User`, `DB-Connect`, and the whiteboard graph datastore.
  Postgres holds the typed social graph (`person`, `relationship`), typed preferences, the
  calendar cache, `mcp_tools`, `consent_grants`, `consent_audit_log` and conversation history;
  Qdrant holds semantic memory and free-text preferences. **Validates writes itself** rather than
  trusting the Registry's gate: every row carries a `source` identifying the server that wrote it,
  a server may only modify rows it authored, and the KC areas it may touch are declared at
  registration and fixed by the approval gate. **The calendar is a mirror, not a source of
  truth** — an external calendar (Google/CalDAV) is authoritative, the KC caches materialised
  occurrences only, and ARIA never implements recurrence expansion, timezone arithmetic or
  attendee state. Calendar writes go through a calendar MCP server.
- **Identity** (`aria-identity`, a library crate, not a deployed service) — thin shared
  middleware around the existing Keycloak instance's token validation and client-credentials
  flow, plus minting and verifying the Gateway's signed end-user context. No user-service to build.
- **Keycloak provisioning broker** (`aria-kc-broker`) — a small deployed service holding the only
  Keycloak admin credential in the cluster, exposing exactly one narrow operation (create/reconcile
  an MCP server's `aria:mcp:` scopes) and refusing anything outside that prefix. Deliberately
  separate from the MCP Registry, which handles model-influenced traffic.
- **Self-Extension MCP Server** (first MCP server to build, lives outside the core repo like any
  other MCP server) — lets ARIA draft, generate, test, and — only after Jann approves — deploy
  new MCP servers. Proposed tools: `draft_service`, `generate_code`, `run_tests`,
  `request_approval`, `deploy_service`. The approval gate: `request_approval` opens a PR adding the
  server to `approved-servers.yaml` with its image digest; merging it is the approval; a CI step on
  merge builds the image, seals the service's credentials, and calls the MCP Registry's
  `RegisterServer`. Open question: PR review vs. a conversational approval flow for smaller runtime
  permission grants — see section 8 and the "ARIA — MCPs" page.
- **Trainer** — deferred, not in initial scope.

---

## See also

- [Service pages](services/) — per-service detail
- [Technology stack](02-stack.md) — the choices behind these boundaries

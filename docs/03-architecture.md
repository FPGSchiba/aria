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

## Amendments from the 2026-09-23 design conversation

The list above is preserved verbatim as of 2026-08-12. Three things changed: **clients entered the
architecture**, **two services were added**, and **the datastores left the cluster**.

### Clients are part of the architecture (D60–D65)

[D60](decisions/0012-clients-gateway-surface.md) puts clients inside the boundary rather than
treating them as an afterthought: **native phone and desktop applications** speaking gRPC
bidirectional streams, and a **Core Web shell** in the browser on Authorization Code + PKCE. The
Gateway remains the only ARIA component a client can reach.

The Gateway therefore grows a **second front door**
([D62](decisions/0012-clients-gateway-surface.md)). gRPC-web cannot do client or bidirectional
streaming, which is the forcing fact: a browser cannot hold the `Converse` stream. So there are two
surfaces — the **gRPC stream for turns**, and **HTTP for everything else** (history, preferences,
consent management, extension UIs). Routing between them is **by path prefix**, and the Gateway
**never decodes a request body to decide where it goes**
([D63](decisions/0012-clients-gateway-surface.md)).

Both doors mint the same thing: the Gateway stays the **sole minter of end-user context**
([D64](decisions/0012-clients-gateway-surface.md)), so nothing downstream learns which door a
request came through — except where that difference is deliberate.
[D65](decisions/0012-clients-gateway-surface.md) makes that explicit: two paths into one service
may carry **different gates**, and a read the user performs on themselves is not the same act as a
tool reading on their behalf.

**The turn log becomes product data** ([D61](decisions/0012-clients-gateway-surface.md)) — the
user-facing record of what ARIA did, not an implementation detail of the Agent Core's conversation
history. [D83](decisions/0016-observability-v2.md) adds the rule that follows from it: the
**user-facing action timeline is built from the turn log, never from traces**.

### The extension surface (D66–D70)

An MCP server may now contribute **more than tools**. It may ship a **UI bundle**, approved by
digest in the same PR as its image ([D66](decisions/0013-extension-surface.md)) — one approval, one
artifact, no second trust path. That UI runs on a **separate origin in a sandboxed iframe**,
communicating over `postMessage` ([D67](decisions/0013-extension-surface.md)), and receives an
**audience-limited token**, never the user's Keycloak token
([D68](decisions/0013-extension-surface.md)).

The **MCP Registry gains a role**: it is the **service catalog**, and it publishes the route table
the Gateway's HTTP door uses ([D69](decisions/0013-extension-surface.md)).

And the boundary holds in the other direction: **MCP servers get no direct Knowledge Core access**
([D70](decisions/0013-extension-surface.md)). The one thing they legitimately need — resolving
which person they are acting for — is a narrow **`ResolvePerson`** call behind its own consent
scope. [D77](decisions/0015-external-datastores.md) then makes this structural rather than a rule
anyone must remember: the Knowledge Core lives in its **own database**, so a cross-schema grant to
it cannot be expressed at all.

### Two new services

- **Notification service** (`aria-notify`) — [D71](decisions/0014-notifications.md). Delivers over
  a live client stream when the Gateway holds one, otherwise over an offline push path. The
  service itself is **stateless**; the inbox and device registrations live in the Knowledge Core.
  Proactivity has been differentiator 1 in the vision since the beginning and had no mechanism
  until now. [D72](decisions/0014-notifications.md) gives it a leash in the same breath: **sending
  a notification is a gated capability** — its own consent scope, quiet hours read from typed
  preferences, a rate limit, a per-server mute, and an audit row per send.
  See [services/notify.md](services/notify.md).
- **Storage provisioning broker** (`aria-storage-broker`) — [D75](decisions/0015-external-datastores.md).
  Issues a newly approved MCP server's Postgres schema and grants at **approval time**, and
  nothing else. Deliberately a **dumb reconciler**: it takes no decisions, reads the approval
  artifact, and refuses anything outside its pattern. Same shape and the same reasoning as
  `aria-kc-broker` — a narrow service holding a credential the Registry must never hold.
  See [services/storage-broker.md](services/storage-broker.md).

### The Knowledge Core no longer owns in-cluster datastores

The bullet above says the Knowledge Core "owns Postgres and Qdrant", both as in-cluster
deployments. Both moved out. **Postgres runs on its own VM** with a schema per service and a
`*_owner` / `*_app` role split ([D73](decisions/0015-external-datastores.md)); **Qdrant runs on its
own VM** ([D78](decisions/0015-external-datastores.md)). The Knowledge Core is still the service
that *fronts* them, and still validates writes itself (D22) — what changed is where the bytes live
and who backs them up.

Two consequences worth naming here rather than leaving in the decision file:

- The **calendar cache relocates** ([D79](decisions/0015-external-datastores.md)). This supersedes
  **D20's placement only** — the calendar is still a mirror, writes still go through a calendar MCP
  server, and ARIA still implements no recurrence expansion.
- **Cross-schema reads are read-only views** ([D74](decisions/0015-external-datastores.md)), with
  `ALTER DEFAULT PRIVILEGES` set and `PUBLIC` revoked, so a new table is not accidentally world-
  readable the moment it is created.

### Observability

Unchanged in instrumentation, changed in destination —
see [02-stack.md](02-stack.md#from-the-2026-09-23-design-conversation-d60d84) and
[D82](decisions/0016-observability-v2.md). The point that belongs on this page:
[D83](decisions/0016-observability-v2.md) requires trace context to survive **four hops**, and
three of them are manual. One of those three is **the MCP boundary**, and the `traceparent` header
must live in the **generated-server template** the Self-Extension server produces — otherwise every
server ARIA writes for herself silently breaks the trace at the most interesting hop.

---

## See also

- [Service pages](services/) — per-service detail
- [Technology stack](02-stack.md) — the choices behind these boundaries

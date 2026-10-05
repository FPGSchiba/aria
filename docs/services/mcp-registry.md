# `aria-mcp-registry` — MCP Registry

[Library index](../README.md) · [Architecture](../03-architecture.md) · [Decisions](../decisions/README.md) · [Open questions](../open-questions/README.md)

**Deployed service · `services/mcp-registry/`**

---

## Amendments since 2026-08-12

- **[D57](../decisions/0010-retention-audit.md)** — the audit write is **two rows per invocation**:
  an **attempt** row appended **synchronously to durable local storage in this service before the
  call is forwarded**, and an **outcome** row appended after it returns; a background task drains
  the local outbox into the Knowledge Core. On an *allowed* call the write retries, then falls back
  to the durable queue, and **fails the call closed only if durability cannot be achieved anywhere**
  — loudly. **Consequence: this service is no longer stateless.** It needs durable local storage
  (a PVC or equivalent) plus drain, ordering and duplicate handling, which `04-deployment.md`'s
  "can run on any node" line did not anticipate. Closes ARIA-95's audit-write-failure clause; its
  reconnect/circuit-breaking and long-call timeout clauses stay open.
- **[D58](../decisions/0010-retention-audit.md)** — audit rows are **self-contained**; the trace ID
  is best-effort correlation, expected to dangle beyond the telemetry window.

---

## Amendments from 2026-09-23 (D69, D70, D75, D83)

- **[D69](../decisions/0013-extension-surface.md)** — the Registry is also the **service catalog**,
  and **publishes the route table** the Gateway's HTTP front door
  ([D62](../decisions/0012-clients-gateway-surface.md)) uses to reach extension UIs. This is new
  responsibility, not a restatement: the Registry already knew which servers exist, and now that
  knowledge has a consumer outside the tool-call path.
- **[D66](../decisions/0013-extension-surface.md)–[D68](../decisions/0013-extension-surface.md)** —
  an approved server may ship a **UI bundle**, approved by digest in the **same PR** as its image.
  The Registry's approval check (D44/D45) therefore covers two artifacts, not one. The UI runs on a
  **separate origin in a sandboxed iframe** and receives an **audience-limited token**, never the
  user's Keycloak token.
- **[D70](../decisions/0013-extension-surface.md)** — MCP servers get **no direct Knowledge Core
  access**. The one legitimate need — knowing which person they act for — is a narrow
  **`ResolvePerson`** call behind **its own consent scope**, which the Registry gates like any
  other. [D77](../decisions/0015-external-datastores.md) then makes this structural: the Knowledge
  Core is in its own database, so the grant that would bypass it cannot be written.
- **[D75](../decisions/0015-external-datastores.md)** — a second broker exists.
  **`aria-storage-broker`** provisions an approved server's Postgres schema and grants, and the
  Registry **must not hold that credential either** — the same reasoning that produced
  `aria-kc-broker` (D8), applied to storage. See
  [storage-broker.md](storage-broker.md).
- **[D83](../decisions/0016-observability-v2.md)** — the `traceparent` header on the
  **streamable-HTTP call to an MCP server** is one of three manual trace-propagation points, and it
  must also be in the **generated-server template**. This is the hop where "what did the tool do"
  is asked, and it is the one most easily forgotten.

---

## Purpose

The **only** component that speaks MCP. Holds the list of connected MCP servers, aggregates their
tools/resources, and exposes the dynamic registration API — "tell ARIA to connect to this".

## Replaces

Nothing in the 2021 design — this is where the extensibility claim is actually implemented.

## Owns

- All MCP connections (via the shared `mcp-client` crate). Keeping every connection here is what
  makes the consent-enforcement claim true rather than aspirational — a second connection path
  would bypass both gates.
- **Enforcement point for both gates** before forwarding any tool call:
  1. **Approval** — the server's code is trusted at all: read from `approved-servers.yaml` (D44),
     identity is the container **image digest** with a stable slug as the registry key (D45)
  2. **Consent** — *this user* allows *this server/tool*: Keycloak-level read with a **~30 s TTL,
     fail-closed** (D43) plus the per-tool grant in `consent_grants` (D7)
- Injecting the reserved `_aria_user_id` argument — stripped from the schemas shown to the model
  and overwritten unconditionally if present (D9)
- Filtering `ListTools` **by server, not by tool** (D46)
- Writing every invocation attempt to `consent_audit_log`, allowed or denied — as an attempt row
  before forwarding and an outcome row after, through a durable local outbox (D57)
- Calling `aria-kc-broker` to provision per-tool scopes (D8) — it never holds the Keycloak admin
  credential itself

## Binding decisions

D7 (per-tool scopes) · D8 (broker, not self) · D9 (reserved argument) · D11 (`mcp-client` crate) · D42 (`consent required` outcome) · D43 (30 s TTL, fail-closed) · D44 (approval artifact) · D45 (image digest identity) · D46 (server-level filtering) · D39 (never auto-retry) · D57 (two-row audit through a durable outbox)

See [the Decision Log](../decisions/README.md) for the full reasoning and rejected alternatives.

## Contract

`RegisterServer` / `DeregisterServer` / `ListTools` / `CallTool` — `proto/aria/mcp_registry/v1/mcp_registry.proto`. Proposed modules: `approval.rs`, `consent.rs`, `audit.rs`, `broker.rs`.

## Open items

- **Awaiting measurement:** the Rust MCP SDK (ARIA-65 — see [the brief](../spikes/ARIA-65-rust-mcp-sdk.md));
  the exact Keycloak API exposing a user's granted consents (ARIA-88)
- Tool-list drift: refresh cadence, and what happens to a grant when a tool's schema changes or the
  tool disappears — sharpened by D7, since drift now requires a Keycloak write (ARIA-75)
- Failure policies: reconnect/circuit-breaking for unreachable servers, timeout and streaming
  semantics for long tool calls (ARIA-95). *The audit-write failure clause is closed by D57.*
- The `consent required` status in the proto contract, and how suspend-and-retry interacts with a
  streaming `Decide` mid-turn (ARIA-118)
- How declared tools get a risk tier (ARIA-100) — needed for a useful consent prompt
- Whether consent grants expire, and whether Keycloak revocation cascades to `consent_grants`

## Jira stories

ARIA-65, 69, 72, 75, 78, 82, 85, 88, 90, 92, 95, 118

---

*Derived from `03-architecture.md` and the Decision Log. Last updated 2026-08-19.*

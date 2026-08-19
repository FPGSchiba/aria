# `aria-mcp-registry` — MCP Registry

[Library index](../README.md) · [Architecture](../03-architecture.md) · [Decisions](../decisions/README.md) · [Open questions](../open-questions/README.md)

**Deployed service · `services/mcp-registry/`**

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
- Writing every invocation attempt to `consent_audit_log`, allowed or denied
- Calling `aria-kc-broker` to provision per-tool scopes (D8) — it never holds the Keycloak admin
  credential itself

## Binding decisions

D7 (per-tool scopes) · D8 (broker, not self) · D9 (reserved argument) · D11 (`mcp-client` crate) · D42 (`consent required` outcome) · D43 (30 s TTL, fail-closed) · D44 (approval artifact) · D45 (image digest identity) · D46 (server-level filtering) · D39 (never auto-retry)

See [the Decision Log](../decisions/README.md) for the full reasoning and rejected alternatives.

## Contract

`RegisterServer` / `DeregisterServer` / `ListTools` / `CallTool` — `proto/mcp-registry/v1/mcp_registry.proto`. Proposed modules: `approval.rs`, `consent.rs`, `audit.rs`, `broker.rs`.

## Open items

- **Awaiting measurement:** the Rust MCP SDK (ARIA-65 — see [the brief](../spikes/ARIA-65-rust-mcp-sdk.md));
  the exact Keycloak API exposing a user's granted consents (ARIA-88)
- Tool-list drift: refresh cadence, and what happens to a grant when a tool's schema changes or the
  tool disappears — sharpened by D7, since drift now requires a Keycloak write (ARIA-75)
- Failure policies: reconnect/circuit-breaking for unreachable servers, audit-write failure on
  *allowed* calls, timeout and streaming semantics for long tool calls (ARIA-95)
- The `consent required` status in the proto contract, and how suspend-and-retry interacts with a
  streaming `Decide` mid-turn (ARIA-118)
- How declared tools get a risk tier (ARIA-100) — needed for a useful consent prompt
- Whether consent grants expire, and whether Keycloak revocation cascades to `consent_grants`

## Jira stories

ARIA-65, 69, 72, 75, 78, 82, 85, 88, 90, 92, 95, 118

---

*Derived from `03-architecture.md` and the Decision Log. Last updated 2026-08-19.*

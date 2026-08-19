# `aria-identity` — Identity

[Library index](../README.md) · [Architecture](../03-architecture.md) · [Decisions](../decisions/README.md) · [Open questions](../open-questions/README.md)

****Library crate, not a deployed service** · `crates/identity/`**

---

## Amendments since 2026-08-12

- **[D47](../decisions/0009-measurement-session.md)** — the client role set is decided:
  **`aria-user`** (may interact with ARIA at all) and **`aria-admin`** (may approve MCP servers and
  manage the circle), as client roles on the ARIA client, never realm roles — the realm is shared
  with other homelab apps. This closes ARIA-19's last open item. Revisit when a second person needs
  partial admin authority (likely with ARIA-114).

---

## Purpose

Thin shared middleware around the existing Keycloak instance. **There is no user-service to build** —
no user CRUD, no profile storage, no preferences. Keycloak's job is strictly authN/authZ.

## Replaces

The 2021 hand-rolled `users` / `role` / `permission` schema.

## Owns

- Keycloak JWT validation, and the client-credentials flow for service-to-service auth
- OIDC discovery + JWKS fetch **from two sources**: the shared Keycloak realm's, and the Gateway's
  own context-signing key set (D1) — parameterised by key source, with independent caches and
  independent unreachable-behaviour
- Minting and verifying the Gateway's **Ed25519-signed end-user context**: `sign()` for the
  Gateway, `verify()` for everyone else, `kid` selection, and two keys live throughout rotation
- The tonic/tower interceptor layer wiring validation, client credentials and user context

**Security property the API must enforce:** `verify()` returns a typed identity that cannot be
constructed anywhere else in the crate's public API. There is no path from a raw `String` user id
to a trusted identity — so a prompt-injected tool argument claiming to be someone else cannot
become one.

## Binding decisions

D1 (Ed25519 Gateway-minted context) · D2 (`sub` as `user_id`) · D3 (public client, PKCE + Device Grant) · D4 (some circle members have no account) · D5 (audience restriction via per-callee client scopes) · D31 (client secrets arrive sealed)

See [the Decision Log](../decisions/README.md) for the full reasoning and rejected alternatives.

## Contract

Library API: `sign()`, `verify()`, JWKS caching, tonic interceptors. No proto of its own.

## Open items

- Crate choices are **proposed, not decided**: `openidconnect` vs. plain `oauth2` for discovery,
  `moka` vs. a simpler cache for JWKS (ARIA-20)
- Cache TTL and refresh strategy (time-based vs. refresh-on-unknown-`kid` vs. both)
- Whether the context freshness window is per-request or per-session, and its value (ARIA-28)
- Concrete client role names and their granularity (ARIA-19)
- Gateway signing-key generation, storage and rotation mechanics (ARIA-113)
- Onboarding and reconciling circle members in the shared realm (ARIA-114)

## Jira stories

ARIA-19, 20, 22, 23, 24, 28, 29, 31, 38, 113, 114

---

*Derived from `03-architecture.md` and the Decision Log. Last updated 2026-08-19.*

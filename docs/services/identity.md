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
- **[D59](../decisions/0011-identity-wire-encoding.md)** — the signed context's payload is fixed:
  D1's `user_id`, `roles`, `exp`, `jti` plus `aud`, `iss`, `nbf`, `iat`. See
  [Wire encoding](#wire-encoding) below.

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

## Wire encoding

The signed end-user context is a standard JWT, Ed25519-signed (`alg: EdDSA`, D1).

**Header.** `alg: "EdDSA"`, `typ: "JWT"`, `kid: <string>` — identifies which of the Gateway's live
signing keys produced this token. The same `kid` must appear in the Gateway's published key set (the
JWKS export, ARIA-28) so a verifier can pick the matching public key without trying every key it
holds.

**Claims (D1, D59).**

| Claim | Type | Meaning |
|---|---|---|
| `user_id` | string | Keycloak `sub` (D2) |
| `roles` | array of `"aria-user"` \| `"aria-admin"` | D47's two client roles |
| `aud` | string | intended callee; `verify()` rejects a mismatch |
| `exp` | number (unix seconds) | expiry; `verify()` rejects an expired token |
| `iat` | number (unix seconds) | issued-at |
| `iss` | string | issuer; `verify()` rejects a mismatch |
| `nbf` | number (unix seconds) | not valid before |
| `jti` | string (UUIDv4) | unique per token, for replay detection and audit correlation |

This table is the single source of truth for the wire shape — the Gateway and every verifying
service must agree with it, not re-derive it from either side's code independently.

**`kid` derivation — open, not yet correct.** The intent is that a `kid` identifies one Ed25519
keypair and is computed identically by the Gateway (signing) and by every verifier (checking against
its published key set), so both sides agree without coordinating out of band. **The current
implementation does not do this**: the signing side hashes the raw *private*-key bytes (a 48-byte
ASN.1-wrapped blob for an Ed25519 key), while the verifying side hashes the raw *public*-key bytes
(32 bytes) — two different byte strings for the same keypair, so a `kid` minted while signing will
never match a `kid` computed while verifying. This must be fixed — by deriving both sides' `kid` from
the same public-key bytes — before rotation or verification can work end-to-end. Tracked below.

## Open items

- Crate choices are **proposed, not decided**: `openidconnect` vs. plain `oauth2` for discovery,
  `moka` vs. a simpler cache for JWKS (ARIA-20)
- Cache TTL and refresh strategy (time-based vs. refresh-on-unknown-`kid` vs. both)
- Whether the context freshness window is per-request or per-session, and its value (ARIA-28)
- `kid` derivation is inconsistent between the signing side (hashes private-key bytes) and the
  verifying side (hashes public-key bytes) — a signed token's `kid` will never match a verifier's
  key set until both sides hash the same (public) key material (ARIA-28)
- Concrete client role names and their granularity (ARIA-19)
- Gateway signing-key generation, storage and rotation mechanics (ARIA-113)
- Onboarding and reconciling circle members in the shared realm (ARIA-114)

## Jira stories

ARIA-19, 20, 22, 23, 24, 28, 29, 31, 38, 113, 114

---

*Derived from `03-architecture.md` and the Decision Log. Last updated 2026-08-27.*

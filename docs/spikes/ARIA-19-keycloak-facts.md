# ARIA-19 — Keycloak realm facts (fill-in template)

[Library index](../README.md) · [Spikes](README.md) · [Identity](../services/identity.md) · [Sprint 1](../sprints/sprint-01.md)

**Status: In Progress in Jira. This is the artefact the story is supposed to produce.**

ARIA-19 is not a spike — it is a configuration story. But its stated scope is *"produces the
Keycloak-side configuration and **the facts** the `aria-identity` crate is built against"*, and
those facts do not exist anywhere yet. ARIA-20, 22, 28, 29 and 31 are all built against them.

> ⚠️ **This is an additive change to a live realm shared with other homelab apps.** The first
> section exists so that a later ARIA story cannot collide with Finance Manager, and so the
> pre-change state is recoverable. Capture it **before** touching anything.

Fill this in and commit it. Values below are placeholders.

---

## 1. Pre-change state — capture before touching anything

| Fact | Value |
|---|---|
| Keycloak base URL | `https://…` |
| Realm name | `…` |
| Issuer URL (`iss`) | `https://…/realms/<realm>` |
| OIDC discovery URL | `https://…/realms/<realm>/.well-known/openid-configuration` |
| JWKS URI | *(read from discovery — do not assemble from a path template)* |
| Token endpoint | *(read from discovery)* |
| Device authorization endpoint | *(read from discovery — confirms D3 is supported)* |
| Keycloak version | `…` |

```bash
# The one command that gets most of this:
curl -s https://<host>/realms/<realm>/.well-known/openid-configuration | jq
```

**Existing clients** (so ARIA's naming cannot collide):

| clientId | Type | Notes |
|---|---|---|
| | | |

**Existing realm roles** (ARIA must add *none* of these):

| Role | Used by |
|---|---|
| | | 

**Naming conventions other apps already follow** — e.g. does Finance Manager use
`finance-manager` or `fm`? Client roles or realm roles? ARIA should look like it belongs.

---

## 2. The ARIA client

Per **D3**: a single **public** client, Authorization Code + PKCE as the primary flow, with the
**Device Authorization Grant enabled on the same client** so a screenless voice device can enrol
later without a realm change.

| Setting | Value |
|---|---|
| clientId | `aria` *(confirm against §1 conventions)* |
| Access type | **public** (D3) |
| Standard flow (Auth Code) | **on** |
| PKCE method | **S256** |
| Device authorization grant | **on** (D3) |
| Direct access grants | **off** unless justified |
| Valid redirect URIs | `…` |
| Web origins | `…` |

### Client roles on the ARIA client

Per the story: **client roles on the ARIA client, not realm roles**, so they cannot leak into other
apps' tokens.

> **Open — needs a decision.** CLAUDE.md says permissions are modelled as client roles but names
> none. This is a genuine bucket-A item and the last one blocking ARIA-19.
>
> Note the scope of what these roles must cover is *smaller than it looks*: per-tool MCP
> permissions are **not** client roles — D7 puts them in per-server Keycloak clients as
> `aria:mcp:<server>:<tool>` scopes, provisioned at runtime by `aria-kc-broker` (D8). So this role
> set covers only ARIA-core authority, and a plausible starting set is small — something like a
> plain user role and an admin role, with more added when a distinction actually needs expressing.
> **Do not invent the list here; decide it and record it as a D-entry.**

| Role | Authorises |
|---|---|
| | |

---

## 3. Token facts the `aria-identity` crate is built against

| Fact | Value |
|---|---|
| `user_id` claim | **`sub`** (D2) — immutable, so a rename does not orphan user-scoped tables |
| Roles claim path | e.g. `resource_access.aria.roles` — **record the actual path** |
| Audience (`aud`) behaviour | *(see D5 — audience restriction per callee)* |
| Token lifetime | `…` |

### Required evidence

The story requires **concrete decoded examples**, not descriptions:

- [ ] A decoded token for a test user **with** an ARIA client role, showing the role at the
      recorded claim path
- [ ] A decoded token for a user **without** the assignment, showing it absent

These become the fixtures ARIA-22's validation tests run against. Paste them here with any
personal fields redacted.

---

## 4. Circle members in the realm (D4)

D4 established as a **fact, not a choice**: some circle members already exist in the shared realm
via other homelab apps; some do not; some never will.

Record which is which. This is the input to ARIA-114 (onboarding + reconciliation) and it is why
D17 makes `person.keycloak_sub` **nullable**.

| Person | In realm? | `sub` | Will ever authenticate? |
|---|---|---|---|
| | | | |

*(Names only — do not record personal detail here beyond what identity resolution needs.)*

---

## 5. Reproducibility

The story requires the client config committed in a reviewable form, so it can be audited or
recreated without recalling admin-console clicks.

```bash
# Partial realm export limited to the ARIA client:
kcadm.sh get clients -r <realm> -q clientId=aria > aria-client.json
```

- [ ] Export committed (secrets stripped — a public client has none, which helps)
- [ ] Or: a written change procedure, step by step

## 6. Non-regression check

- [ ] Logged into a **non-ARIA** app in the same realm after the change and confirmed it still
      works. This is an acceptance criterion, not a nicety — the realm is shared.

---

## What is needed to close this story

- [ ] Sections 1–5 filled in and committed
- [ ] The client role set **decided** and recorded as a D-entry
- [ ] Both PKCE and Device Grant exercised once against a test user
- [ ] Both decoded token examples captured
- [ ] Non-regression check done

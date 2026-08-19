# ARIA — Decision Log · Identity, tokens & service auth

**Decisions D1–D9** · taken 2026-08-12 (session 1)

One entry per decision: what was decided, why, what was rejected and why, and which stories it
affects. Part of [the decision index](README.md). Several entries carry an explicit **revisit
trigger** — those are not settled forever, but must not be reopened until the trigger fires.

---

### D1 · X1 — The propagated user context is a Gateway-minted asymmetric JWT

**Decided.** The Gateway mints a short-lived JWT containing `user_id`, roles, `exp` and `jti`,
signed with an Ed25519 private key held only by the Gateway. Every service verifies it against the
Gateway's published public key (JWKS-style endpoint, two keys live during rotation). The token is
attached as gRPC metadata on every downstream call, alongside the existing service-to-service
client-credentials auth.

**Why.** Keeps Keycloak off the per-request hot path, which matters on a voice pipeline with a
latency budget. Preserves the §2 invariant that the Gateway is the only place a raw user credential
exists. Asymmetric signing means a compromised Speech or MCP Registry pod can verify contexts but
cannot mint one claiming to be someone else — the precise threat this mechanism exists to stop.
`exp` plus `jti` covers replay.

**Rejected.**

- *Keycloak token exchange* — the purest "don't reinvent what's solved" answer, and rotation comes
  free, but it makes Keycloak a hard dependency on every internal call and introduces an unchecked
  version dependency on the shared homelab instance (standard token exchange needs a recent
  Keycloak).
- *Forward the original user JWT* — breaks the "only the Gateway holds a raw credential" invariant,
  and any compromised service could replay the user's token anywhere, including to Keycloak.
- *Symmetric HMAC* — every verifier would hold the minting key, so one compromised pod could forge
  any identity.

**Affects.** ARIA-28 (owner), ARIA-31, ARIA-32, ARIA-82. Newly revealed: Gateway key generation,
storage and rotation is work nobody owns yet, and it lands on the secrets decision (X6).

---

### D2 · X2 — `user_id` is the Keycloak `sub` claim

**Decided.** The immutable Keycloak `sub` UUID is `user_id` everywhere it crosses a service
boundary, and is the external identity reference persisted by services that store user-scoped data.

**Why.** Immutable — renaming a user or changing their email does not migrate the social graph,
preferences, calendar, consent grants and audit log. Opaque, which also helps the telemetry
question (X8).

**Rejected.**

- *`preferred_username`* — readable in logs, which genuinely helps at this scale, but mutable in
  Keycloak; a single rename silently orphans every user-scoped table.
- *An ARIA-issued person UUID with `sub` mapped to it* — puts a lookup on every authenticated
  request and pre-empts K3 rather than deciding it deliberately.

**Scope note.** This fixes the identity *on the wire*. Whether the Knowledge Core's `person` rows
are keyed on `sub` is K3's question (batch 3) — and given D4, they cannot be.

**Affects.** ARIA-19, ARIA-32, and every story that persists user-scoped data.

---

### D3 · I3 — Public client, Authorization Code + PKCE, with the Device Authorization Grant enabled

**Decided.** One public Keycloak client for the user-facing ARIA client, driving Authorization Code
with PKCE as the primary flow, with the Device Authorization Grant enabled on the same client for
headless devices.

**Why.** PKCE is the correct flow for native, mobile and CLI clients that cannot hold a secret.
Enabling the Device Grant now covers a screenless voice device — a plausible ARIA client — without
having to reconfigure the realm later.

**Rejected.**

- *PKCE only* — simpler, but a screenless device could not enrol without a realm change.
- *Confidential client* — the device is the client here; a secret shipped to a device is not a
  secret.

**Affects.** ARIA-19, ARIA-29.

---

### D4 · I4 — Some circle members already exist in the shared realm; the rest do not

**Established fact, not a design choice.** A subset of the circle already has accounts in the shared
realm via other homelab apps (e.g. Finance Manager). The remainder do not.

**Consequences.**

1. Onboarding circle members into the shared realm is real work that no story currently covers.
2. Existing accounts were created under another app's assumptions, so reconciliation — which
   existing realm user corresponds to which person in ARIA's graph — is also unowned.
3. K3 is now constrained rather than open: the Knowledge Core will hold `person` rows for people
   with no Keycloak subject for an extended period, so `person` cannot be keyed on `sub`.

**Affects.** ARIA-19, ARIA-57, ARIA-59. Newly revealed: an account-onboarding/reconciliation story
in the Identity epic.

---

### D5 · I1 — Service-to-service authorization is audience restriction via per-callee client scopes

**Decided.** Each service is a Keycloak confidential client. An optional client scope per callee
(e.g. `aria-knowledge-core-aud`) is assigned only to the services permitted to call it; the callee
rejects any token whose `aud` is not itself. Client roles are added later only where two callers
need genuinely different rights against the same callee.

**Why.** Keycloak-native and it restricts token *acquisition*, not just use — a compromised Speech
pod cannot obtain a Knowledge Core token at all. With five services and roughly ten call edges,
a full role matrix would be ceremony before there is a distinction to express.

**Rejected.**

- *Client roles on the callee* — more granular immediately, but a matrix to maintain, and it only
  constrains use after a token has been issued.
- *Capability scopes (`knowledge-core:write`)* — reads well and survives service splits, but
  generic capability names collide in a realm shared with other homelab apps.

**Affects.** ARIA-23, ARIA-24.

---

### D6 · X7 — Internal gRPC stays plaintext in-cluster, with NetworkPolicy, conditional on ARIA-79

**Decided.** No additional TLS wrapping for now. Every internal call already carries an
authenticated client-credentials token; NetworkPolicy restricts which pods may reach which
services. Confidentiality is recorded as resting on cluster network isolation.

**Why.** On a single-node k3s box, internal traffic never leaves the host, and a certificate
lifecycle is real operational burden with a silent failure mode (expiry takes down internal calls).

**Conditional — this is a live tripwire, not a settled answer.** If ARIA-79 reports the cluster is
multi-node, voice audio crosses the LAN in plaintext between nodes. This decision must be revisited
at that point rather than inherited.

**Rejected.**

- *mTLS via cert-manager* — correct regardless of topology, but meaningful work against a threat a
  single-node cluster may not have.
- *Linkerd or another service mesh* — gets mTLS without hand-rolled certificate plumbing, but adds a
  whole component to operate and overlaps the OpenTelemetry work already decided.

**Affects.** ARIA-14, ARIA-42, ARIA-91. Newly revealed: NetworkPolicy authoring is not in any
story; and ARIA-79 gains an explicit obligation to report node count back to this decision.

---

### D7 · Q10 — Per-tool Keycloak scopes, `aria:mcp:<server>:<tool>` (closes ARIA-35)

**Decided.** One Keycloak scope per *tool*, not per server, named `aria:mcp:<server-slug>:<tool>`.
The `aria:` prefix avoids collision in the shared realm; slug and tool charset are fixed at
registration so a server name cannot produce an invalid or colliding scope.

**Why (Jann's call).** Puts fine-grained consent in front of Keycloak's own consent screen and
account-console revocation UI, so a user can see and revoke individual tools through a surface that
already exists rather than only through ARIA's own enforcement.

**Rejected.**

- *`mcp:<server-name>` as suggested in §8* — claims the whole `mcp:` namespace in a realm ARIA does
  not own.
- *Per-server scope with per-tool granularity only in `consent_grants`* (the §2 two-layer model, and
  my recommendation) — needs no Keycloak write on tool drift and treats remote LAN servers
  identically, but keeps per-tool visibility out of Keycloak's revocation UI.

**Known cost, accepted deliberately.** Keycloak scopes end up roughly 1:1 with `consent_grants`
rows, and tool-list drift (M5) now requires a Keycloak write. This is what forced D8.

**Affects.** ARIA-35 (closed by this), ARIA-38, ARIA-70, ARIA-75, ARIA-88, ARIA-90.

---

### D8 · I2 — Runtime scope provisioning, through a narrow broker service

**Decided.** Per-tool scopes are created at runtime — which requires a Keycloak admin credential in
the cluster. That credential lives in a **dedicated provisioning broker service**, not in the MCP
Registry. The broker exposes exactly one operation (create/reconcile the scopes for a named MCP
server) and refuses any scope name outside the `aria:mcp:` prefix. The Registry calls the broker.

**Why.** D7 makes runtime Keycloak writes unavoidable, and Keycloak's `manage-clients` is
realm-wide — a compromised writer could modify other homelab apps' clients. The MCP Registry is the
component that handles LLM-chosen tool traffic, so it is the worst possible holder of that
credential. A broker with a hardcoded prefix policy means a prompt-injected tool call can reach one
narrow API and nothing else.

**Rejected.**

- *Registry holds the credential directly* — simplest, but puts realm-wide client management one
  step from model-influenced input.
- *Deploy-pipeline-only provisioning, no runtime credential* — the safest option and initially
  chosen, but incompatible with D7: remote LAN MCP servers never touch the pipeline, and runtime
  tool-list changes could not be represented.
- *Kubernetes Job per write* — narrow credential lifetime, but gives the Registry pod-create rights,
  which is the same escalation path E1 worries about for Self-Extension.

**Residual risk, recorded not solved.** Keycloak cannot scope `create-client`/scope-creation rights
below realm level, so the broker's credential remains realm-capable. The broker's prefix policy is
the only thing narrowing it, and that policy is ARIA's code, not Keycloak's enforcement.

**Affects.** ARIA-38, ARIA-75, ARIA-88. Newly revealed: the provisioning broker is a **new service**
— crate, deployment, Keycloak service account, and its own story set. It also adds a member to §3
and §5 of CLAUDE.md.

---

### D9 · C2 — Enforcement identity and contextual identity are separated; the Registry injects a reserved field

**Contradiction resolved.** Two different things were being called the same thing.

- *Enforcement identity* — who the caller is, for the purposes of consent and audit — comes **only**
  from the Gateway's signed context metadata (D1). Never from an argument. §2 holds as written.
- *Contextual identity* — a tool that legitimately needs to know which person it is acting for —
  is supplied by the **MCP Registry** injecting a reserved argument (`_aria_user_id`) into the tool
  call. The reserved field is stripped from the tool schemas exposed to the model, so the model
  cannot see or set it, and the Registry overwrites it unconditionally if present.

The Confluence "ARIA — MCPs" sentence is corrected on one point: the **Registry**, not the Agent
Core, does the injecting — consistent with the Registry already being the enforcement point.

**Rejected.**

- *Hard-fail any identity-shaped argument* — "identity-shaped" is undecidable; it means maintaining
  a parameter-name blocklist that gives false confidence while breaking legitimate tools.
- *Inertly forward, Registry never reads arguments* — honest about what the Registry guarantees,
  but leaves a naive target server spoofable with nothing in ARIA stopping it, which matters most
  for self-generated servers.

**Affects.** ARIA-82 (owner), ARIA-61, ARIA-92, ARIA-95. Requires an edit to the Confluence
"ARIA — MCPs" page, not just an addition.

---

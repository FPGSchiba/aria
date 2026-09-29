# ARIA — Technology stack

[Library index](README.md) · [Decisions](decisions/README.md) · [Open questions](open-questions/README.md) · [Sprint 1](sprints/sprint-01.md)

Every settled technology choice with its rationale and what it replaces from the 2021 prototype.
Each row is backed by one or more entries in the [Decision Log](decisions/README.md), which holds
the rejected alternatives.

**Source:** CLAUDE.md §2 as of 2026-08-12, moved here verbatim.

---

## Amendments since 2026-08-12

The table below is preserved verbatim as of 2026-08-12. These later decisions amend it — read them
first where they overlap.

- **[D47](decisions/0009-measurement-session.md)** amends *Identity & authentication*: the ARIA
  client's roles are now named — **`aria-user`** and **`aria-admin`**, as client roles on the ARIA
  client. Per-tool MCP permissions are **not** here; D7 puts those in per-server clients as
  `aria:mcp:<server>:<tool>` scopes.
- **[D48](decisions/0009-measurement-session.md)** amends *Knowledge Core*: D16 said preferences
  are partitioned by kind; D48 gives the rule for deciding which kind. A preference is **typed**
  only if it has a closed value domain, is read by exact lookup, and being wrong is a bug —
  **free-text is the default**. Kind changes are a migration, never a dual-write; typed wins on
  conflict. Qdrant-primary categories are **free-text preferences and learned facts**.
- **[D49](decisions/0009-measurement-session.md)** amends *Agent Core "brain"*: the canonical
  internal tool-call shape is **ARIA's own type**, modelled close to OpenAI's; both backends adapt
  into it. Measured constraint behind it: Ollama's OpenAI-compatible endpoint **cannot disable a
  model's thinking phase**, so the Ollama backend needs the native API and neither adapter is free.
- **[D56](decisions/0010-retention-audit.md)** amends *Knowledge Core* and *Consent & permissions*
  with a **retention schedule**: conversation history and `consent_audit_log` are kept
  **indefinitely** as deliberate choices; the unrecognized-request log is pruned at **90 days**;
  `aria-speech` keeps **no** transcript at any level; telemetry is **forward-only** with a bounded
  disk-backed outage buffer, its retention a property of the destination.
- **[D57](decisions/0010-retention-audit.md)** amends *Consent & permissions*: `consent_audit_log`
  records **two rows per invocation** — an attempt row written to a **durable local outbox in the
  MCP Registry before the call is forwarded**, and an outcome row after it returns, drained
  asynchronously into the Knowledge Core. On an allowed call the write retries, then queues, and
  fails the call closed **only if durability cannot be achieved anywhere**. This is deliberately a
  weaker posture than D43's consent fail-closed, and the entry argues why: D43 protects the user's
  control, this protects the record. **The MCP Registry is stateful as a result.**


### From the 2026-09-23 design conversation (D60–D84)

Twenty-five further decisions. These five amend the table directly; the rest add surfaces the
table never covered (clients, notifications, extension UIs) and are described in
[Architecture](03-architecture.md).

- **[D82](decisions/0016-observability-v2.md)** replaces the *Observability* row outright.
  Instrumentation is unchanged — still OpenTelemetry, still vendor-neutral, which is exactly why
  this was a config change. The backend is not: an **in-cluster OpenTelemetry collector** ARIA
  owns, **metrics into the existing Prometheus** (`lens-metrics`), **traces into Jaeger deployed
  in-cluster**, **logs on stdout** with no export pipeline. **AppSignal is dropped** — it would
  have been a third telemetry pipeline beside Prometheus and Zabbix, and it shipped traces
  off-site. **Telemetry no longer crosses the network boundary.** [D83](decisions/0016-observability-v2.md)
  adds the propagation requirement: four points, three of them manual (a `tonic` interceptor,
  a `traceparent` header across the MCP boundary that must live in the generated-server template,
  and a root span started on the client device).
- **[D73](decisions/0015-external-datastores.md)** and **[D78](decisions/0015-external-datastores.md)**
  amend *Knowledge Core (RAG)*: Postgres and Qdrant are **no longer in-cluster deployments**. Each
  moves to its own VM, outside the cluster, reached over the LAN. Postgres is organised as
  **one schema per service** with a `*_owner` / `*_app` role split
  ([D73](decisions/0015-external-datastores.md)), cross-schema reads go through **read-only views**
  ([D74](decisions/0015-external-datastores.md)), and the **Knowledge Core gets its own database**
  so that a cross-schema grant to it is structurally inexpressible
  ([D77](decisions/0015-external-datastores.md)).
- **[D76](decisions/0015-external-datastores.md)** amends *Secrets management*, and **corrects the
  reasoning in the row as written**: it rejected Vault as "a whole secret-management system for a
  handful of credentials", which the ARIA-79 baseline disproved — **Vault is already running in
  the homelab**. Database credentials now come from Vault. Sealed-secrets is **not** replaced:
  it remains the deploy-time artifact in git; Vault issues runtime identity. The two answer
  different questions and the entry draws the line.
- **[D75](decisions/0015-external-datastores.md)** adds a service to the *Keycloak
  client/scope provisioning* pattern: **`aria-storage-broker`**, a deliberately dumb reconciler
  that issues schema grants at approval time. Same shape as `aria-kc-broker` and for the same
  reason — a narrow service holding a credential that the Registry must never hold.
- **[D81](decisions/0015-external-datastores.md)** supersedes **D6**'s conditional on internal
  TLS. Service-to-service mTLS is **explicitly still open**, not settled; D6's cost argument
  against it (standing up a CA) is no longer true, because cert-manager and `fpg-ca` already exist.

Two further rows are affected without being replaced. *Source control & CI* keeps its accepted
trade-off, but the list of things that leave the network is now **shorter by one** — telemetry is
off it. And *Consent & permissions* gains a third thing a server may contribute beyond tools: a
**UI bundle**, approved by digest in the same PR as its image
([D66](decisions/0013-extension-surface.md)), running on a separate origin in a sandboxed iframe
with an audience-limited token ([D67](decisions/0013-extension-surface.md),
[D68](decisions/0013-extension-surface.md)) — never the user's Keycloak token.

---

| Concern | Decision | Rationale / replaces |
|---|---|---|
| Language | **Rust**, across all services | `tonic` gRPC is first-class and type-safe; Qdrant is Rust-native with a first-party client. Keycloak does auth externally over standard OIDC, so services just validate JWTs. Replaces the original all-Python client + server. |
| Internal service transport | **gRPC** via `tonic`, protobuf in a shared `proto/` directory, bidirectional streaming for the audio channel. Schemas are versioned with **both a directory and a package suffix** — `proto/<service>/v1/<service>.proto` containing `package aria.<service>.v1;`. Generated types are built at build time into `OUT_DIR`; **nothing generated is committed**. | Replaces the three raw hand-wrapped SSL sockets (Audio / Input / Admin) from 2021. The dual versioning convention is what buf's lint and breaking-change rules expect; build-time generation makes schema/code drift impossible by construction rather than something CI must police. |
| Extension / tool protocol | **Standard, unmodified MCP**. ARIA's Agent Core is an MCP **Host**; each external capability is an MCP **Server**. Default transport to MCP servers is **HTTP (streamable HTTP/SSE)**, not stdio — fits a Kubernetes deployment where each server is its own pod/service. **All MCP connections live in the MCP Registry**; the Agent Core holds no MCP SDK dependency and speaks only gRPC to the Registry, with tool schemas carried across that boundary as JSON Schema. What MCP doesn't provide (dynamic runtime registration, user/relationship context, approval gating) is added as an ARIA-specific control-plane layer in the MCP Registry — the protocol itself is never forked. See the "ARIA — MCPs" Confluence page for the full reasoning. | This is the "connect a new system without much intervention" requirement, while staying compatible with the wider MCP ecosystem. Keeping every MCP connection in one service is what makes the consent-enforcement claim true rather than aspirational — a second connection path would bypass both gates. |
| Identity & authentication | **Keycloak** — reusing the **existing Keycloak instance already running in Jann's homelab**, not a new deployment. ARIA is registered as a new client in the **existing shared realm** used by other homelab apps (e.g. Finance Manager), with ARIA-specific permissions modeled as client roles on that client. The user-facing client is **public**, driving **Authorization Code + PKCE**, with the **Device Authorization Grant** enabled on the same client for headless/screenless voice devices. Services validate JWTs via shared middleware. Keycloak's job is strictly authN/authZ — not preferences or relationships. | Replaces the hand-rolled `users`/`role`/`permission` schema. Sharing the realm means one identity across Jann's personal apps instead of each app reinventing user management. PKCE is the correct flow for clients that cannot hold a secret; enabling the Device Grant now avoids reconfiguring the realm when a screenless device appears. |
| Knowledge Core (RAG) | **Postgres** + **Qdrant**, both new in-cluster deployments, fronted by the `aria-knowledge-core` **service** (it is a Deployment, not a library). Postgres holds the social graph, **typed** preferences, the MCP server registry, the calendar cache and consent tables; Qdrant holds semantic memory and **free-text** preferences. Preferences are **partitioned by kind, not duplicated**: a typed attribute (`timezone`, quiet hours) is authoritative in Postgres; a fuzzy one ("I like my coffee strong") is authoritative in Qdrant. Neither store is a cache of the other, so **Qdrant holds primary data and is not re-derivable**. Social graph is a typed `person` + `relationship(from, to, type, attrs JSONB)` model; `person` has its own UUID key with a **nullable, unique `keycloak_sub`**. Every row carries an `owner_user_id` and reads filter on it. Database access is via **`sqlx`** with compile-time-checked queries. | Replaces the legacy `relation`/`relation_type` tables and the hand-built sharded graph datastore. Partitioning by kind avoids a dual-write with no consistency story, at the cost of two systems of record. The nullable `keycloak_sub` is what lets the graph hold family members who will never authenticate. Per-row ownership gives "relationship-aware" a concrete meaning without inventing a cross-user sharing model today. |
| Agent Core "brain" | Pluggable LLM interface, **server-streaming**: `Decide` emits text chunks and tool-call events rather than one complete response, so speech synthesis can start on the first sentence. **Default: hosted frontier API** (e.g. Claude) for tool-calling reliability. **Fallback backend: the Ollama instance already running on Jann's GPU node**, engaged on error or timeout, plus an explicit per-request/config override. No automatic task-class routing yet. | Keeps ARIA's core decoupled from any one model provider. Streaming is the largest latency win available on a voice path. Automatic routing is deliberately withheld until the local model's tool-calling reliability is measured — a routing policy built on an unmeasured quality assumption is exactly the invented certainty this project avoids. |
| Service-to-service auth | Internal services authenticate to each other as **Keycloak confidential clients using client-credentials grants**. Authorization is expressed as **audience restriction via per-callee client scopes**: an optional client scope per callee is assigned only to permitted callers, and the callee rejects any token whose `aud` is not itself. Client roles are added later only where two callers need genuinely different rights against the same callee. | Consistent with "don't reinvent what's already solved." Audience restriction constrains token *acquisition*, not merely use — a compromised Speech pod cannot obtain a Knowledge Core token at all. With five services and roughly ten call edges, a full role matrix would be ceremony before there is a distinction to express. |
| End-user identity propagation | The **Gateway** is the only place a raw user credential exists. It validates the Keycloak JWT once, then mints a **short-lived Ed25519-signed JWT** carrying `user_id`, roles, `exp` and `jti`, attached to every downstream gRPC call as metadata alongside the service-to-service auth above. Services verify it against the Gateway's published public key (JWKS-style, two keys live during rotation). `user_id` is the Keycloak **`sub`** claim. Every service that touches user data operates against an explicit `user_id` — there is no implicit "current user." **This context is injected by trusted infrastructure only — never accepted as an LLM-fillable tool argument.** Where a tool legitimately needs to know which person it acts for, the **MCP Registry** — not the Agent Core — injects a reserved argument (`_aria_user_id`) that is stripped from the schemas shown to the model and overwritten unconditionally if present. | Answers "which user is requesting what," raised explicitly as a gap in the earlier design. Asymmetric signing means a compromised Speech or Registry pod can verify a context but cannot mint one. `sub` is immutable, so renaming a user does not migrate every user-scoped table. The reserved-argument rule separates *enforcement* identity (only ever from signed metadata) from *contextual* identity (data a tool receives), which is what previously read as a contradiction between this document and the MCPs page. |
| Consent & permissions | Two independent gates, not one. **(1) Approval** — is an MCP server's *code* trustworthy at all: one-time, admin-level, PR-based. The artifact is an **`approved-servers.yaml` in the repository**, rendered into the Helm chart and read by the Registry as config; merging the PR *is* the approval, so nothing ARIA runs can approve itself. Server identity is the **container image digest** (a rebuild requires re-approval), with a stable slug as the Registry key; remote LAN servers fall back to URL plus a pinned certificate, which is explicitly weaker. **(2) Consent** — does *this specific user* allow *this server/tool* to act on their behalf: per-user, ongoing, revocable. Consent is two-layered: a **Keycloak client per MCP server** with **per-tool scopes** named `aria:mcp:<server-slug>:<tool>`, and a **per-tool `consent_grants` table in the Knowledge Core** (plus `mcp_tools` and a `consent_audit_log` of every invocation, allowed or denied). Grants are issued **conversationally**: a call with no grant returns `consent required`, the Agent Core asks in the conversation, and on approval the original call is **retried automatically** within a bounded timeout. Keycloak's account console is the review and revocation surface. The Registry caches the Keycloak-level consent read with a **~30 s TTL and fails closed** when it cannot refresh. `ListTools` filters **by server, not by tool** — servers the user has no relationship with are hidden, but all tools within a connected server are listed so an ungranted tool can still trigger the consent prompt. The **MCP Registry enforces both gates** before forwarding any tool call. | Full UMA was considered and rejected: provisioning UMA resources/scopes/policies per server is real per-service ceremony, and its main payoff isn't needed when only ARIA's own Registry checks these grants. Per-tool Keycloak scopes put individual tools in front of Keycloak's own consent screen and account-console revocation, at the cost of Keycloak scopes being roughly 1:1 with `consent_grants` rows and tool-list drift requiring a Keycloak write. Conversational granting is required by a voice-first product — bouncing a user to a browser mid-sentence is not usable. A 30 s TTL makes revocation lag statable; failing closed is the only defensible posture for a consent gate. `granted_by` on `consent_grants` remains a deliberate hook for future relationship-aware delegation. |
| Keycloak client/scope provisioning | Per-tool scopes are created **at runtime**, which requires a Keycloak admin credential in the cluster. That credential lives in a dedicated **provisioning broker service** (`aria-kc-broker`), never in the MCP Registry. The broker exposes exactly one operation — create/reconcile the scopes for a named MCP server — and refuses any scope name outside the `aria:mcp:` prefix. | Keycloak's client-management rights are realm-wide, and the realm is shared with other homelab apps. The MCP Registry handles LLM-chosen tool traffic and is therefore the worst possible holder of that credential; a broker with a hardcoded prefix policy means a prompt-injected tool call reaches one narrow API and nothing else. **Residual risk, recorded not solved:** Keycloak cannot scope scope-creation below realm level, so the broker's credential remains realm-capable and the prefix policy is ARIA's code, not Keycloak's enforcement. |
| Secrets management | **Sealed-secrets**: encrypted secrets live in git, decrypted in-cluster by the sealed-secrets controller. Credentials for a newly approved self-generated service are sealed by the **approval pipeline**, so they cannot exist before approval. | Sealing needs only the cluster's public certificate, so with GitHub-hosted runners no private key ever leaves the network — decisive against SOPS+age, which would require storing the unlocking key in GitHub Actions secrets. Rejected: plain Kubernetes Secrets (no record of what exists, no rebuild path) and External Secrets + Vault (a whole secret-management system for a handful of credentials). The controller's private key is critical backup state. |
| Source control & CI | **GitHub**, with **GitHub Actions on hosted runners**. This is also the Git host for the Self-Extension server's own repo and the PR surface its approval gate assumes. | No CI infrastructure to operate, and the PR-based approval surface already exists rather than needing to be stood up. **Accepted trade-off:** source and build logs leave the network. This narrows the local-by-default posture from "nothing leaves" to "no *personal* data leaves" — see section 4. |
| Observability | **OpenTelemetry** for instrumentation (`tracing` + `tracing-opentelemetry` + `opentelemetry-otlp` in every Rust service — vendor-neutral, backend is just an OTLP endpoint) exported to **AppSignal** via AppSignal's **self-hosted collector** running in-cluster. **All three signals — traces, logs and metrics — leave via that collector, which is the only telemetry egress.** Logs are bridged from `tracing`, exported at reduced level with allow-listed fields only. | Routing every signal through the collector is what makes "the one place to enforce that conversation/voice content never lands in a span attribute or log line" literally true; specifying tracing alone left that claim false for two of three signal types. AppSignal's Rust support is OpenTelemetry-based and currently **beta**, and the Rust OTel *logs* bridge is less mature than tracing — vendor-neutral instrumentation means a rough patch there is a config change, not re-instrumentation. Free tier headroom should be confirmed against real traffic rather than assumed. |

---

## See also

- [Decision Log](decisions/README.md) — why each of these won, and what lost
- [Deployment & hosting](04-deployment.md) — where this stack runs
- [Conventions](05-conventions.md) — how it is laid out in code

# ARIA — Decision Log

[Library index](../README.md) · [Decisions](README.md) · [Open questions](../open-questions/README.md) · [Sprint 1](../sprints/sprint-01.md)

The reasoning trail. One entry per decision: **what was decided, why, what was rejected and
why, and which stories it affects.** `docs/02-stack.md` and friends hold the *current state*;
this directory holds *how we got there* — which is what stops a future session from re-litigating
a settled choice or, worse, silently reversing one.

## Rules

- **Append-only.** A decision that turns out wrong gets a *new* entry that supersedes it, naming
  the trigger that fired. Never edit a past entry.
- **Revisit triggers are binding in both directions.** Several entries below carry an explicit
  condition under which they must be reopened (D6 is the sharpest: if the cluster turns out to be
  multi-node, or its CNI does not enforce NetworkPolicy, the no-internal-TLS decision collapses).
  Do not treat a triggered decision as settled — and do not reopen an untriggered one.
- **Rejected alternatives are the point.** An entry without them cannot tell a future reader
  whether an option was considered and dismissed or simply never occurred to anyone.
- Next number to allocate: **D87**.

---

## Index


### [Identity, tokens & service auth](0001-identity-tokens-service-auth.md) · D1–D9

- **D1** (X1) — The propagated user context is a Gateway-minted asymmetric JWT
- **D2** (X2) — `user_id` is the Keycloak `sub` claim
- **D3** (I3) — Public client, Authorization Code + PKCE, with the Device Authorization Grant enabled
- **D4** (I4) — Some circle members already exist in the shared realm; the rest do not
- **D5** (I1) — Service-to-service authorization is audience restriction via per-callee client scopes
- **D6** (X7) — Internal gRPC stays plaintext in-cluster, with NetworkPolicy, conditional on ARIA-79 — ⚠️ **revisited by D81**
- **D7** (Q10) — Per-tool Keycloak scopes, `aria:mcp:<server>:<tool>` (closes ARIA-35)
- **D8** (I2) — Runtime scope provisioning, through a narrow broker service
- **D9** (C2) — Enforcement identity and contextual identity are separated; the Registry injects a reserved field

### [Repo, proto & CI foundations](0002-repo-proto-ci.md) · D10–D15

- **D10** (C1) — The Knowledge Core is a deployed service; §4 was wrong
- **D11** (C4) — `mcp-client` lives at `crates/mcp-client/`
- **D12** (P2) — Protobuf versioning uses both a directory and a package suffix
- **D13** (X9) — GitHub with GitHub Actions on hosted runners
- **D14** (P1) — Generated protobuf code is built at build time into `OUT_DIR`
- **D15** (Q13) — One umbrella Helm chart for the whole `aria` namespace (closes ARIA-21)

### [Knowledge model & data ownership](0003-knowledge-model.md) · D16–D23

- **D16** (C3) — Preferences are split by kind: typed in Postgres, free-text in Qdrant
- **D17** (K3) — `person.id` is an ARIA UUID; `keycloak_sub` is a nullable unique column
- **D18** (K4) — Per-fact ownership: every row carries the user it was learned from
- **D19** (K2) — Typed `person` + `relationship` tables
- **D20** (K5) — The calendar is a mirror with a read-through cache; writes go via a calendar MCP server — ⚠️ **placement superseded by D79**
- **D21** (Q4) — MCP service writes are namespaced by source, with areas declared at registration (closes ARIA-47)
- **D22** (K6) — The Knowledge Core validates writes itself
- **D23** (K1) — `sqlx`

### [Audio pipeline & turn-taking](0004-audio-pipeline.md) · D24–D30

- **D24** (S1) — STT serves English only
- **D25** (X3) — Opus on the client link, PCM internally
- **D26** (X4a) — The Speech service owns VAD and endpointing
- **D27** (X4b) — One stream carries many turns; barge-in is supported
- **D28** (S6) — ARIA may change Ollama's server configuration
- **D29** (G1) — Bounded drain with an explicit session-ending control frame
- **D30** (S5) — Speech owns TTS text normalisation, starting from the library's own handling

### [Secrets & deployment posture](0005-secrets-deployment.md) · D31–D34

- **D31** (X6) — Sealed-secrets — ⚠️ **narrowed by D76**; its Vault rejection rested on a claim ARIA-79 disproved
- **D32** (D3) — Local backups replicated encrypted to off-site object storage
- **D33** (D4) — Observability owns the collector's config and pipeline; Infra owns the chart
- **D34** (Q7) — The approval pipeline seals a new service's credentials (closes ARIA-99)

### [Agent Core, sessions & conversation](0006-agent-core-sessions.md) · D35–D40

- **D35** (Q12) — Failover plus an explicit override; automatic routing deferred (closes ARIA-44)
- **D36** (X5a) — The Gateway mints a `session_id`; the Agent Core owns history and persists it to the Knowledge Core
- **D37** (X5b) — `Decide` is server-streaming
- **D38** (A5) — No MCP SDK in the Agent Core
- **D39** (G2) — Read-only calls retry; tool calls never auto-retry
- **D40** (A1) — "Unrecognized" is defined structurally

### [Observability, consent & approval](0007-observability-consent.md) · D41–D46

- **D41** (O1) — All three signals leave via the collector — ⚠️ **superseded by D82**
- **D42** (M1) — Conversational grant, with the triggering call suspended and retried
- **D43** (M4) — Short-TTL consent cache, fail-closed on refresh failure
- **D44** (M3a) — The approval artifact is a git file rendered into the Helm chart
- **D45** (M3b) — Server identity is the image digest, with a stable slug as the registry key
- **D46** (M2) — `ListTools` filters by server, not by tool

### [Measurement session](0009-measurement-session.md) · D47–D54

Taken 2026-08-19. Three of the four were informed by measurements taken the same day.

- **D47** (ARIA-19) — The ARIA Keycloak client carries two client roles: `aria-user`, `aria-admin`
- **D48** (ARIA-43) — The typed / free-text classification rule; free-text is the default
- **D49** (ARIA-40) — The canonical internal tool-call shape is ARIA's own, OpenAI-flavoured
- **D50** — `aria-speech` asserts GPU availability at startup and fails closed
- **D51** (ARIA-79) — The container registry is GitHub Container Registry (`ghcr.io`) — ⚠️ **superseded by D52**
- **D52** — Supersedes D51: same outcome, corrected reasoning (Harbor already exists and was a strawman)
- **D53** (ARIA-19) — ARIA registers in the **master** realm — ⚠️ raises D8's residual risk
- **D54** — The docs library moves into the `aria` monorepo; GitHub is the canonical link target

### [Retention & the audit record](0010-retention-audit.md) · D55–D58

Taken 2026-08-20 (decision batch 1). Closed eleven open items across six issues; six of them were
the same retention question asked about five stores, answered here as one schedule.

- **D55** (ARIA-108) — An unrecognized-request record holds the raw utterance, under a short window, with a review path
- **D56** — One retention schedule, with a row per data class (closes the "retention policies, generally" item) — ⚠️ its **telemetry row** is invalidated by D82
- **D57** (ARIA-73 / ARIA-85) — Two audit rows per invocation: a durable attempt row before forwarding, an outcome row after
- **D58** (ARIA-83) — The audit row stands alone; the trace ID is a best-effort hint expected to dangle

### [Identity wire encoding](0011-identity-wire-encoding.md) · D59

Taken 2026-08-27, during ARIA-28 implementation review.

- **D59** (ARIA-28) — The Gateway-signed context keeps four standard JWT claims beyond D1's minimum

### [Clients, the turn log & the Gateway surface](0012-clients-gateway-surface.md) · D60–D65

Taken 2026-09-23 (design conversation). Clients enter the architecture for the first time.

- **D60** — Clients are in scope: native phone/desktop, plus a Core Web shell
- **D61** — The turn log is product data, not an implementation detail of conversation history
- **D62** — The Gateway has two front doors: a gRPC stream for turns, HTTP for everything else
- **D63** — Front-door routing is by path prefix; the Gateway never decodes a body to route
- **D64** — The Gateway remains the sole minter of end-user context, on both front doors
- **D65** — Two paths into one service may carry different gates, and the difference is deliberate

### [The extension surface](0013-extension-surface.md) · D66–D70

Taken 2026-09-23 (design conversation). What an MCP server may contribute beyond tools.

- **D66** — An MCP server may ship a UI bundle, approved by digest in the same PR as its image
- **D67** — Extension UI runs on a separate origin, in a sandboxed iframe, over `postMessage`
- **D68** — An extension UI gets an audience-limited token, never the user's Keycloak token
- **D69** — The Registry is the service catalog and publishes the route table
- **D70** — MCP servers get no direct Knowledge Core access; a narrow `ResolvePerson` behind its own consent scope

### [Notifications & proactivity](0014-notifications.md) · D71–D72

Taken 2026-09-23 (design conversation). Proactivity gets a mechanism and a leash in one breath.

- **D71** — `aria-notify` is a core service, stateless, with inbox and devices in the Knowledge Core
- **D72** — Sending a notification is a gated capability: consent scope, quiet hours, rate limit, mute, audit row

### [External datastores](0015-external-datastores.md) · D73–D81

Taken 2026-09-23 (design conversation). The largest batch, and the one that corrects two earlier
rejections that rested on claims the ARIA-79 baseline disproved.

- **D73** — Postgres moves to an external VM: schema per service, `*_owner`/`*_app` role split
- **D74** — Cross-schema reads are read-only views, with `ALTER DEFAULT PRIVILEGES` and `PUBLIC` revoked
- **D75** — Schema grants are issued at approval time; `aria-storage-broker` is a dumb reconciler
- **D76** — Database credentials come from Vault — ⚠️ **corrects D31's reasoning**
- **D77** — The Knowledge Core gets its own database, so the cross-schema grant is inexpressible
- **D78** — Qdrant moves to its own VM; capacity relief is a consequence, not the motive
- **D79** — The calendar cache relocates — ⚠️ **supersedes D20's placement only**
- **D80** — The data-VM posture: `verify-full` TLS from `fpg-ca`, layered egress control, tested restore
- **D81** — ⚠️ **Supersedes D6**, and leaves service-to-service mTLS explicitly open

### [Observability, second pass](0016-observability-v2.md) · D82–D83

Taken 2026-09-23 (design conversation). The instrumentation is unchanged; the backend is not.

- **D82** — ⚠️ **Supersedes D41**: in-cluster collector, metrics to the existing Prometheus, traces to Jaeger, logs on stdout. AppSignal dropped; telemetry no longer crosses the network boundary
- **D83** — Trace context propagates at four points, three of them by hand; the user-facing timeline is built from the turn log, never from traces

### [Working conventions](0017-conventions.md) · D84

Taken 2026-09-23 (design conversation).

- **D84** — Definition of done extends: no service is done without its migration, its Helm chart and its OTel spans

### [Hosted LLM backend](0018-hosted-llm-backend.md) · D85

Taken 2026-10-02 (measurement — ARIA-122).

- **D85** — The default hosted backend is Qwen 3.8 27B on Cerebras, through an OpenAI-compatible adapter; reasoning at the provider default; `api.cerebras.ai` is the one LLM egress host

### [Container packages](0019-container-packages.md) · D86

Taken 2026-10-05 (planning — ARIA-123).

- **D86** — ARIA's `ghcr.io` packages are public and pulled anonymously; no secret and no homelab CA is ever baked into an image

### [Session 1 summary](0008-session-1-summary.md)

What the 46 decisions changed across the backlog: spikes resolved, stories whose scope changed,
newly revealed work, newly discovered open questions, and what was deliberately deferred.

---

## Provenance

D1–D46 were taken in a single session on **2026-08-12**, recorded at the time in
`ARIA-decisions-2026-08-12.md`. That file was split into this directory on 2026-08-19 with no
content changes — the split is verbatim and was verified by diff.

The same reasoning is mirrored on the Confluence page **ARIA — Decision Log** (a child of
Architecture & Hosting). If the two ever disagree, this directory is authoritative and Confluence
is the stale copy.

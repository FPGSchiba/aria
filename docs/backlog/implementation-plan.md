> [!WARNING]
> # ⚠️ STALE — DO NOT TRUST THIS DOCUMENT
>
> This plan is **derived**, and it has drifted. It was already wrong in **seven places** relative
> to live Jira before the 2026-08-12 decision session, and it was **not regenerated afterwards** —
> so it does not reflect the 46 decisions, the 12 new stories (ARIA-109…ARIA-120), the four
> spikes closed as Done, the repurposing of ARIA-21, or the 16th sprint.
>
> **Known wrong as of 2026-08-19:** it lists 98 stories / 195 links / 15 sprints. Live Jira has
> **110 stories / 226 links / 16 sprints**. It shows ARIA-11 as pending; ARIA-11 is **Done**.
>
> **Rebuild from Jira's live `Blocks` links.** Never from this file.
> Coordinates and the query recipe are in [README.md](README.md).
>
> Kept only because the critical-path and bottleneck *analysis* below is still structurally
> useful — as a shape, not as fact.

---

<details>
<summary><b>Original document, 2026-08-12 — click to expand</b></summary>

## ARIA — Implementation Plan *(archived snapshot)*

**10 epics · 98 stories · 195 dependency links · 15 sprints**

Derived from `CLAUDE.md` (single source of truth) and the ARIA Confluence space.
Jira project **ARIA** · board 170 · <https://firephoenixgames.atlassian.net/jira/software/projects/ARIA/boards/170/backlog>

Ordering comes purely from the `Blocks` dependency graph. No story points, dates or velocity
assumptions — sprint count is set by the critical path, not by capacity.

Sprint sizes: `[9, 8, 7, 6, 5, 5, 10, 10, 10, 5, 6, 5, 4, 4, 4]`

## How to use this

The Atlassian tooling available to me can create issues and issue links but **cannot create
sprints**. Create 15 sprints on board 170 and drag each story below into its sprint. Every story
lists what it waits on, so you can spot-check any placement without rebuilding the graph.

## Epics

| Key | Epic | Stories |
|---|---|---|
| ARIA-1 | Platform Foundation | 9 |
| ARIA-2 | Keycloak / Identity | 9 |
| ARIA-3 | Knowledge Core | 13 |
| ARIA-4 | Gateway | 9 |
| ARIA-5 | Speech | 8 |
| ARIA-6 | Agent Core (DEC) | 10 |
| ARIA-7 | MCP Registry | 11 |
| ARIA-8 | Deployment & Infra | 10 |
| ARIA-9 | Observability | 8 |
| ARIA-10 | Self-Extension | 11 |

## Critical path — sets the 15-sprint floor

1. **ARIA-11** · _Platform Foundation_ · Create the `aria` Cargo workspace skeleton
2. **ARIA-12** · _Platform Foundation_ · Define the `proto/` directory layout and per-service schema versioning convention
3. **ARIA-13** · _Platform Foundation_ · Wire up `tonic-build`/`prost` codegen in the shared `proto` crate
4. **ARIA-14** · _Platform Foundation_ · Prove the gRPC transport end-to-end with a unary and bidirectional-streaming reference
5. **ARIA-18** · _Platform Foundation_ · Establish the shared service conventions — error handling, config, logging init, test layout — and fill in CLAUDE.md §7
6. **ARIA-55** · _Knowledge Core_ · Bootstrap aria-knowledge-core: crate skeleton, config, gRPC contract shell, Postgres/Qdrant clients, migration harness and local dev stack
7. **ARIA-68** · _Knowledge Core_ · MCP server registry and mcp_tools: schema plus grpc/registry.rs CRUD for the MCP Registry service
8. **ARIA-75** · _MCP Registry_ · `RegisterServer` / `DeregisterServer`: dynamic runtime registration, persisted via the Knowledge Core
9. **ARIA-38** · _Keycloak / Identity_ · Provision a per-MCP-server Keycloak client with consent required and prove the account-console revocation path
10. **ARIA-101** · _Self-Extension_ · Bootstrap the Self-Extension server: own repo, MCP skeleton over streamable HTTP, own CI
11. **ARIA-102** · _Self-Extension_ · draft_service: turn a natural-language ask into a persisted, validated service spec
12. **ARIA-103** · _Self-Extension_ · generate_code: scaffold a buildable standalone MCP server from a stored spec
13. **ARIA-104** · _Self-Extension_ · run_tests: build and run a candidate in an ephemeral sandbox with no real credentials or device access
14. **ARIA-105** · _Self-Extension_ · Approval gate: request_approval, the candidate lifecycle state machine, and a recorded approval as the only path to deploy
15. **ARIA-106** · _Self-Extension_ · deploy_service: build the image and call the MCP Registry's RegisterServer, post-approval only

Six of these fifteen levels are the Self-Extension epic, which can only be built once the
Registry, Identity's per-server clients and image publishing all exist. That epic is the single
biggest lever on total length.

## Biggest bottlenecks

| Story | Epic | Blocks |
|---|---|---|
| ARIA-55 | Knowledge Core | 11 others |
| ARIA-18 | Platform Foundation | 8 others |
| ARIA-81 | Deployment & Infra | 7 others |
| ARIA-75 | MCP Registry | 7 others |
| ARIA-60 | Observability | 7 others |
| ARIA-79 | Deployment & Infra | 5 others |

---

## Sprints


### Sprint 1 — 9 stories

| Key | Epic | Story | Waits on |
|---|---|---|---|
| **ARIA-40** | Agent Core (DEC) | Spike: pick the hosted frontier LLM provider and model for the Agent Core default backend | — *(nothing)* |
| **ARIA-79** | Deployment & Infra | Spike: confirm the k3s cluster baseline for ARIA — nodes, storage classes, GPU runtime, registry | — *(nothing)* |
| **ARIA-19** | Keycloak / Identity | Register the ARIA client and its client roles in the existing shared Keycloak realm | — *(nothing)* |
| **ARIA-28** | Keycloak / Identity | aria-identity: sign and verify the propagated end-user context | — *(nothing)* |
| **ARIA-43** | Knowledge Core | Decision spike: what gets embedded into Qdrant vs. kept as structured Postgres attributes | — *(nothing)* |
| **ARIA-47** | Knowledge Core | Decision spike: write-permission model for connected MCP services writing into the Knowledge Core | — *(nothing)* |
| **ARIA-57** | Knowledge Core | Design the social graph schema: people and relationships replacing the legacy relation/relation_type tables and the sharded whiteboard graph store | — *(nothing)* |
| **ARIA-65** | MCP Registry | Spike: confirm the Rust MCP SDK to standardize on (`rmcp` or equivalent) | — *(nothing)* |
| **ARIA-11** | Platform Foundation | Create the `aria` Cargo workspace skeleton | — *(nothing)* |

### Sprint 2 — 8 stories

| Key | Epic | Story | Waits on |
|---|---|---|---|
| **ARIA-84** | Deployment & Infra | Spike: decide the in-cluster Postgres operator/chart and the Qdrant deployment topology | ARIA-79 |
| **ARIA-20** | Keycloak / Identity | aria-identity: OIDC discovery and JWKS fetch with caching | ARIA-11, ARIA-19 |
| **ARIA-23** | Keycloak / Identity | Provision per-service confidential clients in the shared realm for service-to-service auth | ARIA-19 |
| **ARIA-35** | Keycloak / Identity | Decision spike: fix the Keycloak scope-naming convention for per-MCP-server clients | ARIA-19 |
| **ARIA-52** | Knowledge Core | Decision spike: choose the embedding model/provider and define the embeddings client abstraction | ARIA-43 |
| **ARIA-12** | Platform Foundation | Define the `proto/` directory layout and per-service schema versioning convention | ARIA-11 |
| **ARIA-15** | Platform Foundation | CI pipeline: build, test, fmt and clippy across the workspace | ARIA-11 |
| **ARIA-27** | Speech | Spike: measure GPU VRAM headroom next to Ollama and publish the aria-speech memory budget | ARIA-79 |

### Sprint 3 — 7 stories

| Key | Epic | Story | Waits on |
|---|---|---|---|
| **ARIA-81** | Deployment & Infra | Bootstrap the `aria` namespace: service accounts, secret delivery and endpoint config for the reused Keycloak and Ollama | ARIA-19, ARIA-23, ARIA-79 |
| **ARIA-22** | Keycloak / Identity | aria-identity: validate end-user Keycloak JWTs | ARIA-20 |
| **ARIA-24** | Keycloak / Identity | aria-identity: client-credentials flow for service-to-service authentication | ARIA-20, ARIA-23 |
| **ARIA-13** | Platform Foundation | Wire up `tonic-build`/`prost` codegen in the shared `proto` crate | ARIA-12 |
| **ARIA-17** | Platform Foundation | Developer bootstrap: one task runner that reproduces every CI check locally | ARIA-15 |
| **ARIA-30** | Speech | Spike: confirm the STT library and model that fit the VRAM budget | ARIA-27 |
| **ARIA-33** | Speech | Spike: confirm the TTS library/voice and the GPU access approach | ARIA-27 |

### Sprint 4 — 6 stories

| Key | Epic | Story | Waits on |
|---|---|---|---|
| **ARIA-44** | Agent Core (DEC) | Spike: decide the routing rule for preferring the Ollama fallback over the hosted API default | — *(nothing)* |
| **ARIA-89** | Deployment & Infra | Deploy Qdrant in the `aria` namespace with durable storage | ARIA-52, ARIA-81, ARIA-84 |
| **ARIA-26** | Gateway | Define the Gateway Interface gRPC contract: `Converse` (bidi stream) + `SendCommand` (unary) | ARIA-13, ARIA-30 |
| **ARIA-31** | Keycloak / Identity | aria-identity: tonic/tower interceptor layer wiring validation, client credentials and user context | ARIA-13, ARIA-22, ARIA-24, ARIA-28 |
| **ARIA-14** | Platform Foundation | Prove the gRPC transport end-to-end with a unary and bidirectional-streaming reference | ARIA-13 |
| **ARIA-16** | Platform Foundation | CI: enforce protobuf hygiene — generated types current, schemas linted, breaking changes caught | ARIA-13, ARIA-15 |

### Sprint 5 — 5 stories

| Key | Epic | Story | Waits on |
|---|---|---|---|
| **ARIA-18** | Platform Foundation | Establish the shared service conventions — error handling, config, logging init, test layout — and fill in CLAUDE.md §7 | ARIA-11, ARIA-14 |
| **ARIA-97** | Self-Extension | Spike: decide the approval surface for request_approval — GitHub PR, conversational, or both | — *(nothing)* |
| **ARIA-98** | Self-Extension | Spike: decide where sandboxed run_tests execution runs for a candidate service | — *(nothing)* |
| **ARIA-100** | Self-Extension | Spike: decide how declared tools get a risk tier during draft_service and registration | — *(nothing)* |
| **ARIA-37** | Speech | Define the aria-speech protobuf contract: streaming Transcribe and Synthesize | ARIA-26 |

### Sprint 6 — 5 stories

| Key | Epic | Story | Waits on |
|---|---|---|---|
| **ARIA-48** | Agent Core (DEC) | Scaffold aria-agent-core: crate, config, tracing, and the Decide gRPC contract | ARIA-18, ARIA-31 |
| **ARIA-25** | Gateway | Scaffold the `aria-gateway` crate: bootstrap, config and tonic server lifecycle | ARIA-18 |
| **ARIA-55** | Knowledge Core | Bootstrap aria-knowledge-core: crate skeleton, config, gRPC contract shell, Postgres/Qdrant clients, migration harness and local dev stack | ARIA-18 |
| **ARIA-69** | MCP Registry | `mcp-client` shared crate: streamable-HTTP sessions to unmodified MCP servers | ARIA-18, ARIA-65 |
| **ARIA-60** | Observability | aria-observability: shared tracing + OTLP init behind a vendor-neutral interface | ARIA-18 |

### Sprint 7 — 10 stories

| Key | Epic | Story | Waits on |
|---|---|---|---|
| **ARIA-50** | Agent Core (DEC) | Define the LlmBackend trait and a backend-agnostic conformance test suite | ARIA-48 |
| **ARIA-87** | Deployment & Infra | Deploy Postgres in the `aria` namespace with durable storage | ARIA-55, ARIA-81, ARIA-84 |
| **ARIA-29** | Gateway | Wire `aria-identity` JWT validation into the Gateway auth interceptor | ARIA-22, ARIA-25 |
| **ARIA-59** | Knowledge Core | Implement the social graph: migration plus people and relationship gRPC API (grpc/people.rs) | ARIA-31, ARIA-55, ARIA-57 |
| **ARIA-68** | Knowledge Core | MCP server registry and mcp_tools: schema plus grpc/registry.rs CRUD for the MCP Registry service | ARIA-35, ARIA-55 |
| **ARIA-76** | Knowledge Core | Semantic memory: Qdrant collection setup and the grpc/memory.rs embed/retrieve API | ARIA-43, ARIA-47, ARIA-52, ARIA-55 |
| **ARIA-72** | MCP Registry | Scaffold `aria-mcp-registry` and define its gRPC contract in `proto/` | ARIA-18, ARIA-60 |
| **ARIA-66** | Observability | Span and attribute conventions: naming, required attributes, and a code-enforced attribute allow-list | ARIA-60 |
| **ARIA-21** | Platform Foundation | Spike: decide and document the packaging convention — plain manifests or a Helm chart per service | ARIA-25 |
| **ARIA-42** | Speech | Scaffold the aria-speech service: crate, config, tonic server, health and telemetry | ARIA-18, ARIA-31, ARIA-37, ARIA-60 |

### Sprint 8 — 10 stories

| Key | Epic | Story | Waits on |
|---|---|---|---|
| **ARIA-53** | Agent Core (DEC) | Implement the hosted frontier API LlmBackend (default path) | ARIA-40, ARIA-50, ARIA-81 |
| **ARIA-54** | Agent Core (DEC) | Implement the Ollama LlmBackend against the existing GPU-node instance (fallback path) | ARIA-50 |
| **ARIA-58** | Agent Core (DEC) | Retrieve context from the Knowledge Core and assemble it into the LLM prompt | ARIA-48, ARIA-76 |
| **ARIA-86** | Deployment & Infra | Build and publish container images for the deployed ARIA services | ARIA-15, ARIA-21, ARIA-79 |
| **ARIA-96** | Deployment & Infra | Back up and restore the Postgres and Qdrant persistent data | ARIA-87, ARIA-89 |
| **ARIA-32** | Gateway | Mint the signed end-user context and attach it to every downstream gRPC call | ARIA-28, ARIA-29 |
| **ARIA-70** | Knowledge Core | consent_grants table with granted_by, plus the grant accessors the MCP Registry calls | ARIA-55, ARIA-59, ARIA-68 |
| **ARIA-75** | MCP Registry | `RegisterServer` / `DeregisterServer`: dynamic runtime registration, persisted via the Knowledge Core | ARIA-65, ARIA-68, ARIA-69, ARIA-72 |
| **ARIA-71** | Observability | Run AppSignal's self-hosted collector in the aria namespace as the only telemetry egress | ARIA-21, ARIA-60, ARIA-81 |
| **ARIA-46** | Speech | Load model weights from the PVC and enforce a VRAM-aware residency and admission policy | ARIA-27, ARIA-42 |

### Sprint 9 — 10 stories

| Key | Epic | Story | Waits on |
|---|---|---|---|
| **ARIA-56** | Agent Core (DEC) | Wire backend selection and hosted-to-Ollama failover behind the LlmBackend interface | ARIA-44, ARIA-53, ARIA-54 |
| **ARIA-91** | Deployment & Infra | Deploy the stateless ARIA services as Deployments behind ClusterIP Services | ARIA-21, ARIA-25, ARIA-48, ARIA-55, ARIA-72, ARIA-81, ARIA-86, ARIA-87, ARIA-89 |
| **ARIA-34** | Gateway | Implement `SendCommand` and the downstream Agent Core gRPC client | ARIA-24, ARIA-26, ARIA-32, ARIA-48 |
| **ARIA-38** | Keycloak / Identity | Provision a per-MCP-server Keycloak client with consent required and prove the account-console revocation path | ARIA-35, ARIA-75 |
| **ARIA-73** | Knowledge Core | consent_audit_log: append-only invocation record plus append and query accessors | ARIA-55, ARIA-68, ARIA-70 |
| **ARIA-78** | MCP Registry | Approval gate in front of registration: refuse MCP servers whose code was never approved | ARIA-75 |
| **ARIA-82** | MCP Registry | Resolve the caller only from the Gateway's signed context metadata, never from tool arguments | ARIA-31, ARIA-32 |
| **ARIA-92** | MCP Registry | `ListTools`: aggregate the tool catalogue across registered servers for the Agent Core | ARIA-72, ARIA-75 |
| **ARIA-74** | Observability | Collector-side scrubbing pipeline: enforce that no conversation or voice content is exported | ARIA-66, ARIA-71 |
| **ARIA-49** | Speech | Implement streaming Transcribe (STT) end to end | ARIA-30, ARIA-46 |

### Sprint 10 — 5 stories

| Key | Epic | Story | Waits on |
|---|---|---|---|
| **ARIA-94** | Deployment & Infra | Deployment pattern for MCP servers: in-cluster workloads and remote LAN endpoints | ARIA-75, ARIA-91 |
| **ARIA-36** | Gateway | Implement `Converse` bidirectional audio transport: chunk framing and buffering | ARIA-26, ARIA-34, ARIA-37 |
| **ARIA-108** | Knowledge Core | Unrecognized-request log: schema and write accessor for the Agent Core's unknown fallback | ARIA-55 |
| **ARIA-85** | MCP Registry | Audit writer: record every tool invocation attempt, allowed or denied, to `consent_audit_log` | ARIA-73, ARIA-82 |
| **ARIA-101** | Self-Extension | Bootstrap the Self-Extension server: own repo, MCP skeleton over streamable HTTP, own CI | ARIA-38, ARIA-75, ARIA-86 |

### Sprint 11 — 6 stories

| Key | Epic | Story | Waits on |
|---|---|---|---|
| **ARIA-39** | Gateway | Stream flow control: backpressure, per-stream limits and session teardown | ARIA-36 |
| **ARIA-45** | Gateway | Minimal CLI test harness for driving `Converse` and `SendCommand` | ARIA-36 |
| **ARIA-88** | MCP Registry | Consent gate 1: verify the user's Keycloak-level consent for the MCP server has not been revoked | ARIA-38, ARIA-85 |
| **ARIA-90** | MCP Registry | Consent gate 2: require an active per-tool grant in `consent_grants` before forwarding | ARIA-70, ARIA-85 |
| **ARIA-83** | Observability | Correlate security audit events with traces without leaking audit content into telemetry | ARIA-60, ARIA-73, ARIA-85 |
| **ARIA-102** | Self-Extension | draft_service: turn a natural-language ask into a persisted, validated service spec | ARIA-100, ARIA-101 |

### Sprint 12 — 5 stories

| Key | Epic | Story | Waits on |
|---|---|---|---|
| **ARIA-93** | Deployment & Infra | Deploy `aria-speech` pinned to the GPU node with a PVC for model weights | ARIA-27, ARIA-46, ARIA-79, ARIA-81, ARIA-86 |
| **ARIA-41** | Gateway | Assemble the Gateway `tower` middleware stack: tracing spans, error-to-Status mapping, request limits | ARIA-25, ARIA-60 |
| **ARIA-95** | MCP Registry | `CallTool`: the enforced forwarding path — both gates, then audit, then forward | ARIA-69, ARIA-72, ARIA-75, ARIA-88, ARIA-90 |
| **ARIA-99** | Self-Extension | Spike: decide credential provisioning for a newly approved service | — *(nothing)* |
| **ARIA-103** | Self-Extension | generate_code: scaffold a buildable standalone MCP server from a stored spec | ARIA-18, ARIA-102 |

### Sprint 13 — 4 stories

| Key | Epic | Story | Waits on |
|---|---|---|---|
| **ARIA-61** | Agent Core (DEC) | MCP tool-calling loop: fetch the toolset from the MCP Registry, execute the model's chosen call, and block identity spoofing | ARIA-48, ARIA-50, ARIA-92, ARIA-95 |
| **ARIA-67** | Observability | Trace-context propagation across every gRPC boundary, including bidirectional streams | ARIA-31, ARIA-60 |
| **ARIA-104** | Self-Extension | run_tests: build and run a candidate in an ephemeral sandbox with no real credentials or device access | ARIA-81, ARIA-98, ARIA-103 |
| **ARIA-107** | Self-Extension | Credential handling across the candidate lifecycle: nothing before approval, provisioned path after | ARIA-99, ARIA-103 |

### Sprint 14 — 4 stories

| Key | Epic | Story | Waits on |
|---|---|---|---|
| **ARIA-63** | Agent Core (DEC) | Log unrecognized requests back to the Knowledge Core (replaces the old Unknown fallback box) | ARIA-55, ARIA-58, ARIA-61, ARIA-108 |
| **ARIA-77** | Observability | End-to-end leak test: canary conversation content must appear in zero exported spans or logs | ARIA-15, ARIA-41, ARIA-49, ARIA-61, ARIA-67, ARIA-74, ARIA-76, ARIA-95 |
| **ARIA-80** | Observability | Measure AppSignal's billable-request definition against ARIA's per-interaction fan-out | ARIA-41, ARIA-61, ARIA-67 |
| **ARIA-105** | Self-Extension | Approval gate: request_approval, the candidate lifecycle state machine, and a recorded approval as the only path to deploy | ARIA-97, ARIA-104 |

### Sprint 15 — 4 stories

| Key | Epic | Story | Waits on |
|---|---|---|---|
| **ARIA-62** | Knowledge Core | Preferences-as-attributes: schema and read/write gRPC API | ARIA-43, ARIA-47, ARIA-55, ARIA-59 |
| **ARIA-64** | Knowledge Core | Calendar: Postgres schema and gRPC accessors | ARIA-55, ARIA-59 |
| **ARIA-106** | Self-Extension | deploy_service: build the image and call the MCP Registry's RegisterServer, post-approval only | ARIA-75, ARIA-86, ARIA-105, ARIA-107 |
| **ARIA-51** | Speech | Implement streaming Synthesize (TTS) end to end | ARIA-33, ARIA-46 |

</details>

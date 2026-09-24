# ARIA — Roadmap

[Library index](../README.md) · [Decisions](../decisions/README.md) · [Open questions](../open-questions/README.md) · [Jira rulebook](jira-rulebook.md) · [Sprint guideline](sprint-guideline.md)

The themes ARIA is built in, **in dependency order**. Each has a one-line scope, a gate saying
when a sprint may focus on it, and a status. There are **no dates and no story assignments** here:
sprints are planned one at a time from this page ([sprint-guideline.md](sprint-guideline.md)), and
every Jira story carries exactly one theme label ([jira-rulebook.md](jira-rulebook.md)).

A gate names what must **run** or be **decided** first. Where a gate depends on an open question,
it links to it — this page never answers one. A theme stays *waiting* until the answer is in
[`decisions/`](../decisions/README.md).

| Status | Meaning |
|---|---|
| **ready** | Gate met; the next sprint may focus here |
| **in progress** | Has merged work and remaining scope, or is the current sprint's focus |
| **waiting** | Gate not met |
| **done** | Scope delivered and demonstrated |

---

## The walking skeleton

Themes 1–5 exist to reach one milestone:

> **One end-to-end text turn — authenticated, persisted, and visible in the web shell.**

Each of those themes contributes only a thin slice to it; the rest of their scope follows
afterwards. **No sprint focuses on theme 6 or later until the skeleton runs.**

**Status:** not running.

---

## 1 · Foundations — `theme-foundations`

Workspace, proto conventions, CI, container images.

- **Ready when:** now.
- **Status:** in progress — the Cargo workspace exists; proto generation, CI and image publishing
  do not.

## 2 · Conversation path (thin) — `theme-conversation`

Gateway `Converse`, Agent Core `Decide` and the hosted LLM backend — text only.

- **Ready when:** foundations exist — protos build, CI is green, and an image can be deployed to
  the cluster.
- **Status:** waiting.
- **Open:** the hosted provider and model are
  [awaiting a measurement](../open-questions/awaiting-measurement.md).

## 3 · Identity & context — `theme-identity`

Keycloak client, PKCE login, the Ed25519 end-user context, service-to-service auth.

- **Ready when:** a text turn runs on the cluster, so there is a path to authenticate.
- **Status:** waiting — sign and verify for the context already exist in `crates/identity`.
- **Open** ([needs-decision](../open-questions/needs-decision.md)): Gateway signing-key
  generation, storage and rotation; how pods trust `fpg-ca`; service-to-service mTLS.

## 4 · Data platform — `theme-data`

Postgres VM, Qdrant VM, `aria-storage-broker`, Vault credentials, migrations, the Knowledge Core
service.

- **Ready when:** a verified `user_id` reaches downstream services, so persisted rows have an owner.
- **Status:** waiting.
- **Open:** VM sizing is [awaiting a measurement](../open-questions/awaiting-measurement.md); VM
  ownership (patching, monitoring scope) [needs a decision](../open-questions/needs-decision.md).

## 5 · Clients & UI — `theme-clients`

Core Web shell, a chat view over the turn log, native clients.

- **Ready when:** a persisted turn can be read back through the Gateway's Service router. Native
  clients also wait on what they are built with
  ([needs-decision](../open-questions/needs-decision.md)).
- **Status:** waiting.

---

*Walking skeleton gate — themes 6–11 start only after it runs.*

---

## 6 · Tools & extension — `theme-tools`

MCP Registry, approval and consent gates, `aria-kc-broker`, the first real MCP server.

- **Ready when:** the skeleton runs and the Rust MCP SDK is
  [measured](../open-questions/awaiting-measurement.md). Two
  [open decisions](../open-questions/needs-decision.md) gate parts of it: how ARIA prevents
  hallucinated required arguments — before the Agent Core executes a model-chosen tool call; and
  whether `aria-kc-broker`'s credential can be scoped below master-realm admin — before the broker
  is built.
- **Status:** waiting.

## 7 · Speech — `theme-speech`

VAD, STT, TTS, barge-in.

- **Ready when:** the skeleton runs; where `aria-speech` can run at all is decided — the GPU is
  outside the cluster ([needs-decision](../open-questions/needs-decision.md)); and the STT/TTS
  choice and VRAM budget are [measured](../open-questions/awaiting-measurement.md).
- **Status:** waiting — a preliminary GPU baseline exists
  ([ARIA-27 brief](../spikes/ARIA-27-gpu-baseline.md)).

## 8 · Notifications — `theme-notify`

`aria-notify`, delivery paths, gating.

- **Ready when:** a client holds a live stream to deliver to (theme 5) and the consent gate exists
  (theme 6). The offline push path, who may send, and delivery semantics are
  [undecided](../open-questions/needs-decision.md) — the push half waits on them.
- **Status:** waiting.

## 9 · Observability — `theme-observability`

The in-cluster collector, Prometheus, Jaeger, trace propagation, the metric set.

- **Ready when:** the skeleton runs. Services emit spans from their first story (Definition of Done
  in the [rulebook](jira-rulebook.md)); this theme gives those spans somewhere to go.
- **Status:** waiting.
- **Open:** whether the collector and Jaeger fit on the node is
  [awaiting a measurement](../open-questions/awaiting-measurement.md); Jaeger retention
  [needs a decision](../open-questions/needs-decision.md).

## 10 · Self-extension — `theme-selfext`

The Self-Extension MCP server and its approval pipeline.

- **Ready when:** theme 6 runs end to end with one approved server, and the self-extension items in
  [needs-decision](../open-questions/needs-decision.md) are decided — approval surface, the
  server's cluster authority, who tests generated code, and which language and model generate it.
- **Status:** waiting.

## 11 · Operations — `theme-ops`

Backups, restore drill, capacity, secrets.

- **Ready when:** the Postgres and Qdrant VMs hold data worth restoring (theme 4).
- **Status:** waiting — the cluster baseline is recorded
  ([ARIA-79 findings](../spikes/ARIA-79-cluster-findings-2026-08-19.md)).
- **Open:** the cost of a real restore is
  [a measurement](../open-questions/awaiting-measurement.md); custody of the off-site backup key
  [needs a decision](../open-questions/needs-decision.md). Capacity on the single schedulable node
  is unsized — a risk to every theme that deploys, not only this one.

---

## Keeping this page true

Update a theme's status when a sprint closes ([sprint-guideline.md](sprint-guideline.md)). Add a
theme only with a label in the [rulebook](jira-rulebook.md) in the same change. Reorder only when
a dependency changes, and say which one.

*Created 2026-09-24.*

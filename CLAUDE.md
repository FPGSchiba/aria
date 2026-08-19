# CLAUDE.md — ARIA

Instructions for any Claude/Cowork session working on this project. **Read this file first, then
follow the pointers.** It is deliberately short: it orients you and states the rules. The
substance lives in [`docs/`](docs/README.md).

ARIA is being redesigned from a 2021 personal-assistant prototype (Python, everything
hand-rolled) into a modern, self-hostable, multi-service assistant.

---

## 1. Orientation — read in this order

| # | Read | For |
|---|---|---|
| 1 | [`docs/01-vision.md`](docs/01-vision.md) | What ARIA is, who it serves, explicit non-goals |
| 2 | [`docs/02-stack.md`](docs/02-stack.md) | Every settled technology choice + rationale |
| 3 | [`docs/03-architecture.md`](docs/03-architecture.md) | The services and what each owns |
| 4 | [`docs/04-deployment.md`](docs/04-deployment.md) | Where it runs, what it reuses, what leaves the network |
| 5 | [`docs/05-conventions.md`](docs/05-conventions.md) | Repo layout, crates, code conventions |
| 6 | [`docs/decisions/README.md`](docs/decisions/README.md) | **The reasoning trail** — D1–D50, with rejected alternatives |
| 7 | [`docs/open-questions/README.md`](docs/open-questions/README.md) | What is *not* decided, and what kind of answer each needs |
| 8 | [`docs/sprints/sprint-01.md`](docs/sprints/sprint-01.md) | What is being worked right now |

Full library map and the "where does a new fact go?" decision tree:
[`docs/README.md`](docs/README.md).

> **Where this library lives (D54).** `docs/` is authoritative **in the `aria` monorepo on GitHub**,
> not in Google Drive. Jira stories link to GitHub file URLs; Confluence is a read-only mirror. If
> you are reading a copy in Drive, check it against the repo before trusting it.

---

## 2. The rules

These are load-bearing. Each exists because its absence already cost this project time.

1. **Never invent an architecture decision.** If Jann hasn't decided it, it stays marked open in
   `docs/open-questions/`. This rule is why this documentation is trustworthy — don't break it.

2. **Respect the three-way triage.** `open-questions/` is split by what kind of answer an item
   needs. Items in `awaiting-measurement.md` **cannot be settled by discussion** — answering one
   in chat manufactures certainty this project has deliberately refused to fake.

3. **`docs/decisions/` is append-only.** Supersede with a new entry; never edit a past one.
   Several entries carry explicit **revisit triggers** — don't reopen them until the trigger fires.

4. **Follow consequences through.** When a choice creates new work or new risk, record that in
   the same edit rather than smoothing it over.

5. **Reconcile contradictions, don't record them.** If two answers conflict, stop and resolve it.

6. **Keep the three surfaces consistent** — this library, Confluence, and Jira. A decision landing
   in only one is worse than one recorded nowhere, because it looks authoritative.

7. **No story points, dates, or velocity estimates.** Ordering comes from dependencies only.

8. **Write Jira stories to the house style** — [`docs/backlog/story-style.md`](docs/backlog/story-style.md).
   A story is ~150–200 words: goal, "not this story", testable criteria, decisions as **one-line**
   citations, open items, links. **A decision in a story is one sentence and a D-number, never a
   paragraph** — the reasoning lives in `docs/decisions/` and the story links to it. Never quote
   the architecture docs into a story; quotes go stale silently.

9. **Ask before any large batch write or anything hard to undo.**

---

## 3. Where the other surfaces are

**Jira** — project `ARIA`, Scrum board 170, cloudId `d5d40395-9142-4047-bb1c-387c991640b9`.
Field IDs, tooling limits and the known-stale derived plan:
[`docs/backlog/README.md`](docs/backlog/README.md).

**Repo** — <https://github.com/FPGSchiba/aria>. **Holds this library at `docs/` (D54)** and is the
authoritative copy. Local clone: `D:\Projects\aria`.

**Confluence** — space **ARIA**, <https://firephoenixgames.atlassian.net/wiki/spaces/ARIA>.
A **read-only mirror** since D54. Page IDs in `docs/backlog/README.md`.

**Legacy source material** (2021 prototype — historical context, *not* current design):
`ARIA Documentation.docx`, `Planing/ARIA User API.docx`, `Planing/DB Planning/*`,
`Planing/Overviews/*`, `Planing/Datagraph-Concept.JPG`.

**Superseded working files** live in [`archive/`](archive/) with a note on what replaced each.

---

## 4. Version history

| Date | Editor | Note |
|---|---|---|
| 2026-08-12 | Jann Erhardt + Claude | Initial rewrite of project direction: Python → Rust, gRPC (`tonic`) for internal service communication, Keycloak for identity, Qdrant + Postgres replacing the hand-built graph store. Superseded the 2021 `ARIA Documentation.docx` plan. |
| 2026-08-12 | Jann Erhardt + Claude | Revised vision: reframed ARIA as an MCP Host with a growing, externally-extensible toolset; reframed "user management" as a Knowledge Core / RAG system distinct from Keycloak's strictly-authN/authZ role; retired hand-built NLU/NLG in favor of a pluggable hosted-LLM-driven Agent Core. |
| 2026-08-12 | Jann Erhardt + Claude | Added deployment & hosting decisions: target the existing home k3s cluster (not new infra); reuse the existing Keycloak instance (shared realm, new client) and the existing Ollama/GPU setup (fallback LLM backend, default stays hosted API); MCP transport defaults to HTTP in this k8s context rather than stdio; service-to-service auth reuses Keycloak client-credentials. |
| 2026-08-12 | Jann Erhardt + Claude | Added per-service repo/crate naming and structure (documented in full on individual Confluence pages); confirmed standard MCP stays unmodified with ARIA-specific extensions in the MCP Registry's control plane; added the Self-Extension MCP Server as the first server to build, with a proposed PR-based approval gate before it can deploy anything it generates. |
| 2026-08-12 | Jann Erhardt + Claude | Closed two gaps: (1) Observability — OpenTelemetry instrumentation exported to AppSignal via its self-hosted collector, run in-cluster. (2) Identity & consent — Gateway-issued end-user context propagated on every internal call (never LLM-fillable), plus a two-layer consent model: Keycloak per-server client consent (coarse, free revocation UI) + a per-tool `consent_grants`/`consent_audit_log` schema in the Knowledge Core (fine-grained, needed from day one per Jann), enforced by the MCP Registry. Distinguished consent (per-user, ongoing) from the earlier approval gate (admin-level, one-time, about code trust). Rejected full Keycloak UMA as unnecessary ceremony at this scale. |
| 2026-08-12 | Jann Erhardt + Claude | **46 architecture decisions taken in one session** (full reasoning and rejected alternatives in [`docs/decisions/`](docs/decisions/README.md)). Resolved all four documented contradictions: the Knowledge Core is a deployed service (§4 was wrong); `mcp-client` gains a §5 path; preferences are partitioned by kind rather than dual-written; and enforcement identity is separated from contextual identity, with the MCP Registry — not the Agent Core — injecting a reserved, non-LLM-fillable argument. Fixed the identity foundation (Ed25519 Gateway-minted context, `sub` as `user_id`, audience-restricted service-to-service auth, PKCE + Device Grant). Chose per-tool Keycloak scopes, which forced runtime provisioning and therefore a new `aria-kc-broker` service holding the only Keycloak admin credential. Settled the audio pipeline (English-only STT, Opus on the client link, VAD owned by Speech, many turns per stream with barge-in) and the Agent Core's shape (`session_id`, server-streaming `Decide`, no MCP SDK). Added sealed-secrets, GitHub Actions, one umbrella Helm chart, and encrypted off-site backups — narrowing "local by default" to "no personal data leaves in plaintext". Closed the largest hole in the consent design with conversational granting and automatic retry. Restructured §8 to separate decisions from measurements. |
| 2026-08-19 | Jann Erhardt + Claude | **Documentation restructured into an information library.** The single 40 KB CLAUDE.md became this router plus [`docs/`](docs/README.md): vision, stack, architecture, deployment and conventions as separate pages; the 46-decision record split into an indexed `decisions/` tree; §8 split into a live `open-questions/` tree preserving the three-way triage; per-service pages; a `spikes/` directory for research briefs; `backlog/` for Jira coordinates and the known-stale derived plan. Transient session artefacts moved to `archive/`. **No decision content was changed** — the split is verbatim, verified by diff. Added spike briefs for ARIA-65 (Rust MCP SDK), ARIA-40 (hosted LLM backend) and ARIA-43 (knowledge classification), plus a cluster-baseline discovery script for ARIA-79 and a Keycloak fact checklist for ARIA-19. |
| 2026-08-19 | Jann Erhardt + Claude | **Measurement session — D47–D50, plus the first evidence-backed spike results.** Ran a tool-calling battery against the real GPU node: `qwen3:8b` leads on spurious-call rate (0/5 vs llama3.1:8b's 3/5), `qwen2.5-coder:7b` cannot tool-call at all. **Both capable models fabricated a required argument on 100% of runs** — a design constraint on ARIA, not a model defect, now tracked as **ARIA-121** (blocks ARIA-61). Diagnosed and fixed a GPU-passthrough outage that had Ollama silently running on CPU: an LXC `bind,optional,create=file` mount created empty placeholder files because the host's device nodes did not exist at container start. Recorded the GPU baseline (RTX 2070, 8 GB, ≈2.6 GB left for Speech at a 4096 context — preliminary). Four decisions taken: **D47** two Keycloak client roles (`aria-user`, `aria-admin`); **D48** the typed/free-text classification rule with free-text as the default, closing ARIA-43's central question and publishing the Qdrant-primary list for ARIA-96; **D49** an ARIA-owned canonical tool-call shape, after a measurement removed the argument for adopting a vendor's; **D50** `aria-speech` asserts GPU availability at startup and fails closed. **No story was moved to Done** — every sprint-1 spike still has unmet acceptance criteria, and ARIA-109's largest gap (streaming tool-call interleaving, required by D37) is now measurable but not yet measured. |

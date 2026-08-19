# ARIA — Backlog coordinates & tooling notes

[Library index](../README.md) · [Decisions](../decisions/README.md) · [Open questions](../open-questions/README.md) · [Sprint 1](../sprints/sprint-01.md)

Everything a session needs to work with Jira and Confluence without rediscovering it. **Live Jira
is always authoritative** — this page holds coordinates, not state.

---

## Jira

| Fact | Value |
|---|---|
| Site | <https://firephoenixgames.atlassian.net> |
| Repo | <https://github.com/FPGSchiba/aria> (local clone: `D:\Projects\aria`) |
| cloudId | `d5d40395-9142-4047-bb1c-387c991640b9` |
| Project key | `ARIA` |
| Board | Scrum board **170** |
| Issue types | exactly `Epic` and `Story` |
| Link type | exactly `Blocks` (inward = blocker, outward = blocked) |
| Sprint field | `customfield_10020` |
| Transition to Done | id `41` |
| Sprints | **16**, ids **473–488** — sprint *n* = id 472+*n* |

### Epics

| Key | Epic |
|---|---|
| ARIA-1 | Platform Foundation |
| ARIA-2 | Keycloak Integration & aria-identity |
| ARIA-3 | Knowledge Core |
| ARIA-4 | Gateway |
| ARIA-5 | Speech |
| ARIA-6 | Agent Core (DEC) |
| ARIA-7 | MCP Registry |
| ARIA-8 | Deployment & Infra |
| ARIA-9 | Observability |
| ARIA-10 | Self-Extension |

**110 stories** (ARIA-11 … ARIA-120), each parented to its epic. **226 `Blocks` links** + 18
`Relates` links.

### The invariant

> Every `Blocks` blocker sits in a **strictly earlier sprint** than the story it blocks.

Verified across all 226 edges at the end of the 2026-08-12 decision session. Any session that
moves stories between sprints must re-verify this **by reading the links back from Jira**, not
from an in-memory graph.

### Stories closed or repurposed

- **Done:** ARIA-35, ARIA-44, ARIA-47, ARIA-99 — four spikes resolved by decisions. They keep
  their outgoing `Blocks` links deliberately: a Done blocker holds nothing up, and the links
  record that the dependency was once real.
- **ARIA-11** — Done (Cargo workspace skeleton). *Verified live 2026-08-19.*
- **ARIA-79, ARIA-43** — Done 2026-08-19, each with a closing comment mapping every acceptance
  criterion to its evidence. Sprint 1 is now **4 of 9** complete.
- **ARIA-21 was repurposed**, not closed: now "Author the umbrella Helm chart for the aria
  namespace", and still genuinely blocks ARIA-71, ARIA-86 and ARIA-91.

### Created by the 2026-08-12 session

ARIA-109 (Ollama model spike) · ARIA-110 (`aria-kc-broker`) · ARIA-111 (deploy broker) ·
ARIA-112 (sealed-secrets controller) · ARIA-113 (Gateway signing keys) · ARIA-114 (circle
onboarding) · ARIA-115 (conversation history) · ARIA-116 (echo cancellation) · ARIA-117
(NetworkPolicy) · ARIA-118 (`consent required` path) · ARIA-119 (off-site backups) ·
ARIA-120 (calendar MCP server).

### Created 2026-08-19 (measurement session)

**ARIA-121** — *Decision spike: how ARIA prevents a model inventing a required tool argument.*
Sprint 12, parented to ARIA-6 (Agent Core), **blocks ARIA-61**. Created from a measured finding
with no existing owner: both tool-capable local models fabricated a required argument on 100% of
runs where the user supplied none. Evidence:
[ARIA-109 brief](../spikes/ARIA-109-ollama-model.md).

Story count is now **111**; `Blocks` edges **227**. Invariant re-checked for the new edge only
(ARIA-121 sprint 12 → ARIA-61 sprint 13 ✓); the other 226 were not re-verified in this session.

---

## Confluence

Same cloudId. Space key **ARIA**, space id `196411395`.

| Page | ID |
|---|---|
| ARIA — Vision | 196542465 |
| ARIA — Architecture & Hosting | 196575233 |
| ARIA — Decision Log | *child of Architecture & Hosting — look it up* |
| ARIA — Gateway | 196411754 |
| ARIA — Speech | 196509698 |
| ARIA — Agent Core (DEC) | 196378626 |
| ARIA — MCP Registry | 196608001 |
| ARIA — Knowledge Core | 196640769 |
| ARIA — Identity | 196673537 |
| ARIA — MCPs | 196706305 |
| ARIA — Observability, Identity & Consent | 196739073 |

Every page ends with "Full rationale lives in `CLAUDE.md`" and a "Last updated" line — maintain
both. **Note:** that pointer now resolves to this `docs/` library rather than a single file.

---

## Tooling limits — learned the hard way, don't rediscover these

1. **The Atlassian MCP cannot create sprints and cannot delete issue links.** For either, drive a
   logged-in Jira tab with Claude in Chrome against the Agile/REST API:
   `POST /rest/agile/1.0/sprint`, `POST /rest/agile/1.0/sprint/{id}/issue` with
   `{issues:[keys]}`, `DELETE /rest/api/3/issueLink/{id}`. That is how sprints 1–16 were made.

2. **`searchJiraIssuesUsingJql` overflows the tool-result limit on this project.** It silently
   truncates (watch for a non-zero `remainingCount` alongside `hasNextPage: false`) or saves to a
   file. The `fields` parameter does **not** reliably suppress `description`, which is what blows
   the budget — ARIA descriptions run to several KB each. Either process the saved file with
   `jq`/Python, or run the whole query in-browser with `fetch('/rest/api/3/search/jql', …)` and
   compute the answer in JS so only a summary comes back. The second is much cheaper.

3. **Story description rewrites are expensive** — roughly 10 KB of context each once the echo is
   counted. Doing 60+ inline will exhaust a session. Delegate them to parallel subagents split by
   epic, with explicit instructions to preserve everything except the resolved block.

4. **Jira's ADF round-trip is lossy in cosmetic ways** — it escapes `~` as `\~` and drops some
   italic markers wrapping inline code. Harmless, no content lost, not worth fixing.

---

## Conventions

- **[`story-style.md`](story-style.md)** — the house style for Jira story and epic descriptions.
  ~150–200 words; testable criteria; decisions as **one-line** citations with links, never
  paragraphs; never quote the architecture docs. Established 2026-08-19 because the original
  descriptions were written for an LLM needing maximum context and run 500–900 words.
- **[`story-rework-prompt.md`](story-rework-prompt.md)** — a ready-to-paste session prompt that
  applies that style to all 111 existing descriptions, one epic first for review, then nine
  subagents in parallel. **Prerequisite: the docs move (D54)**, or the rewrites get dead links.

## Derived documents

[`implementation-plan.md`](implementation-plan.md) — **STALE. Do not trust it.** It drifted from
live Jira in seven places before the 2026-08-12 session and was never regenerated afterwards. It
is kept only because its critical-path and bottleneck analysis is still structurally interesting.
Rebuild anything you need from Jira's live `Blocks` links.

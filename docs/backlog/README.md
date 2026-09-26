# ARIA — Backlog coordinates & tooling notes

[Library index](../README.md) · [Decisions](../decisions/README.md) · [Open questions](../open-questions/README.md) · [Roadmap](roadmap.md) · [Sprint plan](sprint-plan-1-2.md)

Everything a session needs to work with Jira and Confluence without rediscovering it. **Live Jira
is always authoritative** — this page holds coordinates, not state.

---

## Jira

| Fact | Value |
|---|---|
| Site | <https://firephoenixgames.atlassian.net> |
| Repo | <https://github.com/FPGSchiba/aria> (local clone: `D:\Projects\aria`) |
| cloudId | `d5d40395-9142-4047-bb1c-387c991640b9` |
| Project key | `ARIA` (team-managed) |
| Board | Scrum board **170** |
| Issue types | `Story` (10163), `Spike` (10235), `Subtask` (10160). **No epics** — the theme is a label ([rulebook](jira-rulebook.md)) |
| Story points | `customfield_10016` (*Story point estimate*) |
| Sprint field | `customfield_10020` |
| Link type | only `Blocks` (inward = blocker, outward = blocked) |
| Sprints | **521** `ARIA Sprint 1 — CLI streaming` · **522** `ARIA Sprint 2 — Logged-in CLI` |

### What the project holds (2026-09-26)

- **The 8 Done stories of the old backlog** — ARIA-11, 28, 35, 43, 44, 47, 79, 99 — kept as the
  record of work that exists. Everything else from before 2026-09-26 (10 epics, 103 open stories,
  227 Blocks links) was deleted. The full export is
  [`archive/backlog-2026-09.json`](archive/backlog-2026-09.json); old keys cited elsewhere in
  `docs/` now resolve only there.
- **21 issues and 79 subtasks** created from [`sprint-plan-1-2.md`](sprint-plan-1-2.md), which
  lists every key: sprint 1 (10 issues), sprint 2 (5), and 6 written backlog stories for the next
  two plannings. **16 Blocks links**, only where a story genuinely cannot start before another.
- The old sprints 473–488 belong to the deleted backlog and are being removed by hand.

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

1. **The Atlassian MCP cannot create sprints and cannot delete issue links.** Sprints are created
   from a logged-in Jira tab with the Agile API: `POST /rest/agile/1.0/sprint` with
   `originBoardId: 170`, a name and a goal. **Sprint names must be under 30 characters.** Issues
   can be put in a sprint at creation through `customfield_10020`; **subtasks follow their
   parent's sprint** automatically. A wrong-direction link cannot be removed through the MCP, so
   check the direction on the first link before creating the rest.

2. **`searchJiraIssuesUsingJql` overflows the tool-result limit on this project.** An explicit
   `fields` list does restrict the fields returned, but a page of 100 issues with descriptions
   still exceeds the limit. The result is then **saved to a file**: page with `nextPageToken`
   and process the files with Python. That is how the 2026-09 export and the post-creation check
   were done — mechanically, never by retyping tool output.

3. **Deleting issues or sprints is done by Jann, by hand.** Both are permanent. Claude prepares
   the exact JQL or list and verifies the result afterwards.

4. **Jira's ADF round-trip is lossy in cosmetic ways** — it escapes `~` as `\~`, turns `-`
   bullets into `*`, and drops some italic markers around inline code. Harmless, no content lost.

---

## Conventions

- **[`roadmap.md`](roadmap.md)** — the themes in dependency order, each with its gate and status.
  Sprints are planned from it.
- **[`jira-rulebook.md`](jira-rulebook.md)** — how a Jira issue is written: issue types, story
  anatomy, acceptance criteria, subtasks, points, Definition of Done and of Ready. Supersedes
  `story-style.md`, removed 2026-09-23.
- **[`sprint-guideline.md`](sprint-guideline.md)** — how a sprint is planned, sized, created and
  closed.
- **[`story-rework-prompt.md`](story-rework-prompt.md)** — **obsolete.** It targeted the
  pre-2026-09 backlog, which was deleted and is archived in
  [`archive/backlog-2026-09.json`](archive/backlog-2026-09.json).

## Derived documents

[`implementation-plan.md`](implementation-plan.md) — **obsolete.** It was derived from the backlog
deleted on 2026-09-26 and had drifted from it long before. Kept only as history; plan from the
[roadmap](roadmap.md) instead.

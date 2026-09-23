# Jira Rulebook — ARIA

Supersedes `docs/backlog/story-style.md`. Companion: `docs/backlog/sprint-guideline.md`.

Jira project ARIA, board 170, cloudId `d5d40395-9142-4047-bb1c-387c991640b9`.

A story is a task for one developer to implement. It is not documentation, not a design
record, and not a place to explain the system. Everything explanatory lives in `docs/`.

## Issue types

| Type | Use |
|---|---|
| Story | A unit of implementation work that ends in something merged and deployed. |
| Spike | A question that must be answered by measurement or experiment, not by thinking. Produces a written finding, never production code. |
| Subtask | A commit-sized step inside a story. |

No Epics. The roadmap in `docs/backlog/roadmap.md` holds the hierarchy; duplicating it in Jira
guarantees the two drift. Each story carries exactly one label naming its roadmap theme
(`theme-foundations`, `theme-identity`, `theme-data`, `theme-conversation`, `theme-tools`,
`theme-clients`, `theme-notify`, `theme-observability`, `theme-selfext`, `theme-ops`).

## Story anatomy

Summary: imperative, names the artifact, under 80 characters.
Description: **under 120 words**, in this order.

```
Goal
One or two sentences. What exists after this story that did not before.

Not this story:
- explicitly excluded scope
- the obvious adjacent thing someone would drift into

Acceptance criteria:
- testable statement
- testable statement

Open items:
- only unresolved questions that affect this story, with what to do meanwhile

Links: <GitHub file URLs>
```

Rules:

- **No references to closed decisions.** A closed decision is already clear; repeating it
  inflates the story. Traceability lives in `docs/decisions/` and the roadmap, not in Jira.
- **Open decisions are referenced** — by name or question, plus the interim instruction, so the
  story is implementable today rather than blocked on a discussion.
- No rationale, no rejected alternatives, no architecture description, no quoted docs.

## Acceptance criteria

Two to four per story. Each one must be checkable by running something. "Works correctly" and
"is well tested" are not criteria; "`grpcurl` against the deployed Gateway returns a streamed
response for a text turn" is.

If a story needs more than four criteria, it is two stories.

## Subtasks

Two to six per story, each commit-sized and phrased as an imperative. Subtasks carry no
acceptance criteria and no points — the story owns both. More than six subtasks means the story
is too large; split it.

## Story points

Points measure uncertainty, not hours. Scale: 1, 2, 3, 5, 8.

| Points | Meaning | Reference |
|---|---|---|
| 1 | Mechanical, known path, no design left. | Add a new proto message and regenerate. |
| 2 | Small, one component, nothing to decide. | Add a health endpoint plus its Helm probe. |
| 3 | One component, some shape to work out. | Persist the turn log through the Knowledge Core. |
| 5 | Crosses two components or has real unknowns. | Gateway mints the signed context and the Agent Core verifies it. |
| 8 | Too big. Split before committing to it. | — |

Spikes carry a **timebox** (hours) instead of points, and name their output document.

## Definition of Done

A story is done when all of the following are true:

- Merged to `main`, CI green.
- Tests exist for the behaviour the acceptance criteria describe.
- Schema changes ship as a migration in the owning service's migration set.
- The Helm chart is updated in the same PR.
- The code emits OTel spans on its own paths — retrofitting tracing later is not acceptable.
- Deployed to the cluster and demonstrated against the acceptance criteria.
- If the work changed or invalidated a decision, `docs/` is updated in the same PR.

## Definition of Ready

A story may enter a sprint only if it has: a goal, 2–4 acceptance criteria, subtasks, a point
value, and a theme label — and no unresolved open item that makes it unimplementable.

## Links

Only the **Blocks** link type, and only where one story genuinely cannot start before another.
Two blocking links per story is already a lot. The previous backlog accumulated 227 edges
because linking felt free; it is not, because MCP tooling cannot delete links.

## Never in a story

- Decision text or its reasoning
- Architecture explanation
- Anything copied from `docs/`
- A checklist standing in for acceptance criteria
- "Investigate…" as a story (that is a Spike)

## Tooling notes

- `searchJiraIssuesUsingJql` overflows and silently truncates, and `fields` does not suppress
  `description`. Fetch issues singly, or compute in the browser.
- The Atlassian MCP cannot create sprints or delete issue links — use Claude in Chrome with the
  Agile API.
- Never close a story without reading its acceptance criteria.

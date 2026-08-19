# Prompt — rework every Jira story description

*Paste this as the opening message of a fresh Cowork session with the `Projects/ARIA` folder
connected and the Atlassian MCP available.*

---

I want to rewrite the descriptions of every story in the ARIA Jira project. They are far too long
and written for an LLM that needs maximum context, not for a person deciding what to build. The
information in them is correct — **this is a compression job, not a redraft.**

## Read these first

- `docs/backlog/story-style.md` — **the style guide. This is the specification.** Read it fully
  before touching anything.
- `CLAUDE.md` — orientation and the project's rules. Rule 1 (never invent a decision) applies here
  with full force.
- `docs/backlog/README.md` — Jira coordinates, field IDs, and tooling limits learned the hard way.

## The single most important instruction

**Preserve substance exactly. Change only presentation.**

- Never drop an acceptance criterion. Compress the wording; keep the condition.
- Never invent one. If something obviously should be tested and is not, list it in your report —
  do not add it silently.
- Never resolve an open item. If a story says something is undecided, it stays undecided.
- Preserve every D-number citation.
- Preserve "out of scope" content as the "**Not this story:**" line.

A rewrite that reads beautifully and quietly drops a criterion has done real damage, because the
criterion is gone and nobody will notice. **When in doubt, keep it and flag it.**

## Method

**Phase 1 — one epic, reviewed.**
Start with **ARIA-2 (Keycloak & aria-identity)** — 9 stories, and several were touched recently so
the before/after contrast is easy to judge. Rewrite all of them, write the results to a file, and
**show Jann before writing anything to Jira.** Do not proceed to phase 2 until he approves.

**Phase 2 — the remaining nine epics, in parallel.**
One subagent per epic. Give each subagent:

- the full text of `docs/backlog/story-style.md`
- the approved ARIA-2 rewrites as a worked reference
- its epic's story keys
- the preserve-substance rules above, stated explicitly

Epics: ARIA-1 Platform Foundation · ARIA-3 Knowledge Core · ARIA-4 Gateway · ARIA-5 Speech ·
ARIA-6 Agent Core · ARIA-7 MCP Registry · ARIA-8 Deployment & Infra · ARIA-9 Observability ·
ARIA-10 Self-Extension.

Have each subagent **write its rewrites to a file and report a summary** — word count before and
after, criteria preserved, anything it wanted to change but didn't. Then apply to Jira in batches,
per epic, so a bad batch is recoverable.

## Tooling limits — do not rediscover these

1. **`searchJiraIssuesUsingJql` overflows and silently truncates** on this project. Watch for a
   non-zero `remainingCount` alongside `hasNextPage: false`. The `fields` parameter does **not**
   suppress `description`, which is what blows the budget — descriptions run to several KB each.
   Fetch stories individually, or query in-browser with `fetch('/rest/api/3/search/jql', …)` and
   compute in JS so only a summary returns.
2. **Description rewrites cost roughly 10 KB of context each** once the echo is counted. Doing 111
   in one context will fail. This is why the work is split across subagents.
3. **Jira's ADF round-trip is cosmetically lossy** — it escapes `~` as `\~` and drops some italics
   around inline code. Harmless. Do not chase it.
4. Use `editJiraIssue` with `contentFormat: "markdown"`.

## Links

Per **D54** the docs library lives in the `aria` monorepo on GitHub, so link to GitHub file URLs:

```
https://github.com/FPGSchiba/aria/blob/main/docs/decisions/0003-knowledge-model.md
```

Link to files, not heading anchors — anchors break when a document is reorganised.

**If the docs move has not happened yet, stop and say so** — rewriting every description with dead
links, then rewriting them again, is exactly the double work this sequencing exists to avoid.

## Also do

- **Epics too** — ten of them, two or three sentences each, no acceptance criteria.
- **Fix the stale lines noted in `archive/ARIA-session-2-prompt.md`** ("Also worth cleaning up") if
  they are still present — several stories carry text that a later decision contradicted. Removing
  contradictions is in scope; resolving open questions is not.

## Report at the end

- Stories rewritten, with before/after word counts
- Any criterion you merged, and why
- Anything you believed should change but left alone
- Any story whose description contradicted a decision, listed for a human to resolve

## Rules

- **Never invent an architecture decision.** If something is undecided it stays open.
- Ask before any large batch write.
- If a story's rewrite would lose information that has no other home, **stop and raise it** rather
  than deciding where it should live.

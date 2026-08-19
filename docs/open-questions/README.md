# ARIA — Open Questions

[Library index](../README.md) · [Decisions](../decisions/README.md) · [Open questions](README.md) · [Sprint 1](../sprints/sprint-01.md)

Everything not yet decided, **sorted by what kind of answer it needs** rather than by topic.
That split is the whole point of this directory and it is load-bearing — it is what stopped
session 1 from inventing a GPU memory budget or a model quality threshold out of a conversation.

---

## The three buckets

| File | Bucket | Meaning | How it gets closed |
|---|---|---|---|
| [needs-decision.md](needs-decision.md) | **A** | Answerable by preference, judgement, or homelab knowledge | A decision session with Jann → new entry in `decisions/` |
| [awaiting-measurement.md](awaiting-measurement.md) | **B** | Needs a number, a benchmark, or a look at real hardware | A spike story runs and produces evidence |
| [deferred.md](deferred.md) | **C** | Genuinely fine to leave open for now | Revisited when its stated precondition arrives |

## The rule that matters

**Bucket B items must not be answered in conversation.** Not by Jann, not by Claude. A confident
answer to "how much VRAM is left next to Ollama" or "which local model calls tools reliably" that
came out of a chat rather than a measurement is exactly the manufactured certainty this project
exists to avoid — and it is *more* dangerous than an open item, because downstream stories will
build on it.

If an item looks like A but a tail of it secretly needs a measurement, split it: decide the policy
now, leave the measured tail with its spike. Session 1 found six of these. Session 1 also found a
genuine gap this way — "which Ollama model" was sitting inside an implementation story with
nothing measuring it, and became ARIA-109.

## Moving an item out

1. Record the decision in [`decisions/`](../decisions/README.md) with its rejected alternatives.
2. Update the affected page (`02-stack.md`, `03-architecture.md`, `04-deployment.md`, or a
   `services/` page) so it describes the new reality.
3. Delete the item from its bucket file here.
4. Update the Jira story: replace its "Open / needs decision" block, and note if scope changed.
5. Mirror to Confluence.

An item that lands on only some of those surfaces is worse than one recorded nowhere, because
the incomplete version looks authoritative.

---

## Current state

Counts as of **2026-08-19**, carried forward from CLAUDE.md §8:

| Bucket | Count |
|---|---|
| A — needs a decision | 19 carried + 10 newly surfaced by the session-1 decisions |
| B — awaiting a measurement | 10 |
| C — deferred | 7 |

A follow-up decision session was scoped but has not yet run — its agenda (~17 deferred items plus
~12 newly surfaced) is preserved in
[`../../archive/ARIA-session-2-prompt.md`](../../archive/ARIA-session-2-prompt.md).

## Historical note

An earlier, larger triage of all 78 originally-open items (with per-item "first needed" sprint
mapping) lives in [`../../archive/ARIA-decision-triage.md`](../../archive/ARIA-decision-triage.md).
It predates the 46 session-1 decisions, so **most of its bucket-A rows are now closed** — read it
for the bucket definitions and the A-that-is-really-B analysis, not for current status.

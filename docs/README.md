# ARIA — Information Library

The documentation set for the ARIA project. `CLAUDE.md` at the repository root is the entry
point and router; **this directory holds the substance**.

**Authoritative location: the `aria` monorepo on GitHub, at `docs/` (D54).** Jira stories link to
GitHub file URLs; Confluence is a read-only mirror. A copy read anywhere else — including Google
Drive — should be checked against the repo before it is trusted.

---

## Where things live

| Directory | Holds | Changes when |
|---|---|---|
| `01-vision.md` | What ARIA is, who it serves, non-goals | The ambition changes — rarely |
| `02-stack.md` | Settled technology choices + rationale | A decision changes the stack |
| `03-architecture.md` | Service boundaries and ownership | A service gains or loses responsibility |
| `04-deployment.md` | Hosting, packaging, backups, network boundary | Infra decisions land |
| `05-conventions.md` | Repo layout, crates, code conventions | Code lands and teaches us something |
| `decisions/` | **The reasoning trail** — every decision with its rejected alternatives | Append-only. Never edit a past entry |
| `services/` | Per-service internals: modules, frameworks, contracts | That service's design firms up |
| `open-questions/` | Everything *not* decided, sorted by what kind of answer it needs | Continuously |
| `spikes/` | Research briefs answering a specific spike story | A spike is worked |
| `backlog/` | Jira coordinates, tooling limits, **story style guide**, the derived plan | Jira changes |
| `sprints/` | Per-sprint dossiers: what closes it, what blocks it | Each sprint |

---

## The rules that keep this trustworthy

These are not style preferences. Each one exists because its absence has already cost this
project time.

**1. Never invent a decision.** If something is undecided, it stays in `open-questions/`
marked open. A plausible-sounding answer written down as settled is worse than a gap, because
the gap gets filled and the fake answer gets built on.

**2. `decisions/` is append-only.** A decision that turns out wrong gets a *new* entry that
supersedes it, with the trigger that fired. Editing history destroys the reasoning trail, which
is the only thing that lets a future session know whether an alternative was considered and
rejected or simply never thought of.

**3. Consequences get followed through.** When a choice creates new work, new risk, or a new
open question, that consequence is recorded in the same edit — not left for someone to discover.
Several session-1 decisions did exactly this (D7 forcing `aria-kc-broker` into existence, D16
making Qdrant non-rebuildable), and those follow-throughs are the most valuable lines in the log.

**4. Three-way triage is load-bearing.** `open-questions/` splits by *what kind of answer an
item needs*, not by topic. An item awaiting a measurement must never be answered in conversation.
See [open-questions/README.md](open-questions/README.md).

**5. One fact, one home.** If a fact appears in two files, one of them is a pointer. Drift
between copies is how `ARIA-implementation-plan.md` became a trap.

**6. Derived documents are labelled as derived.** Anything rebuilt from Jira carries the date it
was generated and a warning that live Jira wins.

---

## Where does a new fact go?

```
Is it a choice between real alternatives?
├─ YES → decisions/  (new D-number, with what lost and why)
│         then update the affected page in 02-stack / 03-architecture / 04-deployment
└─ NO
   ├─ Is it unresolved?
   │   ├─ Needs a preference or homelab knowledge → open-questions/needs-decision.md
   │   ├─ Needs a number or a benchmark          → open-questions/awaiting-measurement.md
   │   └─ Genuinely fine to leave                → open-questions/deferred.md
   ├─ Is it the output of a spike?               → spikes/ARIA-NN-<topic>.md
   ├─ Is it internal to one service?             → services/<service>.md
   ├─ Is it about the backlog itself?            → backlog/
   └─ Is it how we write stories?                → backlog/story-style.md
```

---

## Reading order for a new session

1. `CLAUDE.md` — orientation and rules (5 min)
2. [01-vision.md](01-vision.md) — why any of this exists
3. [02-stack.md](02-stack.md) + [03-architecture.md](03-architecture.md) — current state
4. [decisions/README.md](decisions/README.md) — skim the index, read the entries touching your area
5. [open-questions/README.md](open-questions/README.md) — what you may and may not decide
6. [sprints/sprint-01.md](sprints/sprint-01.md) — what is actually being worked right now

Do **not** start from `backlog/implementation-plan.md`. It is derived and has drifted before.

---

*Last restructured: 2026-08-19.*

# Open — deferred, recorded

[Library index](../README.md) · [Decisions](../decisions/README.md) · [Open questions](README.md) · [Sprint 1](../sprints/sprint-01.md)

Bucket C. Genuinely fine to leave open — but recorded, so leaving them open stays a choice rather
than an oversight. Each names the precondition that should bring it back.

**Source:** the checkpoint-1 triage (`archive/ARIA-decision-triage.md`), re-verified against the
session-1 decisions on 2026-08-19.

---

| # | Item | Why it can wait | Revisit when |
|---|---|---|---|
| C-1 | Who polices the "one packaging convention, don't mix" rule once MCP servers live in their own repos outside the monorepo | Process question; no repo exists outside the monorepo yet | The second out-of-monorepo MCP server is created |
| C-2 | Adopting the proposed `services/knowledge-core/` internal layout | Real but zero-cost to change | Lands naturally with ARIA-55 |
| C-3 | How retrieved context is formatted into the prompt | ARIA-58 already pins the invariants (bounded, deterministic) | ARIA-58 is implemented |
| C-4 | The numeric tool-loop iteration bound | ARIA-61 requires *a* bound as acceptance criteria; the value is tuning | First real tool loop runs |
| C-5 | Alerting — entirely undecided, and no story delivers any | Nothing to alert on until services actually run | Services are deployed and running (post-ARIA-91) |
| C-6 | Delegation UX for `consent_grants.granted_by` — a circle member consenting on behalf of another | The schema hook already exists; the flow is only needed when a second person is onboarded | A second circle member is onboarded (ARIA-114) |
| C-7 | External/remote access design (e.g. Tailscale or Cloudflare Tunnel in front of the Gateway) | Explicitly deferred in `04-deployment.md` already | The core works locally end-to-end |

---

## Deferred by nature, not by triage

**ARIA-57's column list.** ARIA-57 is a design story whose *output* is the schema. The shape
principle behind it was decided (D19: typed `person` + `relationship`, not a generic property
graph) — the columns are that story's job, not an open question to be answered elsewhere.

---

## See also

- [needs-decision.md](needs-decision.md) — bucket A
- [awaiting-measurement.md](awaiting-measurement.md) — bucket B

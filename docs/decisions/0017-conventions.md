# ARIA — Decision Log · Working conventions

**Decision D84** · taken 2026-09-23 (design conversation)

One entry per decision: what was decided, why, what was rejected and why, and what it affects.

---

### D84 · C1 — Definition of done extends to migration, chart and spans

**Decided.** A service is **not done** without three things beyond its code and tests:

1. **Its migration** — the `sqlx` migrations for whatever schema it owns
   ([D73](0015-external-datastores.md), [D77](0015-external-datastores.md)), checked in and
   applied by the same process everything else uses.
2. **Its Helm chart** — its slice of the umbrella chart (D33), including the values it exposes,
   rather than a chart written later by whoever first needs to deploy it.
3. **Its OpenTelemetry spans** — instrumentation per [D82](0016-observability-v2.md) and the
   propagation points in [D83](0016-observability-v2.md), not a `TODO: add tracing`.

This sits alongside the existing repo gate and the 80% coverage floor in
[§7](../05-conventions.md), which are unchanged.

**Why.** Each of the three is a thing that is cheap while the service is being written and
expensive afterwards, and each has a specific failure mode when deferred.

- **A migration written later** is written by someone reconstructing what the schema was supposed
  to be from the code that uses it. With schema-per-service and a `*_owner`/`*_app` role split
  (D73), the migration is also where the ownership boundary is *expressed* — deferring it means the
  boundary exists only as an intention.
- **A chart written later** is written under deployment pressure, which is when the shortcuts get
  taken. The umbrella chart (D33) makes the whole namespace one atomic release; a service that
  joins it late is a service whose first rollback was never tested.
- **Spans added later never get added.** D83's whole argument is that three of four propagation
  points are manual; a service that ships without them is a permanent hole in exactly the trace
  that made Jaeger worth deploying (D82). And unlike the other two, nothing breaks without it —
  the system runs fine and simply cannot be debugged, so nothing forces the work.

**Accepted cost.**

- **Every service gets slower to finish**, and the first one pays the most: it writes the chart
  conventions, the migration conventions and the instrumentation boilerplate that later services
  copy. That front-loading is deliberate but real.
- **A spike or throwaway is now awkward** — this rule is about *services*, not about every branch,
  and treating it as universal would make exploratory work expensive. The line between "a service"
  and "an experiment" is not written down here and will be argued at some point.
- **It cannot be machine-enforced today.** The repo gate checks formatting, lints, tests and
  coverage; none of these three are checkable by `cargo`. This is a convention held by review, and
  the coverage rule's honesty about being enforced by a local hook applies here too — with less
  enforcement, not more.

**Rejected.**

- *Leave the definition of done as it is — code, tests, 80% coverage* — the lightest rule, and the
  one already being followed. Rejected because it is what produces a working system nobody can
  deploy or debug, and because all three additions were already implied by decisions taken and not
  written down as obligations on anyone.
- *Add them to the repo gate as hard checks* — enforcement rather than convention, consistent with
  how coverage is handled. Rejected as premature: a check for "has a chart" or "has spans" is
  either trivially gamed (a file exists) or a real static analysis nobody is going to write now.
  **Worth revisiting once there is more than one service to check** — a linter for "every service
  directory has a `migrations/` and a chart fragment" is cheap and catches the common case.
- *Make it per-service, decided at planning time* — flexible, and lets a service that genuinely
  owns no schema skip the migration. Rejected because it makes the default "no" and turns each
  instance into a negotiation; a service that owns no schema simply has no migration to write, and
  that is not a negotiation either.

**Affects.** ARIA-18 (which owns the next expansion of §7), every service story's acceptance
criteria, and the generated-server template the Self-Extension server produces — which is where
this rule has to be encoded for servers nobody has written yet.

---

## Related

- [Working conventions](../05-conventions.md) — §7, which this extends
- [Repo, proto & CI](0002-repo-proto-ci.md) — the existing gate
- [Observability, second pass](0016-observability-v2.md) — D82, D83
- [External datastores](0015-external-datastores.md) — D73, D77, the schema ownership this makes concrete

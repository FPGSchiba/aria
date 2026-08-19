# ARIA — Decision Log · Knowledge model & data ownership

**Decisions D16–D23** · taken 2026-08-12 (session 1)

One entry per decision: what was decided, why, what was rejected and why, and which stories it
affects. Part of [the decision index](README.md). Several entries carry an explicit **revisit
trigger** — those are not settled forever, but must not be reopened until the trigger fires.

---

### D16 · C3 — Preferences are split by kind: typed in Postgres, free-text in Qdrant

**Contradiction resolved by partition, not by precedence.** Structured, typed preferences
(`timezone=Europe/Zurich`, units, quiet hours) are authoritative in Postgres. Free-text preferences
("I like my coffee strong") are authoritative in Qdrant. No preference lives in both as a peer, so
there is no dual-write and no "which wins" question — but there are two systems of record,
partitioned by the kind of thing being stored.

**Why (Jann's call).** The two kinds genuinely are different data. A typed attribute wants exact
lookup and constraints; a fuzzy preference only ever gets used through semantic retrieval, and
round-tripping it through a Postgres row adds nothing.

**Rejected.**

- *Postgres authoritative, Qdrant purely derived* (my recommendation) — one system of record and a
  fully rebuildable index, at the cost of storing free-text preferences somewhere they're never
  read from directly.
- *Genuine dual-write* — the state the document was accidentally in; no consistency story at all.

**Three consequences, recorded because they are not free:**

1. **Qdrant now holds primary data.** It is no longer a rebuildable index, so backup and restore
   (D-batch-5 / ARIA-96) must treat it as a source of truth, not as something re-derivable from
   Postgres. This materially changes that story.
2. **ARIA-43's scope shifts.** The spike no longer just decides "what gets embedded" — it must
   produce the **classification rule** that says which kind a given preference is, and what happens
   when one changes kind.
3. **"What do we know about X" spans two stores.** Any read that assembles a full picture of a
   person queries both. Newly open: is there a reconciliation or merge order when a typed and a
   free-text preference conflict?

**Affects.** ARIA-43 (scope changed), ARIA-52, ARIA-62, ARIA-76, ARIA-96 (scope changed).

---

### D17 · K3 — `person.id` is an ARIA UUID; `keycloak_sub` is a nullable unique column

**Decided.** The `person` table has its own UUID primary key and a nullable, unique `keycloak_sub`
column linking it to a realm account when one exists.

**Why.** Follows directly from D4 and D2. The graph must hold people who will never authenticate,
and linking an account later becomes a single-column update rather than a key migration. `sub`
remains the wire identity (D2); `person.id` is the graph identity.

**Rejected.**

- *Keying `person` on `keycloak_sub`* — one identifier throughout and no join, but family members
  who never log in simply couldn't be represented.
- *Separate `person` and `account` tables, one-to-many* — correct for real account complexity, but
  ARIA has one realm and a handful of people.

**Affects.** ARIA-57, ARIA-59, ARIA-62, ARIA-64, ARIA-70.

---

### D18 · K4 — Per-fact ownership: every row carries the user it was learned from

**Decided.** Every `person`, `relationship`, preference and memory row carries an `owner_user_id`.
Reads filter on it. Traversal works inside your own graph — you can ask about your mother because
you own that edge — but facts learned from another circle member are not visible to you.

**Why.** Gives "relationship-aware" a concrete meaning that does not require inventing a sharing
model today: ARIA traverses relationships to act on your behalf; it does not disclose other people's
contributed facts to you. Cross-user sharing becomes a later, explicit feature that pairs naturally
with the deferred `granted_by` delegation hook.

**Rejected.**

- *Shared circle graph with private preferences* — a truer model of a real circle and avoids storing
  your mother once per person who knows her, but it lets any circle member enumerate everyone,
  which nobody consented to.
- *Circle-visible by default* — matches the intimacy of a close circle, but it's a poor default and
  very hard to walk back once data exists under it.

**Known cost.** The same real-world person may exist as several `person` rows with different
owners. Deduplication across owners is deliberately **not** solved here and is a new open question.

**Affects.** ARIA-57, ARIA-59, ARIA-62, ARIA-76, ARIA-108.

---

### D19 · K2 — Typed `person` + `relationship` tables

**Decided.** A `person` table and a `relationship(from, to, type, attrs JSONB)` edge table with an
enumerated relation type. Columns remain ARIA-57's output; this fixes the modelling approach it
builds against.

**Why.** Closest to the legacy `relation`/`relation_type` schema being replaced, and §6 already
commits to not reinventing a graph database. Postgres indexes it properly, recursive CTEs cover the
traversal depth a personal circle needs, and JSONB absorbs the long tail of edge attributes.

**Rejected.**

- *Generic property graph (nodes/edges/properties)* — reproduces the generality of the legacy
  sharded whiteboard store, but every query becomes a join through untyped tables Postgres cannot
  index usefully. This is the design §6 walked away from.
- *Apache AGE or similar* — real graph queries, but another extension to run and version, for a
  graph small enough that CTEs won't be the bottleneck.

**Affects.** ARIA-57, ARIA-59.

---

### D20 · K5 — The calendar is a mirror with a read-through cache; writes go via a calendar MCP server

**Decided.** An external calendar (Google/CalDAV) is the source of truth. The Knowledge Core caches
**materialised occurrences** only. ARIA never implements recurrence expansion, timezone arithmetic,
recurring-series exceptions or attendee state. Writes go through a calendar MCP server.

**Why.** All of that is solved upstream and none of it is assistant behaviour — it is the classic
place personal-assistant projects sink. Routing writes through an MCP server is also exactly the
extensibility model the vision describes, rather than a special case in the core.

**Rejected.**

- *ARIA authoritative* — no external dependency and it could hold events nothing else knows about,
  at the cost of implementing RFC 5545 properly.
- *Both, with two-way sync* — covers ARIA-native events but needs conflict resolution, which is
  harder than either pure option.

**Direct effect on an existing story.** ARIA-64 narrows sharply: a cache of materialised occurrences
plus accessors, not a calendar schema. Newly revealed: a calendar MCP server is now an assumed
component that no story creates, and cache invalidation/refresh cadence is open.

**Affects.** ARIA-64 (scope reduced), ARIA-57.

---

### D21 · Q4 — MCP service writes are namespaced by source, with areas declared at registration (closes ARIA-47)

**Decided.** Two rules together. Every Knowledge Core row carries a `source` identifying the server
that wrote it, and a server may only modify rows it authored. Separately, the KC *areas* a server
may touch (`memory:write`, `people:read`, …) are declared at registration and fixed by the approval
gate.

**Why.** Delivers the vision's "extendable by every connected MCP service" without letting a Sonos
controller rewrite the social graph. Per-row provenance also means a wrong fact can be attributed
to whoever wrote it — which matters most for self-generated servers.

**Rejected.**

- *No direct writes; only the Agent Core writes* — simplest and safest, one writer and one place to
  reason about provenance, but it contradicts the vision and makes the Agent Core a bottleneck for
  everything learned.
- *Capability scopes without source ownership* — less schema overhead, but two servers can silently
  overwrite each other and nothing is attributable.

**Affects.** ARIA-47 (closed by this), ARIA-55, ARIA-62, ARIA-68, ARIA-76, ARIA-108.

---

### D22 · K6 — The Knowledge Core validates writes itself

**Decided.** The KC checks the calling service's identity from its own client-credentials token and
enforces the D21 source-ownership and area rules itself, independently of the MCP Registry's gate.

**Why.** Consistent with D10's reasoning — the KC exists as a service precisely so there is one
enforcement point — and it means a bug or bypass in the Registry becomes a rejected call rather than
data corruption. Without it, any service with network access to the KC bypasses every consent and
permission check, leaving NetworkPolicy (D6) as the only barrier.

**Rejected.** *Trusting the Registry gate* — one implementation instead of two, and §2's "defense in
depth" wording is arguably narrower than this. Not worth the bypass surface.

**Accepted cost.** The permission rule is implemented in two places and must not drift.

**Affects.** ARIA-47, ARIA-55, ARIA-59, ARIA-62, ARIA-68, ARIA-76.

---

### D23 · K1 — `sqlx`

**Decided.** `sqlx` with compile-time-checked queries and its built-in migration harness.

**Why.** The two hardest parts of this schema — recursive CTEs over the D19 relationship table and
JSONB attribute columns — are exactly where an ORM adds friction instead of removing it.
Compile-time query verification provides most of the safety an ORM promises, without the
abstraction.

**Rejected.** *`sea-orm`* — entity models and typed relations make ordinary CRUD much shorter and
read better for anyone joining later, but graph traversal and JSONB both drop back to raw SQL
anyway, so the abstraction would be carried without benefit where it counts.

**Affects.** ARIA-55 (must fix this before any schema story), and every Knowledge Core story
downstream of it.

---

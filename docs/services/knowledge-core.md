# `aria-knowledge-core` — Knowledge Core

[Library index](../README.md) · [Architecture](../03-architecture.md) · [Decisions](../decisions/README.md) · [Open questions](../open-questions/README.md)

**Deployed service · `services/knowledge-core/`**

---

## Amendments since 2026-08-12

- **[D48](../decisions/0009-measurement-session.md)** — the typed/free-text classification rule.
  Typed only if: closed value domain **and** read by exact lookup **and** wrong = bug. Free-text is
  the default. A kind change is a migration in a fixed order (write the new store, then delete the
  old), never a dual-write. On conflict **typed wins**, and the stale free-text point is flagged
  rather than merged. **Qdrant-primary list, for ARIA-96:** free-text preferences and learned
  facts. Still open: whether conversation history gets a *derived* (rebuildable) Qdrant index.
- **[D55](../decisions/0010-retention-audit.md)** — the unrecognized-request record holds the **raw
  utterance** plus `user_id`, timestamp and serving backend, bounded by a 90-day window (D56) rather
  than by redaction, with ARIA-108's query accessor as the documented review path. ARIA-63's payload
  criterion stands unchanged.
- **[D56](../decisions/0010-retention-audit.md)** — **one retention schedule, one row per data
  class.** In this service: **conversation history — indefinite** (a deliberate choice, with a
  permanent verbatim record of the household named as the accepted cost);
  **`consent_audit_log` — indefinite** (it is the only place a conversationally-granted permission's
  origin is reconstructable, per D42); **unrecognized-request log — 90 days**. Exactly **one** store
  is pruned, by a single CronJob in the umbrella chart reading one config surface. Because history
  is not pruned, the pruner never touches Qdrant — the derived-index question above is **untouched**
  and stays open.
- **[D57](../decisions/0010-retention-audit.md)** — `consent_audit_log` takes **two rows per
  invocation**: an attempt row written before forwarding and an outcome row after, correlated by an
  attempt key. The schema gains that key and an entry-kind discriminator. Audit volume roughly
  doubles against an indefinite window, so ARIA-73's write-volume criterion is now load-bearing —
  and the volume figure is a **measured tail**, not a number this decision invented.
- **[D58](../decisions/0010-retention-audit.md)** — the audit row is **self-contained**; the trace
  ID stays as best-effort correlation expected to dangle. The column list is therefore load-bearing
  for a second reason: a future schema change must not quietly drop one of the fields the row's
  independence rests on.

---

## Purpose

The real centre of ARIA — a RAG system holding people, relationships, preferences and what ARIA has
learned. **A deployed service, not a library** (D10 — the old §4 was wrong). Owns both Postgres and
Qdrant; nothing else talks to them directly.

## Replaces

`DB-ARIA`, `DB-ARIA-User`, `DB-Connect`, and the hand-built sharded whiteboard graph datastore.

## Owns

**Postgres** — typed social graph (`person`, `relationship`), typed preferences, calendar cache,
`mcp_tools`, `consent_grants`, `consent_audit_log`, conversation history, MCP server registry.

**Qdrant** — semantic memory and free-text preferences.

Key properties:
- Preferences are **partitioned by kind, not duplicated** (D16): typed → Postgres, free-text →
  Qdrant. **Consequence: Qdrant holds primary data and is NOT rebuildable from Postgres** — it must
  be backed up as a source of truth.
- `person.id` is an ARIA UUID with a **nullable, unique `keycloak_sub`** (D17) — which is what lets
  the graph hold family members who will never authenticate (D4).
- Every row carries `owner_user_id` and reads filter on it (D18). You can traverse to your mother
  because you own that edge; facts learned *from* her are not visible to you.
- **Validates writes itself** rather than trusting the Registry's gate (D22): every row carries a
  `source`, a server may only modify rows it authored, and the areas it may touch are declared at
  registration and fixed by the approval gate (D21).
- **The calendar is a mirror, never authoritative** (D20). ARIA never implements recurrence
  expansion, timezone arithmetic or attendee state. Writes go through a calendar MCP server.

## Binding decisions

D10 (deployed service) · D16 (split by kind) · D17 (UUID PK, nullable `sub`) · D18 (per-fact ownership) · D19 (typed tables) · D20 (calendar is a mirror) · D21 (source-namespaced writes) · D22 (validates writes) · D23 (`sqlx`)

See [the Decision Log](../decisions/README.md) for the full reasoning and rejected alternatives.

## Contract

`grpc/people.rs`, `grpc/memory.rs`, `grpc/registry.rs` — `proto/knowledge-core/v1/knowledge_core.proto`. Data access via **`sqlx`** with compile-time-checked queries and its migration harness.

## Open items

- **Awaiting measurement:** embedding model/provider (ARIA-52); which facts get embedded
  (ARIA-43 — see [the brief](../spikes/ARIA-43-knowledge-classification.md)); Postgres operator and
  Qdrant topology (ARIA-84)
- The **classification rule** separating typed from free-text preferences, and what happens when a
  preference changes kind (ARIA-43)
- Reconciliation order when a typed and a free-text preference conflict
- **Cross-owner person deduplication** — per-row ownership means the same real person may exist as
  several `person` rows under different owners (ARIA-59)
- Calendar cache invalidation and refresh cadence; who builds the calendar MCP server (ARIA-120)

## Jira stories

ARIA-43, 52, 55, 57, 59, 62, 64, 68, 70, 73, 76, 108, 115

---

*Derived from `03-architecture.md` and the Decision Log. Last updated 2026-08-19.*

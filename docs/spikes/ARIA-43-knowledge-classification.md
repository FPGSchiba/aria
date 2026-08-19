# ARIA-43 — The typed / free-text classification rule (+ ARIA-52 embedding model)

[Library index](../README.md) · [Spikes](README.md) · [Knowledge Core](../services/knowledge-core.md) · [Sprint 1](../sprints/sprint-01.md)

**Status: proposal ready for decision on the rule; the embedding model stays measurement-blocked.**
Desk research 2026-08-19.

---

## What changed, and why this spike is not what its title says

D16 already settled the *split*: typed preferences are authoritative in **Postgres**, free-text
preferences are authoritative in **Qdrant**, and no preference lives in both as a peer. So this
spike is no longer "what gets embedded" — it is:

> **How do you decide which kind a given preference is, and what happens when one changes kind
> after it has already been stored?**

That is a **bucket-A question**. It is a rule, not a measurement — and it is separable from the
embedding model (ARIA-52), which is genuinely bucket B. Keeping them tangled is what has kept this
story open.

### The consequence D16 recorded, which this spike must carry forward

**Qdrant now holds primary data and is not re-derivable from Postgres.** That is not a footnote —
it changes ARIA-96's backup scope and raises ARIA-89's durability bar. This spike must publish the
list of Qdrant-primary categories so backup can act on it.

---

## Proposed classification rule — *a proposal, not a decision*

> ⚠️ Nothing in this section is decided. It is written as a concrete proposal so it can be argued
> with rather than re-derived. Record the outcome as a D-entry.

**A preference is typed (Postgres-authoritative) if and only if all three hold:**

1. **A closed value domain exists.** The set of valid values is enumerable or constrained —
   an IANA timezone, a unit system, a `HH:MM` range, a boolean, an enum.
2. **Something reads it by exact lookup**, not by similarity. Some code path asks *"what is X's
   timezone?"* and needs the answer, not the three nearest answers.
3. **Being wrong is a bug, not a nuance.** Wrong timezone → the alarm fires at the wrong hour.

**Everything else is free-text (Qdrant-authoritative).** Free-text is the *default*, and promoting
something to typed is a deliberate act that adds a column and a constraint.

This ordering matters: it means the failure mode of an unclassified fact is "retrievable but
fuzzy" rather than "silently dropped".

### Worked examples

| Fact | Kind | Why |
|---|---|---|
| `timezone = Europe/Zurich` | **Typed** | Closed domain (IANA), exact lookup, wrong = bug |
| Quiet hours `22:00–07:00` | **Typed** | Closed domain, exact lookup, wrong = wakes someone |
| Units: metric | **Typed** | Enum, exact lookup |
| "I like my coffee strong" | **Free-text** | No domain, only ever read by similarity |
| "Don't play Christmas music before December" | **Free-text** | Expressible as a rule *in principle*, but the domain is unbounded |
| "My mother's name is Anna" | **Neither** — this is the social graph | `person` + `relationship` (D19), not a preference |
| "Prefers being called Jann, not Jann Erhardt" | **Typed** (display name) | Closed domain of one string, exact lookup |

That last row is the useful one: it shows the rule has a real edge, and that the edge is decidable.

### The categories beyond preferences

The story's acceptance criteria demand every known knowledge category be classified. Proposal:

| Category | Store | Qdrant-primary? |
|---|---|---|
| People, relationships | Postgres only (D19) | no |
| Typed preferences | Postgres only (D16) | no |
| Free-text preferences | **Qdrant only** (D16) | **yes** |
| Learned facts from interactions | **Qdrant only** | **yes** |
| Calendar occurrences | Postgres only — a **mirror** (D20) | no |
| Conversation history | Postgres authoritative (D36) | no — *see open item below* |
| MCP registry, `consent_grants`, `consent_audit_log` | Postgres only | no |

**Qdrant-primary list for ARIA-96: free-text preferences + learned facts.** Those two must be
backed up as sources of truth.

**Open, and it is a real question:** should conversation history *also* be embedded for retrieval?
D36 makes Postgres authoritative for it, so an embedded copy would be a genuine derived index —
the one place where a rebuildable Qdrant collection is legitimate. That is a different pattern
from the two primary collections and should be decided explicitly rather than drifting into
existence.

### What happens when a preference changes kind

The story asks, and it is the sharper half. Proposal: **kind changes are a migration, never an
implicit dual-write.**

- Free-text → typed: write the Postgres row, **then** delete the Qdrant point, in that order. A
  crash between the two leaves a duplicate, which is recoverable; the other order loses data.
- Typed → free-text: same shape, reversed. This should be rare and probably wants a human in the
  loop.
- **Never both at once.** The moment a preference exists in both stores as a peer, D16's "no
  consistency story" problem is back.

### Reconciliation order when the two conflict

Listed as newly-open in `04-deployment.md`'s open items. Proposal: **typed wins on any fact the
typed schema can express**, because the typed store only ever contains facts someone deliberately
promoted. A conflicting free-text point is stale by construction and should be flagged for
cleanup, not silently averaged with the typed value.

---

## Qdrant payload contract

The story requires this and it is not blocked on anything. Every vector carries at minimum:

| Field | Why |
|---|---|
| `owner_user_id` | **D18** — every row carries the user it was learned from, and reads filter on it. This is a filter, not a post-hoc check |
| `source` | **D21** — the MCP server that wrote it; a server may only modify rows it authored |
| `kind` | `free_text_preference` \| `learned_fact` \| … — makes the Qdrant-primary list queryable |
| `person_id` | The `person` row this is about, where one exists — traceability back into Postgres |
| `created_at`, `updated_at` | Retention (X10) cannot be implemented without these |

**Retrieval scoping:** every query filters on `owner_user_id` **in Qdrant's filter clause**, not by
discarding results afterwards. §2's rule is "there is no implicit current user" — a post-filter
still ranks another person's facts against the query, which leaks through relevance scores and
result counts even when the payloads are dropped.

---

## ARIA-52 — the embedding model (bucket B, stays blocked)

**Do not decide this from this page.** It depends on the ARIA-27 VRAM measurement, because a local
embedder shares the GPU with Ollama *and* the STT/TTS models. Candidates, for when that number
exists:

| Model | MTEB | Dims | Params | Licence | Notes |
|---|---|---|---|---|---|
| Qwen3-Embedding-8B | 70.58 | 7168 (flex) | 8B | Apache-2.0 | Best quality; 8B on a contended GPU is the problem |
| BGE-M3 | 63.0 | 1024 | 568M | MIT | Strong quality-per-VRAM; quantisable to CPU |
| Nomic embed-text-v1.5 | ~62 | 768 (flex) | 137M | Apache-2.0 | 8K context, runs on CPU at reasonable latency |
| all-MiniLM-L6-v2 | 56.3 | 384 | 22M | Apache-2.0 | <10 ms on CPU; prototype-grade quality |
| Gemini `embedding-001` | 68.32 | 3072 (flex) | — | proprietary | $0.15/1M — **but see below** |
| voyage-3-large | ~67 | 2048 (flex) | — | proprietary | $0.06/1M — **but see below** |

### The constraint that eliminates the hosted options

`04-deployment.md` names exactly three things that cross the network boundary: the hosted LLM API
call, source/CI logs, and encrypted backups. **A hosted embedding API would be a fourth** — and
unlike the LLM call, it would ship *knowledge-base content* out, which is precisely what the
"no personal data leaves in plaintext" line protects.

So this is not really a six-way choice. **It is a choice among the self-hostable rows**, unless
Jann decides to widen the network boundary — which would be a D-entry amending §4, not a quiet
consequence of an embedding-model spike.

**Recommended shape of the answer:** run the CPU-capable candidates (Nomic, BGE-M3) on CPU first.
If quality suffices there, the GPU contention question disappears entirely and ARIA-27's budget is
spent on STT/TTS where it is actually scarce. That reframing is worth testing before assuming an
embedder needs the GPU at all.

### Two things to lock once chosen

- **Vector dimension is baked into the Qdrant collection.** Changing embedder later means
  re-embedding every point — and since these are *primary* data (D16), that is a migration with a
  correctness risk, not a reindex.
- **Record the exact model + revision in the payload**, so a future migration can tell which points
  were embedded with what.

---

## What is needed to close ARIA-43

- [ ] Decide the classification rule (the proposal above is the starting point) — record as a D-entry
- [ ] Decide the kind-change migration order and the conflict-reconciliation rule
- [ ] Decide whether conversation history gets a derived Qdrant index
- [ ] Publish the Qdrant-primary category list so **ARIA-96** can act on it
- [ ] Confirm the payload contract with the Agent Core epic (the consumer of retrieval)
- [ ] Move resolved items out of the open-question buckets

ARIA-52 stays open, blocked on ARIA-27.

---

## Sources

- [Best embedding models for RAG 2026 — MTEB, cost, self-hosting](https://www.premai.io/blog/best-embedding-models-for-rag-2026-ranked-by-mteb-score-cost-and-self-hosting/)
- [Open-source embedding models guide — BentoML](https://www.bentoml.com/blog/a-guide-to-open-source-embedding-models)

*MTEB figures are secondary-sourced. Verify against the live MTEB leaderboard before pinning a model.*

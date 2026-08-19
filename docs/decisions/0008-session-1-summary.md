# ARIA — Decision Log · Session 1 summary — what changed

**Decisions —** · taken 2026-08-12 (session 1)

One entry per decision: what was decided, why, what was rejected and why, and which stories it
affects. Part of [the decision index](README.md). Several entries carry an explicit **revisit
trigger** — those are not settled forever, but must not be reopened until the trigger fires.

---

**46 decisions taken** (D1–D46), covering batches 1–6 in full plus O1, M1, M2, M3, M4.

### Spikes resolved — these stories no longer have a question to answer

| Story | Resolved by |
|---|---|
| ARIA-35 · Keycloak scope naming | D7 |
| ARIA-47 · KC write-permission model | D21 |
| ARIA-44 · Ollama routing rule | D35 |
| ARIA-99 · Credential provisioning | D34 |
| ARIA-21 · Packaging convention | D15 |

### Existing stories whose scope changed

| Story | Change | From |
|---|---|---|
| ARIA-16 | **Narrowed** — "generated types are current" is now vacuous; lint + breaking-change only | D14 |
| ARIA-64 | **Narrowed sharply** — a cache of materialised occurrences, not a calendar schema | D20 |
| ARIA-43 | **Changed** — must now produce the typed-vs-free-text classification rule | D16 |
| ARIA-96 | **Expanded** — Qdrant is primary data; sealed-secrets key and backup encryption key are critical state | D16, D31, D32 |
| ARIA-30 / ARIA-33 | **Unblocked** — English-only removes the multilingual size penalty | D24 |
| ARIA-27 | Budget less constrained; also now owes D6 a node count | D24, D6 |

### Newly revealed work that no story covers

1. Gateway signing-key generation, storage and rotation (D1)
2. Circle-member account onboarding and reconciliation in the shared realm (D4)
3. **A new service** — the Keycloak provisioning broker: crate, deployment, service account (D8)
4. NetworkPolicy authoring (D6)
5. Reconciliation order when a typed and a free-text preference conflict (D16)
6. Person deduplication across owners (D18)
7. **A calendar MCP server** is now an assumed component nothing creates; plus cache refresh cadence (D20)
8. **Acoustic echo cancellation / half-duplex gating** for barge-in — unowned, and plausibly belongs on the client (D27)
9. Ollama configuration as ARIA-managed state that belongs in the chart (D28)
10. Backup encryption key custody — cannot be backed up into what it encrypts (D32)
11. Conversation-history schema and accessors in the Knowledge Core (D36)
12. `consent required` status in the Registry proto; Agent Core consent-prompt path; suspend-and-retry timeout interacting with streaming `Decide` (D42)

### Newly discovered open questions for §8

- **Which Ollama model** — needs its own spike; currently buried in ARIA-54 and gates D35's revisit
- Retention for conversation history (extends X10, which predates D36)
- Cross-owner person deduplication policy
- Backup encryption key custody
- Whether echo cancellation lives on the client or in Speech

### Still open, deliberately — deferred to a follow-up session

Batch 7 minus the pulled-forward items (Q8 risk tiers, M5 tool drift, K7 grant expiry, M6 failure
policies), all of batch 8 (Self-Extension: Q5, E1–E6), and batch 9 minus O1 (X8, O2, O4, O5, O7).
Plus everything in buckets B and C from the triage.

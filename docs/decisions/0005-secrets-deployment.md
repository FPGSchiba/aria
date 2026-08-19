# ARIA — Decision Log · Secrets & deployment posture

**Decisions D31–D34** · taken 2026-08-12 (session 1)

One entry per decision: what was decided, why, what was rejected and why, and which stories it
affects. Part of [the decision index](README.md). Several entries carry an explicit **revisit
trigger** — those are not settled forever, but must not be reopened until the trigger fires.

---

### D31 · X6 — Sealed-secrets

**Decided.** Encrypted secrets live in git; the sealed-secrets controller decrypts them in-cluster.

**Why.** Decisive in combination with D13: sealing requires only the cluster's *public* certificate,
so with GitHub-hosted runners no private key ever leaves the network. It also gives Q7 a clean
mechanism — the approval pipeline seals a new service's credentials into the repo.

**Rejected.**

- *SOPS + age* — no controller to operate and readable diffs, but the age private key must exist
  wherever deploys run, which with hosted runners means storing the key that unlocks everything in
  GitHub Actions secrets.
- *Plain Kubernetes Secrets* — nothing to install, but no record of what exists and no way to rebuild
  the cluster from source.
- *External Secrets Operator + Vault/1Password* — real rotation and audit, and the most elegant
  answer to Q7, but a whole secret-management system to run and back up for a handful of
  credentials.

**Accepted cost.** The controller's private key becomes critical backup state — it must be covered
by D3.

**Affects.** ARIA-81 (carries the decision), ARIA-23, ARIA-53, ARIA-87, ARIA-89, ARIA-96, ARIA-107,
and Q7 below.

---

### D32 · D3 — Local backups replicated encrypted to off-site object storage

**Decided.** Postgres operator backups and Qdrant snapshots go to local storage first, then
replicate to off-site object storage (Backblaze B2, S3 or equivalent) with **client-side**
encryption under a key Jann holds.

**Why.** Two decisions made today raised the stakes: D16 made Qdrant a system of record rather than
a rebuildable index, and D31 made the sealed-secrets private key the thing that unlocks everything
else. Local-only backup means a single site event loses the social graph, every learned preference,
and the ability to restore any of it.

**Posture note.** Data leaving the network as ciphertext under a locally-held key is a narrower
exception than the hosted-LLM call §4 already accepts. §4 should say so explicitly rather than let
"keeps voice and personal data local by default" read as absolute.

**Rejected.**

- *Local only* — strictest reading of §4 and no third-party trust, but no recovery from site loss.
- *Off-site to a second location you control* — no third party at all, but costs hardware at the far
  end and is the setup most likely to quietly stop replicating unnoticed.

**Newly revealed.** The client-side encryption key is itself critical state that cannot be backed up
into the thing it encrypts. Key custody is a new open question.

**Affects.** ARIA-96 (scope expanded — must now cover Qdrant as primary data and the sealed-secrets
key), ARIA-87, ARIA-89.

---

### D33 · D4 — Observability owns the collector's config and pipeline; Infra owns the chart

**Decided.** ARIA-71 stays in Observability and keeps the scrubbing pipeline and export
configuration. The collector's manifest lives in the umbrella Helm chart (D15), which is Infra's.
The boundary is manifest versus config. No duplicate story is created.

**Why.** D15 partly forces it — there is one chart and it belongs to Infra — while the scrubbing
rules are the entire justification for ARIA-71's existence and belong with the epic that owns the
privacy guarantee.

**Rejected.** *Observability end to end* — matches the "one place to enforce" framing but means one
epic writing into another's chart. *Infra end to end* — consistent with how Postgres and Qdrant are
handled, but separates the collector from the rules that justify it.

**Affects.** ARIA-71, ARIA-74, ARIA-91.

---

### D34 · Q7 — The approval pipeline seals a new service's credentials (closes ARIA-99)

**Decided.** On approval, the pipeline seals the candidate service's credentials (e.g. a Sonos API
key) using the sealed-secrets public certificate and commits them; the deployed service reads them
from a mounted Secret. The Self-Extension server never holds or issues credentials.

**Why.** Follows directly from D31, and gives ARIA-107 the property it actually needs: credentials
cannot exist before approval, because the only thing that creates them runs after it.

**Rejected.**

- *Self-Extension injects at runtime* — most automatic, but gives the component that writes and
  deploys model-generated code the ability to hand out credentials, which is precisely the
  escalation E1 exists to bound.
- *Manual provisioning after approval* — nothing automated can leak a credential, but a service that
  deploys and then fails at runtime for want of a key is a confusing failure mode.

**Affects.** ARIA-99 (closed by this), ARIA-107, ARIA-106, ARIA-81.

---

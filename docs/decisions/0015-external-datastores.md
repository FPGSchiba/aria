# ARIA — Decision Log · External datastores & the data VMs

**Decisions D73–D81** · taken 2026-09-23 (design conversation)

One entry per decision: what was decided, why, what was rejected and why, and what it affects.

Postgres and Qdrant leave the cluster. That single move pulls in an authorization model for
cross-schema reads, a credential story, a relocation of the calendar cache, and the reopening of
**D6** — whose revisit trigger fired on 2026-08-19 and has been waiting for a decision since.

> **Two corrections carried through this file.** D31 and D6 each rejected an option on a factual
> claim about the homelab that the [ARIA-79 baseline](../spikes/ARIA-79-cluster-findings-2026-08-19.md)
> later disproved. Those claims are **not** repeated below. This is the same class of error D52 had
> to correct for D51; finding 7 of that baseline names all three cases.

---

### D73 · One external Postgres instance, one schema per service, split owner and runtime roles

**Decided.** **One Postgres instance on its own VM**, on the same LAN as the cluster. **One schema
per service.** Each schema is owned by a **`*_owner` role that runs migrations**, invoked from a
**Helm pre-upgrade hook**. Each service connects at runtime as a separate **`*_app` role with no
DDL rights**.

**Why.** Running Postgres as a StatefulSet was never examined, only assumed — §4 said "a Postgres
operator such as CloudNativePG… are reasonable starting points", which is a placeholder, not a
decision. Two facts made the external instance the better answer. First, **Longhorn's default
StorageClass has `reclaimPolicy: Delete`**, so deleting a PVC destroys the volume — a sharp edge
under a primary datastore. Second, the operational story for a database is backup, restore and
version upgrades, none of which Kubernetes makes easier and all of which it complicates.

One instance with per-schema isolation, rather than one database per service, is what makes D74's
cross-schema reads expressible at all: grants do not cross databases in Postgres.

Splitting `*_owner` from `*_app` means a compromised running service cannot alter its own schema —
the privilege that matters for migrations exists only during a deploy, in a hook, not in a pod
serving traffic.

**Rejected.**

- *Postgres as a StatefulSet in-cluster (the §4 placeholder)* — keeps everything in one packaging
  story under D15, and inherits `reclaimPolicy: Delete` under primary data, plus an operator to
  learn and a backup path that has to be built anyway.
- *One database per service* — the strongest isolation, and it makes D74 impossible: cross-database
  grants are not expressible in Postgres. That property is used deliberately in D77, for exactly one
  database.
- *One shared schema for everything* — simplest to start, and it removes every boundary this file
  goes on to rely on.

**Affects.** ARIA-84, ARIA-87, ARIA-89, ARIA-91, ARIA-96. §4 (supersedes the in-cluster StatefulSet
plan), §5, the Knowledge Core page, and the umbrella chart.

---

### D74 · Cross-schema access is read-only, through views, never base tables

**Decided.** A service that needs another service's data reads it **through named views the
producer exposes** — **never base tables**. A **per-schema read role** is granted `SELECT` on those
views, and consumers are granted **membership** in that role. **`ALTER DEFAULT PRIVILEGES` must be
set per owner role** so that new objects inherit the grant. **`CONNECT` and the public schema are
revoked from `PUBLIC`.**

**Why.** Views are the seam. A base-table grant freezes the producer's physical schema into every
consumer's queries, so the producer can no longer refactor without breaking readers it cannot see.
A view is a published contract the producer controls and can reshape underneath.

The `ALTER DEFAULT PRIVILEGES` clause is called out explicitly because it is the standard way this
is got wrong: grants apply to objects that exist at grant time, so without it the first new view a
producer adds is silently unreadable, and the failure appears days later as a permissions error
nobody can explain.

Revoking `CONNECT` and the public schema from `PUBLIC` removes Postgres's default-open posture,
which otherwise undercuts every grant above.

**Rejected.**

- *Direct base-table grants* — no views to write, and it couples every consumer to the producer's
  physical layout permanently.
- *Service-to-service gRPC calls for all cross-service reads* — the purest boundary, with no shared
  database at all; but it turns every read into a network hop and a new API surface, and ARIA's
  services genuinely share a relational dataset.
- *Read replicas per consumer* — strong isolation, and disproportionate machinery for one small
  instance.

**Affects.** ARIA-55, ARIA-62, ARIA-64, ARIA-84, ARIA-87. The Knowledge Core page.

---

### D75 · Schema access is an approval-time fact; `aria-storage-broker` only reconciles

**Decided.** **Which schemas a service may read is an approval-time fact**, declared in
**`approved-servers.yaml`** and authorized by **merging the PR**. A running service **can never
request additional access**; it can only ask for credentials matching what it was already granted.
**`aria-storage-broker` is a reconciler that applies the manifests — it makes no authorization
decisions.**

**Why.** This is D44's approval gate applied to data instead of code, and for the same reason: the
authorization decision should be made by a human reading a diff, at a moment when nothing is
running, rather than by a service at runtime. It also mirrors the lesson D8 recorded about
`aria-kc-broker` — the component that handles model-influenced traffic is the worst possible holder
of a privilege, so the broker here is deliberately dumb. It reads a manifest and makes the database
match it. Ask it for something the manifest does not say, and there is nothing for it to decide.

The distinction matters for the Self-Extension server (ARIA-103): a service ARIA writes herself
gets its data access reviewed in the same PR as its code, and cannot widen it afterwards.

**Rejected.**

- *A runtime grant API* — flexible, and it puts the authorization decision inside a running system
  where a prompt-injected call can reach it. This is exactly the shape D8 rejected for Keycloak.
- *Manual `GRANT` statements run by hand* — no new component, and no record of what exists, no way
  to rebuild, and it drifts the first time someone is in a hurry.
- *A broker that evaluates policy* — one place to express rules, at the cost of making the broker a
  decision-maker with a credential, which is the thing D8 spent a whole entry avoiding.

**Affects.** New component (`aria-storage-broker`). ARIA-97, ARIA-103, ARIA-105, ARIA-106.
`approved-servers.yaml`'s schema (also extended by D66).

---

### D76 · Vault issues database credentials; no service holds a static superuser credential

**Decided.** **Vault** — already running in this cluster — issues database credentials via its
**database secrets engine**, with **Kubernetes auth**. **No service holds a static Postgres
superuser credential.**

**⚠ Corrects D31's reasoning.** D31 rejected "External Secrets Operator + Vault" on the grounds
that it would be *"a whole secret-management system to run and back up for a handful of
credentials."* **That was factually wrong when it was written.** Vault has been running in this
homelab in its own namespace with a 10 GiB PVC; the ARIA-79 baseline lists it as one of three
decisions argued against infrastructure that already existed. The system does not need to be stood
up, so the cost that justified the rejection is not real.

**What each mechanism covers now — D31 is narrowed, not replaced.**

| Mechanism | Covers |
|---|---|
| **Sealed-secrets (D31)** | Secrets that must exist **in git** and be reconstructable from source: the approval pipeline sealing a new service's credentials (D34), config-shaped secrets, anything the umbrella chart renders |
| **Vault (D76)** | **Database credentials**, issued to a workload that authenticates as itself via Kubernetes auth. Not stored in git at all |

The dividing line is whether the secret is a **deploy-time artifact** (sealed) or a **runtime
identity** (Vault). D34's guarantee — that a self-generated service's credentials cannot exist
before approval — is unaffected, because it is about the approval pipeline, not the storage
mechanism.

**Why Vault for database credentials specifically.** A database credential is the one kind that
genuinely wants to be short-lived and per-workload: it grants access to data rather than to a
config value, and Postgres can mint and expire roles on demand. That is exactly the database secrets
engine's job.

**Rejected.**

- *Sealed-secrets for database credentials too* — one mechanism instead of two, and it means a
  long-lived credential in git whose rotation is a commit, for the most sensitive credential class
  ARIA has.
- *A static superuser per service* — trivial, and it makes D73's owner/app split decorative.

**Open — not settled here.** **Dynamic leases versus static roles**, given connection pooling. A
pooled connection outliving its lease is a real failure mode and the answer depends on the pooler.

**Affects.** ARIA-23, ARIA-53, ARIA-81, ARIA-87, ARIA-89, ARIA-107, ARIA-112. D31 (narrowed).
§2, §4, the deployment page.

---

### D77 · The Knowledge Core lives in its own database, so its schema cannot be granted away

**Decided.** The Knowledge Core lives in **its own database** on that instance. Consequently
**cross-database grants are not expressible**, and **`reads: [knowledge_core]` cannot be
configured** at all. The reconciler (D75) additionally **refuses that schema by name**.

**Why.** D70 decided extension services get no direct Knowledge Core access. A policy that depends
on nobody writing the wrong line in a manifest is a policy that will eventually be violated by a
typo. Putting the Knowledge Core in a separate database makes the prohibition **structural**: there
is no grant that expresses it, so the manifest cannot ask for it and the reconciler cannot apply it.

The name-based refusal is belt and braces, and deliberately so — it means the intent is legible in
the reconciler's own code, not only in a property of Postgres that a future migration might undo by
consolidating databases.

**Rejected.**

- *One database, with the Knowledge Core schema simply never granted* — simpler topology, and it
  relies on discipline where a structural guarantee is available for free.
- *Name-based refusal alone* — one mechanism, and it is ARIA's code rather than the database's,
  which is precisely the residual risk D8 had to record about `aria-kc-broker`'s prefix policy.

**Affects.** ARIA-55, ARIA-62, ARIA-84, ARIA-87. D70 (enforced structurally). The Knowledge Core page.

---

### D78 · Qdrant runs on its own VM; only `aria-knowledge-core` holds its credentials

**Decided.** **Qdrant runs on its own VM.** **Only `aria-knowledge-core` holds Qdrant
credentials**, and **`owner_user_id` filtering is enforced in that service.** The two datastores are
on **separate VMs for separate backup and monitoring** — that is the rationale, not capacity.

**Why.** Under D16, Qdrant holds **primary, non-rebuildable data** — free-text preferences are
authoritative there and cannot be regenerated from Postgres. It therefore needs a backup and
restore story of its own, on its own schedule, and separating the host is what makes that
independent rather than entangled with Postgres's.

Concentrating credentials in one service follows D18: `owner_user_id` filtering is the mechanism by
which one circle member's facts stay invisible to another, and a filter is only a control if there
is no way around it. A second credential holder is a way around it.

**Consequence worth recording, though it is not the motive.** The ARIA-79 baseline found that
**only one node is schedulable** — `kube-worker-01`, 4 CPU and ~7.75 GiB, already running Harbor,
Vault, Longhorn, monitoring, nginx-ingress and MetalLB — and flagged that adding ARIA's services
**plus Postgres and Qdrant** was tight, with no story sizing it. Moving both datastores off-cluster
relieves that pressure materially. It did not drive the decision, but it is a real effect and the
open capacity item should be updated rather than left reading as though nothing changed.

**Rejected.**

- *Qdrant on the Postgres VM* — one host to operate, and it couples two backup and restore stories
  that have different shapes and different recovery objectives.
- *Qdrant in-cluster as a StatefulSet* — consistent with D15's packaging, and it inherits the same
  `reclaimPolicy: Delete` exposure under primary data that D73 moved away from.
- *Credentials shared with other services for direct vector reads* — faster for a consumer, and it
  puts `owner_user_id` enforcement outside the one service that owns it.

**Open.** **Verify Qdrant's actual access-control granularity against the deployed version** — the
design assumes a single API key is the unit, and that should be checked rather than inherited.

**Affects.** ARIA-52, ARIA-76, ARIA-84, ARIA-89, ARIA-96. §2, §4, the Knowledge Core page.

---

### D79 · The calendar cache moves out of the Knowledge Core into the calendar service's own schema

**Decided.** The **calendar cache moves out of the Knowledge Core** and into the **calendar
service's own schema**. **This supersedes D20's placement of the cache**, and only that: D20's
substance — that the external calendar is authoritative, that ARIA caches materialised occurrences
only, and that ARIA never implements recurrence expansion, timezone arithmetic or attendee state —
is unchanged and still binding.

**Why.** D20 put the cache in the Knowledge Core when the Knowledge Core was the only place data
lived. With D73's schema-per-service model, a cache of another system's data belongs to the service
that maintains it. The Knowledge Core's job is ARIA's own knowledge — people, relationships,
preferences, memory — and a materialised view of Google Calendar is not that.

It also fixes a specific awkwardness: under D70 and D77, extension services cannot read the
Knowledge Core, yet the calendar cache is exactly the kind of data another service might legitimately
want. In the calendar service's own schema it can be exposed through a view under D74, which is the
right mechanism.

**Rejected.**

- *Leave it in the Knowledge Core* — no migration, and it keeps a third-party cache inside the store
  that D70 and D77 go to some lengths to seal off.
- *No cache; call the providerevery time* — always fresh, and it puts an external network call on a
  latency path and a rate limit on ARIA's own reads.

**Affects.** **ARIA-64** (scope moves), ARIA-57, ARIA-120. D20 (placement superseded). The Knowledge
Core page.

---

### D80 · The data-VM posture: transport, reach, naming, backup and restore

**Decided.** Six things, recorded together because they are one posture:

- **Postgres connections use TLS** from the **`fpg-ca`** root with **`sslmode=verify-full`**
- **Qdrant is fronted by TLS**, because its **API key is a bearer token** — anyone who sees it
  holds it
- **Antrea egress policy**, plus **`pg_hba.conf`** and a **host firewall**, restrict who can reach
  the hosts
- both hosts are addressed by **DNS name, not IP**
- backups are **pgBackRest** and **Qdrant snapshots**, to the **existing off-site target under a
  locally-held key** (D32's shape, unchanged)
- **neither VM has Kubernetes restarting it**, so **a tested restore is the availability story**

**Why.** Traffic to these hosts crosses the LAN by construction — that is what leaving the cluster
means. `verify-full` rather than `require` is the point worth being explicit about: `require`
encrypts but authenticates nothing, so it stops passive sniffing and not an impostor. `fpg-ca` is
already the homelab root and the ARIA-79 baseline established that it signs internal services
directly, with no intermediate — so this is the simplest possible trust distribution, and ARIA's
pods need `fpg-ca` anyway for Keycloak.

Antrea is a full NetworkPolicy implementation with eight policies already deployed by real charts,
so egress policy here is a mechanism that demonstrably works rather than an aspiration.

DNS names rather than IPs is a small thing that becomes a large thing on the day a VM moves.

The last point is the honest one: in-cluster workloads get restarted by Kubernetes, and these do
not. There is no orchestrator to paper over a failure, so the recovery path has to be one somebody
has actually run.

**Rejected.**

- *`sslmode=require`* — encryption with no server authentication; stops the wrong attack.
- *IP allow-listing alone, without TLS* — a network control substituting for a transport control; it
  does nothing about anything already on the LAN.
- *Backups only to local storage* — faster to restore, and it shares a failure domain with the thing
  being backed up.

**Open.** **Scheduling the restore drill for both VMs.** An untested restore is not a backup, and
nothing currently schedules one.

**Affects.** ARIA-87, ARIA-89, ARIA-96, ARIA-117, ARIA-119. §4. The `fpg-ca` trust open item now has
a second consumer.

---

### D81 · D6's trigger has fired: the data plane is settled, service-to-service mTLS stays open

**Decided.** **D6 is superseded.** Its revisit trigger — *"if ARIA-79 reports the cluster is
multi-node"* — **fired on 2026-08-19**. This entry records what is now settled and what is
deliberately not.

**Settled.** Traffic to the external datastores is TLS with `verify-full` (D80). That is the leg
that unambiguously crosses the LAN, and it is no longer a question.

**Explicitly still open.** **Whether internal service-to-service gRPC gets mTLS.** This entry does
**not** settle it, and a future session should not read D81 as having done so.

**Why the picture is more nuanced than "multi-node, therefore mTLS".** Three facts from the ARIA-79
baseline bear on it, and they do not all point the same way:

- **D6's *second* trigger did not fire.** The CNI is **Antrea**, a full NetworkPolicy
  implementation, with eight policies already deployed. D6's original reasoning required that
  NetworkPolicy genuinely restrict pod-to-pod reachability, and it does.
- **Only one node is schedulable.** `kube-control` carries `node-role.kubernetes.io/control-plane=:NoSchedule`,
  so unless ARIA tolerates that taint, every ARIA pod lands on `kube-worker-01` and inter-service
  gRPC never crosses the LAN in practice. That is a **scheduling accident, not a guarantee** — a
  reopened D6 could make it explicit with a nodeSelector instead of relying on it.
- **⚠ D6's cost argument against mTLS is no longer true.** D6 rejected *"mTLS via cert-manager"* as
  "meaningful work against a threat a single-node cluster may not have", costed partly on standing
  up certificate infrastructure. **cert-manager is already running** in `kube-system`. The ARIA-79
  baseline says so directly: mTLS "is far less work than standing up a CA — which was the main cost
  argument against it". Whoever closes this question must weigh it against the real cost, not D6's.

**Why not decide it now.** The three facts above genuinely pull in different directions, and the
cheap option (pin ARIA to one node) has a capacity interaction with D78 that deserves its own look.
Recording a decision here would mean inventing a resolution to make the page read as finished.

**Rejected — as ways of closing this today.**

- *Inherit D6 unchanged* — the trigger fired; the append-only rule requires a superseding entry, and
  silence would let a triggered decision read as settled.
- *Adopt mTLS now* — defensible, and it deserves the real cost comparison above rather than being
  waved through on a trigger.
- *Pin all ARIA workloads to one node and call it solved* — cheapest, and it makes a scheduling
  accident load-bearing while wasting the second node and colliding with the open capacity question.

**Affects.** ARIA-14, ARIA-42, ARIA-91, ARIA-117. D6 (superseded). §4's internal-networking
paragraph.

---

## See also

- [Identity, tokens & service auth](0001-identity-tokens-service-auth.md) — D5, D6, D8
- [Secrets & deployment posture](0005-secrets-deployment.md) — D31 (narrowed by D76), D32, D34
- [Knowledge model & data ownership](0003-knowledge-model.md) — D16, D18, D20 (placement superseded by D79)
- [The extension surface](0013-extension-surface.md) — D70, which D77 enforces structurally
- [ARIA-79 cluster findings](../spikes/ARIA-79-cluster-findings-2026-08-19.md) — the facts this file rests on

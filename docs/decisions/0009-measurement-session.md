# ARIA — Decision Log · Measurement session

**Decisions D47–D54** · taken 2026-08-19 (session 3)

One entry per decision: what was decided, why, what was rejected and why, and which stories it
affects. Part of [the decision index](README.md).

This session's decisions differ from D1–D46 in one way worth noting: most of them were
**informed by measurements taken the same day**, not by discussion alone. Where a measurement
drove a decision, the entry says so and links the evidence.

---

### D47 · ARIA-19 — The ARIA Keycloak client carries two client roles

**Decided.** `aria-user` and `aria-admin`, as **client roles on the ARIA client**, not realm roles.

- `aria-user` — may interact with ARIA at all.
- `aria-admin` — may approve MCP servers and manage the circle.

**Why.** This role set is much smaller than it first appears, because D7 already moved per-tool
MCP permissions out of it: those live in per-MCP-server Keycloak clients as
`aria:mcp:<server>:<tool>` scopes, provisioned at runtime by `aria-kc-broker` (D8). What is left
for the core client is only ARIA-level authority, and at circle scale there are exactly two kinds:
people who use ARIA, and Jann. Adding a third role before a third distinction exists is ceremony —
the same reasoning D5 applied to service-to-service roles, and it should be applied consistently.

Client roles rather than realm roles because the realm is **shared with other homelab apps**;
realm roles would leak into other applications' tokens.

**Rejected.** *A third `aria-circle-manager` role*, splitting circle onboarding and reconciliation
(ARIA-114) from full admin — a real distinction eventually, and it anticipates the delegation work
the `consent_grants.granted_by` hook exists for, but there is currently exactly one person who
would hold either role, so the split would express nothing. *A single `aria-user` role*, with all
admin authority gated by the PR/approval path instead (D44) — attractively minimal and consistent
with "nothing ARIA runs can approve itself", but it conflates *code trust* (D44's one-time,
admin-level gate) with *runtime authority*, and not every admin action routes through a PR.

**Revisit trigger.** The moment a second person needs admin-ish authority over some of ARIA but
not all of it — most likely when a circle member is onboarded under ARIA-114.

**Affects.** ARIA-19 (this is its last open item), ARIA-22, ARIA-29, ARIA-31, and ARIA-114.

---

### D48 · ARIA-43 — The typed / free-text classification rule

**Decided.** A preference is **typed** (authoritative in Postgres) if and only if all three hold:

1. **A closed value domain exists** — enumerable or constrained (an IANA timezone, a unit system,
   an `HH:MM` range, a boolean, an enum).
2. **Something reads it by exact lookup**, not by similarity — code asks *"what is X's timezone?"*
   and needs the answer, not the three nearest answers.
3. **Being wrong is a bug, not a nuance** — a wrong timezone fires an alarm at the wrong hour.

**Everything else is free-text** (authoritative in Qdrant). Free-text is the **default**;
promoting something to typed is a deliberate act that adds a column and a constraint.

Two rules follow from it:

- **A kind change is a migration, never a dual-write.** Free-text → typed: write the Postgres row,
  *then* delete the Qdrant point, in that order — a crash between the two leaves a recoverable
  duplicate, whereas the reverse order loses data. Typed → free-text is the same shape reversed
  and should involve a human.
- **On conflict, typed wins** on any fact the typed schema can express. The typed store only ever
  holds facts someone deliberately promoted, so a conflicting free-text point is stale by
  construction and should be flagged for cleanup, not merged.

**Why.** D16 settled *that* preferences are partitioned by kind but not *how to decide which kind*,
which is what left ARIA-43 open. Making free-text the default matters more than the three-part test
itself: it means the failure mode of an unclassified fact is "retrievable but fuzzy" rather than
"silently dropped". The test is deliberately about **value domains and read patterns**, both of
which are properties of the data — so it does not depend on which embedder ARIA-52 eventually
picks, which is why this was decidable now rather than blocked.

**Rejected.** *Typed as the default* — safer for correctness and pushes new facts toward structure,
but it forces schema to be invented for things that are only ever read by similarity, which is
precisely the overbuilding D16 walked away from. *Deferring until ARIA-52* — superficially
cautious, but the embedding model has no bearing on whether a fact has a closed value domain, so
deferring would have blocked a decision on an unrelated measurement.

**Consequence carried forward.** The **Qdrant-primary category list** is now publishable, which
ARIA-96 needs in order to back Qdrant up as a source of truth rather than a rebuildable index:
**free-text preferences** and **learned facts**. Everything else is Postgres-authoritative.

**Still open, deliberately.** Whether conversation history additionally gets a *derived* Qdrant
index for retrieval. D36 makes Postgres authoritative for it, so such an index would be the one
legitimately rebuildable collection — a different pattern from the two primary ones, and it should
be decided explicitly rather than drift into existence.

**Affects.** ARIA-43 (closes its central question), ARIA-62, ARIA-76, ARIA-96, ARIA-52.

---

### D49 · ARIA-40 — The canonical internal tool-call shape is ARIA's own

**Decided.** An **ARIA-owned type**, deliberately modelled close to the OpenAI tool-call shape.
Both the hosted backend and the Ollama backend adapt *into* it. No vendor's wire format becomes
ARIA's internal type.

**Why.** ARIA-50 requires a **backend-agnostic conformance suite** that both backends pass
unmodified (D35 makes the Ollama fallback non-optional). If a vendor's format were canonical, the
other backend would permanently translate into a foreign shape and the conformance suite would
quietly become a test of that vendor.

The original counter-argument was that Ollama exposes an OpenAI-compatible endpoint, so adopting
OpenAI's shape would make one adapter nearly free. **Measurement on 2026-08-19 removed that
saving**: Ollama's OpenAI-compatible endpoint cannot disable a model's thinking phase — `think:
false` exists only on the native `/api/chat` — and thinking models reason before emitting a tool
call, which with five tool schemas in the prompt exceeded a 120 s timeout in practice. The Ollama
backend therefore needs the native endpoint regardless, so **neither adapter is free** and the
saving that justified vendor-shaped canonicalisation does not exist.

Staying *close to* the OpenAI shape keeps the translation cheap without the coupling.

**Rejected.** *OpenAI's shape verbatim* — one adapter nearly free, and it is the de-facto
interchange format, but it makes ARIA-50's suite vendor-flavoured and the measured thinking
constraint already undercuts the saving. *Deferring to ARIA-50* — costs nothing today, but
ARIA-53 and ARIA-54 both inherit the answer, and leaving it implicit is how a vendor shape becomes
canonical by accident.

**New requirement this creates.** The Ollama backend (ARIA-54) must **set the thinking parameter
explicitly** rather than inherit the model's default, and ARIA-61's tool-loop latency budget must
account for a reasoning phase the hosted path does not have.

**Open, and measured separately.** Whether tool arguments arrive **fragmented** (OpenAI streams
them as partial strings needing accumulation) or **complete** (Ollama sends whole objects) is an
adapter difference the `LlmBackend` trait must absorb. The probe harness now measures it.

**Affects.** ARIA-40, ARIA-50, ARIA-53, ARIA-54, ARIA-56, ARIA-61.

---

### D50 · `aria-speech` asserts GPU availability at startup and fails closed

**Decided.** `aria-speech` verifies it can reach the GPU at startup and **refuses to become ready**
without it, rather than falling back to CPU and serving degraded.

**Why.** This is not hypothetical — it was observed on 2026-08-19. After a Proxmox host
power-cycle, Ollama ran **100% on CPU while appearing entirely healthy**: API responsive, models
listed, generation roughly 50× too slow. The cause was an LXC bind mount silently creating empty
placeholder files instead of device nodes, suppressed by `bind,optional,create=file`.

A pod that starts, passes its health check and runs 50× too slow **pages nobody**. On a voice path
the degradation is not subtle — it is the difference between a usable assistant and one nobody
uses — but it produces no signal a monitoring system would catch. A crash is strictly better than
a silent capability loss, because a crash is actionable.

**Rejected.** *Warn loudly but keep serving* — "degraded speech beats no speech" is a real argument
for a household assistant, and it is what most services do. It was rejected because it requires the
alerting question (deferred as C-5) to be answered *first*, and warning into a log nobody reads is
exactly how this failure went unnoticed for weeks. *Also dropping `bind,optional` from the LXC
mounts* so the container refuses to start — belt-and-braces and arguably correct for a host named
`llm-inference`, but that is a host-configuration decision on shared homelab infrastructure rather
than an ARIA service decision, and it is recorded as an open item instead.

**Consequence.** The alerting gap (C-5) is no longer purely theoretical. It was deferred on the
grounds that there is "nothing to alert on until services run" — this incident is a counter-example
that already happened, and C-5 should be revisited when Speech is deployed rather than when
services first run.

**Affects.** ARIA-42 (crate scaffold — the assertion belongs in startup), ARIA-93 (deployment —
readiness probe must reflect it), ARIA-46, and the deferred C-5.

---

### D51 · ARIA-79 — The container registry is GitHub Container Registry (`ghcr.io`)

**Decided.** ARIA's container images are published to and pulled from **`ghcr.io`**. No registry is
stood up in the homelab.

**Why.** D13 already put CI on GitHub Actions, and `ghcr.io` is the registry that comes with it:
a workflow can push using the automatically-provided `GITHUB_TOKEN` with no additional credential
to provision, rotate or seal. Standing up an in-cluster registry would mean another workload to
run, secure and **back up** — and since the cluster is now known to be two nodes with no shared
storage established, registry storage would be one more thing to get right before a single image
could be built.

It also composes cleanly with **D45**, which makes an MCP server's identity its **image digest**:
`ghcr.io` returns digests on push, so the approval artifact can record exactly what CI produced.

**Rejected.** *An in-cluster registry* (`registry:2`, or Harbor) — keeps images on the LAN and
removes an external dependency from the deploy path, but it is a stateful workload with its own
storage, TLS and backup story, standing between "no code" and "first image". Harbor additionally
brings replication and scanning that a one-person circle does not need. *Docker Hub* — ubiquitous,
but pull rate limits and public-by-default visibility are both wrong for this.

**Consequence, recorded because it is not free.** D13 accepted that *"source and build logs leave
the network"*, narrowing the local-by-default posture to *"no **personal** data leaves"*. This
extends that: **built artifacts now live off-site too.** The posture is unchanged in substance —
images are derived from source that already left — but §4's list of what crosses the boundary
gains a fourth entry, and it should say so rather than let a reader infer that images stay home.

**Follow-through for ARIA-86 and ARIA-91.** Private `ghcr.io` packages require an
**`imagePullSecret`** on the pulling nodes. Under **D31** that secret is a sealed secret like any
other. Making the packages public would avoid it, at the cost of publishing what ARIA is built
from — a choice worth making deliberately rather than by default.

**Affects.** ARIA-79 (closes its registry criterion), ARIA-86, ARIA-91, ARIA-94, ARIA-106.

---

### D52 · Supersedes D51 — `ghcr.io` confirmed, on corrected reasoning

**Decided.** Unchanged outcome: ARIA's images are published to and pulled from **`ghcr.io`**.
**Changed reasoning:** D51 rejected an in-cluster registry as a workload that would have to be
stood up. **That was factually wrong.** Harbor — registry, Trivy scanning, its own Postgres and
Redis, five PVCs — has been running in this cluster for over a year. D51 was written hours after
the cluster investigation began and before the baseline run revealed it.

D51 is superseded rather than edited, so the error stays visible.

**Why `ghcr.io` still wins, argued against Harbor as a real option rather than a hypothetical cost:**

- **CI runs on GitHub-hosted runners (D13).** Pushing to `ghcr.io` uses the automatically-provided
  `GITHUB_TOKEN` — no credential to provision, seal, rotate, or leak. Pushing to Harbor from a
  hosted runner means either exposing Harbor to the internet or self-hosting runners, and D13
  already rejected self-hosted runners on the grounds that image-building runners are a privileged
  workload interacting badly with the Self-Extension blast-radius question (E1).
- **It removes a dependency from the critical path**, rather than adding one. Harbor is a real
  service Jann already operates; making ARIA's deploys depend on it means a Harbor outage is also
  an ARIA outage, and Harbor's own Postgres is one more thing whose backup matters.

**Genuinely rejected — Harbor**, which is now a serious alternative and was recorded as a strawman:
images stay on the LAN (better for §4's posture), Trivy scanning comes free and would compose well
with D44's approval gate, and D45's image-digest identity works identically. **The deciding factor
is where CI lives, not what the cluster has.** If CI ever moves in-cluster, this should be
reopened — that is the revisit trigger.

**Also rejected:** *ghcr.io with Harbor as a pull-through proxy cache*. Cluster pulls stay on-LAN
and survive a GitHub outage, which is real value — but two locations means two places an image can
be stale, and D45 pins identity to a digest that must resolve consistently in both.

**Unchanged from D51 and still true:** the network-boundary list in §4 gains a fourth entry (built
artifacts now live off-site); private packages need an `imagePullSecret`, which under D31 is a
sealed secret; **the baseline confirmed no `imagePullSecret` exists anywhere in the cluster today**,
so this is new work for ARIA-86/ARIA-91.

**Affects.** ARIA-79, ARIA-86, ARIA-91, ARIA-94, ARIA-106.

---

### D53 · ARIA-19 — ARIA registers in the **master** realm, alongside `kubernetes`

**Decided.** ARIA's Keycloak client, its client roles (D47), and the per-MCP-server clients that
D7/D8 provision at runtime all live in the **`master`** realm — the same realm that already holds
the `kubernetes` client and the circle's existing accounts.

**Why.** D4 established as fact that some circle members already hold accounts in "the existing
shared realm". The 2026-08-19 cluster investigation established which realm that is: **master**.
Putting ARIA anywhere else would mean those members need a second account, which is precisely the
onboarding and reconciliation burden D4 identified and ARIA-114 exists to minimise. One identity
across Jann's personal apps was the point of reusing Keycloak at all (§2).

**Rejected.** *A dedicated `aria` realm* — cleaner isolation, and it would keep D7's unbounded
runtime client creation out of the administrative realm entirely. Rejected because it splits the
circle's identities across two realms and re-creates the account-reconciliation problem the reuse
decision was meant to avoid.

---

> ### ⚠️ Consequence that materially raises D8's recorded residual risk
>
> **In Keycloak, the `master` realm is not just another realm — it is the administrative realm.**
> A service account there with client-management rights can create clients in master, and master
> clients can be granted administrative authority over **every other realm on the instance**.
>
> D8 already recorded a residual risk in plain terms: *"Keycloak cannot scope scope-creation below
> realm level, so the broker's credential remains realm-capable and the prefix policy is ARIA's
> code, not Keycloak's enforcement."*
>
> **In master, "realm-capable" becomes "capable over the whole Keycloak instance."** The blast
> radius of a compromised or prompt-injected `aria-kc-broker` grows from "ARIA's realm" to
> "Keycloak, and therefore every homelab app that depends on it — including the Kubernetes cluster
> itself, whose API server authenticates against this realm."
>
> This does not reverse D53 — the identity-reuse argument is real. But it is a **strictly larger
> risk than D8 assessed**, and D8's assessment was written assuming a non-administrative realm.
>
> **Open, and it should be answered before ARIA-110 is built:** can the broker's service account be
> scoped down — a dedicated service account holding only `manage-clients` rather than full admin,
> or the broker moved to operate on a non-master realm even if ARIA's *users* live in master? The
> second may reconcile both goals: user accounts stay in master, while ARIA's *machine* clients and
> per-MCP-server scopes live in a separate realm the broker administers with no cross-realm reach.
> That option was not considered when this was framed as a binary.

**Affects.** ARIA-19, ARIA-38, ARIA-110 (the broker), ARIA-111, ARIA-114, and D8's risk record.

---

### D54 — The docs library moves into the `aria` monorepo; GitHub is the canonical link target

**Decided.** `docs/` moves from the Google Drive folder into the **`aria` monorepo on GitHub**, and
becomes the single authoritative copy. Jira stories link to **GitHub file URLs**. Confluence
becomes a **read-only mirror** for anyone who prefers reading there, and stops being a place
decisions are edited.

**Why.** The immediate trigger is small and concrete: **Jira cannot open a Google Drive path.** The
story-description rework needs every decision to be one clickable link, and today there is nowhere
to point.

The deeper reason is that the library was designed for this from the start — `docs/README.md` says
it "transplants verbatim into the `aria` monorepo as `docs/`". ARIA-11 is Done, so the repo exists.
Moving it now also means:

- **Documentation versions with the code it describes.** A decision and the commit implementing it
  sit in the same history, and a branch can carry both.
- **One authoritative copy.** Rule 5 of the library — *one fact, one home* — currently holds inside
  `docs/` but not across Drive/Confluence/Jira. Drift between those three is exactly what produced
  the stale `implementation-plan.md`.
- **Review comes free.** A decision change becomes a PR, which composes with D44's use of PRs as
  the approval surface.

**Rejected.** *Confluence as the link target* — native Jira→Confluence links, clickable today, no
repo work. Rejected because it makes Confluence a second editable copy that must be kept in sync
with the library, which is the drift problem the 2026-08-19 restructure existed to end. *Keeping
the library in Drive and linking to Confluence* — same problem with an extra hop. *Both, migrating
later* — would mean rewriting 111 story descriptions twice, or living with dead links in between.

**Consequences.**

- **Confluence's role changes** from "durable record" to "mirror". Its pages should say so, and
  the "Full rationale lives in `CLAUDE.md`" footer they all carry should point at GitHub instead.
- **The Drive folder keeps the legacy 2021 material and `archive/`**, which have no reason to enter
  the repo.
- **Sessions reach the library through the repo**, not the connected Drive folder. `CLAUDE.md` must
  say so, or a future session will read a stale Drive copy and not know it.
- **Do the move before the story rework**, or 111 descriptions get written with dead links.

**Affects.** Every Jira story (link targets), the Confluence space's role, `CLAUDE.md`'s
orientation section, and `docs/backlog/story-style.md`.

---

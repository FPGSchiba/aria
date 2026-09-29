# Open — needs a decision

[Library index](../README.md) · [Decisions](../decisions/README.md) · [Open questions](README.md) · [Sprint 1](../sprints/sprint-01.md)

Answerable by discussion, preference, or homelab knowledge. These are the items a decision
session works through. **Nothing here may be invented** — if it is not decided, it stays here.

**Source:** CLAUDE.md §8, first group, as of 2026-08-12.

---

- Self-Extension approval flow: is a GitHub PR the right approval surface for `request_approval`,
  or should some/all approvals be conversational (ARIA asks Jann directly)? The two can coexist
  for different stakes.
- How much cluster authority the Self-Extension server itself holds — a service that can create
  pods to run unreviewed, model-written code is an escalation path in its own right, and that
  blast radius should be named explicitly rather than fall out of implementation.
- Whether an approval can be revoked after a service is live, and what that does to a running
  workload.
- Generated candidates' tests are themselves generated — how to avoid the model marking its own
  homework.
- Which language generated services are written in, and which model does the generating (the
  Agent Core's backend choice must **not** be inherited as settled here).
- How a server's declared tools get a risk tier during `draft_service` / registration — useful
  context for the conversational consent prompt ("control volume" vs. "unlock the front door").
- Tool-list drift: refresh cadence, and what happens to an existing grant when a tool's schema
  changes or the tool disappears. Sharpened by the per-tool Keycloak scope decision — drift now
  requires a Keycloak write.
- Whether consent grants expire.
- Registry failure policies: reconnect/circuit-breaking for unreachable servers, timeout and
  streaming semantics for long tool calls. *(The audit-write failure clause was closed by
  [D57](../decisions/0010-retention-audit.md) on 2026-08-20.)*
- May `user_id` appear in telemetry, or must it be pseudonymised? At ARIA's scale a stable ID is
  very nearly identifying. *(Still open. [D82](../decisions/0016-observability-v2.md) lowers the
  stakes — telemetry no longer leaves the network — but "local" is not "unrestricted", and Jaeger
  is readable by anyone who can reach it.)*
- Scrubbing failure mode: fail-closed (drop the payload, lose observability during an incident)
  vs. fail-open (export with a warning, risk leaking). *(Still open, with reduced stakes after
  [D82](../decisions/0016-observability-v2.md): a leak is now a leak onto the LAN, not off-site.
  It does **not** become moot — a voice transcript in a span is still a voice transcript.)*
- Does `traceparent` ride along on the outbound hosted-LLM call? Propagating discloses a
  correlation identifier to a third party; not propagating breaks the trace at the most
  interesting hop. *(Sharpened by [D83](../decisions/0016-observability-v2.md), which decided the
  other three propagation points and deliberately left this one alone. The hosted call is now one
  of only two things that still cross the network boundary, which makes the question narrower and
  more answerable, not less.)*
- Whether the end-to-end leak test blocks merges from day one or starts advisory. *(Unchanged, and
  note there is still no CI workflow to block anything with — D13.)*
- Alerting is entirely undecided and no story delivers any. *(Unblocked by
  [D82](../decisions/0016-observability-v2.md): application-level alerting now has a concrete
  substrate — the existing Prometheus for metrics, Jaeger for traces — where before it was waiting
  on what D41 would become. Infrastructure alerting stays Zabbix's, per the audit.)*
- Design the actual delegation UX for `consent_grants.granted_by` (a circle member consenting on
  behalf of another) — the schema hook exists but the flow doesn't yet.
- Decide external/remote access design (e.g. Tailscale/Cloudflare Tunnel in front of the
  Gateway) once the core is working locally — deliberately deferred for now.
- Who polices the "one packaging convention, don't mix" rule once MCP servers live in their own
  repos outside the monorepo.

## Newly surfaced by the 2026-08-12 decisions

These arose *because* of a session-1 decision — each is a consequence that was followed through
and recorded rather than smoothed over.

- **Which Ollama model.** Tool-calling capability varies sharply between local models and directly
  determines whether the backend-agnostic conformance suite passes unmodified. This needs a
  measurement spike of its own; it is currently buried in an implementation story. It also gates
  reopening the Ollama-vs-hosted routing rule.
- Gateway signing-key generation, storage and rotation mechanics.
- Onboarding and reconciliation of circle members in the shared realm — some already have accounts
  via other homelab apps, some do not.
- Reconciliation order when a typed preference (Postgres) and a free-text preference (Qdrant)
  conflict, and what happens when a preference changes kind.
- Cross-owner person deduplication: per-row ownership means the same real person may exist as
  several `person` rows under different owners.
- Calendar cache invalidation and refresh cadence, and who builds the calendar MCP server the
  mirror design now assumes. *(Unchanged. [D79](../decisions/0015-external-datastores.md) moved
  where the cache lives and nothing else — the mirror design, and this gap in it, are untouched.)*
- **Acoustic echo cancellation (or half-duplex gating)** so barge-in does not trigger on ARIA's own
  synthesised voice. Unowned, and plausibly belongs on the client — which sits awkwardly with the
  thin-client reasoning behind putting VAD in Speech.
- Custody of the off-site backup encryption key, which cannot be backed up into what it encrypts.
- Where Ollama's ARIA-managed configuration is recorded and applied.
- The `consent required` status in the Registry's proto contract, and how the suspend-and-retry
  timeout interacts with streaming `Decide` while the user is mid-turn.

---

## See also

- [Triage rules](README.md) — what makes an item A rather than B
- [Decision Log](../decisions/README.md)

## Newly surfaced 2026-08-19 (measurement session)

Each of these came out of running real probes against real hardware. None is answerable by
picking a better model or a better setting — they are design choices ARIA has not yet made.

- **How ARIA prevents hallucinated required arguments.** 🔴 **Highest-value item here.**
  Measured: `qwen3:8b` and `llama3.1:8b` both invented a required `room` argument on
  **100%** of runs where the user did not supply one, despite a system prompt explicitly
  forbidding it. This has physical consequences — a wrong `room` plays music in the wrong place;
  the same shape on a light, a lock or a calendar write is worse. The Agent Core **cannot** rely
  on the model to decline.
  Candidate mechanisms: (a) **MCP elicitation** — the protocol capability for a server requesting
  more information mid-call, exposed by `rmcp` behind a feature flag; (b) **schema design** —
  make such arguments optional with a per-user default resolved from the Knowledge Core as a
  typed preference under D16; (c) **Registry-side validation** — weakest, since the Registry
  cannot judge whether an argument had support in the conversation.
  **Note the rhyme with D42:** conversational consent already returns `consent required`, asks in
  the conversation, and retries the suspended call. A missing argument wants the same machine —
  `information required`, ask, retry. If that reading holds this is an extension of a designed
  mechanism, not new architecture. Test that first. Evidence:
  [ARIA-109 brief](../spikes/ARIA-109-ollama-model.md). Affects ARIA-61, ARIA-118, ARIA-95.

- **Must `aria-speech` fail closed when the GPU is unavailable?** Observed 2026-08-19: after a
  Proxmox power-cycle, Ollama ran **100% on CPU while appearing entirely healthy** — API
  responsive, models listed, ~50× too slow. A pod that starts, passes its health check and runs
  50× too slow pages nobody. Proposal: `aria-speech` asserts GPU availability at startup and
  refuses readiness without it, rather than degrading silently. Affects ARIA-42, ARIA-93.
  Evidence: [ARIA-27 brief](../spikes/ARIA-27-gpu-baseline.md).

- **Does the GPU node's LXC config drop `bind,optional`?** Host-side, but it is the reason the
  above failed silently: `optional` + `create=file` produced empty placeholder files instead of
  device nodes, with no error. Removing `optional` makes the container refuse to start without
  the GPU. Trade-off: a GPU fault becomes a container-won't-start fault. Arguably correct for a
  host named `llm-inference`; it is Jann's call, and it belongs in the deployment record either way.

---

## Resolved 2026-08-19 — moved to the Decision Log

Kept as a pointer for one revision so a reader mid-edit is not confused by the disappearance.

| Was | Now |
|---|---|
| ARIA client role names and granularity | **[D47](../decisions/0009-measurement-session.md)** — `aria-user` + `aria-admin`, client roles not realm roles |
| The typed/free-text classification rule, and what happens on a kind change | **[D48](../decisions/0009-measurement-session.md)** — three-part test, free-text default, kind change is a migration, typed wins on conflict |
| Whether the provider's tool-call format becomes ARIA's canonical shape | **[D49](../decisions/0009-measurement-session.md)** — ARIA-owned type, OpenAI-flavoured; both backends adapt into it |
| Must `aria-speech` fail closed without a GPU | **[D50](../decisions/0009-measurement-session.md)** — yes, asserts at startup, refuses readiness |

Still open from the same batch: whether the GPU node's LXC config drops `bind,optional` (a host
configuration choice on shared homelab infrastructure, deliberately not folded into D50).

## Resolved 2026-08-20 — moved to the Decision Log

| Was | Now |
|---|---|
| What an unrecognized-request record contains | **[D55](../decisions/0010-retention-audit.md)** — raw utterance, 90-day window, ARIA-108's query accessor as the review path |
| **Retention policies, generally** (audit log, unknown log, conversation history, telemetry) — *and* whether a transcript is retained in `aria-speech` | **[D56](../decisions/0010-retention-audit.md)** — one schedule, one row per data class: history and audit **indefinite**, unknown log **90 days**, Speech keeps **nothing**, telemetry **forward-only** |
| Whether the audit append is synchronous on the tool-call path, and whether a failed write on an *allowed* call fails closed | **[D57](../decisions/0010-retention-audit.md)** — two rows through a durable local outbox; fail closed only if durability fails everywhere |
| What happens to a trace ID in an audit row after the trace is gone | **[D58](../decisions/0010-retention-audit.md)** — the row is self-contained; the trace ID is a best-effort hint |

**Deliberately not decided in that batch, though each was adjacent:** the **D41 re-examination**
(above), **whether consent grants expire** (above), **ARIA-74's scrubbing fail-closed vs fail-open**
(above), and **whether conversation history gets a derived Qdrant index** (D48's amendment). D56
states its dependence on each without settling any of them.

## 🔴 Reopened / newly surfaced 2026-08-19 (cluster access work)

Evidence: [ARIA-79 cluster findings](../spikes/ARIA-79-cluster-findings-2026-08-19.md).

- **Service-to-service mTLS inside the cluster.** *(Narrowed 2026-09-23.)* D6's revisit trigger
  fired — the cluster is two nodes — and
  **[D81](../decisions/0015-external-datastores.md) supersedes D6**. But D81 deliberately **did
  not** settle this: it recorded that **service-to-service mTLS is still open**, and corrected the
  one part of D6's reasoning that is no longer true — the cost argument rested on having to stand
  up a CA, and **cert-manager and `fpg-ca` already exist**, so mTLS is much cheaper than D6
  assumed. Options unchanged: mTLS between services, a service mesh, pinning all ARIA workloads to
  one node, or accepting the risk in writing. What is **no longer** part of this question: the
  **data-VM** connections, which [D80](../decisions/0015-external-datastores.md) settled as TLS
  `verify-full` against `fpg-ca`. Affects ARIA-14, ARIA-42, ARIA-91, ARIA-117.

- **🔴 Where can `aria-speech` actually run?** The GPU is **not in the cluster**. Neither node is
  the GPU host — the RTX 2070 is on Proxmox `prox2`, reached by Ollama in LXC CT 200 via bind
  mounts. `04-deployment.md`'s "Speech is pinned to the GPU node" is **not achievable as written**.
  Options: join the GPU host as a node; expose the GPU to an existing node; run `aria-speech`
  outside Kubernetes (breaks D15's packaging convention); or use a hosted STT/TTS service
  (contradicts §4's local-by-default posture). **Bigger than D6** — D6 changes how services talk,
  this changes where one can run. Do not size ARIA-93 or ARIA-46 until decided.

- **How do ARIA's pods obtain trust for the homelab root CA (`fpg-ca`)?** Keycloak's certificate
  is issued **directly** by `fpg-ca`, the homelab's self-signed root — no intermediate, no chain
  assembly. (The cluster's OIDC authenticator had never initialised because the wrong CA,
  `CN = My Internal CA`, was installed at `/etc/kubernetes/pki/keycloak-ca.crt`; that is a homelab
  fix, not an ARIA one.) `aria-identity` (ARIA-20) performs the same TLS operation — fetching OIDC
  discovery and JWKS — so **every ARIA pod needs `fpg-ca` in its trust store**, and presumably for
  every other internal service too. Options: a ConfigMap mounted by the umbrella chart (D15); baked
  into the base image; or a cluster-wide trust bundle. **Nothing in the backlog does this.** Per
  D50, failing to validate must be loud rather than a silent 401 — which is exactly the failure
  mode the API server demonstrated all day.

  **Operational constraint, learned the hard way 2026-08-19:** a trusted CA is read **once, at
  process start**, and held in an in-memory TLS config. Replacing the file on disk changes nothing
  until the process restarts — the Kubernetes API server retried its discovery fetch every 10
  seconds for hours against a cached, wrong CA and never noticed the corrected file. Consequences
  for ARIA: (a) **rotating `fpg-ca` means restarting every ARIA service**, not just updating a
  ConfigMap, so the rotation procedure is a rollout and belongs in the runbook; (b) it strengthens
  the D50 argument — startup is the only moment a service looks at its trust store, so startup is
  the only moment it can fail loudly about it. A service that starts without valid trust will
  reject every request for as long as it runs, with no further signal.

- **Correct the documented distribution.** `02-stack.md`, `04-deployment.md` and several story
  descriptions say **k3s**; it is **kubeadm v1.33.0** with containerd 2.0.5 on Ubuntu 24.04. Not a
  decision so much as a correction, but it invalidates the k3s-specific assumptions that were
  built on it (notably `registries.yaml`, and flannel-by-default for ARIA-117).

## Surfaced by the ARIA-79 baseline run + decision audit, 2026-08-19

See the [decision audit](../spikes/decision-audit-2026-08-19.md) and
[cluster findings](../spikes/ARIA-79-cluster-findings-2026-08-19.md).

- **Does the single schedulable node have capacity for ARIA?** *(Relieved, not closed,
  2026-09-23.)* `kube-control` is tainted `NoSchedule`, so everything lands on `kube-worker-01`:
  **4 vCPU, 7.75 GiB**, already running Harbor, Vault, Longhorn, monitoring, nginx-ingress and
  MetalLB. **Postgres and Qdrant have left the cluster**
  ([D73](../decisions/0015-external-datastores.md),
  [D78](../decisions/0015-external-datastores.md)) — which is the largest single relief available,
  though note that relief was a **consequence** of those decisions and not their motive. What ARIA
  still adds: **eight** services (the original six plus `aria-notify` and `aria-storage-broker`),
  an **OpenTelemetry collector**, and **Jaeger**
  ([D82](../decisions/0016-observability-v2.md)) — the last two replacing the single AppSignal
  collector that was there before. **Still no story sizes this.** Options if it does not fit are
  unchanged. Affects ARIA-91, ARIA-93.

- **What `reclaimPolicy` do ARIA's remaining stateful volumes use?** *(Narrowed 2026-09-23.)*
  Longhorn's default StorageClass is `reclaim=Delete`, so deleting a PVC destroys the volume. The
  sharpest case is gone — **Qdrant's primary, non-rebuildable data is no longer on a Longhorn PVC**
  ([D78](../decisions/0015-external-datastores.md)), and neither is Postgres
  ([D73](../decisions/0015-external-datastores.md)). What remains in-cluster and stateful: the
  **MCP Registry's durable audit outbox** ([D57](../decisions/0010-retention-audit.md)) — which
  holds attempt rows not yet drained, i.e. exactly the rows whose loss the two-row design exists to
  prevent — and **Jaeger's** storage. Smaller stakes, same question. Affects ARIA-87, ARIA-89.

- ~~**Should D32's off-site backup use Longhorn's native S3 support?**~~ **Largely moot,
  2026-09-23.** The question was about Longhorn volumes holding Postgres and Qdrant; those data no
  longer live on Longhorn volumes. [D80](../decisions/0015-external-datastores.md) answers the real
  question directly and application-consistently — **pgBackRest plus Qdrant snapshots**, off-site
  under client-side encryption, **with a restore that has actually been tested**. The
  application-consistency caveat that made Longhorn snapshots unsatisfying is precisely why. *What
  survives:* the two in-cluster stateful things above (the Registry outbox, Jaeger) are backed up
  by nothing, and no story covers them. Affects ARIA-96, ARIA-119.

- **Where does `aria-kc-broker`'s Keycloak admin credential live?** *(Unblocked 2026-09-23, still
  open.)* D8 gave the broker sole custody; Vault's existence offers a third option neither D8 nor
  D31 considered — fetching a short-lived credential rather than holding a long-lived sealed one.
  **[D76](../decisions/0015-external-datastores.md) sets the precedent** by putting *database*
  credentials in Vault and drawing the line between a deploy-time artifact in git (sealed-secrets)
  and runtime identity (Vault). It does **not** decide this one, and the argument does not carry
  automatically: a realm-capable Keycloak admin credential is a much sharper thing than a database
  role. The same question now also applies to **`aria-storage-broker`'s** credential
  ([D75](../decisions/0015-external-datastores.md)).

- **C-5 (alerting) splits in two, and half is already answered.** **Infrastructure alerting is
  solved:** Zabbix is Jann's internal monitoring of the cluster and hosts, with alerting, running
  on both nodes. "Is `aria-speech` up, is the node healthy, is the GPU present" belongs there —
  ARIA deploys nothing, it **exposes** the right signals. **Application-level alerting stays open**
  (consent-gate failures, tool-loop exhaustion, backend failover) and follows whatever D41 becomes.
  The D50 incident cuts both ways here: a GPU silently on CPU for weeks *is* Zabbix's domain on a
  cluster it already watches, and it went unnoticed — because the item did not exist. That
  reinforces D50 from the other side: asserting at startup gives monitoring something unmissable,
  rather than depending on someone having predicted the failure.

- **🔴 Can `aria-kc-broker`'s Keycloak credential be scoped below master-realm admin?** D53 puts
  ARIA in the **master** realm, which is Keycloak's *administrative* realm — a service account
  there with client-management rights can reach **every realm on the instance**, including the one
  the Kubernetes API server authenticates against. That is strictly larger than the residual risk
  D8 recorded, which assumed a non-administrative realm. Options: a dedicated service account
  holding only `manage-clients`; or **split the concerns** — circle *user accounts* stay in master
  (D53's actual requirement), while ARIA's *machine* clients and per-MCP-server scopes live in a
  separate realm the broker administers with no cross-realm reach. That third option was not on the
  table when the realm question was framed as a binary. **Answer before ARIA-110 is built.**

---

## Resolved 2026-09-23 — moved to the Decision Log

| Was | Now |
|---|---|
| **🔴 Re-examine D41 (observability)** — two monitoring stacks already run, and AppSignal would be a third | **[D82](../decisions/0016-observability-v2.md)** — in-cluster collector; **metrics to the existing Prometheus**, **traces to Jaeger in-cluster**, **logs on stdout**. AppSignal dropped. Telemetry no longer crosses the network boundary. Instrumentation unchanged |
| **🔴 Re-decide D31 (sealed-secrets) against Vault as it actually exists** | **[D76](../decisions/0015-external-datastores.md)** — **database credentials come from Vault**; sealed-secrets is **not** replaced but narrowed, with the entry drawing the line between a deploy-time artifact in git and runtime identity. D31's rejection of Vault rested on a claim ARIA-79 disproved, and the entry says so |
| **🔴 D6 must be reopened — its revisit trigger has fired** | **[D81](../decisions/0015-external-datastores.md)** supersedes D6 — but **deliberately leaves service-to-service mTLS open** rather than inventing an answer. The narrowed version is retained above, not deleted |
| Whether trace context propagates across the service, MCP and client boundaries *(implicit — never written down as a question)* | **[D83](../decisions/0016-observability-v2.md)** — four points, three of them manual; the MCP one must live in the generated-server template; the user-facing timeline is built from the turn log, never from traces |
| Where conversation history stops being an implementation detail *(implicit)* | **[D61](../decisions/0012-clients-gateway-surface.md)** — the **turn log is product data** |

**Made moot rather than answered** — the distinction matters, because a moot question leaves no
recorded reasoning to inherit:

- *A named fallback OTLP target if AppSignal's beta Rust support proves unworkable* — there is no
  AppSignal to fall back from. The instinct survives in D82's note that **Tempo is the obvious
  substitute if Jaeger proves wrong**, and that swapping it is a collector config change.
- *Whether ARIA's telemetry egress is acceptable under §4's local-by-default posture* — there is no
  telemetry egress.

## Newly surfaced by the 2026-09-23 decisions

Each arose *because* of a decision taken that day. **None of these is answered anywhere** — a page
that reads as finished on one of them is wrong.

### Notifications ([D71](../decisions/0014-notifications.md), [D72](../decisions/0014-notifications.md))

- **What the offline push path actually is.** APNs, FCM, a self-hosted option like ntfy, or
  something else. Each has different privacy properties — an off-site push service carrying
  notification content would quietly re-add a network-boundary crossing that D82 just removed — and
  different device-registration mechanics. D60's native-client decision does not settle it.
- **Who may send.** D72 gates sending; it does not say whether the Agent Core, an MCP server and a
  scheduled internal job are gated identically.
- **Delivery semantics.** Whether a notification delivered over a live stream is *also* written to
  the inbox, and what happens to one queued for a user with no registered device at all.
- **Quiet-hours overrides.** Whether anything may override them, and who decides. A smoke alarm and
  a podcast recommendation are not the same class of message, and nothing classifies them. Note the
  rhyme with the still-open **tool risk tier** question above — plausibly the same mechanism.
- **Rate-limit shape.** Per server, per user, per pair, and over what window.

### Storage ([D73](../decisions/0015-external-datastores.md)–[D80](../decisions/0015-external-datastores.md))

- **Deprovisioning.** What happens to an MCP server's schema, its data and its roles when its
  approval is revoked or its image digest changes. This is a sharper instance of the long-standing
  "can an approval be revoked after a service is live" item above — it now has data attached to it.
- **Reconciliation drift.** Whether `aria-storage-broker` runs continuously or only on approval,
  and what it does when it finds Postgres and the approval artifact disagree.
- **VM ownership.** Two new VMs exist that nobody has sized, patched or put in a monitoring scope.
  The **backup** regime is decided (D80); the rest of operating them is not. Note that Zabbix
  already monitors Jann's hosts, so this is likely a scope addition rather than new machinery.

### Observability ([D82](../decisions/0016-observability-v2.md), [D83](../decisions/0016-observability-v2.md))

- **🔴 Jaeger's trace retention.** [D56](../decisions/0010-retention-audit.md)'s retention table
  states telemetry retention as *"30/45/60 days for traces depending on AppSignal plan"* — **that
  row is now invalid**, because it describes a vendor plan ARIA does not have. Retention became
  ARIA's own choice and D82 did not make it. This worsens the interaction D56 and
  [D58](../decisions/0010-retention-audit.md) already flagged: an indefinitely-retained
  `consent_audit_log` row carries a trace ID that will dangle on a schedule **ARIA** picked.
- **Who owns client-side tracing**, across native phone, native desktop and the web shell. D83
  requires a turn's root span to start on the device; no story owns the library that does it, and
  it is now a trace boundary that includes code ARIA does not operate.
- **Whether the turn-log row stores a trace ID**, and what it means once that trace has expired.
  D58 answered this shape for audit rows — self-contained row, best-effort hint — and the same
  answer probably applies, but D83 did not say so and the two audiences differ.
- **Log aggregation.** "Logs on stdout for now" is honest, not solved: nothing aggregates across
  pods, and a restart loses history. No story covers it.

### Clients & the extension surface ([D60](../decisions/0012-clients-gateway-surface.md)–[D70](../decisions/0013-extension-surface.md))

- **What the native clients are built with**, on phone and desktop. D60 puts them in scope and
  commits them to gRPC streaming; it deliberately does not resurrect the 2021 iOS/Swift and
  Android/Xamarin plans, and nothing replaces them.
- **Turn-log payload retention.** D56 set conversation-history retention to indefinite, which
  covers the log itself. The narrower question — how long the **per-event payloads** a timeline
  renders are kept — was not asked then, because the turn log was not yet product data.
- **Acoustic echo cancellation**, still unowned (above), and now more awkward rather than less:
  D60 gives the clients an identity, which turns "it plausibly belongs on the client" from a
  hypothetical into a concrete claim about a component that exists.

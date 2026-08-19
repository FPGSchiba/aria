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
- Registry failure policies: reconnect/circuit-breaking for unreachable servers, audit-write
  failure behaviour on *allowed* calls, timeout and streaming semantics for long tool calls.
- May `user_id` appear in telemetry, or must it be pseudonymised? At ARIA's scale a stable ID is
  very nearly identifying.
- Scrubbing failure mode: fail-closed (drop the payload, lose observability during an incident)
  vs. fail-open (export with a warning, risk leaking).
- Does `traceparent` ride along on the outbound hosted-LLM call? Propagating discloses a
  correlation identifier to a third party; not propagating breaks the trace at the most
  interesting hop.
- A named fallback OTLP target if AppSignal's beta Rust support proves unworkable.
- Whether the end-to-end leak test blocks merges from day one or starts advisory.
- **Retention policies, generally** — `consent_audit_log`, the unrecognized-request log,
  conversation history, and telemetry. A security audit trail normally outlives operational
  telemetry, so stored trace IDs may resolve to traces that no longer exist.
- Alerting is entirely undecided and no story delivers any.
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
  mirror design now assumes.
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

## 🔴 Reopened / newly surfaced 2026-08-19 (cluster access work)

Evidence: [ARIA-79 cluster findings](../spikes/ARIA-79-cluster-findings-2026-08-19.md).

- **🔴 D6 must be reopened — its revisit trigger has fired.** The cluster is **two nodes**
  (`kube-control` 192.168.1.20, `kube-worker-01` 192.168.1.21). D6 kept internal gRPC plaintext
  *conditional on the cluster being single-node*; it is not. Voice audio would cross the LAN in
  the clear between nodes. Options: mTLS between services, a service mesh, pinning all ARIA
  workloads to one node, or accepting the risk with the reasoning written down. Per the
  append-only rule this becomes a **new entry superseding D6**, not an edit. Affects ARIA-14,
  ARIA-42, ARIA-91, ARIA-117.

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

- **🔴 Re-decide D31 (sealed-secrets) against Vault as it actually exists.** D31 rejected
  External Secrets + Vault as "a whole secret-management system for a handful of credentials" —
  but **Vault is already deployed and administered**. D31's *decisive* argument survives (sealing
  needs only a public cert, so no private key reaches GitHub Actions), but the rejection was
  argued against a hypothetical burden rather than a running service. Recommendation in the audit
  is to keep sealed-secrets on the surviving argument — but it needs a decision entry that argues
  honestly. Affects D34 (whose pipeline design assumes sealing), ARIA-112, ARIA-81.

- **🔴 Does the single schedulable node have capacity for ARIA?** `kube-control` is tainted
  `NoSchedule`, so everything lands on `kube-worker-01`: **4 vCPU, 7.75 GiB**, already running
  Harbor, Vault, Longhorn, monitoring, nginx-ingress and MetalLB. ARIA adds six services plus
  **Postgres and Qdrant**. **No story sizes this.** If it does not fit, the options are tolerating
  the control-plane taint, adding a node, or trimming what ARIA deploys — all of which change
  ARIA-91 and ARIA-93.

- **What `reclaimPolicy` do ARIA's stateful volumes use?** Longhorn's default StorageClass is
  `reclaim=Delete`, so deleting a PVC destroys the volume. Under **D16** Qdrant holds **primary,
  non-rebuildable** data. `Retain`, or a dedicated StorageClass, deserves a deliberate choice in
  ARIA-87/ARIA-89 rather than inheriting the default. (Volume expansion **is** supported, which
  removes the size-it-right-first-time pressure.)

- **🔴 Re-examine D41 (observability) — CONFIRMED: two monitoring stacks already run.**
  **Prometheus** (StatefulSet + node-exporter + kube-state-metrics, `lens-metrics`) and a full
  **Zabbix** stack (`monitoring`), both for 479 days. D41 would make ARIA's OTel → AppSignal
  collector the **third**. What survives: the single-collector enforcement point, which is what
  makes "no conversation or voice content leaves in telemetry" checkable in one place — and
  Prometheus gives no traces, which is ARIA's actual need. What does not: that ARIA needs a
  *complete external* stack. **Metrics have a local home already**, so exporting them is a network
  egress §4 does not require; the ARIA-80 free-tier concern shrinks if only traces leave; and a
  **fully local trace backend (Tempo/Jaeger) beside the existing Prometheus** is now a serious
  option that removes the AppSignal-Rust-beta risk D41 itself flagged. Decide before ARIA-60.

- **Should D32's off-site backup use Longhorn's native S3 support?** Verified: **no Longhorn
  backup target is currently configured**, so nothing is already solved — but Longhorn does
  scheduled snapshots and S3 replication natively, and D32 specified building it without
  considering that. The application-consistency caveat stands: a volume snapshot of a running
  Postgres is not a `pg_dump`, and under D16 Qdrant's consistency matters equally. Likely shape is
  Longhorn for volume-level off-site replication **plus** application-level dumps — but that is a
  decision. Before ARIA-96 and ARIA-119.

- **Where does `aria-kc-broker`'s Keycloak admin credential live?** D8 gave the broker sole custody;
  Vault's existence offers a third option neither D8 nor D31 considered — fetching a short-lived
  credential rather than holding a long-lived sealed one. Does not change D8's service boundary,
  only the secret's storage. Follows whatever the D31 re-decision concludes.

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

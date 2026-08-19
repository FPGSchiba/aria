# Decision audit — D1–D52 against the real cluster

[Library index](../README.md) · [Decisions](../decisions/README.md) · [Cluster findings](ARIA-79-cluster-findings-2026-08-19.md) · [Open questions](../open-questions/README.md)

**Why this exists.** The ARIA-79 baseline run revealed that several decisions were argued partly on
*"that would be a whole system to stand up"* — about systems that have been running in this
homelab for over a year. D51 was the clearest case: decided in the morning, invalidated by the
afternoon's baseline, superseded by D52.

**The failure mode is not "wrong answer". It is "right answer, fictional reasoning".** A rejected
alternative dismissed on a cost that does not exist is a strawman, and a strawman in an
append-only decision log is worse than an open question — future readers treat it as settled.

This audit re-reads every decision whose reasoning depends on what the homelab does or does not
have, against what the cluster actually contains.

---

## What the cluster actually has

From the [baseline run](ARIA-79-cluster-findings-2026-08-19.md):

| Running | Namespace | Relevance |
|---|---|---|
| **Harbor** (registry, Trivy scanning, Postgres, Redis) | `default` | Container registry + image scanning |
| **Vault** (10 GiB PVC) | `vault` | Secret management |
| **cert-manager** | `kube-system` | Certificate issuance — makes mTLS cheap |
| **Longhorn** (default SC, expansion, `reclaim=Delete`) | `longhorn-system` | Replicated block storage + its own backup capability |
| **Antrea** | `kube-system` | CNI that **does** enforce NetworkPolicy |
| **MetalLB** + **nginx-ingress** | `kube-system` | LoadBalancer + ingress |
| **Keycloak** | `keycloak` | **In-cluster**, not external as assumed |
| **Zabbix** (agent DaemonSet, proxy, kube-state-metrics) | `monitoring` | Jann's internal monitoring **of the cluster/hosts**, with alerting |
| **Prometheus** + node-exporter + kube-state-metrics | `lens-metrics` | Cluster/node metrics. **No tracing** |

Cluster shape: **kubeadm v1.33.0**, 2 nodes, only `kube-worker-01` schedulable (4 vCPU, 7.75 GiB).

---

## Findings

### 🔴 A1 — D31 (sealed-secrets) rests on the same false premise. **Needs re-decision.**

D31 rejected External Secrets + Vault as *"a whole secret-management system for a handful of
credentials"*. **Vault is already deployed and Jann already administers it** (his Keycloak token
carries `/VaultAdmin`).

**What survives.** D31's *decisive* argument is independent of Vault's existence and still holds:
sealing needs only the cluster's **public** certificate, so with GitHub-hosted runners **no private
key ever leaves the network**. That is a real property, not a cost estimate.

**What does not survive.** The characterisation of Vault as disproportionate. The honest comparison
now:

| | sealed-secrets | Vault + External Secrets Operator |
|---|---|---|
| CI can seal without a private key | **yes** — decisive for D13's hosted runners | no — CI would need Vault credentials or a separate write path |
| New system to operate | yes (controller) | **no — already running** |
| Secret rotation | re-seal and re-commit | **native** |
| Backup burden | controller private key is critical state (D32) | Vault's own, already solved by Jann |
| Audit trail of secret access | none | **native** |
| Fits D34 (approval pipeline seals a new service's credentials) | directly | needs rethinking — the pipeline would write to Vault instead |

**Recommendation: keep sealed-secrets, but record the real trade-off.** The no-private-key-in-CI
property is worth more than rotation ergonomics at this scale, and D34's pipeline design assumes
sealing. But this needs a decision entry that argues against Vault-as-it-exists rather than
Vault-as-a-hypothetical-burden. **Not decided here.**

### 🟡 A2 — D32 (backups) may be reinventing what Longhorn provides

D32 decided *"Postgres operator backups and Qdrant snapshots to local storage, replicated to
off-site object storage with client-side encryption."*

**Longhorn has native backup to S3-compatible object storage**, including scheduled snapshots and
restore. That does not automatically replace application-consistent Postgres backups — a
volume snapshot of a running database is not the same as a `pg_dump` or a WAL-archived base backup,
and D16 makes Qdrant primary data whose consistency matters. **But D32 never considered it**, and
the off-site replication half may be entirely solved already.

**Verified 2026-08-19: no Longhorn backup target is configured** (`settings.longhorn.io
backup-target` returns NotFound). So D32's off-site replication is **not** already solved — but
Longhorn can do it natively, and D32 specified building it without considering that.

**Action:** when ARIA-96/ARIA-119 are worked, evaluate Longhorn's native S3 backup as the
*mechanism* for D32's *policy*, rather than assuming custom tooling. The application-consistency
caveat stands: a volume snapshot of a running Postgres is not a `pg_dump`, and under D16 Qdrant's
consistency matters as much. Likely answer is Longhorn for volume-level off-site replication plus
application-level dumps for Postgres — but that is a decision, not an assumption. Confirm current
state via the Longhorn UI at `longhorn.fpg`.

### 🟡 A3 — D6's reopening is cheaper than assumed (already reflected in the findings doc)

The multi-node trigger fired, but **cert-manager is already running**, so mTLS between services no
longer means standing up a CA — which was the main cost argument. And **Antrea genuinely enforces
NetworkPolicy**, so D6's original reasoning about NetworkPolicy holds; only the LAN-crossing
concern is live. The reopening should weigh cert-manager-issued mTLS as a *cheap* option, not an
expensive one.

### 🔴 A4 — CONFIRMED, and worse than suspected: **two** monitoring stacks already run

Verified 2026-08-19:

```
lens-metrics   prometheus-0 (StatefulSet), node-exporter (DaemonSet), kube-state-metrics
monitoring     zabbix-agent (DaemonSet), zabbix-proxy, zabbix-kube-state-metrics, metrics-server
```

**Both have been deployed for 479 days.** But they are not interchangeable, and Jann's
clarification is what makes the conclusion precise:

| Stack | What it is for | Does it cover ARIA's need? |
|---|---|---|
| **Zabbix** | Jann's internal monitoring **of the cluster and hosts themselves** — infrastructure health, with alerting | **No** for application telemetry. **Yes** for "is `aria-speech` up, is the node healthy" |
| **Prometheus** (`lens-metrics`) | Cluster/node metrics via node-exporter + kube-state-metrics | Could absorb ARIA's **metrics**. No traces |
| **Neither** | — | **Distributed tracing.** ARIA is trace-first (ARIA-67 propagates context across every gRPC boundary, including bidi streams) |

So D41 is **not** simply redundant. The gap it fills — traces — is real and nothing else covers it.
What was never weighed is that **two of the three signal types already have somewhere to go.**

**What survives, and it is the important half.** D41's core property is independent of what else
runs: routing traces, logs **and** metrics through a single collector is what makes "conversation
and voice content never leaves in telemetry" enforceable in **one** place rather than three. That
argument gets *stronger* in a homelab with multiple monitoring systems, not weaker — the failure
mode D41 guards against is voice content leaking into a span attribute or log line, and more
export paths means more places for that to happen. **Whatever is decided, ARIA's telemetry should
still funnel through one enforcement point.**

**What does not survive.** The implicit assumption that ARIA needs a *complete* external
observability stack. Concretely:

- **Metrics have a local home already.** Sending ARIA's metrics to AppSignal is a **new network
  egress** that §4 does not require and Prometheus could absorb — vendor-neutral OTLP was chosen
  precisely so the backend is a config change. Prometheus can ingest OTLP directly.
- **The AppSignal free-tier concern (ARIA-80) shrinks or disappears** if only traces are exported.
- **A fully local trace backend is now a serious option** that was never on the table: Tempo or
  Jaeger alongside the existing Prometheus, keeping *all* telemetry on the LAN and removing the
  AppSignal-Rust-beta risk D41 explicitly flagged.

**Recommendation: re-examine D41 before ARIA-60.** Keep the single-collector enforcement point;
reconsider what it exports where. Not decided here.

### 🔵 A4b — C-5 (alerting) splits into two questions, and one is already answered

C-5 was deferred on the grounds that there is "nothing to alert on until services run". The split
is now clear:

- **Infrastructure alerting — solved.** Zabbix already monitors the cluster and hosts, with
  alerting, and Jann administers it (`/ZabbixAdmin`). "Is `aria-speech` running, is the node
  healthy, is the GPU present" belongs there. ARIA does not need to deploy anything for this; it
  needs to **expose the right signals**.
- **Application-level alerting — still open.** Consent-gate failures, tool-loop exhaustion, LLM
  backend failover — Zabbix is the wrong shape for these and they follow whatever D41 becomes.

**The D50 incident becomes sharper, not softer.** A GPU silently falling back to CPU for weeks is
precisely infrastructure health — Zabbix's own domain, on a cluster Zabbix already watches — and it
went unnoticed anyway. That is not an argument for a different tool; it is an argument that the
*item did not exist*. It reinforces D50 from the other side: ARIA asserting GPU availability at
startup and refusing readiness gives Zabbix something unmissable to observe, instead of requiring
someone to have thought to add a VRAM check in advance.

### 🟢 A5 — Decisions checked and unaffected

- **D13** (GitHub Actions) — "no CI infrastructure to operate" is still true; Harbor is a registry,
  not CI. The self-hosted-runner rejection also still stands on its own reasoning.
- **D45** (image digest as server identity) — works identically on `ghcr.io` or Harbor.
- **D15** (one umbrella Helm chart) — unaffected by what else runs.
- **D7/D8** (per-tool Keycloak scopes, `aria-kc-broker`) — Keycloak being in-cluster does not change
  the authorization model. *But see the new note below on where the broker's admin credential lives.*
- **D16–D23** (knowledge model) — no infrastructure dependency.
- **D24–D30** (audio) — no infrastructure dependency, though GPU placement is a separate open issue.
- **D50** (Speech asserts GPU) — unaffected, and reinforced by the passthrough incident.

### 🔵 A6 — New consideration created by A1, not a correction

D8 put the **only Keycloak admin credential** in `aria-kc-broker` because the MCP Registry is the
worst possible holder of it. **Vault's existence offers a third option** neither D8 nor D31
considered: the broker fetching a short-lived credential from Vault rather than holding a
long-lived sealed one. That does not change D8's *service boundary* decision, only where the secret
lives — but it is exactly the kind of option that was invisible while Vault was assumed absent.

---

## Gaps — **closed 2026-08-19**

All four discovery commands were run. Results folded into A2 and A4 above.

| Question | Answer |
|---|---|
| Prometheus/Grafana already running? | **Yes** — Prometheus StatefulSet + node-exporter + kube-state-metrics in `lens-metrics` |
| Anything else monitoring? | **Yes** — full Zabbix agent/proxy stack in `monitoring` |
| Longhorn backup target configured? | **No** — `backup-target` setting NotFound |
| Other workloads | Harbor, Vault, cert-manager, MetalLB, nginx-ingress, Longhorn, metrics-server |

## What this audit changes about process

Two rules worth adding to how decisions get taken here, both learned today:

1. **A rejected alternative must be checked against reality before it is written down.** "That would
   be a whole system to stand up" is a claim about the world, not a judgement — and it is cheap to
   verify. D51 could have been right the first time with one `kubectl get pods -A`.

2. **Infrastructure inventory is a prerequisite for infrastructure decisions**, not a parallel
   track. ARIA-79 sat in sprint 1 for good reason; forty-six decisions were taken before it ran.
   Most survived. The ones that did not were all of the same kind.

*Audit performed 2026-08-19 against the ARIA-79 baseline run. Covers D1–D52. Gaps closed
the same day; no open verification remains.*

# ARIA — Deployment & hosting

[Library index](README.md) · [Decisions](decisions/README.md) · [Open questions](open-questions/README.md) · [Sprint 1](sprints/sprint-01.md)

Where ARIA runs, what infrastructure it reuses rather than duplicates, how it is packaged, and
what crosses the network boundary.

**Source:** CLAUDE.md §4 as of 2026-08-12, moved here verbatim.

---

> [!WARNING]
> ## Contradicted by observation, 2026-08-19
>
> Three statements below are now known to be wrong. See
> [ARIA-79 cluster findings](spikes/ARIA-79-cluster-findings-2026-08-19.md).
>
> 1. **It is not k3s.** The cluster is **kubeadm v1.33.0**, containerd 2.0.5, Ubuntu 24.04.
> 2. **It is not single-node.** Two nodes — which has **fired D6's revisit trigger**.
> 3. **There is no GPU node in the cluster.** The GPU is on Proxmox `prox2`, outside Kubernetes,
>    so "Speech is pinned to the GPU node" is not achievable as written.
>
> The text below is preserved as written until the superseding decisions are taken. Do not build
> on points 1-3.

ARIA runs on **Jann's existing k3s cluster** at home, in its own `aria` namespace — not a new
cluster, not a cloud VPS. This keeps voice and personal data local by default. Three things
deliberately cross the network boundary, and it is worth naming them rather than letting
"local by default" read as absolute: the **hosted LLM API call**, **source code and CI logs**
(GitHub-hosted runners, section 2), and **encrypted off-site backups** (below). No voice audio,
conversation content or knowledge-base content leaves except as ciphertext under a key Jann holds.

- **Reused infrastructure (no new deployment):** Keycloak (new client + client roles in the
  existing shared realm) and Ollama (new fallback backend behind the Agent Core's LLM
  interface, reached over its existing in-cluster/LAN endpoint). **ARIA may reconfigure Ollama's
  server settings** — residency, concurrency, model preloading — because that is the only way to
  guarantee a VRAM budget for Speech on a shared GPU. This makes ARIA a co-owner of that shared
  infrastructure, and a change made for Speech's benefit affects everything else using it.
- **New in-cluster components:** all ARIA services (Gateway, Speech, Agent Core, MCP Registry,
  **Knowledge Core**, **KC provisioning broker**) as Deployments; Postgres and Qdrant as
  StatefulSets with persistent volumes (a Postgres operator such as CloudNativePG, and Qdrant's
  official Helm chart, are reasonable starting points — see section 8); the sealed-secrets
  controller; the AppSignal self-hosted collector.
- **Packaging:** **one umbrella Helm chart** covering every ARIA workload in the namespace — one
  release, one `values.yaml` holding all image tags, atomic upgrade and rollback of the namespace
  as a unit. Each MCP server, living outside the monorepo, gets its own small chart following the
  same convention; that convention is also the template `deploy_service` generates.
- **Scheduling:** stateless services (Gateway, Agent Core, KC broker) can run on any
  node. **The MCP Registry is no longer among them (D57):** its audit outbox requires durable local
  storage, so it needs a volume and the scheduling constraints that follow from one. Speech is pinned (nodeSelector/toleration) to the GPU node, reusing whatever GPU runtime
  config already makes Ollama work there (NVIDIA device plugin or ROCm, whichever is set up).
- **MCP servers as workloads:** each MCP server (e.g. a future Sonos controller, the calendar
  server, or the Self-Extension server) is deployed as its own Deployment + Service in the cluster
  (or reached as a remote HTTP endpoint if it runs on a LAN device outside the cluster), registered
  with the MCP Registry by DNS name/URL.
- **Internal networking:** ClusterIP services + k8s DNS for service-to-service gRPC calls, with
  Keycloak client-credentials for auth between them (see section 2). Internal gRPC is **not**
  additionally TLS-wrapped; **NetworkPolicy** restricts which pods may reach which services, and
  confidentiality otherwise rests on cluster network isolation. **This is conditional:** if the
  cluster turns out to be multi-node, voice audio crosses the LAN in plaintext between nodes and
  this decision must be revisited rather than inherited.
- **Backups:** Postgres operator backups and Qdrant snapshots to local storage, replicated to
  off-site object storage with **client-side encryption** under a locally-held key. **Retention
  interaction (D56):** conversation history and `consent_audit_log` are kept indefinitely, so no
  backup rotation can outlive their retention. Exactly one store is pruned — the
  unrecognized-request log at 90 days — and a row deleted there **does** survive in any older dump.
  Reconciling that is handed to ARIA-96 / ARIA-119; it is not solved here. Qdrant holds
  primary data (section 2) and is **not** re-derivable from Postgres, so it must be backed up as a
  source of truth. The sealed-secrets controller's private key is likewise critical state.
- **External/remote access** (phone away from home, etc.) is explicitly **not designed yet** —
  see open questions.

---

## Amendments from the 2026-09-23 design conversation

Two of the bullets above are now substantially wrong, and the list of things that cross the network
boundary has changed in both directions.

### The datastores left the cluster (D73, D78, D80)

"Postgres and Qdrant as StatefulSets with persistent volumes" is no longer the plan. **Each runs on
its own VM**, outside the cluster, reached over the LAN:

- **Postgres** — one VM, **one schema per service**, with a `*_owner` / `*_app` role split so that
  the role an application connects as cannot alter its own schema
  ([D73](decisions/0015-external-datastores.md)). Cross-schema reads are **read-only views**
  ([D74](decisions/0015-external-datastores.md)). The **Knowledge Core gets its own database**, not
  merely its own schema ([D77](decisions/0015-external-datastores.md)).
- **Qdrant** — its own VM ([D78](decisions/0015-external-datastores.md)). Note the ordering of the
  argument there: the motive is **backup and monitoring** — Qdrant holds primary, non-re-derivable
  data and belongs in the same backup regime as the rest of Jann's VMs. **Capacity relief on a
  one-node cluster is a consequence, not the motive.**

This makes the **backup bullet above partly obsolete**: "Postgres operator backups" describes an
operator ARIA no longer deploys. [D80](decisions/0015-external-datastores.md) replaces it with
**pgBackRest plus Qdrant snapshots**, and adds the requirement the old bullet never stated — **a
restore that has actually been tested**. Off-site replication with client-side encryption under a
locally-held key is unchanged, and so is the D56 retention interaction.

[D80](decisions/0015-external-datastores.md) also sets the **posture for the data VMs**, which the
in-cluster plan never needed: TLS with **`verify-full`** against a certificate from **`fpg-ca`**,
reached by **DNS name and not IP**, with access restricted in three layers — Antrea egress policy
on the cluster side, `pg_hba.conf` in Postgres, and a host firewall on the VM.

### Two new workloads, one dropped

- **`aria-notify`** ([D71](decisions/0014-notifications.md)) — stateless, schedulable anywhere.
- **`aria-storage-broker`** ([D75](decisions/0015-external-datastores.md)) — holds the credential
  that can create schemas and grants. Like `aria-kc-broker`, it is deliberately small and deliberately
  not the Registry.
- **The AppSignal self-hosted collector is dropped** ([D82](decisions/0016-observability-v2.md)).
  In its place: an **in-cluster OpenTelemetry collector** ARIA owns, and **Jaeger deployed
  in-cluster** as the trace backend. Metrics go to the **Prometheus that already runs in
  `lens-metrics`** — not a new deployment. **Logs go to stdout**, with nothing aggregating them;
  that is a known gap, not a solved problem.

### What crosses the network boundary

The opening paragraph names three things. **It is now two.**

| | Status |
|---|---|
| The hosted LLM API call | unchanged |
| Source code and CI logs (GitHub-hosted runners) | unchanged |
| Encrypted off-site backups | unchanged (ciphertext only) |
| ~~Telemetry~~ | **removed** — [D82](decisions/0016-observability-v2.md) keeps all three signals on the LAN |

Telemetry was never in the original three-item list, but D41 had made it a fourth. D82 takes it
back off. This is the first change to that list in the narrowing direction.

### Internal networking

[D81](decisions/0015-external-datastores.md) supersedes **D6**. The revisit trigger in the bullet
above ("if the cluster turns out to be multi-node") **fired** — ARIA-79 found two nodes. D81's
answer is not "add mTLS everywhere": it records that **service-to-service mTLS is still open**, and
corrects the one part of D6's reasoning that is no longer true — the cost argument rested on having
to stand up a CA, and **cert-manager and `fpg-ca` already exist**. The decision has to be retaken
on its real merits rather than inherited or dismissed on a stale cost.

The **data-VM connections are a separate matter and are not open**: those are TLS `verify-full`
today, per D80.

### Scheduling

Unchanged for the services listed, with one addition: **Speech's pinning is still unachievable as
written** — ARIA-79 found the GPU is on Proxmox `prox2`, outside Kubernetes, and no decision taken
on 2026-09-23 addresses that. It remains open.

---

## See also

- [ARIA-79 cluster baseline](spikes/ARIA-79-cluster-baseline.md) — the facts this section assumes but does not yet record
- [Decision Log · Secrets & deployment](decisions/0005-secrets-deployment.md)

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
- **Scheduling:** stateless services (Gateway, Agent Core, MCP Registry, KC broker) can run on any
  node. Speech is pinned (nodeSelector/toleration) to the GPU node, reusing whatever GPU runtime
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
  off-site object storage with **client-side encryption** under a locally-held key. Qdrant holds
  primary data (section 2) and is **not** re-derivable from Postgres, so it must be backed up as a
  source of truth. The sealed-secrets controller's private key is likewise critical state.
- **External/remote access** (phone away from home, etc.) is explicitly **not designed yet** —
  see open questions.

---

## See also

- [ARIA-79 cluster baseline](spikes/ARIA-79-cluster-baseline.md) — the facts this section assumes but does not yet record
- [Decision Log · Secrets & deployment](decisions/0005-secrets-deployment.md)

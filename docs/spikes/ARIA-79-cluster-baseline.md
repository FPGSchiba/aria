# ARIA-79 — Confirm the k3s cluster baseline

[Library index](../README.md) · [Spikes](README.md) · [Deployment](../04-deployment.md) · [Sprint 1](../sprints/sprint-01.md)

**Status: discovery script ready — needs a run against the real cluster.**
This is a pure bucket-B item: nothing here is answerable from a conversation, and every attempt to
guess would be recorded as fact by a downstream story.

---

## Why this one is first

ARIA-79 blocks five other stories and is a bottleneck in the dependency graph, but that
undersells it. **Two already-taken decisions are conditional on its output**, and one of them can
be invalidated:

| Finding | Consequence |
|---|---|
| **Cluster is multi-node** | **D6 must be reopened.** Voice audio would cross the LAN in plaintext between nodes, which the decision explicitly does not cover. |
| **CNI does not enforce NetworkPolicy** | **D6 must be reopened**, harder. Its entire justification for skipping internal TLS is "NetworkPolicy restricts which pods may reach which services". If nothing enforces it, that sentence is false. |
| No container registry exists | ARIA-86's scope grows; `ghcr.io` becomes the default given D13 |
| StorageClass lacks expansion / weak provisioner | ARIA-87/89 sizing becomes a one-shot decision, and D16 raises the bar because **Qdrant holds primary data** |
| Ollama runs outside the cluster | ARIA-27's VRAM budget is not negotiated through Kubernetes resource requests at all — it becomes a host-level agreement |

The NetworkPolicy one deserves emphasis, because it is a **trap with a plausible-looking false
positive**: k3s ships flannel by default, flannel does not implement NetworkPolicy, and the API
server will happily accept NetworkPolicy objects that then silently do nothing. `kubectl get
networkpolicy` returning rows is *not* evidence of enforcement.

---

## How to run it

```bash
./scripts/aria-cluster-baseline.sh > aria-cluster-baseline.txt 2>&1
```

Read-only — it deploys nothing and changes nothing. It needs a kubeconfig that can read
cluster-scoped resources. Then paste the output back into a session, or attach it to ARIA-79.

The script covers every acceptance criterion on the story:

1. Node inventory — count, k3s version, labels, taints, which node carries the GPU
2. StorageClasses — provisioner, default flag, reclaim policy, expansion support, plus what other
   apps' PVCs actually use in practice
3. GPU runtime — derived from **how Ollama already runs**, which is the story's stated method:
   the resource name a pod requests, the taint to tolerate, the nodeSelector, the runtimeClassName
4. CNI and NetworkPolicy — with the flannel warning above
5. Container registry — which registries are in use, whether imagePullSecrets are needed, and the
   k3s `registries.yaml` rewrite config
6. Reused endpoints — the in-cluster DNS names of the existing Keycloak and Ollama, recorded
   without touching either
7. Namespace state — whether `aria` exists, whether sealed-secrets is already installed (D31)

### Three things kubectl cannot tell you

The script prints these as a MANUAL section at the end:

**(a) GPU VRAM headroom** — `nvidia-smi` and `ollama ps` on the GPU node. This is ARIA-27's
measurement, not ARIA-79's, but it is collected on the same visit. Record VRAM at rest **and**
mid-inference: `aria-speech`'s budget is what remains at the *worst* moment, not the average.

**(b) Ollama's current server configuration** — `OLLAMA_KEEP_ALIVE`, `OLLAMA_NUM_PARALLEL`,
`OLLAMA_MAX_LOADED_MODELS`, and where they are set. D28 permits ARIA to change these, which makes
them ARIA-managed state that currently has no home. `04-deployment.md` flags "where Ollama's
ARIA-managed configuration is recorded and applied" as open — this is the input to answering it.

**(c) NetworkPolicy enforcement, proven** — deploy two throwaway pods, apply a deny-all policy,
confirm the second can no longer reach the first. Anything less is inference.

---

## Recording the result

Findings go in a durable location the other Deployment stories reference by link — that is this
library. Create `docs/deployment/cluster-baseline.md` from the script output, and:

- If node count > 1 **or** NetworkPolicy is unenforced → open a decision item to **reopen D6**,
  and note it in [`../open-questions/needs-decision.md`](../open-questions/needs-decision.md)
- Move the settled facts out of
  [`../open-questions/awaiting-measurement.md`](../open-questions/awaiting-measurement.md)
- Record the StorageClass and registry choices as D-entries — they are choices, not just findings

## What is needed to close this story

- [ ] Run the script; commit the output
- [ ] Prove NetworkPolicy enforcement rather than inferring it
- [ ] Name the StorageClass ARIA uses (or flag the gap as a decision)
- [ ] Establish whether a registry exists and whether an imagePullSecret is needed
- [ ] Record Keycloak's and Ollama's in-cluster endpoints
- [ ] **Check D6's two triggers explicitly and state in writing whether either fired**

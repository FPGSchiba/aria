# ARIA-79 — Cluster findings, 2026-08-19

[Library index](../README.md) · [Spikes](README.md) · [Deployment](../04-deployment.md) · [Decisions](../decisions/README.md)

**Four findings, discovered while setting up kubectl access. Three of them contradict
`04-deployment.md`, and one has fired a decision's revisit trigger.**

Recorded before the full baseline script has been run — these came out of the access work itself
and are too consequential to leave in a chat log. The remaining ARIA-79 criteria (StorageClasses,
registry, NetworkPolicy enforcement) are still outstanding.

---

## 1. It is not k3s — it is kubeadm

| | Documented | Actual |
|---|---|---|
| Distribution | "Jann's existing **k3s cluster**" | **kubeadm**, `/usr/bin/kubeadm`, `/etc/kubernetes/` |
| Version | unstated | **v1.33.0** |
| Runtime | unstated | **containerd 2.0.5** |
| OS | unstated | Ubuntu 24.04.1 LTS |

`02-stack.md`, `04-deployment.md` and several story descriptions say "k3s". **They are wrong.**

Consequences that are not merely cosmetic:

- **There is no `/etc/rancher/k3s/registries.yaml`** — the k3s registry-rewrite mechanism the
  registry investigation assumed does not exist here.
- **kubeadm ships no CNI**, so one was chosen deliberately. That matters for D6's *second*
  trigger (ARIA-117): unlike k3s's default flannel, a deliberately chosen CNI may well enforce
  NetworkPolicy. Still to be proven, not assumed.
- The API server runs as a **static pod** (`/etc/kubernetes/manifests/kube-apiserver.yaml`), so
  its flags are inspectable and editable in a way k3s's are not.

---

## 2. 🔴 Two nodes — **D6's first revisit trigger has fired**

```
kube-control     control-plane   192.168.1.20   v1.33.0   Ubuntu 24.04.1
kube-worker-01   <none>          192.168.1.21   v1.33.0   Ubuntu 24.04.1
```

D6 decided internal gRPC stays **plaintext in-cluster**, and stated the condition explicitly:

> **This is conditional:** if the cluster turns out to be multi-node, voice audio crosses the LAN
> in plaintext between nodes and this decision must be revisited rather than inherited.

**It is multi-node. The trigger has fired. D6 must be reopened**, and per the append-only rule it
is superseded by a new entry rather than edited.

What is now in scope for that reopening: Gateway↔Speech audio, and every other inter-service gRPC
call that could be scheduled across the two nodes. Options include mTLS between services, a
service mesh, pinning all ARIA workloads to one node (cheap but wastes the second), or accepting
the risk explicitly with the reasoning written down.

**Affects:** ARIA-14, ARIA-42, ARIA-91, ARIA-117, and every service page.

---

## 3. 🔴 The GPU is not in the cluster at all

Neither node is `192.168.1.15`. The GPU (RTX 2070) lives on Proxmox host `prox2`, reached by
Ollama in **LXC CT 200** through bind-mounted device nodes — not through Kubernetes.

`04-deployment.md` says:

> Speech is pinned (nodeSelector/toleration) to the GPU node, reusing whatever GPU runtime config
> already makes Ollama work there.

**There is no GPU node in this cluster.** As written, that plan is not achievable. Every option is
real work that no story currently covers:

| Option | Cost |
|---|---|
| Join the GPU host to the cluster as a node | New node, needs GPU passthrough to whatever runs kubelet, plus a device plugin |
| Expose the GPU to an existing node | Same passthrough problem, different machine |
| Run `aria-speech` outside Kubernetes on the GPU host | Breaks the "one umbrella Helm chart" packaging convention (D15) |
| Use a hosted STT/TTS service | Contradicts the local-by-default posture in §4 |

**This is a bigger rework than D6**, because D6 changes how services talk while this changes where
one of them can run. **Do not size ARIA-93 or ARIA-46 until it is decided.**

**Affects:** ARIA-27, ARIA-30, ARIA-33, ARIA-46, ARIA-93, and §4's scheduling paragraph.

---

## 4. The API server trusts the wrong CA — and ARIA inherits the underlying problem

The cluster's OIDC authentication **has never worked**. The API server logs, every 10 seconds:

```
oidc authenticator: initializing plugin:
  Get "https://keycloak.fpg/realms/master/.well-known/openid-configuration":
  tls: failed to verify certificate: x509: certificate signed by unknown authority
→ [invalid bearer token, oidc: authenticator not initialized]
```

The configuration is correct — `--oidc-issuer-url`, `--oidc-client-id=kubernetes`,
`--oidc-username-claim=preferred_username`, `--oidc-groups-claim=groups`, `--oidc-ca-file` are all
live on the running process, and the CA file exists.

### Root cause: the wrong CA was installed

> **Correction.** An earlier version of this document said Keycloak was serving a leaf without its
> intermediate. **That was wrong**, and is corrected here rather than quietly edited away —
> `Verify return code: 21` is consistent with both explanations, and I picked the wrong one before
> checking. The certificates settle it:

```
Keycloak leaf   subject = O = FPGArmy, CN = keycloak.fpg
                issuer  = C = CH, ST = Local, L = Home, O = FPG, CN = fpg-ca

keycloak-ca.crt subject = CN = My Internal CA      <-- a DIFFERENT CA
                issuer  = CN = My Internal CA      (self-signed)
                dates   = 2025-05-01 .. 2035-04-29
                contains exactly 1 certificate
```

`My Internal CA` is not `fpg-ca`. **There is no intermediate and no chain problem** — the leaf is
issued directly by the homelab root. The file the API server was told to trust is simply the wrong
certificate, apparently a leftover from May 2025.

**Fix:** replace `/etc/kubernetes/pki/keycloak-ca.crt` with the real `fpg-ca` root. The API server
retries plugin initialisation every 10 seconds, so it should recover without a restart.

### Why this is good news for ARIA

Because the leaf is issued **directly by the root**, distributing trust to ARIA's pods is the
simplest possible case: **one self-signed root certificate, no chain assembly, no ordering
concerns.** `fpg-ca` is the homelab's own root and presumably signs every internal service, so
solving it once serves everything ARIA will ever talk to internally.

That makes the open item concrete and small: get `fpg-ca` into ARIA's pods — as a ConfigMap mounted
by the umbrella chart (D15), baked into the base image, or via a cluster trust bundle. **Still
nothing in the backlog does it**, which is the actual gap.

**New open item.** How ARIA's pods obtain trust for `fpg-ca` — a ConfigMap mounted by the umbrella
chart, baked into the base image, or a cluster-wide trust bundle. Tracked in
[needs-decision.md](../open-questions/needs-decision.md).

---

## Still outstanding for ARIA-79

- [ ] StorageClasses — provisioner, default, expansion
- [ ] **Does the CNI enforce NetworkPolicy** (ARIA-117 — D6's *second* trigger). kubeadm required
      a CNI choice, so unlike k3s+flannel this may well be enforced. **Prove it**, don't infer it
- [x] ~~Container registry~~ — **decided: `ghcr.io`** ([D51](../decisions/0009-measurement-session.md)).
      Still to confirm from the baseline run: whether nodes already carry an `imagePullSecret`.
- [ ] In-cluster vs LAN endpoints for Keycloak and Ollama (both appear to be outside the cluster)
- [ ] Run [`../../scripts/aria-cluster-baseline.ps1`](../../scripts/aria-cluster-baseline.ps1)

*Findings 1–3 come from `kubectl get nodes -o wide` and `ls /etc/kubernetes/`; finding 4 from the
API server's own logs and `openssl s_client`. All are directly observed, not inferred.*

---

# Baseline run — results (same day, after the findings above)

The script ran. Five things it turned up, one of which invalidates a decision taken hours earlier.

## 5. ✅ D6's **second** trigger has NOT fired — the CNI is Antrea

```
CNI indicators: antrea
```

And NetworkPolicy is not theoretical here — **eight policies are already deployed** by real charts
(Keycloak, cert-manager, MetalLB, nginx-ingress). Antrea is a full NetworkPolicy implementation.

**This matters for how expensive reopening D6 is.** The trigger that fired was multi-node, not
unenforced-policy. So the reopening has cheaper options available than I assumed when the first
trigger fired:

- NetworkPolicy genuinely restricts pod-to-pod reachability, as D6's original reasoning required.
- **cert-manager is already running** (`kube-system`), so mTLS between services is far less work
  than standing up a CA — which was the main cost argument against it.

Still to prove empirically: two throwaway pods, deny-all, confirm the second cannot reach the
first. The prior is strong but "policies exist" is not "policies are enforced".

## 6. 🔴 Only **one** node is schedulable, and it is small

```
kube-control     4 CPU   ~3.8 GiB   taint: node-role.kubernetes.io/control-plane=:NoSchedule
kube-worker-01   4 CPU   ~7.75 GiB  no taints
```

Unless ARIA tolerates the control-plane taint, **every ARIA pod lands on `kube-worker-01`**.

Two consequences pulling in opposite directions:

- **Good for D6.** If everything is on one node, inter-service gRPC never crosses the LAN, and the
  plaintext decision survives *in practice* — though as a scheduling accident, not a guarantee. A
  reopened D6 could make that explicit (a nodeSelector) rather than rely on it.
- **Bad for capacity, and nobody has checked.** That single node has ~7.75 GiB and already runs
  Harbor, Vault, Longhorn, monitoring, nginx-ingress and MetalLB. ARIA adds six services plus
  **Postgres and Qdrant**. This is tight, and **no story sizes it.** New open item.

## 7. 🔴 The homelab is far better equipped than the backlog assumes

Three decisions were argued partly on "that would be a whole system to stand up". **All three are
already running:**

| Already deployed | Decision that assumed otherwise |
|---|---|
| **Harbor** (registry + Trivy scanning + its own Postgres/Redis) | **D51** — rejected an in-cluster registry as "a stateful workload with its own storage, TLS and backup story, standing between 'no code' and 'first image'". It is already standing |
| **Vault** (`vault` namespace, 10 GiB PVC) | **D31** — rejected External Secrets + Vault as "a whole secret-management system for a handful of credentials" |
| **cert-manager** | Assumed cost of mTLS when weighing D6's reopening |

**This is a pattern, not three coincidences: the backlog was built against a barer homelab than
actually exists.** D51 is the sharpest case — it was decided *today*, and its central rejection
argument is factually wrong. That does not automatically make `ghcr.io` the wrong choice; it makes
the *recorded reasoning* wrong, which by this project's own rules has to be corrected rather than
left standing.

## 8. Keycloak is in-cluster after all

```
keycloak.keycloak.svc.cluster.local   ClusterIP   80/TCP,443/TCP
ingress: keycloak.fpg -> 192.168.1.21 (MetalLB, nginx)
```

Correcting an earlier assumption in this document that Keycloak ran outside the cluster.

**Design consequence for ARIA-20:** the OIDC **issuer** is `https://keycloak.fpg`, and an issuer
claim must match exactly — so ARIA cannot simply substitute the in-cluster DNS name. ARIA's pods
will resolve `keycloak.fpg`, which means they need **both** DNS resolution for that name from
inside the cluster **and** the `fpg-ca` trust discussed above. Worth settling deliberately in
ARIA-20/ARIA-81 rather than discovering at runtime.

## 9. Storage: Longhorn, and a reclaim policy worth noticing

```
longhorn (default)   provisioner=driver.longhorn.io   expansion=True   reclaim=Delete
```

Longhorn is replicated block storage — appropriate for Postgres and Qdrant, and volume expansion
is supported, which removes the "size it right first time" pressure.

**But `reclaimPolicy: Delete`** means deleting a PVC destroys the volume. Under **D16**, Qdrant
holds **primary, non-rebuildable data**. A `Retain` policy (or a dedicated StorageClass) for the
stateful sets deserves a deliberate decision in ARIA-87/ARIA-89 rather than inheriting the default.

Also note: nothing currently pulls images from Harbor, and **no `imagePullSecret` exists anywhere**
in the cluster.

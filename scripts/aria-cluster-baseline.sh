#!/usr/bin/env bash
# ARIA-79 — k3s cluster baseline discovery.
#
# Read-only. Deploys nothing, changes nothing. Run against the existing homelab cluster
# with a kubeconfig that can read cluster-scoped resources.
#
#   ./aria-cluster-baseline.sh > aria-cluster-baseline.txt 2>&1
#
# Then paste the output back into the session, or attach it to ARIA-79.
#
# Answers every acceptance criterion on ARIA-79 except the two marked MANUAL at the end.
# Also answers the ARIA-117 question that D6 hangs on: does the CNI enforce NetworkPolicy?

set -uo pipefail

KUBECTL=${KUBECTL:-kubectl}
hr() { printf '\n\n═══ %s ═══\n\n' "$1"; }
try() { "$@" 2>&1 || echo "  (command failed — note this, it is itself a finding)"; }

echo "ARIA-79 cluster baseline — generated $(date -Is) on $(hostname)"
echo "kubectl: $($KUBECTL version --client -o json 2>/dev/null | grep -o '\"gitVersion\":[^,]*' | head -1)"

# ---------------------------------------------------------------------------------------
hr "1. NODES  (node count is load-bearing — D6 depends on it)"
try $KUBECTL get nodes -o wide
echo
echo "--- labels and taints per node ---"
try $KUBECTL get nodes -o custom-columns=\
'NAME:.metadata.name,VERSION:.status.nodeInfo.kubeletVersion,OS:.status.nodeInfo.osImage,RUNTIME:.status.nodeInfo.containerRuntimeVersion,TAINTS:.spec.taints[*].key'
echo
try $KUBECTL get nodes -o json | python3 -c '
import json,sys
d=json.load(sys.stdin)
for n in d["items"]:
    m=n["metadata"]; print("NODE:", m["name"])
    print("  labels:")
    for k,v in sorted(m.get("labels",{}).items()): print(f"    {k}={v}")
    print("  taints:", n["spec"].get("taints") or "none")
    cap=n["status"].get("capacity",{})
    gpu={k:v for k,v in cap.items() if "gpu" in k.lower() or "nvidia" in k.lower() or "amd" in k.lower()}
    print("  cpu/mem:", cap.get("cpu"), cap.get("memory"))
    print("  GPU capacity:", gpu or "NONE ADVERTISED")
    print()
' 2>/dev/null || echo "  (python3 unavailable — read the raw JSON instead)"

echo ">>> RECORD: node count = ?   If >1, D6 (plaintext in-cluster gRPC) MUST be reopened,"
echo ">>>         because voice audio would cross the LAN in plaintext between nodes."

# ---------------------------------------------------------------------------------------
hr "2. STORAGE CLASSES  (which one ARIA's PVCs use)"
try $KUBECTL get storageclass -o custom-columns=\
'NAME:.metadata.name,PROVISIONER:.provisioner,RECLAIM:.reclaimPolicy,BINDING:.volumeBindingMode,EXPANSION:.allowVolumeExpansion,DEFAULT:.metadata.annotations.storageclass\.kubernetes\.io/is-default-class'
echo
echo "--- existing PVCs (what other apps actually use in practice) ---"
try $KUBECTL get pvc -A -o custom-columns=\
'NS:.metadata.namespace,NAME:.metadata.name,SC:.spec.storageClassName,SIZE:.spec.resources.requests.storage,STATUS:.status.phase'
echo
echo ">>> RECORD: chosen StorageClass = ?  Does it support expansion? Is the provisioner"
echo ">>>         adequate for Postgres AND Qdrant, both of which hold PRIMARY data (D16)?"

# ---------------------------------------------------------------------------------------
hr "3. GPU RUNTIME  (derived from how Ollama already runs)"
echo "--- device-plugin / GPU-operator style workloads ---"
try $KUBECTL get pods -A -o wide | grep -Ei 'nvidia|gpu|rocm|amdgpu' || echo "  none matched"
echo
echo "--- RuntimeClasses (k3s + nvidia usually needs one) ---"
try $KUBECTL get runtimeclass
echo
echo "--- the Ollama workload itself: resource requests, nodeSelector, tolerations ---"
OLLAMA=$($KUBECTL get pods -A -o json 2>/dev/null | python3 -c '
import json,sys
d=json.load(sys.stdin)
for p in d["items"]:
    if "ollama" in p["metadata"]["name"].lower():
        print(p["metadata"]["namespace"], p["metadata"]["name"]); break
' 2>/dev/null)
if [ -n "${OLLAMA:-}" ]; then
  set -- $OLLAMA
  echo "  found: namespace=$1 pod=$2"
  try $KUBECTL get pod -n "$1" "$2" -o json | python3 -c '
import json,sys
p=json.load(sys.stdin); s=p["spec"]
print("  nodeName:      ", s.get("nodeName"))
print("  nodeSelector:  ", s.get("nodeSelector") or "none")
print("  tolerations:   ", s.get("tolerations") or "none")
print("  runtimeClass:  ", s.get("runtimeClassName") or "none")
for c in s["containers"]:
    print("  container:", c["name"], "resources =", c.get("resources"))
    print("     env:", [e["name"] for e in c.get("env",[])])
'
else
  echo "  No pod with 'ollama' in the name found. If Ollama runs OUTSIDE the cluster"
  echo "  (bare metal / docker on the GPU host), record that instead — it changes how"
  echo "  aria-speech shares the GPU, and means the VRAM budget (ARIA-27) is NOT"
  echo "  negotiated through Kubernetes resource requests."
fi
echo
echo ">>> RECORD: resource name a pod requests (e.g. nvidia.com/gpu), the taint to tolerate,"
echo ">>>         the nodeSelector, and the runtimeClassName — aria-speech copies all four."

# ---------------------------------------------------------------------------------------
hr "4. CNI + NETWORKPOLICY ENFORCEMENT  (ARIA-117 — this one can reopen D6)"
try $KUBECTL get pods -n kube-system -o wide
echo
echo "--- looking for a CNI ---"
try $KUBECTL get pods -A -o json | python3 -c '
import json,sys
d=json.load(sys.stdin)
hits=set()
for p in d["items"]:
    n=p["metadata"]["name"].lower()
    for cni in ("flannel","calico","cilium","canal","weave","kube-router","antrea"):
        if cni in n: hits.add(cni)
print("  CNI indicators found:", sorted(hits) or "NONE — likely stock k3s flannel")
' 2>/dev/null
echo
echo "--- existing NetworkPolicies (does anything already rely on them?) ---"
try $KUBECTL get networkpolicy -A
echo
cat <<'NOTE'
>>> CRITICAL: k3s ships flannel by default, and flannel DOES NOT enforce NetworkPolicy.
>>>           NetworkPolicy objects will be accepted by the API server and silently do
>>>           nothing. If that is the case here:
>>>
>>>             D6's justification for skipping internal TLS COLLAPSES and D6 must be
>>>             reopened — see docs/decisions/0001-identity-tokens-service-auth.md
>>>
>>>           Do not record "NetworkPolicy present" as "NetworkPolicy enforced". Prove it:
>>>           deploy two throwaway pods, apply a deny-all policy, and confirm the second
>>>           can no longer reach the first.
NOTE

# ---------------------------------------------------------------------------------------
hr "5. CONTAINER REGISTRY"
echo "--- image registries currently in use across the cluster ---"
try $KUBECTL get pods -A -o json | python3 -c '
import json,sys,collections
d=json.load(sys.stdin); c=collections.Counter()
for p in d["items"]:
    for ct in p["spec"].get("containers",[])+p["spec"].get("initContainers",[]):
        img=ct["image"]
        host=img.split("/")[0] if ("/" in img and ("." in img.split("/")[0] or ":" in img.split("/")[0])) else "docker.io (implicit)"
        c[host]+=1
for host,n in c.most_common(): print(f"  {n:4}  {host}")
' 2>/dev/null
echo
echo "--- imagePullSecrets in use ---"
try $KUBECTL get pods -A -o json | python3 -c '
import json,sys
d=json.load(sys.stdin); seen=set()
for p in d["items"]:
    for s in p["spec"].get("imagePullSecrets",[]) or []:
        seen.add((p["metadata"]["namespace"], s["name"]))
print("  ", sorted(seen) or "none")
' 2>/dev/null
echo
echo "--- k3s registry rewrite config (if present on this node) ---"
try cat /etc/rancher/k3s/registries.yaml
echo
echo ">>> RECORD: does a registry already exist (in-cluster / LAN / hosted)? Do nodes need"
echo ">>>         an imagePullSecret? D13 puts CI on GitHub Actions, so ghcr.io is the"
echo ">>>         path of least resistance unless something already exists."

# ---------------------------------------------------------------------------------------
hr "6. REUSED SERVICE ENDPOINTS  (record, do not change)"
echo "--- services that look like Keycloak or Ollama ---"
try $KUBECTL get svc -A -o wide | grep -Ei 'keycloak|ollama|auth|sso' || echo "  none matched by name"
echo
echo "--- ingresses (external names for the above) ---"
try $KUBECTL get ingress -A
echo
echo ">>> RECORD: in-cluster DNS name + port for Keycloak and for Ollama."
echo ">>>         Format: <svc>.<namespace>.svc.cluster.local:<port>"
echo ">>>         If either runs outside the cluster, record the LAN address instead."

# ---------------------------------------------------------------------------------------
hr "7. NAMESPACE + MISC"
try $KUBECTL get ns
echo
echo "--- does 'aria' already exist? ---"
try $KUBECTL get ns aria
echo
echo "--- is sealed-secrets already installed (D31)? ---"
try $KUBECTL get pods -A | grep -i sealed || echo "  not found — ARIA-112 will install it"
echo
echo "--- cert-manager / metrics-server / other helpers ---"
try $KUBECTL get pods -A | grep -Ei 'cert-manager|metrics-server|traefik' || echo "  none matched"

# ---------------------------------------------------------------------------------------
hr "MANUAL — not discoverable by kubectl"
cat <<'MANUAL'
  a) GPU VRAM headroom next to Ollama's resident model  (ARIA-27, separate spike)
     On the GPU node:   nvidia-smi          (or rocm-smi)
                        ollama ps           # what is currently resident
     Record: total VRAM, VRAM used by Ollama at rest, VRAM used mid-inference.
     aria-speech's budget is what is left at the WORST moment, not the average.

  b) Ollama's current server configuration  (D28 makes this ARIA-managed state)
     Record OLLAMA_KEEP_ALIVE, OLLAMA_NUM_PARALLEL, OLLAMA_MAX_LOADED_MODELS,
     and where those are set (systemd unit / compose file / k8s env).
     D28 permits ARIA to change these — so where they live needs an owner.

  c) Whether NetworkPolicy is ENFORCED (see section 4 — prove it, do not infer it)
MANUAL

hr "DONE"
echo "Paste this output back into the session, or attach to ARIA-79."

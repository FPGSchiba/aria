<#
    ARIA-79 - k3s cluster baseline discovery (PowerShell port of aria-cluster-baseline.sh).

    Read-only. Deploys nothing, changes nothing. Needs a kubectl that can read
    cluster-scoped resources.

        # capture EVERYTHING (Write-Host writes to the information stream, so a bare
        # pipe would give you an almost-empty file):
        .\aria-cluster-baseline.ps1 *>&1 | Tee-Object aria-cluster-baseline.txt

        # or, equivalently:
        Start-Transcript aria-cluster-baseline.txt; .\aria-cluster-baseline.ps1; Stop-Transcript

    Answers every ARIA-79 acceptance criterion except the MANUAL items at the end, which
    need a shell on the node. Also answers the ARIA-117 question D6 depends on.

    Section 1 and section 3 carry the two findings most likely to force rework.
#>

[CmdletBinding()]
param(
    [string]$OllamaHost = "192.168.1.15",   # to test whether k3s and the GPU box are the same machine
    [string]$Namespace  = "aria"
)

$ErrorActionPreference = "Continue"
$ProgressPreference    = "SilentlyContinue"

function Section($t) { Write-Host "`n`n=== $t ===`n" -ForegroundColor Cyan }
function Note($t)    { Write-Host $t -ForegroundColor Yellow }
function Warn($t)    { Write-Host $t -ForegroundColor Red }
function Good($t)    { Write-Host $t -ForegroundColor Green }

function KJson {
    param([string[]]$KArgs)
    $raw = & kubectl @KArgs 2>&1
    if ($LASTEXITCODE -ne 0) {
        Warn "  kubectl $($KArgs -join ' ') failed:"
        Write-Host "  $raw"
        return $null
    }
    try { return ($raw | Out-String | ConvertFrom-Json) }
    catch { Warn "  could not parse JSON from: kubectl $($KArgs -join ' ')"; return $null }
}

function KText {
    param([string[]]$KArgs)
    $raw = & kubectl @KArgs 2>&1
    if ($LASTEXITCODE -ne 0) { Warn "  (failed - note this, it is itself a finding)" }
    return $raw
}

function Props($obj) {
    if ($null -eq $obj) { return @() }
    return $obj.PSObject.Properties
}

if (-not (Get-Command kubectl -ErrorAction SilentlyContinue)) {
    Warn "kubectl not found on PATH. Nothing else will work."
    exit 1
}

Write-Host "ARIA-79 cluster baseline - generated $(Get-Date -Format s) on $env:COMPUTERNAME"
Write-Host "kubectl context: $((& kubectl config current-context 2>&1))"
Write-Host "PowerShell $($PSVersionTable.PSVersion)"

# `kubectl version` prints the CLIENT version even when the server is unreachable, which
# buries the actual error. Ask for something only the server can answer.
$probe = & kubectl get --raw=/version 2>&1
if ($LASTEXITCODE -ne 0) {
    Warn "kubectl cannot reach the cluster."
    Write-Host "  $probe"
    Write-Host ""
    # NOTE: single quotes throughout - these lines contain $( ) and $env: which must be
    # printed literally, never evaluated.
    Note '  401 / "must be logged in"  -> authentication. Either your OIDC login is not working,'
    Note '     or the API server cannot validate the token. Check the API server''s own view,'
    Note '     ON THE CONTROL PLANE NODE (not here):'
    Note '       sudo crictl logs $(sudo crictl ps --name kube-apiserver -q) 2>&1 | grep -i oidc | tail'
    Note '     A token that decodes correctly can still be rejected if the server''s OIDC plugin'
    Note '     failed to initialise - that is a server problem, not a kubeconfig problem.'
    Note ''
    Note '  403 / Forbidden           -> authentication worked, RBAC did not: kubectl auth can-i --list'
    Note ''
    Note '  Connection refused to [::1]:8080 -> no kubeconfig is loaded at all. kubectl fell back'
    Note '     to its default localhost address. Check: kubectl config get-contexts'
    Note ''
    Note '  To run with a different credential:'
    Note '       $env:KUBECONFIG = "$env:USERPROFILE\.kube\config.admin"'
    Note '       kubectl config get-contexts        # confirm a context is CURRENT'
    exit 1
}

# ---------------------------------------------------------------------------------------
Section "1. NODES  (node count is load-bearing - D6 depends on it)"
$nodes = KJson @("get","nodes","-o","json")
if ($nodes) {
    $count = @($nodes.items).Count
    Write-Host "  NODE COUNT: $count"
    foreach ($n in $nodes.items) {
        $st = $n.status
        $addr = ($st.addresses | Where-Object { $_.type -eq "InternalIP" }).address
        Write-Host ""
        Write-Host ("  NODE: {0}   ({1})" -f $n.metadata.name, ($addr -join ', '))
        Write-Host ("    kubelet   : {0}" -f $st.nodeInfo.kubeletVersion)
        Write-Host ("    OS        : {0}" -f $st.nodeInfo.osImage)
        Write-Host ("    runtime   : {0}" -f $st.nodeInfo.containerRuntimeVersion)
        Write-Host ("    cpu / mem : {0} / {1}" -f $st.capacity.cpu, $st.capacity.memory)

        $gpu = @()
        foreach ($p in (Props $st.capacity)) {
            if ($p.Name -match 'gpu|nvidia|amd') { $gpu += "$($p.Name)=$($p.Value)" }
        }
        if ($gpu.Count) { Good ("    GPU capacity: " + ($gpu -join ', ')) }
        else            { Warn  "    GPU capacity: NONE ADVERTISED" }

        Write-Host "    taints    : $(if ($n.spec.taints) { ($n.spec.taints | ForEach-Object { "$($_.key)=$($_.value):$($_.effect)" }) -join ', ' } else { 'none' })"
        Write-Host "    labels    :"
        foreach ($p in (Props $n.metadata.labels | Sort-Object Name)) {
            Write-Host ("      {0}={1}" -f $p.Name, $p.Value)
        }
    }

    Write-Host ""
    if ($count -gt 1) {
        Warn "  >>> MULTI-NODE ($count). D6 (plaintext in-cluster gRPC) MUST BE REOPENED."
        Warn "  >>> Voice audio would cross the LAN in plaintext between nodes, which D6"
        Warn "  >>> explicitly does not cover."
    } else {
        Good "  >>> Single node. D6's first trigger has NOT fired. Still check section 4."
    }

    # Is the k3s cluster the same machine as the Ollama/GPU host?
    $ips = @()
    foreach ($n in $nodes.items) {
        $ips += ($n.status.addresses | Where-Object { $_.type -eq "InternalIP" }).address
    }
    Write-Host ""
    if ($ips -contains $OllamaHost) {
        Good "  >>> A node has IP $OllamaHost - k3s and the Ollama/GPU host ARE the same machine."
        Note "  >>> Then section 3 decides whether k3s can actually SEE that GPU."
    } else {
        Warn "  >>> NO node has IP $OllamaHost (node IPs: $($ips -join ', '))."
        Warn "  >>> k3s and the GPU host look like DIFFERENT machines. If so, 04-deployment.md's"
        Warn "  >>> 'aria-speech is pinned to the GPU node' is not achievable as written, and"
        Warn "  >>> that is a larger rework than D6. Confirm before sizing ARIA-93."
    }
}

# ---------------------------------------------------------------------------------------
Section "2. STORAGE CLASSES"
$scs = KJson @("get","storageclass","-o","json")
if ($scs) {
    if (@($scs.items).Count -eq 0) { Warn "  NO StorageClasses. PVCs cannot bind." }
    foreach ($sc in $scs.items) {
        $isDefault = $sc.metadata.annotations."storageclass.kubernetes.io/is-default-class"
        Write-Host ("  {0,-24} provisioner={1}" -f $sc.metadata.name, $sc.provisioner)
        Write-Host ("      default={0}  reclaim={1}  binding={2}  expansion={3}" -f `
            $(if ($isDefault -eq "true") { "YES" } else { "no" }),
            $sc.reclaimPolicy, $sc.volumeBindingMode,
            $(if ($null -ne $sc.allowVolumeExpansion) { $sc.allowVolumeExpansion } else { "false" }))
    }
}
Write-Host ""
Write-Host "  --- existing PVCs (what other apps actually use) ---"
KText @("get","pvc","-A","-o","custom-columns=NS:.metadata.namespace,NAME:.metadata.name,SC:.spec.storageClassName,SIZE:.spec.resources.requests.storage,STATUS:.status.phase")
Note @"

  >>> RECORD: which StorageClass ARIA uses. Does it support expansion? Is the provisioner
  >>> adequate for Postgres AND Qdrant - both of which hold PRIMARY data under D16, so
  >>> Qdrant is NOT a rebuildable index and its durability bar is the same as Postgres'.
"@

# ---------------------------------------------------------------------------------------
Section "3. GPU RUNTIME  (does Kubernetes see the GPU at all?)"
Note "  Context from 2026-08-19: Ollama reaches the GPU through an LXC bind mount on"
Note "  Proxmox, NOT through Kubernetes. So a node advertising no GPU resource is the"
Note "  expected-but-consequential outcome here, not a mistake."
Write-Host ""
Write-Host "  --- device-plugin / GPU-operator style workloads ---"
$pods = KJson @("get","pods","-A","-o","json")
if ($pods) {
    $gpuPods = $pods.items | Where-Object { $_.metadata.name -match 'nvidia|gpu|rocm|amdgpu' }
    if ($gpuPods) {
        foreach ($p in $gpuPods) { Write-Host ("    {0}/{1}  [{2}]" -f $p.metadata.namespace, $p.metadata.name, $p.status.phase) }
    } else {
        Warn "    none - no NVIDIA device plugin or GPU operator is running."
    }
}
Write-Host ""
Write-Host "  --- RuntimeClasses (k3s + nvidia usually needs one) ---"
KText @("get","runtimeclass")

Write-Host ""
Write-Host "  --- any pod named like ollama? ---"
if ($pods) {
    $oll = $pods.items | Where-Object { $_.metadata.name -match 'ollama' } | Select-Object -First 1
    if ($oll) {
        Good ("    found: {0}/{1} on node {2}" -f $oll.metadata.namespace, $oll.metadata.name, $oll.spec.nodeName)
        Write-Host ("      nodeSelector : {0}" -f (($(Props $oll.spec.nodeSelector) | ForEach-Object { "$($_.Name)=$($_.Value)" }) -join ', '))
        Write-Host ("      runtimeClass : {0}" -f $oll.spec.runtimeClassName)
        Write-Host ("      tolerations  : {0}" -f (($oll.spec.tolerations | ForEach-Object { $_.key }) -join ', '))
        foreach ($c in $oll.spec.containers) {
            Write-Host ("      container {0}: requests={1} limits={2}" -f $c.name,
                (($(Props $c.resources.requests) | ForEach-Object { "$($_.Name)=$($_.Value)" }) -join ' '),
                (($(Props $c.resources.limits)   | ForEach-Object { "$($_.Name)=$($_.Value)" }) -join ' '))
        }
    } else {
        Warn "    No pod named like 'ollama' in this cluster."
        Note "    Consistent with Ollama running OUTSIDE Kubernetes (LXC CT 200 on prox2)."
        Note "    Consequence: the VRAM budget for aria-speech is NOT negotiated through"
        Note "    Kubernetes resource requests. It is a host-level agreement between two"
        Note "    things Kubernetes cannot see. Record that on ARIA-27 and ARIA-93."
    }
}
Note @"

  >>> RECORD: the resource name a pod requests (e.g. nvidia.com/gpu), the taint to
  >>> tolerate, the nodeSelector, and the runtimeClassName - aria-speech copies all four.
  >>> If NOTHING advertises a GPU resource, say so explicitly: it means ARIA-93 cannot
  >>> schedule aria-speech onto the GPU until k8s-level GPU access is set up, which is
  >>> work no story currently covers.
"@

# ---------------------------------------------------------------------------------------
Section "4. CNI + NETWORKPOLICY  (ARIA-117 - this can reopen D6)"
$sys = KJson @("get","pods","-n","kube-system","-o","json")
if ($sys) {
    $hits = @()
    foreach ($p in $sys.items) {
        foreach ($cni in @('flannel','calico','cilium','canal','weave','kube-router','antrea')) {
            if ($p.metadata.name -match $cni) { $hits += $cni }
        }
    }
    $hits = $hits | Sort-Object -Unique
    if ($hits.Count) { Write-Host ("  CNI indicators: " + ($hits -join ', ')) }
    else             { Warn "  CNI indicators: NONE FOUND - likely stock k3s flannel" }
    if ($hits -contains 'flannel' -and $hits.Count -eq 1) {
        Warn "  >>> FLANNEL ONLY. Flannel does NOT enforce NetworkPolicy."
    }
}
Write-Host ""
Write-Host "  --- existing NetworkPolicies ---"
KText @("get","networkpolicy","-A")
if ($hits -and ($hits -notcontains 'flannel')) {
    Good "  >>> CNI is $($hits -join ', ') - a full NetworkPolicy implementation. D6's SECOND"
    Good "  >>> trigger has probably NOT fired. Still prove it (see below)."
}
Note @"

  >>> If the CNI is flannel (k3s default), it DOES NOT enforce NetworkPolicy.
  >>> The API server accepts NetworkPolicy objects and they silently do nothing.
  >>>
  >>>   If nothing enforces them, D6's justification for skipping internal TLS COLLAPSES
  >>>   and D6 must be reopened - see docs/decisions/0001-identity-tokens-service-auth.md
  >>>
  >>> Do NOT record 'NetworkPolicy objects exist' as 'NetworkPolicy is enforced'. Prove it:
  >>> deploy two throwaway pods, apply a deny-all policy, confirm the second can no longer
  >>> reach the first. Anything less is inference.
"@

# ---------------------------------------------------------------------------------------
Section "5. CONTAINER REGISTRY"
if ($pods) {
    $tally = @{}
    foreach ($p in $pods.items) {
        $ctrs = @($p.spec.containers) + @($p.spec.initContainers)
        foreach ($c in $ctrs) {
            if (-not $c) { continue }
            $img = $c.image
            $first = $img.Split('/')[0]
            $host2 = if ($img.Contains('/') -and ($first.Contains('.') -or $first.Contains(':'))) { $first } else { 'docker.io (implicit)' }
            if ($tally.ContainsKey($host2)) { $tally[$host2]++ } else { $tally[$host2] = 1 }
        }
    }
    Write-Host "  --- image registries in use ---"
    foreach ($k in ($tally.Keys | Sort-Object { -$tally[$_] })) {
        Write-Host ("    {0,4}  {1}" -f $tally[$k], $k)
    }
    Write-Host ""
    Write-Host "  --- imagePullSecrets in use ---"
    $secrets = @()
    foreach ($p in $pods.items) {
        foreach ($s in @($p.spec.imagePullSecrets)) {
            if ($s) { $secrets += "$($p.metadata.namespace)/$($s.name)" }
        }
    }
    if ($secrets) { ($secrets | Sort-Object -Unique) | ForEach-Object { Write-Host "    $_" } }
    else          { Write-Host "    none" }
}
Note @"

  >>> RECORD: does a registry already exist (in-cluster / LAN / hosted)? Do nodes need an
  >>> imagePullSecret? D13 puts CI on GitHub Actions, so ghcr.io is the path of least
  >>> resistance unless something already exists.
"@

# ---------------------------------------------------------------------------------------
Section "6. REUSED SERVICE ENDPOINTS  (record, do not change)"
Write-Host "  --- services matching keycloak / ollama / auth / sso ---"
$svcs = KJson @("get","svc","-A","-o","json")
if ($svcs) {
    $m = $svcs.items | Where-Object { $_.metadata.name -match 'keycloak|ollama|auth|sso' }
    if ($m) {
        foreach ($s in $m) {
            $ports = ($s.spec.ports | ForEach-Object { "$($_.port)/$($_.protocol)" }) -join ','
            Write-Host ("    {0}.{1}.svc.cluster.local  type={2}  ports={3}" -f `
                $s.metadata.name, $s.metadata.namespace, $s.spec.type, $ports)
        }
    } else {
        Warn "    none matched by name."
        Note "    Expected if Keycloak and Ollama run OUTSIDE the cluster - record their LAN"
        Note "    addresses instead (Keycloak: https://keycloak.fpg, Ollama: http://$OllamaHost:11434)."
    }
}
Write-Host ""
Write-Host "  --- ingresses ---"
KText @("get","ingress","-A")

# ---------------------------------------------------------------------------------------
Section "7. NAMESPACE + PREREQUISITES"
KText @("get","ns")
Write-Host ""
Write-Host "  --- does the '$Namespace' namespace exist? ---"
$ns = & kubectl get ns $Namespace 2>&1
if ($LASTEXITCODE -eq 0) { Good "    yes"; Write-Host "    $ns" } else { Write-Host "    no - ARIA-81 creates it" }
Write-Host ""
Write-Host "  --- sealed-secrets controller (D31 / ARIA-112)? ---"
if ($pods) {
    $sealed = $pods.items | Where-Object { $_.metadata.name -match 'sealed' }
    if ($sealed) { foreach ($p in $sealed) { Good ("    {0}/{1}" -f $p.metadata.namespace, $p.metadata.name) } }
    else { Write-Host "    not found - ARIA-112 will install it" }
    Write-Host ""
    Write-Host "  --- other helpers ---"
    $helpers = $pods.items | Where-Object { $_.metadata.name -match 'cert-manager|metrics-server|traefik' }
    if ($helpers) { foreach ($p in $helpers) { Write-Host ("    {0}/{1}" -f $p.metadata.namespace, $p.metadata.name) } }
    else { Write-Host "    none matched" }
}

# ---------------------------------------------------------------------------------------
Section "MANUAL - not discoverable via kubectl"
Note @"
  Run these where they apply, and paste the output back.

  a) GPU VRAM headroom next to Ollama's resident model  (ARIA-27)
     On the GPU host (prox2) or inside CT 200:
        nvidia-smi
        ollama ps
     Record VRAM total, used at rest, AND used mid-inference. aria-speech's budget is
     what is left at the WORST moment, not the average.

  b) Ollama's server configuration  (D28 makes this ARIA-managed state)
     OLLAMA_KEEP_ALIVE, OLLAMA_NUM_PARALLEL, OLLAMA_MAX_LOADED_MODELS, OLLAMA_CONTEXT_LENGTH
     and WHERE they are set. Already partly captured in docs/spikes/ARIA-27-gpu-baseline.md.

  c) Whether NetworkPolicy is ENFORCED - prove it, do not infer it (section 4).

  d) On the k3s node, the API server's OIDC config, for the ARIA-19 record:
        sudo ps -ef | tr ' ' '\n' | grep -i oidc | sort -u

  e) THE REBOOT TEST, still outstanding: reboot prox2, then BEFORE running anything
     NVIDIA-related:
        ls -l /dev/nvidia*
     They must already be character devices. Running nvidia-smi CREATES the nodes it
     checks for, so using it to verify proves nothing.
"@

Section "DONE"
Write-Host "Paste this output back into the session, or attach it to ARIA-79."
Write-Host "The two findings most likely to force rework are in sections 1 and 3."

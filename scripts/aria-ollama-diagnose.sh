#!/usr/bin/env bash
# ARIA — Ollama / GPU diagnosis.
#
# Written for: "requests time out, preflight succeeds, Ollama runs in a container on
# Proxmox, the host was recently power-cycled."
#
# Run this ON THE MACHINE OR CONTAINER WHERE OLLAMA RUNS.
# Read-only apart from one small generation request per model.
#
#   ./aria-ollama-diagnose.sh > ollama-diag.txt 2>&1
#
# Section 2 is the decisive one. Everything else explains it.

set -uo pipefail
OLLAMA_URL=${OLLAMA_URL:-http://localhost:11434}
hr() { printf '\n\n═══ %s ═══\n\n' "$1"; }
have() { command -v "$1" >/dev/null 2>&1; }

echo "ARIA Ollama diagnosis — $(date -Is)"
echo "host: $(hostname)   target: $OLLAMA_URL"

# ---------------------------------------------------------------------------------------
hr "1. WHERE AM I? (container vs VM vs bare metal)"
if [ -f /run/systemd/container ]; then echo "  systemd container type: $(cat /run/systemd/container)"; fi
if grep -qa 'lxc\|docker' /proc/1/cgroup 2>/dev/null; then echo "  cgroup says: containerised"; fi
if have systemd-detect-virt; then echo "  systemd-detect-virt: $(systemd-detect-virt || echo none)"; fi
echo "  kernel: $(uname -r)"
echo
echo "  >>> LXC and VM fail differently:"
echo "      LXC  — GPU is bind-mounted; /dev/nvidia* must exist AND the driver version"
echo "             inside must match the Proxmox host's exactly."
echo "      VM   — GPU is vfio-passed; the guest loads its own driver. A host reboot can"
echo "             leave the card bound to the host driver instead of vfio-pci."

# ---------------------------------------------------------------------------------------
hr "2. ★ IS OLLAMA USING THE GPU? — the decisive check"
if have ollama; then
  echo "--- ollama ps (look at the PROCESSOR column) ---"
  ollama ps 2>&1
else
  echo "--- /api/ps ---"
  curl -s "$OLLAMA_URL/api/ps" 2>&1 | head -40
fi
cat <<'NOTE'

  >>> READ THE PROCESSOR COLUMN:
  >>>   "100% GPU"  → GPU is fine. Your timeout is NOT passthrough. Go to section 6.
  >>>   "100% CPU"  → PASSTHROUGH IS BROKEN. This is your timeout. Go to sections 3-5.
  >>>   "48%/52% CPU/GPU" → model does not fit in available VRAM; partial offload.
  >>>                       Still slow. Check section 4 for what else holds VRAM.
  >>>   (empty)     → no model resident right now; run section 6 first, then re-check.
NOTE

# ---------------------------------------------------------------------------------------
hr "3. DOES THE CONTAINER SEE THE GPU AT ALL?"
echo "--- /dev/nvidia* device nodes ---"
ls -l /dev/nvidia* 2>&1 || echo "  NONE — the container has no GPU device nodes."
echo
echo "--- /dev/dri (needed for some setups) ---"
ls -l /dev/dri 2>&1 || echo "  none"
echo
if have nvidia-smi; then
  echo "--- nvidia-smi ---"
  nvidia-smi 2>&1
  echo
  echo "--- driver / CUDA version ---"
  nvidia-smi --query-gpu=name,driver_version,memory.total,memory.used,memory.free --format=csv 2>&1
else
  echo "  nvidia-smi NOT INSTALLED in this container."
  echo "  For LXC this is normal-ish, but then Ollama also cannot see the GPU unless the"
  echo "  userspace libs are bind-mounted. For a VM, install the guest driver."
fi
cat <<'NOTE'

  >>> If /dev/nvidia* is missing inside an LXC container, the passthrough is not wired up.
  >>> On the PROXMOX HOST, the container config needs (Proxmox 8, cgroup2):
  >>>
  >>>   lxc.cgroup2.devices.allow: c 195:* rwm
  >>>   lxc.cgroup2.devices.allow: c 507:* rwm      # check your actual major numbers
  >>>   lxc.mount.entry: /dev/nvidia0 dev/nvidia0 none bind,optional,create=file
  >>>   lxc.mount.entry: /dev/nvidiactl dev/nvidiactl none bind,optional,create=file
  >>>   lxc.mount.entry: /dev/nvidia-uvm dev/nvidia-uvm none bind,optional,create=file
  >>>   lxc.mount.entry: /dev/nvidia-uvm-tools dev/nvidia-uvm-tools none bind,optional,create=file
  >>>
  >>> Get YOUR major numbers on the host with:  ls -l /dev/nvidia*
  >>> They can change across a host reboot if the driver loads in a different order —
  >>> which is exactly the failure mode a power-cycle produces.
NOTE

# ---------------------------------------------------------------------------------------
hr "4. WHAT ELSE IS HOLDING VRAM? (this is ARIA-27's measurement)"
if have nvidia-smi; then
  nvidia-smi --query-compute-apps=pid,process_name,used_memory --format=csv 2>&1
  echo
  echo "--- total vs free ---"
  nvidia-smi --query-gpu=memory.total,memory.used,memory.free --format=csv 2>&1
else
  echo "  (no nvidia-smi here — run this section on the Proxmox host instead)"
fi
echo
echo "  >>> Record these numbers even if the GPU turns out fine. ARIA-27 needs VRAM at rest"
echo "  >>> AND mid-inference; aria-speech's budget is what's left at the WORST moment."

# ---------------------------------------------------------------------------------------
hr "5. OLLAMA'S OWN VIEW"
echo "--- version ---"
curl -s "$OLLAMA_URL/api/version" 2>&1; echo
echo
echo "--- relevant environment (D28 makes these ARIA-managed state) ---"
for v in OLLAMA_HOST OLLAMA_KEEP_ALIVE OLLAMA_NUM_PARALLEL OLLAMA_MAX_LOADED_MODELS \
         OLLAMA_GPU_OVERHEAD OLLAMA_FLASH_ATTENTION CUDA_VISIBLE_DEVICES; do
  echo "  $v=${!v-<unset>}"
done
echo
echo "--- what the service actually runs with ---"
if have systemctl; then
  systemctl show ollama -p Environment 2>/dev/null || echo "  (no systemd unit named ollama)"
  echo
  echo "--- last 40 log lines: look for 'no compatible GPUs', 'library=cpu', CUDA errors ---"
  journalctl -u ollama -n 40 --no-pager 2>&1 | tail -40 || echo "  (no journal access)"
else
  echo "  (no systemctl — if Ollama is in Docker: docker logs <container> | tail -40)"
fi
cat <<'NOTE'

  >>> The log line that settles it looks like one of:
  >>>   "inference compute ... library=cuda"   → GPU in use
  >>>   "no compatible GPUs were discovered"   → passthrough broken, running on CPU
NOTE

# ---------------------------------------------------------------------------------------
hr "6. TIMED GENERATION — how slow is it actually?"
MODEL=${MODEL:-qwen3:8b}
echo "Using model: $MODEL   (override with MODEL=... $0)"
echo
echo "--- 6a. tiny request, thinking DISABLED, native /api/chat ---"
time curl -s "$OLLAMA_URL/api/chat" -d "{
  \"model\": \"$MODEL\",
  \"messages\": [{\"role\":\"user\",\"content\":\"Say OK and nothing else.\"}],
  \"think\": false,
  \"stream\": false,
  \"options\": {\"num_predict\": 16}
}" 2>&1 | head -c 600
echo
echo
echo "--- 6b. same request, thinking left at default ---"
time curl -s "$OLLAMA_URL/api/chat" -d "{
  \"model\": \"$MODEL\",
  \"messages\": [{\"role\":\"user\",\"content\":\"Say OK and nothing else.\"}],
  \"stream\": false,
  \"options\": {\"num_predict\": 512}
}" 2>&1 | head -c 900
echo
cat <<'NOTE'

  >>> COMPARE 6a AND 6b:
  >>>   6a fast, 6b slow          → THINKING is your timeout, not the GPU.
  >>>                                Qwen3 reasons before answering; with 5 tool schemas
  >>>                                in the prompt that easily exceeds 120 s.
  >>>   both slow (>20 s for 16 tokens) → CPU inference. Passthrough. Sections 3-5.
  >>>   both fast                 → neither; the tool schemas are the trigger. Run 6c.
NOTE
echo
echo "--- 6c. with tool schemas, via the OpenAI-compatible endpoint the probe uses ---"
time curl -s "$OLLAMA_URL/v1/chat/completions" -d "{
  \"model\": \"$MODEL\",
  \"messages\": [{\"role\":\"user\",\"content\":\"Play some jazz in the kitchen.\"}],
  \"max_tokens\": 256,
  \"tools\": [{\"type\":\"function\",\"function\":{
      \"name\":\"music_play\",
      \"description\":\"Play music on a speaker in the house.\",
      \"parameters\":{\"type\":\"object\",
        \"properties\":{\"query\":{\"type\":\"string\"},\"room\":{\"type\":\"string\"}},
        \"required\":[\"query\",\"room\"]}}}]
}" 2>&1 | head -c 900
echo

# ---------------------------------------------------------------------------------------
hr "RUN THESE ON THE PROXMOX HOST (not here)"
cat <<'HOST'
  # Which driver currently owns the card? This is what a reboot changes.
  lspci -nnk | grep -A3 -i -E 'nvidia|vga'
  #   "Kernel driver in use: vfio-pci"  → bound for VM passthrough
  #   "Kernel driver in use: nvidia"    → bound to the host (correct for LXC, WRONG for VM)
  #   "Kernel driver in use: nouveau"   → open driver grabbed it; blacklist nouveau

  # Did IOMMU come up?
  dmesg | grep -i -e DMAR -e IOMMU | head
  # Passthrough errors since boot?
  dmesg | grep -i -e vfio -e nvidia | tail -30

  # Host-side device nodes and driver version (must MATCH inside an LXC container)
  ls -l /dev/nvidia*
  nvidia-smi

  # If Ollama is in an LXC container, dump its config and check the mount entries:
  pct config <CTID>
  # If it is a VM:
  qm config <VMID> | grep -i hostpci
HOST

hr "DONE"
echo "Paste this output back into the session."
echo "Whatever the cause, record the outcome on ARIA-27 (VRAM) and ARIA-79 (GPU runtime)."

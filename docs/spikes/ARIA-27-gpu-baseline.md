# ARIA-27 / ARIA-79 — GPU node baseline and the passthrough incident

[Library index](../README.md) · [Spikes](README.md) · [Speech](../services/speech.md) · [Deployment](../04-deployment.md)

**Status: hardware facts recorded 2026-08-19. VRAM budget is preliminary — needs re-measurement
under real load.**

Two things landed here at once: the GPU node's actual specification, and an outage that revealed a
deployment fragility `04-deployment.md` does not currently account for.

---

## 1. The hardware

| Fact | Value |
|---|---|
| Host | Proxmox `prox2` |
| GPU | **NVIDIA GeForce RTX 2070, 8 GB** (TU106, `10de:1f02`, compute capability 7.5) |
| Driver / CUDA | 580.173.02 / CUDA 13.0 (`.run` installer, **not** Debian packages) |
| Ollama location | **LXC container CT 200** (`llm-inference`), not a VM |
| Passthrough style | bind-mounted device nodes + `lxc.cgroup2.devices.allow` |
| Ollama endpoint | `http://192.168.1.15:11434` |

**8 GB is the binding constraint on the entire Speech epic.** Not 24, not 16 — eight, shared with
whatever Ollama holds.

### Ollama's current configuration (D28 makes this ARIA-managed state)

| Setting | Value | Why it matters to ARIA |
|---|---|---|
| `OLLAMA_KEEP_ALIVE` | `2562047h47m…` — **max int64, i.e. never unload** | VRAM is held permanently. This is the single biggest lever on Speech's budget |
| `OLLAMA_MAX_LOADED_MODELS` | `1` | Model swaps cost a full reload — measured at **13–14 s** |
| `OLLAMA_NUM_PARALLEL` | `1` | No concurrent inference |
| `OLLAMA_KV_CACHE_TYPE` | `q8_0` | KV cache already quantised — less headroom to reclaim than it looks |
| `OLLAMA_CONTEXT_LENGTH` | `0` (default, 4096) | **See the warning below** |
| `OLLAMA_FLASH_ATTENTION` | `true` | Already on |

---

## 2. VRAM budget — preliminary

From Ollama's own memory breakdown loading `qwen3:8b` (recorded while the GPU was working):

```
CUDA0 (RTX 2070) | 7782 MiB total | 7683 free | 5049 used = 4643 weights + 306 KV + 100 compute
```

**≈ 2.6 GB left for `aria-speech`** with `qwen3:8b` resident.

That is workable for a small STT model plus a compact TTS voice, and it is the number ARIA-30 and
ARIA-33 must size against. But two caveats change it, and both push the wrong way:

> ### ⚠️ The 2.6 GB is measured at a 4096-token context
>
> `OLLAMA_CONTEXT_LENGTH=0` means the default. ARIA's Agent Core prompt is system prompt +
> retrieved Knowledge Core context + the **full MCP tool catalogue** + conversation history — it
> will want considerably more than 4096. Raising context grows the KV cache and **shrinks Speech's
> budget further**. The real contention is worse than the headline figure.

> ### ⚠️ Freeing VRAM by lowering `keep_alive` has a hidden dependency
>
> The obvious way to give Speech room is to let Ollama unload when idle. **That is exactly what
> would have re-triggered the outage below** — with persistence mode off, the driver can unload
> when no process holds the GPU, taking the device nodes with it. Ollama's infinite `keep_alive`
> was masking that. Persistence mode is now on, which is what makes lowering `keep_alive` safe.
>
> Recorded because the two settings look independent and are not.

**Still needed for a real ARIA-27 answer:** VRAM at rest *and* mid-inference, at the context length
ARIA will actually use. `aria-speech`'s budget is what remains at the **worst** moment.

---

## 3. The passthrough incident — a deployment finding

### What happened

After a Proxmox host power-cycle, Ollama ran **100% on CPU** while appearing entirely healthy. The
API answered, models listed, `/api/version` responded. Only generation was ~50× too slow.

```
Jul 31   load_tensors: offloaded 37/37 layers to GPU        ← before
Aug 19   inference compute id=cpu library=cpu               ← after
```

### Root cause

The NVIDIA driver creates `/dev/nvidia*` **lazily, on first use**. Nothing touched the GPU after
the reboot, so at CT 200's start the nodes did not exist on the host. The container config uses:

```
lxc.mount.entry: /dev/nvidia0 dev/nvidia0 none bind,optional,create=file
```

`create=file` made an **empty regular file**; `optional` suppressed any error. Inside the
container:

```
----------  1 root root        0  /dev/nvidia0      ← regular file, not a device
crw-rw-rw-  1 root root 195, 255  /dev/nvidiactl    ← the one that happened to exist in time
```

The `cgroup2` allow rules were correct throughout — 195:0, 195:255, 510:0/1, 235:1/2 all matched
the host's majors. **The device-major-renumbering theory was wrong**; the cause was ordering.

### The fix

A systemd unit that creates the nodes **before Proxmox starts guests**, which is the actual
requirement:

```ini
# /etc/systemd/system/nvidia-persistenced.service
[Unit]
After=systemd-modules-load.service
Before=pve-guests.service
[Service]
Type=forking
ExecStartPre=/usr/bin/nvidia-modprobe -c 0 -u
ExecStart=/usr/bin/nvidia-persistenced --user nvidia-persistenced
```

Plus `/etc/modules-load.d/nvidia.conf` and a `nvidia-persistenced` service user. Persistence mode
now reads **On**.

### Why this belongs in the ARIA record

`04-deployment.md` says Speech is "pinned (nodeSelector/toleration) to the GPU node, reusing
whatever GPU runtime config already makes Ollama work there." This incident shows that
configuration **did not survive a reboot**, and failed *silently* — which is the dangerous part.

Consequences to carry into the deployment design:

1. **A silent CPU fallback is worse than a crash.** A pod that starts, passes its health check and
   runs 50× too slow will not page anyone. `aria-speech` should **assert GPU availability at
   startup and refuse to become ready without it**, rather than quietly degrading. That is a
   concrete requirement for ARIA-42/ARIA-93.
2. **Consider removing `optional`** from the LXC mount entries so the container fails loudly. The
   trade is that a GPU fault becomes a container-won't-start fault — for a host named
   `llm-inference`, arguably correct.
3. **The alerting gap is now concrete.** `open-questions/deferred.md` C-5 defers alerting because
   "nothing to alert on until services run." This incident is a counter-example that already
   happened: a silent capability loss with no signal. Worth revisiting when Speech is deployed.
4. **Verification must not use `nvidia-smi`.** Running it *creates* the nodes it checks for. The
   only valid test is `ls -l /dev/nvidia*` after a reboot, before touching anything NVIDIA.

### Still outstanding

- [ ] **Reboot test not yet performed.** Everything above proves it works now, not that it
      survives a restart. Until that test runs, this is unverified.
- [ ] Confirm `/etc/modules-load.d/nvidia.conf` exists (the unit's `After=` assumes it)

---

## What is needed to close ARIA-27

- [x] GPU model, driver, runtime style recorded
- [x] Ollama's configuration recorded as ARIA-managed state (D28)
- [ ] VRAM at rest and mid-inference **at ARIA's real context length**
- [ ] Then decide resident vs. load-on-demand (a function of that number)
- [ ] Publish the `aria-speech` memory budget for ARIA-30 / ARIA-33

## What is needed to close ARIA-79

The cluster baseline is a separate question from this GPU node — node count, StorageClasses,
NetworkPolicy enforcement and the registry are still unrecorded. See
[the ARIA-79 brief](ARIA-79-cluster-baseline.md) and run
[`../../scripts/aria-cluster-baseline.sh`](../../scripts/aria-cluster-baseline.sh).

**D6's two triggers remain unchecked.**

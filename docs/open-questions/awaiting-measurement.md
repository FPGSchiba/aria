# Open — awaiting a measurement

[Library index](../README.md) · [Decisions](../decisions/README.md) · [Open questions](README.md) · [Sprint 1](../sprints/sprint-01.md)

**Do not settle these by discussion.** Each needs a number, a benchmark, or a look at the actual
hardware. Answering them from a conversation would manufacture the certainty this project has
deliberately refused to fake. Every item names the spike that owns it.

**Source:** CLAUDE.md §8, third group, as of 2026-08-12.

---

- Pick the specific hosted LLM provider/model for the Agent Core's default backend.
- Confirm the Rust MCP SDK to standardize on (`rmcp` or equivalent).
- Confirm the STT library/model and the TTS library/voice that fit the VRAM budget, and the GPU
  access approach.
- Measure GPU VRAM headroom next to Ollama and publish the `aria-speech` memory budget; then
  decide resident vs. load-on-demand model residency.
- ~~Confirm the cluster baseline — node count (which the internal-TLS decision explicitly depends
  on), storage classes, GPU runtime, and whether a container registry already exists.~~
  **ANSWERED 2026-08-19** by ARIA-79. *(The source text said "k3s"; the cluster is **kubeadm
  v1.33.0**. Corrected here because it is a factual error, not a decision.)* See the
  [cluster findings](../spikes/ARIA-79-cluster-findings-2026-08-19.md).
- Which facts get embedded into Qdrant, and the classification rule separating typed from
  free-text preferences.
- Where sandboxed `run_tests` execution physically runs for a candidate self-generated service —
  a disposable namespace/pod on the existing cluster is the natural default. *(The baseline is now
  known: one schedulable node, 4 vCPU / 7.75 GiB. That does not answer this item, but it makes the
  "natural default" a claim that needs checking rather than assuming.)*
- The exact Keycloak API that exposes a user's granted consents.

---

## See also

- [Spike briefs](../spikes/) — research done so far against these
- [Sprint 1](../sprints/sprint-01.md) — which of these sprint 1 must close

## Newly surfaced 2026-08-19 (measurement session)

- **Does Qwen3's thinking mode change tool-calling quality enough to pay for its latency?**
  The 2026-08-19 battery ran with `think: false` throughout, because reasoning before every tool
  call is what caused the original timeouts. It is plausible that thinking fixes the
  missing-argument case — and equally plausible it is unaffordable on a voice path. **Do not
  decide this from the existing run.** One `--think` pass of the same battery prices it.
  Owned by ARIA-109.

- ~~**Does `qwen3:8b` interleave tool calls correctly in a *streaming* response?**~~
  **ANSWERED 2026-08-19.** It does not interleave — the tool call arrives **complete in a single
  chunk** after the model finishes deciding, with arguments unfragmented. Streaming buys nothing on
  the tool path; it delivers a ~10× TTFT win on the prose path (0.18 s vs ~2.3 s). D37's scope is
  now known rather than assumed. See [the ARIA-109 brief](../spikes/ARIA-109-ollama-model.md).
  *ARIA-109 still cannot close: it must run against ARIA-50's conformance suite, which does not
  exist yet.*


## Changed by the 2026-09-23 decisions

**Closed as moot — not answered:**

| Was | Why it is moot |
|---|---|
| Confirm which **Postgres operator/Helm chart and Qdrant deployment topology** to use in-cluster | Neither runs in-cluster. [D73](../decisions/0015-external-datastores.md) puts Postgres on its own VM with a schema-per-service layout; [D78](../decisions/0015-external-datastores.md) puts Qdrant on its own VM. There is no operator to choose |
| Confirm **AppSignal's billable-request definition** against ARIA's fan-out before assuming the free tier has headroom | [D82](../decisions/0016-observability-v2.md) drops AppSignal. There is no plan to fit inside |

**Newly needing a measurement:**

- **Do the OpenTelemetry collector and Jaeger fit on `kube-worker-01`?**
  ([D82](../decisions/0016-observability-v2.md).) They replace a single AppSignal collector on a
  node with 4 vCPU and 7.75 GiB that already runs Harbor, Vault, Longhorn, monitoring,
  nginx-ingress and MetalLB. Jaeger's footprint depends on its storage backend, which is itself
  undecided. **Do not assume the Postgres/Qdrant departure leaves room — measure it.**
- **How large do the Postgres and Qdrant VMs need to be?**
  ([D73](../decisions/0015-external-datastores.md), [D78](../decisions/0015-external-datastores.md).)
  Moving them off Longhorn removes the volume-expansion safety net the cluster provided, so the
  initial sizing matters more than it did. No story sizes either one.
- **What does a real restore cost, in time and in steps?**
  [D80](../decisions/0015-external-datastores.md) requires a **tested** restore rather than a
  configured backup. The test is the measurement, and until it runs "we have backups" is an
  assumption.

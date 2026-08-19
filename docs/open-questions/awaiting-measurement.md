# Open — awaiting a measurement

[Library index](../README.md) · [Decisions](../decisions/README.md) · [Open questions](README.md) · [Sprint 1](../sprints/sprint-01.md)

**Do not settle these by discussion.** Each needs a number, a benchmark, or a look at the actual
hardware. Answering them from a conversation would manufacture the certainty this project has
deliberately refused to fake. Every item names the spike that owns it.

**Source:** CLAUDE.md §8, third group, as of 2026-08-12.

---

- Pick the specific hosted LLM provider/model for the Agent Core's default backend.
- Confirm which Postgres operator/Helm chart and Qdrant deployment topology to use in-cluster.
- Confirm the Rust MCP SDK to standardize on (`rmcp` or equivalent).
- Confirm the STT library/model and the TTS library/voice that fit the VRAM budget, and the GPU
  access approach.
- Measure GPU VRAM headroom next to Ollama and publish the `aria-speech` memory budget; then
  decide resident vs. load-on-demand model residency.
- Confirm the k3s cluster baseline — node count (which the internal-TLS decision explicitly
  depends on), storage classes, GPU runtime, and whether a container registry already exists.
- Which facts get embedded into Qdrant, and the classification rule separating typed from
  free-text preferences.
- Confirm AppSignal's billable-request definition against ARIA's per-interaction service fan-out
  before assuming the free tier (50K requests/month) has real headroom; then decide sampling.
- Where sandboxed `run_tests` execution physically runs for a candidate self-generated service —
  a disposable namespace/pod on the existing k3s cluster is the natural default, pending the
  cluster baseline.
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

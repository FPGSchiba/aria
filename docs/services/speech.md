# `aria-speech` — Speech

[Library index](../README.md) · [Architecture](../03-architecture.md) · [Decisions](../decisions/README.md) · [Open questions](../open-questions/README.md)

**Deployed service (GPU node) · `services/speech/`**

---

## Amendments since 2026-08-12

- **[D50](../decisions/0009-measurement-session.md)** — `aria-speech` **asserts GPU availability at
  startup and refuses to become ready without it**, rather than falling back to CPU and serving
  degraded. Observed 2026-08-19: after a host power-cycle Ollama ran 100% on CPU while appearing
  entirely healthy — API responsive, ~50× too slow, no signal any monitor would catch. A crash is
  strictly better than a silent capability loss. Affects this service's scaffold (ARIA-42) and its
  readiness probe (ARIA-93).
- **GPU node facts (preliminary)** — RTX 2070, **8 GB**; roughly **2.6 GB** left with `qwen3:8b`
  resident, measured at a 4096-token context and at rest. See
  [the ARIA-27 brief](../spikes/ARIA-27-gpu-baseline.md); the budget is not yet published.

---

## Purpose

STT and TTS only, **English only** (D24). Scheduled on the same GPU node already running Ollama.

## Replaces

The 2021 `Speech2Text` / `Text2Speech` boxes.

## Owns

- **VAD and utterance endpointing**, emitting utterance-boundary events upstream — the Gateway
  stays pure transport (D26)
- **TTS text normalisation** (numbers, dates, units), starting from whatever the chosen TTS library
  provides and adding rules only where it demonstrably fails (D30)
- Consuming mono s16le PCM (D25)
- Continuing to listen during synthesis, so barge-in works (D27)
- VRAM-aware model residency and admission — made possible by D28, which permits ARIA to
  reconfigure the shared Ollama instance's residency, concurrency and preloading

## Binding decisions

D24 (English only) · D25 (PCM internally) · D26 (owns VAD) · D27 (barge-in, cancellable synthesis) · D28 (may tune Ollama) · D30 (owns TTS normalisation) · D6 (plaintext in-cluster)

See [the Decision Log](../decisions/README.md) for the full reasoning and rejected alternatives.

## Contract

Streaming `Transcribe` and `Synthesize` — `proto/speech/v1/speech.proto`

## Open items

- **Awaiting measurement:** STT library/model and TTS library/voice that fit the VRAM budget
  (ARIA-30, ARIA-33); GPU VRAM headroom next to Ollama (ARIA-27); resident vs. load-on-demand
  (a pure function of the ARIA-27 number)
- Model-facing sample rate falls out of ARIA-30
- **Acoustic echo cancellation / half-duplex gating is unowned** and plausibly belongs on the
  client — which sits awkwardly with the thin-client reasoning behind D26 (ARIA-116)

## Jira stories

ARIA-27, 30, 33, 37, 42, 46, 49, 51, 116

---

*Derived from `03-architecture.md` and the Decision Log. Last updated 2026-08-19.*

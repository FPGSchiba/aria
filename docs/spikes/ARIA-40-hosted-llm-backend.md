# ARIA-40 — Pick the hosted LLM provider and model

[Library index](../README.md) · [Spikes](README.md) · [Agent Core](../services/agent-core.md) · [Sprint 1](../sprints/sprint-01.md)

**Status: cannot close on research. Framework + probe harness ready; the measurement is yours to run.**
Desk research 2026-08-19.

> ### ⚠️ Conflict of interest, stated up front
> This brief was written by Claude, and one of the candidates is Claude. Treat the recommendation
> section accordingly — it is deliberately written as *criteria and a harness*, not as a verdict.
> The story's own acceptance criteria demand observed evidence over documentation, which is the
> right defence here: **run the probes and let the numbers decide.**

---

## Why research alone cannot close this

The story requires:

> Each candidate is exercised with a throwaway probe against a representative ARIA prompt carrying
> at least five MCP-style tool schemas; observed tool-selection behaviour and malformed-tool-call
> rate are recorded as **evidence, not asserted from documentation**.

That criterion exists precisely to stop a decision like this being made from a comparison table.
Honouring it needs API credentials this session does not have. So this brief delivers everything
*around* the measurement — and [`../../scripts/aria-llm-probe.py`](../../scripts/aria-llm-probe.py)
runs the measurement itself.

---

## What actually drives the choice

ARIA serves one person plus a small circle. **Scale is not the driver.** Ranked by what will
actually bite:

1. **Tool-calling reliability under a realistic toolset.** The Agent Core's whole job is choosing
   the right MCP tool. A model that picks plausible-but-wrong tools, or emits arguments that fail
   schema validation, produces a broken assistant regardless of its prose quality.
2. **Streaming with interleaved tool-call events.** D37 makes `Decide` server-streaming so speech
   synthesis can start on the first sentence. A provider whose streaming does not surface tool
   calls incrementally forces buffering, which throws away the single largest latency win on a
   voice path.
3. **Multiple tool calls in one response** — or a clean way to serialise them. Affects the shape of
   the tool loop (ARIA-61).
4. **Cost per interaction.** Real, but at circle scale it is a rounding error against the above.
   ARIA sends a system prompt + retrieved context + a full tool catalogue on every turn, so *input*
   tokens dominate and prompt caching matters more than headline rate.
5. **Rust client story.** Weakest constraint — all three are HTTP+JSON and the trait boundary
   (`LlmBackend`, ARIA-50) is ARIA's own regardless.

---

## Candidates

Primary-source figures, read **2026-08-19**. Pricing per million tokens.

### Anthropic — Claude

| Model | API ID | Context | Max output | In | Out |
|---|---|---|---|---|---|
| Claude Fable 5 | `claude-fable-5` | 1M | 128k | $10 | $50 |
| Claude Opus 5 | `claude-opus-5` | 1M | 128k | $5 | $25 |
| Claude Sonnet 5 | `claude-sonnet-5` | 1M | 128k | $2 | $10 |
| Claude Haiku 4.5 | `claude-haiku-4-5-20251001` | 200k | 64k | $1 | $5 |

Docs describe **Opus 5** as the starting point for most agentic applications, and Fable 5 as aimed
at long-running agents. For ARIA's turn-shaped workload, **Sonnet 5 is the obvious first probe** —
Opus/Fable pricing buys depth on long autonomous runs that a voice turn does not have.

### Google — Gemini

| Model | API ID | In | Out |
|---|---|---|---|
| Gemini 3.7 Flash | `gemini-3.7-flash` | $0.75 | $3.75 |
| Gemini 3.6 Flash | `gemini-3.6-flash` | $0.75 | $3.75 |
| Gemini 3.5 Flash-Lite | `gemini-3.5-flash-lite` | $0.30 | $2.50 |
| Gemini 2.5 Pro | `gemini-2.5-pro` | $1.25 (≤200k) | $10.00 |

Materially cheaper, and a free tier exists for probing. Flash-class pricing runs through
2026-12-31 — a date worth noting rather than assuming permanent.

### OpenAI

**Not sourced.** `platform.openai.com/docs/pricing` could not be fetched in this session. Read the
current model IDs and rates off OpenAI's own pricing page before probing; do not take them from an
aggregator blog, several of which disagreed with each other while researching this.

---

## What this brief *can* settle

These are the story's other acceptance criteria, and none of them needs a probe.

### Egress contract

CLAUDE.md's "only the hosted LLM API call leaves the network" needs a single host:port so
Deployment & Infra can write the egress rule. One line per candidate, all **HTTPS/443**:

| Provider | Host |
|---|---|
| Anthropic | `api.anthropic.com` |
| Google | `generativelanguage.googleapis.com` |
| OpenAI | `api.openai.com` |

**Only the chosen one is allow-listed.** The egress rule is part of the decision, not a follow-up.

### Credential contract

The service reads the key from an environment variable named **`ARIA_LLM_API_KEY`**, populated
from a sealed secret (D31) with key `llm-api-key`. Provider-neutral on purpose: switching provider
must not require a manifest change, only a config change. Provisioning is Deployment & Infra's.

### Canonical internal shape — a real decision, and it leans one way

The story asks whether the chosen provider's tool-call format becomes ARIA's internal canonical
shape or whether an adapter is needed in `src/llm/hosted.rs`.

**It should be neither — the canonical shape should be ARIA's own, and *both* backends should
adapt into it.** The reasoning is D35: the Ollama fallback is not optional, and ARIA-50 requires a
**backend-agnostic conformance suite** that both backends pass unmodified. If the hosted
provider's wire format becomes canonical, the Ollama backend is permanently translating into a
foreign shape and the conformance suite quietly becomes a test of the hosted provider.

There is a pragmatic counter-argument worth recording: Ollama exposes an **OpenAI-compatible**
endpoint, so adopting OpenAI's tool-call shape as canonical would make one of the two adapters
nearly free. That is a genuine saving, and it is a reason to prefer OpenAI's *shape* — but it is
not a reason to let a vendor own ARIA's internal type. **Recommendation: an ARIA-owned enum that
is deliberately close to the OpenAI shape**, taking the convenience without the coupling.

> ### ⚠️ Finding, 2026-08-19 — the compat shim is not sufficient for Ollama
>
> Discovered while probing: **Ollama's OpenAI-compatible endpoint cannot disable a model's
> thinking/reasoning phase.** `think: false` exists only on the native `/api/chat`. Thinking
> models (Qwen3, DeepSeek-R1, GPT-OSS) reason before emitting a tool call, and with five tool
> schemas in the prompt that routinely exceeds a 120 s timeout — observed, not theorised.
>
> So the "one adapter is nearly free" saving **does not survive contact with a thinking model**.
> The Ollama backend needs the native endpoint, which means two genuinely different adapters
> either way.
>
> This *strengthens* the recommendation above rather than changing it: since neither backend
> maps for free, there is no remaining reason to let either vendor's wire format be ARIA's
> internal type. It also adds a requirement to **ARIA-54**: the Ollama backend must control the
> thinking parameter explicitly rather than inheriting the model's default, and the tool-loop
> latency budget (ARIA-61) has to account for a reasoning phase that the hosted path does not
> have. The probe harness now uses `/api/chat` with `think: false` by default for this reason.

*This is a proposal, not a decision.* It needs to be taken with the Agent Core epic and recorded
as a D-entry — ARIA-50 depends on it.

---

## The probe harness

[`../../scripts/aria-llm-probe.py`](../../scripts/aria-llm-probe.py) satisfies acceptance
criterion 2. It carries **five MCP-style tool schemas** modelled on what ARIA will actually expose
(calendar, music, knowledge lookup, reminders, home control) and runs a fixed battery of prompts
per provider, including the cases that break naive tool-callers:

- an unambiguous single-tool request
- a request needing **two** tools
- a request needing **no** tool — measures spurious calls, the failure mode that most annoys a user
- an **ambiguous** request between two similar tools
- a request whose required argument is **absent** from the prompt — measures hallucinated arguments

It records, per model: tool-selection accuracy, **malformed-call rate** (arguments failing schema
validation), spurious-call rate, time-to-first-token, and whether tool calls arrive incrementally
while streaming.

```bash
export ANTHROPIC_API_KEY=... GEMINI_API_KEY=... OPENAI_API_KEY=...
python3 scripts/aria-llm-probe.py --repeats 5
```

It writes `probe-results.json` plus a markdown summary to attach to ARIA-40.

**Run it against the free tiers first** where they exist — the whole battery is a few thousand
tokens per model.

---

## What is needed to close this story

- [ ] Read current OpenAI model IDs and pricing from OpenAI's own page
- [ ] Run the probe harness against at least two providers, ≥5 repeats
- [ ] Record observed tool-selection and malformed-call rates as evidence on ARIA-40
- [ ] Decide the canonical internal tool-call shape **with the Agent Core epic** (see above)
- [ ] Confirm the egress host and the `ARIA_LLM_API_KEY` contract with Deployment & Infra
- [ ] Write the decision as a D-entry with rejected alternatives
- [ ] Move the item out of [`../open-questions/awaiting-measurement.md`](../open-questions/awaiting-measurement.md)

### Note for whoever runs this

ARIA-109 — *which Ollama model* — needs the **same** battery run locally. That is not a
coincidence: ARIA-50's conformance suite is meant to be backend-agnostic, so the hosted probe and
the local probe should be the same harness. The script takes `--provider ollama --base-url` for
exactly this. Running both from one harness is what makes D35's deferred routing rule answerable
later with evidence instead of a guess.

---

## Sources

- [Claude models overview — platform.claude.com](https://platform.claude.com/docs/en/about-claude/models/overview)
- [Gemini API pricing — ai.google.dev](https://ai.google.dev/gemini-api/docs/pricing)
- [Berkeley Function Calling Leaderboard (BFCL V4)](https://gorilla.cs.berkeley.edu/leaderboard.html) — V4, last updated 2026-04-12; adds holistic agentic evaluation. The ranking table could not be extracted from the page in this session; consult it live, but treat it as *supporting* evidence only. BFCL measures generic function calling, not ARIA's toolset — which is exactly why the story demands a probe against ARIA's own schemas.

*Pricing and model IDs move. Re-read both pages before deciding.*

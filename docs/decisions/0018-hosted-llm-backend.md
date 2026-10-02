# ARIA — Decision Log · Hosted LLM backend

**Decision D85** · taken 2026-10-02 (measurement — ARIA-122)

One entry per decision: what was decided, why, what was rejected and why, and what it affects.
Evidence: [hosted-llm-providers-2026-09.md](../spikes/hosted-llm-providers-2026-09.md) — desk research,
probe results and follow-up checks — with raw runs in
[`probe-runs-2026-10/`](../spikes/probe-runs-2026-10/).

> **Conflict of interest, carried over from the brief.** The research and the recommendation were
> written by Claude; Anthropic was a candidate. The chosen backend is not Anthropic's, and the reasons
> below are the measured ones.

---

### D85 · ARIA-122 — The default hosted backend is Qwen 3.8 27B on Cerebras

**Decided.**

- The Agent Core's default hosted backend is the model **`qwen-3.8-27b`**, served by **Cerebras
  Inference** at `https://api.cerebras.ai/v1`.
- It is called through an **OpenAI-compatible Chat Completions adapter**. The host is configuration
  (base URL and key), so another host serving the same open weights is a configuration change, not a
  new adapter. D49's ARIA-owned tool-call shape is unchanged; this adapter maps into it.
- **Reasoning stays at Cerebras's default** (`high`). `reasoning_effort: "none"` was measured and
  rejected (see *Why*, point 6).
- **Egress:** `api.cerebras.ai:443` is the one LLM host allow-listed, per the egress contract in the
  [ARIA-40 brief](../spikes/ARIA-40-hosted-llm-backend.md).
- **Credential:** unchanged — `ARIA_LLM_API_KEY`, from the sealed secret key `llm-api-key`.
- **D35 is unchanged:** the Ollama backend remains the fallback on error or timeout. Its model is
  ARIA-109's question and is not decided here.

**Why.**

1. **It tied for first on every measurement.** 25/25 with today's date in the prompt and a
   clarification tool available; both tool calls in all 15 two-tool runs across three hosts; no
   malformed calls; no invented arguments with reasoning on.
2. **It wins the heaviest criterion, data handling** (W1, 20 of 100). Cerebras's privacy policy:
   *"We do not retain inputs and outputs associated with our training, inference and chatbot
   Services."* OpenAI keeps abuse logs for up to 30 days; Anthropic's own pages contradict each other.
3. **It is the fastest.** Median time to first token 0.42 s and median total 0.53 s on the dated
   battery — roughly twice as fast as the nearest alternative (0.72–0.84 s TTFT).
4. **The weights are open, so ARIA is not tied to the host.** The same model was measured on Groq and
   on OpenRouter with zero-retention routing, and is listed by Scaleway. It is also the same family as
   the local fallback candidate `qwen3:8b`, so a D35 failover should change behaviour less than a
   switch between vendors would.
5. **It is the least to maintain.** A static API key, one host, and plain Chat Completions with tools —
   no Responses API. It was used from Switzerland throughout the measurements.
6. **Its reasoning setting was measured, not assumed.** With `reasoning_effort: "none"` TTFT fell by
   about 0.1 s, but the model invented the missing room in 3 of 10 runs and picked the wrong tool twice.

**Accepted cost.**

- **No published deprecation notice.** Cerebras's deprecations page shows `gemma-4-31b` announced and
  removed on the same day (2026-09-03). A model can disappear without warning. The D35 failover covers
  the outage; the open weights cover the exit. See the revisit trigger.
- **Cost:** $0.99 per 1M input and $1.49 per 1M output tokens, with **no documented cache price** —
  about **$10.35 per 1,000 reference calls**, roughly $15 a month at the brief's assumed 1,500 calls.
  Cheaper than GPT-6 Sol or Sonnet 5.5 uncached; dearer than DeepSeek V4.1 Flash at its listed prices.
- **The desk score falls with the right candidate.** The brief's 72.6 for Cerebras was computed on
  gpt-oss-120b. Scored on Qwen 3.8 27B and with the same-day removal counted, W2 drops from 4 to 2 and
  W3 from 3 to 1, giving **61.8**. The measurements outweigh that here, but the number is recorded rather
  than left at the flattering figure.
- **Streamed tool calls arrive as one complete chunk**, not in pieces. ARIA-109 already showed the tool
  path gains little from incremental arguments; prose still streams (TTFT 0.40 s on prose turns).
- **Without a date in the prompt it invents or drops dates.** With the date injected it was right in
  every run. That makes date injection new work, below.
- **Trial credit only.** The free trial allows 5 requests per minute; production needs the
  pay-as-you-go Developer tier.
- **Household data still leaves the network in plaintext to the provider.** Choosing a provider that
  does not retain it narrows the exposure; it does not resolve the contradiction already recorded
  against `04-deployment.md`.

**Rejected.**

- *GPT-6 Sol on OpenAI* — equal accuracy (24/25), correct dates even without injection, and at least
  six months' notice before retirement. Rejected for 30-day abuse logs with zero retention only by
  approval, the requirement to use the Responses API or switch reasoning off (Chat Completions rejects
  tools while reasoning is on), slower TTFT (0.79 s), and $8.60 cached / $23.00 uncached per 1,000 calls.
- *Claude Sonnet 5.5 on Anthropic* — Jann's stated preference, and equal on accuracy once a
  clarification tool existed (25/25, no malformed calls). Rejected for contradictory retention
  documentation, zero retention only via sales, no EU or Swiss processing, 60 days' notice, the slowest
  TTFT of the four (0.84 s), and $8.60 / $23.00 per 1,000 calls.
- *DeepSeek V4.1 Flash* — equal accuracy, 0.72 s TTFT, and about a tenth of the cost at Fireworks' and
  Together's listed prices. Rejected for now because it was measured only through OpenRouter, which does
  not record the serving host; adopting it needs one pinned host and a rerun. DeepSeek's own API stays
  knocked out (training on personal data with no self-serve opt-out; data stored in the PRC).
- *Claude Haiku 4.5* — tied in the first round and the cheapest closed model with a known cache price
  ($4.30 per 1,000 cached). Not rerun with the follow-up checks; invented dates in all five runs; the same
  Anthropic data terms as Sonnet 5.5.
- *gpt-oss-120b on any host* — made a single tool call in all 15 two-tool runs on Groq, OpenRouter and
  Cerebras.
- *Groq* — serves Qwen 3.8 27B only as a preview ("evaluation purposes only"); its free tier (8K
  tokens/min) is smaller than ARIA's reference call.
- *xAI* — excluded by Jann.
- *A mixture, or automatic routing between models* — reopens D35, whose trigger asks for evidence of
  where a cheaper model is good enough. A battery that four models saturate provides none.

**Revisit trigger.**

- Cerebras announces the deprecation of `qwen-3.8-27b`, removes it, or changes its inference
  data-retention terms → rerun `scripts/aria-llm-checks.py` against another host of the same weights
  and against the runners-up, and supersede this entry.
- ARIA-50's backend-agnostic conformance suite exists → run it against this backend; a failure reopens
  the choice.

**Creates new work — recorded, not decided here.**

- **The Agent Core injects the current date, time and time zone into every request.** Measured: all four
  leading models wrote "tomorrow at 3pm" correctly in every run once the date was given, and invented or
  dropped it without. Where and how is open.
- **ARIA-121 gains evidence for a clarification path.** Given a `request_information` tool, all four
  leading models asked in 5 of 5 missing-argument runs instead of inventing a value.
- **If strict tool schemas are used, null optional arguments must be dropped before forwarding.**
  OpenAI-style strict mode (OpenAI, Cerebras, OpenRouter) requires every property, so models send
  `null` for unused optional fields. Whether to use strict mode, and whether the adapter or the MCP
  Registry drops the nulls, is open.
- **Move the Cerebras account to pay-as-you-go** before production.
- **Allow-list `api.cerebras.ai:443`** in the egress policy (Deployment & Infra).
- **`aria-llm-probe.py` still lacks** `--base-url` on its OpenAI adapter, a User-Agent and streamed
  reads for hosted providers; `aria-llm-checks.py` covers these for now.

**Affects.** ARIA-122 (closes its choice), ARIA-143 (the hosted backend adapter — OpenAI-compatible,
Cerebras), ARIA-50, ARIA-61, ARIA-121, ARIA-54; D35, D37, D49.

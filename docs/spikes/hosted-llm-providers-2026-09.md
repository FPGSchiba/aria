# Hosted LLM providers — desk research for ARIA-122

[Library index](../README.md) · [Spikes](README.md) · [ARIA-40 brief](ARIA-40-hosted-llm-backend.md) · [Agent Core](../services/agent-core.md) · [Sprint 1](../sprints/sprint-01.md)

**Status: desk research complete; probe results and follow-up checks added 2026-10-02 ([Probe results](#probe-results-aria-122)).
It ranks candidates and records measurements.** **Decision, 2026-10-02:
[D85](../decisions/0018-hosted-llm-backend.md) — Qwen 3.8 27B on Cerebras.** The choice stays in
[`../open-questions/awaiting-measurement.md`](../open-questions/awaiting-measurement.md) until
ARIA-122 has run `scripts/aria-llm-probe.py` against the shortlist. No D-entry is written here.

Facts were read from the providers' own pages on **2026-09-26** and **2026-10-01**; every source
carries its date in [Sources](#sources).

> ### ⚠️ Conflict of interest
> Written by Claude; Anthropic is a candidate. Anthropic was scored by the same rubric, and every
> Anthropic score that rests on judgement is marked ⚖ and listed in
> [Anthropic judgement calls](#anthropic-judgement-calls). The largest one moves Anthropic from
> 8th to 3rd, and the ranking uses the reading that is worse for Anthropic.

---

## Shortlist for ARIA-122 — top 5 by points

| # | Provider | Model scored | Points | Free tier | Probe today | Why it earns a probe run |
|---|---|---|---|---|---|---|
| 1 | **Groq** | `openai/gpt-oss-120b` | **82.4** | Recurring (30 RPM, 1K RPD, 8K TPM) | after `--base-url` patch | Self-serve zero data retention, $1.68 per 1,000 reference calls, a `parallel_tool_calls` switch, and a free tier big enough to run the whole battery at no cost |
| 2 | **OpenRouter** | `gpt-oss-120b` with account-level ZDR routing | **81.6** | Recurring (`:free` models, 20 RPM, 50/1,000 RPD) | after `--base-url` patch | The only router that can *enforce* ZDR-only routing from the account; first-class OpenAI-compatible API; one key reaches many models |
| 3 | **Fireworks AI** | `gpt-oss-120b` | **73.6** | $1 one-off credit | after `--base-url` patch | No prompt logging for open models by default, the only gpt-oss host with a documented cache price ($0.60 / 1,000 calls), documented incremental tool-call streaming, and full JSON Schema incl. `$ref` |
| 4 | **Cerebras** | `gpt-oss-120b` | **72.6** | $5 one-off trial | after `--base-url` patch | Does not retain inference inputs/outputs; strict tool mode; `parallel_tool_calls` switch |
| 5 | **OpenAI** | `gpt-6-sol` | **70.8** | none | **native** | The only top-5 entry that passes all four knock-outs on documentation; native probe adapter; ≥ 6 months' deprecation notice; documented incremental tool-call events |

**Highest desk score: Groq — 82.4 points.** This is a desk score, not the decision.

Four things to know before spending keys on this list:

1. **Four of the five run the same model.** Groq, OpenRouter, Fireworks and Cerebras were all scored
   on `gpt-oss-120b`. The probe mostly measures the *model's* tool-calling, so these four will largely
   re-measure one model on four hosts; what differs between them is latency, which the probe cannot yet
   measure for hosted providers (see [Probe gaps](#probe-gaps)).
2. **Only OpenAI is reachable by the probe today.** The other four need `--base-url` wired to the
   OpenAI adapter.
3. **Places 5–7 are within 0.4 points.** OpenAI 70.8, OVHcloud 70.4, DeepInfra 70.4. Several of
   their scores carry ⚖; treat 5th place as a tie.
4. **Switzerland availability (K2) is unverified for Groq, OpenRouter, Fireworks and Cerebras.**
   Nothing read excludes Switzerland, but no page read confirms it either. Check at sign-up.

## Probe results (ARIA-122)

Jann ran `scripts/aria-llm-probe.py` with `--repeats 5` (25 requests per model) on **2026-10-01 and
2026-10-02**. Raw results and the probe's own summaries are in
[`probe-runs-2026-10/`](probe-runs-2026-10/). Scoring uses the measured-criteria rubric
pre-registered on 2026-09-26 (M1 35, M2 25, M3 15, M4 10; M5 not measurable, so the maximum is 85).

| # | Host · model | Correct (probe rule) | Correct (refusals count) | `multi` | Malformed | Unneeded calls | Made-up `room` | Median total latency | Points /85 (probe rule) | Points /85 (refusals count) |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | cerebras · `qwen-3.8-27b` | 19/25 | 24/25 | 5/5 | 0 | 0/5 | 0/5 | 0.48 s | 64 | **85** |
| 2 | openai · `gpt-6-sol` | 20/25 | 25/25 | 5/5 | 0 | 0/5 | 0/5 | 1.17 s | 64 | **85** |
| 3 | anthropic · `claude-haiku-4-5` | 20/25 | 25/25 | 5/5 | 0 | 0/5 | 0/5 | 1.22 s | 64 | **85** |
| 4 | anthropic · `claude-sonnet-5` | 19/25 | 24/25 | 5/5 | 0 | 0/5 | 0/5 | 2.09 s | 64 | **85** |
| 5 | openrouter · `openai_gpt-6-luna` | 18/25 | 23/25 | 5/5 | 0 | 0/5 | 0/5 | 2.67 s | 57 | **78** |
| 6 | groq · `qwen_qwen3.8-27b` | 18/25 | 22/25 | 5/5 | 0 | 0/5 | 0/5 | 0.36 s | 57 | **71** |
| 7 | anthropic · `claude-opus-5-5` | 16/25 | 21/25 | 5/5 | 0 | 0/5 | 0/5 | 3.21 s | 57 | **71** |
| 8 | openrouter · `qwen_qwen3.8-27b` | 17/25 | 22/25 | 5/5 | 0 | 0/5 | 0/5 | 4.10 s | 57 | **71** |
| 9 | cerebras · `gpt-oss-120b` | 15/25 | 20/25 | 0/5 | 0 | 0/5 | 0/5 | 0.29 s | 57 | **64** |
| 10 | anthropic · `claude-sonnet-5-5` | 21/25 | 23/25 | 5/5 | 3 | 0/5 | 0/5 | 1.74 s | 56 | **63** |
| 11 | openrouter · `openai_gpt-oss-120b` | 13/25 | 18/25 | 0/5 | 0 | 0/5 | 0/5 | 4.03 s | 57 | **57** |
| 12 | groq · `openai_gpt-oss-120b` | 15/25 | 20/25 | 0/5 | 4 | 0/5 | 0/5 | 0.67 s | 37 | **44** |
| — | openrouter · `google_gemini-3.8-flash` *(incomplete: 10 of 25 runs refused for credit)* | — | — | 5/5 | 0 | — | — | 2.97 s | — | — |

How to read the table:

- **Two readings of `missing_arg`.** The prompt is *"Put on some music."* with the required `room`
  absent. The probe counts declining to call a tool as a miss; the
  [ARIA-109 brief](ARIA-109-ollama-model.md) counted the same behaviour as the right instinct. Both are
  shown. **Which reading counts is open — Jann to decide.** The same four models tie at the top either
  way.
- **Corrections to the probe's own summaries.** The summaries leave errored runs out of their
  percentages. Here, Groq's HTTP 400 `Tool call validation failed` responses count as malformed calls
  and as misses, and a model with runs refused for lack of credit is marked incomplete.
- **Sonnet 5.5, run 3 of `missing_arg`** called `music_play` with empty arguments. The probe counts the
  selection as correct and the call as malformed; the rule is kept as written.
- **Latency is total request time**, non-streamed, from Jann's home network. It is **not** time to
  first token.

### Not measured

- **M5 (time to first token) and M6 (stream shape)** in the first round — the probe reads streams only
  from Ollama. Measured afterwards for four models; see [Follow-up checks](#follow-up-checks-2026-10-02).
- **Gemini 3.8 Flash** — incomplete (10 of 25 runs refused for credit). **Mistral Medium 3.5** — its
  OpenRouter ID was not found, so it never ran. **Fireworks** — skipped; it would have been a fourth
  measurement of gpt-oss-120b.
- **Cerebras's price for `qwen-3.8-27b`**, read 2026-10-02: $0.99 in / $1.49 out per 1M tokens; reasoning is on by default at `high` (the probe run used that default). [Model page](https://inference-docs.cerebras.ai/models/qwen-3.8-27b) · [deprecations](https://inference-docs.cerebras.ai/support/deprecation)

### Run conditions

Settings differ between hosts, so cross-host comparisons of the same model are close but not
like-for-like.

| Host | How the probe reached it | Added to every request | Reasoning |
|---|---|---|---|
| Groq | OpenAI adapter via a wrapper, `https://api.groq.com/openai/v1` | `max_completion_tokens: 1024`, custom User-Agent, 20 s / 10 s pause (free tier: 8K tokens/min) | gpt-oss: Groq default (cannot be disabled); Qwen: `reasoning_effort: "none"` |
| OpenRouter | OpenAI adapter via a wrapper, `https://openrouter.ai/api/v1` | `max_tokens: 1024`; `provider: {zdr: true, data_collection: "deny", require_parameters: true}`; custom User-Agent; 3.5 s pause on later runs | provider default |
| Cerebras | OpenAI adapter via a wrapper, `https://api.cerebras.ai/v1` | `max_completion_tokens: 1024`; custom User-Agent; 13 s pause (trial: 5 RPM) | provider default |
| OpenAI | native adapter, one field added by a wrapper | `reasoning_effort: "none"` — required, see below | off |
| Anthropic | native adapter, unmodified | `max_tokens: 1024` (built in) | none requested; the first recorded request of Sonnet 5.5 and of Opus 5.5 reports 0 thinking tokens |

### What the runs show

1. **The top four are tied — the five-case battery is saturated.** Qwen 3.8 27B on Cerebras, GPT-6 Sol,
   Haiku 4.5 and Sonnet 5 all score 85 counting refusals and 64 by the probe's rule. The battery cannot
   separate them; latency, cost, data handling and dates (point 5) can. Harder cases belong in ARIA-50's
   conformance suite, not in more models here.
2. **Two tool calls in one response is model behaviour, not host behaviour.** gpt-oss-120b made a single
   call in all 15 `multi` runs across Groq, OpenRouter and Cerebras. Every other model made both calls in
   every run.
3. **Malformed calls depend on the host as well.** gpt-oss-120b sent `brightness: null` for an optional
   integer in 4 of 5 `multi` runs on Groq — rejected by Groq with HTTP 400 `tool_use_failed` — and never
   on OpenRouter or Cerebras.
4. **Models reach for a way to ask.** With the room missing, Sonnet 5.5 twice called a tool that does not
   exist (`ask_user`), and Qwen 3.8 27B on Groq once created a reminder reading *"Ask the user what song
   and which room they want."* Relevant to ARIA-121: an explicit clarification path may be what models
   look for.
5. **Dates — not scored, but they separate the leaders.** For *"tomorrow at 3pm"* (the `due` / `start`
   argument written):

   | Model (host) | Run date | Written |
   |---|---|---|
   | GPT-6 Sol (OpenAI) | 2026-10-02 | `2026-10-03T15:00:00` ×5 — correct |
   | gpt-oss-120b (Groq) | 2026-10-01 | `2026-10-02T15:00:00` ×5 — correct |
   | Haiku 4.5 (Anthropic) | 2026-10-02 | 2024 and 2025 dates in all 5 runs — invented |
   | Sonnet 5 (Anthropic) | 2026-10-02 | `"tomorrow at 3pm"`, `"tomorrow at 15:00"`, `2025-06-19T15:00:00`, once omitted |
   | Qwen 3.8 27B (Groq, Cerebras) | 2026-10-01 | June 2026 dates, or omitted |
   | Sonnet 5.5, Opus 5.5, GPT-6 Luna | 2026-10-01/02 | omitted every time — the "3pm" is lost |

   The probe's system prompt gives no date. Models that do not know it either guess it or drop it, and
   an omitted optional field is not a malformed call, so the probe scores it as correct. **Consequence:**
   the Agent Core must put the current date, time and time zone into every request rather than rely on
   the model or host; the probe should add the date to its system prompt so this becomes measurable.
6. **Obstacles met while probing** — each one is work for an adapter or for the probe patch:
   - **Groq refused Python's default user agent** (HTTP 403, Cloudflare `error code: 1010`). Every later
     wrapped run set its own.
   - **Groq rejects a schema-invalid tool call with HTTP 400 `tool_use_failed`** instead of returning
     it. A Groq adapter must treat that as a model failure, not a transport failure.
   - **OpenAI's Chat Completions rejects function tools while reasoning is on** for GPT-6 Sol ("Function
     tools with reasoning_effort are not supported"). The run used `reasoning_effort: "none"`. OpenAI's
     reasoning guide says to use the Responses API for function calling, and that Chat Completions does
     not support it for GPT-6 Astra or GPT-6.1 Sol. If OpenAI is chosen, the adapter needs the Responses
     API — which weakens W6's assumption that Chat Completions compatibility is the common denominator.
   - **OpenRouter** limits new accounts to 20 requests per minute per model, and refuses requests (HTTP
     402) once the balance cannot cover them.
   - **Groq's free tier** (8K tokens/min) fits the probe with pauses, not ARIA's ~10k-token reference call.
7. **Total latency by host** (not TTFT): Cerebras 0.29–0.48 s, Groq 0.36–0.67 s, OpenAI 1.17 s,
   Anthropic 1.22–3.21 s, OpenRouter 2.67–4.10 s. The probe does not record which downstream provider
   OpenRouter chose; the cost of one gpt-oss request matched CoreWeave's listed price (inferred).

### The four tied leaders, beyond the probe

| | Qwen 3.8 27B · Cerebras | GPT-6 Sol · OpenAI | Haiku 4.5 · Anthropic | Sonnet 5 · Anthropic |
|---|---|---|---|---|
| Desk score (provider, scored on its candidate model) | 72.6 | 70.8 | 67.6 | 67.6 |
| Median total latency | 0.48 s | 1.17 s | 1.22 s | 2.09 s |
| Cost per 1,000 reference calls | $10.35 (no cache price documented, so billed uncached) | $8.60 cached / $23.00 uncached | $4.30 cached / $11.50 uncached | $8.60 cached / $23.00 uncached |
| Data retention | inputs/outputs not retained | 30-day abuse logs; ZDR by approval | contradictory pages (see [Anthropic judgement calls](#anthropic-judgement-calls)); ZDR via sales | same |
| Dates (point 5) | invented or omitted | correct | invented | inconsistent |
| Catch | trial credit only; **no published deprecation notice** — `gemma-4-31b` was announced deprecated and removed on the same day (2026-09-03) | reasoning off, or Responses API | — | moved to Anthropic's older-models table on 2026-10-02 |

**Anthropic's candidate changes from Sonnet 5 to Sonnet 5.5.** On 2026-10-02 Anthropic's models page
lists Claude Sonnet 5.5 (`claude-sonnet-5-5`) as its current mid-tier model at the same price ($2 /
$0.20 / $10), so under this brief's rule it replaces Sonnet 5. Its desk score is unchanged. Its probe
run placed 10th, held back by the three malformed calls in point 4.

### Follow-up checks (2026-10-02)

Run with [`scripts/aria-llm-checks.py`](../../scripts/aria-llm-checks.py), which re-uses the probe's five
tools and five cases. Four variants per model, each the full battery at 5 repeats: **dated** (today's
date, time and time zone in the system prompt), **clarify** (dated plus a `request_information` tool;
asking is the correct answer to `missing_arg`), **strict** (dated plus strict tool schemas; null optional
arguments stripped and counted) and **stream** (dated, streamed). Raw files:
[`probe-runs-2026-10/checks/`](probe-runs-2026-10/checks/). DeepSeek V4.1 Flash ran through OpenRouter
with zero-retention routing enforced — not through DeepSeek's own API, which stays knocked out.

| Model (host, reasoning) | dated: correct · date right | clarify: correct · asked | strict: malformed · nulls stripped | Median TTFT (tool turns · prose) | Tool args in pieces | Median total (dated) |
|---|---|---|---|---|---|---|
| Qwen 3.8 27B (Cerebras, default `high`) | 20/25 · 5/5 | **25/25** · 5/5 | 0 · 5 | **0.42 s · 0.40 s** | 0/15 — each call in one chunk | **0.53 s** |
| Qwen 3.8 27B (Cerebras, `none`) | 18/25 · 3/5 | — | — | 0.31 s · 0.25 s | 0/14 | 0.35 s |
| GPT-6 Sol (OpenAI, `none`) | 20/25 · 5/5 | 24/25 · 5/5 | 0 · 5 | 0.84 s · 0.75 s | 15/15 | 1.30 s |
| Sonnet 5.5 (Anthropic, none requested) | 21/25 · 5/5 *(1 malformed)* | **25/25** · 5/5 | 0 · 0 | 0.83 s · 0.95 s | 15/16 | 1.83 s |
| DeepSeek V4.1 Flash (OpenRouter, default) | 20/25 · 5/5 | **25/25** · 5/5 | 0 · 5 | 0.73 s · 0.56 s | 15/15 | 1.69 s |

Every `dated` miss except those noted is a refusal on `missing_arg`, which is correct behaviour once a way
to ask exists (the `clarify` column).

**What the checks settled:**

1. **A date in the prompt fixes dates.** All four models wrote tomorrow at 15:00 in every dated run,
   where the first round had invented or dropped it. The Agent Core requirement in point 5 above is now
   a measured fix, not a guess.
2. **Given a way to ask, every model asks.** All four called `request_information` in 5 of 5
   missing-room runs. Sonnet 5.5's invented `ask_user` and Qwen's reminder-as-note from the first round
   were artifacts of a battery with no ask path. **Input to ARIA-121:** give the model an explicit
   clarification tool or path. Sample question (Qwen): *"What would you like me to play, and in which room
   should it play?"*
3. **Strict mode removes malformed calls — and creates nulls on OpenAI-style providers.** No model made a
   malformed call under `strict`. But OpenAI, Cerebras and OpenRouter require every property to be listed
   as required, so models send `brightness: null` for the unused optional field (every `multi` run).
   **Consequence:** if ARIA uses strict schemas on those providers, null optional arguments must be dropped
   before the call reaches an MCP server. Anthropic's strict mode leaves optional fields optional; no nulls.
4. **Time to first token is measured** (M5; ARIA-122 criterion 2) for these four: Qwen on Cerebras
   0.42 s, DeepSeek via OpenRouter 0.72 s, GPT-6 Sol 0.79 s, Sonnet 5.5 0.84 s. Groq, Haiku, Opus and the
   other first-round models were not streamed.
5. **Stream shape** (M6): Cerebras delivers each tool call as one complete chunk; OpenAI, Anthropic and
   DeepSeek via OpenRouter send arguments in pieces. Both work for D37 — ARIA-109 already found that the
   tool path gains little from streaming.
6. **Switching Qwen's reasoning off costs accuracy.** It saved about 0.1 s of TTFT, but invented the
   missing room in 3 of 10 runs, picked `knowledge_lookup` for the dentist reminder twice and added a
   stray tool call once. Keep reasoning on.
7. **With date and a clarification path, the battery is saturated again.** Accuracy no longer separates
   these four. The choice rests on data handling, cost, latency and stability.

**Pre-registered rubric applied to the `stream` run** (dated battery, streamed; M1–M5, out of 100):
Qwen 3.8 27B **79 / 100**, GPT-6 Sol 76 / 97, DeepSeek V4.1 Flash 76 / 97, Sonnet 5.5 70 / 84 (by the
probe's rule / counting refusals). Sonnet 5.5's lower score comes from its one malformed call and from
TTFT; in the `clarify` run it made none.

| After the checks | Qwen 3.8 27B · Cerebras | GPT-6 Sol · OpenAI | Sonnet 5.5 · Anthropic | DeepSeek V4.1 Flash · via OpenRouter |
|---|---|---|---|---|
| Accuracy with date + clarification tool | 25/25 | 24/25 | 25/25 | 25/25 |
| Median TTFT | **0.42 s** | 0.79 s | 0.84 s | 0.72 s |
| Cost per 1,000 reference calls | $10.35 (no cache price documented) | $8.60 cached / $23.00 uncached | $8.60 / $23.00 | ≈ **$1.01** / $3.36 at Fireworks' and Together's listed price, read 2026-09-26; OpenRouter adds 5.5% on credit |
| Data handling | inputs/outputs not retained | 30-day abuse logs; ZDR by approval | contradictory pages; ZDR via sales | zero retention enforced by OpenRouter; the serving host is not recorded |
| Retirement notice | none published (same-day removal precedent) | ≥ 6 months | ≥ 60 days | depends on the host; open weights |
| Adapter | OpenAI-compatible | Responses API, or reasoning off | Anthropic Messages | OpenAI-compatible |

---

## How to read this

**Criteria** are the ones agreed on 2026-09-26: knock-outs K1–K4, weighted criteria W1–W9, measured
criteria M1–M6. **One change since:** Jann asked to weigh the infrastructure he would have to set up
and maintain, so **W10 — setup & maintenance burden (8 points)** was added, funded by
W6 10→8, W7 10→8, W8 8→6 and W9 5→3.

| W10 score | Meaning |
|---|---|
| 1 | Cloud project/subscription with IAM, model-access approval, and non-static credentials (service-account token minting, SigV4, short-term keys) — breaks the single `ARIA_LLM_API_KEY` sealed-secret contract from the ARIA-40 brief |
| 3 | A cloud account and console setup (project, resource, deployment), then a static API key works |
| 5 | Sign up, create a static API key, call one fixed HTTPS host |

**Scoring rules**

- Points = weight × score ÷ 5. Maximum 100.
- A fact not documented on the pages read scores **1**.
- ⚖ marks a score that rests on judgement rather than a quoted fact.
- Time-limited prices are scored at the price that applies after the promotion ends.
- Prices in EUR or CHF are banded without conversion; no band boundary is close enough for the
  exchange rate to change a score.

**Knock-outs — one deviation from the agreed rule.** The agreed rule was that "not documented"
fails a knock-out. Applied literally, it would have ranked how much of each provider's documentation
could be read, not the providers: country lists and streaming-with-tools pages were simply not
retrievable for most hosts. So:

- ✗ = **documented** failure → knocked out.
- ✓ = documented pass.
- ? = not verified on the pages read → kept and scored, but must be checked before a key is bought.
- ⚖ = pass resting on interpretation.

**W2 reference call** (agreed): 8,000-token cached prefix (system prompt + tool catalogue), 2,000
uncached input tokens, 300 output tokens, priced per 1,000 calls. Where no cache-read price is
documented, the prefix is billed at the input rate.

| Band | 5 | 4 | 3 | 2 | 1 |
|---|---|---|---|---|---|
| Cost per 1,000 calls | ≤ 2 | 2–5 | 5–10 | 10–20 | > 20 |

**Coverage.** A research pass and direct reads covered every provider, but unevenly: several
(Vertex AI, Bedrock, Azure, Alibaba, SambaNova, Nebius) have many criteria at 1 because pages were
not read or not retrievable. Their low scores partly measure documentation reached, not product
quality. Nothing near the top of the ranking is in that state.

---

## Full matrix

| # | Provider | W1 Data (20) | W2 Cost (15) | W3 Stability (12) | W4 Schema (10) | W5 Stream (10) | W6 OpenAI-compat (8) | W7 Free (8) | W8 Multi-call (6) | W9 Context (3) | W10 Setup (8) | **Total** |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | Groq | 5⚖ | 5 | 3 | 3 | 3⚖ | 4⚖ | 3 | 5⚖ | 5 | 5 | **82.4** |
| 2 | OpenRouter | 5 | 5 | 2 | 3 | 3⚖ | 5 | 3⚖ | 5 | 5 | 5 | **81.6** |
| 3 | Fireworks AI | 5⚖ | 5 | 2 | 4⚖ | 4 | 4⚖ | 1 | 1 | 1 | 5 | **73.6** |
| 4 | Cerebras | 5⚖ | 4 | 3 | 4 | 1 | 3⚖ | 1 | 5 | 5 | 5 | **72.6** |
| 5 | OpenAI | 3 | 3 | 4⚖ | 4⚖ | 4 | 5 | 1 | 3 | 5 | 5 | **70.8** |
| 6 | OVHcloud AI Endpoints | 5⚖ | 5 | 2 | 3 | 1 | 4 | 3⚖ | 3 | 5 | 3⚖ | **70.4** |
| 7 | DeepInfra | 5 | 5 | 2 | 3 | 1 | 4 | 1 | 3 | 5 | 5 | **70.4** |
| 8 | Anthropic | 3⚖ | 3 | 3 | 4 | 4⚖ | 3 | 1 | 5 | 5 | 5 | **67.6** |
| 9 | Infomaniak | 5 | 4 | 2 | 3 | 1 | 3⚖ | 1 | 1 | 1 | 5 | **61.0** |
| 10 | Mistral | 3⚖ | 2 | 3 | 3 | 1 | 4⚖ | 2⚖ | 5 | 5 | 5 | **59.8** |
| 11 | Google Gemini API | 1 | 3 | 1 | 3⚖ | 4 | 3 | 2⚖ | 3 | 1 | 5 | **49.6** |
| 12 | Nebius Token Factory | 5⚖ | 1 | 2 | 3 | 1 | 1 | 1 | 1 | 5 | 1 | **44.8** |
| 13 | Scaleway | 1 | 5 | 2 | 3 | 1 | 4⚖ | 1 | 1 | 1 | 1 | **43.2** |
| 14 | Hugging Face Inference Providers | 1 | 5⚖ | 2 | 1 | 1 | 1 | 2 | 1 | 1 | 5⚖ | **42.4** |
| 15 | Google Vertex AI | 5⚖ | 1 | 1 | 3⚖ | 1 | 1 | 1 | 1 | 1 | 2⚖ | **41.6** |
| 16 | Cloudflare Workers AI | 1 | 4 | 2 | 1 | 1 | 3 | 2 | 1 | 1 | 3⚖ | **39.4** |
| 17 | Cohere | 3⚖ | 1 | 1 | 1 | 1 | 1 | 2⚖ | 1 | 1 | 5 | **36.0** |
| 18 | Amazon Bedrock | 5⚖ | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | **36.0** |
| 19 | GitHub Models | 1 | 1 | 1 | 1 | 1 | 1 | 3 | 1 | 1 | 5⚖ | **29.6** |
| 20 | SambaNova Cloud | 1 | 1 | 3 | 1 | 1 | 1 | 2 | 1 | 1 | 1 | **26.4** |
| 21 | Alibaba Model Studio | 1 | 1 | 1 | 1 | 1 | 3⚖ | 1 | 1 | 1 | 3⚖ | **26.4** |
| 22 | Azure AI Foundry | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 3⚖ | **23.2** |

**Excluded by Jann (2026-10-01): xAI.** It scored 67.6 and would have been 8th; it is not ranked and not a probe candidate. Its facts stay in [Paid services](#paid-services) for the record.

### Measured criteria

Filled in under [Probe results](#probe-results-aria-122), using the rubric pre-registered on
2026-09-26 (M1 35, M2 25, M3 15, M4 10, M5 15; M6 recorded only). M5 and M6 remain unmeasured.

### Knock-out status

| Provider | K1 Tools + streaming | K2 Usable from CH | K3 Terms | K4 No training | Tier status |
|---|---|---|---|---|---|
| Groq | ✓ | ? | ✓ | ✓⚖ | Free tier production-eligible but too small for daily use |
| OpenRouter | ✓ | ? | ✓ | ✓ | Free (`:free`) and paid |
| Fireworks AI | ✓ | ? | ✓ | ✓⚖ | Paid |
| Cerebras | ✓⚖ | ? | ✓ | ✓⚖ | Paid (trial credits only) |
| OpenAI | ✓ | ✓ | ✓ | ✓ | Paid |
| OVHcloud AI Endpoints | ✓ | ? | ✓ | ✓⚖ | Anonymous free access + paid |
| DeepInfra | ✓ | ? | ✓ | ✓ | Paid |
| xAI | ✓ | ? | ✓ | ✓ | **Excluded by Jann** |
| Anthropic | ✓ | ✓ | ✓ | ✓ | Paid |
| Infomaniak | ? | ? | ✓ | ✓ | Paid (one-off credits) |
| Mistral | ? | ? | ✓ | ✓⚖ | Free tier **probe-only** (evaluation/prototyping) + paid |
| Google Gemini API | ✓ | ✓ | ⚖ | ✓ | Free tier **probe-only** in CH + paid |
| Nebius Token Factory | ? | ? | ✓ | ✓ | Paid |
| Scaleway | ? | ? | ✓ | ? | Paid (1M-token allowance, reset undocumented) |
| Hugging Face Inference Providers | ? | ? | ✓ | ? | Free credits + paid |
| Google Vertex AI | ? | ? | ? | ? | Paid |
| Cloudflare Workers AI | ? | ? | ✓ | ✓ | Free daily allocation + paid |
| Cohere | ? | ? | ✓ | ✓ | Trial keys **probe-only** + paid |
| Amazon Bedrock | ? | ? | ✓ | ✓⚖ | Paid |
| GitHub Models | ? | ? | ✓ | ? | Free tier **probe-only** (experimentation) |
| SambaNova Cloud | ? | ? | ✓ | ✓⚖ | Free + paid |
| Alibaba Model Studio | ? | ? | ✓ | ? | Paid (90-day one-off quota) |
| Azure AI Foundry | ? | ? | ✓ | ✓ | Paid |

K3 was applied as "no clause found that forbids a personal assistant processing household data".
No API terms read address personal use positively, so a literal "terms must permit it" would fail
everyone.

---

## Knocked out

| Provider | Knock-out | Reason |
|---|---|---|
| **DeepSeek** | K4 ✗ | Its privacy policy states it uses personal data "to train and improve our technology, such as our machine learning models", with a "right to opt-out" whose mechanism is not documented as self-serve. Data is "collect[ed], process[ed] and store[d] … in People's Republic of China". The Open Platform terms are silent on training. (Otherwise strong on paper: 1M context, $0.15/$0.60 off-peak, OpenAI- and Anthropic-format endpoints.) |
| **NVIDIA API catalog** | K3 ✗, K4 ✗ | Trial terms: "for limited trial purposes only and without use of the API Service or Generated Content in production"; NVIDIA may use User Content to "improve NVIDIA products and services, including AI models". Production requires a separate paid subscription. |

### Excluded by the stakeholder

| Provider | Reason |
|---|---|
| **xAI** | Excluded at Jann's request on 2026-10-01. Not a documentation finding; the scores in the paid section are kept for reference only. |

---

## Free services

Providers with a recurring free API tier. "Probe-only" means the tier may be used for the ARIA-122
measurement, whose prompts are synthetic, but not to run ARIA.

### Groq — 82.4
- **Model:** `openai/gpt-oss-120b` (Production, 131,072 context). Sibling `openai/gpt-oss-20b`; `qwen/qwen3.8-27b` is Preview ("evaluation purposes only").
- **Free tier (recurring):** gpt-oss-120b 30 RPM, 1K RPD, 8K TPM, 200K TPD. A 10k-token reference call exceeds 8K TPM, so it fits the probe but not daily use.
- **Paid:** $0.15 in / $0.60 out per 1M; cache price not documented. Reference cost $1.68 / 1,000 calls.
- **Data use:** "By default, Groq does not retain customer data for inference requests"; logs up to 30 days only for troubleshooting/abuse; "All customers may enable Zero Data Retention (ZDR) in Data Controls settings". Data in US GCP. Training not addressed (⚖ — no retention means no training corpus).
- **Tools & streaming:** streamed tool calls arrive in `delta.tool_calls`; `parallel_tool_calls` shown; strict mode not mentioned. OpenAI compatibility at `https://api.groq.com/openai/v1`.
- **Stability:** no minimum deprecation notice documented.
- **Probe:** via `--base-url` (fits the probe's `/v1/chat/completions` suffix) once wired.

### OpenRouter — 81.6
- **Model:** router; scored on `gpt-oss-120b` routed only to ZDR endpoints. 131K context.
- **Free tier (recurring):** `:free` models, 20 RPM; 50 RPD below $10 of lifetime credit purchases, 1,000 RPD at or above. TPM not documented. Free variants' downstream providers may log; the training opt-out has a separate setting for free models (⚖ on production-eligibility).
- **Paid:** pass-through, "no markup on inference pricing"; credit purchase fee 5.5% ($0.80 minimum). At $0.15/$0.60 + fee: $1.77 / 1,000 calls. Cheaper gpt-oss endpoints are listed (from $0.03/$0.17) but their ZDR status was not checked.
- **Data use:** "OpenRouter itself has a ZDR policy; your prompts are not retained unless you specifically opt in to prompt logging"; ZDR enforceable "globally, per model group, per guardrail, or per request"; endpoints with unclear policies are assumed to retain and train. EU in-region routing enterprise-only.
- **Tools & streaming:** OpenAI-compatible at `https://openrouter.ai/api/v1`; `tools`, `tool_choice`, `parallel_tool_calls`; tool calls in streaming `delta`. Tool support depends on the downstream provider.
- **Probe:** via `--base-url` once wired.

### OVHcloud AI Endpoints — 70.4
- **Model:** `gpt-oss-120b`, 131k context.
- **Free:** anonymous access at 2 requests per minute per IP and model (⚖ — counted as a recurring free tier); authenticated 400 RPM.
- **Paid:** €0.08 in / €0.40 out. Reference cost €0.92 / 1,000 calls.
- **Data use:** "Data is not stored or shared during or after model use".
- **Tools & streaming:** function calling in streaming mode supported; `parallel_tool_calls: false` "not currently supported"; OpenAI-compatible at `https://oai.endpoints.kepler.ai.cloud.ovh.net/v1`.
- **Setup:** Public Cloud project + API key (⚖).

### Mistral — 59.8
- **Model:** `mistral-medium-3-5-26-04` (Mistral Medium 3.5, "optimized for agentic and coding use cases"), 256k context. Cheaper: Mistral Small 4 ($0.15/$0.6), Mistral Large 3 ($0.5/$1.5).
- **Free tier (recurring monthly) — probe-only:** "intended for evaluation and prototyping purposes"; numeric limits in the console only. Free mode "may use your data … to train", with a right to opt out.
- **Paid:** $1.5 / $7.5; no cache price documented. $17.25 / 1,000 calls.
- **Data use:** input/output kept "for thirty (30) rolling days to monitor abuse (unless zero data retention is activated)"; ZDR only on pay-as-you-go, on request, approved "at our discretion"; data hosted in the EU by default. Paid training: "Customers retain full control … and have the right to opt out" (⚖ — reads as opt-out, not off by default).
- **Tools:** `parallel_tool_calls` true/false; streaming tool calls **not documented** (K1 ?). OpenAI SDK usable by switching base URL to `https://api.mistral.ai/v1`.

### Google Gemini API — 49.6
- **Model:** `gemini-3.8-flash` (Stable).
- **Free tier (recurring) — probe-only in Switzerland:** "You may use only Paid Services when making API Clients available to users in the European Economic Area, Switzerland, or the United Kingdom." In CH, paid-tier data terms apply to free quota too. Limits shown only in AI Studio.
- **Paid:** $0.75 / $3.75, cache $0.075 **until 2026-12-31**, then $1.50 / $7.50 / $0.15. Scored post-promotion: $6.45 / 1,000 calls (promo: $3.23).
- **Data use:** no training on paid; abuse-monitoring retention 55 days [excerpt]; self-serve ZDR controls pointed to the Enterprise Agent Platform (Vertex), not AI Studio.
- **Terms (K3 ⚖):** "for developers building with Google AI models for professional or business purposes, not for consumer use"; 18+. Whether a personal household assistant qualifies is a legal reading this brief does not make.
- **Tools & streaming:** "supported JSON Schema subset"; `validated` mode; parallel function calling; streamed calls documented on the Interactions API (`step.start` → `arguments_delta` → `step.stop`) — the probe's `generateContent` path is not covered by that page. OpenAI compatibility "still in beta".

### Cloudflare Workers AI — 39.4
- **Model:** `@cf/openai/gpt-oss-120b`, $0.35 / $0.75. Free: 10,000 Neurons/day (≈ 29 reference calls/day; enough for a probe run).
- **Data use:** does not train on Customer Content without consent; retention not addressed.
- **Tools:** tools on the OpenAI-compatible endpoint not documented; the Responses API supports only `stream: false` for GPT-OSS.

### Others with a free tier, lower scores
- **Hugging Face Inference Providers — 42.4.** $0.10/month free credits (resets monthly); pass-through pricing, no markup. HF does not store request bodies; no documented way to restrict routing to zero-retention providers (W1 = 1).
- **Cohere — 36.0.** Trial keys free but "restricted from production use" (probe-only). Trains on prompts by default with a self-serve "Data Controls" opt-out; 30-day retention; ZDR on request. Command A price not on the page read.
- **GitHub Models — 29.6.** Free usage "in public preview", "intended to help you get started with experimentation" (probe-only); High-tier models 10 RPM, 50 RPD, 8,000 input tokens per request. Data use not documented (K4 ?).
- **SambaNova Cloud — 26.4.** Free tier for gpt-oss-120b: 20 RPM, **20 RPD**, 200K TPD (smaller than one 25-call probe run). Terms exclude Customer Content from usage data; retention not documented.

---

## Paid services

No recurring free tier, or trial credits only.

### Fireworks AI — 73.6
- **Model:** `gpt-oss-120b`. Context not stated on the pages read.
- **Price:** $0.15 / $0.015 cached / $0.60 → **$0.60 / 1,000 calls**. $1 one-off credit.
- **Data use:** "Fireworks does not log or store prompt or generation data for any open models, without explicit user opt-in."
- **Probe:** OpenAI-compatible base URL `https://api.fireworks.ai/inference/v1` (fits the probe's suffix rule once `--base-url` is wired).
- **Tools & streaming:** OpenAI-compatible tool specs; `tool_choice` auto/none/required; "Tool calls work with streaming responses, with arguments sent incrementally"; JSON Schema with `$defs`, `$ref`, recursive types. Parallel calls not documented.

### Cerebras — 72.6
- **Model:** `gpt-oss-120b` (production), 131k context on paid tiers. Price $0.35 / $0.75 → $3.73 / 1,000 calls.
- **Probe:** `https://api.cerebras.ai/v1` (fits the probe's suffix rule once `--base-url` is wired).
- **Trial:** $5, expires after 30 days; "Is there a permanently free tier? No".
- **Data use:** "We do not retain inputs and outputs associated with our training, inference and chatbot Services." Training not addressed (⚖).
- **Tools:** strict mode "guarantees that tool-call arguments conform to the schema" for the supported subset; `parallel_tool_calls`. Streaming of tool calls not shown explicitly (K1 ⚖). Known model limitation: "may call tools that aren't directly specified".

### OpenAI — 70.8
- **Model:** `gpt-6-sol` ("Built to power complex coding and agentic workflows"), 1.05M context. Siblings: `gpt-6-luna` ($0.10/$0.01/$0.50), `gpt-6-astra` ($10/$1/$50).
- **Price:** $2 / $0.20 cached / $10 → $8.60 / 1,000 calls. Caching automatic; 1,024-token minimum; 30-minute default TTL; cache writes 1.25× input for GPT-5.6 and later.
- **Data use:** "data sent to the OpenAI API is not used to train or improve OpenAI models"; abuse logs up to 30 days; ZDR and EU residency (`eu.api.openai.com`, EEA + Switzerland) require approval.
- **Stability:** GA models get "at least 6 months" notice; no dated snapshot IDs shown for GPT-6 (⚖).
- **Tools:** strict mode (conditional schemas `allOf/anyOf/oneOf` unsupported per the page read ⚖); streamed tool-call start, argument-delta and done events; several calls per response. No disable parameter found on the pages read (W8 = 3).

### DeepInfra — 70.4
- **Model:** `openai/gpt-oss-120b`, 131,072 context. $0.037 / $0.17 → **$0.42 / 1,000 calls**, the lowest cost scored.
- **Data use:** inputs "not stored to disk", outputs deleted once returned; no training "except when using Google or Anthropic models".
- **Tools:** parallel calls ("quality may vary"), `tool_choice` auto/none, streaming mode. OpenAI base URL `https://api.deepinfra.com/v1/openai` — **does not fit the probe's suffix rule** (see Probe gaps).

### xAI — 67.6 · excluded by Jann (2026-10-01), not ranked
- **Model:** `grok-4.7`, 500k context. $2 / $6; cached price not documented → $21.80 / 1,000 calls (W2 = 1).
- **Data use:** "never trains on your API inputs or outputs without your explicit permission"; 30-day retention; **ZDR self-serve** from the Console.
- **Tools:** `parallel_tool_calls: false` available; "the function call is returned in whole in a single chunk"; schemas compiled to a tool-call grammar (⚖ strict). OpenAI-compatible at `https://api.x.ai/v1`, but Chat Completions is labelled "Legacy".

### Anthropic — 67.6
- **Model:** `claude-sonnet-5`, a fixed snapshot ("Anthropic does not update the weights or configuration of an existing model ID"), 1M context. Siblings Haiku 4.5 ($1/$5), Opus 5.5 ($4/$20).
- **Price:** $2 / $0.20 cache hit / $10 → $8.60 / 1,000 calls; 5-minute cache write 1.25×, 1-hour 2×; 1,024-token minimum.
- **Data use:** no training by default; retention is self-contradictory across Anthropic's own pages (see below); ZDR via sales.
- **Stability:** "at least 60 days' notice before model retirement".
- **Tools:** `strict: true` with grammar-constrained sampling; incremental `input_json_delta` streaming; parallel by default with `disable_parallel_tool_use`. The OpenAI compatibility layer is "not considered a long-term or production-ready solution" and ignores `strict`.

### Infomaniak — 61.0
- **Model:** scored on `Qwen/Qwen3.5-122B-A10B-FP8`, CHF 0.40 / 3.20 → CHF 4.96 / 1,000 calls (⚖ choice; Mistral-Small-4 at CHF 0.20/0.75 bands the same). Also hosts `swiss-ai/Apertus-v1.5-70B`.
- **Credits:** "Get started with 1M free credits" (one-off); credit card required.
- **Data use:** LLM API terms: does "not use transmitted data (prompts, response content) to train its own AIs"; data stored only "for the duration necessary to process the Client's request". Swiss-only processing is stated in Infomaniak's general AI guide, not in the API terms read.
- **Tools:** function calling; OpenAI-style path `/2/ai/{product_id}/openai/v1/chat/completions`; streaming with tools not documented (K1 ?).

### Lower-scoring paid providers
- **Nebius Token Factory — 44.8.** gpt-oss-120b "From $0.15 / 1M input"; output price not readable (pricing page blocked by robots.txt). No training; ZDR an optional setting; public endpoints have "no region guarantee".
- **Scaleway — 43.2.** gpt-oss-120b €0.15 / €0.60; "free tier on the first 1,000,000 tokens", reset not documented. Data-privacy page body could not be retrieved on either date (K4 ?).
- **Google Vertex AI — 41.6.** Self-serve ZDR controls referenced from Google's Gemini ZDR page [excerpt]; Vertex pages could not be retrieved. Requires a Google Cloud project (W10 2⚖).
- **Amazon Bedrock — 36.0.** Data retention `mode: "none"` gives zero retention, self-serve; prompts not shared with model providers. Long-term API keys are "for exploration only. For production, use short-term keys" (W10 = 1).
- **Alibaba Model Studio — 26.4.** 1M tokens per model, valid 90 days, Singapore/International only, non-recurring. Data use not read.
- **Azure AI Foundry — 23.2.** Prompts "NOT used to train"; abuse-monitoring retention period not stated on the page read; modified abuse monitoring by application. Regional deployments keep processing in the chosen geography.

---

## Anthropic judgement calls

| Criterion | Score | Why it is judgement |
|---|---|---|
| W1 Data | 3 ⚖ | The platform page says conversation content "is not retained by default"; Anthropic's Privacy Center says API inputs/outputs are deleted "within 30 days". Scored on the 30-day reading. **On the other reading W1 = 5 and Anthropic totals 75.6 — 3rd place.** |
| W5 Streaming | 4 ⚖ | Incremental tool input is documented; text-then-tool interleaving was not confirmed on a page read. Same rule applied to OpenAI and Gemini. |

Every other Anthropic score quotes a page read on 2026-09-26.

---

## Probe gaps

Recorded here so the probe work Jann planned later has the facts in one place. No change was made
to the script.

- **`--base-url` reaches only the Ollama adapter.** Four of the top five need it on the OpenAI
  adapter, plus a key variable.
- **The OpenAI adapter appends `/v1/chat/completions`** after stripping a trailing `/v1`. That fits
  Groq, OpenRouter, Fireworks, Cerebras, OVHcloud, Mistral, Scaleway, Cloudflare and Infomaniak. It does **not** fit
  DeepInfra (`…/v1/openai`) or Gemini's compatibility layer (`…/v1beta/openai/`).
- **Send a User-Agent.** Groq (behind Cloudflare) refuses Python's default `Python-urllib` agent with
  HTTP 403 `error code: 1010`.
- **Add today's date to the system prompt**, so date handling (Probe results, point 5) becomes measurable.
- **A Responses API adapter** is needed to probe OpenAI models whose Chat Completions endpoint rejects tools
  with reasoning on (GPT-6 Astra, GPT-6.1 Sol; GPT-6 Sol ran only with reasoning off).
- **TTFT (M5) and stream shape (M6) cannot be measured for hosted providers** until the probe reads
  SSE streams. For the gpt-oss hosts, latency is the main thing that differs between them.

---

## Open items this brief does not settle

- **Gemini "not for consumer use".** Whether ARIA counts is a reading of Google's terms, not a fact.
- **Anthropic's retention contradiction.** Two Anthropic pages disagree; worth a support question
  before relying on either.
- **Which reading of `missing_arg` counts** (Probe results). Jann to decide.
- **Date, time and time zone in every Agent Core request** — a consequence of Probe results point 5;
  belongs with the Agent Core work, not decided here.
- **A clarification path for missing arguments** (Probe results point 4) — input to ARIA-121.
- **Switzerland availability (K2)** for every provider except OpenAI, Anthropic and Gemini.
- **`docs/04-deployment.md`** says no conversation or knowledge-base content leaves except as
  ciphertext, while listing the hosted LLM call — which carries both in plaintext to the provider —
  as crossing the boundary. Raised on 2026-09-26; not edited here.

---

## Sources

Read **2026-09-26** unless marked **(2026-10-01)**. [excerpt] = read from the search engine's
excerpt of the provider's page, not the full page.

**Anthropic** — [pricing](https://platform.claude.com/docs/en/about-claude/pricing) ·
[API and data retention](https://platform.claude.com/docs/en/manage-claude/api-and-data-retention) ·
[Privacy Center 7996866](https://privacy.claude.com/en/articles/7996866) ·
[training policy](https://privacy.claude.com/en/articles/7996868-is-my-data-used-for-model-training) [excerpt] ·
[models overview](https://platform.claude.com/docs/en/about-claude/models/overview) ·
[model IDs and versions](https://platform.claude.com/docs/en/about-claude/models/model-ids-and-versions) ·
[deprecations](https://platform.claude.com/docs/en/about-claude/model-deprecations) ·
[strict tool use](https://platform.claude.com/docs/en/agents-and-tools/tool-use/strict-tool-use) [excerpt] ·
[structured outputs](https://platform.claude.com/docs/en/build-with-claude/structured-outputs) ·
[fine-grained tool streaming](https://platform.claude.com/docs/en/agents-and-tools/tool-use/fine-grained-tool-streaming) [excerpt] ·
[parallel tool use](https://platform.claude.com/docs/en/agents-and-tools/tool-use/parallel-tool-use) ·
[prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching) [excerpt] ·
[OpenAI SDK compatibility](https://platform.claude.com/docs/en/api/openai-sdk) ·
[supported countries](https://www.anthropic.com/supported-countries)

**OpenAI** — [pricing](https://developers.openai.com/api/docs/pricing) ·
[models](https://developers.openai.com/api/docs/models) ·
[your data](https://developers.openai.com/api/docs/guides/your-data) ·
[supported countries](https://developers.openai.com/api/docs/supported-countries) ·
[deprecations](https://developers.openai.com/api/docs/deprecations) ·
[function calling](https://developers.openai.com/api/docs/guides/function-calling) ·
[prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching)

**Google Gemini API** — [terms](https://ai.google.dev/gemini-api/terms) ·
[pricing](https://ai.google.dev/gemini-api/docs/pricing) ·
[rate limits](https://ai.google.dev/gemini-api/docs/rate-limits) ·
[available regions](https://ai.google.dev/gemini-api/docs/available-regions) ·
[models](https://ai.google.dev/gemini-api/docs/models) ·
[function calling](https://ai.google.dev/gemini-api/docs/function-calling) ·
[OpenAI compatibility](https://ai.google.dev/gemini-api/docs/openai) ·
[caching](https://ai.google.dev/gemini-api/docs/caching) ·
[usage policies](https://ai.google.dev/gemini-api/docs/usage-policies) [excerpt] ·
[ZDR](https://ai.google.dev/gemini-api/docs/zdr) [excerpt] ·
[streaming](https://ai.google.dev/gemini-api/docs/streaming) (2026-10-01)

**Mistral** — [Medium 3.5 model card](https://docs.mistral.ai/models/model-cards/mistral-medium-3-5-26-04) ·
[API pricing](https://mistral.ai/pricing/api/) ·
[function calling](https://docs.mistral.ai/capabilities/function_calling) ·
[training on user data](https://help.mistral.ai/en/articles/347617-do-you-use-my-user-data-to-train-your-artificial-intelligence-models) ·
[ZDR](https://help.mistral.ai/en/articles/347612-can-i-activate-zero-data-retention-zdr) ·
[data location](https://help.mistral.ai/en/articles/347629-where-do-you-store-my-data-or-my-organization-s-data) ·
[privacy policy](https://legal.mistral.ai/terms/privacy-policy/) ·
[migration guides](https://docs.mistral.ai/resources/migration-guides) ·
[rate limits](https://help.mistral.ai/en/articles/424390-how-do-api-rate-limits-work-and-how-do-i-increase-them%C2%A0) [excerpt]

**xAI** — [models](https://docs.x.ai/developers/models) ·
[security FAQ](https://docs.x.ai/developers/faq/security) ·
[function calling](https://docs.x.ai/developers/tools/function-calling) (2026-10-01)

**DeepSeek** — [pricing](https://api-docs.deepseek.com/quick_start/pricing/) ·
[Open Platform terms](https://cdn.deepseek.com/policies/en-US/deepseek-open-platform-terms-of-service.html) ·
[privacy policy](https://cdn.deepseek.com/policies/en-US/deepseek-privacy-policy.html)

**Groq** — [your data](https://console.groq.com/docs/your-data) ·
[rate limits](https://console.groq.com/docs/rate-limits) ·
[models](https://console.groq.com/docs/models) ·
[tool use](https://console.groq.com/docs/tool-use) ·
[OpenAI compatibility](https://console.groq.com/docs/openai) ·
[deprecations](https://console.groq.com/docs/deprecations) ·
[local tool calling](https://console.groq.com/docs/tool-use/local-tool-calling) (2026-10-01)

**Cerebras** — [rate limits / free tier](https://inference-docs.cerebras.ai/support/rate-limits) ·
[tool use](https://inference-docs.cerebras.ai/capabilities/tool-use) ·
[privacy policy](https://www.cerebras.ai/privacy-policy) ·
[gpt-oss model page](https://inference-docs.cerebras.ai/models/openai-oss) (2026-10-01) ·
[quickstart](https://inference-docs.cerebras.ai/quickstart) (2026-10-01)

**Together AI** — [pricing](https://www.together.ai/pricing) ·
[privacy and security](https://docs.together.ai/docs/privacy-and-security) ·
[function calling](https://docs.together.ai/docs/function-calling) ·
[OpenAI compatibility](https://docs.together.ai/docs/openai-api-compatibility) (2026-10-01)

**Fireworks AI** — [pricing](https://fireworks.ai/pricing) ·
[serverless pricing](https://docs.fireworks.ai/serverless/pricing) ·
[data handling](https://docs.fireworks.ai/guides/security_compliance/data_handling) ·
[function calling](https://docs.fireworks.ai/guides/function-calling) (2026-10-01) ·
[quickstart](https://docs.fireworks.ai/getting-started/quickstart) (2026-10-01)

**DeepInfra** — [gpt-oss-120b](https://deepinfra.com/openai/gpt-oss-120b) ·
[data privacy](https://docs.deepinfra.com/account/data-privacy) ·
[tool calling](https://docs.deepinfra.com/chat/tool-calling)

**SambaNova** — [rate limits](https://docs.sambanova.ai/docs/en/models/rate-limits) ·
[terms of service](https://sambanova.ai/cloud-end-user-license-agreement)

**Nebius** — [legal quick guide](https://docs.tokenfactory.nebius.com/legal/legal-quick-guide) ·
[gpt-oss page](https://nebius.com/services/token-factory/models/openai-gpt-oss) ·
pricing page blocked by robots.txt

**Scaleway** — [pricing](https://www.scaleway.com/en/pricing/model-as-a-service/) ·
[Generative APIs reference](https://www.scaleway.com/en/developers/api/generative-apis) ·
[data privacy](https://www.scaleway.com/en/docs/generative-apis/reference-content/data-privacy/) — body not retrievable (2026-09-26, 2026-10-01)

**OVHcloud** — [capabilities](https://docs.ovhcloud.com/en/guides/public-cloud/ai-machine-learning/ai-endpoints-capabilities) ·
[gpt-oss-120b](https://www.ovhcloud.com/en/public-cloud/ai-endpoints/catalog/gpt-oss-120b/) ·
[function calling (FR)](https://docs.ovhcloud.com/fr/guides/public-cloud/ai-machine-learning/ai-endpoints-function-calling) (2026-10-01)

**Infomaniak** — [AI Services prices](https://www.infomaniak.com/en/hosting/ai-services/prices) ·
[LLM API terms](https://welcome.infomaniak.com/api/components/cgu/latest?id=87&locale=en_GB) ·
[AI services guide](https://www.infomaniak.com/en/support/faq/2845/getting-started-guide-ai-services-sovereign-ai-services) [excerpt]

**Hugging Face** — [pricing](https://huggingface.co/docs/inference-providers/pricing) ·
[security](https://huggingface.co/docs/inference-providers/security)

**Cloudflare** — [pricing](https://developers.cloudflare.com/workers-ai/platform/pricing/) ·
[data usage](https://developers.cloudflare.com/workers-ai/platform/data-usage/) ·
[OpenAI compatibility](https://developers.cloudflare.com/workers-ai/configuration/open-ai-compatibility/)

**OpenRouter** — [ZDR](https://openrouter.ai/docs/guides/features/zdr) ·
[provider logging](https://openrouter.ai/docs/guides/privacy/provider-logging) ·
[limits](https://openrouter.ai/docs/api_reference/limits) ·
[API overview](https://openrouter.ai/docs/api-reference/overview) ·
[tool calling](https://openrouter.ai/docs/guides/features/tool-calling) ·
[gpt-oss-120b providers](https://openrouter.ai/openai/gpt-oss-120b) (2026-10-01) ·
[FAQ — fees](https://openrouter.ai/docs/faq) (2026-10-01)

**GitHub Models** — [prototyping with AI models](https://docs.github.com/en/github-models/use-github-models/prototyping-with-ai-models)

**NVIDIA** — [API Trial Terms of Service](https://assets.ngc.nvidia.com/products/api-catalog/legal/NVIDIA%20API%20Trial%20Terms%20of%20Service.pdf)

**Cohere** — [pricing](https://cohere.com/pricing) · [data usage policy](https://cohere.com/data-usage-policy)

**Alibaba Cloud** — [new-user free quota](https://www.alibabacloud.com/help/en/model-studio/new-free-quota)

**Amazon Bedrock** — [data protection](https://docs.aws.amazon.com/bedrock/latest/userguide/data-protection.html) ·
[API keys](https://docs.aws.amazon.com/bedrock/latest/userguide/api-keys.html) ·
[data retention](https://docs.aws.amazon.com/bedrock/latest/userguide/data-retention.html) (2026-10-01)

**Azure** — [data, privacy and security](https://learn.microsoft.com/en-us/azure/foundry/responsible-ai/openai/data-privacy) (2026-10-01)

**Google Vertex AI** — [API keys](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/start/api-keys) and
[Gemini 3.8 Flash](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/gemini/3-8-flash) — bodies not retrievable

*Prices, model IDs and free tiers move. Re-read the page for any figure before acting on it.*

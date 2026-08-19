# ARIA-109 — Which Ollama model (measured)

[Library index](../README.md) · [Spikes](README.md) · [Agent Core](../services/agent-core.md) · [Sprint 1](../sprints/sprint-01.md)

**Status: measured 2026-08-19. Recommendation ready. One finding needs a decision that is bigger
than this story.**

Evidence: `probe-results.json` / `probe-summary.md` from
[`../../scripts/aria-llm-probe.py`](../../scripts/aria-llm-probe.py). This is the story that was
created because "which Ollama model" was buried in an implementation story with nothing measuring
it. It now has numbers.

---

## Setup

| | |
|---|---|
| Host | RTX 2070, 8 GB · Ollama in LXC CT 200 on Proxmox `prox2` |
| Endpoint | native `/api/chat`, `think: false` |
| Battery | 5 MCP-style tool schemas × 5 prompt cases × 5 repeats = **25 samples per model** |
| Date | 2026-08-19 |

---

## Results

| Model | Selection acc. | Malformed | Spurious | Hallucinated arg | Median latency |
|---|---|---|---|---|---|
| **`qwen3:8b`** | **96%** (24/25) | **0%** | **0%** (0/5) | 100% (4/4) | 1.33 s |
| `llama3.1:8b` | 88% (22/25) | 4% | **60%** (3/5) | 100% (5/5) | 1.24 s |
| `qwen2.5-coder:7b` | 20% (5/25) | 0% | 0% | n/a | 1.32 s |

### Reading them

**`qwen3:8b` — recommended.** One miss in 25, and it was a *conservative* miss: on the
missing-argument prompt it declined to call anything, which is the right instinct. Zero malformed
arguments, zero spurious calls.

**`llama3.1:8b` — rejected on the spurious rate.** 88% selection looks close to qwen3, and with 25
samples that 8-point gap is roughly two runs — not a real separation. The disqualifying number is
elsewhere: on *"Why is the sky blue?"* it called `knowledge_lookup` **3 times out of 5**. For a
voice assistant that is the failure a user actually notices — you ask a plain question and the
assistant goes rummaging through the household knowledge base. It also produced the only malformed
argument set in the run.

**`qwen2.5-coder:7b` — confirmed negative control.** It emitted **no tool call in any of the 20
tool-requiring runs**. Its 20% is entirely the no-tool case, where doing nothing happens to be
correct. Included deliberately to check the battery could detect a model that cannot tool-call at
all; it did.

### Honest limits on this data

- **25 samples per model.** Enough to separate qwen2.5-coder from the other two decisively, and
  enough to make the 0/5 vs 3/5 spurious split worth acting on. **Not** enough to call 96% vs 88%
  a real quality difference.
- **`think: false` throughout.** Qwen3's reasoning mode was off. It may well handle the
  missing-argument case better with thinking on — but reasoning before every tool call costs
  latency on a voice path, so that is a trade to measure, not assume. Worth one follow-up run.
- Model-swap cost is visible: the first call after each model change took **13–14 s**, because
  `OLLAMA_MAX_LOADED_MODELS=1` forces an unload/load cycle.

---

## Streaming behaviour — measured 2026-08-19 (second run)

The first battery was non-streaming, which left ARIA-109's D37 criterion at zero. It is now
measured. **Tool turns and prose turns behave completely differently**, so averaging them would
have hidden the finding:

| Model | Turn type | Median TTFT | Median chunks | Incremental? |
|---|---|---|---|---|
| `qwen3:8b` | prose | **0.18 s** | 76 | yes |
| `qwen3:8b` | tool call | 0.95 s | **2** | no — single chunk |
| `llama3.1:8b` | prose | **0.24 s** | 74 | yes |
| `llama3.1:8b` | tool call | 0.93 s | **2** | no — single chunk |

Two chunks means the content plus the `done` marker. **The tool call arrives complete, in one
piece, after the model has finished deciding.** Tool arguments are **not fragmented** — Ollama
sends whole objects, unlike OpenAI's partial-string deltas.

### What it means for D37

D37 made `Decide` server-streaming so synthesis could start on the first sentence. Against that
intent:

- **Prose turns — D37 delivers exactly what it promised.** 0.18 s to first token against a ~2.3 s
  full response is a real ~10× latency win on the speech path.
- **Tool-call turns — streaming buys nothing.** There is no partial tool call worth emitting. This
  is not a defect; it is what a tool call is.

**D37 is not weakened, but its scope is now known.** The streaming design is justified by the prose
path; the tool path must be judged on *total* latency rather than time-to-first-token. Anyone
sizing the tool loop (ARIA-61) should plan against ~1 s per tool-calling turn locally, not against
a TTFT figure.

**Consequence for ARIA-54:** because arguments arrive complete and unfragmented, the Ollama
streaming adapter is simple — no accumulation state machine. A hosted backend that fragments them
will need one, which is a concrete asymmetry the `LlmBackend` trait must absorb, and further
evidence for D49.

### A metric of mine that was flattering itself

The first version of this harness reported "tool call arrives before stream end" at 100%, because
the call sat at chunk 0 of 2 and chunk 0 is not the last one. Technically true, and meaningless.
**Chunk count is the honest measure** and the harness now leads with it. Recorded because a metric
that quietly agrees with the design it is testing is worse than no metric.

---

## Spurious-call evidence, strengthened

The model separation now rests on three independent passes rather than one:

| Pass | `qwen3:8b` spurious | `llama3.1:8b` spurious |
|---|---|---|
| Run 1, non-streaming | 0/5 | 3/5 |
| Run 2, non-streaming | 0/5 | 2/5 |
| Run 2, streaming | 0/5 | 3/5 |
| **Total** | **0/15** | **8/15 (53%)** |

Selection accuracy also moved (qwen3 96% → 100%, llama 88% → 92% on run 2), which is exactly the
run-to-run wobble that made me refuse to treat that gap as a separation. **The spurious split did
not wobble.** That is what the recommendation rests on.

---

## ⚠️ The finding that matters more than the model choice

> **Both capable models hallucinated a required argument 100% of the time.**

The prompt was *"Put on some music."* — no room named. Both `qwen3:8b` and `llama3.1:8b` called
`music_play` with an **invented `room`**, on essentially every run. The system prompt said, in
those words, *"Do not invent argument values that the user did not supply."* It made no difference.

This is not a model defect to be solved by picking a better model. It is a **design constraint on
ARIA**, and it has physical consequences: `music_play(room="living room")` plays music in the
wrong room; the same failure shape on a light, a lock, or a calendar write is worse. ARIA's whole
premise is tools with real-world side effects.

**The Agent Core cannot rely on the model to decline when an argument is missing.** Something
structural has to catch it. Three candidate mechanisms, none yet chosen:

1. **MCP elicitation.** The protocol has an elicitation capability for exactly this — a server
   asking the user for more information mid-call. `rmcp` exposes it behind an `elicitation` feature
   flag (see the [ARIA-65 brief](ARIA-65-rust-mcp-sdk.md)). This is the most principled fit.
2. **Schema design.** Make `room` optional with a per-user default resolved from the Knowledge
   Core (a typed preference under D16 — "default speaker"). Removes the guess by removing the gap.
3. **Registry-side validation.** The Registry already gates every call; it could reject a call
   whose required argument has no support in the conversation. Weakest of the three — "no support
   in the conversation" is not something the Registry can evaluate.

**Note how closely this rhymes with D42.** Conversational consent already exists: a call returns
`consent required`, the Agent Core asks in conversation, and the original call is retried
automatically. A missing argument wants the *same machine* — return `information required`, ask,
retry. If that reading holds, this is a modest extension of a mechanism already designed, not new
architecture. That is the argument to test first.

**This needs a decision and a story.** Neither exists. Recommend raising it against the Agent Core
and MCP Registry epics, cross-referencing ARIA-61 (tool loop) and ARIA-118 (`consent required`
path).

---

## Recommendation

**`qwen3:8b` as the Ollama fallback model**, on the spurious-call rate and the conservative miss,
not on the selection-accuracy difference.

Rejected: **`llama3.1:8b`** — indistinguishable on selection at this sample size, but called a tool
on a no-tool question 60% of the time and produced the run's only malformed arguments.
**`qwen2.5-coder:7b`** — cannot tool-call; a code model included as a control.

### Consequences to carry forward

- **This does not reopen D35.** Automatic hosted-vs-Ollama routing stays deferred. That comparison
  needs the *same* battery run against a hosted provider, which has not happened — the ARIA-40
  brief has the harness ready for it.
- **ARIA-50's conformance suite has a candidate that passes.** The suite must exercise streaming
  and interleaved tool-call events; this battery is non-streaming, so it is necessary but not
  sufficient evidence.
- **The hallucinated-argument finding applies to the hosted backend too** until measured
  otherwise. Do not assume a frontier model is immune — measure it with the same battery.

## What is needed to close ARIA-109

- [x] Battery run against candidate local models with recorded evidence
- [ ] Attach `probe-summary.md` and `probe-results.json` to ARIA-109
- [ ] One follow-up run with `--think` to price Qwen3's reasoning mode against the latency budget
- [ ] Record the decision as a D-entry with the rejected models
- [ ] **Raise the hallucinated-argument problem as its own decision item** — see above
- [ ] Move the item out of [`../open-questions/awaiting-measurement.md`](../open-questions/awaiting-measurement.md)

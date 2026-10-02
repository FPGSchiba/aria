# ARIA-40 / ARIA-109 — tool-calling probe results

Generated 2026-10-02 09:39:35

| Provider | Model | Selection acc. | Malformed | Spurious (no-tool case) | Hallucinated arg | Median latency |
|---|---|---|---|---|---|---|
| openai | `gpt-6-sol` | 80% | 0% | 0% | 0% | 1.17s |

**Selection accuracy** — called exactly the expected tool set (for the ambiguous case,
picking exactly one of the two defensible tools counts as correct).
**Malformed** — at least one call had arguments failing schema validation.
**Spurious** — called a tool on the question where none applies.
**Hallucinated arg** — supplied `room` for `music_play` when the prompt never said one.

Attach this file and `probe-results.json` to ARIA-40 (or ARIA-109) as the evidence the
story's acceptance criteria require.

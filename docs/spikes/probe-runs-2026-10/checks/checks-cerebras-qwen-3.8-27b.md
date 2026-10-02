# ARIA-122 follow-up checks — cerebras · `qwen-3.8-27b`

Generated 2026-10-02 10:50. Repeats per case: 5. Reasoning sent: `not set`. Date line: *Current date and time: Friday, 2026-10-02 10:28, time zone Europe/Zurich (UTC+02:00).*

| Variant | Runs ok | Correct | `missing_arg`: refused / asked | `multi` | Malformed | Nulls stripped | Spurious | Made-up room | Date correct | Median total | Median TTFT | Median first tool | Tool args in pieces |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| dated | 25/25 | 20/25 | 5 / 0 | 5/5 | 0 | 0 | 0 | 0 | 5/5 | 0.53 s | — | — | — |
| clarify | 25/25 | 25/25 | 0 / 5 | 5/5 | 0 | 0 | 0 | 0 | 5/5 | 0.46 s | — | — | — |
| strict | 25/25 | 20/25 | 5 / 0 | 5/5 | 0 | 5 | 0 | 0 | 5/5 | 0.47 s | — | — | — |
| stream | 25/25 | 20/25 | 5 / 0 | 5/5 | 0 | 0 | 0 | 0 | 5/5 | 0.49 s | 0.42 s | 0.46 s | 0/15 |

**Correct** — same rule as the probe. In `clarify`, calling `request_information` is the expected answer to `missing_arg`. **Date correct** — of the `ambiguous` runs that made a call, how many wrote tomorrow at 15:00. **Nulls stripped** — `strict` only: null optional arguments removed before validation. **TTFT** — first streamed chunk carrying text or a tool call. **Tool args in pieces** — streamed runs whose tool arguments arrived in more than one chunk.

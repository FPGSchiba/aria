# ARIA-122 follow-up checks — cerebras · `qwen-3.8-27b` (noreason)

Generated 2026-10-02 11:03. Repeats per case: 5. Reasoning sent: `none`. Date line: *Current date and time: Friday, 2026-10-02 10:51, time zone Europe/Zurich (UTC+02:00).*

| Variant | Runs ok | Correct | `missing_arg`: refused / asked | `multi` | Malformed | Nulls stripped | Spurious | Made-up room | Date correct | Median total | Median TTFT | Median first tool | Tool args in pieces |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| dated | 25/25 | 18/25 | 3 / 0 | 4/5 | 0 | 0 | 0 | 2 | 3/5 | 0.35 s | — | — | — |
| stream | 25/25 | 19/25 | 4 / 0 | 5/5 | 0 | 0 | 0 | 1 | 3/3 | 0.34 s | 0.30 s | 0.33 s | 0/14 |

**Correct** — same rule as the probe. In `clarify`, calling `request_information` is the expected answer to `missing_arg`. **Date correct** — of the `ambiguous` runs that made a call, how many wrote tomorrow at 15:00. **Nulls stripped** — `strict` only: null optional arguments removed before validation. **TTFT** — first streamed chunk carrying text or a tool call. **Tool args in pieces** — streamed runs whose tool arguments arrived in more than one chunk.

# ARIA-122 follow-up checks — openrouter · `deepseek/deepseek-v4.1-flash`

Generated 2026-10-02 11:30. Repeats per case: 5. Reasoning sent: `not set`. Date line: *Current date and time: Friday, 2026-10-02 11:21, time zone Europe/Zurich (UTC+02:00).*

| Variant | Runs ok | Correct | `missing_arg`: refused / asked | `multi` | Malformed | Nulls stripped | Spurious | Made-up room | Date correct | Median total | Median TTFT | Median first tool | Tool args in pieces |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| dated | 25/25 | 20/25 | 5 / 0 | 5/5 | 0 | 0 | 0 | 0 | 5/5 | 1.69 s | — | — | — |
| clarify | 25/25 | 25/25 | 0 / 5 | 5/5 | 0 | 0 | 0 | 0 | 5/5 | 1.38 s | — | — | — |
| strict | 25/25 | 20/25 | 5 / 0 | 5/5 | 0 | 5 | 0 | 0 | 5/5 | 1.24 s | — | — | — |
| stream | 25/25 | 20/25 | 5 / 0 | 5/5 | 0 | 0 | 0 | 0 | 5/5 | 0.97 s | 0.72 s | 0.71 s | 15/15 |

**Correct** — same rule as the probe. In `clarify`, calling `request_information` is the expected answer to `missing_arg`. **Date correct** — of the `ambiguous` runs that made a call, how many wrote tomorrow at 15:00. **Nulls stripped** — `strict` only: null optional arguments removed before validation. **TTFT** — first streamed chunk carrying text or a tool call. **Tool args in pieces** — streamed runs whose tool arguments arrived in more than one chunk.

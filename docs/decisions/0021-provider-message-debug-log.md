# ARIA — Decision Log · Provider error messages in the debug log

**Decision D88** · taken 2026-10-23 (ARIA-143 milestone review)

One entry per decision: what was decided, why, what was rejected and why, and what it affects.
Part of [the decision index](README.md).

---

### D88 · ARIA-143 — A provider's error message may reach the local debug log, and nothing else

**Decided.** When the hosted LLM provider returns an error, the Agent Core logs the provider's
error **message** once, **truncated to 200 characters**, at **`debug`** level under its own target
(`agent_core::provider_message`). That target is **dropped from OTLP export**. The message appears
in no span, no exported log, no error's text and no `ERROR`/`WARN` line. Those carry only the HTTP
status, the provider's error code and the kind of failure. This is a **narrow, explicit exception**
to the rule that conversation content never lands in a span attribute or log line
([`02-stack.md`](../02-stack.md) observability row, which [D41](0007-observability-consent.md) relied
on and [D82](0016-observability-v2.md) kept).

**Why it is an exception at all.** The message is provider-controlled text. Cerebras's messages seen
so far do not quote the request, but some OpenAI-compatible servers (vLLM-style validation errors)
echo the offending input, which can be part of a user's prompt. Under D82, stdout is the cluster's
log surface, so **enabling `debug` for the Agent Core on a pod** (`RUST_LOG=agent_core=debug`)
can put up to 200 characters of prompt text into that pod's logs.

**Why accept it.** The message is the fastest diagnosis of a provider rejecting a request. ARIA-143's
first live run failed with `400 wrong_api_format`, and only the message (`stream_options.include_obfuscation:
property … is unsupported`) said why. The exception is bounded on every axis: off by default
(`info`), local only (never exported, so never in Jaeger or the collector), one line per failure,
and capped in length.

**Rejected.**

- *Drop the message entirely* — the strictest answer, but a provider rejection then shows only a
  status and a code, and diagnosing it needs a packet capture or a reproduction outside ARIA.
- *Keep the message in errors, spans and the `ERROR` line* — the most convenient, but it exports
  provider text, possibly prompt text, to telemetry with telemetry's retention and access. That is
  the breach the rule exists to prevent.

**Revisit if** a provider is found whose messages routinely echo input, so that debugging with
`debug` on routinely writes prompt text to pod logs; or when Jaeger/log retention is decided (the
open question D82 left), if pod logs are then kept longer than conversation history.

**Affects.** ARIA-143 (implemented there); [`docs/services/agent-core.md`](../services/agent-core.md),
which states the exception; `crates/telemetry`, whose export filter holds the muted target; and any
future backend (the Ollama fallback, ARIA-54), which should follow the same pattern rather than log
provider text at a higher level.

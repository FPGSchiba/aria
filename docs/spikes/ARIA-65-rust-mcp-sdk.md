# ARIA-65 — Confirm the Rust MCP SDK

[Library index](../README.md) · [Spikes](README.md) · [MCP Registry](../services/mcp-registry.md) · [Sprint 1](../sprints/sprint-01.md)

**Status: recommendation ready — needs the proof-of-concept before it can close.**
Desk research done 2026-08-19. Acceptance criterion 2 (a live transcript against a real
third-party server) is not satisfiable from research and is the only thing standing between this
brief and a decision.

---

## What the story requires

From `03-architecture.md` and `02-stack.md`, any candidate must satisfy two hard constraints:

1. **Streamable HTTP client transport.** Default transport to MCP servers is HTTP, not stdio — it
   fits a Kubernetes deployment where each server is its own pod/service. *An SDK that only
   supports stdio is disqualified.*
2. **Standard, unmodified MCP.** ARIA's extra needs (dynamic registration, approval gating, user
   context) live in the Registry's control plane. *An SDK that would require patching the wire
   protocol is disqualified.*

The dependency lands in `crates/mcp-client/` (D11) and must stay confined there.

---

## Candidates

All figures read from the crates.io API on **2026-08-19**.

| | **`rmcp`** | `rust-mcp-sdk` | `mcp-protocol-sdk` |
|---|---|---|---|
| Newest version | **3.1.3** | 1.0.1 | 0.5.1 |
| Published | **2026-08-17** (2 days ago) | 2026-07-26 | **2025-08-03** |
| Downloads (total / recent) | **21.0M / 10.6M** | 236k / 103k | 8.6k / 1.9k |
| Licence | Apache-2.0 | MIT | MIT |
| Governance | **`modelcontextprotocol/rust-sdk` — the official MCP org** | `rust-mcp-stack` (community) | `mcp-rust` (community) |
| Streamable HTTP **client** | **Yes** — `StreamableHttpClientTransport` | Yes | Claimed ("multiple transport support") |
| Release cadence | 10 releases in ~3 weeks | ~5 releases in 6 months | **none in ~12 months** |

### `rmcp` — recommended

The official SDK, and by a wide margin the most used and most actively maintained. Relevant
feature flags:

| Flag | Gives |
|---|---|
| `client` | Client-side functionality (not on by default — `server` is) |
| `transport-streamable-http-client` | `StreamableHttpClientTransport`, transport-agnostic |
| `transport-streamable-http-client-reqwest` | The same, with a `reqwest` backend |
| `auth` | OAuth 2.0 support |
| `macros` (default), `schemars` | Tool macros and JSON Schema generation |

So the Registry's dependency line is roughly:

```toml
rmcp = { version = "3.1", default-features = false, features = [
    "client",
    "transport-streamable-http-client-reqwest",
] }
```

`default-features = false` matters here: the default set is **server**-side, and ARIA's Registry is
purely a *client*. Pulling the server half in by accident would put a tool-hosting surface inside
the one service that is meant to be an enforcement point.

Two properties beyond the checklist are worth naming because they bear on decisions already taken:

- **Transports are pluggable** — anything implementing the `Transport` trait works, with helpers
  for `(Sink, Stream)` and `(AsyncRead, AsyncWrite)`. This means D45's "remote LAN servers fall
  back to URL plus a pinned certificate" is expressible without forking the SDK.
- **`schemars` is available**, which matters because the Agent Core receives tool schemas as JSON
  Schema across the gRPC boundary (D38) rather than holding an MCP dependency of its own.

### `rust-mcp-sdk` — credible fallback

Genuinely maintained, hit 1.0 in July 2026, MIT-licensed, built on a separate `rust-mcp-schema`
crate for type-safe schema objects. It satisfies both hard constraints. It is the answer if the
PoC finds something disqualifying in `rmcp`. Its ~2% share of `rmcp`'s download volume is the
whole case against it — fewer users means fewer people hitting the interop bugs first.

### `mcp-protocol-sdk` — rejected

No release in roughly twelve months, and 8.6k lifetime downloads. Against a protocol whose spec
revision has moved since, an unmaintained implementation is a liability regardless of what its
README claims.

---

## ⚠️ One fact to verify during the PoC

The GitHub repository page for `modelcontextprotocol/rust-sdk` presents a release numbered
**v1.7.0 (2026-05-13)**, while the `rmcp` crate on crates.io is at **3.1.3 (2026-08-17)**. These
cannot both be the version of the same artifact. The likeliest explanation is that the repo
publishes several crates on independent version lines (`rmcp`, `rmcp-macros`, …) and the repo's
own release tags track a different one — but **this is a guess, and the brief should not rest on
a guess.** Confirm which line is which before pinning, and record the answer here.

Also confirm the **MCP protocol revision** the pinned version implements. The repo references spec
version `2025-11-25`; ARIA needs the negotiated revision recorded, because it is what any future
interop problem will be diagnosed against.

---

## Recommendation

**Standardise on `rmcp`**, pinned to the `3.1.x` line, with `default-features = false` and only
the `client` + streamable-HTTP-client features enabled.

Reasoning, in the order it actually matters:

1. It is the **official** SDK. For a protocol whose whole value is interoperating with servers
   other people wrote, tracking the reference implementation is worth more than any API
   preference.
2. **Two orders of magnitude more users** than the nearest alternative — for a client that must
   talk to arbitrary third-party servers, that is a direct proxy for how many interop bugs have
   already been found by someone else.
3. **Active to the point of noisy** — ten releases in three weeks. That is a real cost (see the
   blast-radius note below), but it is the right failure mode versus a crate that has not moved
   in a year.
4. Its feature split maps cleanly onto ARIA's need: client-only, streamable HTTP, no server
   surface in the Registry.

### Blast radius if it is abandoned upstream

Contained, by construction. D11 already confines MCP session logic to `crates/mcp-client/`, and
D38 keeps the Agent Core free of any MCP dependency — it speaks only gRPC to the Registry, with
tool schemas carried as JSON Schema. So an upstream abandonment is a rewrite of one shared crate
against a stable internal interface, not a change that reaches five services.

**This is worth stating as a requirement, not just an observation:** `crates/mcp-client/` must
expose ARIA's own types at its boundary and must not re-export `rmcp` types to callers. If `rmcp`
types leak into the Registry's internals, the containment argument above stops being true.

### The version-churn cost

Ten releases in three weeks on a 3.x line means breaking changes are plausible within ARIA's build
horizon. Pin to a compatible range and let the lockfile do the work; do not float.

---

## Still open — carried from the story

**Does the Agent Core consume the SDK directly, or only through `mcp-client`?**
`05-conventions.md` says only that `mcp-client` is "used by the Registry"; it does not say the
Agent Core must go through it.

D38 appears to settle this — "no MCP SDK in the Agent Core", it speaks only gRPC to the Registry —
but the story's open item predates that framing and should be closed explicitly rather than
assumed. **Recommendation: confirm D38 answers it, and close the item citing D38.** If that is
right, the Agent Core does not consume `mcp-client` at all, and the story's phrasing is the thing
that is stale.

---

## What is needed to close this story

- [ ] **PoC:** connect to one real, unmodified third-party MCP server over streamable HTTP and list
      its tools. Attach the transcript to ARIA-65. *This is the one thing research cannot do.*
- [ ] Resolve the v1.7.0 / 3.1.3 version-line discrepancy above and record the answer
- [ ] Record the negotiated MCP protocol revision
- [ ] Confirm with the Agent Core side that D38 closes the "direct or via `mcp-client`" question
- [ ] Write the decision as **D47** in [`../decisions/`](../decisions/README.md), with
      `rust-mcp-sdk` and `mcp-protocol-sdk` as the recorded rejected alternatives
- [ ] Move the item out of [`../open-questions/awaiting-measurement.md`](../open-questions/awaiting-measurement.md)

---

## Sources

- [rmcp — crates.io](https://crates.io/crates/rmcp) · [rmcp — docs.rs](https://docs.rs/rmcp)
- [modelcontextprotocol/rust-sdk — GitHub](https://github.com/modelcontextprotocol/rust-sdk)
- [rust-mcp-sdk — crates.io](https://crates.io/crates/rust-mcp-sdk) · [rust-mcp-stack/rust-mcp-sdk — GitHub](https://github.com/rust-mcp-stack/rust-mcp-sdk)
- [mcp-protocol-sdk — crates.io](https://crates.io/crates/mcp-protocol-sdk)

*Figures read 2026-08-19. Re-check before pinning — this crate moves weekly.*

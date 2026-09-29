# `aria-gateway` — Gateway / Interface

[Library index](../README.md) · [Architecture](../03-architecture.md) · [Decisions](../decisions/README.md) · [Open questions](../open-questions/README.md)

**Deployed service · `services/gateway/`**

---

## Amendments from 2026-09-23 (D60–D65, D82–D83)

**The Gateway now has two front doors** ([D62](../decisions/0012-clients-gateway-surface.md)). The
forcing fact: **gRPC-web cannot do client or bidirectional streaming**, so a browser cannot hold the
`Converse` stream. The surface splits into

1. the **gRPC bidirectional stream**, carrying turns, for native clients; and
2. an **HTTP surface** for everything else — history, preferences, consent management, extension UIs.

"Client-agnostic by design" below is still true in intent, but it is no longer true that every
client arrives over the same gRPC surface.

- **[D60](../decisions/0012-clients-gateway-surface.md)** — clients are named: **native phone and
  desktop** apps on the gRPC stream, and a **Core Web shell** on Authorization Code + PKCE (D3).
  The Gateway remains **the only ARIA component a client can reach**.
- **[D63](../decisions/0012-clients-gateway-surface.md)** — routing between the two doors is **by
  path prefix**. The Gateway **never decodes a request body to decide where a request goes**.
- **[D64](../decisions/0012-clients-gateway-surface.md)** — the Gateway is still the **sole minter**
  of the end-user context, **on both doors**. D1 is unchanged; it now applies twice.
- **[D65](../decisions/0012-clients-gateway-surface.md)** — two paths into the same service may
  carry **different gates**, deliberately. A user reading their own preferences over the HTTP door
  is not the same act as a tool reading them on the user's behalf, and the difference is expressed
  in the gate, not hidden.
- **[D61](../decisions/0012-clients-gateway-surface.md)** — the **turn log is product data**, and
  the HTTP door is where a client reads it.
- **[D83](../decisions/0016-observability-v2.md)** — **a turn starts on the device.** The client
  creates the root span and sends its trace context to the Gateway, so the Gateway is no longer the
  start of a trace. Note what this adds: a client can send a malformed or fabricated trace context,
  and nothing security-relevant may rest on it.
- **[D82](../decisions/0016-observability-v2.md)** supersedes **D41** in the binding-decisions list
  below: telemetry goes to an in-cluster collector, not AppSignal's.

---

## Purpose

The client-facing edge. The **only** place in the system where a raw user credential exists.
Client-agnostic by design — voice streaming and text/command input arrive over the same gRPC
surface, and no client type is committed to.

## Replaces

The 2021 "Interface" box and its three raw hand-wrapped SSL sockets (Audio / Input / Admin).

## Owns

- Keycloak JWT validation (once, at the trust boundary)
- Minting the **Ed25519-signed end-user context** attached to every downstream call (D1)
- Minting the **`session_id`** at stream start (D36)
- Opus → PCM decoding (D25)
- Stream lifecycle: a `Converse` stream is a **session carrying many turns**, not one turn (D27)
- Barge-in support (D27)
- Bounded drain on SIGTERM: stop accepting new streams, send a "session ending, reconnect" control
  frame, close after ~30 s (D29)
- Retry policy: read-only downstream calls retry; **anything that can execute a tool is never
  auto-retried** (D39)

## Binding decisions

D1 (signed context) · D2 (`sub` as `user_id`) · D3 (public client, PKCE + Device Grant) · D25 (Opus on the client link) · D27 (many turns, barge-in) · D29 (bounded drain) · D36 (`session_id`) · D39 (no auto-retry of tool-capable calls) · D41 → superseded by D82 (telemetry via the in-cluster collector)

See [the Decision Log](../decisions/README.md) for the full reasoning and rejected alternatives.

## Contract

`Converse` (bidirectional stream) + `SendCommand` (unary) — `proto/gateway/v1/gateway.proto`, `package aria.gateway.v1;`

## Open items

- Concurrent stream cap, idle timeout and max session length are unspecified (ARIA-39)
- Gateway signing key: per-replica or shared across replicas (ARIA-113)
- May `user_id` appear in telemetry (ARIA-41 / X8)
- Barge-in cancellation of the outbound TTS stream is this service's concern; **self-triggering on
  ARIA's own synthesised voice is unowned** (ARIA-116)

## Jira stories

ARIA-25, 26, 29, 32, 34, 36, 39, 41, 45, 113

---

*Derived from `03-architecture.md` and the Decision Log. Last updated 2026-08-19.*

# `aria-gateway` — Gateway / Interface

[Library index](../README.md) · [Architecture](../03-architecture.md) · [Decisions](../decisions/README.md) · [Open questions](../open-questions/README.md)

**Deployed service · `services/gateway/`**

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

D1 (signed context) · D2 (`sub` as `user_id`) · D3 (public client, PKCE + Device Grant) · D25 (Opus on the client link) · D27 (many turns, barge-in) · D29 (bounded drain) · D36 (`session_id`) · D39 (no auto-retry of tool-capable calls) · D41 (telemetry via collector)

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

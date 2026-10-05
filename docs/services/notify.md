# `aria-notify` — Notification service

[Library index](../README.md) · [Architecture](../03-architecture.md) · [Decisions](../decisions/README.md) · [Open questions](../open-questions/README.md)

**Deployed service · `services/notify/`**

---

## Purpose

The mechanism behind ARIA's oldest claim. "A proactive assistant that runs a person's daily life"
has been differentiator 1 in [the vision](../01-vision.md) since 2021, while every decision up to
D70 described a system that only speaks when spoken to. `aria-notify` is what lets ARIA start a
conversation.

## Replaces

Nothing. This is new surface, not a modernisation of a 2021 box.

## Owns

Delivery, and only delivery:

- **Over a live client stream** when the Gateway is holding one for that user.
- **Over an offline push path** when it is not.

The service is **stateless**. The **inbox** and the **device registrations** live in the Knowledge
Core, not here — so a notify pod can be restarted, rescheduled or scaled without losing anything a
user is entitled to see.

## Binding decisions

[D71](../decisions/0014-notifications.md) (core service, stateless, two delivery paths) ·
[D72](../decisions/0014-notifications.md) (sending is a gated capability)

See [the Decision Log](../decisions/README.md) for the full reasoning and rejected alternatives.

## The leash

D71 and D72 were taken together and should be read together. Proactivity is the capability most
easily turned into a nuisance — or, once MCP servers can trigger it, into something worse. **Sending
a notification is a gated capability**, with five constraints, none of which is optional:

| Constraint | Where it comes from |
|---|---|
| Its own **consent scope** | the standard two-gate model — a server that may control the lights may not thereby message the user |
| **Quiet hours** | read from **typed** preferences in Postgres (D16/D48 — quiet hours are the canonical typed preference) |
| A **rate limit** | per sender |
| A **per-server mute** | the user's escape hatch that does not require revoking the server's other tools |
| An **audit row per send** | consistent with `consent_audit_log`'s posture (D57) |

The gate is what makes proactivity a feature rather than a liability. A notification the user did
not consent to, at an hour they did not agree to, from a server they cannot silence, is exactly the
failure mode that makes people turn assistants off.

## Contract

Not yet specified. It will be gRPC like every other internal service
([D12](../decisions/0002-repo-proto-ci.md) versioning and [D87](../decisions/0020-proto-layout.md) path conventions apply:
`proto/aria/notify/v1/notify.proto`, `package aria.notify.v1;`).

## Open items

Everything below is open and is **not** answered by D71 or D72:

- **What the offline push path actually is.** APNs, FCM, a self-hosted alternative like ntfy, or
  something else — each has different privacy properties and different registration mechanics, and
  the native-client decision (D60) does not settle it.
- **Who may send.** D72 says sending is gated; it does not say whether the Agent Core, an MCP
  server, or a scheduled internal job are gated identically.
- **Delivery semantics.** Whether a notification delivered over a live stream is also written to
  the inbox, and what happens to one queued while a user has no device registered at all.
- **Quiet-hours edge cases.** Whether anything may override them, and who decides — a smoke alarm
  and a podcast recommendation are not the same class of message, and nothing classifies them yet.
- **Rate-limit shape.** Per server, per user, per pair, and over what window.

## Jira stories

None yet — this service postdates the current backlog and needs stories written.

---

*Derived from `03-architecture.md` and the Decision Log. Last updated 2026-09-23.*

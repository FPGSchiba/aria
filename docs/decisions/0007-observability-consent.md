# ARIA — Decision Log · Observability, consent & approval

**Decisions D41–D46** · taken 2026-08-12 (session 1)

One entry per decision: what was decided, why, what was rejected and why, and which stories it
affects. Part of [the decision index](README.md). Several entries carry an explicit **revisit
trigger** — those are not settled forever, but must not be reopened until the trigger fires.

---

### D41 · O1 — All three signals leave via the collector

**Decided.** Traces, logs and metrics all export via OTLP through the AppSignal self-hosted
collector, which is the only telemetry egress. Logs are bridged from `tracing`, exported at reduced
level, carrying allow-listed fields only (ARIA-66).

**Why.** Makes §2's "the one place to enforce that conversation/voice content never lands in a span
attribute or log line" literally true rather than aspirational, and preserves log-to-trace
correlation for debugging a voice pipeline.

**Accepted cost.** The Rust OpenTelemetry logs bridge is less mature than the tracing path — a
known rough edge, mitigated by the same vendor-neutrality argument §2 already makes.

**Rejected.**

- *Logs stay in-cluster, never exported* — the strictest privacy answer, with no scrubbing bug able
  to leak a log line, but it loses exported correlation and the hole reopens the day anyone ships
  stdout logs somewhere.
- *Traces only, as §2 literally says today* — smallest change and the most mature path, but leaves
  the claim false for two of three signal types.

**Affects.** ARIA-60, ARIA-66, ARIA-71, ARIA-74, ARIA-77.

---

### D42 · M1 — Conversational grant, with the triggering call suspended and retried

**The largest hole in the consent design, closed.** A tool call with no active grant causes the
Registry to return `consent required`. The Agent Core surfaces it as a question in the conversation
("May I let Sonos control your speakers?"). On approval the grant is written to `consent_grants` and
the **original call is retried automatically**, within a bounded timeout after which it fails
normally.

**Why.** ARIA is voice-first; interrupting a spoken conversation with a browser task is not usable,
and a failed call the user must reconstruct and re-issue is worse. Keycloak's account console —
which D7 populated with per-tool scopes — remains the **review and revocation** surface. Grant
conversationally, revoke in Keycloak.

**Rejected.**

- *Out-of-band via Keycloak's account console only* — no UI to build at all thanks to D7, but it
  interrupts a conversation with a browser task.
- *Pre-grant everything at registration* — one flow, no mid-conversation interruption, full
  capability list in one place, but it's a consent wall at the least informed moment and trains
  reflexive approval.

**Newly revealed.** Three things this exposes that no story covers: the `consent required` status
must exist in the Registry's proto contract; the Agent Core needs a consent-prompt path distinct
from ordinary tool results; and the suspend-and-retry timeout interacts with D37's streaming
`Decide`, since the user is mid-turn while the prompt is answered.

**Affects.** ARIA-90 (owner), ARIA-95, ARIA-61, ARIA-70, ARIA-85.

---

### D43 · M4 — Short-TTL consent cache, fail-closed on refresh failure

**Decided.** The Registry caches the Keycloak-level consent read with a ~30 s TTL. An entry that
expires and cannot be refreshed **fails closed** — the tool call is denied.

**Why.** Bounds revocation lag to something statable: "revocation takes effect within 30 seconds."
Fail-closed means a Keycloak outage stops tool calls rather than silently permitting them, which is
the only defensible posture for a consent gate.

**Rejected.**

- *No cache* — instant revocation and no staleness to reason about, but a Keycloak round-trip on
  every tool call, on a voice latency path.
- *Event-driven invalidation* — most correct and fastest, but needs an event listener configured on
  a shared realm, and a missed event fails silently in the permissive direction.

**Affects.** ARIA-88 (owner), ARIA-95, ARIA-38.

**Open tail (bucket B).** The exact Keycloak API that exposes a user's granted consents is still an
investigation, owned by ARIA-88.

---

### D44 · M3a — The approval artifact is a git file rendered into the Helm chart

**Decided.** An `approved-servers.yaml` in the repository. Merging the PR that adds an entry **is**
the approval — which is exactly what §3 already describes without naming the artifact. The Registry
reads it as deployed config.

**Why.** Consistent with D13 and D15, gives a full approval history for free, and — decisively —
nothing ARIA runs can grant itself approval, which is the property the gate exists to provide.

**Rejected.**

- *A Knowledge Core table* — runtime-queryable with the same mechanism as everything else, and
  approvals wouldn't need a deploy, but approval becomes data ARIA can write.
- *A ConfigMap managed out of band* — changeable without a deploy cycle, but no review trail and it
  drifts from the git state that produced it.

**Affects.** ARIA-78 (owner), ARIA-75, ARIA-97, ARIA-105, ARIA-106.

---

### D45 · M3b — Server identity is the image digest, with a stable slug as the registry key

**Decided.** Approval is recorded against a container image digest, with a stable human-readable
slug as the Registry's key. A rebuild changes the digest and therefore requires re-approval.

**Why.** Content addressing means an approval covers exactly the code that was reviewed, which is
what a code-trust gate claims to do. Re-approval on rebuild is the correct behaviour for that gate,
not friction to be engineered away.

**Rejected.**

- *Keycloak client ID* — stable across rebuilds and unifies identity across both gates, but approval
  would cover an identity rather than code, so a silently rebuilt image passes unchanged.
- *URL or DNS name* — simplest and uniform across in-cluster and remote servers, but anything
  answering at that address inherits trust.

**Recorded weakness.** Remote LAN MCP servers have no image digest and fall back to URL plus a
pinned certificate. That is a genuinely weaker gate and must be documented as such rather than
presented as equivalent.

**Affects.** ARIA-78, ARIA-75, ARIA-94, ARIA-106.

---

### D46 · M2 — `ListTools` filters by server, not by tool

**Decided.** Servers the user has no relationship with are omitted entirely. Within a server the
user is connected to, all tools are listed regardless of per-tool grant state.

**Why.** Pairs correctly with D42: the model must be able to attempt an ungranted tool for the
consent prompt to fire, so per-tool filtering would make first-time grants impossible. Filtering at
the server level still avoids shipping the complete capability list of the house to a third-party
hosted LLM.

**Rejected.**

- *No filtering* — simplest and everything is discoverable, but the full catalogue goes into every
  hosted prompt.
- *Per-tool filtering* — smallest prompts and the LLM never sees an unusable capability, but nothing
  could ever be granted for the first time.

**Affects.** ARIA-92 (owner), ARIA-61, ARIA-90.

---

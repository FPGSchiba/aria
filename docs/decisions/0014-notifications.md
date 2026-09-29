# ARIA — Decision Log · Notifications

**Decisions D71–D72** · taken 2026-09-23 (design conversation)

One entry per decision: what was decided, why, what was rejected and why, and what it affects.

ARIA's vision has always described a **proactive** assistant, but every decision until now has
described a system that only speaks when spoken to. These two decisions give proactivity a
mechanism and, in the same breath, a leash.

---

### D71 · `aria-notify` is a core service, in scope from the start

**Decided.** **`aria-notify` is a core service**, in scope from the start rather than deferred.
Delivery goes **over a live client stream when the Gateway holds one**; otherwise over an **offline
push path**. **Inbox and device registrations live in the Knowledge Core.** The service itself is
**stateless**.

**Why.** Proactivity is differentiator 1 in the vision — "a proactive assistant that runs a person's
daily life" — and nothing in D1–D70 could wake a user. Deferring it would have meant every
capability that wants to tell you something invents its own channel, which is how a system ends up
with five notification mechanisms and no mute button.

Splitting delivery by whether a stream is held is what makes it cheap in the common case: when
you are talking to ARIA, a notification is just another event on a stream that already exists
(D62). The offline path is the expensive one and it only pays its cost when needed.

Statelessness follows from where the data belongs: the inbox is user-visible product data of exactly
the kind D61 just established, and device registrations are per-person facts. Both belong in the
Knowledge Core under D18's ownership rule, not in a service's private store.

**Rejected.**

- *Defer notifications entirely* — smaller initial surface, and it leaves the vision's central claim
  unimplemented while letting each capability improvise a channel.
- *Fold delivery into the Gateway* — the Gateway already holds the streams, so it looks like the
  natural home; but it would make the one client-exposed component also responsible for queuing,
  retry and push-provider integration, and the Gateway is the component that most needs to stay
  simple.
- *A stateful notify service with its own store* — fewer cross-service calls, at the cost of a second
  place user data lives, outside D18's ownership model and outside D56's retention schedule.

**Open.** The **offline push provider**: APNs/FCM — a **fourth boundary crossing**, and unavoidable
on iOS — versus a self-hosted path. The assumed mitigation is to push only a **wake-up plus an id**
and fetch the body over the authenticated channel, so no content crosses.

**Affects.** New service. §3, §4, §5. The Knowledge Core schema. The Gateway page.

---

### D72 · Notification sending is a gated capability

**Decided.** Sending a notification is **a gated capability, not an ambient one**. Five controls,
all of them required:

- a **consent scope per server** — a server must be granted the ability to notify, like any tool
- **quiet hours**, read from typed preferences
- a **per-server rate limit**
- a **per-server mute** in settings
- an **audit row per delivery**

**Why.** A capability that can interrupt a person at any hour is the one most worth constraining,
and the failure mode is not hypothetical: a buggy or over-eager server that can notify freely is
indistinguishable from spam, and the natural user response is to disable notifications entirely,
which destroys the feature for everything else. Per-server controls mean one misbehaving capability
can be silenced without silencing ARIA.

Reading quiet hours from **typed preferences** is a deliberate reuse: D16 put exactly this kind of
structured attribute in Postgres, so quiet hours need no new storage concept.

The audit row makes "why did my phone buzz at 3am" answerable, which is the question that actually
gets asked.

**Rejected.**

- *Notification as an ordinary tool call under existing consent* — no new machinery, and it gets the
  consent scope for free; but it has no rate limit, no quiet hours and no per-server mute, which are
  the three controls that matter here.
- *A single global mute* — one switch, easy to explain, and it is the switch people flip once and
  never flip back.
- *Priority tiers that can override quiet hours* — genuinely useful for something urgent, but it
  invites every server to mark its own traffic urgent, and nothing yet decides who assigns priority.
  Left open rather than invented.

**Open.** Rate-limit and priority policy specifics — the numbers, and whether a priority concept
exists at all.

**Affects.** New service. ARIA-70, ARIA-73, ARIA-85, ARIA-90, ARIA-118. The MCP Registry page
(scope enforcement), the Knowledge Core page (preferences, audit).

---

## See also

- [Clients, the turn log & the Gateway surface](0012-clients-gateway-surface.md) — D62's streams, D65's gates
- [Observability, consent & approval](0007-observability-consent.md) — D42's conversational grant, D43's cache
- [Knowledge model & data ownership](0003-knowledge-model.md) — D16's typed preferences, D18's ownership

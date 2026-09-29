# ARIA — Decision Log · Clients, the turn log & the Gateway surface

**Decisions D60–D65** · taken 2026-09-23 (design conversation)

One entry per decision: what was decided, why, what was rejected and why, and what it affects.

These six decisions put clients inside the architecture for the first time, make conversation
history a product surface rather than an implementation detail, and split the Gateway into two
front doors so that a human clicking in a UI and ARIA acting on that human's behalf take
deliberately different paths.

---

### D60 · Clients are in scope and are part of the architecture

**Decided.** ARIA has clients, and they are architecture rather than an afterthought: **native
phone and desktop applications** speaking gRPC bidirectional streams, and a **Core Web shell** in
the browser authenticating with Keycloak Authorization Code + PKCE (D3). **The Gateway is the only
ARIA component exposed to a client** — no service is reachable from a client except through it.

**Why.** Every previous version of this library described ARIA from the Gateway inwards and left
the client as somebody else's problem, which made questions like "where does echo cancellation
live" and "who holds the microphone" unanswerable. It also left D3's public-client-plus-PKCE choice
without a concrete consumer. Naming the clients makes the trust boundary explicit: one component
faces the world, and everything behind it can assume an authenticated, context-carrying call.

**Rejected.**

- *Stay client-agnostic and specify nothing* — the position §6 held ("not committing to the original
  iOS/Swift or Android/Xamarin client plans"). It kept options open at the cost of leaving the
  browser case undesigned, and the browser case is the one with real constraints (see D62).
- *A browser-only client* — one surface to build and no app-store story, but it forecloses the
  screenless voice device D3 deliberately enabled the Device Authorization Grant for.
- *Expose services directly to clients for UI traffic* — fewer hops, but it multiplies the number of
  components that must validate a token and terminate TLS, and it destroys the single-minter
  invariant D64 depends on.

**Consequence.** §6's non-goal about client platforms is narrowed: the *platforms* are now named,
though no framework choice is implied. Acoustic echo cancellation (already an unowned open item)
now has a place to live.

**Affects.** ARIA-25, ARIA-26, ARIA-29, ARIA-36, ARIA-45, ARIA-116. The Gateway page.

---

### D61 · Conversation history is user-visible product data, persisted as a turn log

**Decided.** The Agent Core persists the `Decide` event stream as a **turn log**, not as an opaque
transcript blob. The event kinds are `user_turn` (typed input or STT transcript), assistant text,
`tool_call`, `tool_result`, and `consent_required` / `consent_granted`. **One schema serves both
the live stream and history replay.** **Transcripts only — no audio is retained by default.** This
is **product data** in the Knowledge Core, categorically distinct from telemetry.

**Why.** A user-facing history view and a live conversation stream are the same sequence of events
seen at different times; modelling them separately guarantees they drift, and the drift shows up as
"the transcript doesn't match what actually happened". Recording tool calls and consent events as
first-class entries is what makes an action timeline possible at all (D83) — a plain text
transcript cannot answer "what did ARIA *do*, and who allowed it".

Keeping this on the product side of the line preserves the observability rule intact: conversation
content lives in Postgres, where it is governed by D56's retention schedule and D18's per-row
ownership, and **never** in a span attribute or a log line.

**Rejected.**

- *Store a rendered transcript* — simplest to display, but it cannot reconstruct tool calls or
  consent prompts, so the timeline in D83 would have to be rebuilt from traces, which D83
  explicitly forbids.
- *Two schemas — one for streaming, one for storage* — each optimised for its job, at the cost of a
  translation layer that is exactly where "history doesn't match the conversation" bugs live.
- *Retain audio* — best possible debugging material and the only way to re-run STT on a bad
  transcription later, but it is the most sensitive artifact ARIA could hold, and D56 already
  settled that Speech is stateless and persists nothing.

**Interaction with D56 — read this before adding a retention rule.** D56's retention schedule
already has a **conversation history** row: Postgres, in the Knowledge Core, **kept indefinitely as
a deliberate choice**, with the cost named. The turn log **is** that data class, given a defined
event shape — it is not a new class, and its retention is therefore already decided. What is *not*
covered is whether the event-level rows (`tool_call`, `tool_result`) carry payloads that D56's row
did not contemplate; that narrower question is recorded as open rather than restating the whole
retention question.

**Affects.** ARIA-115, ARIA-58, ARIA-61, ARIA-63, ARIA-108. D36 (refined), D56 (row now has a
defined shape). The Agent Core and Knowledge Core pages.

---

### D62 · The Gateway has two front doors: Conversation and the Service router

**Decided.** The Gateway exposes two distinct surfaces. **Conversation** carries the voice and
command path: **gRPC bidirectional streaming for native clients**, and **WebSocket for browser
voice**. **The Service router** carries UI traffic from micro-frontend panels to their own
services (D63).

**Why.** The forcing fact is a protocol limitation, not a preference: **gRPC-web cannot do client
streaming or bidirectional streaming.** A browser therefore cannot speak the same `Converse` stream
a native client speaks, and pretending otherwise would have produced a design that fails on first
contact with a browser. WebSocket is the standard answer and carries the same framing ARIA already
uses.

Separating the two surfaces also separates two genuinely different traffic shapes: one long-lived
stateful stream per session versus many short typed request/response calls.

**Rejected.**

- *gRPC-web for everything* — one client transport and no second protocol, but it cannot carry the
  audio path at all. Not a trade-off; a dead end.
- *WebSocket for everything, including native* — one transport everywhere, at the cost of throwing
  away `tonic`'s typed streams on the clients that can use them, and re-implementing framing.
- *Separate ingress hostnames per surface* — clean separation, but two TLS terminations and two
  places a token is validated, against D60's single-exposed-component rule.

**Open.** Idle timeouts on long server streams through the proxy, and whether heartbeat messages
are needed. Also the relationship between stream lifetime and context-token lifetime — see D64.

**Affects.** ARIA-25, ARIA-26, ARIA-29, ARIA-36, ARIA-39. The Gateway page.

---

### D63 · The Service router routes by path prefix and never decodes a message body

**Decided.** The Service router does **path-prefix routing of gRPC-web traffic** on the standard
gRPC path shape `/<package>.<Service>/<Method>`. It **never decodes a message body**: there is no
proto for routed traffic, and there is no transcoding. **Each service runs `tonic-web`**, so
gRPC-web framing is handled by the service that owns the schema. The typed contract stays end to
end between a UI bundle and its own service.

**Why.** A router that parses bodies needs every service's schema, which makes the Gateway a
compile-time dependency of every extension service and turns every schema change into a Gateway
release. Routing on the path alone means the Gateway needs to know **names**, not **types** — and
names are exactly what the Registry already publishes (D68). It also means an extension service can
evolve its API without ARIA's core knowing.

**Rejected.**

- *A transcoding gateway (gRPC-JSON)* — friendlier to write a UI against, but it requires the
  descriptor set for every routed service at the Gateway, reintroduces the coupling above, and
  loses the typed contract on the wire.
- *A single aggregate proto owned by the Gateway* — one schema to reason about, but every extension
  service would have to have its API merged into ARIA's core repo, which contradicts the whole
  extension model (§6: keep the core free of per-integration logic).
- *Terminate gRPC-web at the Gateway and re-emit native gRPC* — plausible and common, but the
  Gateway would then be parsing and re-serialising payloads it has no schema for.

**Affects.** ARIA-65, ARIA-69, ARIA-94. The Gateway and MCP Registry pages. `proto/` conventions.

---

### D64 · The Gateway remains the only minter of the end-user context, on both front doors

**Decided.** The **Gateway is the sole minter** of the Ed25519-signed end-user context (D1, D59),
and this holds for **both** front doors — Conversation and the Service router. A request that
arrives on the router is authenticated and given a context exactly as a conversation turn is.

**Why.** D1's security property is that a compromised service can *verify* a context but never
*mint* one. Adding a second entrance is precisely how that property gets lost by accident: the
obvious shortcut is to let the routing layer forge a context for UI traffic, and that shortcut would
mean any component able to route is also able to impersonate. Keeping one minter keeps the
invariant checkable in one place.

**Rejected.**

- *Let each service validate the raw Keycloak token for UI traffic* — no new minting path, but it
  puts a raw user credential in every service, breaking the "only the Gateway holds one" invariant
  that D1 exists to establish.
- *A second minter in the routing layer* — lower latency on the UI path, at the cost of the property
  above. Rejected on principle rather than cost.

**Open — not settled here.** Whether the routing itself runs on the **existing nginx-ingress** or on
**Envoy**, with an auth step that calls the Gateway to mint the context header. Both are viable; the
decision needs a look at what the existing ingress can express. nginx-ingress is already deployed
and already fronts Keycloak, so it is not a new component either way.

**Affects.** ARIA-29, ARIA-31, ARIA-32, ARIA-113. The Gateway and Identity pages.

---

### D65 · Two deliberate paths into a service, with different gates

**Decided.** A service can be reached two ways, and the difference is intentional:

| Path | Who is acting | Gates |
|---|---|---|
| **MCP Registry** | ARIA, on the user's behalf | Approval gate **+ consent gate** |
| **Service router** | A human, clicking in a UI | Approval gate **+ audit entry**, **no consent gate** |

**Why.** Consent (D42, D7) exists to answer "does this user allow ARIA to act for them without
being asked each time". When the user is clicking a button in a panel, that question is already
answered by the click — the user *is* the actor, not the principal on whose behalf an agent acts.
Requiring a consent grant to use a UI the user deliberately opened would be ceremony that teaches
people to click through prompts, which is how consent dialogs stop meaning anything.

The approval gate applies to both because it is about something different: whether this server's
*code* is trusted to run at all (D44, D45). That question does not depend on who is calling.

**Rejected.**

- *Consent on both paths* — superficially safer and one rule to explain, but it prompts for
  permission to do the thing the user just asked for directly. It devalues the prompt on the path
  where it matters.
- *No gate at all on the router path* — the user is acting directly, so why check anything? Because
  approval is about code trust, and because an audit entry is what makes "what did this service do
  for me" answerable at all.

**Consequence.** The audit record now has two shapes: D57's two-row invocation record for the
agent path, and a direct-action entry for the router path. Both land in the same audit table; the
actor field distinguishes them.

**Affects.** ARIA-72, ARIA-85, ARIA-90, ARIA-95, ARIA-118. The MCP Registry and Gateway pages.

---

## See also

- [Extension surface](0013-extension-surface.md) — D66–D70, what an extension service may ship
- [Identity, tokens & service auth](0001-identity-tokens-service-auth.md) — D1, D3, D9
- [Agent Core, sessions & conversation](0006-agent-core-sessions.md) — D36, D37
- [Retention & the audit record](0010-retention-audit.md) — D56's schedule, D57's audit rows

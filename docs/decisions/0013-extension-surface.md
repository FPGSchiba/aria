# ARIA — Decision Log · The extension surface

**Decisions D66–D70** · taken 2026-09-23 (design conversation)

One entry per decision: what was decided, why, what was rejected and why, and what it affects.

An extension service is no longer just an MCP server. It may now ship three things — an MCP
endpoint, a gRPC API, and a UI bundle — registered together and approved together. These five
decisions define that surface and, just as importantly, where it stops: an extension service still
gets no direct access to the Knowledge Core.

---

### D66 · An extension service may ship a UI bundle, approved by digest in the same PR

**Decided.** An extension service may ship a **UI bundle** alongside its MCP server and its gRPC
API. The bundle is **approved by digest in the same PR as the image** — one merge approves the
code, the API and the interface together.

**Why.** D44 made merging the `approved-servers.yaml` PR *the* approval, and D45 made a server's
identity its **image digest**. A UI bundle is executable code that runs in Jann's browser against
his session; approving it by any weaker mechanism than the server's own code would put the softest
gate on the part with the most direct access to the user. Pinning it by digest in the same commit
means there is exactly one approval event per version of a service, and no way for the interface to
change without the approval changing.

**Rejected.**

- *Serve the UI from the service itself, unpinned* — simplest, and the service already runs; but
  then the interface can change without re-approval, which defeats D45's digest identity.
- *A separate approval flow for bundles* — finer-grained, at the cost of two approval artifacts to
  keep in step and a real chance they disagree about which version is live.
- *No UI for extension services at all* — keeps the surface minimal, but the whole point of the
  service catalog is that a capability can bring its own interface; without it every integration
  needs core UI work, which is the coupling §6 exists to avoid.

**Affects.** ARIA-97, ARIA-101, ARIA-105, ARIA-106. `approved-servers.yaml`'s schema. The
Self-Extension page.

---

### D67 · Bundles run on a separate origin, in a sandboxed iframe, talking only via `postMessage`

**Decided.** UI bundles are served by a **static UI host on an origin separate from the shell** and
loaded in a **sandboxed iframe**. **`postMessage` is the only bridge** to the shell. The panel's
token **audience is limited to its own service**.

**Why.** A bundle is third-party code — including, eventually, code ARIA generated herself
(ARIA-103). Same-origin means it can read the shell's storage, its tokens and its DOM; a separate
origin plus the sandbox makes the browser enforce the boundary rather than asking the bundle to
behave. Audience-limiting the token means a compromised or simply buggy panel cannot use its
credential against any other service — it is the browser-side analogue of D5's audience restriction
between services, and it fails the same way: closed.

`postMessage` as the only channel means the shell's surface to a panel is an explicit, reviewable
message contract rather than whatever the panel can reach.

**Rejected.**

- *Same-origin bundles* — no iframe plumbing, no `postMessage` contract, direct access to shared
  state; and a single malicious or compromised panel owns the whole session.
- *Web Components in the shell's own document* — a lighter integration with real composition, but
  it shares an origin and a DOM, so it gives the same access as above with less of a boundary to
  point at.
- *A broad token for all panels* — one token to manage, and it means any panel can call any
  service. Directly contrary to D5.

**Affects.** ARIA-97, ARIA-105. The Gateway page (the shell), and the new UI-host component in §4.

---

### D68 · The MCP Registry becomes the service catalog and publishes the route table

**Decided.** The MCP Registry is now the **service catalog**. **One registration** carries the
**MCP endpoint**, the **gRPC path prefix** and the **UI bundle reference**, and the Registry
**publishes the route table** that the Gateway's Service router routes on (D63).

**Why.** These three facts about a service are born at the same moment — registration — and change
at the same moment. Splitting them across three places would mean three things to keep in step and
three ways for a service to be half-registered. The Registry already holds the approval check and
already knows which servers exist, so it is the component that can refuse to publish a route for a
server whose approval does not cover it.

This is also what keeps D63 honest: the Gateway routes on names it was given, and the authority for
those names is the same component that enforces approval.

**Rejected.**

- *Static routes in the Gateway's own config* — no new publication mechanism, but every new service
  becomes a Gateway redeploy, which contradicts the "tell ARIA to connect to this" promise.
- *A separate catalog service* — clean separation of concerns, at the cost of a third component that
  must agree with the Registry about what exists. The Registry already holds the authoritative list.
- *Kubernetes Ingress objects per service* — reuses cluster machinery, but it puts the route table
  outside the approval gate and outside the Registry's knowledge.

**Consequence.** `RegisterServer`'s contract grows. The Registry's registration record is no longer
MCP-specific, and `ListTools` (D46) is now one view over a broader catalog.

**Affects.** ARIA-69, ARIA-72, ARIA-75, ARIA-94, ARIA-106. The MCP Registry page.

---

### D69 · The Registry forwards the Gateway-signed context to MCP servers; servers verify, never mint

**Decided.** The MCP Registry **forwards the Gateway-signed context token to an MCP server as a
header**, so that a server can prove which user it is acting for when it calls a core API. **Servers
verify only; they cannot mint.** `_aria_user_id` (D9) is unchanged: it remains **injected data for
the tool**, never an authorization input.

**Why.** Without this, a server calling back into a core API has no way to demonstrate whose behalf
it acts on, and the only alternatives are worse: either it asserts a user id (which D9 exists to
forbid) or the core API trusts the caller's service identity to speak for any user (which collapses
per-user authorization entirely). Forwarding a token the server cannot forge gives it a provable
claim while keeping D1's asymmetry intact.

The distinction with `_aria_user_id` is the subtle part and it is worth restating: the **header** is
an authorization credential the callee verifies; the **argument** is contextual data the tool reads.
D9 separated those deliberately after they were conflated once, and this decision does not merge
them back.

**Rejected.**

- *Servers assert the user id themselves* — trivial, and exactly the prompt-injection hole D9 closed.
- *Mint a per-server token at the Registry* — a tighter audience, but it makes the Registry a minter,
  which D64 and D1 both forbid.
- *No forwarding; core APIs trust the service identity* — fewer moving parts, and every server gains
  the ability to act as any user.

**Affects.** ARIA-72, ARIA-82, ARIA-90, ARIA-95. The MCP Registry and Identity pages.

---

### D70 · Extension services get no direct Knowledge Core access; `ResolvePerson` is the only exception

**Decided.** Extension services get **no direct Knowledge Core access**. Retrieval at the Agent Core
already covers "what does ARIA know". A **narrow `ResolvePerson`** on the core API, **behind its own
consent scope**, returns an **opaque person id and a display name** so that a service can keep its
own mapping. **Writes go through the Agent Core.**

**Why.** The Knowledge Core holds facts about people who are not the caller — D18's per-row
ownership exists precisely because the graph contains third parties, and D18's rejected alternatives
record that a shared, readable circle graph "lets any circle member enumerate everyone, which nobody
consented to". Extending that read access to an arbitrary extension service extends it to a party
those third parties consented to even less. A Sonos controller does not need the social graph.

`ResolvePerson` exists because a service legitimately needs stable identity to keep its own state
against — "this is the same person as last time" — and an opaque id plus a display name gives it
exactly that and nothing else.

**Rejected.**

- *Scoped read access to the graph* — more capable, and it would let a service do its own retrieval;
  but any scoping fine enough to be safe is a second authorization model to maintain beside consent,
  and any scoping coarse enough to be simple leaks third-party data.
- *A read-only replica for extension services* — no write risk, same disclosure problem.
- *No `ResolvePerson` at all* — the strictest answer, and it forces every service to invent its own
  user key, which fragments identity and makes "the same person" unanswerable across services.

**Consequence.** This reinforces D76: the Knowledge Core lives in its own database precisely so that
the schema-level read grants of D74 **cannot** be pointed at it.

**Affects.** ARIA-55, ARIA-57, ARIA-59, ARIA-62, ARIA-76. The Knowledge Core and MCP Registry pages.

---

## See also

- [Clients, the turn log & the Gateway surface](0012-clients-gateway-surface.md) — D63's routing, D65's two paths
- [External datastores](0015-external-datastores.md) — D74's read grants, D76's separate database
- [Knowledge model & data ownership](0003-knowledge-model.md) — D18, D21, D22
- [Observability, consent & approval](0007-observability-consent.md) — D44, D45, D46

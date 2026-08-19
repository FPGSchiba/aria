# Sprint 1 — dossier

[Library index](../README.md) · [Spikes](../spikes/README.md) · [Backlog](../backlog/README.md) · [Open questions](../open-questions/README.md)

Sprint id **473**. Status read live from Jira **2026-08-19**.

---

## The headline

Sprint 1 is **seven spikes and two code stories**, which is why it has felt like paperwork. But
the live board says something the stale plan did not:

> **ARIA-28 is unblocked, fully specified, and pure Rust.** It waits on nothing, ARIA-11 (the
> workspace) is already Done, and D1 settles every design question it had. **You can start writing
> code on it today** — no spike has to close first.

Two more are in the same position one sprint out (see [Start coding now](#start-coding-now)).

---

## Live status — updated 2026-08-19 (end of day)

| Story | Epic | Status | Kind | What is left |
|---|---|---|---|---|
| **ARIA-11** Cargo workspace skeleton | Platform Foundation | ✅ **Done** | code | — |
| **ARIA-47** MCP write-permission model | Knowledge Core | ✅ **Done** (D21, D22) | spike | — |
| **ARIA-79** k3s cluster baseline | Deployment & Infra | ✅ **Done today** | spike | — (findings spawned several new items) |
| **ARIA-43** Typed vs. embedded classification | Knowledge Core | ✅ **Done today** (D48) | spike | — |
| **ARIA-19** Register the ARIA client | Identity | 🟡 In Progress | config | Create the client; capture pre-change state; two decoded tokens; non-regression check |
| **ARIA-28** `aria-identity` sign/verify | Identity | ⬜ To Do | **code** | Everything — but **nothing blocks it** |
| **ARIA-57** Social graph schema design | Knowledge Core | ⬜ To Do | design | The column list. All inputs fixed by D17/D18/D19/D4 |
| **ARIA-65** Confirm the Rust MCP SDK | MCP Registry | ⬜ To Do | spike | One PoC transcript against a real third-party MCP server |
| **ARIA-40** Hosted LLM provider | Agent Core | ⬜ To Do | spike | Run the probe harness with API keys |

**4 of 9 done.** The two closed today were closed on evidence, not optimism — both had every
acceptance criterion met and the closing comments say which.

### What ARIA-19 gained today, without anyone working the story

The cluster investigation established most of the realm facts this story exists to capture:

| Fact | Value |
|---|---|
| Issuer | `https://keycloak.fpg/realms/master` |
| Realm | **master** — decided as ARIA's realm too (D53) |
| In-cluster endpoint | `keycloak.keycloak.svc.cluster.local` (80/443) |
| Ingress | `keycloak.fpg` → 192.168.1.21 (MetalLB + nginx) |
| Username claim | `preferred_username` |
| Groups claim | `groups` (values seen: `/KubeAdmin`, `/SuperAdmin`, `/VaultAdmin`, `/ZabbixAdmin`) |
| TLS | Self-signed, issued directly by root `CN = fpg-ca` — no intermediate |
| Existing client | `kubernetes` (used by the API server) |
| ARIA client roles | `aria-user`, `aria-admin` (D47) |

Still to do: **create the ARIA client**, capture the full pre-change client/role list, produce the
two decoded token examples, and run the non-regression check against a non-ARIA app.

---

## Start coding now

Given ARIA-11 is Done, three stories need **no spike output at all**:

### 1. ARIA-28 — `aria-identity` sign/verify *(in sprint 1, unblocked)*

Pure Rust, in `crates/identity/`. D1 fixes everything: **Ed25519**, a short-lived JWT carrying
`user_id`, roles, `exp`, `jti`, published JWKS-style with a `kid` per key and **two keys live
throughout rotation**.

The acceptance criteria are unusually good — treat them as the test list:

- `verify()` returns a typed identity **constructible nowhere else in the public API**. No path
  from a raw `String` to a trusted identity. This is the whole point: it is what makes a
  prompt-injected argument claiming to be someone else structurally unable to become one.
- One test each, all rejected: tampered `user_id`; tampered roles; unsigned; wrong/unknown key;
  past its freshness window (test must advance time).
- A test signing with the *old* key mid-rotation still verifies.

One open item to settle as you go: whether the freshness window is per-request or per-session, and
its value. D1 fixes that it is short-lived; not its shape.

### 2. ARIA-12 — `proto/` layout *(sprint 2, blocked only by the Done ARIA-11)*

D12 fixes the answer: `proto/<service>/v1/<service>.proto` containing `package aria.<service>.v1;`
— both directory *and* package carry the version, because that is what buf's default lint and
breaking-change rules expect. One open item: whether cross-service shared message types are
permitted at all.

### 3. ARIA-15 — CI pipeline *(sprint 2, blocked only by the Done ARIA-11)*

D13 fixes it: GitHub Actions on hosted runners. `build`, `test`, `fmt --check`, `clippy` with
warnings denied, toolchain from `rust-toolchain.toml`, `protoc` provisioned by the pipeline, and a
deliberately broken commit demonstrated to turn it red.

**Suggested order: ARIA-15 → ARIA-12 → ARIA-28.** CI first means the other two land on a verified
baseline, which is the stated purpose of the Platform Foundation epic.

---

## What genuinely needs you

Four things nobody else can do. Roughly an hour, and it unblocks the rest of sprint 1.

1. **Run `scripts/aria-cluster-baseline.sh`** against the k3s cluster and paste the output back.
   *Highest value of the four* — it can invalidate D6, and five stories size themselves off it.
2. **Fill in [`spikes/ARIA-19-keycloak-facts.md`](../spikes/ARIA-19-keycloak-facts.md)** from the
   realm. Mostly one `curl` against the discovery endpoint plus two decoded tokens.
3. **Run `scripts/aria-llm-probe.py`** with whichever API keys you have. Free tiers are enough.
4. **Decide two things:** the ARIA client role names (ARIA-19), and the typed/free-text
   classification rule (ARIA-43 — a proposal is written, argue with it).

---

## Sprint 1 exit criteria

- [ ] ARIA-28 implemented and merged
- [ ] ARIA-19 facts recorded; client roles decided; both flows exercised
- [ ] ARIA-65 PoC transcript attached; decision recorded as D47
- [ ] ARIA-40 probe evidence attached; provider and canonical tool-call shape decided
- [ ] ARIA-43 classification rule decided; Qdrant-primary list published for ARIA-96
- [ ] ARIA-57 schema proposal written and reviewed
- [ ] ARIA-79 baseline recorded; **D6's two triggers checked explicitly in writing**
- [ ] Every closed item moved out of `open-questions/` and mirrored to Jira and Confluence

---

## One risk worth naming

**D6 has two live triggers and both are checked by ARIA-79** — multi-node, and a CNI that does not
enforce NetworkPolicy. k3s ships flannel by default, and flannel does not enforce NetworkPolicy;
the API server accepts the objects and they silently do nothing.

If either fires, D6's justification for skipping internal TLS collapses and it must be reopened —
which reaches into the Gateway, Speech and every gRPC boundary. **Checking this early is worth
more than anything else in the sprint**, which is why ARIA-79 is the first ask above.

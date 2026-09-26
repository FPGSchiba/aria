# ARIA — Sprint plan, sprints 1 and 2

[Library index](../README.md) · [Roadmap](roadmap.md) · [Jira rulebook](jira-rulebook.md) · [Sprint guideline](sprint-guideline.md) · [Open questions](../open-questions/README.md)

**Status: approved 2026-09-26.** Sprint 1 is kept as written. The briefed sprint 2 is split: its
identity stories form sprint 2, and its data and clients stories wait in the backlog. Open
questions follow the guideline's *decide at the last responsible moment* rule (see
[below](#how-the-open-questions-are-handled)). **Jira creation: pending.** That will be 21
issues, 79 subtasks and 16 Blocks links, and nothing else.

The story text is generated from one source. Every description is checked against the rulebook's
limits: under 120 words, 2–4 criteria, 2–6 subtasks, points from 1/2/3/5, and no D-numbers. Story
IDs (`S1-01` …) are local to this document; Jira assigns the real keys.

---

## What already exists

Checked 2026-09-24 against the repo and a clean clone of `main` (`25e25cb`), so no story below
redoes finished work:

- **Workspace** — five service crates that print `Hello, world!`, plus `crates/shared` and
  `crates/identity`.
- **`crates/identity`** — signs and verifies the end-user context; **all 22 tests pass**. Sprint 2
  wires it into the Gateway and Agent Core; it does not rebuild it.
- **Not there yet** — `proto/` holds only `.gitkeep`. There is no CI workflow, Dockerfile or Helm
  chart, and no sealed-secrets controller in the cluster. The ARIA Keycloak client doesn't exist,
  and [its facts page](../spikes/ARIA-19-keycloak-facts.md) is an unfilled template.

---

## Sprint 1 — `ARIA Sprint 1 — CLI turn streams back`

**Goal:** A typed message from a throwaway CLI client reaches the hosted LLM and streams back,
running on the cluster.

**Focus theme:** `theme-conversation`; foundations and one ops story only where the goal needs
them. **Size:** 22 points plus one 4-hour spike. **Demo:** S1-10's second criterion is the
goal sentence.

| ID | Type | Summary | Label | Size | Blocked by |
|---|---|---|---|---|---|
| S1-01 | Spike | Measure the hosted LLM candidates with the probe harness | `theme-conversation` | 4 h | — |
| S1-02 | Story | Add the proto crate with text-only Converse and Decide schemas | `theme-foundations` | 3 | — |
| S1-03 | Story | Add a CI workflow that runs the repo gate on every push | `theme-foundations` | 2 | — |
| S1-04 | Story | Publish Gateway and Agent Core images to ghcr.io from CI | `theme-foundations` | 2 | — |
| S1-05 | Story | Stream Decide from the Agent Core with in-memory session history | `theme-conversation` | 3 | S1-02 |
| S1-06 | Story | Add the hosted LLM backend to the Agent Core | `theme-conversation` | 3 | S1-01, S1-05 |
| S1-07 | Story | Serve a text-only Converse stream from the Gateway | `theme-conversation` | 3 | S1-02 |
| S1-08 | Story | Add a throwaway CLI client for Converse | `theme-conversation` | 1 | S1-02 |
| S1-09 | Story | Install the sealed-secrets controller and seal the LLM API key | `theme-ops` | 2 | — |
| S1-10 | Story | Deploy the Gateway and Agent Core with the umbrella Helm chart | `theme-foundations` | 3 | S1-04 |

**Capacity.** 22 points is above the guideline's "deliberately low" first number; the
previous sprint 1 ran six weeks and closed five of nine issues. It is kept as briefed. Record what
actually completes, because that figure is sprint 2's capacity.

**Two sprint-1-only exceptions.**
- **The spike blocks a story in the same sprint.** S1-01 blocks S1-06 because there is no earlier
  sprint for it to run in. Run the spike first — it is four hours, and its harness already exists.
- **Deployment comes last.** The service stories can't be deployed until S1-10 lands, so within
  this sprint they are demonstrated locally and deployed together by S1-10.

### S1-01 · Measure the hosted LLM candidates with the probe harness

*Spike · `theme-conversation` · timebox 4 h · 84 words*

**Goal** — The Agent Core's default hosted provider and model are chosen from measured evidence.

**Not this story:**
- the Ollama fallback model
- writing the backend adapter

**Acceptance criteria:**
- `scripts/aria-llm-probe.py` has run against each candidate with a key; its results and summary are committed under `docs/spikes/`
- The brief's results section compares selection accuracy, malformed calls and median TTFT per candidate
- The choice is recorded in `docs/decisions/` and removed from `awaiting-measurement.md`

**Open items:**
- None.

**Timebox:** 4 h. **Output:** `docs/spikes/ARIA-40-hosted-llm-backend.md`.

**Links:** [docs/spikes/ARIA-40-hosted-llm-backend.md](https://github.com/FPGSchiba/aria/blob/main/docs/spikes/ARIA-40-hosted-llm-backend.md)

**Subtasks:**
1. Get API keys for each candidate
2. Run the probe with `--repeats 5` per candidate
3. Write the results section and the decision entry

### S1-02 · Add the proto crate with text-only Converse and Decide schemas

*Story · `theme-foundations` · 3 points · 74 words*

**Goal** — A `proto` crate at `proto/` generates tonic types at build time for a text-only `Converse` (Gateway) and `Decide` (Agent Core).

**Not this story:**
- audio frames, tool-call events, `SendCommand`
- Knowledge Core schemas

**Acceptance criteria:**
- `cargo build -p proto` succeeds on a machine with no `protoc` installed
- `cargo test -p proto` round-trips one `Converse` and one `Decide` message
- `git ls-files` lists no generated code

**Open items:**
- None.

**Links:** [docs/05-conventions.md](https://github.com/FPGSchiba/aria/blob/main/docs/05-conventions.md)

**Subtasks:**
1. Create the crate with a `tonic-build` script and vendored `protoc`
2. Write `proto/gateway/v1/gateway.proto` with a text-only `Converse`
3. Write `proto/agent-core/v1/agent_core.proto` with a text-only `Decide`
4. Add the round-trip tests

### S1-03 · Add a CI workflow that runs the repo gate on every push

*Story · `theme-foundations` · 2 points · 79 words*

**Goal** — GitHub Actions runs fmt, clippy, tests and the 80 % coverage floor on every push and pull request, and `main` requires it.

**Not this story:**
- building or publishing images
- deploying

**Acceptance criteria:**
- A pull request with a clippy warning fails the check; the fixed commit passes
- A pull request that drops coverage below 80 % fails the check
- `main` cannot be merged into without the check passing

**Open items:**
- None.

**Links:** [docs/05-conventions.md](https://github.com/FPGSchiba/aria/blob/main/docs/05-conventions.md)

**Subtasks:**
1. Write `.github/workflows/ci.yml` mirroring `.githooks/pre-commit`
2. Cache the cargo registry and build directory
3. Prove red and green with a deliberately broken commit
4. Make the check required on `main`

### S1-04 · Publish Gateway and Agent Core images to ghcr.io from CI

*Story · `theme-foundations` · 2 points · 70 words*

**Goal** — Every merge to `main` builds the `aria-gateway` and `aria-agent-core` images and pushes them to ghcr.io, tagged with the commit SHA.

**Not this story:**
- images for services that do not exist yet
- deploying them

**Acceptance criteria:**
- After a merge, both images exist in ghcr.io under that commit's SHA
- A pod on the cluster pulls each image with no manual login

**Open items:**
- None.

**Links:** [docs/05-conventions.md](https://github.com/FPGSchiba/aria/blob/main/docs/05-conventions.md)

**Subtasks:**
1. Write a multi-stage Dockerfile for the service binaries
2. Add a publish job on `main` using `GITHUB_TOKEN`
3. Make the packages pullable from the cluster

### S1-05 · Stream Decide from the Agent Core with in-memory session history

*Story · `theme-conversation` · 3 points · blocked by S1-02 · 115 words*

**Goal** — The Agent Core serves `Decide`: it adds the user's text to that session's history, calls an LLM backend with it, and streams the reply back as text chunks.

**Not this story:**
- a real LLM provider — a scripted backend only
- persisting history; tools

**Acceptance criteria:**
- A test streams at least two chunks from a scripted backend for one `Decide` call
- A second `Decide` on the same `session_id` passes the first exchange to the backend
- A backend error ends the stream with an error status and is not retried

**Open items:**
- No collector is deployed yet — interim: export spans only when an OTLP endpoint is configured.

**Links:** [docs/services/agent-core.md](https://github.com/FPGSchiba/aria/blob/main/docs/services/agent-core.md)

**Subtasks:**
1. Define a streaming LLM backend trait
2. Implement `Decide` with a per-session history map
3. Add a scripted backend for tests
4. Add a shared tracing setup in `crates/shared` and spans on `Decide`
5. Add gRPC health

### S1-06 · Add the hosted LLM backend to the Agent Core

*Story · `theme-conversation` · 3 points · blocked by S1-01, S1-05 · 114 words*

**Goal** — The Agent Core's default backend calls the provider chosen by the probe spike and streams its tokens into `Decide`.

**Not this story:**
- the Ollama fallback and failover
- tool calls

**Acceptance criteria:**
- With a valid key, `grpcurl` against a running Agent Core streams a reply to a text prompt
- A key-gated test shows the first chunk arriving before the provider finishes
- A missing key stops the Agent Core at startup with an error naming the setting
- A provider timeout ends the stream with an error instead of hanging

**Open items:**
- Whether `traceparent` rides the outbound hosted call is undecided — interim: do not send it.

**Links:** [docs/services/agent-core.md](https://github.com/FPGSchiba/aria/blob/main/docs/services/agent-core.md)

**Subtasks:**
1. Implement the provider's streaming client behind the backend trait
2. Read the key, model and timeout from configuration
3. Add the key-gated integration test
4. Add spans around the provider call

### S1-07 · Serve a text-only Converse stream from the Gateway

*Story · `theme-conversation` · 3 points · blocked by S1-02 · 90 words*

**Goal** — The Gateway accepts `Converse` streams, gives each a new `session_id`, forwards every text turn to `Decide`, and relays the chunks back.

**Not this story:**
- authentication, audio, `SendCommand`
- draining streams on shutdown

**Acceptance criteria:**
- One stream carries two turns, and both reach `Decide` under the same `session_id`
- Two concurrent streams get different `session_id`s
- A failed `Decide` returns an error for that turn and is not retried

**Open items:**
- Stream cap, idle timeout and maximum session length are undecided — interim: none enforced.

**Links:** [docs/services/gateway.md](https://github.com/FPGSchiba/aria/blob/main/docs/services/gateway.md)

**Subtasks:**
1. Implement `Converse` on tonic, minting `session_id` at stream start
2. Call `Decide` per turn and relay its chunks
3. Test against a stub `Decide` server
4. Add gRPC health and spans per turn

### S1-08 · Add a throwaway CLI client for Converse

*Story · `theme-conversation` · 1 point · blocked by S1-02 · 69 words*

**Goal** — A CLI sends each line typed on stdin as a turn on one `Converse` stream and prints the reply as it streams.

**Not this story:**
- login
- anything beyond a developer tool

**Acceptance criteria:**
- Against a local Gateway, a typed line prints a streamed reply
- Two lines in one run share one session
- `--gateway <url>` selects the endpoint

**Open items:**
- None.

**Links:** [docs/services/gateway.md](https://github.com/FPGSchiba/aria/blob/main/docs/services/gateway.md)

**Subtasks:**
1. Add the CLI binary to the workspace
2. Send stdin lines on one stream
3. Print chunks as they arrive

### S1-09 · Install the sealed-secrets controller and seal the LLM API key

*Story · `theme-ops` · 2 points · 98 words*

**Goal** — The sealed-secrets controller runs in the cluster, and the hosted LLM key reaches the Agent Core from a sealed secret committed to git.

**Not this story:**
- Vault or database credentials
- rotating the key

**Acceptance criteria:**
- The sealed key committed to the repo decrypts in the `aria` namespace
- The Agent Core pod reads the key from it
- The controller's private key is exported and stored outside the cluster

**Open items:**
- Where that exported key is kept is undecided — interim: one offline copy, its location noted in the closing comment.

**Links:** [docs/04-deployment.md](https://github.com/FPGSchiba/aria/blob/main/docs/04-deployment.md)

**Subtasks:**
1. Install the controller from its Helm chart
2. Seal the API key and add it to the umbrella chart
3. Export and store the controller's key

### S1-10 · Deploy the Gateway and Agent Core with the umbrella Helm chart

*Story · `theme-foundations` · 3 points · blocked by S1-04 · 115 words*

**Goal** — One Helm release installs the Gateway and Agent Core into the `aria` namespace from one `values.yaml`.

**Not this story:**
- ingress, TLS or exposure outside the cluster
- NetworkPolicy; other services

**Acceptance criteria:**
- `helm upgrade --install` into an empty namespace brings both pods to Ready
- Through `kubectl port-forward`, the CLI gets a streamed reply from the hosted LLM
- `helm rollback` restores the previous tags and both pods return to Ready

**Open items:**
- How releases reach the cluster is undecided — interim: `helm` run by hand.
- Service-to-service mTLS is undecided — interim: plaintext gRPC.
- Node capacity is unsized — interim: set requests and limits; record the footprint.

**Links:** [docs/04-deployment.md](https://github.com/FPGSchiba/aria/blob/main/docs/04-deployment.md)

**Subtasks:**
1. Create the umbrella chart with one template set per service
2. Add probes against gRPC health
3. Wire the Agent Core address and the sealed key
4. Document the deploy and rollback commands

---

## Sprint 2 — `ARIA Sprint 2 — Authenticated CLI turn`

**Goal:** Only a logged-in CLI user gets a reply, and the Agent Core knows who they are.

**Focus theme:** `theme-identity`. **Size:** 12 points plus one 4-hour spike, to be
checked against sprint 1's completed points at planning. **The spike prepares the next sprint:**
S2-11 answers the one open question that would be expensive to guess before the turn log is
persisted (S2-08).

| ID | Type | Summary | Label | Size | Blocked by |
|---|---|---|---|---|---|
| S2-01 | Story | Register the ARIA client and its roles in Keycloak | `theme-identity` | 2 | — |
| S2-02 | Story | Log the CLI in with the device authorization flow | `theme-identity` | 2 | S2-01 |
| S2-03 | Story | Reject unauthenticated Converse streams at the Gateway | `theme-identity` | 3 | S2-01 |
| S2-04 | Story | Mint the signed context at the Gateway and verify it in the Agent Core | `theme-identity` | 5 | S2-03 |
| S2-11 | Spike | Prototype how the end-user context crosses a second service hop | `theme-identity` | 4 h | — |

### S2-01 · Register the ARIA client and its roles in Keycloak

*Story · `theme-identity` · 2 points · 94 words*

**Goal** — The shared realm has a public `aria` client with PKCE and the device grant, plus the `aria-user` and `aria-admin` client roles; the facts ARIA builds against are recorded.

**Not this story:**
- per-MCP-server clients or scopes
- onboarding other circle members

**Acceptance criteria:**
- A decoded token for a user with `aria-user` shows the role at the recorded claim path; one without shows it absent
- The device flow issues a token for `aria` from a terminal
- Another app in the realm still logs in afterwards

**Open items:**
- None.

**Links:** [docs/spikes/ARIA-19-keycloak-facts.md](https://github.com/FPGSchiba/aria/blob/main/docs/spikes/ARIA-19-keycloak-facts.md)

**Subtasks:**
1. Capture the realm's pre-change state
2. Create the client and both roles
3. Record the facts and decoded tokens in the Keycloak facts page

### S2-02 · Log the CLI in with the device authorization flow

*Story · `theme-identity` · 2 points · blocked by S2-01 · 83 words*

**Goal** — The CLI gets a Keycloak token for the `aria` client through the device flow and sends it with every `Converse` stream.

**Not this story:**
- refreshing a token mid-stream
- storing tokens across runs

**Acceptance criteria:**
- `login` prints a verification URL and code, and finishes once the user approves in a browser
- Every `Converse` stream carries the token as metadata
- An expired token is not sent; the CLI asks for a new login

**Open items:**
- None.

**Links:** [docs/spikes/ARIA-19-keycloak-facts.md](https://github.com/FPGSchiba/aria/blob/main/docs/spikes/ARIA-19-keycloak-facts.md)

**Subtasks:**
1. Implement the device flow against the recorded endpoints
2. Attach the token as stream metadata
3. Handle an expired token

### S2-03 · Reject unauthenticated Converse streams at the Gateway

*Story · `theme-identity` · 3 points · blocked by S2-01 · 116 words*

**Goal** — The Gateway validates the Keycloak token on every `Converse` stream and admits only callers holding `aria-user`.

**Not this story:**
- minting the downstream context
- the Service router; service-to-service auth

**Acceptance criteria:**
- No token, an expired token, or one issued to another client is rejected with `UNAUTHENTICATED`
- A valid token without `aria-user` is rejected with `PERMISSION_DENIED`
- The Gateway is not Ready while it cannot fetch the realm's keys

**Open items:**
- How pods trust `fpg-ca` is undecided — interim: a ConfigMap in the chart.
- Key-cache refresh is undecided — interim: refetch on an unknown `kid`.
- Token versus stream lifetime is undecided — interim: validate at stream start only.

**Links:** [docs/services/identity.md](https://github.com/FPGSchiba/aria/blob/main/docs/services/identity.md)

**Subtasks:**
1. Add Keycloak token validation with a key cache to `aria-identity`
2. Make `keycloak.fpg` resolvable from inside the cluster
3. Enforce it on `Converse` in the Gateway
4. Test with the recorded token fixtures

### S2-04 · Mint the signed context at the Gateway and verify it in the Agent Core

*Story · `theme-identity` · 5 points · blocked by S2-03 · 118 words*

**Goal** — The Gateway mints a signed end-user context for each turn and publishes its key set; the Agent Core accepts `Decide` only with a context that verifies.

**Not this story:**
- key rotation
- verification in other services

**Acceptance criteria:**
- `Decide` without a context, or with one signed by an unknown key, is rejected
- A CLI turn reaches the Agent Core with the caller's `sub` as its user id
- The Agent Core is not Ready before it has the Gateway's key set

**Open items:**
- Signing-key generation, storage and rotation are undecided — interim: one hand-made key, sealed.
- The freshness window is undecided — interim: one context per turn, lifetime from config.

**Links:** [docs/services/identity.md](https://github.com/FPGSchiba/aria/blob/main/docs/services/identity.md)

**Subtasks:**
1. Load the signing key into the Gateway
2. Serve the Gateway's key set to verifiers
3. Mint and attach the context on each `Decide` call
4. Verify it in the Agent Core with an interceptor

### S2-11 · Prototype how the end-user context crosses a second service hop

*Spike · `theme-identity` · timebox 4 h · 93 words*

**Goal** — Evidence exists for how a service acting for the user passes on a context the next service accepts, so the audience question can be decided before the turn log is persisted.

**Not this story:**
- implementing the chosen option
- service-to-service client credentials

**Acceptance criteria:**
- Each candidate option is prototyped as a throwaway test against `aria-identity`
- The finding states, per option, what a compromised caller could do with a context it holds
- The choice is recorded in `docs/decisions/`

**Open items:**
- None.

**Timebox:** 4 h. **Output:** `docs/spikes/context-second-hop.md`.

**Links:** [docs/services/identity.md](https://github.com/FPGSchiba/aria/blob/main/docs/services/identity.md)

**Subtasks:**
1. List the candidate options
2. Prototype each as a throwaway test
3. Write the finding and the decision entry

---

## Backlog — written, not scheduled

These stories were briefed as part of sprint 2. They're fully written so the next two plannings
start from text rather than from nothing, but they go into **no sprint**. The guideline schedules
nothing beyond the sprint being planned, and each carries an open question that's answered first.

| Intended next | Runnable goal | Stories | Points |
|---|---|---|---|
| Turns survive restarts (`theme-data`) | A CLI turn is stored and survives an Agent Core restart | S2-05, S2-06, S2-07, S2-08 | 14 |
| History in the browser (`theme-clients`) | The CLI's turns appear in the web shell | S2-09, S2-10 | 10 |

The walking skeleton runs at the end of the second of these.

| ID | Type | Summary | Label | Size | Blocked by |
|---|---|---|---|---|---|
| S2-05 | Story | Provision the Postgres VM with verify-full TLS | `theme-data` | 3 | — |
| S2-06 | Story | Run the Knowledge Core against its own database with Vault credentials | `theme-data` | 5 | S2-05 |
| S2-07 | Story | Store and list turn-log events in the Knowledge Core | `theme-data` | 3 | S2-06, S2-04 |
| S2-08 | Story | Persist every turn from the Agent Core to the Knowledge Core | `theme-conversation` | 3 | S2-07, S2-11 |
| S2-09 | Story | Route browser calls to the Knowledge Core through the Service router | `theme-clients` | 5 | S2-07, S2-04 |
| S2-10 | Story | Build the Core Web shell with PKCE login and a turn history view | `theme-clients` | 5 | — |

### S2-05 · Provision the Postgres VM with verify-full TLS

*Story · `theme-data` · 3 points · 101 words*

**Goal** — A Postgres VM on the LAN accepts TLS connections by DNS name with a certificate from `fpg-ca`, and only the cluster can reach it.

**Not this story:**
- backups, monitoring, Qdrant
- any service's database

**Acceptance criteria:**
- `psql` with `sslmode=verify-full` connects by DNS name from the worker node
- A connection from another LAN host is refused
- A pod outside the `aria` namespace cannot connect

**Open items:**
- VM size awaits a measurement — interim: start small; record usage at sprint close.
- Who patches and monitors the VM is undecided — interim: Jann, by hand.

**Links:** [docs/04-deployment.md](https://github.com/FPGSchiba/aria/blob/main/docs/04-deployment.md)

**Subtasks:**
1. Create the VM and install Postgres
2. Issue its certificate from `fpg-ca` and add a DNS record
3. Restrict `pg_hba.conf` and the host firewall
4. Add the Antrea egress policy

### S2-06 · Run the Knowledge Core against its own database with Vault credentials

*Story · `theme-data` · 5 points · blocked by S2-05 · 116 words*

**Goal** — The Knowledge Core is deployed, migrates its own database as an owner role in a pre-upgrade hook, and connects as an app role with credentials Vault issues.

**Not this story:**
- turn-log RPCs, Qdrant
- other services' schemas or `aria-storage-broker`

**Acceptance criteria:**
- `helm upgrade` runs the migration as the owner role before the new pod starts
- The running pod's role cannot create a table
- The pod gets credentials from Vault as its service account; no password is in git or a hand-made Secret
- `PUBLIC` has no `CONNECT` on the database

**Open items:**
- Dynamic leases or static roles under pooling is undecided — interim: static roles rotated by Vault.

**Links:** [docs/services/knowledge-core.md](https://github.com/FPGSchiba/aria/blob/main/docs/services/knowledge-core.md)

**Subtasks:**
1. Create the database with owner and app roles
2. Configure Vault's database engine and Kubernetes auth
3. Add the Knowledge Core to the chart with a migration hook
4. Connect with `sqlx` over `verify-full`
5. Add gRPC health and spans

### S2-07 · Store and list turn-log events in the Knowledge Core

*Story · `theme-data` · 3 points · blocked by S2-06, S2-04 · 84 words*

**Goal** — The Knowledge Core appends turn-log events per session and owner, and returns them in order to their owner.

**Not this story:**
- tool-call and consent events
- retention pruning; Qdrant

**Acceptance criteria:**
- `AppendTurnEvents` then `ListTurns` returns the events in order for that session
- `ListTurns` under another user's context returns none of them
- A call without a verified context is rejected

**Open items:**
- Whether a turn-log row stores a trace ID is undecided — interim: it does not.

**Links:** [docs/services/knowledge-core.md](https://github.com/FPGSchiba/aria/blob/main/docs/services/knowledge-core.md)

**Subtasks:**
1. Write the turn-log migration
2. Add `knowledge_core.proto`, reusing the `Decide` event type
3. Implement both RPCs with `sqlx`
4. Verify the context with the Agent Core's interceptor

### S2-08 · Persist every turn from the Agent Core to the Knowledge Core

*Story · `theme-conversation` · 3 points · blocked by S2-07, S2-11 · 112 words*

**Goal** — The Agent Core writes each turn's user text and reply to the Knowledge Core under the caller's user id, and rebuilds history from it.

**Not this story:**
- tool events
- retrying failed writes

**Acceptance criteria:**
- After a CLI turn, `ListTurns` returns the user text and the complete reply
- After an Agent Core restart, the same session keeps its earlier context
- A failed write does not break the reply stream and is logged

**Open items:**
- The context's audience on a second hop is open — answered by the second-hop spike first.
- Behaviour on a failed history write is undecided — interim: the third criterion.

**Links:** [docs/services/agent-core.md](https://github.com/FPGSchiba/aria/blob/main/docs/services/agent-core.md)

**Subtasks:**
1. Call `AppendTurnEvents` when a `Decide` finishes
2. Load session history from `ListTurns`
3. Forward the user context on the call

### S2-09 · Route browser calls to the Knowledge Core through the Service router

*Story · `theme-clients` · 5 points · blocked by S2-07, S2-04 · 115 words*

**Goal** — Browser gRPC-web calls to the Knowledge Core's path prefix on the Gateway's HTTPS host arrive with a context the Gateway minted.

**Not this story:**
- extension routes or the Registry's route table
- browser conversation over WebSocket

**Acceptance criteria:**
- A gRPC-web `ListTurns` with a valid Keycloak token returns the caller's turns
- Without a token the request is refused and never reaches the Knowledge Core
- The HTTPS host presents a certificate that verifies against `fpg-ca`

**Open items:**
- Where the Service router runs is open — a spike answers it before this story is planned.
- Whether cert-manager can issue from `fpg-ca` is unknown — interim: issue it like `keycloak.fpg`'s.

**Links:** [docs/services/gateway.md](https://github.com/FPGSchiba/aria/blob/main/docs/services/gateway.md)

**Subtasks:**
1. Expose the Gateway on an HTTPS ingress host
2. Add a context-minting auth endpoint to the Gateway
3. Add the path-prefix route with external auth
4. Enable `tonic-web` on the Knowledge Core

### S2-10 · Build the Core Web shell with PKCE login and a turn history view

*Story · `theme-clients` · 5 points · 103 words*

**Goal** — A browser user logs in with PKCE and sees their past turns replayed in order.

**Not this story:**
- sending turns from the browser
- extension panels; native clients

**Acceptance criteria:**
- Opening the shell redirects to Keycloak and returns logged in
- After a CLI turn, reloading the shell shows the user text and the reply
- A second user sees none of the first user's turns

**Open items:**
- The shell's framework is open — decided when this story is planned.
- Where the shell's files are served from is open — answered with the Service router spike.

**Links:** [docs/03-architecture.md](https://github.com/FPGSchiba/aria/blob/main/docs/03-architecture.md)

**Subtasks:**
1. Scaffold the shell and add its redirect URI to the `aria` client
2. Implement the PKCE login
3. Call `ListTurns` over gRPC-web
4. Render the turns by session
5. Package the shell into the umbrella chart

---

## How the open questions are handled

Per the guideline's *decide at the last responsible moment* rule, every open item above gets one
of three treatments:

- **Interim** — the story carries it because reversing it later is cheap.
- **Spike** — it runs in the sprint before the first story that needs the answer.
- **Decision at planning** — for a question of preference rather than evidence.

| Open question | First needed by | Treatment |
|---|---|---|
| Hosted LLM provider and model | S1-06 | **Spike S1-01** (sprint 1 — the one same-sprint exception) |
| How the context's audience works on a second hop | S2-08 | **Spike S2-11**, in sprint 2 |
| Where the Service router runs — nginx-ingress or Envoy | S2-09 | **Spike**, created when the data sprint is planned; it also answers where the web shell's files are served |
| The Core Web shell's framework | S2-10 | **Decision** when that story is planned — a spike only if a prototype is wanted |
| Vault dynamic leases vs static roles | S2-06 | Interim: static roles. Contained to one service; revisit when that sprint is planned |
| Signing-key generation, storage and rotation | S2-04 | Interim: one hand-made key, sealed. Replacing the key source later touches only the Gateway |
| How pods trust `fpg-ca` | S2-03 | Interim: a ConfigMap in the chart |
| How releases reach the cluster | S1-10 | Interim: `helm` by hand |
| Service-to-service mTLS | S1-10 | Interim: plaintext gRPC; adding mTLS later removes nothing |
| `traceparent` on the hosted call; stream caps; key-cache refresh; token vs stream lifetime; freshness window; turn-log trace ID; failed history write; sealed-key location | various | Interim, each stated in its story |
| Node capacity; Postgres VM size | S1-10, S2-05 | Measured by the story itself: record the footprint |

## New open questions this plan surfaced

These aren't in [`needs-decision.md`](../open-questions/needs-decision.md) yet. That file carries
your uncommitted D60–D84 edits, so they belong in it when that batch is committed:

1. **How releases reach the cluster.** GitHub-hosted runners can't reach the LAN, so CI can
   build images but can't deploy them.
2. **The context's audience on a second hop.** `aud` names one intended callee, but a turn reaches
   the Agent Core and then the Knowledge Core on the same user's behalf. S2-11 answers it.
3. **The Core Web shell's framework.** The existing item covers the native clients only.
4. **Where the Core Web shell's static files are served from**, given the Gateway is the only
   component exposed to clients.
5. **What a failed turn-log write does to the turn in flight.**
6. **Where the sealed-secrets controller's private key is kept.** This is related to custody of the
   off-site backup key, but it's a different key.

## Found while checking

- **`services/identity.md` lists a `kid` mismatch that no longer reproduces.** It says the signer
  and verifier derive `kid` from different key bytes. At `25e25cb` they don't:
  `round_trip_succeeds` signs with a private PEM, verifies with a separately loaded public PEM, and
  passes. The open item looks stale.
- **ghcr.io package visibility is unknown.** S1-04's second criterion holds either way; a private
  package just needs a sealed pull secret.

## Jira mechanics for the creation step

- **Issue types:** Story, Spike (created 2026-09-24) and Subtask. Points go in *Story point
  estimate*, and each issue gets exactly one theme label. A spike's timebox lives in its
  description.
- **Links:** only the 16 Blocks edges in the tables above.
- **Sprints:** named `ARIA Sprint <n> — <goal>` and created without dates; dates are set when you
  start a sprint from the board. The guideline's "sprint id = 472 + n" mapping won't hold for the
  new sprints, since ids 473–488 are already taken.

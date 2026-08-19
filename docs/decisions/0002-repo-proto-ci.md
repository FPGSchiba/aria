# ARIA — Decision Log · Repo, proto & CI foundations

**Decisions D10–D15** · taken 2026-08-12 (session 1)

One entry per decision: what was decided, why, what was rejected and why, and which stories it
affects. Part of [the decision index](README.md). Several entries carry an explicit **revisit
trigger** — those are not settled forever, but must not be reopened until the trigger fires.

---

### D10 · C1 — The Knowledge Core is a deployed service; §4 was wrong

**Contradiction resolved in favour of §5.** `aria-knowledge-core` is a Deployment like the other
services. §4's list of stateless Deployments simply omitted it.

**Why.** It owns Postgres and Qdrant and exposes the gRPC API that §3 describes other services
calling. Keeping it a service confines database credentials to one workload, gives the
write-permission model (Q4) a single enforcement point, and means a schema change doesn't require
redeploying every service.

**Rejected.** *A shared library with each service reaching Postgres/Qdrant directly* — removes a
network hop from the retrieval path, which is not nothing on a voice latency budget, but spreads DB
credentials everywhere, makes the write gate unenforceable, and contradicts §3.

**Affects.** ARIA-91, ARIA-55, and the §4 text itself.

---

### D11 · C4 — `mcp-client` lives at `crates/mcp-client/`

**Contradiction resolved in favour of §7.** The §5 path table gains the missing row.

**Why.** Matches §7 and the existing ARIA-69 story, and gives the streamable-HTTP session logic its
own test surface independent of the Registry's enforcement logic.

**Rejected.** *Folding it into `services/mcp-registry/` as a module* — defensible if the Registry is
the only MCP client in the system, but that is A5's question (batch 6) and this would pre-empt it.

**Affects.** ARIA-11, ARIA-18, ARIA-69, and the §5 table.

---

### D12 · P2 — Protobuf versioning uses both a directory and a package suffix

**Decided.** `proto/<service>/v1/<service>.proto` containing `package aria.<service>.v1;`.

**Why.** This is what buf's default lint and breaking-change rules expect, which matters directly
because ARIA-16 exists to enforce protobuf hygiene. Directory alone leaves the wire package
unversioned; package alone makes a future v2 collide in the filesystem.

**Rejected.** *Package suffix only* — two versions can't coexist as files and buf's default rules
flag the mismatch. *Directory only* — the version never reaches the wire, so a breaking change is
invisible to clients and tooling.

**Affects.** ARIA-12 (closed by this), ARIA-13, ARIA-16.

---

### D13 · X9 — GitHub with GitHub Actions on hosted runners

**Decided.** GitHub is the Git host and GitHub Actions the CI platform, using GitHub-hosted
runners. This also settles E6: the Self-Extension server's repo lives on GitHub.

**Why.** No CI infrastructure to operate, and the PR surface that the Self-Extension approval gate
assumes (ARIA-97/Q5) already exists rather than needing to be stood up.

**Accepted trade-off.** Source and build logs leave the network. That is a deliberate narrowing of
the local-by-default posture from "nothing leaves" to "no *personal* data leaves" — code and CI
logs are not voice or knowledge data. Worth stating plainly in §4 so it isn't read as a slip.

**Rejected.**

- *Self-hosted runners on k3s* — keeps builds local and can reach an in-cluster registry directly,
  but means operating actions-runner-controller, and image-building runners are a privileged
  workload that interacts badly with E1's blast-radius question.
- *Gitea/Forgejo* — fully local including the registry, but another forge to operate.
- *GitLab CE* — mature CI and a built-in registry, but the heaviest footprint of the four on a
  homelab cluster.

**Open tail (bucket B).** Whether a container registry already exists in the homelab remains
ARIA-79's to report. With hosted runners, GHCR is the obvious default if none does.

**Affects.** ARIA-15, ARIA-79, ARIA-86, ARIA-97, ARIA-101, ARIA-106.

---

### D14 · P1 — Generated protobuf code is built at build time into `OUT_DIR`

**Decided.** `tonic-build` generates into `OUT_DIR`; nothing generated is committed.
`protoc-bin-vendored` (or equivalent) so contributors need no local protoc.

**Why.** Drift between schema and generated code becomes impossible by construction rather than
something CI has to police.

**Direct effect on an existing story.** ARIA-16 currently covers three things: generated types
current, schemas linted, breaking changes caught. The first is now vacuous — that story narrows to
lint plus breaking-change detection.

**Rejected.** *Committing generated code* — greppable types and diffs that show the API impact of a
schema change, but it needs a regenerate-and-diff step that fails confusingly, and it buries every
schema review in mechanical diff noise.

**Affects.** ARIA-13, ARIA-16 (scope reduced).

---

### D15 · Q13 — One umbrella Helm chart for the whole `aria` namespace (closes ARIA-21)

**Decided.** A single Helm chart covering every ARIA workload in the namespace: one release, one
`values.yaml` holding all image tags, atomic upgrade and rollback of the namespace as a unit. Each
MCP server, living outside the monorepo, gets its own small chart following the same convention.

**Why.** ARIA is one deployment serving one circle; no service is independently released. Six
near-identical charts would be boilerplate maintained for a separation that doesn't exist. The
per-MCP-server chart convention is also exactly the template `deploy_service` (ARIA-106) needs to
generate.

**Rejected.**

- *A Helm chart per service, as §7 suggested* — right if services will ever release separately;
  they won't, yet.
- *Kustomize* — least machinery for one environment, but CI-driven image bumps need a kustomize
  edit step rather than a values override, and generated MCP-server packaging gets no template.

**Affects.** ARIA-21 (closed by this), ARIA-86, ARIA-91, ARIA-93, ARIA-94, ARIA-106. Requires a §7
text change — the "pick one convention, don't mix" sentence now has an answer.

---

# ARIA — Repos, crates & working conventions

[Library index](README.md) · [Decisions](decisions/README.md) · [Open questions](open-questions/README.md) · [Sprint 1](sprints/sprint-01.md)

The monorepo layout and the conventions every service follows. §7 is explicitly a living section:
it is filled in as code lands, and ARIA-18 owns the next expansion of it.

**Source:** CLAUDE.md §5 and §7 as of 2026-08-12, moved here verbatim.

---

## Repos & crates

Single `aria` monorepo (Cargo workspace) for now — the names below double as container image
tags today and as standalone repo names if any service is ever split out later. Hosted on GitHub.

| Name | Path in monorepo | Kind |
|---|---|---|
| `aria-gateway` | `services/gateway/` | Deployed service |
| `aria-speech` | `services/speech/` | Deployed service (GPU node) |
| `aria-agent-core` | `services/agent-core/` | Deployed service |
| `aria-mcp-registry` | `services/mcp-registry/` | Deployed service |
| `aria-knowledge-core` | `services/knowledge-core/` | Deployed service |
| `aria-kc-broker` | `services/kc-broker/` | Deployed service (Keycloak scope provisioning) |
| `aria-identity` | `crates/identity/` | Shared library crate (not deployed) |
| `mcp-client` | `crates/mcp-client/` | Shared library crate (streamable-HTTP MCP sessions) |
| `proto` | `proto/` | Shared protobuf schemas |
| *(Self-Extension MCP server)* | separate GitHub repo, TBD name | First MCP server — external to the `aria` monorepo by design, like any other MCP server |

Per-service proposed structure, frameworks, and an internals diagram live on each service's
Confluence page (children of "ARIA — Architecture & Hosting").

## Working conventions

*(Filled in as decisions land; keep this section current once code exists.)*

- Cargo workspace, one crate per service (see section 5 for names/paths).
- Shared crates: `proto` (generated `tonic`/`prost` types), `aria-identity` (Keycloak JWT
  validation, client-credentials middleware, and signed end-user context minting/verification),
  `mcp-client` (shared MCP client logic used by the Registry).
- Protobuf schemas live in a top-level `proto/` directory at `proto/aria/<service>/v1/<service>.proto`
  with `package aria.<service>.v1;` — both the directory and the package carry the version, and the
  directory matches the package so buf's default lint passes ([D87](decisions/0020-proto-layout.md);
  `<service>` is the snake_case package segment, e.g. `agent_core`).
  Generated code is produced at build time into `OUT_DIR` via `tonic-build`; nothing generated is
  committed, and `protoc` is vendored so contributors need nothing installed.
- Database access uses **`sqlx`** with compile-time-checked queries and its built-in migration
  harness — chosen because recursive CTEs over the relationship table and JSONB attribute columns
  are exactly where an ORM adds friction rather than removing it.
- New capabilities are written as standalone MCP servers (any language) deployed as their own
  workload, in their own repo — keep the core `aria` repo free of per-integration logic. The
  Self-Extension server follows this same rule: it's not part of the core monorepo either.
- Container images per service, published from GitHub Actions; deployed to the `aria` namespace on
  the existing cluster via **one umbrella Helm chart** for the namespace (section 4). Each
  out-of-monorepo MCP server carries its own small chart following the same convention.
  *(The source text said "k3s"; ARIA-79 found the cluster is **kubeadm v1.33.0**. Corrected here
  because it is a factual error, not a decision.)*
- **Repo gate**: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace`, and the coverage gate below must all pass before a commit
  lands. Enforced locally via a checked-in pre-commit hook (`.githooks/pre-commit`) — enable it
  once per clone with `git config core.hooksPath .githooks`. Requires `cargo install
  cargo-llvm-cov` and `rustup component add llvm-tools-preview` once per machine. **Also enforced
  in CI** by `.github/workflows/ci.yml` (ARIA-140) on every push and pull request. fmt, clippy,
  coverage and a Windows compile run there as parallel jobs, and one aggregate job, `gate`, is the
  check `main` requires: a repository ruleset (`.github/rulesets/main.json`) allows changes only
  through a pull request with a passing `gate`, with no bypass, admins included.
- **Mirror rule (Jann's call, 2026-10-06): the gate is defined twice, in `.githooks/pre-commit` and
  in `.github/workflows/ci.yml`, and a change to the gate changes both files in the same commit.**
  Every check in the hook has a CI counterpart. Two differences are deliberate: CI has no separate
  `cargo test` run, because the coverage job runs the whole suite and fails on a failing test; and
  CI may add platform-only checks the hook cannot run (the Windows `cargo check`).
- **Coverage rule (Jann's call, 2026-08-28): 80% minimum, for all automated testing.** Measured as
  line coverage via `cargo llvm-cov --workspace --fail-under-lines 80` — that's what "coverage"
  means in this repo unless stated otherwise, to avoid the term drifting between line/function/
  region coverage across crates. Applies workspace-wide as each crate gains real code; enforced per
  commit by the pre-commit hook and per pushed head by CI's coverage job, so it's a hard floor, not
  an aspirational target.

### Definition of done (D84, 2026-09-23)

A service is **not done** without three things beyond its code, its tests and the 80% coverage
floor above:

1. **Its migration** — the `sqlx` migrations for the schema it owns. With schema-per-service and
   the `*_owner` / `*_app` role split ([D73](decisions/0015-external-datastores.md),
   [D77](decisions/0015-external-datastores.md)), the migration is also *where the ownership
   boundary is expressed* — deferring it means the boundary exists only as an intention.
2. **Its Helm chart** — its slice of the umbrella chart, written while the service is, not later
   under deployment pressure. A service that joins the umbrella release late is a service whose
   first rollback was never tested.
3. **Its OpenTelemetry spans** — per [D82](decisions/0016-observability-v2.md) and the four
   propagation points in [D83](decisions/0016-observability-v2.md). This is the one that never gets
   added later, because nothing breaks without it: the system runs fine and simply cannot be
   debugged.

**This is a convention held by review, not a gate.** None of the three is checkable by `cargo`, and
no CI workflow exists yet (D13). A cheap static check — "every service directory has a
`migrations/` and a chart fragment" — is worth adding once there is more than one service to check;
see [D84](decisions/0017-conventions.md) for why it was not added now.

This rule also has to reach servers nobody has written yet: it belongs in the **generated-server
template** the Self-Extension server produces, alongside D83's `traceparent` header.

---

## See also

- [Decision Log · Repo, proto & CI](decisions/0002-repo-proto-ci.md)
- [Decision Log · Working conventions](decisions/0017-conventions.md) — D84
- [Architecture](03-architecture.md)

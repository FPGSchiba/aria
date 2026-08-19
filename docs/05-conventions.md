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
- Protobuf schemas live in a top-level `proto/` directory at `proto/<service>/v1/<service>.proto`
  with `package aria.<service>.v1;` — both the directory and the package carry the version.
  Generated code is produced at build time into `OUT_DIR` via `tonic-build`; nothing generated is
  committed, and `protoc` is vendored so contributors need nothing installed.
- Database access uses **`sqlx`** with compile-time-checked queries and its built-in migration
  harness — chosen because recursive CTEs over the relationship table and JSONB attribute columns
  are exactly where an ORM adds friction rather than removing it.
- New capabilities are written as standalone MCP servers (any language) deployed as their own
  workload, in their own repo — keep the core `aria` repo free of per-integration logic. The
  Self-Extension server follows this same rule: it's not part of the core monorepo either.
- Container images per service, published from GitHub Actions; deployed to the `aria` namespace on
  the existing k3s cluster via **one umbrella Helm chart** for the namespace (section 4). Each
  out-of-monorepo MCP server carries its own small chart following the same convention.

---

## See also

- [Decision Log · Repo, proto & CI](decisions/0002-repo-proto-ci.md)
- [Architecture](03-architecture.md)

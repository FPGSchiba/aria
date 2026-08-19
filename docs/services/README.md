# ARIA — Services

[Library index](../README.md) · [Architecture](../03-architecture.md) · [Decisions](../decisions/README.md) · [Open questions](../open-questions/README.md)

One page per service: what it owns, which decisions bind it, and what is still open inside it.

| Service | Kind | Page |
|---|---|---|
| `aria-gateway` — Gateway / Interface | Deployed service · `services/gateway/` | [gateway.md](gateway.md) |
| `aria-speech` — Speech | Deployed service (GPU node) · `services/speech/` | [speech.md](speech.md) |
| `aria-agent-core` — Agent Core (DEC) | Deployed service · `services/agent-core/` | [agent-core.md](agent-core.md) |
| `aria-mcp-registry` — MCP Registry | Deployed service · `services/mcp-registry/` | [mcp-registry.md](mcp-registry.md) |
| `aria-knowledge-core` — Knowledge Core | Deployed service · `services/knowledge-core/` | [knowledge-core.md](knowledge-core.md) |
| `aria-identity` — Identity | **Library crate, not a deployed service** · `crates/identity/` | [identity.md](identity.md) |
| `aria-kc-broker` — Keycloak provisioning broker | Deployed service · `services/kc-broker/` | [kc-broker.md](kc-broker.md) |
| Self-Extension MCP Server | **Separate GitHub repo** — external to the `aria` monorepo by design, like any other MCP server | [self-extension.md](self-extension.md) |

**Deferred, not in initial scope:** the Trainer.

---

The system-level view — how these fit together — is in [`../03-architecture.md`](../03-architecture.md).

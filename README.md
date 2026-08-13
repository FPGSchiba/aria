# aria

A rather intelligent assistant

ARIA is a self-hostable, multi-service personal assistant, built as a Rust Cargo workspace.
See [`CLAUDE.md`](https://firephoenixgames.atlassian.net/wiki/spaces/ARIA) (Google Drive /
Confluence) for the full architecture and decision record — this file just orients you in the
repo layout.

## Workspace layout

Single Cargo workspace, one crate per service/library. The crate (package) name is unprefixed;
the `aria-*` name is the corresponding service/container image name used in deployment.

| Crate (package name) | Path                       | Service name          | Kind                                |
|----------------------|----------------------------|-----------------------|-------------------------------------|
| `gateway`            | `services/gateway/`        | `aria-gateway`        | Deployed service                    |
| `speech`             | `services/speech/`         | `aria-speech`         | Deployed service (GPU node)         |
| `agent-core`         | `services/agent-core/`     | `aria-agent-core`     | Deployed service                    |
| `mcp-registry`       | `services/mcp-registry/`   | `aria-mcp-registry`   | Deployed service                    |
| `knowledge-core`     | `services/knowledge-core/` | `aria-knowledge-core` | Deployed service                    |
| `identity`           | `crates/identity/`         | `aria-identity`       | Shared library crate (not deployed) |

`proto/` holds shared protobuf schemas (`proto/<service>/v1/<service>.proto`), built at compile
time via `tonic-build` — nothing generated is committed.

Each service currently exists as a compiling placeholder (`main.rs`/`lib.rs` stub); shared
dependency versions (`tonic`, `prost`, `tokio`, `tracing`, ...) are pinned once in the root
`Cargo.toml` under `[workspace.dependencies]`.

## Building

```
cargo build --workspace
cargo test --workspace
```

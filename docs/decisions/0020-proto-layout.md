# ARIA — Decision Log · Protobuf file layout

**Decision D87** · taken 2026-10-05 (first full repo review)

One entry per decision: what was decided, why, what was rejected and why, and what it affects.
Part of [the decision index](README.md).

---

### D87 — Supersedes D12's file path: schemas live at `proto/aria/<service>/v1/`

**Decided.** A schema lives at `proto/aria/<service>/v1/<service>.proto`, where `<service>` is the
**protobuf package segment** (snake_case, e.g. `agent_core`), not the crate or image name. The buf
module root is `proto/`. The package stays exactly as [D12](0002-repo-proto-ci.md) set it:
`package aria.<service>.v1;`. D12's substance — the version is carried by **both** a directory and
a package suffix — is unchanged; only the path that expresses it changes.

**Trigger.** The first full review of the repo found that D12's path
(`proto/<service>/v1/<service>.proto` with `package aria.<service>.v1;`) does not satisfy buf's
`PACKAGE_DIRECTORY_MATCH` rule, which is in buf's default lint set: a file in package
`aria.gateway.v1` must sit in `aria/gateway/v1/` relative to the module root. D12's stated reason —
"this is what buf's default lint and breaking-change rules expect" — was therefore wrong for the
layout it chose. The repo also showed that `<service>` cannot be one string: the directory was
`agent-core/` while the package and file were `agent_core`.

**Why.** It keeps D12's reason true rather than editing the reason to fit: buf's default rules pass
on directory/package agreement without a lint exception, so the future lint story starts from zero
exceptions. The package names on the wire do not change, so no generated Rust path changes either.

**Rejected.**

- *Keep D12's directories and add a buf `except: [PACKAGE_DIRECTORY_MATCH]`.* No files move, but it
  disables the very rule D12 cited as its justification, and every future schema inherits the
  exception.
- *Record it as an open question and decide later.* Cheapest now, but only two schemas exist; every
  schema added before the decision would also have to move.

**Not decided here.** buf's default set also flags service names without a `Service` suffix
(`Gateway`, `AgentCore`). Whether ARIA renames them or excepts that rule is open —
see [needs-decision](../open-questions/needs-decision.md#surfaced-by-the-first-full-repo-review-2026-10-05).

**Affects.** ARIA-130 (its schemas move), the future protobuf lint/breaking-change story (formerly
ARIA-16), every service story that adds a schema, and `02-stack.md`, `05-conventions.md` and the
service pages that quote the path.

---

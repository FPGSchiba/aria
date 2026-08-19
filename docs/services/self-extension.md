# Self-Extension MCP Server

[Library index](../README.md) · [Architecture](../03-architecture.md) · [Decisions](../decisions/README.md) · [Open questions](../open-questions/README.md)

****Separate GitHub repo** — external to the `aria` monorepo by design, like any other MCP server**

---

## Purpose

The first MCP server ARIA gets. Lets ARIA draft, generate, test, and — only after Jann approves —
deploy new MCP servers. Extending ARIA's reach stops being solely a task for Jann to do by hand.

## Replaces

Nothing. This is the fifth differentiator from `01-vision.md` made concrete.

## Owns

Proposed tools: `draft_service`, `generate_code`, `run_tests`, `request_approval`, `deploy_service`.

**The approval gate:** `request_approval` opens a PR adding the server to `approved-servers.yaml`
with its image digest. **Merging the PR *is* the approval** — so nothing ARIA runs can approve
itself. A CI step on merge builds the image, seals the service's credentials, and calls the MCP
Registry's `RegisterServer`.

**Credential lifecycle (D34):** nothing exists before approval, because the only thing that creates
credentials runs *after* it. This server never holds or issues credentials itself.

## Binding decisions

D13 (GitHub is the host) · D15 (own small Helm chart on the shared convention) · D34 (approval pipeline seals credentials) · D44 (`approved-servers.yaml`, merge is approval) · D45 (image digest — a rebuild needs re-approval)

See [the Decision Log](../decisions/README.md) for the full reasoning and rejected alternatives.

## Contract

An MCP server over streamable HTTP, with its own CI.

## Open items

- **PR vs. conversational approval surface** — the two can coexist for different stakes (ARIA-97)
- **How much cluster authority this server holds.** A service that can create pods to run
  unreviewed, model-written code is an escalation path in its own right; the blast radius should be
  named explicitly rather than fall out of implementation (ARIA-98)
- Whether an approval can be revoked after a service is live, and what that does to a running
  workload (ARIA-105)
- **Generated candidates' tests are themselves generated** — how to avoid the model marking its own
  homework (ARIA-104)
- Which language generated services are written in (ARIA-103)
- Which model does the generating — this must **not** be inherited from ARIA-40 (ARIA-102)
- The repo name (D13 settled only the host)
- **Awaiting measurement:** where sandboxed `run_tests` execution physically runs (ARIA-98)

## Jira stories

ARIA-97, 98, 100, 101, 102, 103, 104, 105, 106, 107

---

*Derived from `03-architecture.md` and the Decision Log. Last updated 2026-08-19.*

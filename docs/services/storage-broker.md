# `aria-storage-broker` — Storage provisioning broker

[Library index](../README.md) · [Architecture](../03-architecture.md) · [Decisions](../decisions/README.md) · [Open questions](../open-questions/README.md)

**Deployed service · `services/storage-broker/`**

---

## Purpose

Creates a newly approved MCP server's **Postgres schema and grants**, at approval time, and does
nothing else.

## Replaces

Nothing. Like [`aria-kc-broker`](kc-broker.md), this service exists *because of* a decision rather
than as a modernisation of an older box: [D73](../decisions/0015-external-datastores.md) gave every
service its own schema with an owner/app role split, which means something has to create them —
and that something needs a credential that can create schemas and issue grants.

## Owns

One operation: reconcile the schema, roles and grants for a named, **already approved** MCP server.

## Binding decisions

[D73](../decisions/0015-external-datastores.md) (schema per service, `*_owner` / `*_app` split) ·
[D74](../decisions/0015-external-datastores.md) (cross-schema reads are read-only views) ·
[D75](../decisions/0015-external-datastores.md) (grants issued at approval time; the broker is a
dumb reconciler) · [D77](../decisions/0015-external-datastores.md) (the Knowledge Core's own
database, which the broker structurally cannot grant across)

See [the Decision Log](../decisions/README.md) for the full reasoning and rejected alternatives.

## Deliberately dumb

The broker **takes no decisions**. It reads the approval artifact — the same
`approved-servers.yaml` that gates registration ([D44](../decisions/0007-observability-consent.md),
[D45](../decisions/0007-observability-consent.md)) — and reconciles Postgres to match it. It does
not decide what a server should be allowed to read, it does not widen a grant on request, and it
refuses anything that does not match its pattern.

This is the same shape as `aria-kc-broker`, for the same reason: **the MCP Registry handles
LLM-chosen tool traffic and must never hold a credential like this one.** A prompt-injected tool
call that reaches the broker reaches one narrow reconciler operating from a git-reviewed file, not
a database superuser.

The timing matters as much as the narrowness. Grants are issued **at approval time** — when a human
merged a PR — and not at runtime on a server's request. There is no path by which a running server
asks for more access than its approval recorded.

## What it structurally cannot do

[D77](../decisions/0015-external-datastores.md) puts the **Knowledge Core in its own database**,
not merely its own schema. A cross-schema grant from an MCP server's schema to the Knowledge
Core's tables is therefore **not expressible in Postgres at all** — it is not a rule the broker
enforces and could get wrong, it is a grant that cannot be written. MCP servers reach person data
through the Knowledge Core's narrow `ResolvePerson` call behind its own consent scope
([D70](../decisions/0013-extension-surface.md)), or not at all.

## Contract

Not yet specified. gRPC, per [D12](../decisions/0002-repo-proto-ci.md)'s versioning conventions:
`proto/storage-broker/v1/storage-broker.proto`, `package aria.storage_broker.v1;`.

## Residual risk — recorded, not solved

The broker holds a Postgres credential capable of creating schemas and issuing grants across the
ARIA database. That is narrower than a superuser and narrower than `aria-kc-broker`'s realm-capable
Keycloak credential — **but the pattern policy is ARIA's code, not Postgres's enforcement**, which
is the same shape of residual risk `aria-kc-broker` records. What limits the blast radius
structurally rather than by policy is D77: the one grant that would matter most cannot be issued
from this database at all.

## Open items

- **Deprovisioning.** What happens to a schema, its data and its roles when a server's approval is
  revoked or its image digest changes. Nothing decided; related to the long-standing open question
  about revoking an approval after a service is live.
- **Reconciliation drift.** Whether the broker runs continuously or only on approval, and what it
  does when it finds Postgres and the approval artifact disagree.
- **Credential source.** [D76](../decisions/0015-external-datastores.md) puts database credentials
  in Vault; whether the broker's own credential is issued the same way, and how it is rotated,
  is not settled.

## Jira stories

None yet — this service postdates the current backlog and needs stories written.

---

*Derived from `03-architecture.md` and the Decision Log. Last updated 2026-09-23.*

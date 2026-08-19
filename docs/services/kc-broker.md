# `aria-kc-broker` — Keycloak provisioning broker

[Library index](../README.md) · [Architecture](../03-architecture.md) · [Decisions](../decisions/README.md) · [Open questions](../open-questions/README.md)

**Deployed service · `services/kc-broker/`**

---

## Purpose

A small service holding **the only Keycloak admin credential in the cluster**. Exposes exactly one
operation — create/reconcile the `aria:mcp:` scopes for a named MCP server — and refuses any scope
name outside that prefix.

## Replaces

Nothing. This service exists *because of* a decision: D7 chose per-tool Keycloak scopes, which forced runtime scope provisioning, which forced a credential holder.

## Owns

One narrow API. Nothing else.

## Binding decisions

D7 (per-tool scopes — the forcing decision) · D8 (broker, deliberately separate from the Registry)

See [the Decision Log](../decisions/README.md) for the full reasoning and rejected alternatives.

## Contract

A single create/reconcile-scopes operation with a hardcoded `aria:mcp:` prefix policy.

## Why it is separate from the MCP Registry

The MCP Registry handles **LLM-chosen tool traffic** and is therefore the worst possible holder of a
realm-capable admin credential. A broker with a hardcoded prefix policy means a prompt-injected tool
call reaches one narrow API and nothing else.

## Residual risk — recorded, not solved

Keycloak's client-management rights are **realm-wide**, and this realm is shared with other homelab
apps. Keycloak cannot scope scope-creation below realm level, so the broker's credential remains
realm-capable and **the prefix policy is ARIA's code, not Keycloak's enforcement**. This is a real
limitation of the chosen design, not an implementation detail to be fixed later.

## Open items

- Scale-to-zero when no registration is in flight (ARIA-111)
- Does removing a Keycloak scope cascade to `consent_grants` rows (ARIA-110)

## Jira stories

ARIA-110, 111

---

*Derived from `03-architecture.md` and the Decision Log. Last updated 2026-08-19.*

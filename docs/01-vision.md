# ARIA — Vision, scope & non-goals

[Library index](README.md) · [Decisions](decisions/README.md) · [Open questions](open-questions/README.md) · [Sprint 1](sprints/sprint-01.md)

What ARIA is, who it serves, how it differs from the alternatives, and what it deliberately
will not become. This is the *why*; everything else in this library is downstream of it.

**Source:** CLAUDE.md §1 and §6 as of 2026-08-12, moved here verbatim.

---

ARIA = "A Rather Intelligent Assistant" — Jann Erhardt's (FPG Schiba's) personal AI assistant,
originally started in 2021 as a voice-first, Python-based system with a custom socket protocol,
a hand-rolled user/permission database, and a hand-built sharded graph store for knowledge.
This project modernizes the *implementation* while keeping the original *ambition*: a
proactive assistant that runs a person's (and their close circle's) daily life.

### Vision / differentiation

The closest existing comparisons are self-hosted voice assistants — Home Assistant Assist,
Mycroft/OpenVoiceOS, Rhasspy. ARIA is deliberately not a general-purpose chat UI (Open WebUI,
LibreChat) or an autonomous task/coding agent (AutoGPT, OpenDevin-style tools). Against the
voice-assistant comparison set, ARIA differentiates on:

1. **Extensible by anyone, without touching the core.** New capabilities are added by writing
   an **MCP (Model Context Protocol) server** — e.g. a Sonos controller — and telling ARIA to
   connect to it. ARIA acts as an **MCP Host**: it discovers whatever tools that server exposes
   and makes them available to the Agent Core immediately, with no core-code changes and no
   central "connector registry" to maintain by hand.
2. **Relationship-aware, not just multi-user.** The core knows the people it serves and how
   they relate to each other (originally the legacy `relation` / `relation_type` schema), not
   just isolated per-account skills.
3. **A real knowledge core, not a user table.** The center of ARIA is a **RAG system**: a
   knowledge base of people, their preferences, their relationships, and what ARIA has learned
   from past interactions — extendable by every connected MCP service, not owned by any one of
   them.
4. **Self-hosted identity and memory that are actually solved, not glued on.** Keycloak and
   Qdrant/Postgres replace hand-rolled auth and a hand-rolled sharded graph store, so effort
   goes into assistant behavior instead of reinventing infrastructure the open-source world
   already does well — and it reuses infrastructure Jann already runs at home rather than
   standing up parallel copies of it.
5. **Grows its own capabilities, under approval.** The first MCP server ARIA gets isn't a
   controller for some device — it's a **Self-Extension server** that lets ARIA design,
   generate, and (once Jann approves) deploy new MCP servers herself. Extending ARIA's reach
   stops being solely a task for Jann to do by hand.

### Scope

Personal + close circle: Jann plus a small number of friends/family, each modeled as a person in
the Knowledge Core and — where they have one — linked to a real Keycloak account. **Not** a
multi-tenant SaaS — each deployment serves one circle. Note that not every person ARIA knows has
an account: some circle members already exist in the shared realm via other homelab apps, some do
not, and some never will (see section 2, Knowledge Core).

## Explicit non-goals

- Not reinventing a graph database — a typed `person`/`relationship` model in Postgres, with
  recursive CTEs for traversal.
- Not reinventing auth/user management — reuse the existing Keycloak instance.
- Not hand-building NLU/NLG pipelines — the pluggable LLM handles understanding and generation
  directly.
- Not implementing calendar recurrence, timezone arithmetic or attendee state — mirror an
  external calendar instead.
- Not forking the MCP protocol — ARIA-specific needs (dynamic registration, approval gating,
  user context) live in the MCP Registry's control plane, not in a modified protocol.
- Not building a multi-tenant SaaS platform.
- Not standing up new infrastructure that duplicates what's already running (Keycloak, Ollama,
  Kubernetes) — extend the existing homelab instead. Note this now means *reuse with
  modification*, not *reuse untouched*: ARIA may reconfigure the shared Ollama instance
  (section 4).
- Not committing to the original iOS/Swift or Android/Xamarin client plans yet — the
  gRPC-based Gateway service stays client-agnostic.

---

## See also

- [Technology stack](02-stack.md) — the decisions that implement this vision
- [Architecture](03-architecture.md) — the services those decisions produce

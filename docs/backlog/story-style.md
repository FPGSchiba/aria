# ARIA — Jira story style

[Library index](../README.md) · [Backlog coordinates](README.md) · [Decisions](../decisions/README.md)

**A story description is read by a person deciding what to build next.** It is not a briefing
document, not a context dump, and not a place to re-explain decisions that already have a home.

The 2026-08-12 descriptions were written to give an LLM maximum context. That optimised for the
wrong reader: they run 500–900 words, quote the architecture docs at length, and restate each
binding decision in full. The information is correct — it is simply in the wrong place, repeated,
and unreadable at a glance.

**Target: a story should be understood in about 30 seconds.** Roughly 150–200 words.

---

## The shape

Exactly these sections, in this order. Omit any that would be empty — never write "N/A".

```markdown
[1–2 sentences: what this story delivers, and any constraint that changes how.]

**Not this story:** [one line — the scope boundary, and where the excluded work lives]

**Acceptance criteria**
- [testable, one line each]

**Decisions**
- D<n> — [one sentence: what it means *for this story*]

**Open**
- [one line each — genuinely undecided things this story must resolve or work around]

**Links:** [Decision Log] · [service page] · [any spike brief]
```

### Worked example — ARIA-19

```markdown
Add ARIA to the existing shared Keycloak realm (`master`). Additive change to a live
realm other homelab apps depend on.

**Not this story:** no Rust code, no user CRUD. Onboarding missing circle members is ARIA-114.

**Acceptance criteria**
- Pre-change realm state captured: issuer, discovery URL, existing clients and roles
- ARIA exists as a **public** client; PKCE and Device Grant both exercised once
- Client roles `aria-user` + `aria-admin` on the ARIA client, not realm roles
- Two decoded tokens captured: one with an ARIA role, one without
- Client config committed in reproducible form
- A non-ARIA app in the same realm still logs in after the change

**Decisions**
- D2 — `user_id` is the Keycloak `sub` claim
- D3 — public client, PKCE + Device Grant on the same client
- D4 — some circle members already have accounts, some never will
- D47 — two client roles: `aria-user`, `aria-admin`
- D53 — ARIA registers in the `master` realm

**Links:** Decision Log · Identity page · ARIA-19 facts
```

That replaces ~700 words with ~170, and loses nothing a reader needs.

---

## Rules

**1. Acceptance criteria are testable, one line, no rationale.**
A criterion states a condition someone can check. *Why* it exists belongs to the decision that
created it. If a criterion cannot be checked, it is not a criterion — it is a note.

> ❌ "The realm's pre-change state is captured in writing before anything is touched: issuer URL,
> OIDC discovery document URL, the existing client list, existing realm-level roles, and any naming
> conventions other apps (e.g. Finance Manager) already follow — so later ARIA stories cannot
> collide with them."
>
> ✅ "Pre-change realm state captured: issuer, discovery URL, existing clients and roles"

**2. A decision is one sentence and a number. Never a paragraph.**
The decision log holds what was decided, why, and what was rejected. A story says only what that
decision means *here*. If a reader needs the reasoning, the link is right there.

> ❌ 180 words beginning "**Decided 2026-08-12 (D3):** The user-facing ARIA client is a single
> **public** Keycloak client driving Authorization Code with PKCE…"
>
> ✅ "D3 — public client, PKCE + Device Grant on the same client"

**3. Never quote the architecture docs.** Link instead. Quotes go stale silently: the source is
edited, the copy in Jira is not, and nobody notices until someone builds from the copy.

**4. "Not this story" earns its line.** It is the one part of the old format worth keeping verbatim
in spirit — scope boundaries prevent duplicated work and are invisible anywhere else.

**5. Open items are one line and genuinely open.** If it has been decided, it belongs under
**Decisions**. If nothing in this story depends on it, it belongs in
[`open-questions/`](../open-questions/README.md) and not here at all.

**6. Bold sparingly.** The old descriptions bold whole clauses, which makes nothing stand out. Bold
a term the reader must not misread — `public` client, `not` realm roles — and nothing else.

---

## Rewriting an existing story

**The substance is correct. Only the presentation is wrong.** So this is compression, not
redrafting.

- **Never drop an acceptance criterion.** Compress the wording; keep the condition. If two
  criteria genuinely test the same thing, merge them and say so in the rewrite notes.
- **Never invent a criterion.** If something obviously *should* be tested and is not, note it
  separately — do not add it silently.
- **Never resolve an open item.** If a story says something is undecided, it stays undecided. That
  rule is why this backlog is trustworthy.
- **Preserve every D-number.** If a story cites D16, the rewrite cites D16. Losing the citation
  loses the audit trail.
- **Preserve "out of scope" content** as the "Not this story" line.
- **Delete freely:** quoted passages from the architecture docs, restated rationale, "Why this
  story exists" preambles, and any sentence explaining something the linked decision explains
  better.

---

## Links

**Jira cannot open a Google Drive path.** Per **D54**, the library lives in the `aria` monorepo on
GitHub, so every document has a clickable URL:

```
https://github.com/FPGSchiba/aria/blob/main/docs/decisions/0003-knowledge-model.md
```

Link to the **file**, not to a heading anchor — anchors break when a document is reorganised, and
the file is small enough to scan.

Three links cover almost every story:

| Link | When |
|---|---|
| Decision Log (the relevant file) | Whenever the story cites a D-number |
| The service page under `docs/services/` | Whenever the story belongs to a service |
| A spike brief under `docs/spikes/` | Whenever evidence exists for this story |

---

## Epics

Epics get the same treatment, shorter: two or three sentences on what the epic delivers and where
its boundary sits. No acceptance criteria — those live in the stories.

---

*Established 2026-08-19 after the descriptions proved unreadable at a glance. The rework prompt
that applies this guide is [`story-rework-prompt.md`](story-rework-prompt.md).*

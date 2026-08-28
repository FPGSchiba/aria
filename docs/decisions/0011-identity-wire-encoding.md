# ARIA — Decision Log · Identity wire encoding

**Decision D59** · taken 2026-08-27, during ARIA-28 implementation review.

Part of [the decision index](README.md).

---

### D59 (ARIA-28) — The Gateway-signed context keeps four standard JWT claims beyond D1's minimum

**Decided.** The signed end-user context's payload is `user_id`, `roles`, `exp`, `jti` (D1's four)
plus `aud`, `iss`, `nbf`, `iat`. This is the full claim set for `sign()`/`verify()`'s wire encoding —
see [Wire encoding](../services/identity.md#wire-encoding).

**Why (Jann's call).** During ARIA-28 implementation review, trimming the payload back to exactly
D1's four fields was raised as the literal reading of the acceptance criteria ("the payload carries
exactly `user_id` and roles plus only what the signing mechanism requires"). Jann kept the four extra
fields instead: `aud`/`iss` let `verify()` reject a context minted for the wrong callee or by
something other than the expected issuer; `nbf`/`iat` are conventional JWT hygiene fields. All four
are standard registered JWT claims (RFC 7519 §4.1), not ARIA-specific additions requiring their own
validation logic.

**Rejected.**

- *D1's four fields only* — the minimal reading of the acceptance criteria; rejected as unnecessarily
  strict once the fields in question are standard, already-understood JWT claims rather than new
  custom fields each needing their own justification.

**Affects.** ARIA-28 (owner).

---

# ARIA — Decision Log · Container packages

**Decision D86** · taken 2026-10-05 (planning — ARIA-123)

One entry per decision: what was decided, why, what was rejected and why, and what it affects.
Plan: [`2026-10-05-aria-123-ghcr-images.md`](../plans/2026-10-05-aria-123-ghcr-images.md).

---

### D86 · ARIA-123 — ARIA's container packages on `ghcr.io` are public

**Decided.**

- The `ghcr.io` packages for ARIA's core services (`aria-gateway`, `aria-agent-core`, and every core
  service image that follows) are **public**. The cluster pulls them **anonymously**, with no
  `imagePullSecret`.
- **No secret is ever baked into an image.** Credentials reach a pod only at runtime, through
  sealed secrets (D31) or Vault (D76). This rule is what makes public packages safe; it is part of the
  decision, not a side note.
- **No homelab CA is baked into an image either.** How pods trust `fpg-ca` stays open
  (`open-questions/needs-decision.md`); this decision does not answer it by implication.

**Why.**

1. **The repo is already public.** An image built from public source exposes nothing the source
   does not, provided the rule above holds. Private packages would protect nothing.
2. **It needs no credential at all.** The workflow pushes with its automatic token. The cluster
   pulls with nothing. There is no PAT to keep alive, seal, rotate or leak.
3. **It keeps ARIA-123 independent of ARIA-169.** A private package needs a sealed pull secret,
   which needs the sealed-secrets controller. The sprint plan lists ARIA-123 with no dependency;
   private packages would have silently added one.

**Rejected.**

- *Private packages plus a sealed `imagePullSecret`.* Images stay non-public. It costs a dependency
  on ARIA-169 and a **long-lived personal access token**. The workflow's own token cannot be used
  from the cluster, so the pull credential would have to be a PAT with `read:packages`, which
  expires and must be re-sealed when it does. That is all cost for no protection, given point 1.

**Departure, stated rather than buried.** D51 rejected Docker Hub partly because its
*"public-by-default visibility"* was *"wrong for this"*. That leaned towards private packages
without deciding it. D86 consciously goes the other way, for the reasons above. The D51 objection
still holds against Docker Hub's pull rate limits; it no longer holds against visibility.

**Accepted cost.**

- **Anyone can pull ARIA's core images.** Acceptable for the same reason the repo is public.
- **Built artifacts live off-site and are publicly readable.** D51/D52 already added built artifacts
  to §4's list of what crosses the network boundary; that row now says *public*.
- **Possibly one manual step.** A package first pushed from a workflow under a personal account may
  be created private. If so, its visibility is switched to public once, by hand, in the package
  settings. ARIA-123's cluster pull check confirms which applies.

**Revisit trigger.** If the `aria` repo ever becomes private, or if an image ever has to carry
something that may not be public, this decision must be reopened. The answer is then D52's private
path, and ARIA-169's controller will exist by then.

**Affects.** ARIA-123, ARIA-177, ARIA-169 (no longer a prerequisite for pulling).

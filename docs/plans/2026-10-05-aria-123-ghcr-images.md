---
title: Publish Gateway and Agent Core images to ghcr.io from CI (ARIA-123)
slug: aria-123-ghcr-images
tier: technical
status: active
created: 2026-10-05
branch: aria-123-ghcr-images
owns_branch: true
---

## Goal

Every merge to `main` builds the `aria-gateway` and `aria-agent-core` images and pushes them to
`ghcr.io`, tagged with the full commit SHA. A pull request builds both images without pushing, so a
broken image build fails before merge. Each image can be pulled by a pod on the cluster with no
credential, because the packages are public. Story:
[ARIA-123](https://firephoenixgames.atlassian.net/browse/ARIA-123).

## Non-goals

- Images for services that do not exist yet (Speech, Knowledge Core, MCP Registry).
- Deploying the images. That is ARIA-177 (the umbrella Helm chart), which this story unblocks.
- Mirroring the repo gate (fmt, clippy, tests, coverage) in CI. That gets its own story.
- Trust for the homelab root CA (`fpg-ca`). It is still open in `docs/open-questions/needs-decision.md`.
- Multi-arch images.
- Image signing, SBOMs or vulnerability scanning.

## Constraints

- GitHub Actions on GitHub-hosted runners (D13); registry is `ghcr.io` (D52).
- Pushing uses the automatically-provided workflow token only. No personal access token and no
  stored secret.
- One Cargo workspace. Both services depend on the shared `proto`, `identity` and `shared` crates.
  `protoc` is vendored, so the builder needs no system protobuf.
- The toolchain is selected by `rust-toolchain.toml` (currently the unpinned `stable` channel). The
  image build must honour that file, not a version baked into the builder image.
- The cluster is kubeadm 1.33 on containerd 2.0.5, with two nodes, both assumed amd64. There is no
  `imagePullSecret` anywhere in the cluster (D52).
- The Agent Core makes outbound HTTPS calls to Cerebras (D85), so the runtime image must carry the
  public CA bundle.

## Decisions

- **Public packages (to be recorded as D86).** The cluster pulls anonymously, which meets the
  "no manual login" criterion with no credential at all. Chosen because the repo is public, so the
  images expose nothing the source does not, provided no secret is ever baked into an image (a
  rule this plan adopts). Rejected: private packages plus a sealed `imagePullSecret`. That would
  make ARIA-123 depend on ARIA-169 (sealed-secrets), contradicting the sprint plan's "no
  dependency", and would need a long-lived PAT to keep valid, because the workflow token cannot be
  used from the cluster. Note: D51 called Docker Hub's "public-by-default visibility" wrong. This
  decision consciously departs from that leaning, and D86 must say so.
- **One workspace Dockerfile, a shared builder stage, one final target per service.** The workspace
  and its dependencies compile once per build. A dependency-caching layer means a source-only change
  does not rebuild the dependency tree, and a new service adds one small final stage. Rejected: a
  Dockerfile per service, which compiles shared crates twice per merge and creates near-copies (the
  duplication D17 rejected for charts). Also rejected: compiling on the runner and copying binaries
  in, because the binary would link against the runner's glibc, so a mismatch fails on the cluster
  instead of in CI. Cost accepted: the build context is the whole repo, so a `.dockerignore` is
  mandatory.
- **Runtime base: distroless `cc` (Debian 12), `nonroot` variant.** It provides glibc, public CA
  certs and a non-root user, with no shell and no package manager. ARIA-177's chart can set
  `runAsNonRoot` without fighting the image. Debugging uses `kubectl debug` ephemeral containers
  instead of `exec`. Rejected: `debian:bookworm-slim`, which would put a shell, `apt` and root in
  production for convenience. Also rejected: static musl on `scratch`, which needs hand-copied
  certs and users, and whose slow allocator under multi-threaded tonic load would force an
  allocator swap.
- **Pull requests build without pushing; `main` builds and pushes.** A broken build fails the PR
  check rather than breaking `main`, and a PR can never put an image into the registry. Rejected:
  building on `main` only, which finds breakage after merge and leaves ARIA-177 with no image for
  that SHA.
- **Tag with the full 40-character commit SHA only, no `latest`.** A moving tag conflicts with D45
  (identity is the digest) and with pinned tags in the umbrella chart's `values.yaml`. Rejected:
  short SHAs (collision-prone, and they don't match what Git shows by default in trailers and
  links) and `latest` (mutable).
- **No homelab CA in the image.** How pods trust `fpg-ca` is open, and "bake it into the base image"
  is one of the options listed there. This story must not answer it by implication. The image must
  leave the mounted-file option workable.

## Open questions

- Whether a package first pushed by the workflow under a personal account starts out private. If
  it does, its visibility is switched to public once, by hand, in the package settings. C4
  confirms this on the first push rather than assuming it.

## Risks

- **The unpinned `stable` toolchain** means that two builds of the same SHA on different days can
  differ. Signal: a merge with no code change fails in CI on a new lint or compile error. That is
  acceptable for now, but worth noting when it happens.
- **Cold builds on hosted runners may be slow** (the full tonic/prost dependency tree). Signal: PR
  checks take several minutes even for a doc-only change. Mitigation is the dependency layer cache.
  If that does not hold across runs, the cache wiring is the thing to fix.
- **Re-running a workflow re-pushes the same SHA tag with a different digest** (the builds are not
  reproducible). Harmless while nothing pins digests. It becomes relevant once D45's approval
  artifact records digests for MCP servers.
- **The binaries are stubs.** C4's pod may crash once started. The acceptance criterion is about
  the pull, so a successful image pull counts even if the container then exits.

## Components

### C1 — Record D86: public container packages

status: done
kind: chore

**Responsibility.** Put the public-packages decision into the append-only decision record before
any code depends on it, so the repo, Confluence and Jira agree.

**Contracts.** A new decision file after `0018`, holding D86 with its reasoning, the rejected
private-plus-pull-secret alternative, the explicit departure from D51's visibility leaning, and the
"no secret is ever baked into an image" rule it depends on. The decisions index lists it. The
Confluence decision-log mirror gains the entry. No past decision is edited. The ARIA-123 story text
is not changed, because closed decisions are not cited in stories.

**Tests.** Reviewed, not tested. Checks: the D86 entry exists with rejected alternatives, the index
links it, the Confluence mirror matches, and `git diff` touches no earlier decision file.

**Done when.** All three surfaces carry D86.

### C2 — Workspace image build

status: done
kind: chore
depends-on: 1

**Responsibility.** Turn the workspace into one runtime image per service from a single build
definition, with a build context that excludes everything not needed to compile.

**Contracts.** Each service is selectable as its own build target, and the two targets share one
compilation of the workspace. Dependencies are cached in a layer that a source-only change does not
invalidate. The toolchain comes from `rust-toolchain.toml`. The runtime image is distroless `cc`
`nonroot`. It contains the service binary and nothing built alongside it, and runs as a non-root
user by default. It carries an OCI source label linking it to the GitHub repo. It contains no
secret and no homelab CA. The build context excludes `target/` and other local-only files.

**Tests.** Checked by hand, locally:
- Building each target produces an image.
- Each image's configured user is non-root.
- Each image contains its own binary only.
- Rebuilding after a change to a service's source only reuses the dependency layer.
- The build context sent to the daemon is small: no `target/`.

**Done when.** Both images build locally and those checks hold.

### C3 — CI workflow

status: pending
kind: chore
depends-on: 2

**Responsibility.** Build both images on every pull request and on every push to `main`, and
publish them only from `main`.

**Contracts.** A pull request builds both images and pushes nothing. A push to `main` builds both and
pushes each under the full commit SHA, with no other tag. Image names are `aria-gateway` and
`aria-agent-core` under the repo owner's `ghcr.io` namespace. Authentication uses the workflow
token alone. Package-write permission is granted only where pushing happens; everything else is
read-only. The build cache persists between runs. Two quick successive merges each publish their
own SHA; neither is lost or overwritten.

**Tests.** Verified on the branch's own PR and after merge:
- The PR check runs, builds both images and pushes nothing, so no new package version appears.
- After merge, both images exist in `ghcr.io` under the merge commit's SHA.
- A second PR run shows the dependency layer coming from cache.

**Done when.** The PR check is green and, after merge, both SHA-tagged images exist.

### C4 — Cluster pull verification

status: pending
kind: chore
depends-on: 3

**Responsibility.** Prove the second acceptance criterion against the real cluster and leave
nothing behind.

**Contracts.** Both packages are public. If the first push created them as private, they are
switched to public once, and that manual step is recorded in D86's consequences. A throwaway pod
per image, with no `imagePullSecret` and on a node with no registry credential, pulls the image
successfully. The pods are deleted afterwards. The evidence is recorded on ARIA-123 as a comment
naming the SHA and the pull events.

**Tests.**
- Each pod's events show a successful pull and never an authentication failure or
  `ImagePullBackOff`. A later crash of a stub binary does not count against this.
- An anonymous pull of each image from outside the cluster also succeeds.

**Done when.** Both pulls are evidenced on ARIA-123 and the pods are gone.

## Deviations

# syntax=docker/dockerfile:1
#
# One build definition for every ARIA core service image (ARIA-123, plan
# docs/plans/2026-10-05-aria-123-ghcr-images.md).
#
#   docker build --target gateway    -t aria-gateway .
#   docker build --target agent-core -t aria-agent-core .
#
# Pass the commit so the service reports it (an empty value means "unknown"):
#
#   docker build --target agent-core --build-arg ARIA_GIT_SHA=$(git rev-parse HEAD) -t aria-agent-core .
#
# Both targets share the builder stage, so the workspace compiles once.
# Dependencies are cooked into their own layer (cargo-chef), so a source-only
# change does not rebuild the dependency tree.
#
# D86: no secret and no homelab CA is ever baked into an image. Credentials and
# fpg-ca trust reach a pod at runtime.

ARG RUST_IMAGE=rust:1-slim-bookworm
ARG RUNTIME_IMAGE=gcr.io/distroless/cc-debian12:nonroot
ARG CARGO_CHEF_VERSION=0.1.78

# --- chef: toolchain from rust-toolchain.toml, plus cargo-chef -------------
FROM ${RUST_IMAGE} AS chef
ARG CARGO_CHEF_VERSION
WORKDIR /app
COPY rust-toolchain.toml ./
# With no arguments, rustup installs whatever rust-toolchain.toml names, so the
# toolchain follows the repo rather than the builder image's own default.
RUN rustup toolchain install \
    && cargo install cargo-chef --locked --version "${CARGO_CHEF_VERSION}"

# --- planner: reduce the workspace to a dependency recipe -------------------
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# --- builder: cook dependencies, then build the service binaries -----------
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json
COPY . .
# Declared after the cook step and the source copy, so the dependency layer cache never sees it.
# A build argument is visible to the RUN below as an environment variable, which the services read
# at compile time.
ARG ARIA_GIT_SHA=
RUN cargo build --release --locked --package gateway --package agent-core

# --- runtime targets ---------------------------------------------------------
# distroless/cc: glibc (matches the Debian 12 builder), public CA certs, a
# non-root user, no shell. Debug with `kubectl debug`, not `kubectl exec`.

FROM ${RUNTIME_IMAGE} AS gateway
LABEL org.opencontainers.image.source="https://github.com/FPGSchiba/aria" \
      org.opencontainers.image.title="aria-gateway" \
      org.opencontainers.image.description="ARIA Gateway — client front doors and end-user context minting" \
      org.opencontainers.image.licenses="MIT"
COPY --from=builder /app/target/release/gateway /usr/local/bin/gateway
USER nonroot:nonroot
ENTRYPOINT ["/usr/local/bin/gateway"]

FROM ${RUNTIME_IMAGE} AS agent-core
LABEL org.opencontainers.image.source="https://github.com/FPGSchiba/aria" \
      org.opencontainers.image.title="aria-agent-core" \
      org.opencontainers.image.description="ARIA Agent Core — conversation sessions and LLM-driven decisions" \
      org.opencontainers.image.licenses="MIT"
COPY --from=builder /app/target/release/agent-core /usr/local/bin/agent-core
USER nonroot:nonroot
ENTRYPOINT ["/usr/local/bin/agent-core"]

# syntax=docker/dockerfile:1
#
# Multi-stage build: compile `rootcause` in a full Rust toolchain image,
# then ship only the resulting binary (plus the bundled pattern library
# and license) in a minimal runtime image. Root Cause has no system
# library dependencies (no OpenSSL, no network I/O — see README.md's
# "How it works" summary), so the runtime stage needs nothing beyond
# glibc, which the base image already provides.
#
# Build:
#   docker build -t rootcause .
#
# Run against a trace on the host, using the image's bundled pattern
# library:
#   docker run --rm -v "$(pwd):/data" rootcause analyze /data/trace.json --patterns /patterns
#
# See README.md's "Docker" section for more examples, including how to
# mount your own pattern library instead of the bundled one.

FROM rust:1.75-slim-bookworm AS builder
WORKDIR /build

# Copy the whole workspace. Root Cause is small (a few MB of source, no
# vendored assets) and every crate listed in the root Cargo.toml's
# `[workspace] members` must be present for Cargo to resolve the
# workspace at all, so there is no meaningful subset to copy instead —
# see `.dockerignore` for the (build-irrelevant) paths that *are*
# excluded from this copy.
COPY . .

# `--locked` refuses to build if Cargo.lock is out of date with the
# manifests, so a stale lockfile fails the image build loudly instead of
# silently resolving different dependency versions than CI tested.
RUN cargo build --release --locked -p cli

FROM debian:bookworm-slim AS runtime

# Run as an unprivileged user rather than the image's default root,
# per standard container hardening practice — this binary needs no
# elevated privileges (it only reads files it's given and writes to
# stdout/a specified output path).
RUN useradd --system --create-home --uid 10001 rootcause

COPY --from=builder /build/target/release/rootcause /usr/local/bin/rootcause
COPY --from=builder /build/LICENSE /LICENSE
COPY --from=builder /build/patterns /patterns

USER rootcause
WORKDIR /data
ENTRYPOINT ["rootcause"]
CMD ["--help"]

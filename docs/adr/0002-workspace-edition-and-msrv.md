# 0002. Workspace edition and MSRV

Status: Accepted

## Context
The canonical decision "Rust is final" (Architecture Review Gate) fixes the
language but not the edition or minimum supported Rust version (MSRV).
Neither the Architecture nor Engineering Specification document specifies
either. This choice does not alter externally visible behavior.

## Decision
- Edition: 2021 (not 2024), declared once via `[workspace.package] edition`.
- MSRV: 1.75.0, declared via `rust-version` in `[workspace.package]` and
  pinned exactly via `rust-toolchain.toml` so CI and local development use
  an identical compiler.

Edition 2021 rather than 2024 is chosen because, at the time of this
decision, edition 2024's toolchain requirement is newer than what is
reliably available in the project's execution environments (verified
directly: the environment's package manager offers 1.75.0 as the newest
readily installable rustc). Pinning to what is actually reproducible now
is preferred over aspirationally targeting a newer edition. This can be
revisited via a superseding ADR once the target deployment/CI environment
is confirmed to support a newer toolchain.

## Consequences
`workspace.lints` (stable since 1.74) and the 2021 edition's module/path
resolution rules are both available at MSRV 1.75.0. Some newer
clippy/rustc lints introduced after 1.75 will not be available until the
MSRV is bumped; this is an accepted tradeoff for reproducibility over
lint currency, revisitable at any time via a new ADR that also updates
`rust-toolchain.toml` and CI in the same change.

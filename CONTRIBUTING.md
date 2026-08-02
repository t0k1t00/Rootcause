# Contributing to Root Cause

Thanks for your interest in Root Cause. This document covers the mechanics
of contributing: environment setup, the validation a change must pass, and
what to expect from review. For the project's own architectural rationale
and standing invariants, start with `README.md`, `docs/spec/`, and
`docs/adr/` — read those before proposing anything that changes engine
behavior, since most such decisions have already been made deliberately and
recorded there.

## Setting up

```sh
git clone https://github.com/rootcause-project/rootcause
cd rootcause
rustup show   # installs the pinned toolchain from rust-toolchain.toml
cargo build --workspace
cargo test --workspace
```

No other tooling, services, or network access is required to build, test,
or run the engine.

## Before opening a PR

Run the same checks CI runs, in this order:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo test --workspace
cargo doc --workspace --no-deps
```

All five must pass locally. `cargo fmt --all` (without `--check`) will fix
formatting for you; clippy and doc warnings need to be fixed by hand.

If your change touches dependencies, also run `cargo deny check` (see
`deny.toml`) — CI runs this via `EmbarkStudios/cargo-deny-action`, but it's
worth checking locally first since a license or advisory violation will
block merge regardless of what else passes.

## What kind of contribution is this?

- **A new exploit-detection pattern** (a `.rcdsl` file plus demo traces): see
  `docs/PATTERN_AUTHORING_GUIDE.md` for a complete walkthrough. This is the
  contribution type most welcome from new contributors, and does not
  require touching any Rust code. Use `rootcause new-pattern`,
  `rootcause validate-pattern`, `rootcause format-pattern`, and
  `rootcause doctor` (the "pattern SDK") throughout — see the guide's
  step 0 and step 9, and `crates/cli/README.md` for the full flag
  reference. In short:

  ```sh
  rootcause new-pattern my_new_pattern --family MyFamilyName
  # ... edit the scaffolded .rcdsl and demo/*.json files ...
  rootcause validate-pattern patterns/my_new_pattern.rcdsl
  rootcause format-pattern patterns/my_new_pattern.rcdsl --write
  rootcause doctor
  ```
- **A new DSL predicate, attribute, or grounding capability**: read
  `crates/dsl/README.md`, `crates/matcher/README.md`, and
  `crates/grounding/README.md` first, and expect to explain (in your PR
  description, mirroring the standard this repo already holds itself to)
  exactly which existing `fact-model` fact your addition grounds against —
  additions that would require guessing at unrecorded facts (intent,
  authorization, off-chain state) are consistently declined; see the
  "Exploit families evaluated but not added" section of `README.md` for
  worked examples of where that line has already been drawn.
- **A bug fix**: please include a regression test that fails before your
  fix and passes after it.
- **Documentation, tooling, or CI**: no special process — normal PR.

## Code style

- Formatting and most style rules are enforced by `cargo fmt` and
  `cargo clippy` (workspace lint configuration lives in the root
  `Cargo.toml`'s `[workspace.lints]`) — if it passes both, it's
  stylistically acceptable.
- Match the surrounding module's convention for doc comments; most modules
  in this codebase explain *why*, not just *what*, especially for any
  design decision that might look arbitrary out of context. New code
  touching a documented invariant should extend that documentation, not
  just the code.
- Keep changes scoped. A PR that fixes a typo and also refactors an
  unrelated module is harder to review and more likely to be asked to
  split.

## Reporting a security vulnerability

Please see `SECURITY.md` — do not open a public issue for a vulnerability
in Root Cause itself.

## License

By contributing, you agree that your contribution is licensed under the
Apache License 2.0, the same license covering the rest of this repository
(see `LICENSE` and `docs/adr/0001-license-choice.md`).

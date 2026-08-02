## What does this change?

<!-- One or two sentences. -->

## Checklist

- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo build --workspace` passes
- [ ] `cargo test --workspace` passes
- [ ] Documentation updated (`README.md`, relevant crate `README.md`, or
      `docs/`) if this change affects documented behavior
- [ ] `CHANGELOG.md` updated under `[Unreleased]`, if user-visible

### If this adds or changes a detection pattern

- [ ] Positive and negative demo traces added under `demo/`
- [ ] Integration tests added in `integration-tests/tests/exploit_corpus.rs`
- [ ] Taxonomy mapping added in `crates/taxonomy/src/scwe.rs` (and
      `swc.rs`, only if a genuine unforced fit exists) with a test
- [ ] The pattern's doc comment includes a "Scope note" explaining what
      it cannot determine (see `docs/PATTERN_AUTHORING_GUIDE.md`)

## Related issue

<!-- Closes #... , or "N/A" -->

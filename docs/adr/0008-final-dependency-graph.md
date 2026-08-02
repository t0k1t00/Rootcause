# 0008. Final crate dependency graph and error-handling reconciliation

Status: Accepted

## Context
ADR-0005 recorded the *initial, structural* dependency graph wired up
during Task 1, before any crate had real logic or a designed public
API — its own Status line says as much ("subject to revision once each
crate's public API is actually designed") and its Consequences section
requires exactly this: a superseding ADR plus the matching `Cargo.toml`
edit, recorded together, once real implementation work shows the
initial graph was wrong.

That has now happened, in three places, across the crates that came
after ADR-0005:

1. **`taxonomy`** was recorded as having "no workspace dependencies."
   Its actual implementation depends on `dsl` (pattern families are a
   `dsl::ir` type) and `grounding` (taxonomy mappings are keyed to
   ground truth grounding produces, and its own module docs describe
   following `grounding::verifier`'s registry-dispatch precedent).
2. **`grounding`** was recorded as depending only on `fact-model,
   matcher`. Its actual implementation also depends on `dsl` directly
   (compiled pattern/evidence types it grounds against).
3. **`cli`** was recorded as depending on `fact-model, ingestion, dsl,
   matcher, grounding, taxonomy` — but not `benchmark-harness`. Task 9
   (the CLI) added the `rootcause benchmark` subcommand, which
   necessarily depends on `benchmark-harness` to run suites and export
   `BenchmarkReport`s; there is no way to implement that command
   without this edge.

Separately, ADR-0003 ("Per-crate error handling strategy") states that
every listed library crate, `taxonomy` included, "defines its own
`Error` enum in `src/error.rs`." `taxonomy` does not have one: every
operation `taxonomy` exposes (mapping a `PatternFamily` to zero or more
taxonomy entries) is total — there is no input that causes it to fail,
only inputs that produce an empty mapping, which is a valid, reported
result (see `taxonomy::TaxonomyReport`), not an error. ADR-0003 did not
anticipate a crate with no fallible operations at all, so its blanket
"every crate has an `Error` enum" statement is inaccurate for this one
case.

## Decision
The dependency graph, as actually built and enforced by `cargo` today,
is:

```
fact-model        (no workspace dependencies — the leaf fact model)
ingestion         → fact-model
dsl               (no workspace dependencies — unchanged from ADR-0005)
matcher           → fact-model, dsl
grounding         → fact-model, dsl, matcher
taxonomy          → dsl, grounding
benchmark-harness → fact-model, ingestion, dsl, matcher, grounding, taxonomy
cli               → fact-model, ingestion, dsl, matcher, grounding,
                     taxonomy, benchmark-harness
integration-tests → (dev-dependency on) fact-model, ingestion, dsl,
                     matcher, grounding, taxonomy, benchmark-harness
                     — not `cli`, which is a `[[bin]]`-only crate with
                     no `[lib]` target and so cannot be a normal Cargo
                     dependency; see `integration-tests/Cargo.toml`'s
                     own note and `crates/cli/Cargo.toml`.
```

This remains acyclic (a prerequisite ADR-0005 already established as
enforced by `cargo` itself, not just documentation), and every edge
above is a real `path` dependency in the corresponding `Cargo.toml`
today.

This ADR supersedes ADR-0005's specific graph (the diagram in this
document is now canonical); it does not revise ADR-0005's reasoning for
*why* a documented graph is required, which still holds.

It also amends ADR-0003: `taxonomy` is an intentional exception to
"every library crate defines its own `Error` enum" — it has no
`src/error.rs` because it has no fallible operation to report. Every
other crate ADR-0003 lists (`fact-model`, `ingestion`, `dsl`, `matcher`,
`grounding`, `benchmark-harness`) still follows the original rule
unchanged.

## Consequences
`docs/adr/0005-crate-dependency-graph.md`'s Status line is updated to
point here; its own Decision/Context/Consequences text is left as-is,
per this project's stated ADR convention of marking supersession rather
than rewriting history. Any future crate-graph or error-handling-policy
change again requires its own superseding ADR plus the matching
`Cargo.toml`/`src/error.rs` edit in the same change, per ADR-0005's own
Consequences section, which this ADR does not relax.

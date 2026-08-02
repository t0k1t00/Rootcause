# 0003. Per-crate error handling strategy

Status: Accepted, amended by [0008](0008-final-dependency-graph.md) —
`taxonomy` is exempt from the "every library crate defines its own
`Error` enum" rule below, since it has no fallible operation to report.
Every other crate this ADR lists is unaffected.

## Context
The project rule "prefer explicit error types" and "no unwrap() in library
code" is stated in the outer engineering constraints, but neither the
Architecture nor Engineering Specification document specifies a concrete
error-handling mechanism (whether to use `anyhow`, hand-rolled enums, or
`thiserror`). This is an implementation detail with no externally visible
behavior implication as long as each crate's public error type is
exhaustive and documented, so it is delegated.

## Decision
Every library crate (`fact-model`, `ingestion`, `dsl`, `matcher`,
`grounding`, `taxonomy`, `benchmark-harness`) defines its own `Error` enum
in `src/error.rs`, derived via `thiserror::Error`, with `#[non_exhaustive]`
omitted deliberately — enums are kept exhaustive per the "prefer exhaustive
enums" project rule, meaning adding a new error variant is a breaking
change requiring a major version bump, not a silent extension. `thiserror`
is a workspace dependency (`crates.io`, widely used, zero runtime
dependencies beyond `std` + proc-macro).

The `cli` binary crate, which is the only crate allowed to be "terminal" in
the sense of ending the process, may use `anyhow`-style error aggregation
at its outermost boundary if this proves useful later — that choice is
deferred to Task 9 (cli crate) and is not part of this ADR.

## Consequences
Cross-crate error propagation will require explicit `From` impls or
`#[from]` thiserror attributes at each crate boundary, which is intentional:
it keeps each crate's error surface explicit and prevents error types from
leaking implementation details of one crate into another's public API,
directly supporting the "clear module boundaries" project rule.

# 0005. Initial crate dependency graph

Status: Superseded by [0008](0008-final-dependency-graph.md) — the graph
below reflected Task 1's initial, pre-implementation wiring, as its own
"structural only" qualifier below states. See ADR-0008 for the final,
as-built graph.

## Context
Neither canonical document specifies a crate dependency graph. The
Engineering Specification states this outright as unaddressed. A graph is
nonetheless required for the workspace to reflect the actual data-flow
implied by each crate's stated role in `01_ARCHITECTURE.md`'s repository
structure section, and for `cargo build -p <crate>` to mean anything
correctly at each layer as real code is added.

## Decision
Directed edges (A → B means "A depends on B"):

```
ingestion         → fact-model
dsl               (no workspace dependencies — pattern language is
                    self-contained; it does not need to know about the
                    fact model to define pattern *syntax*, only to be
                    matched against it later by `matcher`)
matcher           → fact-model, dsl
grounding         → fact-model, matcher
taxonomy          (no workspace dependencies — a standalone vocabulary
                    crate; nothing else needs to know about it to compile,
                    though `benchmark-harness` and `cli` will use it)
benchmark-harness → fact-model, ingestion, matcher, grounding, taxonomy
cli               → fact-model, ingestion, dsl, matcher, grounding, taxonomy
integration-tests → (dev-dependency on) fact-model, ingestion, dsl,
                    matcher, grounding, taxonomy, cli
```

This is wired as real `path` dependencies in each crate's `Cargo.toml` now,
even though no crate yet calls into another (Task 1 forbids business
logic). This is deliberate: it makes the dependency graph something
`cargo` itself enforces (a cyclic edge would fail to compile) rather than
documentation that can silently drift from reality.

## Consequences
Any crate implementation task that discovers this graph is wrong (e.g.
`dsl` turns out to need `taxonomy` to validate pattern categories at parse
time) requires a superseding ADR plus a `Cargo.toml` edit in the same
change — the graph is not to be altered silently mid-task.

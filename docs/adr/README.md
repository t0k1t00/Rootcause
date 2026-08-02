# Architecture Decision Records

This directory records engineering decisions made under the delegated
authority described in the project's canonical specification set
(`docs/spec/`): decisions made only where (a) neither the Architecture nor
Engineering Specification documents specify an implementation, (b) the
decision does not alter externally visible behavior, and (c) the decision
preserves all stated architectural invariants (determinism, full-grounding-
or-explicit-failure, precision-over-recall, abstention as a first-class
result).

Each ADR is a single Markdown file named `NNNN-short-title.md`, numbered
sequentially, and never renumbered or deleted after merge — a superseded
ADR is marked "Superseded by NNNN," not removed, so the history of *why*
remains legible.

## Template

```markdown
# NNNN. Title

Status: Proposed | Accepted | Superseded by NNNN

## Context
What problem or gap is being decided, and why is this an ADR-eligible
decision (i.e., why doesn't the Architecture or Engineering Spec already
answer it)?

## Decision
The decision, stated plainly.

## Consequences
What this makes easier or harder later; what it does not decide.
```

## Index

- [0001 — License choice](0001-license-choice.md)
- [0002 — Workspace edition and MSRV](0002-workspace-edition-and-msrv.md)
- [0003 — Per-crate error handling strategy](0003-error-handling-strategy.md)
- [0004 — CI toolchain and coverage strategy](0004-ci-toolchain-and-coverage.md)
- [0005 — Initial crate dependency graph](0005-crate-dependency-graph.md) (superseded by 0008)
- [0006 — `Wei` integer width](0006-wei-integer-width.md)
- [0007 — Ingestion source abstraction and raw wire schema](0007-ingestion-source-and-schema.md)
- [0008 — Final crate dependency graph and error-handling reconciliation](0008-final-dependency-graph.md)

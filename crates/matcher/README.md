# matcher

Deterministic structural pattern matching: finds every candidate way a
`dsl::CompiledPattern` matches a `fact_model::Trace`.

This crate does **not** determine whether a match is valid, perform
grounding, or classify exploits — it only discovers structural
candidates. Independent evidence verification is `grounding`'s job.

## Overall matching algorithm

For one pattern against one trace:

1. **Predicate evaluation** (`predicate`) — evaluate every evidence
   clause's predicate independently, once, producing the full set of
   facts that satisfy it (a `Binding` per fact, carrying an approximate
   execution-order key and any attribute the predicate named that this
   crate could not structurally check).
2. **Anchor selection** (`engine`) — pick one evidence clause as the
   pattern's "trigger": a `sequence:` constraint's first step if the
   pattern has one, otherwise the first positively-referenced
   (non-`NOT`-negated) clause in the constraint tree.
3. **Candidate generation** (`engine`) — for each fact that satisfies the
   anchor clause, attempt to extend it to a full match: resolve the
   `sequence:` constraint (if any) starting from that specific anchor
   occurrence (`sequence`), then evaluate the boolean `constraint:` tree
   (`constraint`). If both succeed, bind every positively-referenced,
   satisfied clause to a concrete fact and emit one `CandidateMatch`.

This produces exactly one candidate per real occurrence of the pattern's
trigger condition that can be extended to a full match — neither
collapsing multiple independent occurrences into one match, nor
exploding combinatorially over every alternative fact.

## Module map

| Module | Contents |
|---|---|
| `binding` | `Binding`, `UnresolvedAttribute`. |
| `constraint` | Boolean constraint-tree evaluation. |
| `engine` | Anchor selection and candidate generation. |
| `error` | `MatcherError`. |
| `index` | `TraceIndex`, precomputed per-kind fact lookups. |
| `ordering` | Approximate execution-order keys used by `sequence`. |
| `predicate` | Per-predicate-kind evaluation. |
| `sequence` | `sequence:` constraint resolution. |

## Usage

```rust
use matcher::MatchEngine;

let engine = MatchEngine::new(&trace);
let candidates = engine.find_matches(&compiled_pattern)?;
```

Prefer `MatchEngine` over the one-shot `find_candidate_matches` when
matching more than one pattern against the same trace: it builds one
`TraceIndex` and reuses it, instead of recomputing shared structure once
per pattern.

## Status

Complete. See crate-level rustdoc (`cargo doc -p matcher --open`) for the
full API reference and complexity analysis.

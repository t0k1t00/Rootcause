# taxonomy

Maps a `grounding::GroundingResult` into external vulnerability
taxonomies — SCWE (primary) and SWC (secondary/compatibility) to start —
without ever feeding back into matching or grounding.

## Design

- **Multiple taxonomies, independently.** Each taxonomy is a separate
  `TaxonomyMapper` implementation (`scwe::ScweMapper`, `swc::SwcMapper`),
  looked up by name through a `TaxonomyRegistry` — the same
  "trait + kind-keyed registry" shape `grounding::verifier` already
  uses, so one taxonomy's mapping table can be added, changed, or
  removed without touching any other's.
- **One-to-many mappings, preserved.** `TaxonomyMapper::map_family`
  returns `Vec<TaxonomyEntry>`; a pattern family that plausibly
  corresponds to several entries in the same taxonomy keeps all of
  them, not just a "best" one.
- **Unmapped results, preserved.** A pattern family with no applicable
  entry in some (or every) taxonomy still produces a full
  `TaxonomyReport` — the taxonomy is listed in
  `TaxonomyReport::unmapped_taxonomies` rather than the whole result
  being dropped or defaulted to a guess (a real example: oracle
  manipulation has no SWC entry).
- **Confidence-preserving.** This crate invents no probability or score
  of its own. `MappingEngine::map` copies the source
  `GroundingResult`'s own `GroundingStatus` into
  `TaxonomyReport::grounding_status` unchanged — taxonomy mapping
  answers "what is this called externally," grounding status answers
  "how sure are we," and the two are kept visibly separate.
- **Never influences matching or grounding.** This crate depends on
  `dsl` (for `PatternFamily`) and `grounding` (for `GroundingResult`);
  neither `matcher` nor `grounding` depends on `taxonomy` — see
  ADR-0008 for the as-built dependency graph this relies on.

`TaxonomyMapper` is keyed on `PatternFamily`, not the full
`GroundingResult`: a mapper implementation physically cannot read,
branch on, or feed back into a `GroundingStatus` or `EvidenceOutcome`,
because none of that is in scope of the function it implements.

## Module map

| Module | Contents |
|---|---|
| `mapper` | The `TaxonomyMapper` trait. |
| `entry` | `TaxonomyEntry`. |
| `registry` | `TaxonomyRegistry`. |
| `scwe` | The built-in SCWE mapper. |
| `swc` | The built-in SWC mapper. |
| `engine` | `MappingEngine`, orchestrating every registered mapper. |
| `report` | `TaxonomyReport`. |

## Usage

```rust
use taxonomy::MappingEngine;

let engine = MappingEngine::default();
let report = engine.map(&grounding_result);
```

## Note on error handling

Unlike every other library crate in this workspace, `taxonomy` has no
`src/error.rs`: every operation it exposes is total (a family with no
mapping produces an empty, valid result, not an error). See
[ADR-0008](../../docs/adr/0008-final-dependency-graph.md) for why this
is a deliberate, documented exception to
[ADR-0003](../../docs/adr/0003-error-handling-strategy.md)'s general
rule.

## Status

Complete. See crate-level rustdoc (`cargo doc -p taxonomy --open`) for
the full API reference.

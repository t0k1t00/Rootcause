# grounding

Independent evidence verification: the core scientific contribution of
Root Cause. Given a `matcher::CandidateMatch` (a *structural* claim),
this crate independently re-derives, clause by clause, whether the trace
actually supports it, and produces a `GroundingResult` with an explicit
`GroundingStatus` — never a bare `bool`.

## Overall grounding algorithm

For one candidate against its trace and pattern:

1. **Input coherence check** (`engine`) — confirm the candidate was
   actually matched against the given pattern (same id/version), and
   that every fact it cites resolves in the trace. Failure here is a
   caller error (`GroundingError`), not an evidence-quality question.
2. **Polarity analysis** (`polarity`) — walk the pattern's compiled
   constraint tree once to determine, for every evidence clause, whether
   the pattern requires it present or absent.
3. **Evidence-by-evidence verification** (`verifier`, `verifiers`) — for
   every evidence clause the pattern declares, independently recompute
   which facts in the trace satisfy its predicate, never trusting the
   candidate's own binding at face value, producing one `EvidenceOutcome`
   each.
4. **Sequence re-confirmation** — if the pattern declares a `sequence:`
   constraint, independently re-check that the facts grounding itself
   verified actually occur in the required relative order.
5. **Abstention-aware aggregation** — combine every required clause's
   outcome (and the sequence check) into one `GroundingStatus`: any
   uncertainty anywhere outranks a confident contradiction.
6. **Reporting** (`report`) — assemble the complete `GroundingResult`:
   status, categorized evidence lists, non-probabilistic confidence
   metadata, the full evidence chain, and (if abstaining) every
   `AbstainReason`.

## Verifier architecture

Every predicate kind is verified through the `verifier::Verifier` trait,
dispatched by a `verifier::VerifierRegistry` keyed on predicate-kind
name — never a hardcoded `match` in the engine. A predicate kind with no
registered verifier degrades gracefully to a per-evidence
`EvidenceOutcome::Unsupported` rather than a hard error.

## Abstention policy

Abstention is the honest default whenever independent verification
cannot reach full confidence. This crate never upgrades uncertainty into
`GroundingStatus::Grounded`: aggregation checks for uncertainty *before*
it checks for confident contradiction, so there is no code path that can
produce `Grounded` while any required clause is anything other than
`EvidenceOutcome::Verified`.

## Module map

| Module | Contents |
|---|---|
| `error` | `GroundingError`, this crate's single exhaustive error type. |
| `status` | `GroundingStatus`, `EvidenceOutcome`, `AbstentionCategory`. |
| `polarity` | Constraint polarity analysis. |
| `verifier` | The `Verifier` trait, `VerifierOutcome`, the extensible `VerifierRegistry`. |
| `verifiers` | The built-in `Verifier` implementations, one per predicate kind. |
| `report` | `GroundingResult` and everything it's built from. |
| `engine` | `GroundingEngine`, orchestrating every phase above. |

## Usage

```rust
use grounding::{ground_all, GroundingEngine};

let engine = GroundingEngine::new();
let result = engine.ground(&candidate, &trace, &pattern)?;

// Or, batched across every candidate from one trace:
let results = ground_all(&candidates, &trace, &patterns)?;
```

## Status

Complete. See crate-level rustdoc (`cargo doc -p grounding --open`) for
the full algorithm description, complexity analysis, and the abstention
trigger table.

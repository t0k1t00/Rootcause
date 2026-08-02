# Root Cause — Engineering Specification

**Scope note:** the Scientific Review ("Scientific Review: Does the Core Hypothesis Hold?") is a hypothesis-validation document, not an engineering specification. It contains almost no information at the level this document is meant to operate at (workspace layout, APIs, algorithms, CI). This document is therefore, by necessity, mostly a record of **what is genuinely unspecified**, plus the small set of concrete engineering constraints the Review does impose. Where nothing is said, nothing is invented.

## Rust workspace layout

**Not specified in the Scientific Review.** The Review names one implementation-adjacent detail only: Phase 10 suggests the two-week prototype script may be written in "Rust or Python." No workspace, crate names, or module boundaries appear anywhere in the source document.

**Assumption E-1:** since a later production system is clearly implied (the Review speaks of "the engineering effort," "five more months," Phase 10), Rust is the smallest reasonable choice consistent with the Review's own suggestion, over Python, if a single language must be picked now for planning purposes — but this is a planning convenience, not a Review-derived requirement, and should be revisited when a real workspace is designed.

No crate layout is proposed in this document; doing so would invent structure the canonical artifact does not support.

## Crate dependency graph

**Not specified.** No subsystem-to-subsystem dependency relationship is discussed in the Review at an engineering level. Left undefined.

## Public APIs

**Not specified.** The Review discusses inputs and outputs at a conceptual level only (a transaction trace in; a classification or "explicitly fails" out — Phase 10), never as a function signature, RPC, or CLI shape. **Assumption E-2:** the minimal API surface implied by Phase 10's prototype requirement is: something that accepts one archive-node-sourced trace (plus resolved storage diff) and produces, per case, one of (a) a full grounding record listing which fact backs each clause, or (b) a specific unmet clause plus a reason. No further API detail is invented.

## Data structures

Per `01_ARCHITECTURE.md`'s Fact Model section (Assumption A-4), the Review's own text implies, without defining, at least: `Transaction`, `Call`, `StorageChange`. **Assumption E-3:** a `GroundingResult` type (or equivalent) capable of recording, per pattern clause, either "grounded against fact X" or "ungrounded, reason Y" is the minimal structure needed to satisfy Phase 10's metric-collection requirement ("a precise written note on why" for any non-grounding case). Field names, types, and encodings are not specified by the Review and are not invented here.

## Traits

**Not specified.** No abstraction boundaries, interfaces, or extensibility points are discussed in the source document.

## Algorithms

**Not specified**, with one partial, important exception: the Review's Phase 4 analysis of DSL expressiveness constrains what any matching algorithm must be able to do — it must be able to represent "sequence + temporal + value-flow constraints" (cleanly, for oracle-manipulation and donation/inflation) and "call-depth + storage-write-after-external-call ordering" (for classic reentrancy). This is a **requirement on algorithm capability**, not an algorithm design. No specific matching strategy (graph matching, constraint solving, rule engine) is named or implied by the Review, and none is chosen here.

## Complexity

**Not addressed anywhere in the Scientific Review.** No performance target, complexity bound, or scale assumption (trace size, pattern count) appears in the source document.

## Error handling

The Review's Phase 10 is the sole source of a concrete error-handling requirement: when a clause cannot be grounded, the system must produce "a precise written note on *why* (missing fact in the archive-node trace, storage layout unresolvable, genuine structural mismatch between the incident and the intended pattern)." **Assumption E-4:** this implies at minimum three distinct, named failure categories (missing-fact, unresolvable-layout, structural-mismatch) rather than a single generic failure — this is the smallest reasonable structure satisfying the Review's own wording, marked as an assumption since the Review does not name these as formal error types.

## Testing strategy

The Review's Phase 8 and Phase 10 collectively constitute the only testing strategy the canonical artifact specifies, and it is explicitly a **real-data validation strategy, not a unit-test strategy**:

- Phase 8: one hand-coded predicate, tested against one real, well-documented incident, pulled from a real archive-node trace — success is defined as full groundability with no hand-waving.
- Phase 10: the same predicate concept, tested against exactly 5 real incidents (not synthetic ones), with per-case pass/fail/partial recording and a written failure-cause classification.

**Assumption E-5:** conventional unit/property-based testing of code (as opposed to real-incident validation) is not discussed in the Review at all. It is a reasonable engineering practice to add once implementation begins, but this document does not assert it as something the Review specifies, since it is not.

## Property tests

**Not specified in the Scientific Review.** No mention of property-based testing, invariants, or generative testing appears anywhere in the source document.

## CI

**Not specified.** No continuous-integration process, gating, or automation is discussed.

## Coding standards

**Not specified.** The Review is a scientific/hypothesis document; it contains no coding-style, review-process, or contribution-workflow content.

---

**Summary for the reader:** this document is intentionally sparse. Of its eleven required sections, only two (Error handling, Testing strategy) have direct, substantive source material in the Scientific Review, and even those are scoped narrowly to the two-week validation prototype, not a production system. Treat this document as an honest record of what remains to be designed, not as a specification to build against as-is. A follow-on engineering design pass, informed by whatever architecture decisions get made in response to `01_ARCHITECTURE.md`, will be needed before implementation of the DSL, compiler, matcher, or CI can begin in earnest.

//! # matcher
//!
//! Deterministic structural pattern matching: finds every candidate way
//! a [`dsl::CompiledPattern`] matches a [`fact_model::Trace`].
//!
//! This crate does **not** determine whether a match is valid, perform
//! grounding, or classify exploits — see "Why grounding is intentionally
//! excluded" below. It only discovers candidates.
//!
//! ## Overall matching algorithm
//!
//! For one pattern against one trace:
//!
//! 1. **Predicate evaluation** (module [`predicate`]): evaluate every
//!    evidence clause's predicate independently, once, producing the
//!    full set of facts that satisfy it (a [`predicate::Binding`] per
//!    fact, carrying an approximate execution-order key and any
//!    attribute the predicate named that this crate could not
//!    structurally check).
//! 2. **Anchor selection** (module [`engine`]): pick one evidence clause
//!    as the pattern's "trigger" — a `sequence:` constraint's first step
//!    if the pattern has one, otherwise the first positively-referenced
//!    (non-`NOT`-negated) clause in the constraint tree.
//! 3. **Candidate generation** (module [`engine`]): for *each* fact that
//!    satisfies the anchor clause, attempt to extend it to a full match:
//!    resolve the `sequence:` constraint (if any) starting from that
//!    specific anchor occurrence (module [`sequence`]), then evaluate
//!    the boolean `constraint:` tree (module [`constraint`]) using
//!    "does this clause have a matching fact" as each leaf's truth
//!    value. If both succeed, bind every positively-referenced,
//!    satisfied clause to a concrete fact and emit one
//!    [`CandidateMatch`].
//!
//! This produces exactly one candidate per real occurrence of the
//! pattern's trigger condition that can be extended to a full match —
//! neither collapsing multiple independent occurrences (e.g. two
//! separate reentrant calls in one trace) into one match, nor exploding
//! combinatorially over every `OR` branch and every non-anchor clause's
//! every alternative fact (which would produce many matches describing
//! the same underlying occurrence).
//!
//! ## Matching phases (module map)
//!
//! - [`index`] — [`index::TraceIndex`], precomputed structures built
//!   once per trace and reused across every pattern matched against it.
//! - [`predicate`] — predicate evaluation, one function per DSL
//!   predicate kind (`call`, `storage`, `value_flow`, `token_transfer`,
//!   `transaction`).
//! - [`constraint`] — boolean `AND`/`OR`/`NOT` evaluation over evidence
//!   presence/absence.
//! - [`sequence`] — temporal ordering: resolving a `sequence:`
//!   constraint to a concrete, order-consistent set of bindings.
//! - [`ordering`] — the execution-order approximation [`sequence`] sorts
//!   and compares by.
//! - [`engine`] — orchestrates the four modules above into
//!   [`CandidateMatch`]es (see "Overall matching algorithm").
//! - [`binding`] — the public output types: [`CandidateMatch`],
//!   [`EvidenceBinding`], [`MatchMetadata`], [`UnresolvedAttribute`].
//! - [`error`] — [`MatcherError`], this crate's single exhaustive error
//!   type (ADR-0003).
//!
//! ## Internal indexes
//!
//! [`index::TraceIndex`] precomputes exactly one nontrivial structure:
//! for every call, whether its target address already appears among its
//! own strict ancestors' targets (the structural definition of
//! "reentrant" this crate uses — see [`index::TraceIndex::is_reentrant`]
//! for the full rationale and its documented limits). This is built with
//! a single forward pass over the trace's calls (already topologically
//! ordered by `fact-model`'s own invariants), amortizing an
//! otherwise-per-predicate-evaluation ancestor walk to once per trace.
//! Everything else (per-call/per-storage-change/per-log/per-transfer
//! predicate evaluation) is a direct linear scan with no additional
//! index — see "Complexity" below for why a richer index (e.g. an
//! address→calls map) is not built up front.
//!
//! ## Evidence binding
//!
//! A [`CandidateMatch`]'s [`EvidenceBinding`]s name, for every
//! positively-referenced and satisfied evidence clause, the *specific*
//! [`fact_model::FactRef`] that satisfied it — never a whole match set.
//! `NOT`-negated clauses confirmed absent get no binding (there is no
//! fact to cite for an absence). This directly supports the
//! Architecture document's central grounding requirement ("a
//! classification grounds against a specific `Call`/`StorageChange`") —
//! this crate produces exactly the citation shape grounding needs,
//! without itself performing the verification grounding is responsible
//! for.
//!
//! ## Interaction with `dsl`
//!
//! This crate consumes [`dsl::CompiledPattern`] exactly as `dsl`
//! produces it: [`dsl::ir::EvidenceRef`] indices, the
//! [`dsl::ir::CompiledConstraint`] tree, and [`dsl::ir::CompiledSequence`]
//! are read directly, with no re-interpretation of pattern semantics
//! `dsl` itself already settled (required/optional-ness is folded into
//! the compiled constraint by `dsl::compile`, so this crate does not
//! separately consult [`dsl::ir::Requiredness`] — see [`engine`]'s own
//! docs). Every predicate attribute [`dsl::schema`] defines is handled
//! somewhere in [`predicate`]; see that module's docs for the two
//! attributes (`storage.role`, `token_transfer.unexpected`) this crate
//! cannot structurally evaluate and how it flags that instead of
//! guessing.
//!
//! ## Interaction with `fact-model`
//!
//! This crate reads a [`fact_model::Trace`] exclusively through its
//! public accessors (`trace.arena.calls()`, `.storage_changes()`, etc.)
//! and cites facts back via [`fact_model::FactRef`], `fact-model`'s own
//! sum type for "a reference to exactly one fact, of any kind" — the
//! type the Architecture document specifies grounding results must use.
//! This crate performs no mutation and holds no data `fact-model` did
//! not already compute or store (see [`predicate`]'s "Predicate
//! semantics" for the one derived concept, execution order, this crate
//! adds on top of raw `fact-model` data, and why).
//!
//! ## Why grounding is intentionally excluded
//!
//! A [`CandidateMatch`] is a *structural* claim: "these facts exist, and
//! their shape satisfies this pattern's boolean/temporal constraints."
//! It is not a claim that the match is *correct* — that requires
//! checking things this crate has no way to check on its own:
//! - Whether a `storage(role: PriceOracle)` binding's slot really is a
//!   price oracle (a protocol-specific semantic fact, not a structural
//!   one — see [`predicate`]'s docs).
//! - Whether the evidence this crate bound is *complete*: the
//!   Architecture document's central "No evidence → No classification"
//!   requirement demands every clause be backed by a specific fact with
//!   no hand-waving, which is a stronger bar than "a fact exists that is
//!   structurally consistent with the predicate."
//! - Whether an "optional" clause was silently load-bearing in disguise
//!   — the Architecture document's own named top risk (Phase 6), which
//!   is exactly why [`MatchMetadata::unresolved_attributes`] exists:
//!   this crate refuses to paper over what it did not check.
//!
//! Collapsing candidate generation and grounding into one crate would
//! also make it impossible to reuse "found some structurally-plausible
//! matches" independent of "confirmed those matches are real" — a
//! benchmark harness measuring *matcher* recall separately from
//! *grounding* precision (the Architecture document's own Phase 5/10
//! distinction) needs exactly this seam.
//!
//! ## Complexity
//!
//! For one pattern with `E` evidence clauses (each referencing at most
//! one predicate) against a trace with `C` calls, `S` storage changes,
//! `L` logs, and `T` token transfers:
//!
//! - [`index::TraceIndex::build`]: `O(C × average call depth)`, run
//!   **once per trace**, not per pattern (see [`MatchEngine`]).
//! - Predicate evaluation (phase 1): `O(E × (C + S + L + T))` — each of
//!   the `E` predicates does one linear scan of the fact list its kind
//!   names (a `call` predicate scans calls, a `storage` predicate scans
//!   storage changes, etc.), plus an `O(log n)` sort of its own result
//!   by execution order.
//! - Constraint evaluation: `O(nodes in the constraint tree)` per anchor
//!   occurrence attempted — a small constant relative to trace size.
//! - Sequence resolution: `O(sequence steps × average bindings per
//!   step)` per anchor occurrence attempted (see [`sequence`]'s own
//!   docs for why this is correct, not just a heuristic, and where a
//!   binary-search improvement would go).
//! - Overall: `O(E × (C + S + L + T) + A × (nodes + steps × avg
//!   bindings))`, where `A` is the number of times the anchor clause
//!   matches (bounded by whichever of `C`/`S`/`L`/`T` the anchor
//!   predicate's kind scans). For realistic single-transaction traces
//!   (a few thousand facts at most, per the EVM's own call-depth and gas
//!   limits) and realistic patterns (a handful of evidence clauses),
//!   this is fast in absolute terms, not just asymptotically.
//!
//! ## Memory usage
//!
//! `O(total facts in the trace)` for [`index::TraceIndex`] (one
//! `HashMap` entry and, worst case, one ancestor-address set per call —
//! see [`index::TraceIndex`]'s own docs for the bound), plus `O(E ×
//! matches per clause)` transiently for phase 1's evidence-match lists,
//! held only for the duration of one [`find_candidate_matches`] call.
//! `CandidateMatch`es themselves are small (one [`fact_model::FactRef`]
//! — a `Copy` value — per bound evidence clause), so memory for the
//! *output* scales with match count, not trace size.
//!
//! ## Traversal strategy
//!
//! Every scan in this crate is a forward, single-pass iteration over
//! `fact-model`'s own storage order (`Vec` iteration, not tree
//! recursion) — the one exception is [`index::TraceIndex::build`]'s
//! ancestor-set computation, which is a single forward dynamic-
//! programming pass over the already-topologically-sorted call list
//! (parent's result is available before its children are processed),
//! not a separate recursive tree walk per call.
//!
//! ## Optimization opportunities (not taken, and why)
//!
//! Deliberately left for a future profiling-driven pass, per this task's
//! "avoid premature optimization" instruction:
//! - **Address-keyed call index.** A `HashMap<Address, Vec<CallId>>`
//!   would speed up predicates that filter heavily by address, but no
//!   current predicate does (`value_flow`/`direction` compares against
//!   a single anchor address per call, already `O(1)` per call).
//! - **Binary search in [`sequence::resolve`].** Each step's bindings
//!   are already sorted; a linear scan for the first `>=` bound is
//!   `O(log n)`-improvable but not currently a bottleneck at realistic
//!   per-clause match counts.
//! - **Persistent (structurally-shared) ancestor sets.** The current
//!   `HashSet<Address>`-per-call approach clones on every insertion;
//!   an `Rc`-based persistent set would reduce allocation for very deep,
//!   very wide call trees, at the cost of an extra dependency and
//!   indirection this crate's current trace-size targets don't justify.
//!
//! ## Why `within_seconds` is not enforced
//!
//! [`dsl::ir::CompiledSequence::within_seconds`] exists in the DSL for a
//! cross-transaction temporal pattern this crate's current scope does
//! not address: [`find_candidate_matches`] takes exactly one
//! [`fact_model::Trace`], which carries exactly one
//! [`fact_model::BlockContext`] and therefore exactly one block
//! timestamp for every fact it contains. Every pair of facts within one
//! trace therefore has a wall-clock delta of zero by construction, which
//! trivially satisfies any nonnegative `within_seconds` bound — enforcing
//! it explicitly would be dead code computing `0 <= within_seconds`
//! forever. [`sequence::resolve`] enforces *relative ordering* (via
//! [`ordering`]'s approximate execution-order key) instead, which is the
//! constraint this crate's single-trace scope can actually speak to.

#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::missing_const_for_fn
    )
)]

pub mod binding;
pub mod constraint;
pub mod engine;
pub mod error;
pub mod identity;
pub mod index;
pub mod ordering;
pub mod predicate;
pub mod sequence;

pub use binding::{CandidateMatch, EvidenceBinding, MatchMetadata, UnresolvedAttribute};
pub use error::MatcherError;
pub use index::TraceIndex;

use dsl::ir::CompiledPattern;
use fact_model::Trace;

/// The crate's own semantic version.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Re-exported so a future `benchmark-harness` provenance record can pin
/// exactly which `fact-model` and `dsl` versions a given `matcher` build
/// was compiled against.
pub const FACT_MODEL_VERSION_USED: &str = fact_model::CRATE_VERSION;

/// See [`FACT_MODEL_VERSION_USED`].
pub const DSL_VERSION_USED: &str = dsl::CRATE_VERSION;

/// A trace paired with its precomputed [`TraceIndex`], for matching many
/// patterns against the same trace without recomputing shared structure
/// per pattern.
///
/// Prefer this over calling [`find_candidate_matches`] repeatedly: a
/// pattern *library* (many patterns matched against one trace, the
/// realistic caller shape — see the Architecture document's own
/// "Matching Engine" section) should build one `MatchEngine` and reuse
/// it, so [`TraceIndex::build`]'s `O(calls × depth)` cost is paid once,
/// not once per pattern.
pub struct MatchEngine<'trace> {
    trace: &'trace Trace,
    index: TraceIndex<'trace>,
}

impl<'trace> MatchEngine<'trace> {
    /// Build a `MatchEngine` for `trace`, precomputing everything
    /// [`TraceIndex`] caches.
    #[must_use]
    pub fn new(trace: &'trace Trace) -> Self {
        Self {
            trace,
            index: TraceIndex::build(trace),
        }
    }

    /// The trace this engine was built for.
    #[must_use]
    pub const fn trace(&self) -> &'trace Trace {
        self.trace
    }

    /// Find every candidate match of `pattern` against this engine's
    /// trace.
    ///
    /// # Errors
    /// See [`find_candidate_matches`].
    pub fn find_matches(
        &self,
        pattern: &CompiledPattern,
    ) -> Result<Vec<CandidateMatch>, MatcherError> {
        engine::find_candidates(self.trace, &self.index, pattern)
    }

    /// Find every candidate match of every pattern in `patterns` against
    /// this engine's trace, in `patterns` order. A pattern that fails to
    /// match produces no entries for that pattern (not an error); only a
    /// malformed pattern (see [`MatcherError`]) short-circuits the whole
    /// call, since a pattern library the caller controls should not
    /// realistically contain one.
    ///
    /// # Errors
    /// See [`find_candidate_matches`].
    pub fn find_all_matches(
        &self,
        patterns: &[CompiledPattern],
    ) -> Result<Vec<CandidateMatch>, MatcherError> {
        let mut all = Vec::new();
        for pattern in patterns {
            all.extend(self.find_matches(pattern)?);
        }
        Ok(all)
    }
}

/// Find every candidate match of `pattern` against `trace`.
///
/// This is a convenience wrapper for the one-pattern, one-trace case; it
/// builds a fresh [`TraceIndex`] internally. Matching more than one
/// pattern against the same trace should use [`MatchEngine`] instead, to
/// build that index once and reuse it (see [`MatchEngine`]'s own docs).
///
/// # Errors
/// Returns [`MatcherError::DanglingEvidenceRef`] if `pattern` contains an
/// `EvidenceRef` outside its own `evidence` list — unreachable for any
/// pattern produced by `dsl`'s own compiler; see that error variant's
/// docs.
pub fn find_candidate_matches(
    trace: &Trace,
    pattern: &CompiledPattern,
) -> Result<Vec<CandidateMatch>, MatcherError> {
    MatchEngine::new(trace).find_matches(pattern)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_version_is_nonempty() {
        assert!(!CRATE_VERSION.is_empty());
    }

    /// Proves both path dependencies actually link.
    #[test]
    fn dependencies_link() {
        assert!(!FACT_MODEL_VERSION_USED.is_empty());
        assert!(!DSL_VERSION_USED.is_empty());
    }
}

//! [`MatcherError`]: the crate's single exhaustive error type, per
//! ADR-0003.

use dsl::ir::PatternId;
use dsl::EvidenceRef;

/// Everything that can cause candidate-match generation to fail.
///
/// Under normal operation — matching a [`dsl::CompiledPattern`] that was
/// actually produced by `dsl::compile_str`/`dsl::compile::compile`
/// against any [`fact_model::Trace`] — this crate never returns an
/// error: a validly-compiled pattern's [`dsl::ir::EvidenceRef`]s are
/// guaranteed in-bounds by construction (see `dsl::ir::CompiledPattern`'s
/// own documentation), and every predicate this crate evaluates against
/// fact-model data is total (it always resolves to *some* answer, never
/// panics). This type exists for the one case that guarantee doesn't
/// cover: a `CompiledPattern` assembled some other way (e.g. hand-built,
/// or deserialized from a hand-edited JSON file) that violates it.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum MatcherError {
    /// A pattern's constraint or sequence referenced an
    /// [`EvidenceRef`] that does not index into that same pattern's
    /// `evidence` list. This can only happen for a `CompiledPattern`
    /// not produced by `dsl`'s own compiler (see this type's own
    /// documentation) — `dsl::compile::compile` guarantees every
    /// `EvidenceRef` it emits is in-bounds.
    #[error(
        "pattern `{pattern}` references evidence index {} which does not exist in its own \
         evidence list", .evidence_ref.index()
    )]
    DanglingEvidenceRef {
        /// The pattern containing the dangling reference.
        pattern: PatternId,
        /// The out-of-bounds reference itself.
        evidence_ref: EvidenceRef,
    },
}

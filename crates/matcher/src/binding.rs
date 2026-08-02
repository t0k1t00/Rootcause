//! Output types: [`CandidateMatch`] and the evidence-binding structures
//! it is built from.
//!
//! These are the crate's only public output — deliberately independent
//! of the `grounding` crate (per ADR-0005, `grounding` depends on
//! `matcher`, never the reverse), so nothing here can reference a
//! grounding-crate type. `CandidateMatch` names *which* facts a pattern
//! structurally bound, and flags what the grounding crate will need to
//! independently verify on top of that; it does not itself decide
//! whether the match is valid (see this crate's top-level docs for why
//! that determination is explicitly out of scope here).

use fact_model::FactRef;

use dsl::ir::{PatternFamily, PatternId, PatternVersion, Severity};
use dsl::EvidenceRef;

/// One evidence clause bound to a specific fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceBinding {
    /// Which evidence clause this binds (an index into the matched
    /// pattern's `evidence` list — see [`dsl::ir::CompiledPattern`]).
    pub evidence: EvidenceRef,
    /// The specific fact bound to that clause.
    pub fact: FactRef,
}

/// One predicate attribute this crate could not structurally evaluate
/// against `fact-model` data, flagged for the grounding crate rather
/// than silently treated as satisfied.
///
/// This exists because a small number of DSL predicate attributes
/// (`storage.role`, `token_transfer.unexpected`) describe a semantic or
/// domain classification `fact-model`'s structural vocabulary has no
/// representation for at all (there is no "this slot is a price oracle"
/// or "this transfer was unexpected" fact anywhere in `fact-model`) — see
/// this crate's top-level docs, "Attributes this crate cannot evaluate,"
/// for the full list and rationale. Silently treating such an attribute
/// as satisfied would be exactly the failure mode the Architecture
/// document's grounding verifier exists to prevent (Phase 6: "optional
/// clauses silently treated as load-bearing") — so instead, every
/// [`CandidateMatch`] this crate emits carries an explicit record of
/// which claims it did *not* check, so grounding cannot mistake "matcher
/// didn't check this" for "matcher confirmed this."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedAttribute {
    /// Which evidence clause the attribute belongs to.
    pub evidence: EvidenceRef,
    /// The attribute's name (e.g. `"role"`).
    pub attribute: String,
    /// Why this crate could not evaluate it structurally.
    pub reason: String,
}

/// Metadata a [`CandidateMatch`] carries beyond its bindings, for the
/// grounding crate to consume.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MatchMetadata {
    /// Every predicate attribute across this match's bound evidence that
    /// this crate could not structurally evaluate. Grounding must
    /// independently verify each of these before treating this
    /// candidate as a real classification — an empty `Vec` means every
    /// attribute this pattern's predicates named was structurally
    /// checked.
    pub unresolved_attributes: Vec<UnresolvedAttribute>,
    /// Whether this match's evidence positions were verified against
    /// the pattern's `sequence:` constraint (`true`), or the pattern had
    /// no `sequence:` constraint at all (`false`) — lets grounding (or a
    /// benchmark harness) distinguish "no ordering was required" from
    /// "ordering was required and checked" without re-parsing the
    /// pattern.
    pub sequence_checked: bool,
}

/// One candidate way a [`dsl::ir::CompiledPattern`] structurally matches
/// a [`fact_model::Trace`].
///
/// A `CandidateMatch` is not a classification: it records that a
/// self-consistent set of fact bindings exists satisfying the pattern's
/// boolean constraint (and sequence constraint, if any) *structurally*.
/// Whether every binding is actually correct, complete, and free of
/// contradicting evidence is the grounding crate's job, not this crate's
/// — see this crate's top-level docs for the full rationale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateMatch {
    /// The identity of the pattern that produced this match.
    pub pattern_id: PatternId,
    /// The version of the pattern that produced this match.
    pub pattern_version: PatternVersion,
    /// The pattern's exploit family, copied from the pattern for
    /// convenience (so a consumer iterating many candidate matches
    /// doesn't need to keep the source pattern library in scope just to
    /// group or filter by family).
    pub pattern_family: PatternFamily,
    /// The pattern's severity, copied for the same convenience reason.
    pub pattern_severity: Severity,
    /// Every evidence-to-fact binding this candidate match established,
    /// in the matched pattern's evidence declaration order. An evidence
    /// clause that the pattern's constraint negates (`NOT(...)`) and
    /// that was confirmed *absent* has no entry here — there is no fact
    /// to cite for an absence.
    pub bindings: Vec<EvidenceBinding>,
    /// Metadata the grounding crate needs beyond the raw bindings.
    pub metadata: MatchMetadata,
}

impl CandidateMatch {
    /// Look up the fact bound to a specific evidence clause, if this
    /// candidate match bound one.
    #[must_use]
    pub fn fact_for(&self, evidence: EvidenceRef) -> Option<FactRef> {
        self.bindings
            .iter()
            .find(|b| b.evidence == evidence)
            .map(|b| b.fact)
    }
}

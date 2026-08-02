//! [`GroundingResult`] and the strongly-typed structures it is built
//! from: the evidence chain, confidence metadata, and abstention
//! reasons.

use std::fmt;

use fact_model::FactRef;

use dsl::ir::{PatternFamily, PatternId, PatternVersion, Severity};
use dsl::EvidenceRef;

use crate::status::{AbstentionCategory, EvidenceOutcome, GroundingStatus};

/// A deterministic fingerprint identifying which specific candidate a
/// [`GroundingResult`] describes.
///
/// `matcher::CandidateMatch` carries no identity field of its own (see
/// this crate's top-level docs, "Interaction with matcher," for why that
/// is matcher's correct design, not an oversight this crate works
/// around) — the Task's required "candidate id" field is therefore
/// computed here, deterministically, from the exact content that makes
/// two candidates the same or different: which pattern matched, and
/// which fact is bound to which evidence clause.
///
/// This deliberately does **not** use `std::collections::hash_map::
/// DefaultHasher`: its algorithm is explicitly unspecified by the
/// standard library and may change between Rust releases, which would
/// silently violate this crate's own "the result should be
/// deterministic" requirement across a toolchain upgrade. [`CandidateId`]
/// instead uses a small, self-contained FNV-1a implementation — a fixed,
/// fully-specified algorithm with no version dependency, at the cost of
/// one screen of hand-written code instead of a standard-library
/// one-liner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CandidateId(pub u64);

impl fmt::Display for CandidateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

impl CandidateId {
    /// Compute the fingerprint for `candidate`, hashing its pattern
    /// identity and every `(evidence, fact)` binding pair in a fixed,
    /// binding-order-independent order (bindings are sorted by evidence
    /// index first — see `matcher::engine`'s own docs confirming
    /// `CandidateMatch::bindings` is already produced in that order, but
    /// this sorts again defensively so the fingerprint is stable even
    /// against a hand-constructed `CandidateMatch` with bindings in a
    /// different order).
    #[must_use]
    pub fn compute(candidate: &matcher::CandidateMatch) -> Self {
        let mut bindings: Vec<(EvidenceRef, FactRef)> = candidate
            .bindings
            .iter()
            .map(|b| (b.evidence, b.fact))
            .collect();
        bindings.sort_by_key(|(e, _)| e.index());

        let mut hasher = Fnv1a::new();
        hasher.write_str(&candidate.pattern_id.0);
        hasher.write_u64(u64::from(candidate.pattern_version.0));
        for (evidence, fact) in bindings {
            hasher.write_u64(evidence.index() as u64);
            hasher.write_str(&format!("{fact:?}"));
        }
        Self(hasher.finish())
    }
}

/// A minimal FNV-1a (Fowler-Noll-Vo) hasher: fixed constants, no
/// external dependency, fully specified behavior across any Rust
/// version — see [`CandidateId`]'s own docs for why this crate does not
/// use `DefaultHasher`.
struct Fnv1a(u64);

impl Fnv1a {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01B3;

    const fn new() -> Self {
        Self(Self::OFFSET_BASIS)
    }

    fn write_byte(&mut self, byte: u8) {
        self.0 ^= u64::from(byte);
        self.0 = self.0.wrapping_mul(Self::PRIME);
    }

    fn write_str(&mut self, s: &str) {
        for byte in s.as_bytes() {
            self.write_byte(*byte);
        }
        // A length-prefix-free hash of concatenated strings is
        // ambiguous (`"ab"` + `"c"` hashes the same as `"a"` + `"bc"`);
        // a delimiter byte outside valid UTF-8 (`0xff`) separates every
        // written string unambiguously.
        self.write_byte(0xff);
    }

    fn write_u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.write_byte(byte);
        }
    }

    const fn finish(&self) -> u64 {
        self.0
    }
}

/// One entry in a [`GroundingResult`]'s evidence chain: the complete,
/// self-contained record of independently verifying one evidence
/// clause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceVerification {
    /// Which evidence clause this step verifies.
    pub evidence: EvidenceRef,
    /// That clause's declared name, copied in for human readability
    /// without needing the source pattern in scope.
    pub name: String,
    /// Whether the pattern's DSL declared this clause `required` or
    /// `optional` (see [`dsl::ir::Requiredness`]) — copied here because
    /// [`crate::engine`]'s abstention/grounding decision depends on it
    /// (only required clauses can force [`GroundingStatus::Abstain`] or
    /// [`GroundingStatus::Ungrounded`]) and a reader of one evidence-chain
    /// entry in isolation should be able to tell why it did or didn't.
    pub requiredness: dsl::ir::Requiredness,
    /// Whether the pattern's constraint requires this clause's
    /// predicate to hold, to not hold, or does not reference it at all.
    pub polarity: crate::polarity::Polarity,
    /// The independently-verified outcome.
    pub outcome: EvidenceOutcome,
    /// Every fact this verification step cites — plural, per the Task's
    /// own "referenced FactRef values" phrasing, though every built-in
    /// verifier currently cites at most one (see
    /// `crate::verifier::VerifierOutcome::fact`); kept as a `Vec` so a
    /// future verifier needing to cite several corroborating facts for
    /// one clause does not need this struct's shape to change.
    pub facts: Vec<FactRef>,
    /// Why this outcome was reached, always a complete sentence.
    pub reason: String,
    /// Additional context that doesn't fit `reason` (e.g. which
    /// attribute names were unresolved) — empty for the common case.
    pub notes: Vec<String>,
}

/// One reason grounding abstained, categorized under the Engineering
/// Specification's abstention-reason taxonomy (see
/// [`AbstentionCategory`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbstainReason {
    /// Which of the taxonomy's categories this reason falls under.
    pub category: AbstentionCategory,
    /// Which evidence clause triggered this reason, if the trigger was
    /// clause-specific (every current trigger is; `None` is reserved for
    /// a future whole-pattern-level abstention trigger, e.g. an
    /// engine-level condition unrelated to any one clause).
    pub evidence: Option<EvidenceRef>,
    /// A human-readable explanation.
    pub detail: String,
}

/// Confidence metadata, **deliberately non-probabilistic**: a plain,
/// auditable count of how many required/optional clauses (and whether
/// the sequence constraint, if any) independently verified, rather than
/// a single asserted probability or score. A reader can compute their
/// own ratio from these counts if they want one; this crate does not
/// assert a number it cannot actually justify statistically (no
/// probability model exists anywhere in this crate — see this crate's
/// top-level docs, "Confidence metadata").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfidenceMetadata {
    /// How many required evidence clauses this pattern declares.
    pub required_total: usize,
    /// How many of those came back [`EvidenceOutcome::Verified`].
    pub required_verified: usize,
    /// How many optional evidence clauses this pattern declares.
    pub optional_total: usize,
    /// How many of those came back [`EvidenceOutcome::Verified`].
    pub optional_verified: usize,
    /// Whether the pattern's `sequence:` constraint (if any) was
    /// independently re-confirmed. `None` if the pattern declares no
    /// sequence.
    pub sequence_verified: Option<bool>,
    /// Whether the pattern's `same_call:` constraint (if any) was
    /// independently re-confirmed. `None` if the pattern declares no
    /// `same_call:` group.
    pub same_call_verified: Option<bool>,
}

/// The complete result of independently grounding one
/// [`matcher::CandidateMatch`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroundingResult {
    /// The pattern's identity.
    pub pattern_id: PatternId,
    /// The pattern's version.
    pub pattern_version: PatternVersion,
    /// The pattern's exploit family, copied for convenience.
    pub pattern_family: PatternFamily,
    /// The pattern's severity, copied for convenience.
    pub pattern_severity: Severity,
    /// A deterministic fingerprint of the specific candidate this
    /// result describes — see [`CandidateId`]'s own docs.
    pub candidate_id: CandidateId,
    /// The overall grounding status.
    pub status: GroundingStatus,
    /// Every required evidence clause that came back
    /// [`EvidenceOutcome::Verified`].
    pub verified_evidence: Vec<EvidenceRef>,
    /// Every required evidence clause that came back
    /// [`EvidenceOutcome::Failed`].
    pub failed_evidence: Vec<EvidenceRef>,
    /// Every required evidence clause that came back
    /// [`EvidenceOutcome::Unavailable`].
    pub unavailable_evidence: Vec<EvidenceRef>,
    /// Every required evidence clause that came back
    /// [`EvidenceOutcome::Unsupported`].
    pub unsupported_evidence: Vec<EvidenceRef>,
    /// Non-probabilistic confidence accounting — see
    /// [`ConfidenceMetadata`]'s own docs.
    pub confidence: ConfidenceMetadata,
    /// Every predicate attribute this result's evidence chain could not
    /// structurally resolve, propagated from `matcher`'s own findings
    /// (see `matcher::MatchMetadata::unresolved_attributes`) and
    /// extended with anything grounding itself additionally found
    /// unresolvable.
    pub unresolved_attributes: Vec<matcher::UnresolvedAttribute>,
    /// The complete, per-clause verification record — see
    /// [`EvidenceVerification`].
    pub evidence_chain: Vec<EvidenceVerification>,
    /// Every reason grounding abstained, if [`Self::status`] is
    /// [`GroundingStatus::Abstain`]. Always empty otherwise.
    pub abstain_reasons: Vec<AbstainReason>,
    /// A human-readable, one-paragraph summary of this result, suitable
    /// for a log line or a benchmark report row without needing to
    /// re-derive it from the other fields.
    pub explanation: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_is_deterministic() {
        let mut a = Fnv1a::new();
        a.write_str("hello");
        a.write_u64(42);
        let mut b = Fnv1a::new();
        b.write_str("hello");
        b.write_u64(42);
        assert_eq!(a.finish(), b.finish());
    }

    #[test]
    fn fnv1a_distinguishes_different_input() {
        let mut a = Fnv1a::new();
        a.write_str("hello");
        let mut b = Fnv1a::new();
        b.write_str("world");
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn fnv1a_delimiter_prevents_concatenation_collision() {
        // Without a delimiter, "ab"+"c" and "a"+"bc" would hash
        // identically.
        let mut a = Fnv1a::new();
        a.write_str("ab");
        a.write_str("c");
        let mut b = Fnv1a::new();
        b.write_str("a");
        b.write_str("bc");
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn candidate_id_display_is_fixed_width_hex() {
        let id = CandidateId(0xdead_beef);
        assert_eq!(id.to_string().len(), 16);
    }
}

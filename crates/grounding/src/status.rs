//! [`GroundingStatus`] and [`EvidenceOutcome`]: the strongly-typed
//! result vocabulary this crate uses instead of booleans.

use std::fmt;

/// The overall result of grounding one [`matcher::CandidateMatch`].
///
/// Deliberately three-valued, never a `bool`: a `CandidateMatch` is
/// never simply "matched" or "not matched" once grounding has looked at
/// it — see this crate's top-level docs, "Abstention policy," for why
/// [`Self::Abstain`] is a mandatory, first-class outcome rather than a
/// fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroundingStatus {
    /// Every required evidence clause (and, if the pattern declares
    /// one, the `sequence:` ordering) was independently verified
    /// against the trace with no unresolved semantic attributes and no
    /// contradicting facts. This is the Architecture document's "No
    /// evidence → No classification" bar, met in the positive
    /// direction: every clause *has* evidence, specifically.
    Grounded,
    /// At least one required evidence clause was independently checked
    /// in full (no missing facts, no unresolved attributes) and found
    /// to genuinely contradict the pattern — a confident negative, not
    /// an absence of information. See [`Self::Abstain`]'s own docs for
    /// why this is distinct from, and only reached when, no uncertainty
    /// is present.
    Ungrounded,
    /// Grounding could not confirm *or* refute the candidate with full
    /// confidence — required evidence was missing, a predicate's
    /// attribute could not be structurally verified, an evidence
    /// clause's polarity or ordering was ambiguous, or the trace was
    /// otherwise incomplete relative to what the pattern needs. This is
    /// the mandatory, conservative default whenever any such
    /// uncertainty exists, even alongside an otherwise-contradicting
    /// clause — see [`crate::engine`]'s own docs for the exact
    /// precedence rule and why uncertainty always outranks a confident
    /// negative.
    Abstain,
}

impl fmt::Display for GroundingStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Grounded => "grounded",
            Self::Ungrounded => "ungrounded",
            Self::Abstain => "abstain",
        })
    }
}

/// The independently-verified outcome for one evidence clause.
///
/// This is *not* the same thing as whether [`matcher`] found a binding
/// for the clause: grounding re-derives this from the trace itself (see
/// this crate's top-level docs, "Overall grounding algorithm"), so a
/// clause `matcher` bound can still come back [`Self::Failed`] here if
/// independent re-verification disagrees, and a clause `matcher` left
/// unbound can still come back [`Self::Verified`] if grounding finds its
/// own supporting fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvidenceOutcome {
    /// The clause's requirement (presence, for a positively-referenced
    /// clause; confirmed absence, for a `NOT`-negated one) was
    /// independently confirmed against the trace, with every attribute
    /// the predicate names fully checkable.
    Verified,
    /// The clause's requirement was checkable in full, and the trace
    /// genuinely contradicts it — a cited fact does not actually
    /// satisfy the predicate `matcher` claimed it did, or a
    /// `NOT`-negated clause's forbidden condition was found present
    /// after all.
    Failed,
    /// No fact anywhere in the trace satisfies the clause's predicate
    /// (considered as permissively as possible — see
    /// [`crate::verifier`]'s own docs on how unresolved attributes are
    /// handled), so a positively-referenced required clause has nothing
    /// to ground against. Never produced for a `NOT`-negated clause:
    /// zero matching facts *confirms* the required absence, which is
    /// [`Self::Verified`], not this variant.
    Unavailable,
    /// The predicate names an attribute (or, for a wholly unrecognized
    /// predicate kind, the predicate itself) that this crate's
    /// verifiers cannot structurally check against `fact-model` data —
    /// see [`crate::verifier`]'s own docs for the specific attributes
    /// this applies to and why. Distinct from [`Self::Unavailable`]:
    /// facts *do* exist that structurally match every attribute this
    /// crate *can* check, but at least one attribute's true value is
    /// simply not knowable from `fact-model` alone.
    Unsupported,
}

impl fmt::Display for EvidenceOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Verified => "verified",
            Self::Failed => "failed",
            Self::Unavailable => "unavailable",
            Self::Unsupported => "unsupported",
        })
    }
}

/// The Engineering Specification's own three-category abstention-reason
/// taxonomy (Assumption E-4: missing-fact / unresolvable-layout /
/// structural-mismatch), reused here rather than inventing a new one
/// (extended with one further category — see
/// [`Self::AmbiguousAttribution`] — for a condition specific to this
/// crate's sequence handling that the original three do not cleanly
/// cover). Every [`crate::report::AbstainReason`] and every
/// [`EvidenceOutcome::Failed`] explanation this crate produces is
/// categorized under exactly one of these buckets, so a caller
/// filtering or aggregating abstention/failure reasons across many
/// [`crate::GroundingResult`]s has one stable vocabulary to group by,
/// per the Engineering Specification's own stated intent (Phase 10 of
/// the canonical Scientific Review: "a precise written note on cause").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AbstentionCategory {
    /// No fact anywhere in the trace supports a required clause —
    /// [`EvidenceOutcome::Unavailable`]'s category.
    MissingFact,
    /// A predicate attribute (or predicate kind) could not be
    /// structurally resolved against `fact-model` data —
    /// [`EvidenceOutcome::Unsupported`]'s category. Named
    /// "unresolvable-layout" per the Engineering Specification's own
    /// term (originally coined for storage-slot-to-variable resolution,
    /// Assumption A-5); this crate uses it for the broader, but
    /// conceptually identical, class of "we cannot resolve what this
    /// evidence structurally refers to" — see [`crate::verifier`]'s own
    /// docs.
    UnresolvableLayout,
    /// The candidate's evidence positions could not be attributed to a
    /// single, unambiguous temporal ordering (a `sequence:` constraint
    /// that could not be independently re-confirmed) — grounding cannot
    /// determine which specific facts the pattern's ordering claim
    /// actually refers to.
    AmbiguousAttribution,
    /// Facts were independently checked in full and genuinely
    /// contradict the pattern — [`EvidenceOutcome::Failed`]'s category.
    /// Reused for [`crate::report::AbstainReason`] only in the case
    /// documented on [`crate::engine`]: a `Failed` clause on one branch
    /// combined with genuine uncertainty on another still yields an
    /// overall [`GroundingStatus::Abstain`], and the `Failed` clause's
    /// own reason is still worth surfacing alongside the uncertainty
    /// that forced abstention.
    StructuralMismatch,
}

impl fmt::Display for AbstentionCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MissingFact => "missing-fact",
            Self::UnresolvableLayout => "unresolvable-layout",
            Self::AmbiguousAttribution => "ambiguous-attribution",
            Self::StructuralMismatch => "structural-mismatch",
        })
    }
}

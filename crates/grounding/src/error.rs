//! [`GroundingError`]: the crate's single exhaustive error type, per
//! ADR-0003.
//!
//! This is deliberately a *small* enum: most of what could go wrong
//! grounding a candidate is not an error at all, it's an
//! [`crate::GroundingStatus::Abstain`] result — see this crate's
//! top-level docs, "Abstention policy," for why "we couldn't confirm
//! this evidence" is modeled as a normal, successful outcome rather than
//! a `Result::Err`. [`GroundingError`] is reserved for the narrower set
//! of cases where the *inputs themselves* don't cohere well enough to
//! produce any [`crate::GroundingResult`] at all — a caller/usage
//! problem, not a fact about the trace.

use fact_model::FactRef;

use dsl::ir::{PatternId, PatternVersion};

/// Everything that can prevent grounding from producing a
/// [`crate::GroundingResult`] at all.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum GroundingError {
    /// An internal invariant was violated — a bug in this crate (or in
    /// one of its frozen dependencies) rather than a fact about the
    /// input. Distinguished from every other variant so a caller can
    /// tell "something is wrong with my input" apart from "something is
    /// wrong with the grounding engine itself."
    #[error("internal grounding error: {reason}")]
    Internal {
        /// What invariant was violated.
        reason: String,
    },

    /// The [`crate::verifier::VerifierRegistry`] this call was
    /// configured with has no [`crate::verifier::Verifier`] registered
    /// for a predicate kind the pattern needs, *and* the registry
    /// itself is otherwise empty enough that this looks like a
    /// configuration mistake rather than ordinary DSL-schema evolution.
    ///
    /// This is deliberately **not** the outcome for "a new predicate
    /// kind was added to `dsl::schema` before `grounding` grew a
    /// matching verifier" — that case degrades gracefully to a
    /// per-evidence [`crate::EvidenceOutcome::Unsupported`] and an
    /// overall [`crate::GroundingStatus::Abstain`] result (see this
    /// crate's top-level docs, "Future extensibility"), not a hard
    /// error: an unsupported predicate for one clause of one pattern is
    /// exactly the "unsupported predicate encountered" abstention
    /// trigger, not a reason to refuse producing a report at all. This
    /// variant exists for the narrower case of a caller-supplied
    /// [`crate::verifier::VerifierRegistry`] built with
    /// [`crate::verifier::VerifierRegistry::new`] (empty) and never
    /// populated — using the default registry
    /// ([`crate::verifier::VerifierRegistry::with_default_verifiers`])
    /// makes this variant unreachable for any pattern `dsl::validate`
    /// accepts, since `dsl::schema::ALL` is exactly the five kinds the
    /// default registry covers.
    #[error("verifier registry has no verifiers registered at all (kind requested: `{kind}`)")]
    UnsupportedVerifier {
        /// The predicate kind that was requested.
        kind: String,
    },

    /// A [`FactRef`] the [`matcher::CandidateMatch`] cited does not
    /// resolve against the [`fact_model::Trace`] this call was given —
    /// the trace and the candidate do not correspond to each other
    /// (most likely a caller passed a different trace than the one the
    /// candidate was matched against).
    #[error("candidate cites {fact} which does not exist in the given trace")]
    MissingFacts {
        /// The unresolvable fact reference.
        fact: FactRef,
    },

    /// The candidate, trace, and pattern given to this call do not
    /// describe a coherent whole — currently the only condition
    /// producing this is a candidate whose `pattern_id`/`pattern_version`
    /// does not match the `pattern` argument. Grounding a candidate
    /// against a pattern it was not actually matched against would
    /// silently produce a meaningless report, so this is refused
    /// outright rather than attempted.
    #[error(
        "candidate was matched against pattern `{candidate_pattern_id}` version \
         {candidate_pattern_version}, not the pattern `{given_pattern_id}` version \
         {given_pattern_version} this call was given"
    )]
    IncompleteTrace {
        /// The pattern identity the candidate itself names.
        candidate_pattern_id: PatternId,
        /// The pattern version the candidate itself names.
        candidate_pattern_version: PatternVersion,
        /// The pattern identity actually passed to this call.
        given_pattern_id: PatternId,
        /// The pattern version actually passed to this call.
        given_pattern_version: PatternVersion,
    },

    /// A pattern's constraint is self-contradictory in a way that makes
    /// verification undecidable: the same evidence clause is referenced
    /// under both an even and an odd number of `NOT`s somewhere in the
    /// constraint tree (e.g. `AND(a, NOT(a))`), so there is no single,
    /// well-defined answer to "is this clause required to be present or
    /// absent." `dsl::validate` does not reject this shape (it is not a
    /// structural or reference error at the DSL level), so grounding
    /// must detect and refuse it itself rather than silently picking
    /// one polarity.
    #[error("evidence index {} is referenced with contradictory polarity in the pattern's constraint", .evidence.index())]
    VerificationFailure {
        /// The self-contradictorily-referenced evidence clause.
        evidence: dsl::EvidenceRef,
    },
}

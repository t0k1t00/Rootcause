//! # grounding
//!
//! Independent evidence verification: the core scientific contribution
//! of Root Cause. Given a [`matcher::CandidateMatch`] (a *structural*
//! claim — see `matcher`'s own top-level docs), this crate independently
//! re-derives, clause by clause, whether the trace actually supports it,
//! and produces a [`GroundingResult`] with an explicit
//! [`GroundingStatus`] — never a bare `bool`.
//!
//! ## Overall grounding algorithm
//!
//! For one candidate against its trace and pattern:
//!
//! 1. **Input coherence check** ([`engine`]): confirm the candidate was
//!    actually matched against `pattern` (same id/version), and that
//!    every fact it cites resolves in `trace`. Failure here is a caller
//!    error ([`GroundingError`]), not an evidence-quality question.
//! 2. **Polarity analysis** ([`polarity`]): walk `pattern`'s compiled
//!    constraint tree once to determine, for every evidence clause,
//!    whether the pattern requires it *present* or *absent* — needed
//!    because verifying a `NOT`-negated clause means confirming
//!    absence, not presence (see [`verifier`]'s docs).
//! 3. **Evidence-by-evidence verification** ([`verifier`],
//!    [`verifiers`]): for *every* evidence clause the pattern declares
//!    (not just the ones `matcher` happened to bind), independently
//!    recompute which facts in the trace satisfy its predicate — never
//!    trusting the candidate's own binding at face value — and produce
//!    one [`EvidenceOutcome`] each.
//! 4. **Sequence re-confirmation** (`engine::verify_sequence`, private
//!    but documented on [`engine`]): if the pattern declares a
//!    `sequence:` constraint, independently re-check that the facts
//!    grounding itself verified actually occur in the required relative
//!    order.
//! 5. **Abstention-aware aggregation** (`engine::aggregate_status`,
//!    private but documented on [`engine`]): combine every required
//!    clause's outcome (and the sequence check) into one
//!    [`GroundingStatus`], following the precedence rule documented on
//!    [`engine`]: any uncertainty anywhere outranks a confident
//!    contradiction.
//! 6. **Reporting** ([`report`]): assemble the complete
//!    [`GroundingResult`] — status, categorized evidence lists,
//!    non-probabilistic confidence metadata, the full evidence chain,
//!    and (if abstaining) every [`report::AbstainReason`].
//!
//! ## Verification phases (module map)
//!
//! - [`error`] — [`GroundingError`], this crate's single exhaustive
//!   error type (ADR-0003).
//! - [`status`] — [`GroundingStatus`], [`EvidenceOutcome`], and
//!   [`status::AbstentionCategory`] (the Engineering Specification's own
//!   abstention-reason taxonomy).
//! - [`polarity`] — constraint polarity analysis (phase 2, above).
//! - [`verifier`] — the [`verifier::Verifier`] trait,
//!   [`verifier::VerifierOutcome`], the extensible
//!   [`verifier::VerifierRegistry`], and the shared verification logic
//!   every built-in verifier delegates to.
//! - [`verifiers`] — the five built-in [`verifier::Verifier`]
//!   implementations, one per [`dsl::schema::ALL`] predicate kind.
//! - [`report`] — [`GroundingResult`] and everything it's built from:
//!   [`report::EvidenceVerification`] (the evidence chain),
//!   [`report::ConfidenceMetadata`], [`report::AbstainReason`], and
//!   [`report::CandidateId`] (a deterministic candidate fingerprint —
//!   see "Interaction with matcher" below).
//! - [`engine`] — [`GroundingEngine`], orchestrating every phase above.
//!
//! ## Verifier architecture
//!
//! Every predicate kind is verified through the [`verifier::Verifier`]
//! trait, dispatched by a [`verifier::VerifierRegistry`] keyed on
//! predicate-kind name — never a hardcoded `match` in the engine. Adding
//! support for a future DSL predicate kind means implementing
//! [`verifier::Verifier`] and calling
//! [`verifier::VerifierRegistry::register`]; no existing verifier's code
//! changes (see [`verifiers`]'s own docs for why all five *built-in*
//! verifiers currently share one implementation via
//! [`verifier::verify_via_matcher_predicate`], and why a future one
//! doesn't have to). A predicate kind with no registered verifier
//! degrades gracefully to a per-evidence
//! [`EvidenceOutcome::Unsupported`] (see [`GroundingError::
//! UnsupportedVerifier`]'s own docs for the one case that's a hard error
//! instead) — this is itself the "unsupported predicate encountered"
//! abstention trigger the Task requires, achieved structurally rather
//! than as a special case.
//!
//! ## Abstention policy
//!
//! Abstention is not a fallback path bolted onto this crate — it is the
//! honest default whenever independent verification cannot reach full
//! confidence, and it is reached the same way every other status is:
//! through `engine::aggregate_status`'s documented precedence rule
//! (see [`engine`]'s own docs for the full rule and rationale). Every
//! Task-mandated abstention trigger maps onto one of the four
//! [`EvidenceOutcome`] variants (or the sequence check) rather than
//! being handled as a separate code path:
//!
//! | Task's abstention trigger | How this crate reaches it |
//! |---|---|
//! | required evidence cannot be verified | [`EvidenceOutcome::Unavailable`]/[`EvidenceOutcome::Unsupported`] on a required clause |
//! | trace lacks required information | [`EvidenceOutcome::Unavailable`] (zero matching facts) |
//! | matcher deferred semantic attributes | [`EvidenceOutcome::Unsupported`] (`storage.role`, `token_transfer.unexpected` — see [`verifier`]) |
//! | unsupported predicate encountered | [`EvidenceOutcome::Unsupported`] (no registered verifier) |
//! | incomplete trace | [`GroundingError::MissingFacts`] if uncheckable at all, else [`EvidenceOutcome::Unavailable`] per-clause |
//! | ambiguous attribution | [`status::AbstentionCategory::AmbiguousAttribution`] (sequence re-confirmation fails or cannot run) |
//!
//! This crate **never** upgrades any of these into
//! [`GroundingStatus::Grounded`]: `engine::aggregate_status` checks
//! for uncertainty *before* it checks for confident contradiction, so
//! there is no code path that can produce `Grounded` while any required
//! clause is anything other than [`EvidenceOutcome::Verified`].
//!
//! ## Evidence-chain construction
//!
//! [`GroundingResult::evidence_chain`] contains exactly one
//! [`report::EvidenceVerification`] per evidence clause the pattern
//! declares — required *and* optional, referenced by the constraint or
//! not — so the chain is a complete record of everything grounding
//! checked, not just the subset that happened to affect the final
//! status. Each entry independently carries the evidence identifier,
//! every fact cited, the outcome, a reason, and any additional notes
//! (see [`report::EvidenceVerification`]'s own docs) — self-contained
//! enough to audit in isolation, without cross-referencing the rest of
//! the report.
//!
//! ## Interaction with `matcher`
//!
//! This crate depends on `matcher` for two distinct things, deliberately
//! kept separate:
//! - **What a `matcher::CandidateMatch` claims** (`pattern_id`,
//!   `bindings`, `metadata`) — the *input* grounding independently
//!   checks, never assumed correct.
//! - **What a predicate structurally means**
//!   (`matcher::predicate::evaluate`, `matcher::TraceIndex`,
//!   `matcher::ordering::order_key`) — machinery grounding *reuses*,
//!   because that meaning is `matcher`'s to define and re-defining it
//!   here would risk drift (see [`verifier`]'s own docs for the full
//!   argument).
//!
//! One consequence worth calling out: `matcher::CandidateMatch` carries
//! no identity field of its own, by `matcher`'s own design (a candidate
//! *is* its bindings — see `matcher::binding`'s docs). The Task's
//! required "candidate id" field is therefore computed here, in
//! [`report::CandidateId`], as a deterministic fingerprint over the
//! candidate's pattern identity and bindings — this crate's own
//! responsibility, not a gap in `matcher`.
//!
//! ## Interaction with `fact-model`
//!
//! Every verifier ultimately reads `trace.arena` through `fact-model`'s
//! own public accessors (via `matcher::predicate::evaluate`, which does
//! the same — see that module's docs) and cites facts back via
//! `fact_model::FactRef`. This crate performs no mutation and adds no
//! derived state to `fact-model` beyond what `matcher` already
//! introduced (execution-order approximation — see
//! `matcher::ordering`'s docs, reused unchanged in [`engine`]'s sequence
//! re-confirmation).
//!
//! ## Interaction with `dsl`
//!
//! This crate reads [`dsl::ir::CompiledPattern`] fields `matcher`
//! deliberately does *not* consult: [`dsl::ir::Requiredness`] (matcher
//! folds required-ness into the compiled constraint and never looks at
//! it again — see `matcher::engine`'s docs; grounding is exactly the
//! layer the Architecture document expects to care about it, since only
//! required clauses can force [`GroundingStatus::Abstain`] or
//! [`GroundingStatus::Ungrounded`]) and the constraint tree's *polarity*
//! per clause (`matcher::constraint` only asks whether the whole tree
//! holds, never which clauses were positively vs. negatively
//! referenced — see [`polarity`]'s own docs for why grounding needs
//! that distinction and matcher does not).
//!
//! ## Complexity
//!
//! For one candidate against a pattern with `E` evidence clauses and a
//! trace with `F` total facts:
//! - Polarity analysis: `O(nodes in the constraint tree)`, a small
//!   constant relative to `E`.
//! - Evidence verification: `O(E)` calls into
//!   `matcher::predicate::evaluate`, each `O(F)` (a single linear scan
//!   of the trace's facts of the relevant kind — see `matcher`'s own
//!   complexity docs), for `O(E × F)` total — dominated by the same cost
//!   `matcher` itself already pays to find the candidate in the first
//!   place, since grounding evaluates every predicate exactly once more,
//!   not once per candidate fact.
//! - Sequence re-confirmation: `O(sequence steps)`, using outcomes
//!   already computed above (no additional trace scan).
//! - Aggregation and reporting: `O(E)`.
//! - Overall: `O(E × F)`, matching `matcher`'s own per-pattern cost
//!   order — grounding one candidate costs about the same as finding it
//!   did.
//!
//! Building one `TraceIndex` per [`GroundingEngine::ground`] call (not
//! shared across calls, unlike `matcher::MatchEngine`) is a deliberate
//! simplicity/cost tradeoff: `TraceIndex::build` is `O(calls × average
//! depth)` (see `matcher`'s own docs), small relative to the `O(E × F)`
//! verification cost above for realistic trace sizes, and a
//! caller grounding many candidates from the *same* trace can still
//! avoid repeated `TraceIndex` construction by batching through
//! [`ground_all`], which builds one `TraceIndex` for the whole batch.
//!
//! ## Future extensibility
//!
//! Two independent extension points, deliberately decoupled:
//! - **New predicate kinds**: implement [`verifier::Verifier`], register
//!   it via [`verifier::VerifierRegistry::register`]. No change to
//!   [`engine`], [`report`], or any existing verifier.
//! - **New abstention categories**: [`status::AbstentionCategory`] is a
//!   normal exhaustive enum (per this workspace's "prefer exhaustive
//!   enums" rule); adding a variant is a deliberate, visible, versioned
//!   decision, not a silent extension — consistent with how `dsl` and
//!   `matcher` both treat their own closed enums.
//!
//! Not implemented here, by this task's explicit instruction: taxonomy
//! mapping (translating a [`GroundingResult`] into an external
//! vocabulary like SCWE/SWC) and any CLI surface — both belong to
//! dedicated, later tasks.

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

pub mod engine;
pub mod error;
pub mod polarity;
pub mod report;
pub mod status;
pub mod verifier;
pub mod verifiers;

pub use engine::{ground, GroundingEngine};
pub use error::GroundingError;
pub use report::{
    AbstainReason, CandidateId, ConfidenceMetadata, EvidenceVerification, GroundingResult,
};
pub use status::{AbstentionCategory, EvidenceOutcome, GroundingStatus};

use fact_model::Trace;

/// The crate's own semantic version.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// See the equivalent constant in `ingestion`/`matcher` for rationale.
pub const FACT_MODEL_VERSION_USED: &str = fact_model::CRATE_VERSION;

/// See [`FACT_MODEL_VERSION_USED`].
pub const MATCHER_VERSION_USED: &str = matcher::CRATE_VERSION;

/// Ground every candidate in `candidates` against `trace`, reusing one
/// [`matcher::TraceIndex`] across the whole batch — the batched
/// counterpart to calling [`ground`] once per candidate, for the
/// realistic case of grounding every candidate `matcher` found for one
/// trace in one pass. `patterns` must contain, for every candidate, the
/// exact [`dsl::ir::CompiledPattern`] it was matched against (looked up
/// by `pattern_id`/`pattern_version`).
///
/// # Errors
/// Returns the first [`GroundingError`] encountered (see
/// [`GroundingEngine::ground`]); a candidate whose pattern cannot be
/// found in `patterns` produces
/// [`GroundingError::IncompleteTrace`] naming the candidate's own
/// pattern identity.
pub fn ground_all(
    candidates: &[matcher::CandidateMatch],
    trace: &Trace,
    patterns: &[dsl::ir::CompiledPattern],
) -> Result<Vec<report::GroundingResult>, GroundingError> {
    let engine = GroundingEngine::new();
    let mut results = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let pattern = patterns
            .iter()
            .find(|p| p.id == candidate.pattern_id && p.version == candidate.pattern_version)
            .ok_or_else(|| GroundingError::IncompleteTrace {
                candidate_pattern_id: candidate.pattern_id.clone(),
                candidate_pattern_version: candidate.pattern_version,
                given_pattern_id: candidate.pattern_id.clone(),
                given_pattern_version: candidate.pattern_version,
            })?;
        results.push(engine.ground(candidate, trace, pattern)?);
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_version_is_nonempty() {
        assert!(!CRATE_VERSION.is_empty());
    }

    #[test]
    fn dependencies_link() {
        assert!(!FACT_MODEL_VERSION_USED.is_empty());
        assert!(!MATCHER_VERSION_USED.is_empty());
    }
}

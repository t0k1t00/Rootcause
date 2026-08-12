//! [`GroundingEngine`]: orchestrates verifier dispatch, evidence
//! verification, sequence re-confirmation, and abstention-aware
//! aggregation into one [`GroundingResult`] per
//! [`matcher::CandidateMatch`].
//!
//! See this crate's top-level docs for the full algorithm walkthrough;
//! this module's own docs focus on the aggregation/abstention precedence
//! rule, since that is the one policy decision spread across (rather
//! than owned by) a single function.
//!
//! # Abstention precedence
//!
//! Given the independently-verified outcome of every required evidence
//! clause (and the sequence check, if the pattern declares one):
//!
//! 1. If **any** required clause (or the sequence check) came back
//!    [`EvidenceOutcome::Unavailable`] or [`EvidenceOutcome::Unsupported`]
//!    — genuine uncertainty — the overall result is
//!    [`GroundingStatus::Abstain`], **regardless of** whether some other
//!    clause also came back [`EvidenceOutcome::Failed`].
//! 2. Otherwise, if **any** required clause (or the sequence check) came
//!    back [`EvidenceOutcome::Failed`], the overall result is
//!    [`GroundingStatus::Ungrounded`].
//! 3. Otherwise (every required clause, and the sequence check if
//!    present, came back [`EvidenceOutcome::Verified`]), the overall
//!    result is [`GroundingStatus::Grounded`].
//!
//! Step 1 outranking step 2 is the direct, deliberate consequence of
//! this crate's central instruction: "never upgrade uncertain evidence
//! into a grounded result." A confident [`EvidenceOutcome::Failed`] is
//! not uncertain — it is a real, checked contradiction — but *reporting*
//! it as the headline result (`Ungrounded`) when another clause is
//! simultaneously uncheckable would imply a level of overall confidence
//! the candidate as a whole does not have: "ungrounded" reads as "we
//! checked and it's not real," which is not true if part of the check
//! could not be performed at all. `Abstain` is therefore the honest
//! summary whenever *any* part of the required evidence is uncertain,
//! and the `Failed` clause's own reason is still preserved (see
//! [`crate::status::AbstentionCategory::StructuralMismatch`]) so no
//! information is lost, only the headline status is made appropriately
//! conservative.

use std::collections::HashSet;

use fact_model::{FactRef, Trace};

use dsl::ir::{CompiledPattern, Requiredness};
use dsl::EvidenceRef;

use matcher::CandidateMatch;

use crate::error::GroundingError;
use crate::polarity::{self, Polarity};
use crate::report::{
    AbstainReason, CandidateId, ConfidenceMetadata, EvidenceVerification, GroundingResult,
};
use crate::status::{AbstentionCategory, EvidenceOutcome, GroundingStatus};
use crate::verifier::VerifierRegistry;

/// Grounds [`matcher::CandidateMatch`]es using a configurable
/// [`VerifierRegistry`].
///
/// Prefer reusing one `GroundingEngine` across many candidates from the
/// same pattern library, the same way `matcher::MatchEngine` is meant to
/// be reused across many patterns: building
/// [`VerifierRegistry::with_default_verifiers`] is cheap, but doing it
/// once is still better practice than doing it per call.
pub struct GroundingEngine {
    registry: VerifierRegistry,
}

impl Default for GroundingEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl GroundingEngine {
    /// A `GroundingEngine` using the default verifier registry (every
    /// predicate kind [`dsl::schema::ALL`] currently defines).
    #[must_use]
    pub fn new() -> Self {
        Self {
            registry: VerifierRegistry::with_default_verifiers(),
        }
    }

    /// A `GroundingEngine` using a caller-supplied registry — for
    /// injecting a verifier for a predicate kind not yet built into this
    /// crate (see [`VerifierRegistry::register`]).
    #[must_use]
    pub const fn with_registry(registry: VerifierRegistry) -> Self {
        Self { registry }
    }

    /// Independently ground `candidate` against `trace`, using
    /// `pattern` as the source of truth for evidence declarations,
    /// requiredness, polarity, and the sequence constraint.
    ///
    /// # Errors
    /// See [`GroundingError`]'s own variant docs. In summary: this
    /// returns `Err` only when `candidate`, `trace`, and `pattern` do
    /// not cohere as inputs (wrong trace, wrong pattern, a malformed
    /// pattern, or a genuinely misconfigured verifier registry) —
    /// ordinary uncertainty about the trace's *content* always produces
    /// `Ok(GroundingResult { status: GroundingStatus::Abstain, .. })`
    /// instead, never an `Err` (see this crate's top-level docs,
    /// "Abstention policy").
    pub fn ground(
        &self,
        candidate: &CandidateMatch,
        trace: &Trace,
        pattern: &CompiledPattern,
    ) -> Result<GroundingResult, GroundingError> {
        check_candidate_matches_pattern(candidate, pattern)?;
        check_bindings_resolve(candidate, trace)?;
        let analyzed = polarity::analyze(&pattern.constraint)
            .map_err(|evidence| GroundingError::VerificationFailure { evidence })?;

        let index = matcher::TraceIndex::build(trace);
        let chain = self.verify_all_evidence(trace, &index, pattern, candidate, &analyzed)?;
        let sequence_outcome = verify_sequence(trace, pattern, &chain);
        let same_call_outcome = verify_same_call(trace, pattern, &chain);

        Ok(build_result(
            candidate,
            pattern,
            &chain,
            sequence_outcome,
            same_call_outcome,
        ))
    }

    /// Verify every evidence clause in `pattern`, in declaration order,
    /// producing one [`EvidenceVerification`] per clause.
    fn verify_all_evidence(
        &self,
        trace: &Trace,
        index: &matcher::TraceIndex<'_>,
        pattern: &CompiledPattern,
        candidate: &CandidateMatch,
        analyzed: &[(EvidenceRef, Polarity)],
    ) -> Result<Vec<EvidenceVerification>, GroundingError> {
        let mut chain = Vec::with_capacity(pattern.evidence.len());
        for (i, decl) in pattern.evidence.iter().enumerate() {
            let evidence = EvidenceRef(u32::try_from(i).unwrap_or(u32::MAX));
            let polarity = polarity::polarity_of(analyzed, evidence);
            let cited_fact = candidate.fact_for(evidence);

            let outcome = self.verify_one(
                trace,
                index,
                &decl.predicate.kind,
                decl,
                polarity,
                cited_fact,
            )?;

            chain.push(EvidenceVerification {
                evidence,
                name: decl.name.clone(),
                requiredness: decl.requiredness,
                polarity,
                outcome: outcome.outcome,
                facts: outcome.fact.into_iter().collect(),
                reason: outcome.reason,
                notes: outcome
                    .unresolved_attrs
                    .iter()
                    .map(|a| format!("attribute `{a}` could not be structurally resolved"))
                    .collect(),
            });
        }
        Ok(chain)
    }

    fn verify_one(
        &self,
        trace: &Trace,
        index: &matcher::TraceIndex<'_>,
        kind: &str,
        decl: &dsl::ir::CompiledEvidence,
        polarity: Polarity,
        cited_fact: Option<FactRef>,
    ) -> Result<crate::verifier::VerifierOutcome, GroundingError> {
        match self.registry.get(kind) {
            Some(verifier) => {
                Ok(verifier.verify(trace, index, &decl.predicate, polarity, cited_fact))
            }
            None if self.registry.is_empty() => Err(GroundingError::UnsupportedVerifier {
                kind: kind.to_string(),
            }),
            None => Ok(crate::verifier::VerifierOutcome {
                outcome: EvidenceOutcome::Unsupported,
                fact: None,
                unresolved_attrs: Vec::new(),
                reason: format!(
                    "no verifier is registered for predicate kind `{kind}`; this evidence \
                     clause cannot be independently checked"
                ),
            }),
        }
    }
}

fn check_candidate_matches_pattern(
    candidate: &CandidateMatch,
    pattern: &CompiledPattern,
) -> Result<(), GroundingError> {
    if candidate.pattern_id == pattern.id && candidate.pattern_version == pattern.version {
        Ok(())
    } else {
        Err(GroundingError::IncompleteTrace {
            candidate_pattern_id: candidate.pattern_id.clone(),
            candidate_pattern_version: candidate.pattern_version,
            given_pattern_id: pattern.id.clone(),
            given_pattern_version: pattern.version,
        })
    }
}

fn check_bindings_resolve(candidate: &CandidateMatch, trace: &Trace) -> Result<(), GroundingError> {
    for binding in &candidate.bindings {
        if !fact_resolves(trace, binding.fact) {
            return Err(GroundingError::MissingFacts { fact: binding.fact });
        }
    }
    Ok(())
}

fn fact_resolves(trace: &Trace, fact: FactRef) -> bool {
    match fact {
        FactRef::Call(id) => trace.arena.call(id).is_some(),
        FactRef::StorageChange(id) => trace.arena.storage_change(id).is_some(),
        FactRef::Log(id) => trace.arena.log(id).is_some(),
        FactRef::TokenTransfer(id) => trace.arena.token_transfer(id).is_some(),
    }
}

/// Independently re-confirm the pattern's `sequence:` ordering (if any)
/// using the facts this call's own evidence chain verified — never the
/// candidate's own claimed bindings, for the same independence reason
/// [`crate::verifier`] documents for individual predicates.
///
/// Returns `None` if the pattern declares no sequence.
fn verify_sequence(
    trace: &Trace,
    pattern: &CompiledPattern,
    chain: &[EvidenceVerification],
) -> Option<EvidenceOutcome> {
    let sequence = pattern.sequence.as_ref()?;

    let mut keys = Vec::with_capacity(sequence.steps.len());
    for step in &sequence.steps {
        let Some(entry) = chain.iter().find(|e| e.evidence == *step) else {
            return Some(EvidenceOutcome::Unsupported);
        };
        match entry.outcome {
            EvidenceOutcome::Verified => {
                let Some(&fact) = entry.facts.first() else {
                    // A negatively-referenced, confirmed-absent step
                    // (no fact to order) inside a `sequence:` is a
                    // genuinely ambiguous pattern: there is no concrete
                    // occurrence to attribute a temporal position to.
                    return Some(EvidenceOutcome::Unsupported);
                };
                keys.push(matcher::ordering::order_key(trace, fact));
            }
            EvidenceOutcome::Unavailable => return Some(EvidenceOutcome::Unavailable),
            EvidenceOutcome::Unsupported => return Some(EvidenceOutcome::Unsupported),
            EvidenceOutcome::Failed => return Some(EvidenceOutcome::Failed),
        }
    }

    let mut lower_bound: Option<u32> = None;
    for k in keys.into_iter().flatten() {
        if let Some(bound) = lower_bound {
            if k < bound {
                return Some(EvidenceOutcome::Failed);
            }
        }
        lower_bound = Some(k);
    }
    Some(EvidenceOutcome::Verified)
}

/// Independently re-confirm the pattern's `same_call:` group (if any)
/// using the facts this call's own evidence chain verified — never
/// `matcher`'s own structural binding, mirroring [`verify_sequence`]'s
/// own independence rationale. Every member's producing call is
/// recomputed fresh via [`matcher::identity::producing_call`], the same
/// helper `matcher::engine` uses when structurally building the
/// candidate — reusing it here (rather than re-deriving "the call that
/// produced this fact" a second time) is deliberate: two independently
/// *maintained* notions of "same call" would be the exact kind of
/// matcher/grounding semantic drift this crate's own docs warn against.
///
/// Returns `None` if the pattern declares no `same_call:` group.
fn verify_same_call(
    trace: &Trace,
    pattern: &CompiledPattern,
    chain: &[EvidenceVerification],
) -> Option<EvidenceOutcome> {
    let same_call = pattern.same_call.as_ref()?;

    let mut call_ids = Vec::with_capacity(same_call.members.len());
    for member in &same_call.members {
        let Some(entry) = chain.iter().find(|e| e.evidence == *member) else {
            return Some(EvidenceOutcome::Unsupported);
        };
        match entry.outcome {
            EvidenceOutcome::Verified => {
                let Some(&fact) = entry.facts.first() else {
                    // A negatively-referenced, confirmed-absent member
                    // (no fact to attribute to a call) inside a
                    // `same_call:` group is genuinely ambiguous — there
                    // is no concrete occurrence to correlate.
                    return Some(EvidenceOutcome::Unsupported);
                };
                let Some(call_id) = matcher::identity::producing_call(trace, fact) else {
                    // The cited fact does not resolve against `trace`
                    // at all — checked earlier by
                    // `check_bindings_resolve` for matcher's own
                    // binding, but this entry's fact came from
                    // grounding's own independent re-verification, so
                    // it is re-checked here rather than assumed.
                    return Some(EvidenceOutcome::Unavailable);
                };
                call_ids.push(call_id);
            }
            EvidenceOutcome::Unavailable => return Some(EvidenceOutcome::Unavailable),
            EvidenceOutcome::Unsupported => return Some(EvidenceOutcome::Unsupported),
            EvidenceOutcome::Failed => return Some(EvidenceOutcome::Failed),
        }
    }

    let all_same_call = call_ids.windows(2).all(|pair| pair[0] == pair[1]);
    if all_same_call {
        Some(EvidenceOutcome::Verified)
    } else {
        Some(EvidenceOutcome::Failed)
    }
}

fn build_result(
    candidate: &CandidateMatch,
    pattern: &CompiledPattern,
    chain: &[EvidenceVerification],
    sequence_outcome: Option<EvidenceOutcome>,
    same_call_outcome: Option<EvidenceOutcome>,
) -> GroundingResult {
    let confidence = compute_confidence(pattern, chain, sequence_outcome, same_call_outcome);
    let (status, abstain_reasons) = aggregate_status(chain, sequence_outcome, same_call_outcome);

    let mut verified_evidence = Vec::new();
    let mut failed_evidence = Vec::new();
    let mut unavailable_evidence = Vec::new();
    let mut unsupported_evidence = Vec::new();
    for entry in chain {
        match entry.outcome {
            EvidenceOutcome::Verified => verified_evidence.push(entry.evidence),
            EvidenceOutcome::Failed => failed_evidence.push(entry.evidence),
            EvidenceOutcome::Unavailable => unavailable_evidence.push(entry.evidence),
            EvidenceOutcome::Unsupported => unsupported_evidence.push(entry.evidence),
        }
    }

    let unresolved_attributes = merge_unresolved_attributes(candidate, chain);
    let explanation = explain(pattern, status, chain, sequence_outcome, same_call_outcome);

    GroundingResult {
        pattern_id: pattern.id.clone(),
        pattern_version: pattern.version,
        pattern_family: pattern.family.clone(),
        pattern_severity: pattern.severity,
        candidate_id: CandidateId::compute(candidate),
        status,
        verified_evidence,
        failed_evidence,
        unavailable_evidence,
        unsupported_evidence,
        confidence,
        unresolved_attributes,
        evidence_chain: chain.to_vec(),
        abstain_reasons,
        explanation,
    }
}

fn compute_confidence(
    pattern: &CompiledPattern,
    chain: &[EvidenceVerification],
    sequence_outcome: Option<EvidenceOutcome>,
    same_call_outcome: Option<EvidenceOutcome>,
) -> ConfidenceMetadata {
    let mut required_total = 0usize;
    let mut required_verified = 0usize;
    let mut optional_total = 0usize;
    let mut optional_verified = 0usize;

    for (decl, entry) in pattern.evidence.iter().zip(chain) {
        match decl.requiredness {
            Requiredness::Required => {
                required_total += 1;
                if entry.outcome == EvidenceOutcome::Verified {
                    required_verified += 1;
                }
            }
            Requiredness::Optional => {
                optional_total += 1;
                if entry.outcome == EvidenceOutcome::Verified {
                    optional_verified += 1;
                }
            }
        }
    }

    ConfidenceMetadata {
        required_total,
        required_verified,
        optional_total,
        optional_verified,
        sequence_verified: sequence_outcome.map(|o| o == EvidenceOutcome::Verified),
        same_call_verified: same_call_outcome.map(|o| o == EvidenceOutcome::Verified),
    }
}

/// Apply the abstention precedence rule documented on this module —
/// uncertainty (`Unavailable`/`Unsupported`) on any **required** clause,
/// the sequence check, or the `same_call` check outranks a confident
/// `Failed` on another.
fn aggregate_status(
    chain: &[EvidenceVerification],
    sequence_outcome: Option<EvidenceOutcome>,
    same_call_outcome: Option<EvidenceOutcome>,
) -> (GroundingStatus, Vec<AbstainReason>) {
    let required: Vec<&EvidenceVerification> = chain
        .iter()
        .filter(|e| e.requiredness == Requiredness::Required)
        .collect();

    let mut abstain_reasons = Vec::new();
    let mut any_uncertain = false;
    let mut any_failed = false;

    for entry in &required {
        match entry.outcome {
            EvidenceOutcome::Unavailable => {
                any_uncertain = true;
                abstain_reasons.push(AbstainReason {
                    category: AbstentionCategory::MissingFact,
                    evidence: Some(entry.evidence),
                    detail: entry.reason.clone(),
                });
            }
            EvidenceOutcome::Unsupported => {
                any_uncertain = true;
                abstain_reasons.push(AbstainReason {
                    category: AbstentionCategory::UnresolvableLayout,
                    evidence: Some(entry.evidence),
                    detail: entry.reason.clone(),
                });
            }
            EvidenceOutcome::Failed => {
                any_failed = true;
                abstain_reasons.push(AbstainReason {
                    category: AbstentionCategory::StructuralMismatch,
                    evidence: Some(entry.evidence),
                    detail: entry.reason.clone(),
                });
            }
            EvidenceOutcome::Verified => {}
        }
    }

    match sequence_outcome {
        Some(EvidenceOutcome::Unavailable | EvidenceOutcome::Unsupported) => {
            any_uncertain = true;
            abstain_reasons.push(AbstainReason {
                category: AbstentionCategory::AmbiguousAttribution,
                evidence: None,
                detail: "the pattern's sequence constraint could not be independently \
                         re-confirmed"
                    .to_string(),
            });
        }
        Some(EvidenceOutcome::Failed) => {
            any_failed = true;
            abstain_reasons.push(AbstainReason {
                category: AbstentionCategory::AmbiguousAttribution,
                evidence: None,
                detail: "the pattern's sequence constraint was independently re-checked and \
                         does not hold for these facts' actual execution order"
                    .to_string(),
            });
        }
        Some(EvidenceOutcome::Verified) | None => {}
    }

    match same_call_outcome {
        Some(EvidenceOutcome::Unavailable | EvidenceOutcome::Unsupported) => {
            any_uncertain = true;
            abstain_reasons.push(AbstainReason {
                category: AbstentionCategory::AmbiguousAttribution,
                evidence: None,
                detail: "the pattern's same_call constraint could not be independently \
                         re-confirmed"
                    .to_string(),
            });
        }
        Some(EvidenceOutcome::Failed) => {
            any_failed = true;
            abstain_reasons.push(AbstainReason {
                category: AbstentionCategory::AmbiguousAttribution,
                evidence: None,
                detail: "the pattern's same_call constraint was independently re-checked and \
                         the cited facts do not share a CallId"
                    .to_string(),
            });
        }
        Some(EvidenceOutcome::Verified) | None => {}
    }

    let status = if any_uncertain {
        GroundingStatus::Abstain
    } else if any_failed {
        GroundingStatus::Ungrounded
    } else {
        GroundingStatus::Grounded
    };

    if status == GroundingStatus::Abstain {
        (status, abstain_reasons)
    } else {
        // `Grounded`/`Ungrounded` are confident results; per this
        // crate's public contract (`GroundingResult::abstain_reasons`
        // "always empty otherwise"), abstain reasons are only carried
        // when the status actually is `Abstain`.
        (status, Vec::new())
    }
}

fn merge_unresolved_attributes(
    candidate: &CandidateMatch,
    chain: &[EvidenceVerification],
) -> Vec<matcher::UnresolvedAttribute> {
    let mut seen: HashSet<(EvidenceRef, String)> = HashSet::new();
    let mut merged = Vec::new();

    for attr in &candidate.metadata.unresolved_attributes {
        if seen.insert((attr.evidence, attr.attribute.clone())) {
            merged.push(attr.clone());
        }
    }
    for entry in chain {
        if entry.outcome != EvidenceOutcome::Unsupported {
            continue;
        }
        for note in &entry.notes {
            // Notes are formatted as "attribute `X` could not be
            // structurally resolved" — extract `X` deterministically
            // rather than re-deriving it from `VerifierOutcome`, which
            // is no longer in scope here.
            if let Some(name) = note
                .strip_prefix("attribute `")
                .and_then(|s| s.split('`').next())
            {
                let key = (entry.evidence, name.to_string());
                if seen.insert(key) {
                    merged.push(matcher::UnresolvedAttribute {
                        evidence: entry.evidence,
                        attribute: name.to_string(),
                        reason: entry.reason.clone(),
                    });
                }
            }
        }
    }
    merged
}

fn explain(
    pattern: &CompiledPattern,
    status: GroundingStatus,
    chain: &[EvidenceVerification],
    sequence_outcome: Option<EvidenceOutcome>,
    same_call_outcome: Option<EvidenceOutcome>,
) -> String {
    let required_total = chain
        .iter()
        .filter(|e| e.requiredness == Requiredness::Required)
        .count();
    let required_verified = chain
        .iter()
        .filter(|e| {
            e.requiredness == Requiredness::Required && e.outcome == EvidenceOutcome::Verified
        })
        .count();
    let sequence_note = match sequence_outcome {
        Some(EvidenceOutcome::Verified) => " sequence ordering confirmed.",
        Some(_) => " sequence ordering could not be confirmed.",
        None => "",
    };
    let same_call_note = match same_call_outcome {
        Some(EvidenceOutcome::Verified) => " same_call correlation confirmed.",
        Some(_) => " same_call correlation could not be confirmed.",
        None => "",
    };
    format!(
        "pattern `{}` v{}: {status} ({required_verified}/{required_total} required evidence \
         clauses verified).{sequence_note}{same_call_note}",
        pattern.id, pattern.version.0
    )
}

/// Ground `candidate` against `trace` using the default verifier
/// registry — a convenience wrapper around
/// <code>[GroundingEngine::new].[ground](GroundingEngine::ground)</code>. Build
/// a [`GroundingEngine`] directly (and reuse it) when grounding many
/// candidates.
///
/// # Errors
/// See [`GroundingEngine::ground`].
pub fn ground(
    candidate: &CandidateMatch,
    trace: &Trace,
    pattern: &CompiledPattern,
) -> Result<GroundingResult, GroundingError> {
    GroundingEngine::new().ground(candidate, trace, pattern)
}

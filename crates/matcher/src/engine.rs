//! Candidate generation: orchestrates predicate, constraint, and
//! sequence evaluation into the final [`CandidateMatch`] list for one
//! pattern against one trace.
//!
//! See this crate's top-level docs, "Candidate generation," for the
//! algorithm's full rationale. Summary: evaluate every evidence clause's
//! predicate once (§predicate), pick an *anchor* evidence clause (the
//! pattern's declared entry point — a `sequence:`'s first step if one
//! exists, otherwise the first positively-referenced clause in the
//! constraint), then emit one [`CandidateMatch`] per anchor occurrence
//! that can be extended to a full, constraint-satisfying, sequence-
//! consistent binding. This bounds the candidate count to the number of
//! real occurrences of the pattern's trigger condition, rather than
//! either capping at one match per trace (losing real repeated
//! occurrences, e.g. two independent reentrant calls) or enumerating
//! every combinatorial choice across every `OR` branch and every
//! evidence clause's every match (which would produce a combinatorial
//! explosion of matches that all describe the same underlying
//! occurrence).

use std::collections::{HashMap, HashSet};

use fact_model::{CallId, Trace};

use dsl::ir::{CompiledConstraint, CompiledPattern, CompiledSameCall};
use dsl::EvidenceRef;

use crate::binding::{CandidateMatch, EvidenceBinding, MatchMetadata, UnresolvedAttribute};
use crate::constraint;
use crate::error::MatcherError;
use crate::identity;
use crate::index::TraceIndex;
use crate::predicate::{self, Binding};
use crate::sequence;

/// Find every candidate match of `pattern` against `trace`, reusing an
/// already-built [`TraceIndex`] (see [`crate::MatchEngine`] for the
/// caching entry point most callers should prefer over calling this
/// directly).
///
/// # Errors
/// Returns [`MatcherError::DanglingEvidenceRef`] if `pattern` contains
/// an `EvidenceRef` outside its own `evidence` list — see that error
/// variant's docs for when this can happen (never for a pattern produced
/// by `dsl`'s own compiler).
pub fn find_candidates(
    trace: &Trace,
    index: &TraceIndex<'_>,
    pattern: &CompiledPattern,
) -> Result<Vec<CandidateMatch>, MatcherError> {
    check_in_bounds(pattern)?;

    // Phase 1: evaluate every evidence clause's predicate exactly once.
    // This is the "avoid repeated scans over traces where practical"
    // requirement in practice — every subsequent phase reads from this
    // `Vec`, never re-scanning `trace.arena`.
    let evidence_matches: Vec<Vec<Binding>> = pattern
        .evidence
        .iter()
        .map(|ev| predicate::evaluate(trace, index, &ev.predicate))
        .collect();

    let Some(anchor_idx) = anchor_index(pattern) else {
        // No positively-referenced evidence anywhere in the constraint
        // (e.g. a pathological `constraint: NOT(a)` pattern whose only
        // positive signal is an absence). There is no per-occurrence
        // "trigger" to iterate, so this pattern either matches the
        // trace globally, exactly once, or not at all.
        return Ok(global_candidate(pattern, &evidence_matches)
            .into_iter()
            .collect());
    };

    let mut candidates = Vec::new();
    for anchor_binding in &evidence_matches[anchor_idx] {
        if let Some(candidate) = try_build_candidate(
            trace,
            pattern,
            &evidence_matches,
            anchor_idx,
            anchor_binding,
        ) {
            candidates.push(candidate);
        }
    }
    Ok(candidates)
}

/// Validate every `EvidenceRef` `pattern`'s constraint and sequence
/// reference actually indexes into `pattern.evidence` — see
/// [`MatcherError::DanglingEvidenceRef`].
fn check_in_bounds(pattern: &CompiledPattern) -> Result<(), MatcherError> {
    let len = pattern.evidence.len();
    let mut check = |r: EvidenceRef| -> Result<(), MatcherError> {
        if r.index() < len {
            Ok(())
        } else {
            Err(MatcherError::DanglingEvidenceRef {
                pattern: pattern.id.clone(),
                evidence_ref: r,
            })
        }
    };
    check_constraint_refs(&pattern.constraint, &mut check)?;
    if let Some(seq) = &pattern.sequence {
        for step in &seq.steps {
            check(*step)?;
        }
    }
    if let Some(same_call) = &pattern.same_call {
        for member in &same_call.members {
            check(*member)?;
        }
    }
    Ok(())
}

fn check_constraint_refs(
    constraint: &CompiledConstraint,
    check: &mut impl FnMut(EvidenceRef) -> Result<(), MatcherError>,
) -> Result<(), MatcherError> {
    match constraint {
        CompiledConstraint::Evidence(r) => check(*r),
        CompiledConstraint::And(items) | CompiledConstraint::Or(items) => items
            .iter()
            .try_for_each(|item| check_constraint_refs(item, check)),
        CompiledConstraint::Not(inner) => check_constraint_refs(inner, check),
    }
}

/// The pattern's anchor evidence index: a `sequence:`'s first step if
/// one exists (keeping sequence-consistent iteration below well-
/// defined), otherwise the first positively-referenced evidence clause
/// in the constraint tree (declaration order, since
/// `constraint::positive_leaves` walks the tree depth-first in the
/// order the pattern author wrote it).
fn anchor_index(pattern: &CompiledPattern) -> Option<usize> {
    if let Some(seq) = &pattern.sequence {
        return seq.steps.first().map(|r| r.index());
    }
    constraint::positive_leaves(&pattern.constraint)
        .into_iter()
        .map(dsl::EvidenceRef::index)
        .next()
}

/// Attempt to build one full [`CandidateMatch`] anchored at
/// `anchor_binding` (a specific occurrence of evidence clause
/// `anchor_idx`).
fn try_build_candidate(
    trace: &Trace,
    pattern: &CompiledPattern,
    evidence_matches: &[Vec<Binding>],
    anchor_idx: usize,
    anchor_binding: &Binding,
) -> Option<CandidateMatch> {
    // Sequence-consistent bindings, if the pattern declares a sequence.
    // `sequence::resolve` re-derives the same assignment regardless of
    // which of `anchor_binding`'s sibling occurrences we started from,
    // *except* it always starts the running lower bound fresh at the
    // first step — so to anchor specifically at `anchor_binding` (not
    // just "some" first-step occurrence), the first step's candidate
    // list is narrowed to exactly `anchor_binding` for this attempt.
    let sequence_bindings: Option<HashMap<usize, Binding>> = pattern.sequence.as_ref().map_or_else(
        || Some(HashMap::new()),
        |seq| {
            let mut narrowed = evidence_matches.to_vec();
            narrowed[anchor_idx] = vec![anchor_binding.clone()];
            sequence::resolve(seq, &narrowed).map(|resolved| {
                seq.steps
                    .iter()
                    .zip(resolved)
                    .map(|(step, binding)| (step.index(), binding))
                    .collect()
            })
        },
    );
    let sequence_bindings = sequence_bindings?;

    let has_match = |evidence: EvidenceRef| -> bool {
        let idx = evidence.index();
        idx == anchor_idx
            || sequence_bindings.contains_key(&idx)
            || !evidence_matches[idx].is_empty()
    };
    if !constraint::holds(&pattern.constraint, &has_match) {
        return None;
    }

    // `same_call:` resolution, if the pattern declares one: every
    // member evidence clause must bind to a fact produced by the
    // identical `CallId`, not merely to *any* match — this is the one
    // place candidate-building must look beyond "does a match exist"
    // (see `resolve_same_call`'s own docs).
    let same_call_bindings: HashMap<usize, Binding> = match &pattern.same_call {
        Some(same_call) => resolve_same_call(
            trace,
            same_call,
            evidence_matches,
            anchor_idx,
            anchor_binding,
            &sequence_bindings,
        )?,
        None => HashMap::new(),
    };

    let mut referenced: HashSet<usize> = constraint::positive_leaves(&pattern.constraint)
        .into_iter()
        .map(dsl::EvidenceRef::index)
        .collect();
    referenced.insert(anchor_idx);
    referenced.extend(sequence_bindings.keys().copied());
    referenced.extend(same_call_bindings.keys().copied());

    let mut bindings = Vec::new();
    let mut unresolved_attributes = Vec::new();
    for idx in referenced {
        let evidence_ref = EvidenceRef(u32::try_from(idx).unwrap_or(u32::MAX));
        let chosen = if idx == anchor_idx {
            anchor_binding.clone()
        } else if let Some(seq_binding) = sequence_bindings.get(&idx) {
            seq_binding.clone()
        } else if let Some(sc_binding) = same_call_bindings.get(&idx) {
            sc_binding.clone()
        } else if let Some(first) = evidence_matches[idx].first() {
            // Referenced positively by the constraint but not part of
            // the sequence or a same_call group: any match will do, so
            // pick the deterministic (already order-sorted) first one.
            first.clone()
        } else {
            // This clause is a positively-referenced leaf somewhere in
            // the constraint tree (e.g. the untaken side of an `OR`),
            // but has no match of its own. Since `constraint::holds`
            // already confirmed the pattern is satisfied overall
            // (checked once, above, via `has_match`), this specific
            // leaf simply wasn't the branch that made it true — there
            // is nothing to bind here, and that is not a failure.
            continue;
        };
        for attr in &chosen.unresolved_attrs {
            unresolved_attributes.push(UnresolvedAttribute {
                evidence: evidence_ref,
                attribute: (*attr).to_string(),
                reason: "this attribute describes a semantic classification with no \
                         corresponding structural fact in fact-model; matcher did not filter \
                         on it"
                    .to_string(),
            });
        }
        bindings.push(EvidenceBinding {
            evidence: evidence_ref,
            fact: chosen.fact,
        });
    }
    bindings.sort_by_key(|b| b.evidence.index());
    unresolved_attributes.sort_by_key(|u| u.evidence.index());

    Some(CandidateMatch {
        pattern_id: pattern.id.clone(),
        pattern_version: pattern.version,
        pattern_family: pattern.family.clone(),
        pattern_severity: pattern.severity,
        bindings,
        metadata: MatchMetadata {
            unresolved_attributes,
            sequence_checked: pattern.sequence.is_some(),
        },
    })
}

/// Resolve a pattern's `same_call:` group for one candidate-building
/// attempt, or fail the attempt (`None`) if the group cannot be
/// satisfied.
///
/// Every member evidence clause already fixed by the anchor or by
/// `sequence_bindings` establishes the group's target [`CallId`] (and
/// every such member must already agree, or the attempt fails
/// immediately — two clauses `sequence:`/anchor already pinned to
/// different calls can never be reconciled by picking a *different*
/// occurrence of either). If no member is fixed yet, the target is
/// derived from the lowest-indexed member's first (deterministic)
/// match, mirroring the "any match will do" convention
/// [`try_build_candidate`] already uses elsewhere. Every remaining
/// member is then resolved to the first of *its own* matches whose
/// producing call equals the target — not simply its first match
/// overall, which is exactly the gap `same_call:` exists to close (see
/// [`crate::identity`] and this crate's top-level docs).
fn resolve_same_call(
    trace: &Trace,
    same_call: &CompiledSameCall,
    evidence_matches: &[Vec<Binding>],
    anchor_idx: usize,
    anchor_binding: &Binding,
    sequence_bindings: &HashMap<usize, Binding>,
) -> Option<HashMap<usize, Binding>> {
    let member_indices: Vec<usize> = same_call.members.iter().map(|r| r.index()).collect();

    let mut resolved: HashMap<usize, Binding> = HashMap::new();
    for &idx in &member_indices {
        if idx == anchor_idx {
            resolved.insert(idx, anchor_binding.clone());
        } else if let Some(seq_binding) = sequence_bindings.get(&idx) {
            resolved.insert(idx, seq_binding.clone());
        }
    }

    let mut target: Option<CallId> = None;
    for binding in resolved.values() {
        let call_id = identity::producing_call(trace, binding.fact)?;
        match target {
            None => target = Some(call_id),
            Some(existing) if existing != call_id => return None,
            Some(_) => {}
        }
    }

    let target = if let Some(t) = target {
        t
    } else {
        let first_idx = *member_indices.iter().min()?;
        let first_binding = evidence_matches[first_idx].first()?;
        let call_id = identity::producing_call(trace, first_binding.fact)?;
        resolved.insert(first_idx, first_binding.clone());
        call_id
    };

    for &idx in &member_indices {
        if resolved.contains_key(&idx) {
            continue;
        }
        let chosen = evidence_matches[idx]
            .iter()
            .find(|b| identity::producing_call(trace, b.fact) == Some(target))?;
        resolved.insert(idx, chosen.clone());
    }

    Some(resolved)
}

/// The no-anchor fallback: a pattern whose constraint has no positively
/// referenced evidence clause at all (every reference is `NOT`-negated).
/// Such a pattern's truth value does not depend on any specific fact
/// occurrence, so it is checked once, globally, rather than once per
/// occurrence of anything.
fn global_candidate(
    pattern: &CompiledPattern,
    evidence_matches: &[Vec<Binding>],
) -> Option<CandidateMatch> {
    let has_match =
        |evidence: EvidenceRef| -> bool { !evidence_matches[evidence.index()].is_empty() };
    if !constraint::holds(&pattern.constraint, &has_match) {
        return None;
    }
    // No positive leaves means no bindings to record — an all-negated
    // constraint's satisfaction is witnessed by absence, not presence,
    // and there is no fact to cite for an absence (see
    // `constraint::positive_leaves`'s own docs).
    Some(CandidateMatch {
        pattern_id: pattern.id.clone(),
        pattern_version: pattern.version,
        pattern_family: pattern.family.clone(),
        pattern_severity: pattern.severity,
        bindings: Vec::new(),
        metadata: MatchMetadata {
            unresolved_attributes: Vec::new(),
            sequence_checked: pattern.sequence.is_some(),
        },
    })
}

#[cfg(test)]
mod tests {
    use fact_model::{
        Address, BlockContext, BlockNumber, Call, CallDepth, CallId, CallKind, ChainId,
        FactArenaBuilder, Gas, Nonce, StorageChange, StorageSlot, Timestamp, Transaction, TxHash,
        TxStatus, Wei, Word,
    };

    use super::*;
    use crate::find_candidate_matches;

    fn addr(byte: u8) -> Address {
        Address::new([byte; 20])
    }

    fn hash(byte: u8) -> TxHash {
        TxHash(Word::new([byte; 32]))
    }

    fn make_call(
        builder: &FactArenaBuilder,
        parent: Option<CallId>,
        depth: u16,
        from: Address,
        to: Address,
    ) -> Call {
        Call::new(
            builder.next_call_id(),
            parent,
            CallKind::Call,
            CallDepth(depth),
            from,
            Some(to),
            Wei::ZERO,
            Gas(100_000),
            Gas(50_000),
            true,
        )
        .unwrap()
    }

    fn build_trace(arena: fact_model::FactArena, from: Address) -> Trace {
        let tx = Transaction::new(
            hash(1),
            from,
            Some(addr(2)),
            Wei::ZERO,
            Nonce(0),
            Gas(21_000),
            TxStatus::Success,
        );
        let block = BlockContext::new(BlockNumber(1), Timestamp(1), ChainId(1), None);
        let metadata = fact_model::TraceMetadata::new(
            hash(1),
            ChainId(1),
            BlockNumber(1),
            fact_model::TraceSource::ArchiveNodeRpc {
                endpoint_label: "test".to_string(),
            },
        );
        Trace::new(metadata, block, tx, arena).unwrap()
    }

    const REENTRANCY_PATTERN: &str = r"
        pattern classic_reentrancy version 1 {
            family: Reentrancy
            severity: Critical
            evidence {
                required vulnerable_call: call(kind: External, reentrant: true)
                required state_write: storage(changed: true)
            }
            constraint: AND(vulnerable_call, state_write)
        }
    ";

    #[test]
    fn single_reentrant_occurrence_produces_one_candidate() {
        let attacker = addr(1);
        let victim = addr(9);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, attacker, victim);
        let root_id = builder.add_call(root);
        let mid = make_call(&builder, Some(root_id), 1, victim, addr(5));
        let mid_id = builder.add_call(mid);
        let reentrant = make_call(&builder, Some(mid_id), 2, addr(5), victim);
        builder.add_call(reentrant);
        builder.add_storage_change(StorageChange::new(
            root_id,
            StorageSlot::new(victim, Word::ZERO),
            Word::ZERO,
            Word::new([1; 32]),
        ));
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, attacker);

        let pattern = dsl::compile_str(REENTRANCY_PATTERN).unwrap();
        let candidates = find_candidate_matches(&trace, &pattern).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].pattern_id.0, "classic_reentrancy");
        assert_eq!(candidates[0].bindings.len(), 2);
    }

    #[test]
    fn two_independent_reentrant_calls_produce_two_candidates() {
        let attacker = addr(1);
        let victim_a = addr(9);
        let victim_b = addr(8);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, attacker, victim_a);
        let root_id = builder.add_call(root);

        // First reentrant chain: root -> mid_a -> back into victim_a.
        let mid_a = make_call(&builder, Some(root_id), 1, victim_a, addr(5));
        let mid_a_id = builder.add_call(mid_a);
        let reentrant_a = make_call(&builder, Some(mid_a_id), 2, addr(5), victim_a);
        builder.add_call(reentrant_a);
        builder.add_storage_change(StorageChange::new(
            root_id,
            StorageSlot::new(victim_a, Word::ZERO),
            Word::ZERO,
            Word::new([1; 32]),
        ));

        // Second, independent reentrant chain rooted at victim_b, hung
        // off the same top-level root.
        let entry_b = make_call(&builder, Some(root_id), 1, attacker, victim_b);
        let entry_b_id = builder.add_call(entry_b);
        let second_mid = make_call(&builder, Some(entry_b_id), 2, victim_b, addr(6));
        let second_mid_id = builder.add_call(second_mid);
        let reentrant_b = make_call(&builder, Some(second_mid_id), 3, addr(6), victim_b);
        builder.add_call(reentrant_b);
        builder.add_storage_change(StorageChange::new(
            entry_b_id,
            StorageSlot::new(victim_b, Word::ZERO),
            Word::ZERO,
            Word::new([2; 32]),
        ));

        let arena = builder.build().unwrap();
        let trace = build_trace(arena, attacker);

        let pattern = dsl::compile_str(REENTRANCY_PATTERN).unwrap();
        let candidates = find_candidate_matches(&trace, &pattern).unwrap();
        assert_eq!(candidates.len(), 2);
    }

    #[test]
    fn no_reentrant_call_produces_no_candidates() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, attacker, addr(2));
        builder.add_call(root);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, attacker);

        let pattern = dsl::compile_str(REENTRANCY_PATTERN).unwrap();
        let candidates = find_candidate_matches(&trace, &pattern).unwrap();
        assert!(candidates.is_empty());
    }

    const OR_PATTERN: &str = r"
        pattern either_call_kind version 1 {
            family: Other
            severity: Low
            evidence {
                required static_call: call(kind: StaticCall)
                required delegate_call: call(kind: Delegate)
            }
            constraint: OR(static_call, delegate_call)
        }
    ";

    #[test]
    fn or_branch_with_no_match_does_not_abort_candidate() {
        // Only a StaticCall exists; `delegate_call` (the other OR
        // branch) has zero matches. This must NOT prevent a candidate
        // from being produced for the branch that *does* match.
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = Call::new(
            builder.next_call_id(),
            None,
            CallKind::StaticCall,
            CallDepth(0),
            attacker,
            Some(addr(2)),
            Wei::ZERO,
            Gas(1),
            Gas(1),
            true,
        )
        .unwrap();
        builder.add_call(root);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, attacker);

        let pattern = dsl::compile_str(OR_PATTERN).unwrap();
        let candidates = find_candidate_matches(&trace, &pattern).unwrap();
        assert_eq!(candidates.len(), 1);
        // Only the matching branch (static_call) should have a binding;
        // delegate_call has no fact to cite.
        assert_eq!(candidates[0].bindings.len(), 1);
    }

    const SEQUENCE_PATTERN: &str = r"
        pattern ordered_pair version 1 {
            family: Other
            severity: Low
            evidence {
                required first: call(kind: External)
                required second: storage(changed: true)
            }
            sequence: [first, second]
        }
    ";

    #[test]
    fn sequence_pattern_requires_order() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, attacker, addr(2));
        let root_id = builder.add_call(root);
        // Storage change happens "during" the root call (same anchor
        // call), which this crate's order approximation treats as
        // compatible (see `crate::ordering`'s own docs).
        builder.add_storage_change(StorageChange::new(
            root_id,
            StorageSlot::new(addr(2), Word::ZERO),
            Word::ZERO,
            Word::new([1; 32]),
        ));
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, attacker);

        let pattern = dsl::compile_str(SEQUENCE_PATTERN).unwrap();
        let candidates = find_candidate_matches(&trace, &pattern).unwrap();
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].metadata.sequence_checked);
    }

    const SAME_CALL_PATTERN: &str = r"
        pattern write_from_matched_call version 1 {
            family: Other
            severity: Low
            evidence {
                required entry: call(kind: External)
                required write: storage(changed: true)
            }
            constraint: AND(entry, write)
            same_call: [entry, write]
        }
    ";

    #[test]
    #[allow(clippy::similar_names)]
    fn same_call_attributes_each_anchor_occurrence_to_its_own_write() {
        // Two independent External calls, each producing its own
        // storage write. Without `same_call:`, `write` would be bound
        // to whichever storage change sorts first for *every* anchor
        // occurrence (see `try_build_candidate`'s "any match will do"
        // fallback); with it, each candidate must cite the write that
        // its own anchor call actually produced.
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let call_a = make_call(&builder, None, 0, attacker, addr(2));
        let call_a_id = builder.add_call(call_a);
        let call_b = make_call(&builder, Some(call_a_id), 1, addr(2), addr(3));
        let call_b_id = builder.add_call(call_b);
        let write_a = builder.add_storage_change(StorageChange::new(
            call_a_id,
            StorageSlot::new(addr(2), Word::ZERO),
            Word::ZERO,
            Word::new([1; 32]),
        ));
        let write_b = builder.add_storage_change(StorageChange::new(
            call_b_id,
            StorageSlot::new(addr(3), Word::ZERO),
            Word::ZERO,
            Word::new([2; 32]),
        ));
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, attacker);

        let pattern = dsl::compile_str(SAME_CALL_PATTERN).unwrap();
        let mut candidates = find_candidate_matches(&trace, &pattern).unwrap();
        candidates.sort_by_key(|c| c.fact_for(EvidenceRef(0)));
        assert_eq!(candidates.len(), 2);

        let entry = EvidenceRef(0);
        let write = EvidenceRef(1);
        for candidate in &candidates {
            let entry_fact = candidate.fact_for(entry).unwrap();
            let write_fact = candidate.fact_for(write).unwrap();
            assert_eq!(
                identity::producing_call(&trace, entry_fact),
                identity::producing_call(&trace, write_fact)
            );
        }
        // Confirm the two candidates are genuinely distinct occurrences,
        // not the same call matched twice.
        let cited_writes: HashSet<_> = candidates
            .iter()
            .map(|c| c.fact_for(write).unwrap())
            .collect();
        assert_eq!(
            cited_writes,
            HashSet::from([
                fact_model::FactRef::StorageChange(write_a),
                fact_model::FactRef::StorageChange(write_b),
            ])
        );
    }

    #[test]
    fn same_call_rejects_a_write_from_an_unrelated_call() {
        // Only the anchor call itself never writes storage; the only
        // storage write in the trace belongs to a different call. A
        // plain AND(entry, write) constraint would still match (both
        // clauses are independently satisfiable), but `same_call:`
        // must reject it because the two facts do not share a `CallId`.
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let entry_call = make_call(&builder, None, 0, attacker, addr(2));
        let entry_id = builder.add_call(entry_call);
        // A second, unrelated call (not `External`, so it is never
        // itself matched by the `entry` clause) that performs the
        // trace's only storage write.
        let other_call = Call::new(
            builder.next_call_id(),
            Some(entry_id),
            CallKind::StaticCall,
            CallDepth(1),
            addr(2),
            Some(addr(4)),
            Wei::ZERO,
            Gas(1),
            Gas(1),
            true,
        )
        .unwrap();
        let other_id = builder.add_call(other_call);
        builder.add_storage_change(StorageChange::new(
            other_id,
            StorageSlot::new(addr(4), Word::ZERO),
            Word::ZERO,
            Word::new([1; 32]),
        ));
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, attacker);

        let pattern = dsl::compile_str(SAME_CALL_PATTERN).unwrap();
        let candidates = find_candidate_matches(&trace, &pattern).unwrap();
        assert!(candidates.is_empty());
    }

    const NOT_ONLY_PATTERN: &str = r"
        pattern absence_only version 1 {
            family: Other
            severity: Low
            evidence {
                required forbidden: call(kind: Delegate)
            }
            constraint: NOT(forbidden)
        }
    ";

    #[test]
    fn all_negated_constraint_uses_global_fallback() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, attacker, addr(2)); // plain Call, not Delegate
        builder.add_call(root);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, attacker);

        let pattern = dsl::compile_str(NOT_ONLY_PATTERN).unwrap();
        let candidates = find_candidate_matches(&trace, &pattern).unwrap();
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].bindings.is_empty());
    }

    #[test]
    fn all_negated_constraint_with_forbidden_present_matches_nothing() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = Call::new(
            builder.next_call_id(),
            None,
            CallKind::DelegateCall,
            CallDepth(0),
            attacker,
            Some(addr(2)),
            Wei::ZERO,
            Gas(1),
            Gas(1),
            true,
        )
        .unwrap();
        builder.add_call(root);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, attacker);

        let pattern = dsl::compile_str(NOT_ONLY_PATTERN).unwrap();
        let candidates = find_candidate_matches(&trace, &pattern).unwrap();
        assert!(candidates.is_empty());
    }

    #[test]
    fn unresolved_attributes_propagate_to_candidate_metadata() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, attacker, addr(2));
        let root_id = builder.add_call(root);
        builder.add_storage_change(StorageChange::new(
            root_id,
            StorageSlot::new(addr(2), Word::ZERO),
            Word::ZERO,
            Word::new([1; 32]),
        ));
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, attacker);

        let src = r"
            pattern oracle_check version 1 {
                family: OracleManipulation
                severity: High
                evidence {
                    required price_read: storage(changed: true, role: PriceOracle)
                }
            }
        ";
        let pattern = dsl::compile_str(src).unwrap();
        let candidates = find_candidate_matches(&trace, &pattern).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].metadata.unresolved_attributes.len(), 1);
        assert_eq!(
            candidates[0].metadata.unresolved_attributes[0].attribute,
            "role"
        );
    }

    #[test]
    fn dangling_evidence_ref_is_reported_as_error() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, attacker, addr(2));
        builder.add_call(root);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, attacker);

        let good_pattern = dsl::compile_str(REENTRANCY_PATTERN).unwrap();
        // Hand-corrupt an otherwise-valid compiled pattern to reference
        // an evidence index that does not exist, simulating a
        // `CompiledPattern` not produced by `dsl`'s own compiler (see
        // `MatcherError::DanglingEvidenceRef`'s own docs).
        let mut bad_pattern = good_pattern;
        bad_pattern.constraint = CompiledConstraint::And(vec![
            CompiledConstraint::Evidence(EvidenceRef(0)),
            CompiledConstraint::Evidence(EvidenceRef(99)),
        ]);

        let result = find_candidate_matches(&trace, &bad_pattern);
        assert!(matches!(
            result,
            Err(MatcherError::DanglingEvidenceRef { .. })
        ));
    }
}

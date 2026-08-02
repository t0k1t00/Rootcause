//! Sequence/temporal evaluation: given each evidence clause's candidate
//! [`Binding`]s, find one binding per `sequence:` step such that their
//! approximate execution order is non-decreasing, if such an assignment
//! exists.
//!
//! # Algorithm
//! Process steps left to right. For step `i`, choose the
//! *earliest-ordered* binding of that step's evidence clause whose order
//! is `>=` the order chosen for step `i - 1` (the first step has no
//! lower bound). If no such binding exists, the whole sequence fails —
//! for *this* choice of predecessor bindings. This greedy rule is
//! correct (not merely a heuristic) by a standard exchange argument:
//! choosing the earliest binding that satisfies the lower bound at each
//! step never makes a later step harder to satisfy than choosing any
//! later-ordered valid binding would have, since every later step only
//! ever imposes a `>=` lower bound derived from the *current* step's
//! choice — picking the smallest possible value for that bound is always
//! at least as permissive for what follows.
//!
//! A step whose order is `None` (see [`crate::ordering`] — currently
//! only the `transaction` predicate's synthetic binding) is treated as
//! compatible with any position: it neither constrains the running lower
//! bound nor is constrained by it.
//!
//! # Complexity
//! `O(steps × average bindings per step)`: each step does a linear scan
//! of its evidence clause's bindings (already sorted by
//! [`crate::predicate::evaluate`]) to find the first one `>=` the
//! running bound. A binary search would improve this to `O(steps × log
//! bindings)`, noted in this crate's top-level docs as an optimization
//! left for when profiling shows it matters — sequence step counts and
//! per-clause match counts are both small in practice (a handful of
//! steps, rarely more than a few dozen matches for a single evidence
//! clause in one transaction's trace).

use dsl::ir::CompiledSequence;

use crate::predicate::Binding;

/// Attempt to resolve `sequence` against `evidence_matches` (indexed by
/// [`dsl::ir::EvidenceRef::index`]), returning one [`Binding`] per step in
/// declaration order if a non-decreasing assignment exists.
///
/// `within_seconds`, if present on `sequence`, is intentionally not
/// enforced here — see this crate's top-level docs, "Why
/// `within_seconds` is not enforced," for the full rationale (a single
/// `fact_model::Trace` carries exactly one block timestamp, so every
/// intra-trace step pair has a wall-clock delta of zero by construction,
/// making the constraint trivially satisfied for any trace this crate
/// can be handed).
#[must_use]
pub fn resolve(
    sequence: &CompiledSequence,
    evidence_matches: &[Vec<Binding>],
) -> Option<Vec<Binding>> {
    let mut resolved = Vec::with_capacity(sequence.steps.len());
    let mut lower_bound: Option<u32> = None;

    for step in &sequence.steps {
        let candidates = evidence_matches.get(step.index())?;
        let chosen = candidates.iter().find(|b| lower_bound.permits(b.order))?;
        if let Some(order) = chosen.order {
            lower_bound = Some(order);
        }
        resolved.push(chosen.clone());
    }

    Some(resolved)
}

/// Small local extension trait spelling `lower_bound.is_none() ||
/// lower_bound <= candidate` at each call site as one readable method,
/// without reaching for the standard library's `Option::is_none_or`
/// (stabilized after this workspace's pinned `rust-toolchain.toml`
/// version — see `.rust-toolchain.toml`/ADR-0002 — so it is not
/// available here).
trait LowerBoundExt {
    fn permits(self, candidate: Option<u32>) -> bool;
}

impl LowerBoundExt for Option<u32> {
    fn permits(self, candidate: Option<u32>) -> bool {
        match (self, candidate) {
            (None, _) | (Some(_), None) => true,
            (Some(bound), Some(value)) => bound <= value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fact_model::{CallDepth, CallId, CallKind, FactArenaBuilder, FactRef, Gas, Wei};

    /// `CallId::from_index` is `pub(crate)` to `fact-model` (by design —
    /// see that type's own docs), so test code outside `fact-model`
    /// cannot manufacture one directly. This builds one legitimate call
    /// and returns its assigned ID, for tests where only `Binding::order`
    /// matters and the specific `FactRef` value is otherwise arbitrary.
    fn any_call_id() -> CallId {
        let mut builder = FactArenaBuilder::new();
        let call = fact_model::Call::new(
            builder.next_call_id(),
            None,
            CallKind::Call,
            CallDepth(0),
            fact_model::Address::new([1; 20]),
            None,
            Wei::ZERO,
            Gas(1),
            Gas(1),
            true,
        )
        .unwrap();
        builder.add_call(call)
    }

    fn binding(order: u32) -> Binding {
        Binding {
            fact: FactRef::Call(any_call_id()),
            order: Some(order),
            unresolved_attrs: Vec::new(),
        }
    }

    fn seq(steps: &[&str]) -> CompiledSequence {
        CompiledSequence {
            steps: steps
                .iter()
                .map(|s| dsl::ir::EvidenceRef(name_to_index(s)))
                .collect(),
            within_seconds: None,
        }
    }

    // Test-only helper mapping evidence names "e0".."e9" to their index,
    // since `CompiledSequence::steps` is `Vec<EvidenceRef>` in the
    // compiled IR (names only exist pre-compilation, in `dsl::ast`).
    fn name_to_index(name: &str) -> u32 {
        name.trim_start_matches('e').parse().unwrap()
    }

    #[test]
    fn increasing_order_resolves() {
        let evidence_matches = vec![vec![binding(0)], vec![binding(5)]];
        let sequence = seq(&["e0", "e1"]);
        let resolved = resolve(&sequence, &evidence_matches).unwrap();
        assert_eq!(resolved[0].order, Some(0));
        assert_eq!(resolved[1].order, Some(5));
    }

    #[test]
    fn out_of_order_only_binding_fails() {
        let evidence_matches = vec![vec![binding(10)], vec![binding(2)]];
        let sequence = seq(&["e0", "e1"]);
        assert!(resolve(&sequence, &evidence_matches).is_none());
    }

    #[test]
    fn greedy_picks_earliest_compatible_binding() {
        // Step 0 has bindings at order 0 and 3. Step 1 only has a
        // binding at order 2. Greedy must pick order 0 for step 0 (not
        // 3) so step 1's order-2 binding remains reachable.
        let evidence_matches = vec![vec![binding(0), binding(3)], vec![binding(2)]];
        let sequence = seq(&["e0", "e1"]);
        let resolved = resolve(&sequence, &evidence_matches).unwrap();
        assert_eq!(resolved[0].order, Some(0));
        assert_eq!(resolved[1].order, Some(2));
    }

    #[test]
    fn equal_order_is_allowed() {
        let evidence_matches = vec![vec![binding(4)], vec![binding(4)]];
        let sequence = seq(&["e0", "e1"]);
        assert!(resolve(&sequence, &evidence_matches).is_some());
    }

    #[test]
    fn missing_evidence_index_fails() {
        let evidence_matches = vec![vec![binding(0)]];
        let sequence = seq(&["e0", "e5"]); // index 5 out of range
        assert!(resolve(&sequence, &evidence_matches).is_none());
    }

    #[test]
    fn wildcard_none_order_is_always_compatible() {
        let evidence_matches = vec![
            vec![Binding {
                fact: FactRef::Call(any_call_id()),
                order: None,
                unresolved_attrs: Vec::new(),
            }],
            vec![binding(0)],
        ];
        let sequence = seq(&["e0", "e1"]);
        assert!(resolve(&sequence, &evidence_matches).is_some());
    }

    #[test]
    fn three_step_sequence_resolves() {
        let evidence_matches = vec![vec![binding(1)], vec![binding(4)], vec![binding(9)]];
        let sequence = seq(&["e0", "e1", "e2"]);
        let resolved = resolve(&sequence, &evidence_matches).unwrap();
        assert_eq!(
            resolved.iter().map(|b| b.order).collect::<Vec<_>>(),
            vec![Some(1), Some(4), Some(9)]
        );
    }
}

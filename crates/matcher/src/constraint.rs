//! Boolean constraint evaluation: whether a [`CompiledConstraint`] tree
//! is satisfied, given which evidence clauses currently have at least
//! one matching fact.
//!
//! This module is deliberately existence-only: `AND`/`OR`/`NOT` only
//! ever ask "does this evidence clause have a match" (`true`/`false`),
//! never "which specific fact." Picking a concrete fact for each
//! satisfied clause — and making sure that choice is consistent with any
//! `sequence:` ordering constraint — is [`crate::sequence`] and
//! [`crate::engine`]'s job, layered on top of this module's boolean
//! result. Keeping the two separate is what lets both be tested (and
//! reasoned about) independently: this module has no notion of "which
//! fact," and [`crate::sequence`] has no notion of `AND`/`OR`/`NOT`.

use dsl::ir::CompiledConstraint;
use dsl::EvidenceRef;

/// Evaluate `constraint`, calling `has_match(evidence)` for each leaf to
/// determine whether that evidence clause currently has at least one
/// matching fact.
///
/// # Complexity
/// `O(nodes in the constraint tree)`, each node visited exactly once. A
/// realistic pattern's constraint tree has at most a few dozen nodes (one
/// per evidence clause reference), so this is effectively constant time
/// relative to trace size.
#[must_use]
pub fn holds(constraint: &CompiledConstraint, has_match: &impl Fn(EvidenceRef) -> bool) -> bool {
    match constraint {
        CompiledConstraint::Evidence(evidence) => has_match(*evidence),
        CompiledConstraint::And(items) => items.iter().all(|item| holds(item, has_match)),
        CompiledConstraint::Or(items) => items.iter().any(|item| holds(item, has_match)),
        CompiledConstraint::Not(inner) => !holds(inner, has_match),
    }
}

/// Collect every [`EvidenceRef`] `constraint` references at **even**
/// `NOT`-nesting depth — i.e. every clause the constraint asserts must
/// be *present* for the constraint to hold, as opposed to a clause
/// wrapped in an odd number of `NOT`s (asserted *absent*).
///
/// [`crate::engine`] uses this to decide which evidence clauses need a
/// concrete fact binding in the final [`crate::CandidateMatch`]: a
/// positively-referenced, satisfied clause gets a binding citing the
/// fact that satisfied it; a negatively-referenced (`NOT`-wrapped)
/// clause that is correctly absent gets no binding at all — there is no
/// fact to cite for an absence.
///
/// The returned list is **not deduplicated**: a clause referenced twice
/// (e.g. `AND(a, OR(a, b))`) appears twice. Callers that only care about
/// *which* clauses need binding (not how many times they're referenced)
/// should deduplicate; [`crate::engine`] does so via a `HashSet` at the
/// call site, since dedup strategy is a caller concern, not a property
/// of the tree walk itself.
#[must_use]
pub fn positive_leaves(constraint: &CompiledConstraint) -> Vec<EvidenceRef> {
    let mut out = Vec::new();
    collect_positive_leaves(constraint, true, &mut out);
    out
}

fn collect_positive_leaves(
    constraint: &CompiledConstraint,
    positive: bool,
    out: &mut Vec<EvidenceRef>,
) {
    match constraint {
        CompiledConstraint::Evidence(evidence) => {
            if positive {
                out.push(*evidence);
            }
        }
        CompiledConstraint::And(items) | CompiledConstraint::Or(items) => {
            for item in items {
                collect_positive_leaves(item, positive, out);
            }
        }
        CompiledConstraint::Not(inner) => collect_positive_leaves(inner, !positive, out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(i: u32) -> EvidenceRef {
        EvidenceRef(i)
    }

    fn leaf(i: u32) -> CompiledConstraint {
        CompiledConstraint::Evidence(ev(i))
    }

    #[test]
    fn single_evidence_holds_iff_present() {
        let c = leaf(0);
        assert!(holds(&c, &|e| e == ev(0)));
        assert!(!holds(&c, &|_| false));
    }

    #[test]
    fn and_requires_all() {
        let c = CompiledConstraint::And(vec![leaf(0), leaf(1)]);
        assert!(holds(&c, &|_| true));
        assert!(!holds(&c, &|e| e == ev(0)));
    }

    #[test]
    fn or_requires_any() {
        let c = CompiledConstraint::Or(vec![leaf(0), leaf(1)]);
        assert!(holds(&c, &|e| e == ev(1)));
        assert!(!holds(&c, &|_| false));
    }

    #[test]
    fn not_inverts() {
        let c = CompiledConstraint::Not(Box::new(leaf(0)));
        assert!(holds(&c, &|_| false));
        assert!(!holds(&c, &|_| true));
    }

    #[test]
    fn nested_and_or_not() {
        // AND(a, OR(b, NOT(c)))
        let c = CompiledConstraint::And(vec![
            leaf(0),
            CompiledConstraint::Or(vec![leaf(1), CompiledConstraint::Not(Box::new(leaf(2)))]),
        ]);
        // a present, b absent, c absent -> NOT(c) true -> OR true -> AND true
        assert!(holds(&c, &|e| e == ev(0)));
        // a present, b absent, c present -> NOT(c) false, b false -> OR false -> AND false
        assert!(!holds(&c, &|e| e == ev(0) || e == ev(2)));
    }

    #[test]
    fn positive_leaves_excludes_negated() {
        let c = CompiledConstraint::And(vec![leaf(0), CompiledConstraint::Not(Box::new(leaf(1)))]);
        assert_eq!(positive_leaves(&c), vec![ev(0)]);
    }

    #[test]
    fn positive_leaves_double_negation_is_positive_again() {
        let c = CompiledConstraint::Not(Box::new(CompiledConstraint::Not(Box::new(leaf(0)))));
        assert_eq!(positive_leaves(&c), vec![ev(0)]);
    }

    #[test]
    fn positive_leaves_keeps_duplicates() {
        let c = CompiledConstraint::And(vec![
            leaf(0),
            CompiledConstraint::Or(vec![leaf(0), leaf(1)]),
        ]);
        assert_eq!(positive_leaves(&c), vec![ev(0), ev(0), ev(1)]);
    }
}

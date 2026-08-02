//! Constraint polarity analysis: for every evidence clause a pattern's
//! `constraint:` tree references, determine whether it is required to
//! be *present* (referenced under an even number of `NOT`s) or *absent*
//! (odd), and detect the one shape [`dsl::validate`] does not itself
//! reject: a clause referenced with **both** polarities somewhere in the
//! same tree.
//!
//! `matcher::constraint::positive_leaves` already computes the positive
//! set; this module additionally computes the negative set and their
//! intersection, which grounding needs (and matcher does not) to decide
//! how to independently verify a `NOT`-negated clause — confirming
//! *absence*, not presence — and to detect the contradictory-polarity
//! case matcher has no reason to care about (matcher only asks "does the
//! whole tree hold," never "verify this clause in isolation").

use std::collections::HashSet;

use dsl::ir::CompiledConstraint;
use dsl::EvidenceRef;

/// An evidence clause's polarity within a pattern's constraint tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    /// Referenced only under an even number of `NOT`s (or not negated
    /// at all): the constraint requires this clause's predicate to be
    /// satisfied by some fact.
    Positive,
    /// Referenced only under an odd number of `NOT`s: the constraint
    /// requires this clause's predicate to be satisfied by **no** fact.
    Negative,
    /// Not referenced anywhere in the constraint tree at all (possible
    /// for a required evidence clause the pattern author's explicit
    /// `constraint:` simply didn't mention — `dsl::validate` only warns
    /// about this, it does not reject it). Grounding still attempts to
    /// verify such a clause, treating it as [`Self::Positive`] (see
    /// [`crate::engine`]'s own docs) since "required" with no stated
    /// polarity defaults to "required present," the far more common
    /// intent.
    Unreferenced,
}

/// Compute every evidence index's [`Polarity`] within `constraint`.
///
/// # Errors
/// Returns the offending [`EvidenceRef`] if any evidence index is
/// referenced under **both** polarities somewhere in the tree (e.g.
/// `AND(a, NOT(a))`) — a self-contradictory constraint `dsl::validate`
/// does not itself reject, since it is not a structural or reference
/// error at the DSL level, only a semantic one grounding is positioned
/// to detect.
pub fn analyze(
    constraint: &CompiledConstraint,
) -> Result<Vec<(EvidenceRef, Polarity)>, EvidenceRef> {
    let mut positive: HashSet<EvidenceRef> = HashSet::new();
    let mut negative: HashSet<EvidenceRef> = HashSet::new();
    walk(constraint, true, &mut positive, &mut negative);

    if let Some(&evidence) = positive.intersection(&negative).next() {
        return Err(evidence);
    }

    let mut result: Vec<(EvidenceRef, Polarity)> = positive
        .into_iter()
        .map(|e| (e, Polarity::Positive))
        .collect();
    result.extend(negative.into_iter().map(|e| (e, Polarity::Negative)));
    result.sort_by_key(|(e, _)| e.index());
    Ok(result)
}

fn walk(
    constraint: &CompiledConstraint,
    positive: bool,
    pos_set: &mut HashSet<EvidenceRef>,
    neg_set: &mut HashSet<EvidenceRef>,
) {
    match constraint {
        CompiledConstraint::Evidence(evidence) => {
            if positive {
                pos_set.insert(*evidence);
            } else {
                neg_set.insert(*evidence);
            }
        }
        CompiledConstraint::And(items) | CompiledConstraint::Or(items) => {
            for item in items {
                walk(item, positive, pos_set, neg_set);
            }
        }
        CompiledConstraint::Not(inner) => walk(inner, !positive, pos_set, neg_set),
    }
}

/// Look up a specific evidence index's polarity in an already-analyzed
/// list (as returned by [`analyze`]), defaulting to
/// [`Polarity::Unreferenced`] if the index does not appear at all.
#[must_use]
pub fn polarity_of(analyzed: &[(EvidenceRef, Polarity)], evidence: EvidenceRef) -> Polarity {
    analyzed
        .iter()
        .find(|(e, _)| *e == evidence)
        .map_or(Polarity::Unreferenced, |(_, p)| *p)
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
    fn simple_and_is_all_positive() {
        let c = CompiledConstraint::And(vec![leaf(0), leaf(1)]);
        let analyzed = analyze(&c).unwrap();
        assert_eq!(polarity_of(&analyzed, ev(0)), Polarity::Positive);
        assert_eq!(polarity_of(&analyzed, ev(1)), Polarity::Positive);
    }

    #[test]
    fn not_wrapped_leaf_is_negative() {
        let c = CompiledConstraint::And(vec![leaf(0), CompiledConstraint::Not(Box::new(leaf(1)))]);
        let analyzed = analyze(&c).unwrap();
        assert_eq!(polarity_of(&analyzed, ev(0)), Polarity::Positive);
        assert_eq!(polarity_of(&analyzed, ev(1)), Polarity::Negative);
    }

    #[test]
    fn double_negation_is_positive_again() {
        let c = CompiledConstraint::Not(Box::new(CompiledConstraint::Not(Box::new(leaf(0)))));
        let analyzed = analyze(&c).unwrap();
        assert_eq!(polarity_of(&analyzed, ev(0)), Polarity::Positive);
    }

    #[test]
    fn unreferenced_evidence_defaults_unreferenced() {
        let c = leaf(0);
        let analyzed = analyze(&c).unwrap();
        assert_eq!(polarity_of(&analyzed, ev(99)), Polarity::Unreferenced);
    }

    #[test]
    fn contradictory_polarity_is_rejected() {
        let c = CompiledConstraint::And(vec![leaf(0), CompiledConstraint::Not(Box::new(leaf(0)))]);
        assert_eq!(analyze(&c), Err(ev(0)));
    }

    #[test]
    fn same_polarity_twice_is_not_contradictory() {
        let c = CompiledConstraint::Or(vec![leaf(0), leaf(0)]);
        assert!(analyze(&c).is_ok());
    }
}

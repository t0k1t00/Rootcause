//! Property tests over [`benchmark_harness::metrics::compute`] and the
//! ratio helpers it builds on, using hand-built [`CaseResult`]s (no
//! pipeline dependency, per [`metrics::compute`]'s own "pure function of
//! case results" design note in the crate's top-level docs).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use benchmark_harness::metrics::compute;
use benchmark_harness::runner::{CaseResult, PatternResult};
use benchmark_harness::timing::StageTimings;
use benchmark_harness::types::CaseId;
use benchmark_harness::PatternOutcome;
use dsl::ir::{PatternId, PatternVersion};
use proptest::prelude::*;

/// The four [`PatternOutcome`] variants as a `proptest` strategy.
fn outcome_strategy() -> impl Strategy<Value = PatternOutcome> {
    prop_oneof![
        Just(PatternOutcome::Grounded),
        Just(PatternOutcome::Abstain),
        Just(PatternOutcome::Ungrounded),
        Just(PatternOutcome::NoMatch),
    ]
}

/// Build one single-pattern, single-case [`CaseResult`] from an
/// (expected, actual) pair, with a case index folded into both the case
/// id and the pattern id so a whole generated `Vec` has no accidental
/// id collisions.
fn case_result(index: usize, expected: PatternOutcome, actual: PatternOutcome) -> CaseResult {
    let pattern_result = PatternResult {
        pattern_id: PatternId(format!("pattern-{index}")),
        pattern_version: PatternVersion(1),
        candidate_count: 1,
        actual,
        expected: Some(expected),
        grounding_results: Vec::new(),
        taxonomy_reports: Vec::new(),
    };
    CaseResult {
        case_id: CaseId(format!("case-{index}")),
        description: "generated".to_string(),
        pattern_results: vec![pattern_result],
        timings: StageTimings::default(),
        error: None,
    }
}

proptest! {
    /// `compute` is a pure function: calling it twice on the same input
    /// (built fresh both times) produces the same aggregate counts.
    #[test]
    fn compute_is_deterministic(
        outcomes in prop::collection::vec((outcome_strategy(), outcome_strategy()), 0..20)
    ) {
        let build = || {
            outcomes
                .iter()
                .enumerate()
                .map(|(i, (expected, actual))| case_result(i, *expected, *actual))
                .collect::<Vec<_>>()
        };
        let a = compute(&build());
        let b = compute(&build());

        prop_assert_eq!(a.overall.true_positives, b.overall.true_positives);
        prop_assert_eq!(a.overall.false_positives, b.overall.false_positives);
        prop_assert_eq!(a.overall.false_negatives, b.overall.false_negatives);
        prop_assert_eq!(a.overall.true_negatives, b.overall.true_negatives);
        prop_assert_eq!(a.confusion.total(), b.confusion.total());
        prop_assert_eq!(a.confusion.correct(), b.confusion.correct());
    }

    /// Every scored pattern result lands in exactly one of the four
    /// `BenchmarkMetrics` buckets, so they always sum to the total
    /// number of scored results — and that total always equals the
    /// confusion matrix's own total over the same input.
    #[test]
    fn precision_recall_buckets_partition_every_scored_result(
        outcomes in prop::collection::vec((outcome_strategy(), outcome_strategy()), 0..20)
    ) {
        let results: Vec<CaseResult> = outcomes
            .iter()
            .enumerate()
            .map(|(i, (expected, actual))| case_result(i, *expected, *actual))
            .collect();
        let summary = compute(&results);
        let m = &summary.overall;

        let bucket_sum = m.true_positives + m.false_positives + m.false_negatives + m.true_negatives;
        prop_assert_eq!(bucket_sum, outcomes.len());
        prop_assert_eq!(summary.confusion.total(), outcomes.len());
    }

    /// Precision, recall, and F1 are always finite and within `[0, 1]`,
    /// regardless of input — the zero-denominator convention never
    /// produces `NaN` or an out-of-range ratio.
    #[test]
    fn precision_recall_f1_always_in_unit_range(
        outcomes in prop::collection::vec((outcome_strategy(), outcome_strategy()), 0..20)
    ) {
        let results: Vec<CaseResult> = outcomes
            .iter()
            .enumerate()
            .map(|(i, (expected, actual))| case_result(i, *expected, *actual))
            .collect();
        let m = compute(&results).overall;

        for value in [m.precision(), m.recall(), m.f1()] {
            prop_assert!(!value.is_nan());
            prop_assert!((0.0..=1.0).contains(&value));
        }
    }

    /// The confusion matrix's accuracy is always finite and within
    /// `[0, 1]`.
    #[test]
    fn confusion_matrix_accuracy_always_in_unit_range(
        outcomes in prop::collection::vec((outcome_strategy(), outcome_strategy()), 0..20)
    ) {
        let results: Vec<CaseResult> = outcomes
            .iter()
            .enumerate()
            .map(|(i, (expected, actual))| case_result(i, *expected, *actual))
            .collect();
        let accuracy = compute(&results).confusion.accuracy();

        prop_assert!(!accuracy.is_nan());
        prop_assert!((0.0..=1.0).contains(&accuracy));
    }
}

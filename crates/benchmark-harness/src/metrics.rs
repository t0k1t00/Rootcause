//! Correctness metrics: precision, recall, F1, a confusion matrix over
//! [`PatternOutcome`], and per-pattern / per-family breakdowns.

use std::collections::BTreeMap;

use dsl::ir::{PatternFamily, PatternId};

use crate::runner::CaseResult;
use crate::types::PatternOutcome;

/// A 4×4 confusion matrix over [`PatternOutcome`]: for every
/// (expected, actual) pair among scored pattern results, how many times
/// it occurred.
///
/// Only pattern results with an expectation are counted — a pattern run
/// with no [`crate::types::ExpectedFinding`] contributes to no cell.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfusionMatrix {
    counts: BTreeMap<(PatternOutcome, PatternOutcome), usize>,
}

/// The four [`PatternOutcome`] variants, in a fixed, stable order — used
/// wherever this crate needs to enumerate them (confusion-matrix rows/
/// columns, CSV/Markdown export).
pub const ALL_OUTCOMES: [PatternOutcome; 4] = [
    PatternOutcome::Grounded,
    PatternOutcome::Abstain,
    PatternOutcome::Ungrounded,
    PatternOutcome::NoMatch,
];

impl ConfusionMatrix {
    fn record(&mut self, expected: PatternOutcome, actual: PatternOutcome) {
        *self.counts.entry((expected, actual)).or_insert(0) += 1;
    }

    /// How many times `expected` was predicted as `actual`.
    #[must_use]
    pub fn get(&self, expected: PatternOutcome, actual: PatternOutcome) -> usize {
        self.counts.get(&(expected, actual)).copied().unwrap_or(0)
    }

    /// The total number of scored pattern results this matrix
    /// summarizes.
    #[must_use]
    pub fn total(&self) -> usize {
        self.counts.values().sum()
    }

    /// How many scored pattern results had `actual == expected`.
    #[must_use]
    pub fn correct(&self) -> usize {
        ALL_OUTCOMES.iter().map(|o| self.get(*o, *o)).sum()
    }

    /// Overall accuracy: [`Self::correct`] over [`Self::total`]. `0.0`
    /// if nothing was scored (never `NaN` — see
    /// [`BenchmarkMetrics::precision`] for the same convention).
    #[must_use]
    pub fn accuracy(&self) -> f64 {
        safe_ratio(self.correct(), self.total())
    }
}

#[allow(clippy::cast_precision_loss)] // counts here are benchmark-run tallies, always << 2^52
fn safe_ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

/// Precision/recall/F1, treating [`PatternOutcome::Grounded`] as the
/// positive class and every other outcome (`Abstain`, `Ungrounded`,
/// `NoMatch`) as negative.
///
/// This is the standard framing for this benchmark: "did the pipeline
/// confidently confirm a real finding" is the question precision and
/// recall answer here, deliberately collapsing the three negative
/// outcomes together rather than picking one of them as "the" negative
/// class — a case expecting `Ungrounded` and a pattern that actually
/// abstained are both "failed to confirm," which is the distinction
/// this metric is for; the confusion matrix ([`ConfusionMatrix`])
/// preserves the finer-grained distinction for anyone who needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BenchmarkMetrics {
    /// True positives: expected `Grounded`, actual `Grounded`.
    pub true_positives: usize,
    /// False positives: expected not-`Grounded`, actual `Grounded`.
    pub false_positives: usize,
    /// False negatives: expected `Grounded`, actual not-`Grounded`.
    pub false_negatives: usize,
    /// True negatives: expected not-`Grounded`, actual not-`Grounded`.
    pub true_negatives: usize,
}

impl BenchmarkMetrics {
    /// `true_positives / (true_positives + false_positives)`. `0.0` if
    /// the pipeline never predicted `Grounded` at all — a deliberate,
    /// documented convention (not `NaN`) so this value is always safe
    /// to print, export, and compare without a caller needing to
    /// special-case division by zero.
    #[must_use]
    pub fn precision(&self) -> f64 {
        safe_ratio(
            self.true_positives,
            self.true_positives + self.false_positives,
        )
    }

    /// `true_positives / (true_positives + false_negatives)`. `0.0` if
    /// no case expected `Grounded` at all — see [`Self::precision`]'s
    /// zero-denominator convention.
    #[must_use]
    pub fn recall(&self) -> f64 {
        safe_ratio(
            self.true_positives,
            self.true_positives + self.false_negatives,
        )
    }

    /// The harmonic mean of [`Self::precision`] and [`Self::recall`].
    /// `0.0` if both are `0.0`.
    #[must_use]
    pub fn f1(&self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p + r == 0.0 {
            0.0
        } else {
            2.0 * p * r / (p + r)
        }
    }

    fn record(&mut self, expected: PatternOutcome, actual: PatternOutcome) {
        let expected_positive = expected == PatternOutcome::Grounded;
        let actual_positive = actual == PatternOutcome::Grounded;
        match (expected_positive, actual_positive) {
            (true, true) => self.true_positives += 1,
            (false, true) => self.false_positives += 1,
            (true, false) => self.false_negatives += 1,
            (false, false) => self.true_negatives += 1,
        }
    }
}

/// Aggregated statistics for one pattern across every case that ran it.
#[derive(Debug, Clone, Default)]
pub struct PatternStats {
    /// How many cases ran this pattern.
    pub runs: usize,
    /// How many of those runs matched their expectation (unscored runs
    /// count as matching, per [`CaseResult::passed`]'s convention).
    pub passed: usize,
    /// How many candidate matches this pattern produced, summed across
    /// every run.
    pub total_candidates: usize,
    /// This pattern's own confusion matrix, restricted to its runs.
    pub confusion: ConfusionMatrix,
}

/// Aggregated statistics for one pattern family across every pattern in
/// it and every case that ran one of those patterns.
#[derive(Debug, Clone, Default)]
pub struct FamilyStats {
    /// How many cases ran a pattern in this family.
    pub runs: usize,
    /// How many of those runs matched their expectation.
    pub passed: usize,
    /// This family's own confusion matrix.
    pub confusion: ConfusionMatrix,
}

/// One case/pattern combination that failed to reach its expected
/// outcome, or errored outright — the raw material for
/// [`MetricsSummary::failures`].
#[derive(Debug, Clone)]
pub struct FailureSummary {
    /// The case this failure occurred in.
    pub case_id: crate::types::CaseId,
    /// The pattern this failure concerns, `None` for a whole-case
    /// failure (e.g. an ingestion error) that happened before any
    /// pattern could run.
    pub pattern_id: Option<PatternId>,
    /// A human-readable description of the failure.
    pub detail: String,
}

/// Everything [`compute`] derives from a completed suite run: the
/// overall confusion matrix and precision/recall/F1, plus per-pattern
/// and per-family breakdowns and a flat list of failures.
#[derive(Debug, Clone, Default)]
pub struct MetricsSummary {
    /// The overall confusion matrix across every scored pattern result
    /// in the suite.
    pub confusion: ConfusionMatrix,
    /// Overall precision/recall/F1 (see [`BenchmarkMetrics`]'s own
    /// docs for the positive-class convention).
    pub overall: BenchmarkMetrics,
    /// Per-pattern statistics, keyed by pattern id, in first-seen
    /// order.
    pub per_pattern: Vec<(PatternId, PatternStats)>,
    /// Per-family statistics, keyed by family, in first-seen order.
    pub per_family: Vec<(PatternFamily, FamilyStats)>,
    /// Every failed or errored case/pattern combination.
    pub failures: Vec<FailureSummary>,
}

/// Compute a [`MetricsSummary`] from a completed suite's [`CaseResult`]s.
#[must_use]
pub fn compute(results: &[CaseResult]) -> MetricsSummary {
    let mut summary = MetricsSummary::default();
    let mut pattern_index: BTreeMap<PatternId, usize> = BTreeMap::new();
    let mut family_index: BTreeMap<PatternFamily, usize> = BTreeMap::new();

    for case in results {
        if let Some(err) = &case.error {
            summary.failures.push(FailureSummary {
                case_id: case.case_id.clone(),
                pattern_id: None,
                detail: err.clone(),
            });
        }

        for pr in &case.pattern_results {
            let idx = *pattern_index
                .entry(pr.pattern_id.clone())
                .or_insert_with(|| {
                    summary
                        .per_pattern
                        .push((pr.pattern_id.clone(), PatternStats::default()));
                    summary.per_pattern.len() - 1
                });
            let stats = &mut summary.per_pattern[idx].1;
            stats.runs += 1;
            stats.total_candidates += pr.candidate_count;

            let family = pr
                .grounding_results
                .first()
                .map(|g| g.pattern_family.clone());

            let is_pass = pr.matches_expectation().unwrap_or(true);
            if is_pass {
                stats.passed += 1;
            } else {
                summary.failures.push(FailureSummary {
                    case_id: case.case_id.clone(),
                    pattern_id: Some(pr.pattern_id.clone()),
                    detail: format!(
                        "expected {}, got {}",
                        pr.expected
                            .map_or_else(|| "n/a".to_string(), |e| e.to_string()),
                        pr.actual
                    ),
                });
            }

            if let Some(expected) = pr.expected {
                stats.confusion.record(expected, pr.actual);
                summary.confusion.record(expected, pr.actual);
                summary.overall.record(expected, pr.actual);

                if let Some(family) = &family {
                    let family_idx = *family_index.entry(family.clone()).or_insert_with(|| {
                        summary
                            .per_family
                            .push((family.clone(), FamilyStats::default()));
                        summary.per_family.len() - 1
                    });
                    let fam_stats = &mut summary.per_family[family_idx].1;
                    fam_stats.runs += 1;
                    if is_pass {
                        fam_stats.passed += 1;
                    }
                    fam_stats.confusion.record(expected, pr.actual);
                }
            }
        }
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confusion_matrix_accumulates_and_reports_accuracy() {
        let mut m = ConfusionMatrix::default();
        m.record(PatternOutcome::Grounded, PatternOutcome::Grounded);
        m.record(PatternOutcome::Grounded, PatternOutcome::Abstain);
        m.record(PatternOutcome::NoMatch, PatternOutcome::NoMatch);
        assert_eq!(m.total(), 3);
        assert_eq!(m.correct(), 2);
        assert!((m.accuracy() - 2.0 / 3.0).abs() < f64::EPSILON);
    }

    #[test]
    #[allow(clippy::float_cmp)] // asserting the exact-0.0 zero-denominator contract, not a computed value
    fn confusion_matrix_empty_accuracy_is_zero_not_nan() {
        let m = ConfusionMatrix::default();
        assert_eq!(m.accuracy(), 0.0);
        assert!(!m.accuracy().is_nan());
    }

    #[test]
    fn benchmark_metrics_precision_recall_f1() {
        let mut m = BenchmarkMetrics::default();
        // 2 true positives, 1 false positive, 1 false negative.
        m.record(PatternOutcome::Grounded, PatternOutcome::Grounded);
        m.record(PatternOutcome::Grounded, PatternOutcome::Grounded);
        m.record(PatternOutcome::Abstain, PatternOutcome::Grounded);
        m.record(PatternOutcome::Grounded, PatternOutcome::Abstain);

        assert_eq!(m.true_positives, 2);
        assert_eq!(m.false_positives, 1);
        assert_eq!(m.false_negatives, 1);
        assert!((m.precision() - (2.0 / 3.0)).abs() < 1e-9);
        assert!((m.recall() - (2.0 / 3.0)).abs() < 1e-9);
        assert!((m.f1() - (2.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    #[allow(clippy::float_cmp)] // asserting the exact-0.0 zero-denominator contract, not a computed value
    fn benchmark_metrics_zero_denominator_is_zero_not_nan() {
        let m = BenchmarkMetrics::default();
        assert_eq!(m.precision(), 0.0);
        assert_eq!(m.recall(), 0.0);
        assert_eq!(m.f1(), 0.0);
    }

    #[test]
    fn compute_from_hand_built_case_results_is_pure() {
        use crate::runner::{CaseResult, PatternResult};
        use crate::timing::StageTimings;
        use dsl::ir::{PatternId, PatternVersion};

        let pass = PatternResult {
            pattern_id: PatternId("p1".to_string()),
            pattern_version: PatternVersion(1),
            candidate_count: 1,
            actual: PatternOutcome::Grounded,
            expected: Some(PatternOutcome::Grounded),
            grounding_results: Vec::new(),
            taxonomy_reports: Vec::new(),
        };
        let fail = PatternResult {
            pattern_id: PatternId("p2".to_string()),
            pattern_version: PatternVersion(1),
            candidate_count: 1,
            actual: PatternOutcome::Abstain,
            expected: Some(PatternOutcome::Grounded),
            grounding_results: Vec::new(),
            taxonomy_reports: Vec::new(),
        };
        let results = vec![
            CaseResult {
                case_id: crate::types::CaseId("case1".to_string()),
                description: "one".to_string(),
                pattern_results: vec![pass],
                timings: StageTimings::default(),
                error: None,
            },
            CaseResult {
                case_id: crate::types::CaseId("case2".to_string()),
                description: "two".to_string(),
                pattern_results: vec![fail],
                timings: StageTimings::default(),
                error: None,
            },
        ];

        let summary = compute(&results);
        assert_eq!(summary.overall.true_positives, 1);
        assert_eq!(summary.overall.false_negatives, 1);
        assert_eq!(summary.per_pattern.len(), 2);
        assert_eq!(summary.failures.len(), 1);
        assert_eq!(summary.failures[0].case_id.0, "case2");
    }

    #[test]
    fn compute_records_case_level_error_as_failure() {
        use crate::runner::CaseResult;
        use crate::timing::StageTimings;

        let results = vec![CaseResult {
            case_id: crate::types::CaseId("broken".to_string()),
            description: "broken case".to_string(),
            pattern_results: Vec::new(),
            timings: StageTimings::default(),
            error: Some("ingestion failed".to_string()),
        }];

        let summary = compute(&results);
        assert_eq!(summary.failures.len(), 1);
        assert!(summary.failures[0].pattern_id.is_none());
    }
}

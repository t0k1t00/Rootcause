//! [`BenchmarkReport`]: the strongly-typed, exportable summary of a
//! whole suite run.

use crate::metrics::{self, MetricsSummary};
use crate::runner::CaseResult;
use crate::timing::StageTimings;
use crate::types::BenchmarkSuite;

/// The complete result of running a [`BenchmarkSuite`]: every case's
/// raw result, the derived correctness metrics, and aggregate timings.
///
/// This is the single type [`crate::export`] serializes to JSON, CSV,
/// and Markdown — every export format is a projection of exactly this
/// data, never a separately maintained view, so the three formats
/// cannot silently drift apart.
#[derive(Debug, Clone)]
pub struct BenchmarkReport {
    /// The suite's name.
    pub suite_name: String,
    /// Every case's result, in suite order.
    pub case_results: Vec<CaseResult>,
    /// Correctness metrics derived from [`Self::case_results`] — see
    /// [`MetricsSummary`].
    pub metrics: MetricsSummary,
    /// Stage timings summed across every case in
    /// [`Self::case_results`].
    pub total_timings: StageTimings,
    /// How many cases ran without a case-level error (whether or not
    /// their patterns matched expectations).
    pub cases_run: usize,
    /// How many cases failed outright (see [`CaseResult::error`]).
    pub cases_errored: usize,
}

impl BenchmarkReport {
    /// Build a report from a suite and its already-computed case
    /// results (typically from [`crate::runner::run_suite`]).
    #[must_use]
    pub fn build(suite: &BenchmarkSuite, case_results: Vec<CaseResult>) -> Self {
        let metrics = metrics::compute(&case_results);
        let mut total_timings = StageTimings::default();
        let mut cases_errored = 0;
        for case in &case_results {
            total_timings.accumulate(&case.timings);
            if case.error.is_some() {
                cases_errored += 1;
            }
        }

        Self {
            suite_name: suite.name.clone(),
            cases_run: case_results.len(),
            cases_errored,
            case_results,
            metrics,
            total_timings,
        }
    }

    /// Run `suite` and build its report in one step.
    ///
    /// # Errors
    /// Returns [`crate::error::BenchmarkError::DuplicateCaseId`] if the
    /// suite fails validation before any case is run.
    pub fn run(suite: &BenchmarkSuite) -> Result<Self, crate::error::BenchmarkError> {
        let results = crate::runner::run_suite(suite)?;
        Ok(Self::build(suite, results))
    }

    /// How many cases passed — see [`CaseResult::passed`].
    #[must_use]
    pub fn cases_passed(&self) -> usize {
        self.case_results.iter().filter(|c| c.passed()).count()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::sample_fixtures;
    use crate::types::BenchmarkSuite;

    fn sample_suite() -> BenchmarkSuite {
        BenchmarkSuite::from_cases(
            "sample-suite",
            vec![
                sample_fixtures::grounded_reentrancy_case(),
                sample_fixtures::abstained_oracle_manipulation_case(),
                sample_fixtures::no_match_case(),
            ],
        )
    }

    #[test]
    fn run_builds_a_complete_report() {
        let suite = sample_suite();
        let report = BenchmarkReport::run(&suite).expect("valid suite should run");

        assert_eq!(report.suite_name, "sample-suite");
        assert_eq!(report.cases_run, 3);
        assert_eq!(report.cases_errored, 0);
        assert_eq!(report.case_results.len(), 3);
        // All three cases match their own expectation by construction.
        assert_eq!(report.cases_passed(), 3);
    }

    #[test]
    fn build_sums_timings_and_counts_errors() {
        let suite = sample_suite();
        let results = crate::runner::run_suite(&suite).expect("valid suite should run");
        let report = BenchmarkReport::build(&suite, results);

        let expected_total: StageTimings = {
            let mut t = StageTimings::default();
            for c in &report.case_results {
                t.accumulate(&c.timings);
            }
            t
        };
        assert_eq!(report.total_timings, expected_total);
        assert_eq!(report.cases_errored, 0);
    }

    #[test]
    fn run_rejects_duplicate_case_ids_before_running_anything() {
        let suite = BenchmarkSuite::from_cases(
            "dup",
            vec![
                sample_fixtures::grounded_reentrancy_case(),
                sample_fixtures::grounded_reentrancy_case(),
            ],
        );
        let err = BenchmarkReport::run(&suite).expect_err("duplicate ids must be rejected");
        assert!(matches!(
            err,
            crate::error::BenchmarkError::DuplicateCaseId { .. }
        ));
    }
}

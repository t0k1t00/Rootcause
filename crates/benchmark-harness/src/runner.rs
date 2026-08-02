//! Orchestrates the complete pipeline (ingestion → matcher → grounding
//! → taxonomy) for one [`BenchmarkCase`] or a whole [`BenchmarkSuite`],
//! producing timed, per-pattern results.

use std::time::Instant;

use dsl::ir::{PatternId, PatternVersion};
use fact_model::Trace;
use grounding::GroundingResult;
use taxonomy::{MappingEngine, TaxonomyReport};

use crate::error::BenchmarkError;
use crate::timing::StageTimings;
use crate::types::{BenchmarkCase, BenchmarkSuite, ExpectedFinding, PatternOutcome, TraceInput};

/// The complete result of running one pattern against one case's trace.
#[derive(Debug, Clone)]
pub struct PatternResult {
    /// The pattern's identity.
    pub pattern_id: PatternId,
    /// The pattern's version.
    pub pattern_version: PatternVersion,
    /// How many candidate matches `matcher` found for this pattern.
    pub candidate_count: usize,
    /// This pattern's coarsened outcome against this case's trace — see
    /// [`PatternOutcome`].
    pub actual: PatternOutcome,
    /// What the case's author expected, if this pattern had an
    /// [`ExpectedFinding`] entry.
    pub expected: Option<PatternOutcome>,
    /// Every independent grounding result for this pattern's
    /// candidates, in candidate order.
    pub grounding_results: Vec<GroundingResult>,
    /// Every taxonomy mapping for this pattern's grounding results, in
    /// the same order as [`Self::grounding_results`].
    pub taxonomy_reports: Vec<TaxonomyReport>,
}

impl PatternResult {
    /// Whether this pattern reached its case's expected outcome.
    /// `None` if the case asserted no expectation for this pattern
    /// (never scored as a pass or a fail).
    #[must_use]
    pub fn matches_expectation(&self) -> Option<bool> {
        self.expected.map(|expected| expected == self.actual)
    }
}

/// The complete result of running one [`BenchmarkCase`]: every
/// pattern's [`PatternResult`], the stage timings spent producing them,
/// and any case-level error.
#[derive(Debug, Clone)]
pub struct CaseResult {
    /// The case's id, copied for convenience.
    pub case_id: crate::types::CaseId,
    /// The case's description, copied for convenience.
    pub description: String,
    /// Every pattern's result, in the case's pattern order. Empty if
    /// [`Self::error`] is `Some` and the failure happened before any
    /// pattern could be run (e.g. ingestion failure).
    pub pattern_results: Vec<PatternResult>,
    /// Stage timings for this case, summed across every pattern it ran.
    pub timings: StageTimings,
    /// A human-readable description of a case-level failure, if
    /// running this case failed outright (as opposed to a pattern
    /// simply reaching an unexpected outcome, which is not a failure at
    /// this level — see [`crate::metrics`] for how that is scored
    /// instead).
    pub error: Option<String>,
}

impl CaseResult {
    /// Whether every pattern in this case that had an expectation
    /// reached it, and the case itself ran without error. A case with
    /// no expectations at all is trivially `true` (nothing to fail).
    #[must_use]
    pub fn passed(&self) -> bool {
        self.error.is_none()
            && self
                .pattern_results
                .iter()
                .all(|p| p.matches_expectation().unwrap_or(true))
    }
}

/// Run every pattern in `case` against its trace, producing one
/// [`CaseResult`].
///
/// A failure ingesting the case's trace is fatal to the whole case (no
/// pattern can run without a trace) and is recorded on
/// [`CaseResult::error`] with an empty [`CaseResult::pattern_results`].
/// A failure matching or grounding one specific pattern is recorded the
/// same way but does not prevent the case's remaining patterns from
/// still being run — see the loop body below.
///
/// # Errors
/// This function itself never returns `Err`: every failure mode is
/// captured on the returned [`CaseResult`] instead, so that
/// [`run_suite`] can always produce a complete report even when
/// individual cases fail. It returns a `Result` only for symmetry with
/// the rest of this crate's fallible API and to leave room for a truly
/// unrecoverable failure (there is none today).
///
/// # Panics
/// Never panics in practice: the one `.expect("just assigned")` below
/// fires only on the branch that itself just set `owned_trace = Some(..)`
/// two lines earlier, so the value is always present.
#[allow(clippy::too_many_lines)] // one linear ingestion->matching->grounding->taxonomy pipeline
pub fn run_case(case: &BenchmarkCase) -> Result<CaseResult, BenchmarkError> {
    let mut timings = StageTimings::default();

    let owned_trace: Option<Trace>;
    #[allow(unused_assignments)]
    let trace_ref: &Trace = match &case.trace_input {
        TraceInput::FromSource {
            source,
            provenance_label: _,
        } => {
            let start = Instant::now();
            let ingested = ingestion::ingest(source.as_ref(), case.provenance());
            timings.ingestion = start.elapsed();
            match ingested {
                Ok(trace) => {
                    owned_trace = Some(trace);
                    // Unreachable panic: `owned_trace` was just set to
                    // `Some(..)` on the line above.
                    #[allow(clippy::expect_used)]
                    owned_trace.as_ref().expect("just assigned")
                }
                Err(source_err) => {
                    return Ok(CaseResult {
                        case_id: case.id.clone(),
                        description: case.description.clone(),
                        pattern_results: Vec::new(),
                        timings,
                        error: Some(
                            BenchmarkError::Ingestion {
                                case_id: case.id.0.clone(),
                                source: source_err,
                            }
                            .to_string(),
                        ),
                    });
                }
            }
        }
        TraceInput::Prebuilt(trace) => {
            owned_trace = None;
            trace.as_ref()
        }
    };

    let engine = matcher::MatchEngine::new(trace_ref);
    let taxonomy_engine = MappingEngine::new();

    let mut pattern_results = Vec::with_capacity(case.patterns.len());
    let mut case_errors: Vec<String> = Vec::new();

    for pattern in &case.patterns {
        let match_start = Instant::now();
        let candidates = engine.find_matches(pattern);
        timings.matching += match_start.elapsed();
        let candidates = match candidates {
            Ok(c) => c,
            Err(source_err) => {
                case_errors.push(
                    BenchmarkError::Matching {
                        case_id: case.id.0.clone(),
                        pattern_id: pattern.id.0.clone(),
                        source: source_err,
                    }
                    .to_string(),
                );
                continue;
            }
        };

        if candidates.is_empty() {
            pattern_results.push(PatternResult {
                pattern_id: pattern.id.clone(),
                pattern_version: pattern.version,
                candidate_count: 0,
                actual: PatternOutcome::NoMatch,
                expected: expected_for(&case.expected, &pattern.id),
                grounding_results: Vec::new(),
                taxonomy_reports: Vec::new(),
            });
            continue;
        }

        let ground_start = Instant::now();
        let grounded = grounding::ground_all(&candidates, trace_ref, std::slice::from_ref(pattern));
        timings.grounding += ground_start.elapsed();
        let grounded = match grounded {
            Ok(g) => g,
            Err(source_err) => {
                case_errors.push(
                    BenchmarkError::Grounding {
                        case_id: case.id.0.clone(),
                        pattern_id: pattern.id.0.clone(),
                        source: source_err,
                    }
                    .to_string(),
                );
                continue;
            }
        };

        let taxonomy_start = Instant::now();
        let taxonomy_reports = taxonomy_engine.map_all(&grounded);
        timings.taxonomy += taxonomy_start.elapsed();

        let actual = grounded
            .iter()
            .map(|g| PatternOutcome::from_grounding_status(g.status))
            .fold(PatternOutcome::NoMatch, PatternOutcome::strongest);

        pattern_results.push(PatternResult {
            pattern_id: pattern.id.clone(),
            pattern_version: pattern.version,
            candidate_count: candidates.len(),
            actual,
            expected: expected_for(&case.expected, &pattern.id),
            grounding_results: grounded,
            taxonomy_reports,
        });
    }

    Ok(CaseResult {
        case_id: case.id.clone(),
        description: case.description.clone(),
        pattern_results,
        timings,
        error: if case_errors.is_empty() {
            None
        } else {
            Some(case_errors.join("; "))
        },
    })
}

fn expected_for(expected: &[ExpectedFinding], pattern_id: &PatternId) -> Option<PatternOutcome> {
    expected
        .iter()
        .find(|e| &e.pattern_id == pattern_id)
        .map(|e| e.expected)
}

/// Run every case in `suite`, in order, producing one [`CaseResult`]
/// per case.
///
/// Like [`run_case`], a failing case does not abort the suite: its
/// failure is captured on its own [`CaseResult`] and the remaining
/// cases still run.
///
/// # Errors
/// Returns [`BenchmarkError::DuplicateCaseId`] if `suite` fails
/// [`BenchmarkSuite::validate`] before any case is run.
pub fn run_suite(suite: &BenchmarkSuite) -> Result<Vec<CaseResult>, BenchmarkError> {
    suite.validate()?;
    suite.cases.iter().map(run_case).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample_fixtures;

    #[test]
    fn grounded_reentrancy_runs_end_to_end() {
        let case = sample_fixtures::grounded_reentrancy_case();
        let result = run_case(&case).expect("run_case is infallible");
        assert!(result.error.is_none());
        assert_eq!(result.pattern_results.len(), 1);
        let pr = &result.pattern_results[0];
        assert_eq!(pr.actual, PatternOutcome::Grounded);
        assert_eq!(pr.expected, Some(PatternOutcome::Grounded));
        assert_eq!(pr.matches_expectation(), Some(true));
        assert!(result.passed());
    }

    #[test]
    fn abstained_oracle_manipulation_runs_end_to_end() {
        let case = sample_fixtures::abstained_oracle_manipulation_case();
        let result = run_case(&case).expect("run_case is infallible");
        assert!(result.error.is_none());
        let pr = &result.pattern_results[0];
        assert_eq!(pr.actual, PatternOutcome::Abstain);
        assert!(result.passed());
    }

    #[test]
    fn no_match_case_runs_end_to_end() {
        let case = sample_fixtures::no_match_case();
        let result = run_case(&case).expect("run_case is infallible");
        assert!(result.error.is_none());
        let pr = &result.pattern_results[0];
        assert_eq!(pr.actual, PatternOutcome::NoMatch);
        assert_eq!(pr.candidate_count, 0);
        assert!(pr.grounding_results.is_empty());
        assert!(result.passed());
    }

    #[test]
    fn false_positive_candidate_is_caught_as_ungrounded() {
        let (trace, pattern, fabricated) = sample_fixtures::false_positive_ungrounded_fixture();
        let grounded = grounding::ground_all(
            std::slice::from_ref(&fabricated),
            &trace,
            std::slice::from_ref(&pattern),
        )
        .expect("grounding a single fabricated candidate should not error");
        assert_eq!(grounded.len(), 1);
        assert_eq!(grounded[0].status, grounding::GroundingStatus::Ungrounded);
    }

    #[test]
    fn unscored_pattern_result_counts_as_passed() {
        let mut case = sample_fixtures::grounded_reentrancy_case();
        case.expected.clear();
        let result = run_case(&case).expect("run_case is infallible");
        assert_eq!(result.pattern_results[0].expected, None);
        assert_eq!(result.pattern_results[0].matches_expectation(), None);
        assert!(result.passed());
    }

    #[test]
    fn suite_run_reports_every_case() {
        let suite = crate::types::BenchmarkSuite::from_cases(
            "smoke",
            vec![
                sample_fixtures::grounded_reentrancy_case(),
                sample_fixtures::abstained_oracle_manipulation_case(),
                sample_fixtures::no_match_case(),
            ],
        );
        let results = run_suite(&suite).expect("valid suite");
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.error.is_none()));
    }

    #[test]
    fn ingestion_failure_is_captured_on_case_result_not_returned_as_error() {
        let case = crate::types::BenchmarkCase::from_bytes(
            "broken",
            "not valid JSON",
            b"not json at all".to_vec(),
            "test",
            Vec::new(),
            Vec::new(),
        );
        let result = run_case(&case).expect("run_case captures failures on the result");
        assert!(result.error.is_some());
        assert!(result.pattern_results.is_empty());
        assert!(!result.passed());
    }
}

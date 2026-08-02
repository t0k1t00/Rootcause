//! JSON export.
//!
//! None of `matcher::CandidateMatch`, `grounding::GroundingResult`, or
//! `taxonomy::TaxonomyReport` implement `serde::Serialize` (several of
//! their own fields — `grounding::GroundingStatus` in particular —
//! deliberately don't either, since those crates were not required to
//! justify carrying a `serde` dependency for a public shape they don't
//! otherwise need). Rather than adding `Serialize` impls this crate
//! does not own to those upstream types, this module builds its own
//! flat, owned `ReportView` tree — strings and numbers only — that
//! *this* crate does own and can freely derive `Serialize` for.

use serde::Serialize;

use crate::report::BenchmarkReport;

#[derive(Serialize)]
struct ReportView {
    suite_name: String,
    cases_run: usize,
    cases_passed: usize,
    cases_errored: usize,
    total_timings_ms: TimingsView,
    metrics: MetricsView,
    cases: Vec<CaseView>,
}

#[derive(Serialize)]
#[allow(clippy::struct_field_names)] // every field is deliberately unit-suffixed (`_ms`)
struct TimingsView {
    ingestion_ms: f64,
    matching_ms: f64,
    grounding_ms: f64,
    taxonomy_ms: f64,
    total_ms: f64,
}

impl From<crate::timing::StageTimings> for TimingsView {
    fn from(t: crate::timing::StageTimings) -> Self {
        Self {
            ingestion_ms: t.ingestion.as_secs_f64() * 1000.0,
            matching_ms: t.matching.as_secs_f64() * 1000.0,
            grounding_ms: t.grounding.as_secs_f64() * 1000.0,
            taxonomy_ms: t.taxonomy.as_secs_f64() * 1000.0,
            total_ms: t.total().as_secs_f64() * 1000.0,
        }
    }
}

#[derive(Serialize)]
struct MetricsView {
    precision: f64,
    recall: f64,
    f1: f64,
    true_positives: usize,
    false_positives: usize,
    false_negatives: usize,
    true_negatives: usize,
    accuracy: f64,
    confusion_matrix: Vec<ConfusionCellView>,
    per_pattern: Vec<PatternStatsView>,
    per_family: Vec<FamilyStatsView>,
    failures: Vec<FailureView>,
}

#[derive(Serialize)]
struct ConfusionCellView {
    expected: String,
    actual: String,
    count: usize,
}

#[derive(Serialize)]
struct PatternStatsView {
    pattern_id: String,
    runs: usize,
    passed: usize,
    total_candidates: usize,
    accuracy: f64,
}

#[derive(Serialize)]
struct FamilyStatsView {
    family: String,
    runs: usize,
    passed: usize,
    accuracy: f64,
}

#[derive(Serialize)]
struct FailureView {
    case_id: String,
    pattern_id: Option<String>,
    detail: String,
}

#[derive(Serialize)]
struct CaseView {
    case_id: String,
    description: String,
    passed: bool,
    error: Option<String>,
    timings_ms: TimingsView,
    patterns: Vec<PatternResultView>,
}

#[derive(Serialize)]
struct PatternResultView {
    pattern_id: String,
    pattern_version: u32,
    candidate_count: usize,
    actual: String,
    expected: Option<String>,
    matches_expectation: Option<bool>,
    grounding_statuses: Vec<String>,
    taxonomy_entries: Vec<TaxonomyEntryView>,
}

#[derive(Serialize)]
struct TaxonomyEntryView {
    taxonomy: &'static str,
    entry_id: String,
    title: String,
}

#[allow(clippy::too_many_lines)] // one flat, mechanical field-by-field projection
fn build_view(report: &BenchmarkReport) -> ReportView {
    let confusion_matrix = crate::metrics::ALL_OUTCOMES
        .iter()
        .flat_map(|expected| {
            crate::metrics::ALL_OUTCOMES.iter().map(move |actual| {
                let count = report.metrics.confusion.get(*expected, *actual);
                (*expected, *actual, count)
            })
        })
        .filter(|(_, _, count)| *count > 0)
        .map(|(expected, actual, count)| ConfusionCellView {
            expected: expected.to_string(),
            actual: actual.to_string(),
            count,
        })
        .collect();

    let per_pattern = report
        .metrics
        .per_pattern
        .iter()
        .map(|(id, stats)| PatternStatsView {
            pattern_id: id.0.clone(),
            runs: stats.runs,
            passed: stats.passed,
            total_candidates: stats.total_candidates,
            accuracy: stats.confusion.accuracy(),
        })
        .collect();

    let per_family = report
        .metrics
        .per_family
        .iter()
        .map(|(family, stats)| FamilyStatsView {
            family: family.0.clone(),
            runs: stats.runs,
            passed: stats.passed,
            accuracy: stats.confusion.accuracy(),
        })
        .collect();

    let failures = report
        .metrics
        .failures
        .iter()
        .map(|f| FailureView {
            case_id: f.case_id.0.clone(),
            pattern_id: f.pattern_id.as_ref().map(|p| p.0.clone()),
            detail: f.detail.clone(),
        })
        .collect();

    let cases = report
        .case_results
        .iter()
        .map(|case| CaseView {
            case_id: case.case_id.0.clone(),
            description: case.description.clone(),
            passed: case.passed(),
            error: case.error.clone(),
            timings_ms: case.timings.into(),
            patterns: case
                .pattern_results
                .iter()
                .map(|pr| PatternResultView {
                    pattern_id: pr.pattern_id.0.clone(),
                    pattern_version: pr.pattern_version.0,
                    candidate_count: pr.candidate_count,
                    actual: pr.actual.to_string(),
                    expected: pr.expected.map(|e| e.to_string()),
                    matches_expectation: pr.matches_expectation(),
                    grounding_statuses: pr
                        .grounding_results
                        .iter()
                        .map(|g| g.status.to_string())
                        .collect(),
                    taxonomy_entries: pr
                        .taxonomy_reports
                        .iter()
                        .flat_map(|t| t.mappings.iter())
                        .flat_map(|m| {
                            let taxonomy = m.taxonomy;
                            m.entries.iter().map(move |e| TaxonomyEntryView {
                                taxonomy,
                                entry_id: e.code.clone(),
                                title: e.title.clone(),
                            })
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect();

    ReportView {
        suite_name: report.suite_name.clone(),
        cases_run: report.cases_run,
        cases_passed: report.cases_passed(),
        cases_errored: report.cases_errored,
        total_timings_ms: report.total_timings.into(),
        metrics: MetricsView {
            precision: report.metrics.overall.precision(),
            recall: report.metrics.overall.recall(),
            f1: report.metrics.overall.f1(),
            true_positives: report.metrics.overall.true_positives,
            false_positives: report.metrics.overall.false_positives,
            false_negatives: report.metrics.overall.false_negatives,
            true_negatives: report.metrics.overall.true_negatives,
            accuracy: report.metrics.confusion.accuracy(),
            confusion_matrix,
            per_pattern,
            per_family,
            failures,
        },
        cases,
    }
}

/// Serialize `report` to a pretty-printed JSON string.
///
/// # Errors
/// Returns [`crate::error::BenchmarkError::Export`] if serialization
/// fails (unreachable for any [`BenchmarkReport`] this crate itself
/// constructs, since `ReportView` contains no map with non-string
/// keys or other `serde_json`-incompatible shape; kept fallible so a
/// future field addition that could fail does not require a breaking
/// signature change).
pub fn to_json(report: &BenchmarkReport) -> Result<String, crate::error::BenchmarkError> {
    let view = build_view(report);
    serde_json::to_string_pretty(&view).map_err(|e| crate::error::BenchmarkError::Export {
        format: "json",
        reason: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::sample_fixtures;
    use crate::types::BenchmarkSuite;

    fn sample_report() -> BenchmarkReport {
        let suite = BenchmarkSuite::from_cases(
            "json-export-suite",
            vec![
                sample_fixtures::grounded_reentrancy_case(),
                sample_fixtures::abstained_oracle_manipulation_case(),
                sample_fixtures::no_match_case(),
            ],
        );
        BenchmarkReport::run(&suite).expect("sample suite should run")
    }

    #[test]
    fn to_json_produces_valid_parseable_json() {
        let report = sample_report();
        let json = to_json(&report).expect("export should not fail");
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("output must be valid JSON");
        assert_eq!(value["suite_name"], "json-export-suite");
        assert_eq!(value["cases_run"], 3);
        assert_eq!(value["cases_errored"], 0);
        assert_eq!(value["cases"].as_array().expect("cases array").len(), 3);
    }

    #[test]
    fn to_json_reports_metrics_and_confusion_matrix() {
        let report = sample_report();
        let json = to_json(&report).expect("export should not fail");
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

        let metrics = &value["metrics"];
        assert!(metrics["precision"].is_number());
        assert!(metrics["recall"].is_number());
        assert!(metrics["f1"].is_number());
        assert!(!metrics["confusion_matrix"]
            .as_array()
            .expect("confusion matrix array")
            .is_empty());
    }

    #[test]
    fn to_json_is_pretty_printed_with_indentation() {
        let report = sample_report();
        let json = to_json(&report).expect("export should not fail");
        assert!(json.contains("\n  "));
    }
}

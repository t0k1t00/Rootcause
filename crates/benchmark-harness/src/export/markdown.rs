//! Markdown export: a human-readable summary report.

use std::fmt::Write as _;

use crate::report::BenchmarkReport;

/// Render `report` as a Markdown document: a summary table, overall
/// metrics, a confusion matrix, per-pattern/per-family breakdowns, and
/// a failure list.
///
/// # Errors
/// Infallible in practice (only writes to an in-memory `String`);
/// returns `Result` for the same forward-compatibility reason as
/// [`crate::export::to_csv`].
#[allow(clippy::too_many_lines)] // one flat, mechanical section-by-section render
pub fn to_markdown(report: &BenchmarkReport) -> Result<String, crate::error::BenchmarkError> {
    let mut out = String::new();
    let m = &report.metrics;

    let _ = writeln!(out, "# Benchmark Report: {}", report.suite_name);
    let _ = writeln!(out);
    let _ = writeln!(out, "## Summary");
    let _ = writeln!(out);
    let _ = writeln!(out, "| Metric | Value |");
    let _ = writeln!(out, "|---|---|");
    let _ = writeln!(out, "| Cases run | {} |", report.cases_run);
    let _ = writeln!(out, "| Cases passed | {} |", report.cases_passed());
    let _ = writeln!(out, "| Cases errored | {} |", report.cases_errored);
    let _ = writeln!(out, "| Precision | {:.3} |", m.overall.precision());
    let _ = writeln!(out, "| Recall | {:.3} |", m.overall.recall());
    let _ = writeln!(out, "| F1 | {:.3} |", m.overall.f1());
    let _ = writeln!(out, "| Accuracy | {:.3} |", m.confusion.accuracy());
    let _ = writeln!(
        out,
        "| Total time | {:.3} ms |",
        report.total_timings.total().as_secs_f64() * 1000.0
    );
    let _ = writeln!(out);

    let _ = writeln!(out, "## Stage timings");
    let _ = writeln!(out);
    let _ = writeln!(out, "| Stage | Time (ms) |");
    let _ = writeln!(out, "|---|---|");
    let _ = writeln!(
        out,
        "| Ingestion | {:.3} |",
        report.total_timings.ingestion.as_secs_f64() * 1000.0
    );
    let _ = writeln!(
        out,
        "| Matching | {:.3} |",
        report.total_timings.matching.as_secs_f64() * 1000.0
    );
    let _ = writeln!(
        out,
        "| Grounding | {:.3} |",
        report.total_timings.grounding.as_secs_f64() * 1000.0
    );
    let _ = writeln!(
        out,
        "| Taxonomy | {:.3} |",
        report.total_timings.taxonomy.as_secs_f64() * 1000.0
    );
    let _ = writeln!(out);

    let _ = writeln!(out, "## Confusion matrix");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "| Expected \\ Actual | Grounded | Abstain | Ungrounded | No-match |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|");
    for expected in crate::metrics::ALL_OUTCOMES {
        let _ = write!(out, "| {expected} |");
        for actual in crate::metrics::ALL_OUTCOMES {
            let _ = write!(out, " {} |", m.confusion.get(expected, actual));
        }
        let _ = writeln!(out);
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## Per-pattern statistics");
    let _ = writeln!(out);
    let _ = writeln!(out, "| Pattern | Runs | Passed | Candidates | Accuracy |");
    let _ = writeln!(out, "|---|---|---|---|---|");
    for (id, stats) in &m.per_pattern {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {:.3} |",
            id.0,
            stats.runs,
            stats.passed,
            stats.total_candidates,
            stats.confusion.accuracy()
        );
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## Per-family statistics");
    let _ = writeln!(out);
    let _ = writeln!(out, "| Family | Runs | Passed | Accuracy |");
    let _ = writeln!(out, "|---|---|---|---|");
    for (family, stats) in &m.per_family {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {:.3} |",
            family.0,
            stats.runs,
            stats.passed,
            stats.confusion.accuracy()
        );
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## Failures");
    let _ = writeln!(out);
    if m.failures.is_empty() {
        let _ = writeln!(out, "None.");
    } else {
        let _ = writeln!(out, "| Case | Pattern | Detail |");
        let _ = writeln!(out, "|---|---|---|");
        for f in &m.failures {
            let _ = writeln!(
                out,
                "| {} | {} | {} |",
                f.case_id,
                f.pattern_id
                    .as_ref()
                    .map_or("(case-level)", |p| p.0.as_str()),
                f.detail
            );
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::sample_fixtures;
    use crate::types::BenchmarkSuite;

    #[test]
    fn to_markdown_renders_title_and_summary_table() {
        let suite = BenchmarkSuite::from_cases(
            "md-suite",
            vec![sample_fixtures::grounded_reentrancy_case()],
        );
        let report = BenchmarkReport::run(&suite).expect("sample suite should run");
        let md = to_markdown(&report).expect("export should not fail");

        assert!(md.starts_with("# Benchmark Report: md-suite"));
        assert!(md.contains("## Summary"));
        assert!(md.contains("| Cases run | 1 |"));
        assert!(md.contains("| Cases passed | 1 |"));
        assert!(md.contains("| Cases errored | 0 |"));
    }

    #[test]
    fn to_markdown_reports_no_failures_when_every_case_passes() {
        let suite = BenchmarkSuite::from_cases(
            "md-suite-clean",
            vec![
                sample_fixtures::grounded_reentrancy_case(),
                sample_fixtures::abstained_oracle_manipulation_case(),
                sample_fixtures::no_match_case(),
            ],
        );
        let report = BenchmarkReport::run(&suite).expect("sample suite should run");
        let md = to_markdown(&report).expect("export should not fail");

        assert!(md.contains("## Failures"));
        assert!(md.contains("None."));
    }

    #[test]
    fn to_markdown_lists_a_case_level_failure() {
        let broken = crate::types::BenchmarkCase::from_bytes(
            "broken",
            "not valid JSON",
            b"not json at all".to_vec(),
            "test",
            Vec::new(),
            Vec::new(),
        );
        let suite = BenchmarkSuite::from_cases("md-suite-broken", vec![broken]);
        let report = BenchmarkReport::run(&suite).expect("suite validation should pass");
        let md = to_markdown(&report).expect("export should not fail");

        assert!(md.contains("| Case | Pattern | Detail |"));
        assert!(md.contains("broken"));
        assert!(md.contains("(case-level)"));
    }

    #[test]
    fn to_markdown_includes_confusion_matrix_and_stage_timings_sections() {
        let suite = BenchmarkSuite::from_cases(
            "md-suite-sections",
            vec![sample_fixtures::grounded_reentrancy_case()],
        );
        let report = BenchmarkReport::run(&suite).expect("sample suite should run");
        let md = to_markdown(&report).expect("export should not fail");

        assert!(md.contains("## Stage timings"));
        assert!(md.contains("## Confusion matrix"));
        assert!(md.contains("## Per-pattern statistics"));
        assert!(md.contains("## Per-family statistics"));
    }
}

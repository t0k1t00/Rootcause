//! CSV export: one row per (case, pattern) result.
//!
//! Hand-rolled rather than pulling in a `csv` crate dependency: the
//! schema is fixed and small (see `HEADER`), and every field this
//! module writes is either a number or a value drawn from a closed,
//! comma/quote-free vocabulary (pattern ids are DSL identifiers,
//! [`crate::types::PatternOutcome`] renders as a fixed lowercase word)
//! — except free-text fields (case description, error detail), which
//! are escaped per RFC 4180 by `escape`.

use std::fmt::Write as _;

use crate::report::BenchmarkReport;

const HEADER: &str = "case_id,description,pattern_id,pattern_version,candidate_count,expected,actual,matches_expectation,ingestion_ms,matching_ms,grounding_ms,taxonomy_ms,error";

/// RFC 4180 field escaping: wrap in quotes and double any embedded
/// quote whenever the field contains a comma, quote, or newline.
fn escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// Render `report` as CSV, one row per (case, pattern) result. A case
/// that errored before any pattern ran still gets exactly one row (its
/// pattern fields empty, its `error` column populated).
///
/// # Errors
/// This function is infallible in practice (it only ever writes to an
/// in-memory `String`) but returns a `Result` for consistency with
/// [`crate::export::to_json`] and [`crate::export::to_markdown`], and
/// to leave room for a future streaming writer that could fail on I/O.
pub fn to_csv(report: &BenchmarkReport) -> Result<String, crate::error::BenchmarkError> {
    let mut out = String::new();
    out.push_str(HEADER);
    out.push('\n');

    for case in &report.case_results {
        if case.pattern_results.is_empty() {
            let _ = writeln!(
                out,
                "{},{},,,,,,,{:.3},{:.3},{:.3},{:.3},{}",
                escape(&case.case_id.0),
                escape(&case.description),
                case.timings.ingestion.as_secs_f64() * 1000.0,
                case.timings.matching.as_secs_f64() * 1000.0,
                case.timings.grounding.as_secs_f64() * 1000.0,
                case.timings.taxonomy.as_secs_f64() * 1000.0,
                escape(case.error.as_deref().unwrap_or("")),
            );
            continue;
        }

        for pr in &case.pattern_results {
            let _ = writeln!(
                out,
                "{},{},{},{},{},{},{},{},{:.3},{:.3},{:.3},{:.3},{}",
                escape(&case.case_id.0),
                escape(&case.description),
                escape(&pr.pattern_id.0),
                pr.pattern_version.0,
                pr.candidate_count,
                pr.expected.map_or_else(String::new, |e| e.to_string()),
                pr.actual,
                pr.matches_expectation()
                    .map_or_else(String::new, |b| b.to_string()),
                case.timings.ingestion.as_secs_f64() * 1000.0,
                case.timings.matching.as_secs_f64() * 1000.0,
                case.timings.grounding.as_secs_f64() * 1000.0,
                case.timings.taxonomy.as_secs_f64() * 1000.0,
                escape(case.error.as_deref().unwrap_or("")),
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
    fn escape_wraps_fields_containing_comma_quote_or_newline() {
        assert_eq!(escape("plain"), "plain");
        assert_eq!(escape("a,b"), "\"a,b\"");
        assert_eq!(escape("a\"b"), "\"a\"\"b\"");
        assert_eq!(escape("a\nb"), "\"a\nb\"");
    }

    #[test]
    fn to_csv_starts_with_the_declared_header() {
        let suite = BenchmarkSuite::from_cases(
            "csv-suite",
            vec![sample_fixtures::grounded_reentrancy_case()],
        );
        let report = BenchmarkReport::run(&suite).expect("sample suite should run");
        let csv = to_csv(&report).expect("export should not fail");
        assert!(csv.starts_with(HEADER));
    }

    #[test]
    fn to_csv_emits_one_row_per_pattern_result() {
        let suite = BenchmarkSuite::from_cases(
            "csv-suite",
            vec![
                sample_fixtures::grounded_reentrancy_case(),
                sample_fixtures::abstained_oracle_manipulation_case(),
                sample_fixtures::no_match_case(),
            ],
        );
        let report = BenchmarkReport::run(&suite).expect("sample suite should run");
        let csv = to_csv(&report).expect("export should not fail");
        // Header + exactly one data row per case (each sample fixture
        // case runs exactly one pattern).
        assert_eq!(csv.lines().count(), 1 + 3);
        assert!(csv.contains("grounded_reentrancy"));
        assert!(csv.contains("abstained_oracle_manipulation"));
        assert!(csv.contains("no_match"));
    }

    #[test]
    fn to_csv_gives_a_case_level_error_exactly_one_row() {
        let broken = crate::types::BenchmarkCase::from_bytes(
            "broken",
            "not valid JSON",
            b"not json at all".to_vec(),
            "test",
            Vec::new(),
            Vec::new(),
        );
        let suite = BenchmarkSuite::from_cases("csv-error-suite", vec![broken]);
        let report = BenchmarkReport::run(&suite).expect("suite validation should pass");
        let csv = to_csv(&report).expect("export should not fail");
        assert_eq!(csv.lines().count(), 1 + 1);
        let data_line = csv.lines().nth(1).expect("one data row");
        assert!(data_line.starts_with("broken,"));
        assert!(data_line.contains("ingestion failed"));
    }
}

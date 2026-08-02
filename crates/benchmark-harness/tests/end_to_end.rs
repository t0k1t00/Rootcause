//! End-to-end integration tests: load real fixture files from disk,
//! run them through the full pipeline, and export the resulting
//! report.
//!
//! This file is its own compilation unit (a `tests/` integration test
//! binary), so it does not inherit `src/lib.rs`'s `cfg(test)` lint
//! exception — restated here for the same reason `integration-tests`
//! and `ingestion`'s own fixture tests do: test code's canonical
//! failure mode is panicking (`assert!`, `.expect()`), so applying the
//! library-code `unwrap_used`/`expect_used`/`panic` lints here would
//! fight the standard test idiom rather than serve its purpose.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use benchmark_harness::{
    export, fixtures, report::BenchmarkReport, types::BenchmarkSuite, PatternOutcome,
};

fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reentrancy_basic")
}

#[test]
fn load_suite_from_dir_and_run_end_to_end() {
    let suite =
        fixtures::load_suite_from_dir(fixtures_dir()).expect("fixture directory should load");
    assert_eq!(suite.cases.len(), 1);
    assert_eq!(suite.cases[0].id.0, "reentrancy_basic");

    let report = BenchmarkReport::run(&suite).expect("suite should run end to end");
    assert_eq!(report.cases_run, 1);
    assert_eq!(report.cases_errored, 0);
    assert_eq!(report.cases_passed(), 1);

    let case = &report.case_results[0];
    assert!(case.error.is_none());
    assert_eq!(case.pattern_results.len(), 1);
    let pr = &case.pattern_results[0];
    assert_eq!(pr.actual, PatternOutcome::NoMatch);
    assert_eq!(pr.candidate_count, 0);
    assert_eq!(pr.matches_expectation(), Some(true));
}

#[test]
fn load_case_directly_from_the_fixture_directory() {
    let case_path = fixtures_dir().join("case.json");
    let case = fixtures::load_case(case_path).expect("case definition should load");
    assert_eq!(case.id.0, "reentrancy_basic");
    assert_eq!(case.patterns.len(), 1);
    assert_eq!(case.expected.len(), 1);
    assert_eq!(case.expected[0].expected, PatternOutcome::NoMatch);
}

#[test]
fn full_report_exports_to_all_three_formats() {
    let suite =
        fixtures::load_suite_from_dir(fixtures_dir()).expect("fixture directory should load");
    let report = BenchmarkReport::run(&suite).expect("suite should run end to end");

    let json = export::to_json(&report).expect("JSON export should not fail");
    let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(value["suite_name"], "reentrancy_basic");
    assert_eq!(value["cases_run"], 1);

    let csv = export::to_csv(&report).expect("CSV export should not fail");
    assert!(csv.contains("reentrancy_basic"));
    assert!(csv.contains("no-match"));

    let markdown = export::to_markdown(&report).expect("Markdown export should not fail");
    assert!(markdown.starts_with("# Benchmark Report: reentrancy_basic"));
    assert!(markdown.contains("None.")); // no failures
}

#[test]
fn sample_fixtures_suite_runs_and_exports_consistently_across_formats() {
    // Exercises the in-code fixtures (crate::sample_fixtures) alongside
    // the on-disk ones above, covering both supported fixture sources
    // through the same export pipeline.
    let suite = BenchmarkSuite::from_cases(
        "sample-fixtures-suite",
        vec![
            benchmark_harness::sample_fixtures::grounded_reentrancy_case(),
            benchmark_harness::sample_fixtures::abstained_oracle_manipulation_case(),
            benchmark_harness::sample_fixtures::no_match_case(),
        ],
    );
    let report = BenchmarkReport::run(&suite).expect("suite should run end to end");
    assert_eq!(report.cases_passed(), 3);

    let json = export::to_json(&report).expect("JSON export should not fail");
    let csv = export::to_csv(&report).expect("CSV export should not fail");
    let markdown = export::to_markdown(&report).expect("Markdown export should not fail");

    // All three formats agree on how many cases ran.
    let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(value["cases_run"], 3);
    assert_eq!(csv.lines().count(), 1 + 3); // header + one row per case
    assert!(markdown.contains("| Cases run | 3 |"));
}

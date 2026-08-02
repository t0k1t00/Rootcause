//! `rootcause benchmark`: load a benchmark suite, run it through
//! `benchmark-harness`, and export the resulting
//! [`benchmark_harness::BenchmarkReport`].
//!
//! Every format this command supports (JSON, CSV, Markdown) is rendered
//! by `benchmark-harness`'s own `export` module — this file only
//! decides *which* suite to load and *which* exporter to call, per the
//! Task's "must not duplicate any logic already implemented elsewhere"
//! instruction. The one format `benchmark-harness` does not itself
//! render, human-readable console text, is a thin, deliberately
//! non-duplicative summary built directly from
//! [`benchmark_harness::BenchmarkReport`]'s own already-computed
//! fields (see [`render_human`]).

use std::fmt::Write as _;
use std::path::Path;

use benchmark_harness::BenchmarkReport;

use crate::error::CliError;
use crate::output::{Format, Logger};

/// Run `rootcause benchmark`: load the suite at `suite_path`, run it,
/// and render the resulting report in `format`.
///
/// `suite_path` may be a single case-definition JSON file or a
/// directory of them — see
/// `benchmark_harness::fixtures::load_suite_from_dir` and
/// `benchmark_harness::fixtures::load_case` for the accepted schema.
///
/// # Errors
/// - [`CliError::InputPath`] if `suite_path` does not exist.
/// - [`CliError::Benchmark`] if loading or running the suite fails
///   (malformed definition, pattern compile failure, duplicate case
///   ids, ...).
/// - [`CliError::Export`] if rendering the report in `format` fails.
pub fn run(suite_path: &Path, format: Format, logger: Logger) -> Result<String, CliError> {
    if !suite_path.exists() {
        return Err(CliError::InputPath {
            path: suite_path.to_path_buf(),
            reason: "no such file or directory".to_string(),
        });
    }

    logger.verbose(format_args!(
        "Loading benchmark suite from `{}`",
        suite_path.display()
    ));
    let suite = if suite_path.is_dir() {
        benchmark_harness::fixtures::load_suite_from_dir(suite_path)?
    } else {
        let case = benchmark_harness::fixtures::load_case(suite_path)?;
        let name = suite_path
            .file_stem()
            .map_or_else(|| "suite".to_string(), |s| s.to_string_lossy().into_owned());
        benchmark_harness::BenchmarkSuite::from_cases(name, vec![case])
    };

    logger.verbose(format_args!(
        "Running {} case(s) through the pipeline",
        suite.cases.len()
    ));
    let report = BenchmarkReport::run(&suite)?;

    logger.info(format_args!(
        "{}/{} cases passed ({} errored)",
        report.cases_passed(),
        report.cases_run,
        report.cases_errored
    ));

    match format {
        Format::Json => benchmark_harness::export::to_json(&report).map_err(|e| CliError::Export {
            format: "json",
            reason: e.to_string(),
        }),
        Format::Csv => benchmark_harness::export::to_csv(&report).map_err(|e| CliError::Export {
            format: "csv",
            reason: e.to_string(),
        }),
        Format::Markdown => {
            benchmark_harness::export::to_markdown(&report).map_err(|e| CliError::Export {
                format: "markdown",
                reason: e.to_string(),
            })
        }
        Format::Human => Ok(render_human(&report)),
    }
}

/// A compact, human-readable console summary of `report`, built
/// entirely from fields [`BenchmarkReport`] and its `metrics` already
/// compute — no re-derivation of any correctness or timing figure.
fn render_human(report: &BenchmarkReport) -> String {
    let mut out = String::new();
    let m = &report.metrics;

    let _ = writeln!(out, "Benchmark: {}", report.suite_name);
    let _ = writeln!(
        out,
        "  {}/{} cases passed ({} errored)",
        report.cases_passed(),
        report.cases_run,
        report.cases_errored
    );
    let _ = writeln!(
        out,
        "  precision {:.3}  recall {:.3}  f1 {:.3}  accuracy {:.3}",
        m.overall.precision(),
        m.overall.recall(),
        m.overall.f1(),
        m.confusion.accuracy()
    );
    let _ = writeln!(
        out,
        "  total time: {:.3} ms (ingestion {:.3}, matching {:.3}, grounding {:.3}, taxonomy {:.3})",
        report.total_timings.total().as_secs_f64() * 1000.0,
        report.total_timings.ingestion.as_secs_f64() * 1000.0,
        report.total_timings.matching.as_secs_f64() * 1000.0,
        report.total_timings.grounding.as_secs_f64() * 1000.0,
        report.total_timings.taxonomy.as_secs_f64() * 1000.0,
    );

    if !m.per_pattern.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "Per-pattern:");
        for (id, stats) in &m.per_pattern {
            let _ = writeln!(
                out,
                "  {} — {}/{} passed, {} candidate(s), accuracy {:.3}",
                id,
                stats.passed,
                stats.runs,
                stats.total_candidates,
                stats.confusion.accuracy()
            );
        }
    }

    if !m.failures.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "Failures:");
        for f in &m.failures {
            let _ = writeln!(
                out,
                "  {} [{}]: {}",
                f.case_id,
                f.pattern_id
                    .as_ref()
                    .map_or("(case-level)", |p| p.0.as_str()),
                f.detail
            );
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(label: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "rootcause-cli-benchmark-test-{}-{label}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    const MINIMAL_TRACE_JSON: &str = r#"{
        "transaction": {"hash": "0x1111111111111111111111111111111111111111111111111111111111111111", "from": "0x0101010101010101010101010101010101010101", "to": "0x0202020202020202020202020202020202020202", "value": "0x0", "nonce": "0x0", "gasUsed": "0x5208", "status": "success"},
        "block": {"number": "0x64", "timestamp": "0x1", "chainId": "0x1", "baseFee": null},
        "root": {"kind": "call", "from": "0x0101010101010101010101010101010101010101", "to": "0x0202020202020202020202020202020202020202", "value": "0x0", "gasLimit": "0x5208", "gasUsed": "0x5208", "succeeded": true}
    }"#;

    fn write_case(dir: &Path) {
        // `trace.dat`, not `trace.json`: `load_suite_from_dir` loads
        // every `*.json` file directly inside its target directory as
        // a case definition (see
        // `benchmark_harness::fixtures`' own documented "Trap"), so a
        // same-directory raw trace fixture must use a non-`.json`
        // extension to avoid being picked up a second time.
        std::fs::write(dir.join("trace.dat"), MINIMAL_TRACE_JSON).unwrap();
        let def = r#"{
            "id": "trivial_case",
            "description": "a trivial case",
            "trace_file": "trace.dat",
            "patterns": [],
            "expected": []
        }"#;
        std::fs::write(dir.join("case.json"), def).unwrap();
    }

    #[test]
    fn missing_suite_path_is_input_error() {
        let dir = scratch_dir("missing");
        let err = run(
            &dir.join("does-not-exist"),
            Format::Human,
            Logger::new(true, false),
        )
        .expect_err("must fail");
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn runs_a_directory_suite_and_renders_every_format() {
        let dir = scratch_dir("dir-suite");
        write_case(&dir);

        for format in [Format::Human, Format::Json, Format::Csv, Format::Markdown] {
            let out =
                run(&dir, format, Logger::new(true, false)).expect("benchmark should succeed");
            assert!(!out.is_empty());
        }
    }

    #[test]
    fn runs_a_single_file_suite() {
        let dir = scratch_dir("file-suite");
        write_case(&dir);
        let case_path = dir.join("case.json");

        let out = run(&case_path, Format::Human, Logger::new(true, false))
            .expect("benchmark should succeed");
        assert!(out.contains("Benchmark:"));
        assert!(out.contains("1/1 cases passed"));
    }
}

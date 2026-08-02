//! `rootcause analyze`: run the full pipeline
//! (ingestion → matcher → grounding → taxonomy) against one trace and
//! report every finding.
//!
//! This module orchestrates only — every pipeline stage's actual logic
//! belongs to its own crate (`ingestion`, `matcher`, `grounding`,
//! `taxonomy`), exactly as the Task requires. What this module owns is
//! the CLI-specific concerns those crates deliberately do not: turning
//! their rich, non-`Serialize` result types into a flat, owned
//! [`AnalyzeReport`] this crate *can* export to JSON/Markdown/human
//! text (the same "build an owned view" pattern
//! `benchmark_harness::export::json` already uses, for the identical
//! reason — see that module's own docs), and reporting progress via
//! [`Logger`].

use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;

use fact_model::TraceSource as FactTraceSource;
use grounding::GroundingResult;
use taxonomy::TaxonomyReport;

use crate::error::CliError;
use crate::output::{Format, Logger};
use crate::patterns;

/// One taxonomy classification attached to a [`Finding`], flattened
/// into owned, `Serialize`-able fields from
/// [`taxonomy::entry::TaxonomyEntry`].
#[derive(Debug, Clone, Serialize)]
pub struct TaxonomyLine {
    /// Which taxonomy this entry belongs to (e.g. `"SCWE"`, `"SWC"`).
    pub taxonomy: &'static str,
    /// The taxonomy's own identifier, e.g. `"SCWE-046"`.
    pub code: String,
    /// A short human-readable title.
    pub title: String,
    /// A stable link to the taxonomy's canonical entry, if known.
    pub reference_url: Option<&'static str>,
}

/// One independently-grounded candidate finding, combining a
/// [`GroundingResult`] with its [`TaxonomyReport`] into one flat,
/// owned, exportable record.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    /// The pattern's identity.
    pub pattern_id: String,
    /// The pattern's version.
    pub pattern_version: u32,
    /// The pattern's exploit family.
    pub pattern_family: String,
    /// The pattern's declared severity.
    pub pattern_severity: String,
    /// A deterministic fingerprint of this specific candidate.
    pub candidate_id: String,
    /// The overall grounding status (`"grounded"`, `"abstain"`, or
    /// `"ungrounded"`).
    pub status: String,
    /// How many required evidence clauses this pattern declares.
    pub required_total: usize,
    /// How many of those independently verified.
    pub required_verified: usize,
    /// How many optional evidence clauses this pattern declares.
    pub optional_total: usize,
    /// How many of those independently verified.
    pub optional_verified: usize,
    /// A human-readable one-paragraph summary of this finding.
    pub explanation: String,
    /// Every reason grounding abstained, if [`Self::status`] is
    /// `"abstain"`. Empty otherwise.
    pub abstain_reasons: Vec<String>,
    /// Every taxonomy classification this finding's pattern family maps
    /// to, across every registered taxonomy.
    pub taxonomy: Vec<TaxonomyLine>,
}

impl Finding {
    fn from_results(grounding: &GroundingResult, taxonomy: &TaxonomyReport) -> Self {
        let taxonomy_lines = taxonomy
            .mappings
            .iter()
            .flat_map(|mapping| {
                mapping.entries.iter().map(|entry| TaxonomyLine {
                    taxonomy: entry.taxonomy,
                    code: entry.code.clone(),
                    title: entry.title.clone(),
                    reference_url: entry.reference_url,
                })
            })
            .collect();

        Self {
            pattern_id: grounding.pattern_id.0.clone(),
            pattern_version: grounding.pattern_version.0,
            pattern_family: grounding.pattern_family.0.clone(),
            pattern_severity: format!("{:?}", grounding.pattern_severity),
            candidate_id: grounding.candidate_id.to_string(),
            status: grounding.status.to_string(),
            required_total: grounding.confidence.required_total,
            required_verified: grounding.confidence.required_verified,
            optional_total: grounding.confidence.optional_total,
            optional_verified: grounding.confidence.optional_verified,
            explanation: grounding.explanation.clone(),
            abstain_reasons: grounding
                .abstain_reasons
                .iter()
                .map(|r| r.detail.clone())
                .collect(),
            taxonomy: taxonomy_lines,
        }
    }
}

/// A summary of the trace [`run`] ran the pipeline against.
#[derive(Debug, Clone, Serialize)]
pub struct TraceSummary {
    /// The trace file path, as given on the command line.
    pub trace_path: String,
    /// The transaction hash.
    pub transaction_hash: String,
    /// The block number the transaction executed in.
    pub block_number: String,
    /// The chain id.
    pub chain_id: String,
    /// How many calls the trace contains.
    pub call_count: usize,
    /// How many storage changes the trace contains.
    pub storage_change_count: usize,
    /// How many logs the trace contains.
    pub log_count: usize,
    /// How many token transfers the trace contains.
    pub token_transfer_count: usize,
}

/// The complete, owned result of `rootcause analyze`.
#[derive(Debug, Clone, Serialize)]
pub struct AnalyzeReport {
    /// A summary of the analyzed trace.
    pub trace: TraceSummary,
    /// How many patterns were matched against the trace.
    pub patterns_run: usize,
    /// How many structural candidates `matcher` found, across every
    /// pattern.
    pub candidate_count: usize,
    /// One [`Finding`] per grounded candidate, in the order `matcher`
    /// and `grounding` produced them.
    pub findings: Vec<Finding>,
}

/// Run `rootcause analyze`: ingest `trace_path`, compile every pattern
/// named by `patterns_path`, match, ground, and taxonomy-map every
/// candidate, and render the result in `format`.
///
/// # Errors
/// - [`CliError::Usage`] if `format` is [`Format::Csv`] (not supported
///   for `analyze`) or `patterns_path` names an empty directory.
/// - [`CliError::InputPath`] if `patterns_path` cannot be read.
/// - [`CliError::Ingestion`] if `trace_path` fails to ingest.
/// - [`CliError::Dsl`] if a pattern fails to compile.
/// - [`CliError::Matcher`] / [`CliError::Grounding`] if those stages
///   fail against otherwise well-formed input.
pub fn run(
    trace_path: &Path,
    patterns_path: &Path,
    format: Format,
    logger: Logger,
) -> Result<String, CliError> {
    if format == Format::Csv {
        return Err(CliError::Usage(
            "CSV output is not supported for `analyze`; use `json`, `markdown`, or `human`"
                .to_string(),
        ));
    }

    logger.verbose(format_args!("Ingesting trace `{}`", trace_path.display()));
    let source = ingestion::FileTraceSource::new(trace_path);
    let provenance = FactTraceSource::ArchiveNodeRpc {
        endpoint_label: format!("cli:{}", trace_path.display()),
    };
    let trace = ingestion::ingest(&source, provenance)?;

    logger.verbose(format_args!(
        "Loading patterns from `{}`",
        patterns_path.display()
    ));
    let compiled_patterns = patterns::load_patterns(patterns_path)?;

    logger.verbose(format_args!(
        "Matching {} pattern(s) against the trace",
        compiled_patterns.len()
    ));
    let engine = matcher::MatchEngine::new(&trace);
    let candidates = engine.find_all_matches(&compiled_patterns)?;

    logger.verbose(format_args!(
        "Grounding {} candidate match(es)",
        candidates.len()
    ));
    let grounded = grounding::ground_all(&candidates, &trace, &compiled_patterns)?;

    logger.verbose("Mapping findings to external taxonomies");
    let taxonomy_engine = taxonomy::MappingEngine::new();
    let taxonomy_reports = taxonomy_engine.map_all(&grounded);

    let findings: Vec<Finding> = grounded
        .iter()
        .zip(taxonomy_reports.iter())
        .map(|(g, t)| Finding::from_results(g, t))
        .collect();

    let report = AnalyzeReport {
        trace: TraceSummary {
            trace_path: trace_path.display().to_string(),
            transaction_hash: trace.metadata.transaction_hash.to_string(),
            block_number: trace.metadata.block_number.to_string(),
            chain_id: trace.metadata.chain_id.to_string(),
            call_count: trace.arena.calls().count(),
            storage_change_count: trace.arena.storage_changes().count(),
            log_count: trace.arena.logs().count(),
            token_transfer_count: trace.arena.token_transfers().count(),
        },
        patterns_run: compiled_patterns.len(),
        candidate_count: candidates.len(),
        findings,
    };

    match format {
        Format::Human => Ok(render_human(&report)),
        Format::Json => render_json(&report),
        Format::Markdown => Ok(render_markdown(&report)),
        Format::Csv => unreachable!("rejected above"),
    }
}

fn render_json(report: &AnalyzeReport) -> Result<String, CliError> {
    serde_json::to_string_pretty(report).map_err(|e| CliError::Export {
        format: "json",
        reason: e.to_string(),
    })
}

fn render_human(report: &AnalyzeReport) -> String {
    let mut out = String::new();
    let t = &report.trace;
    let _ = writeln!(out, "Trace: {}", t.trace_path);
    let _ = writeln!(
        out,
        "  transaction {} (block {}, chain {})",
        t.transaction_hash, t.block_number, t.chain_id
    );
    let _ = writeln!(
        out,
        "  {} call(s), {} storage change(s), {} log(s), {} token transfer(s)",
        t.call_count, t.storage_change_count, t.log_count, t.token_transfer_count
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Ran {} pattern(s), found {} candidate match(es), {} finding(s) after grounding:",
        report.patterns_run,
        report.candidate_count,
        report.findings.len()
    );

    if report.findings.is_empty() {
        let _ = writeln!(out, "  (no candidates found)");
        return out;
    }

    for finding in &report.findings {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "[{}] {} v{} ({}, {})",
            finding.status.to_uppercase(),
            finding.pattern_id,
            finding.pattern_version,
            finding.pattern_family,
            finding.pattern_severity
        );
        let _ = writeln!(out, "  candidate: {}", finding.candidate_id);
        let _ = writeln!(
            out,
            "  required evidence: {}/{} verified; optional: {}/{} verified",
            finding.required_verified,
            finding.required_total,
            finding.optional_verified,
            finding.optional_total
        );
        let _ = writeln!(out, "  {}", finding.explanation);
        if !finding.abstain_reasons.is_empty() {
            let _ = writeln!(out, "  abstain reasons:");
            for reason in &finding.abstain_reasons {
                let _ = writeln!(out, "    - {reason}");
            }
        }
        if !finding.taxonomy.is_empty() {
            let _ = writeln!(out, "  taxonomy:");
            for entry in &finding.taxonomy {
                match entry.reference_url {
                    Some(url) => {
                        let _ = writeln!(
                            out,
                            "    - {} {}: {} ({url})",
                            entry.taxonomy, entry.code, entry.title
                        );
                    }
                    None => {
                        let _ = writeln!(
                            out,
                            "    - {} {}: {}",
                            entry.taxonomy, entry.code, entry.title
                        );
                    }
                }
            }
        }
    }

    out
}

fn render_markdown(report: &AnalyzeReport) -> String {
    let mut out = String::new();
    let t = &report.trace;
    let _ = writeln!(out, "# Root Cause Analysis: {}", t.trace_path);
    let _ = writeln!(out);
    let _ = writeln!(out, "## Trace");
    let _ = writeln!(out);
    let _ = writeln!(out, "| Field | Value |");
    let _ = writeln!(out, "|---|---|");
    let _ = writeln!(out, "| Transaction | {} |", t.transaction_hash);
    let _ = writeln!(out, "| Block | {} |", t.block_number);
    let _ = writeln!(out, "| Chain | {} |", t.chain_id);
    let _ = writeln!(out, "| Calls | {} |", t.call_count);
    let _ = writeln!(out, "| Storage changes | {} |", t.storage_change_count);
    let _ = writeln!(out, "| Logs | {} |", t.log_count);
    let _ = writeln!(out, "| Token transfers | {} |", t.token_transfer_count);
    let _ = writeln!(out);
    let _ = writeln!(out, "## Summary");
    let _ = writeln!(out);
    let _ = writeln!(out, "| Metric | Value |");
    let _ = writeln!(out, "|---|---|");
    let _ = writeln!(out, "| Patterns run | {} |", report.patterns_run);
    let _ = writeln!(out, "| Candidate matches | {} |", report.candidate_count);
    let _ = writeln!(out, "| Findings | {} |", report.findings.len());
    let _ = writeln!(out);
    let _ = writeln!(out, "## Findings");
    let _ = writeln!(out);

    if report.findings.is_empty() {
        let _ = writeln!(out, "No candidates found.");
        return out;
    }

    for finding in &report.findings {
        let _ = writeln!(
            out,
            "### {} v{} — {}",
            finding.pattern_id,
            finding.pattern_version,
            finding.status.to_uppercase()
        );
        let _ = writeln!(out);
        let _ = writeln!(out, "- **Family:** {}", finding.pattern_family);
        let _ = writeln!(out, "- **Severity:** {}", finding.pattern_severity);
        let _ = writeln!(out, "- **Candidate:** {}", finding.candidate_id);
        let _ = writeln!(
            out,
            "- **Required evidence:** {}/{} verified",
            finding.required_verified, finding.required_total
        );
        let _ = writeln!(
            out,
            "- **Optional evidence:** {}/{} verified",
            finding.optional_verified, finding.optional_total
        );
        let _ = writeln!(out, "- **Explanation:** {}", finding.explanation);
        if !finding.abstain_reasons.is_empty() {
            let _ = writeln!(out, "- **Abstain reasons:**");
            for reason in &finding.abstain_reasons {
                let _ = writeln!(out, "  - {reason}");
            }
        }
        if !finding.taxonomy.is_empty() {
            let _ = writeln!(out, "- **Taxonomy:**");
            for entry in &finding.taxonomy {
                match entry.reference_url {
                    Some(url) => {
                        let _ = writeln!(
                            out,
                            "  - {} {}: {} ([reference]({url}))",
                            entry.taxonomy, entry.code, entry.title
                        );
                    }
                    None => {
                        let _ = writeln!(
                            out,
                            "  - {} {}: {}",
                            entry.taxonomy, entry.code, entry.title
                        );
                    }
                }
            }
        }
        let _ = writeln!(out);
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
            "rootcause-cli-analyze-test-{}-{label}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    const TRACE_JSON: &str = r#"{
        "transaction": {"hash": "0x1111111111111111111111111111111111111111111111111111111111111111", "from": "0x0101010101010101010101010101010101010101", "to": "0x0202020202020202020202020202020202020202", "value": "0x0", "nonce": "0x0", "gasUsed": "0x5208", "status": "success"},
        "block": {"number": "0x64", "timestamp": "0x1", "chainId": "0x1", "baseFee": null},
        "root": {"kind": "call", "from": "0x0101010101010101010101010101010101010101", "to": "0x0202020202020202020202020202020202020202", "value": "0x0", "gasLimit": "0x5208", "gasUsed": "0x5208", "succeeded": true}
    }"#;

    const TRIVIAL_PATTERN: &str = r"
        pattern trivial version 1 {
            family: Reentrancy
            severity: Low
            evidence {
                required c: call(kind: External)
            }
            constraint: c
        }
    ";

    #[test]
    fn csv_format_is_rejected() {
        let dir = scratch_dir("csv-reject");
        let trace_path = dir.join("trace.json");
        std::fs::write(&trace_path, TRACE_JSON).unwrap();
        let pattern_path = dir.join("p.rcdsl");
        std::fs::write(&pattern_path, TRIVIAL_PATTERN).unwrap();

        let err = run(
            &trace_path,
            &pattern_path,
            Format::Csv,
            Logger::new(true, false),
        )
        .expect_err("csv must be rejected");
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn analyze_runs_end_to_end_and_renders_json() {
        let dir = scratch_dir("e2e-json");
        let trace_path = dir.join("trace.json");
        std::fs::write(&trace_path, TRACE_JSON).unwrap();
        let pattern_path = dir.join("p.rcdsl");
        std::fs::write(&pattern_path, TRIVIAL_PATTERN).unwrap();

        let out = run(
            &trace_path,
            &pattern_path,
            Format::Json,
            Logger::new(true, false),
        )
        .expect("analyze should succeed");
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid json");
        assert_eq!(parsed["patterns_run"], 1);
        assert!(parsed["trace"]["transaction_hash"].is_string());
    }

    #[test]
    fn analyze_renders_human_output() {
        let dir = scratch_dir("e2e-human");
        let trace_path = dir.join("trace.json");
        std::fs::write(&trace_path, TRACE_JSON).unwrap();
        let pattern_path = dir.join("p.rcdsl");
        std::fs::write(&pattern_path, TRIVIAL_PATTERN).unwrap();

        let out = run(
            &trace_path,
            &pattern_path,
            Format::Human,
            Logger::new(true, false),
        )
        .expect("analyze should succeed");
        assert!(out.starts_with("Trace:"));
        assert!(out.contains("Ran 1 pattern(s)"));
    }

    #[test]
    fn analyze_renders_markdown_output() {
        let dir = scratch_dir("e2e-md");
        let trace_path = dir.join("trace.json");
        std::fs::write(&trace_path, TRACE_JSON).unwrap();
        let pattern_path = dir.join("p.rcdsl");
        std::fs::write(&pattern_path, TRIVIAL_PATTERN).unwrap();

        let out = run(
            &trace_path,
            &pattern_path,
            Format::Markdown,
            Logger::new(true, false),
        )
        .expect("analyze should succeed");
        assert!(out.starts_with("# Root Cause Analysis"));
        assert!(out.contains("## Findings"));
    }

    #[test]
    fn missing_trace_file_is_an_ingestion_error() {
        let dir = scratch_dir("missing-trace");
        let trace_path = dir.join("does-not-exist.json");
        let pattern_path = dir.join("p.rcdsl");
        std::fs::write(&pattern_path, TRIVIAL_PATTERN).unwrap();

        let err = run(
            &trace_path,
            &pattern_path,
            Format::Human,
            Logger::new(true, false),
        )
        .expect_err("must fail");
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn missing_patterns_path_is_a_usage_or_input_error() {
        let dir = scratch_dir("missing-patterns");
        let trace_path = dir.join("trace.json");
        std::fs::write(&trace_path, TRACE_JSON).unwrap();
        let pattern_path = dir.join("does-not-exist.rcdsl");

        let err = run(
            &trace_path,
            &pattern_path,
            Format::Human,
            Logger::new(true, false),
        )
        .expect_err("must fail");
        assert_eq!(err.exit_code(), 1);
    }
}

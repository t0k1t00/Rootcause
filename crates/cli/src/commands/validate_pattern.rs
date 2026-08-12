//! `rootcause validate-pattern`: run every check a pattern author needs
//! before opening a PR, in one command.
//!
//! This orchestrates existing infrastructure only:
//! [`dsl::parse_pattern`] / [`dsl::validate_pattern`] / [`dsl::compile_str`]
//! for syntax, metadata, and compilation; [`taxonomy::MappingEngine`]
//! for taxonomy coverage; and [`matcher::MatchEngine`] +
//! [`grounding::ground_all`] — the identical two calls
//! [`crate::commands::analyze::run`] makes — against a positive and
//! negative demo trace, if one is available. No new pipeline logic is
//! introduced here.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use dsl::ir::CompiledPattern;

use crate::error::CliError;

/// One check's outcome, rendered as a `✓`/`✗` line.
struct CheckResult {
    label: &'static str,
    passed: bool,
    detail: String,
}

impl CheckResult {
    fn render(&self) -> String {
        let mark = if self.passed { "\u{2713}" } else { "\u{2717}" };
        if self.detail.is_empty() {
            format!("{mark} {}", self.label)
        } else {
            format!("{mark} {} — {}", self.label, self.detail)
        }
    }
}

/// Run every `validate-pattern` check against `pattern_path`.
///
/// Positive/negative traces are taken from `positive`/`negative` if
/// given, otherwise looked up by convention at
/// `<workspace-root>/demo/<pattern-file-stem>_positive.json` and
/// `..._negative.json`, where `<workspace-root>` is `pattern_path`'s
/// grandparent directory (i.e. `patterns/x.rcdsl` implies `demo/` as a
/// sibling of `patterns/`) — the same layout [`crate::commands::new_pattern`]
/// scaffolds.
///
/// Returns the rendered report and whether every check passed. This
/// deliberately never returns `Err` for a *failing* check (a pattern
/// with unmapped taxonomy is a valid, reportable outcome, not a CLI
/// usage error) — only for genuine I/O failures reading `pattern_path`
/// itself.
///
/// # Errors
/// [`CliError::InputPath`] if `pattern_path` cannot be read at all.
pub fn run(
    pattern_path: &Path,
    positive: Option<&Path>,
    negative: Option<&Path>,
) -> Result<(String, bool), CliError> {
    let source = std::fs::read_to_string(pattern_path).map_err(|e| CliError::InputPath {
        path: pattern_path.to_path_buf(),
        reason: e.to_string(),
    })?;

    let mut checks = Vec::new();

    // 1. Syntax (lex + parse).
    let ast = match dsl::parse_pattern(&source) {
        Ok(ast) => {
            checks.push(CheckResult {
                label: "syntax",
                passed: true,
                detail: String::new(),
            });
            ast
        }
        Err(e) => {
            checks.push(CheckResult {
                label: "syntax",
                passed: false,
                detail: e.to_string(),
            });
            return Ok((render(pattern_path, &checks), false));
        }
    };

    // 2. Metadata / semantic validation.
    let diagnostics = dsl::validate_pattern(&ast);
    if diagnostics.has_errors() {
        let detail = diagnostics
            .iter()
            .map(|d| d.render(&source))
            .collect::<Vec<_>>()
            .join("; ");
        checks.push(CheckResult {
            label: "metadata",
            passed: false,
            detail,
        });
        return Ok((render(pattern_path, &checks), false));
    }
    let warning_note = if diagnostics.is_empty() {
        String::new()
    } else {
        diagnostics
            .iter()
            .map(|d| d.render(&source))
            .collect::<Vec<_>>()
            .join("; ")
    };
    checks.push(CheckResult {
        label: "metadata",
        passed: true,
        detail: warning_note,
    });

    // Compilation (needed for every later check; not itself one of the
    // eight documented ✓ lines since it can't newly fail once syntax
    // and metadata both passed, but its error is still surfaced if it
    // somehow does).
    let compiled = match dsl::compile_str(&source) {
        Ok(c) => c,
        Err(e) => {
            checks.push(CheckResult {
                label: "compile",
                passed: false,
                detail: e.to_string(),
            });
            return Ok((render(pattern_path, &checks), false));
        }
    };

    // 3. Taxonomy.
    checks.push(taxonomy_check(&compiled));

    // 4/5. Positive and negative demo traces.
    let (positive_path, negative_path) = resolve_demo_trace_paths(pattern_path, positive, negative);

    checks.push(trace_check(
        "positive trace",
        &compiled,
        &positive_path,
        true,
    ));
    checks.push(trace_check(
        "negative trace",
        &compiled,
        &negative_path,
        false,
    ));

    let all_passed = checks.iter().all(|c| c.passed);
    Ok((render(pattern_path, &checks), all_passed))
}

/// Resolve the positive/negative demo trace paths to use for
/// `pattern_path`: the caller-given override if present, otherwise the
/// naming-convention default `demo/<stem>_{positive,negative}.json`
/// next to `pattern_path`'s workspace root (see [`run`]'s own docs).
fn resolve_demo_trace_paths(
    pattern_path: &Path,
    positive: Option<&Path>,
    negative: Option<&Path>,
) -> (PathBuf, PathBuf) {
    let stem = pattern_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("pattern");
    let workspace_root = pattern_path
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."));

    let positive_path = positive.map_or_else(
        || {
            workspace_root
                .join("demo")
                .join(format!("{stem}_positive.json"))
        },
        std::borrow::ToOwned::to_owned,
    );
    let negative_path = negative.map_or_else(
        || {
            workspace_root
                .join("demo")
                .join(format!("{stem}_negative.json"))
        },
        std::borrow::ToOwned::to_owned,
    );
    (positive_path, negative_path)
}

fn taxonomy_check(compiled: &CompiledPattern) -> CheckResult {
    let engine = taxonomy::MappingEngine::new();
    let mappings = engine.map_family(&compiled.family);
    if mappings.is_empty() {
        CheckResult {
            label: "taxonomy",
            passed: false,
            detail: format!(
                "family `{}` is not mapped in any taxonomy — see \
                 docs/PATTERN_AUTHORING_GUIDE.md step 5",
                compiled.family.0
            ),
        }
    } else {
        let names: Vec<&str> = mappings.iter().map(|m| m.taxonomy).collect();
        CheckResult {
            label: "taxonomy",
            passed: true,
            detail: format!("mapped in {}", names.join(", ")),
        }
    }
}

/// Run `compiled` against the trace at `trace_path`, expecting at least
/// one `Grounded` finding if `expect_match` is `true`, or zero if
/// `false`. Missing/unreadable trace files are reported as a failed
/// check with an actionable message, not a hard `CliError` — a
/// not-yet-written demo trace is exactly the state `validate-pattern`
/// exists to catch and report on.
fn trace_check(
    label: &'static str,
    compiled: &CompiledPattern,
    trace_path: &Path,
    expect_match: bool,
) -> CheckResult {
    if !trace_path.exists() {
        return CheckResult {
            label,
            passed: false,
            detail: format!(
                "no trace at `{}` — create one (see docs/PATTERN_AUTHORING_GUIDE.md step 6) \
                 or pass --positive/--negative explicitly",
                trace_path.display()
            ),
        };
    }

    let ingest_source = ingestion::FileTraceSource::new(trace_path);
    let provenance = fact_model::TraceSource::ArchiveNodeRpc {
        endpoint_label: format!("validate-pattern:{}", trace_path.display()),
    };
    let trace = match ingestion::ingest(&ingest_source, provenance) {
        Ok(t) => t,
        Err(e) => {
            return CheckResult {
                label,
                passed: false,
                detail: format!("failed to ingest `{}`: {e}", trace_path.display()),
            };
        }
    };

    let patterns = std::slice::from_ref(compiled);
    let engine = matcher::MatchEngine::new(&trace);
    let candidates = match engine.find_all_matches(patterns) {
        Ok(c) => c,
        Err(e) => {
            return CheckResult {
                label,
                passed: false,
                detail: format!("matcher error: {e}"),
            };
        }
    };
    let grounded = match grounding::ground_all(&candidates, &trace, patterns) {
        Ok(g) => g,
        Err(e) => {
            return CheckResult {
                label,
                passed: false,
                detail: format!("grounding error: {e}"),
            };
        }
    };

    let grounded_count = grounded
        .iter()
        .filter(|g| g.status == grounding::GroundingStatus::Grounded)
        .count();

    let passed = if expect_match {
        grounded_count > 0
    } else {
        grounded_count == 0
    };

    let detail = if expect_match {
        format!(
            "`{}`: {grounded_count} grounded finding(s) (expected \u{2265} 1)",
            trace_path.display()
        )
    } else {
        format!(
            "`{}`: {grounded_count} grounded finding(s) (expected 0)",
            trace_path.display()
        )
    };

    CheckResult {
        label,
        passed,
        detail,
    }
}

fn render(pattern_path: &Path, checks: &[CheckResult]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Validating {}", pattern_path.display());
    for check in checks {
        let _ = writeln!(out, "  {}", check.render());
    }
    let passed = checks.iter().filter(|c| c.passed).count();
    let _ = writeln!(out, "\n{passed}/{} checks passed", checks.len());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(label: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "rootcause-cli-validate-pattern-test-{}-{label}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn malformed_source_fails_syntax_check() {
        let dir = scratch_dir("syntax");
        let path = dir.join("bad.rcdsl");
        std::fs::write(&path, "not a valid pattern").unwrap();
        let (report, passed) = run(&path, None, None).expect("should not error");
        assert!(!passed);
        assert!(report.contains("\u{2717} syntax"));
    }

    #[test]
    fn well_known_pattern_passes_taxonomy_but_may_lack_demo_traces() {
        // classic_reentrancy is a real, shipped pattern with a real
        // taxonomy mapping, so its taxonomy check must pass regardless
        // of where this test runs from.
        let src = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../patterns/classic_reentrancy.rcdsl"),
        )
        .expect("repo fixture must exist");
        let dir = scratch_dir("known-pattern");
        let path = dir.join("classic_reentrancy.rcdsl");
        std::fs::write(&path, src).expect("write scratch fixture");
        let (report, _passed) = run(&path, None, None).expect("should not error");
        assert!(report.contains("\u{2713} syntax"));
        assert!(report.contains("\u{2713} metadata"));
        assert!(report.contains("\u{2713} taxonomy"));
    }

    #[test]
    fn missing_pattern_file_is_an_input_path_error() {
        let dir = scratch_dir("missing");
        let err = run(&dir.join("nope.rcdsl"), None, None).expect_err("must error");
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn explicit_positive_and_negative_paths_are_used_when_given() {
        let dir = scratch_dir("explicit-traces");
        let pattern_src = r"
            pattern trivial version 1 {
                family: Reentrancy
                severity: Low
                evidence {
                    required c: call(kind: External)
                }
                constraint: c
            }
        ";
        let pattern_path = dir.join("trivial.rcdsl");
        std::fs::write(&pattern_path, pattern_src).unwrap();

        let missing_positive = dir.join("does-not-exist-positive.json");
        let missing_negative = dir.join("does-not-exist-negative.json");
        let (report, passed) = run(
            &pattern_path,
            Some(&missing_positive),
            Some(&missing_negative),
        )
        .expect("should not error");
        assert!(!passed);
        assert!(report.contains(&missing_positive.display().to_string()));
        assert!(report.contains(&missing_negative.display().to_string()));
    }
}

//! `rootcause doctor`: scan the whole pattern library for cross-file
//! authoring mistakes that `validate-pattern` — which only ever looks
//! at one pattern at a time — cannot catch: duplicate pattern ids,
//! patterns with no taxonomy mapping, and patterns whose conventional
//! demo trace files are missing.
//!
//! Like [`crate::commands::validate_pattern`], this orchestrates
//! existing infrastructure only ([`dsl::parse_pattern`],
//! [`dsl::validate_pattern`], [`taxonomy::MappingEngine`]) and
//! introduces no new pipeline logic. Also like `validate-pattern`, a
//! pattern library with issues is a valid, reportable outcome, not a
//! CLI usage error — [`run`] only ever returns `Err` for a genuine I/O
//! failure reading `patterns_dir` itself.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use crate::error::CliError;

/// One issue found while scanning the pattern library.
struct Issue {
    /// The pattern file this issue concerns, relative to `patterns_dir`.
    file: String,
    /// A human-readable description of the problem.
    message: String,
}

/// Scan every `.rcdsl` file directly inside `patterns_dir`
/// (non-recursive, matching [`crate::patterns::load_patterns`]'s own
/// convention) for:
///
/// 1. Duplicate pattern ids across files.
/// 2. Patterns whose `family` has no taxonomy mapping (mirrors
///    `validate-pattern`'s own taxonomy check, run here across the
///    whole library at once).
/// 3. Patterns missing one or both of their conventional demo traces
///    at `<demo_dir>/<stem>_positive.json` / `..._negative.json`.
///
/// Files that fail to parse or compile are reported as their own issue
/// rather than aborting the scan — one malformed pattern shouldn't
/// hide problems in every other file.
///
/// Returns the rendered report and whether the library is issue-free.
///
/// # Errors
/// [`CliError::InputPath`] if `patterns_dir` cannot be read, or
/// [`CliError::Usage`] if it contains no `.rcdsl` files.
pub fn run(patterns_dir: &Path, demo_dir: &Path) -> Result<(String, bool), CliError> {
    let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(patterns_dir)
        .map_err(|e| CliError::InputPath {
            path: patterns_dir.to_path_buf(),
            reason: e.to_string(),
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "rcdsl"))
        .collect();
    entries.sort();

    if entries.is_empty() {
        return Err(CliError::Usage(format!(
            "no `.rcdsl` pattern files found in `{}`",
            patterns_dir.display()
        )));
    }

    let mut issues = Vec::new();
    let mut ids_seen: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let taxonomy_engine = taxonomy::MappingEngine::new();
    let mut checked = 0usize;

    for path in &entries {
        if scan_pattern_file(
            path,
            patterns_dir,
            demo_dir,
            &taxonomy_engine,
            &mut ids_seen,
            &mut issues,
        ) {
            checked += 1;
        }
    }

    for (id, files) in &ids_seen {
        if files.len() > 1 {
            issues.push(Issue {
                file: files.join(", "),
                message: format!("duplicate pattern id `{id}` used in more than one file"),
            });
        }
    }

    let ok = issues.is_empty();
    Ok((render(patterns_dir, checked, entries.len(), &issues), ok))
}

/// Run every per-file check from [`run`] against one pattern file,
/// pushing any problems onto `issues` and recording its id in
/// `ids_seen` (for the caller's cross-file duplicate-id check).
///
/// Split out of [`run`] purely to stay under `clippy::too_many_lines`
/// on that function — there is no other reason this isn't inlined
/// there (mirrors [`crate::run_pattern_sdk_command`]'s own reason for
/// existing).
///
/// Returns `true` if the file parsed, validated, and compiled
/// successfully (i.e. counts toward `run`'s "compiled successfully"
/// tally).
fn scan_pattern_file(
    path: &Path,
    patterns_dir: &Path,
    demo_dir: &Path,
    taxonomy_engine: &taxonomy::MappingEngine,
    ids_seen: &mut BTreeMap<String, Vec<String>>,
    issues: &mut Vec<Issue>,
) -> bool {
    let file_label = path
        .strip_prefix(patterns_dir)
        .unwrap_or(path)
        .display()
        .to_string();

    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            issues.push(Issue {
                file: file_label,
                message: format!("failed to read: {e}"),
            });
            return false;
        }
    };

    let ast = match dsl::parse_pattern(&source) {
        Ok(ast) => ast,
        Err(e) => {
            issues.push(Issue {
                file: file_label,
                message: format!("syntax error: {e}"),
            });
            return false;
        }
    };

    let diagnostics = dsl::validate_pattern(&ast);
    if diagnostics.has_errors() {
        let detail = diagnostics
            .iter()
            .map(|d| d.render(&source))
            .collect::<Vec<_>>()
            .join("; ");
        issues.push(Issue {
            file: file_label,
            message: format!("metadata error: {detail}"),
        });
        return false;
    }

    let compiled = match dsl::compile_str(&source) {
        Ok(c) => c,
        Err(e) => {
            issues.push(Issue {
                file: file_label,
                message: format!("failed to compile: {e}"),
            });
            return false;
        }
    };

    ids_seen
        .entry(compiled.id.0.clone())
        .or_default()
        .push(file_label.clone());

    if taxonomy_engine.map_family(&compiled.family).is_empty() {
        issues.push(Issue {
            file: file_label.clone(),
            message: format!(
                "family `{}` is not mapped in any taxonomy — see \
                 docs/PATTERN_AUTHORING_GUIDE.md step 5",
                compiled.family.0
            ),
        });
    }

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("pattern");
    for (label, suffix) in [("positive", "positive"), ("negative", "negative")] {
        let trace_path = demo_dir.join(format!("{stem}_{suffix}.json"));
        if !trace_path.exists() {
            issues.push(Issue {
                file: file_label.clone(),
                message: format!("missing {label} demo trace at `{}`", trace_path.display()),
            });
        }
    }

    true
}

fn render(patterns_dir: &Path, checked: usize, total: usize, issues: &[Issue]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Scanning {} ({total} pattern file(s), {checked} compiled successfully)",
        patterns_dir.display()
    );
    if issues.is_empty() {
        out.push_str("\n\u{2713} no issues found\n");
        return out;
    }
    out.push('\n');
    for issue in issues {
        let _ = writeln!(out, "\u{2717} {}: {}", issue.file, issue.message);
    }
    let _ = writeln!(out, "\n{} issue(s) found", issues.len());
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
            "rootcause-cli-doctor-test-{}-{label}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    const VALID_PATTERN: &str = r"
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
    fn empty_patterns_dir_is_a_usage_error() {
        let dir = scratch_dir("empty");
        let patterns_dir = dir.join("patterns");
        std::fs::create_dir_all(&patterns_dir).unwrap();
        let err = run(&patterns_dir, &dir.join("demo")).expect_err("must fail");
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn missing_demo_traces_are_reported() {
        let dir = scratch_dir("missing-demo");
        let patterns_dir = dir.join("patterns");
        let demo_dir = dir.join("demo");
        std::fs::create_dir_all(&patterns_dir).unwrap();
        std::fs::create_dir_all(&demo_dir).unwrap();
        std::fs::write(patterns_dir.join("trivial.rcdsl"), VALID_PATTERN).unwrap();

        let (report, ok) = run(&patterns_dir, &demo_dir).expect("should not error");
        assert!(!ok);
        assert!(report.contains("missing positive demo trace"));
        assert!(report.contains("missing negative demo trace"));
    }

    #[test]
    fn duplicate_ids_across_files_are_reported() {
        let dir = scratch_dir("dup-ids");
        let patterns_dir = dir.join("patterns");
        let demo_dir = dir.join("demo");
        std::fs::create_dir_all(&patterns_dir).unwrap();
        std::fs::create_dir_all(&demo_dir).unwrap();
        std::fs::write(patterns_dir.join("a.rcdsl"), VALID_PATTERN).unwrap();
        std::fs::write(patterns_dir.join("b.rcdsl"), VALID_PATTERN).unwrap();

        let (report, ok) = run(&patterns_dir, &demo_dir).expect("should not error");
        assert!(!ok);
        assert!(report.contains("duplicate pattern id `trivial`"));
    }

    #[test]
    fn malformed_pattern_is_reported_but_does_not_abort_the_scan() {
        let dir = scratch_dir("malformed");
        let patterns_dir = dir.join("patterns");
        let demo_dir = dir.join("demo");
        std::fs::create_dir_all(&patterns_dir).unwrap();
        std::fs::create_dir_all(&demo_dir).unwrap();
        std::fs::write(patterns_dir.join("bad.rcdsl"), "not a valid pattern").unwrap();
        std::fs::write(patterns_dir.join("ok.rcdsl"), VALID_PATTERN).unwrap();

        let (report, ok) = run(&patterns_dir, &demo_dir).expect("should not error");
        assert!(!ok);
        assert!(report.contains("syntax error"));
    }

    #[test]
    fn well_known_pattern_library_reports_no_taxonomy_issues() {
        // The real, shipped classic_reentrancy pattern must not be
        // flagged as unmapped, regardless of where this test runs from.
        let real_patterns_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../patterns");
        let src = std::fs::read_to_string(real_patterns_dir.join("classic_reentrancy.rcdsl"))
            .expect("repo fixture must exist");
        let dir = scratch_dir("known-pattern");
        let patterns_dir = dir.join("patterns");
        let demo_dir = dir.join("demo");
        std::fs::create_dir_all(&patterns_dir).unwrap();
        std::fs::create_dir_all(&demo_dir).unwrap();
        std::fs::write(patterns_dir.join("classic_reentrancy.rcdsl"), src).unwrap();

        let (report, _ok) = run(&patterns_dir, &demo_dir).expect("should not error");
        assert!(!report.contains("is not mapped in any taxonomy"));
    }
}

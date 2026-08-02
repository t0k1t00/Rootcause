//! Loading one or more compiled patterns for `rootcause analyze`.
//!
//! This module deliberately duplicates no `dsl` logic: it only decides
//! *which files* to hand to [`dsl::load_pattern_file`] (a single
//! `.rcdsl` file, or every `.rcdsl` file directly inside a directory,
//! sorted for reproducible ordering — mirroring
//! `benchmark_harness::fixtures::load_suite_from_dir`'s own convention
//! for the same reason). Parsing, validating, and compiling pattern
//! source text remains entirely `dsl`'s job.

use std::path::{Path, PathBuf};

use dsl::ir::CompiledPattern;

use crate::error::CliError;

/// The file extension every pattern-definition file must use to be
/// picked up from a directory (see [`load_patterns`]).
const PATTERN_EXTENSION: &str = "rcdsl";

/// Load every pattern `path` names: a single pattern file, or every
/// `.rcdsl` file directly inside a directory (non-recursive), in sorted
/// filename order.
///
/// # Errors
/// Returns [`CliError::InputPath`] if `path` does not exist or is
/// neither a file nor a directory, [`CliError::Usage`] if a directory
/// contains no `.rcdsl` files, or [`CliError::Dsl`] if any pattern
/// fails to parse, validate, or compile.
pub fn load_patterns(path: &Path) -> Result<Vec<CompiledPattern>, CliError> {
    if path.is_file() {
        let compiled = dsl::load_pattern_file(path)?;
        return Ok(vec![compiled]);
    }

    if !path.is_dir() {
        return Err(CliError::InputPath {
            path: path.to_path_buf(),
            reason: "not a file or directory".to_string(),
        });
    }

    let mut entries: Vec<PathBuf> = std::fs::read_dir(path)
        .map_err(|e| CliError::InputPath {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == PATTERN_EXTENSION))
        .collect();
    entries.sort();

    if entries.is_empty() {
        return Err(CliError::Usage(format!(
            "no `.{PATTERN_EXTENSION}` pattern files found in `{}`",
            path.display()
        )));
    }

    entries
        .into_iter()
        .map(|p| dsl::load_pattern_file(&p).map_err(CliError::from))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn scratch_dir(label: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "rootcause-cli-patterns-test-{}-{label}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn loads_a_single_pattern_file() {
        let dir = scratch_dir("single-file");
        let path = dir.join("trivial.rcdsl");
        std::fs::write(&path, VALID_PATTERN).unwrap();

        let patterns = load_patterns(&path).expect("should load");
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].id.0, "trivial");
    }

    #[test]
    fn loads_every_rcdsl_file_in_a_directory_sorted() {
        let dir = scratch_dir("multi");
        let second = VALID_PATTERN.replace("trivial", "trivial_b");
        std::fs::write(dir.join("b.rcdsl"), second).unwrap();
        std::fs::write(dir.join("a.rcdsl"), VALID_PATTERN).unwrap();
        std::fs::write(dir.join("ignore.txt"), "not a pattern").unwrap();

        let patterns = load_patterns(&dir).expect("should load");
        assert_eq!(patterns.len(), 2);
        assert_eq!(patterns[0].id.0, "trivial");
        assert_eq!(patterns[1].id.0, "trivial_b");
    }

    #[test]
    fn empty_directory_is_a_usage_error() {
        let dir = scratch_dir("empty");
        let err = load_patterns(&dir).expect_err("must fail");
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn missing_path_is_an_input_path_error() {
        let dir = scratch_dir("missing");
        let missing = dir.join("does-not-exist.rcdsl");
        let err = load_patterns(&missing).expect_err("must fail");
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn malformed_pattern_is_a_dsl_error() {
        let dir = scratch_dir("malformed");
        let path = dir.join("bad.rcdsl");
        std::fs::write(&path, "not a valid pattern").unwrap();

        let err = load_patterns(&path).expect_err("must fail");
        assert_eq!(err.exit_code(), 2);
    }
}

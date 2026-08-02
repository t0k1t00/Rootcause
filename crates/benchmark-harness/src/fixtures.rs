//! Loading [`BenchmarkCase`]s and [`BenchmarkSuite`]s from disk: JSON
//! benchmark definitions, referencing raw trace fixture files and DSL
//! pattern sources.
//!
//! ## JSON benchmark definition schema
//!
//! One case per JSON file:
//!
//! ```json
//! {
//!   "id": "reentrancy_basic",
//!   "description": "A single classic reentrant call with a state write.",
//!   "trace_file": "reentrancy_basic.trace.json",
//!   "provenance_label": "fixture:reentrancy_basic",
//!   "patterns": [
//!     { "source": "pattern classic_reentrancy version 1 { ... }" }
//!   ],
//!   "expected": [
//!     { "pattern_id": "classic_reentrancy", "outcome": "grounded" }
//!   ]
//! }
//! ```
//!
//! `trace_file` is resolved relative to the definition file's own
//! parent directory. Each pattern entry is either `{"source": "..."}`
//! (inline DSL source) or `{"file": "relative/path.pattern"}` (DSL
//! source loaded from a sibling file, also resolved relative to the
//! definition file); exactly one of the two must be present.
//!
//! **Trap:** [`load_suite_from_dir`] loads every `*.json` file directly
//! inside its target directory as a case definition (see that
//! function's own docs). If a `trace_file` also ends in `.json` (as the
//! `reentrancy_basic.trace.json` example above does) and is placed in
//! that same directory, it will itself be picked up a second time as a
//! (malformed) case definition. Give raw trace fixtures referenced by
//! `trace_file` either a non-`.json` extension or their own
//! subdirectory to avoid this.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::BenchmarkError;
use crate::types::{BenchmarkCase, BenchmarkSuite, ExpectedFinding, PatternOutcome};

#[derive(Debug, Deserialize)]
struct PatternDef {
    source: Option<String>,
    file: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct ExpectedDef {
    pattern_id: String,
    outcome: PatternOutcome,
}

#[derive(Debug, Deserialize)]
struct CaseDef {
    id: String,
    description: String,
    trace_file: PathBuf,
    #[serde(default)]
    provenance_label: Option<String>,
    patterns: Vec<PatternDef>,
    #[serde(default)]
    expected: Vec<ExpectedDef>,
}

fn read_to_string(path: &Path) -> Result<String, BenchmarkError> {
    std::fs::read_to_string(path).map_err(|e| BenchmarkError::FixtureIo {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })
}

/// Load one [`BenchmarkCase`] from a JSON definition file at `path`.
///
/// # Errors
/// Returns [`BenchmarkError::FixtureIo`] if the definition file or a
/// referenced pattern/trace file cannot be read,
/// [`BenchmarkError::MalformedDefinition`] if the definition JSON does
/// not match the expected schema, or
/// [`BenchmarkError::PatternCompile`] if a pattern's DSL source fails
/// to compile.
pub fn load_case(path: impl AsRef<Path>) -> Result<BenchmarkCase, BenchmarkError> {
    let path = path.as_ref();
    let raw = read_to_string(path)?;
    let def: CaseDef =
        serde_json::from_str(&raw).map_err(|e| BenchmarkError::MalformedDefinition {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;

    let base = path.parent().unwrap_or_else(|| Path::new("."));

    let mut patterns = Vec::with_capacity(def.patterns.len());
    for (i, pd) in def.patterns.iter().enumerate() {
        let source = match (&pd.source, &pd.file) {
            (Some(s), None) => s.clone(),
            (None, Some(f)) => read_to_string(&base.join(f))?,
            _ => {
                return Err(BenchmarkError::MalformedDefinition {
                    path: path.to_path_buf(),
                    reason: format!("pattern entry {i} must set exactly one of `source` or `file`"),
                })
            }
        };
        let compiled = dsl::compile_str(&source).map_err(|e| BenchmarkError::PatternCompile {
            case_id: def.id.clone(),
            pattern_label: format!("#{i}"),
            source: e,
        })?;
        patterns.push(compiled);
    }

    let expected = def
        .expected
        .into_iter()
        .map(|e| ExpectedFinding::new(e.pattern_id, e.outcome))
        .collect();

    let trace_path = base.join(&def.trace_file);
    let provenance_label = def
        .provenance_label
        .unwrap_or_else(|| format!("fixture:{}", def.id));

    Ok(BenchmarkCase::from_file(
        def.id,
        def.description,
        trace_path,
        provenance_label,
        patterns,
        expected,
    ))
}

/// Load every `*.json` file directly inside `dir` (non-recursive) as a
/// case definition, producing one [`BenchmarkSuite`] named after the
/// directory.
///
/// Files are loaded in sorted filename order, for reproducible suite
/// ordering across platforms and filesystems.
///
/// # Errors
/// Returns [`BenchmarkError::InvalidFixtureDirectory`] if `dir` does
/// not exist or is not a directory, or any error [`load_case`] can
/// return for an individual definition file.
pub fn load_suite_from_dir(dir: impl AsRef<Path>) -> Result<BenchmarkSuite, BenchmarkError> {
    let dir = dir.as_ref();
    if !dir.is_dir() {
        return Err(BenchmarkError::InvalidFixtureDirectory {
            path: dir.to_path_buf(),
        });
    }

    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| BenchmarkError::FixtureIo {
            path: dir.to_path_buf(),
            reason: e.to_string(),
        })?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
        .collect();
    entries.sort();

    let mut suite = BenchmarkSuite::new(dir.file_name().map_or_else(
        || "fixtures".to_string(),
        |n| n.to_string_lossy().into_owned(),
    ));
    for entry in entries {
        suite.push(load_case(entry)?);
    }
    Ok(suite)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    /// A fresh, empty scratch directory under the system temp dir, unique
    /// per test (process id + a per-process counter), so parallel test
    /// runs never collide.
    fn scratch_dir(label: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "benchmark-harness-fixtures-test-{}-{label}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    const TRIVIAL_PATTERN_SOURCE: &str = r"
        pattern trivial version 1 {
            family: Reentrancy
            severity: Low
            evidence {
                required c: call(kind: External)
            }
            constraint: c
        }
    ";

    const MINIMAL_TRACE_JSON: &str = r#"{
        "block": { "number": 1, "timestamp": 1, "chain_id": 1 },
        "transaction": {
            "hash": "0x0101010101010101010101010101010101010101010101010101010101010101",
            "from": "0x0101010101010101010101010101010101010101",
            "to": "0x0202020202020202020202020202020202020202",
            "value": "0x0",
            "nonce": 0,
            "gas_limit": 21000,
            "status": "success"
        },
        "calls": [
            {
                "id": 0,
                "parent": null,
                "kind": "call",
                "depth": 0,
                "from": "0x0101010101010101010101010101010101010101",
                "to": "0x0202020202020202020202020202020202020202",
                "value": "0x0",
                "gas": 100000,
                "gas_used": 50000,
                "success": true
            }
        ],
        "storage_changes": [],
        "logs": []
    }"#;

    #[test]
    fn load_case_with_inline_pattern_source() {
        let dir = scratch_dir("inline");
        std::fs::write(dir.join("trace.json"), MINIMAL_TRACE_JSON).unwrap();
        let def = format!(
            r#"{{
                "id": "trivial_case",
                "description": "a trivial case",
                "trace_file": "trace.json",
                "patterns": [ {{ "source": {} }} ],
                "expected": [ {{ "pattern_id": "trivial", "outcome": "grounded" }} ]
            }}"#,
            serde_json::to_string(TRIVIAL_PATTERN_SOURCE).unwrap()
        );
        let def_path = dir.join("case.json");
        std::fs::write(&def_path, def).unwrap();

        let case = load_case(&def_path).expect("well-formed definition should load");
        assert_eq!(case.id.0, "trivial_case");
        assert_eq!(case.description, "a trivial case");
        assert_eq!(case.patterns.len(), 1);
        assert_eq!(case.expected.len(), 1);
        assert_eq!(case.expected[0].pattern_id.0, "trivial");
    }

    #[test]
    fn load_case_with_file_referenced_pattern_source() {
        let dir = scratch_dir("file-ref");
        std::fs::write(dir.join("trace.json"), MINIMAL_TRACE_JSON).unwrap();
        std::fs::write(dir.join("trivial.pattern"), TRIVIAL_PATTERN_SOURCE).unwrap();
        let def = r#"{
            "id": "file_ref_case",
            "description": "pattern loaded from a sibling file",
            "trace_file": "trace.json",
            "patterns": [ { "file": "trivial.pattern" } ],
            "expected": []
        }"#;
        let def_path = dir.join("case.json");
        std::fs::write(&def_path, def).unwrap();

        let case = load_case(&def_path).expect("well-formed definition should load");
        assert_eq!(case.id.0, "file_ref_case");
        assert_eq!(case.patterns.len(), 1);
        assert!(case.expected.is_empty());
    }

    #[test]
    fn load_case_default_provenance_label_names_the_case() {
        let dir = scratch_dir("provenance");
        std::fs::write(dir.join("trace.json"), MINIMAL_TRACE_JSON).unwrap();
        let def = r#"{
            "id": "unlabeled",
            "description": "no explicit provenance_label",
            "trace_file": "trace.json",
            "patterns": [],
            "expected": []
        }"#;
        let def_path = dir.join("case.json");
        std::fs::write(&def_path, def).unwrap();

        let case = load_case(&def_path).expect("well-formed definition should load");
        assert_eq!(
            case.provenance(),
            fact_model::TraceSource::ArchiveNodeRpc {
                endpoint_label: "fixture:unlabeled".to_string(),
            }
        );
    }

    #[test]
    fn load_case_rejects_malformed_json() {
        let dir = scratch_dir("malformed");
        let def_path = dir.join("case.json");
        std::fs::write(&def_path, "not valid json").unwrap();

        let err = load_case(&def_path).expect_err("malformed JSON must not load");
        assert!(matches!(err, BenchmarkError::MalformedDefinition { .. }));
    }

    #[test]
    fn load_case_rejects_pattern_entry_with_neither_source_nor_file() {
        let dir = scratch_dir("neither");
        std::fs::write(dir.join("trace.json"), MINIMAL_TRACE_JSON).unwrap();
        let def = r#"{
            "id": "bad_pattern_entry",
            "description": "pattern entry sets neither source nor file",
            "trace_file": "trace.json",
            "patterns": [ {} ],
            "expected": []
        }"#;
        let def_path = dir.join("case.json");
        std::fs::write(&def_path, def).unwrap();

        let err = load_case(&def_path).expect_err("must reject an empty pattern entry");
        assert!(matches!(err, BenchmarkError::MalformedDefinition { .. }));
    }

    #[test]
    fn load_case_reports_fixture_io_for_missing_definition_file() {
        let dir = scratch_dir("missing-def");
        let err = load_case(dir.join("does-not-exist.json")).expect_err("must not load");
        assert!(matches!(err, BenchmarkError::FixtureIo { .. }));
    }

    #[test]
    fn load_case_reports_pattern_compile_error() {
        let dir = scratch_dir("bad-pattern");
        std::fs::write(dir.join("trace.json"), MINIMAL_TRACE_JSON).unwrap();
        let def = r#"{
            "id": "bad_pattern_source",
            "description": "pattern source that fails to compile",
            "trace_file": "trace.json",
            "patterns": [ { "source": "not a valid pattern" } ],
            "expected": []
        }"#;
        let def_path = dir.join("case.json");
        std::fs::write(&def_path, def).unwrap();

        let err = load_case(&def_path).expect_err("must not load");
        assert!(matches!(err, BenchmarkError::PatternCompile { .. }));
    }

    #[test]
    fn load_suite_from_dir_loads_in_sorted_filename_order() {
        let dir = scratch_dir("suite-order");
        std::fs::write(dir.join("trace.dat"), MINIMAL_TRACE_JSON).unwrap();
        for id in ["b_case", "a_case", "c_case"] {
            let def = format!(
                r#"{{
                    "id": "{id}",
                    "description": "{id}",
                    "trace_file": "trace.dat",
                    "patterns": [],
                    "expected": []
                }}"#
            );
            // Filenames sort in a-b-c order regardless of the id string,
            // so the suite's case order should follow the filenames.
            let filename = format!("{}.json", &id[..1]);
            std::fs::write(dir.join(filename), def).unwrap();
        }

        let suite = load_suite_from_dir(&dir).expect("directory of valid cases should load");
        assert_eq!(suite.name, dir.file_name().unwrap().to_string_lossy());
        let ids: Vec<&str> = suite.cases.iter().map(|c| c.id.0.as_str()).collect();
        assert_eq!(ids, vec!["a_case", "b_case", "c_case"]);
    }

    #[test]
    fn load_suite_from_dir_ignores_non_json_files() {
        let dir = scratch_dir("suite-ignore");
        std::fs::write(dir.join("trace.dat"), MINIMAL_TRACE_JSON).unwrap();
        let def = r#"{
            "id": "only_case",
            "description": "the only real case definition",
            "trace_file": "trace.dat",
            "patterns": [],
            "expected": []
        }"#;
        std::fs::write(dir.join("case.json"), def).unwrap();
        std::fs::write(dir.join("README.md"), "not a case definition").unwrap();

        let suite = load_suite_from_dir(&dir).expect("directory should load");
        assert_eq!(suite.cases.len(), 1);
        assert_eq!(suite.cases[0].id.0, "only_case");
    }

    #[test]
    fn load_suite_from_dir_rejects_non_directory_path() {
        let dir = scratch_dir("not-a-dir");
        let file_path = dir.join("i_am_a_file.json");
        std::fs::write(&file_path, "{}").unwrap();

        let err = load_suite_from_dir(&file_path).expect_err("a file is not a fixture directory");
        assert!(matches!(
            err,
            BenchmarkError::InvalidFixtureDirectory { .. }
        ));
    }
}

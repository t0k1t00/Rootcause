//! `rootcause new-pattern`: scaffold every file a first-time contributor
//! needs to start authoring a pattern, matching the workflow
//! `docs/PATTERN_AUTHORING_GUIDE.md` already documents step-by-step.
//!
//! This command creates no new infrastructure of its own — it only
//! writes starter files in the exact locations and shapes
//! [`crate::patterns::load_patterns`] (`patterns/*.rcdsl`) and
//! `rootcause analyze` / `validate-pattern` (`demo/*.json`) already
//! expect, so the very next command a contributor runs
//! (`rootcause validate-pattern`) works against files this command just
//! produced.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::error::CliError;

/// One file this command intends to write, and its rendered content.
struct ScaffoldFile {
    path: PathBuf,
    content: String,
}

/// Scaffold a new pattern named `name` under `dir`.
///
/// # Errors
/// - [`CliError::Usage`] if `name` is not a valid pattern identifier
///   (must match the DSL's identifier grammar: ASCII letters, digits,
///   underscores, not starting with a digit), or if any target file
///   already exists and `force` is `false`.
/// - [`CliError::InputPath`] if a target directory cannot be created or
///   a file cannot be written.
pub fn run(name: &str, dir: &Path, family: Option<&str>, force: bool) -> Result<String, CliError> {
    validate_identifier(name)?;
    let family = family.unwrap_or("TODO_ReplaceWithExploitFamily");

    let pattern_path = dir.join("patterns").join(format!("{name}.rcdsl"));
    let positive_path = dir.join("demo").join(format!("{name}_positive.json"));
    let negative_path = dir.join("demo").join(format!("{name}_negative.json"));
    let readme_path = dir.join("patterns").join(format!("{name}.README.md"));
    let taxonomy_note_path = dir
        .join("docs")
        .join("taxonomy-todo")
        .join(format!("{name}.md"));

    let files = vec![
        ScaffoldFile {
            content: pattern_template(name, family),
            path: pattern_path,
        },
        ScaffoldFile {
            content: positive_trace_template(),
            path: positive_path,
        },
        ScaffoldFile {
            content: negative_trace_template(),
            path: negative_path,
        },
        ScaffoldFile {
            content: readme_template(name, family),
            path: readme_path,
        },
        ScaffoldFile {
            content: taxonomy_note_template(name, family),
            path: taxonomy_note_path,
        },
    ];

    if !force {
        for file in &files {
            if file.path.exists() {
                return Err(CliError::Usage(format!(
                    "`{}` already exists; pass --force to overwrite, or pick a different name",
                    file.path.display()
                )));
            }
        }
    }

    for file in &files {
        write_file(&file.path, &file.content)?;
    }

    let mut out = String::new();
    let _ = writeln!(out, "Scaffolded pattern `{name}`:");
    for file in &files {
        let _ = writeln!(out, "  {}", file.path.display());
    }
    out.push('\n');
    out.push_str("Next steps:\n");
    let _ = writeln!(
        out,
        "  1. Edit patterns/{name}.rcdsl — write the evidence and constraint your pattern needs."
    );
    let _ = writeln!(
        out,
        "  2. Edit demo/{name}_positive.json / demo/{name}_negative.json — build a real positive and negative trace (see docs/PATTERN_AUTHORING_GUIDE.md step 6)."
    );
    let _ = writeln!(
        out,
        "  3. Map `{family}` to a taxonomy in crates/taxonomy/src/scwe.rs (and swc.rs if it genuinely fits) — see docs/taxonomy-todo/{name}.md."
    );
    let _ = writeln!(
        out,
        "  4. Run `rootcause validate-pattern patterns/{name}.rcdsl` until every check passes."
    );
    let _ = writeln!(
        out,
        "  5. Run `rootcause format-pattern patterns/{name}.rcdsl --write` before committing."
    );

    Ok(out)
}

/// A pattern identifier is checked here against the same shape the
/// DSL's own lexer accepts for identifiers, so `new-pattern` fails fast
/// with an actionable message instead of scaffolding a file that
/// [`dsl::compile_str`] would reject anyway.
fn validate_identifier(name: &str) -> Result<(), CliError> {
    let mut chars = name.chars();
    let starts_ok = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_');
    let rest_ok = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if name.is_empty() || !starts_ok || !rest_ok {
        return Err(CliError::Usage(format!(
            "`{name}` is not a valid pattern identifier: use ASCII letters, digits, and \
             underscores, starting with a letter or underscore (e.g. `my_new_pattern`)"
        )));
    }
    Ok(())
}

fn write_file(path: &Path, content: &str) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| CliError::InputPath {
            path: parent.to_path_buf(),
            reason: e.to_string(),
        })?;
    }
    std::fs::write(path, content).map_err(|e| CliError::InputPath {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })
}

fn pattern_template(name: &str, family: &str) -> String {
    format!(
        r#"// TODO: describe what this pattern detects, why the evidence below is
// a genuine structural signature (not a coincidental proxy) for it,
// and — a dedicated "Scope note" — what it cannot determine and why.
// See docs/PATTERN_AUTHORING_GUIDE.md step 4 for the expected shape of
// this comment; every pattern in patterns/ is a worked example.
pattern {name} version 1 {{
    family: {family}
    severity: Medium
    tags: ["TODO"]
    references: []

    evidence {{
        required TODO_evidence_name: call(kind: External)
    }}

    constraint: TODO_evidence_name
}}
"#
    )
}

fn positive_trace_template() -> String {
    // A minimal, valid, single-call trace — deliberately generic so it
    // compiles/ingests immediately; the author edits it into a genuine
    // positive case for their pattern (see
    // docs/PATTERN_AUTHORING_GUIDE.md step 6).
    r#"{
  "transaction": {
    "hash": "0x1111111111111111111111111111111111111111111111111111111111111111",
    "from": "0x1111111111111111111111111111111111111111",
    "to": "0x2222222222222222222222222222222222222222",
    "value": "0x0",
    "nonce": "0x1",
    "gasUsed": "0x9c40",
    "status": "success"
  },
  "block": {
    "number": "0x1",
    "timestamp": "0x60000000",
    "chainId": "0x1",
    "baseFee": null
  },
  "root": {
    "kind": "call",
    "from": "0x1111111111111111111111111111111111111111",
    "to": "0x2222222222222222222222222222222222222222",
    "value": "0x0",
    "gasLimit": "0x9c40",
    "gasUsed": "0x9c40",
    "succeeded": true,
    "storageChanges": [],
    "logs": [],
    "calls": []
  }
}
"#
    .to_string()
}

fn negative_trace_template() -> String {
    // Same starting shape as the positive template — the author's job
    // is to make it *structurally similar* (to guard against false
    // positives) while genuinely lacking the evidence the pattern
    // requires.
    r#"{
  "transaction": {
    "hash": "0x3333333333333333333333333333333333333333333333333333333333333333",
    "from": "0x1111111111111111111111111111111111111111",
    "to": "0x2222222222222222222222222222222222222222",
    "value": "0x0",
    "nonce": "0x2",
    "gasUsed": "0x5208",
    "status": "success"
  },
  "block": {
    "number": "0x2",
    "timestamp": "0x60000010",
    "chainId": "0x1",
    "baseFee": null
  },
  "root": {
    "kind": "call",
    "from": "0x1111111111111111111111111111111111111111",
    "to": "0x2222222222222222222222222222222222222222",
    "value": "0x0",
    "gasLimit": "0x5208",
    "gasUsed": "0x5208",
    "succeeded": true,
    "storageChanges": [],
    "logs": [],
    "calls": []
  }
}
"#
    .to_string()
}

fn readme_template(name: &str, family: &str) -> String {
    format!(
        r#"# `{name}`

Status: draft (scaffolded by `rootcause new-pattern`, not yet reviewed).

## What this detects

TODO — one paragraph, plain language.

## Family and taxonomy

- `family: {family}`
- SCWE: TODO (map in `crates/taxonomy/src/scwe.rs`)
- SWC: TODO or "no genuine counterpart" (see
  `docs/PATTERN_AUTHORING_GUIDE.md` step 5 — do not force a fit)

## Evidence

TODO — explain each evidence clause and why it's a genuine structural
signature, not a coincidental proxy.

## Scope note

TODO — what this pattern cannot determine (intent, authorization,
off-chain state), and why.

## Demo traces

- `demo/{name}_positive.json` — TODO describe the case that should fire.
- `demo/{name}_negative.json` — TODO describe the structurally similar
  case that should not.

Verify by hand:

```sh
cargo run -p cli -- analyze demo/{name}_positive.json --patterns patterns/
cargo run -p cli -- analyze demo/{name}_negative.json --patterns patterns/
```

## Checklist before opening a PR

- [ ] `rootcause validate-pattern patterns/{name}.rcdsl` passes every check
- [ ] `rootcause format-pattern patterns/{name}.rcdsl --check` passes
- [ ] Taxonomy mapping added with a test (step 5)
- [ ] Integration tests added to `integration-tests/tests/exploit_corpus.rs` (step 7)
- [ ] Documented in `README.md`'s exploit-corpus section (step 8)
- [ ] `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo build --workspace && cargo test --workspace`
"#
    )
}

fn taxonomy_note_template(name: &str, family: &str) -> String {
    format!(
        r"# Taxonomy mapping TODO: `{name}`

`family: {family}` is not yet mapped to any taxonomy. Delete this file
once mapped.

1. Search https://scs.owasp.org/SCWE/ for a real, unforced match and add
   an entry to `crates/taxonomy/src/scwe.rs`.
2. Only if a genuine, unforced SWC counterpart exists, add one to
   `crates/taxonomy/src/swc.rs` too — it is fine and already precedented
   to leave a family unmapped in SWC (see
   `docs/PATTERN_AUTHORING_GUIDE.md` step 5).
3. Add a test asserting the mapping, mirroring existing tests in both
   files.
4. Run `rootcause validate-pattern patterns/{name}.rcdsl` — the taxonomy
   check will pass once this mapping exists.
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(label: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "rootcause-cli-new-pattern-test-{}-{label}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn scaffolds_every_expected_file() {
        let dir = scratch_dir("basic");
        let report = run("my_pattern", &dir, Some("MyFamily"), false).expect("should scaffold");
        assert!(report.contains("my_pattern"));

        assert!(dir.join("patterns/my_pattern.rcdsl").exists());
        assert!(dir.join("demo/my_pattern_positive.json").exists());
        assert!(dir.join("demo/my_pattern_negative.json").exists());
        assert!(dir.join("patterns/my_pattern.README.md").exists());
        assert!(dir.join("docs/taxonomy-todo/my_pattern.md").exists());
    }

    #[test]
    fn scaffolded_pattern_compiles() {
        let dir = scratch_dir("compiles");
        run("compiles_ok", &dir, None, false).expect("should scaffold");
        let src = std::fs::read_to_string(dir.join("patterns/compiles_ok.rcdsl")).unwrap();
        dsl::compile_str(&src).expect("scaffolded pattern must compile as-is");
    }

    #[test]
    fn scaffolded_traces_ingest() {
        let dir = scratch_dir("ingest");
        run("ingest_ok", &dir, None, false).expect("should scaffold");
        for name in ["ingest_ok_positive.json", "ingest_ok_negative.json"] {
            let path = dir.join("demo").join(name);
            let source = ingestion::FileTraceSource::new(&path);
            let provenance = fact_model::TraceSource::ArchiveNodeRpc {
                endpoint_label: "test".to_string(),
            };
            ingestion::ingest(&source, provenance)
                .unwrap_or_else(|e| panic!("scaffolded trace {name} must ingest: {e}"));
        }
    }

    #[test]
    fn rejects_invalid_identifier() {
        let dir = scratch_dir("bad-name");
        let err = run("not valid!", &dir, None, false).expect_err("must fail");
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn refuses_to_overwrite_without_force() {
        let dir = scratch_dir("no-clobber");
        run("dup", &dir, None, false).expect("first scaffold succeeds");
        let err = run("dup", &dir, None, false).expect_err("second must fail without --force");
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn force_overwrites_existing_files() {
        let dir = scratch_dir("force");
        run("dup2", &dir, None, false).expect("first scaffold succeeds");
        run("dup2", &dir, Some("NewFamily"), true).expect("force overwrite succeeds");
        let src = std::fs::read_to_string(dir.join("patterns/dup2.rcdsl")).unwrap();
        assert!(src.contains("NewFamily"));
    }
}

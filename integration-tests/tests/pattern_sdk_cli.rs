//! Binary-level integration tests for the Milestone 15 "pattern SDK"
//! subcommands (`new-pattern`, `validate-pattern`, `format-pattern`,
//! `doctor`).
//!
//! Unlike the per-crate unit tests in `crates/cli/src/commands/*.rs`,
//! which call each command's `run()` function directly in-process,
//! these tests spawn the actual compiled `rootcause` binary as a
//! subprocess and assert on its exit code and stdout/stderr — the same
//! thing a CI job or a contributor's shell actually observes. See
//! `workspace_smoke.rs`'s `rootcause_binary_runs_successfully` for the
//! binary-locating convention this file reuses.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Locate the compiled `rootcause` binary next to this test executable,
/// exactly as `workspace_smoke.rs` does.
fn rootcause_bin() -> PathBuf {
    let test_exe =
        std::env::current_exe().expect("integration test must know its own executable path");
    let profile_dir = test_exe
        .parent()
        .and_then(Path::parent)
        .expect("test executable must live under target/{profile}/deps");
    let bin_name = if cfg!(windows) {
        "rootcause.exe"
    } else {
        "rootcause"
    };
    let bin = profile_dir.join(bin_name);
    assert!(
        bin.exists(),
        "expected rootcause binary at {}; was `cargo build --workspace` run first?",
        bin.display()
    );
    bin
}

fn run(args: &[&str], cwd: &Path) -> Output {
    Command::new(rootcause_bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("failed to execute rootcause")
}

fn scratch_dir(label: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "rootcause-integration-pattern-sdk-{}-{label}-{n}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// The full happy-path workflow a contributor is expected to follow,
/// exercised end-to-end through the real binary: scaffold a pattern,
/// validate it (expected to fail — the scaffold is a TODO template),
/// format it (expected to already be canonical), and confirm doctor
/// sees it.
#[test]
fn new_pattern_then_validate_then_format_workflow() {
    let dir = scratch_dir("workflow");

    let scaffold = run(&["new-pattern", "cli_workflow_test", "--dir", "."], &dir);
    assert!(
        scaffold.status.success(),
        "new-pattern failed: {}",
        String::from_utf8_lossy(&scaffold.stderr)
    );
    assert!(dir.join("patterns/cli_workflow_test.rcdsl").exists());
    assert!(dir.join("demo/cli_workflow_test_positive.json").exists());
    assert!(dir.join("demo/cli_workflow_test_negative.json").exists());

    // The scaffolded pattern is a TODO template with an unmapped
    // family, so validate-pattern must report at least one failing
    // check and exit non-zero (ChecksFailed => exit code 1).
    let validate = run(
        &["validate-pattern", "patterns/cli_workflow_test.rcdsl"],
        &dir,
    );
    assert_eq!(validate.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&validate.stdout);
    assert!(stdout.contains("syntax"));

    // format-pattern --check must succeed against the freshly
    // scaffolded (already canonical) file.
    let fmt_check = run(
        &[
            "format-pattern",
            "patterns/cli_workflow_test.rcdsl",
            "--check",
        ],
        &dir,
    );
    assert!(
        fmt_check.status.success(),
        "expected scaffolded pattern to already be canonically formatted: {}",
        String::from_utf8_lossy(&fmt_check.stderr)
    );
}

/// `doctor` against a real, shipped pattern directory (this repository's
/// own `patterns/` and `demo/`) must find every pattern compiles and
/// every `family` has a taxonomy mapping. It will still report missing
/// demo traces by naming convention for these patterns: the shipped
/// `demo/*.json` files predate the `<stem>_positive.json` /
/// `<stem>_negative.json` convention `new-pattern`/`doctor` use and are
/// wired to their patterns explicitly in
/// `integration-tests/tests/exploit_corpus.rs` instead, so that part of
/// the report is expected, not a bug.
#[test]
fn doctor_reports_no_taxonomy_or_duplicate_id_issues_against_the_real_pattern_library() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("integration-tests has a parent directory");

    let doctor = run(
        &[
            "doctor",
            "--patterns-dir",
            &repo_root.join("patterns").display().to_string(),
            "--demo-dir",
            &repo_root.join("demo").display().to_string(),
        ],
        repo_root,
    );
    let stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        stdout.contains("8 pattern file(s), 8 compiled successfully"),
        "expected every shipped pattern to compile:\n{stdout}"
    );
    assert!(
        !stdout.contains("is not mapped in any taxonomy"),
        "expected every shipped pattern's family to have a taxonomy mapping:\n{stdout}"
    );
    assert!(
        !stdout.contains("duplicate pattern id"),
        "expected no duplicate pattern ids in the shipped library:\n{stdout}"
    );
}

/// `doctor` must catch a pattern with no demo traces at all, and exit
/// non-zero.
#[test]
fn doctor_detects_missing_demo_traces() {
    let dir = scratch_dir("doctor-missing-demo");
    let patterns_dir = dir.join("patterns");
    let demo_dir = dir.join("demo");
    std::fs::create_dir_all(&patterns_dir).unwrap();
    std::fs::create_dir_all(&demo_dir).unwrap();
    std::fs::write(
        patterns_dir.join("lonely.rcdsl"),
        "pattern lonely version 1 {\n    family: Reentrancy\n    severity: Low\n    evidence {\n        required c: call(kind: External)\n    }\n    constraint: c\n}\n",
    )
    .unwrap();

    let doctor = run(
        &[
            "doctor",
            "--patterns-dir",
            &patterns_dir.display().to_string(),
            "--demo-dir",
            &demo_dir.display().to_string(),
        ],
        &dir,
    );
    assert_eq!(doctor.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(stdout.contains("missing positive demo trace"));
    assert!(stdout.contains("missing negative demo trace"));
}

/// `format-pattern --write` must actually rewrite a messily formatted
/// file on disk to the canonical style, in place.
#[test]
fn format_pattern_write_rewrites_file_in_place() {
    let dir = scratch_dir("format-write");
    let path = dir.join("messy.rcdsl");
    std::fs::write(
        &path,
        "pattern messy version 1 {\n  family: Reentrancy\n  severity:Low\n  evidence { required c: call(kind: External) }\n  constraint: c\n}\n",
    )
    .unwrap();

    let write = run(&["format-pattern", "messy.rcdsl", "--write"], &dir);
    assert!(
        write.status.success(),
        "format-pattern --write failed: {}",
        String::from_utf8_lossy(&write.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).unwrap();
    assert!(rewritten.contains("    family: Reentrancy\n"));

    // A second --check run against the now-canonical file must succeed.
    let check = run(&["format-pattern", "messy.rcdsl", "--check"], &dir);
    assert!(check.status.success());
}

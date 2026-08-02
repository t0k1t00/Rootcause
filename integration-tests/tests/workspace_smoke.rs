//! Workspace-wide smoke tests.
//!
//! This file intentionally opts out of the workspace's `unwrap_used` /
//! `expect_used` / `panic` lints (all `warn`-level workspace-wide, see
//! root `Cargo.toml`). Those lints encode "no `unwrap()` in *library* code"
//! from the project's engineering rules; test code's canonical failure
//! mode *is* panicking (`assert!`, `.expect()` with a diagnostic message),
//! so applying the library-code lint here would fight the standard test
//! idiom rather than serve its purpose. Restated per-file rather than at
//! the workspace level so the exception is visibly scoped to tests only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//!
//! These are intentionally the only tests in this crate for Task 1: with
//! no business logic implemented anywhere yet, there is nothing
//! cross-crate to exercise except that the dependency graph described in
//! ADR-0005 actually links, and that the `rootcause` binary built by the
//! `cli` crate runs successfully end-to-end as a process. Real
//! integration tests (e.g. "ingest this trace, match this pattern, expect
//! this grounded finding") belong here once the crates they exercise have
//! real implementations.

/// Proves every crate in the ADR-0005 dependency graph links together in
/// a single test binary, by referencing each crate's version constant.
/// This is the integration-level equivalent of the per-crate
/// `dependencies_link` unit tests, but exercised through the crate that
/// depends on literally everything.
#[test]
fn full_dependency_graph_links() {
    let versions = [
        fact_model::CRATE_VERSION,
        ingestion::CRATE_VERSION,
        dsl::CRATE_VERSION,
        matcher::CRATE_VERSION,
        grounding::CRATE_VERSION,
        taxonomy::CRATE_VERSION,
        benchmark_harness::CRATE_VERSION,
    ];
    assert!(versions.iter().all(|v| !v.is_empty()));
}

/// Proves the compiled `rootcause` binary (from the `cli` crate) actually
/// runs as a standalone process and exits successfully.
///
/// This deliberately does *not* use `env!("CARGO_BIN_EXE_rootcause")`:
/// that mechanism requires `cli` to be a declared dependency of this test
/// package, but `cli` has no `[lib]` target (see crates/cli/Cargo.toml),
/// so Cargo cannot resolve it as one — attempting the dependency produces
/// "ignoring invalid dependency `cli` which is missing a lib target" at
/// the workspace level and a compile-time error on the `env!` lookup here.
///
/// Instead, the binary is located relative to `integration-tests`' own
/// test-executable path: Cargo places every workspace binary and every
/// test executable as siblings under the same `target/{profile}/`
/// directory (test binaries live one level deeper, under `deps/`), so
/// walking up from `std::env::current_exe()` to that shared directory and
/// then into it reliably finds `rootcause` regardless of profile
/// (debug/release) or whether this is run via `cargo test` or
/// `cargo nextest run`.
#[test]
fn rootcause_binary_runs_successfully() {
    let test_exe =
        std::env::current_exe().expect("integration test must know its own executable path");
    // test_exe looks like `.../target/{profile}/deps/workspace_smoke-<hash>`;
    // its grandparent is `target/{profile}`, where `[[bin]]` outputs land.
    let profile_dir = test_exe
        .parent() // .../target/{profile}/deps
        .and_then(std::path::Path::parent) // .../target/{profile}
        .expect("test executable must live under target/{profile}/deps");

    let bin_name = if cfg!(windows) {
        "rootcause.exe"
    } else {
        "rootcause"
    };
    let bin = profile_dir.join(bin_name);

    assert!(
        bin.exists(),
        "expected rootcause binary at {}; was `cargo build --workspace` \
         (or an equivalent full build) run before this test?",
        bin.display()
    );

    let status = std::process::Command::new(&bin)
        .status()
        .unwrap_or_else(|e| panic!("failed to execute {}: {e}", bin.display()));
    assert!(status.success());
}

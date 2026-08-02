//! Fixture-based tests: realistic, on-disk pattern definitions exercised
//! through the full public API, rather than inline source strings. These
//! complement the unit tests inline in each module (which target
//! individual stages) by exercising `dsl`'s public entry points exactly
//! as a future `matcher`/`cli` caller would.
//!
//! This file is its own compilation unit (a `tests/` integration test
//! binary), so it does not inherit `src/lib.rs`'s `cfg(test)` lint
//! exception — restated here per the same project convention
//! `ingestion`'s and the workspace's own integration tests use.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use dsl::DslError;

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read fixture {path}: {e}"))
}

#[test]
fn classic_reentrancy_fixture_compiles() {
    let source = fixture("classic_reentrancy.rcdsl");
    let compiled = dsl::compile_str(&source).expect("fixture should compile");
    assert_eq!(compiled.id.0, "classic_reentrancy");
    assert_eq!(compiled.family.0, "Reentrancy");
    assert_eq!(compiled.evidence.len(), 3);
    assert!(compiled.sequence.is_some());
}

#[test]
fn donation_attack_fixture_compiles() {
    let source = fixture("donation_attack.rcdsl");
    let compiled = dsl::compile_str(&source).expect("fixture should compile");
    assert_eq!(compiled.id.0, "donation_attack");
    assert_eq!(compiled.family.0, "OracleManipulation");
    assert_eq!(compiled.tags.len(), 3);
    assert_eq!(compiled.references, vec!["SCWE-042".to_string()]);
}

#[test]
fn unclosed_brace_fixture_fails_to_parse() {
    let source = fixture("malformed_unclosed_brace.rcdsl");
    let err = dsl::compile_str(&source).unwrap_err();
    assert!(matches!(err, DslError::Parse(_)));
}

#[test]
fn duplicate_and_reference_fixture_fails_validation() {
    let source = fixture("invalid_duplicate_and_reference.rcdsl");
    let err = dsl::compile_str(&source).unwrap_err();
    let DslError::Validation(diags) = err else {
        panic!("expected a Validation error, got a different DslError variant");
    };
    // Both problems (duplicate evidence name, and an undeclared
    // `ghost_evidence` reference) should be reported in the same pass.
    assert!(diags
        .iter()
        .any(|d| d.reason.contains("duplicate evidence")));
    assert!(diags.iter().any(|d| d.reason.contains("not declared")));
}

#[test]
fn all_valid_fixtures_produce_no_warnings() {
    for name in ["classic_reentrancy.rcdsl", "donation_attack.rcdsl"] {
        let source = fixture(name);
        let ast =
            dsl::parse_pattern(&source).unwrap_or_else(|e| panic!("{name} failed to parse: {e:?}"));
        let diags = dsl::validate_pattern(&ast);
        assert!(
            diags.is_empty(),
            "{name} produced unexpected diagnostics: {diags:?}"
        );
    }
}

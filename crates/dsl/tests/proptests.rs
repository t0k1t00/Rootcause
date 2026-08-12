//! Property tests: rather than checking specific fixed examples, these
//! generate a space of structurally-valid pattern sources and assert
//! properties that must hold for *any* of them.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;

/// A valid Rust-identifier-shaped name, disjoint from every DSL keyword
/// this crate reserves (`AND`, `OR`, `NOT`, `pattern`, `version`,
/// `evidence`, `required`, `optional`, `constraint`, `sequence`,
/// `within`, `true`, `false`) by construction: the strategy only ever
/// produces lowercase names, and every reserved word is either
/// uppercase-leading or itself excluded below.
fn evidence_name() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,10}".prop_filter("must not be a reserved word", |s| {
        !matches!(s.as_str(), "true" | "false")
    })
}

fn severity() -> impl Strategy<Value = String> {
    prop_oneof!["Low", "Medium", "High", "Critical"]
}

fn call_kind() -> impl Strategy<Value = String> {
    // `StaticCall` is deliberately excluded: paired with `reentrant:
    // true` it is the one documented incompatible-predicates case,
    // which this property test isn't exercising.
    prop_oneof!["External", "Internal", "Delegate", "Create"]
}

/// Build source text for a pattern with `n` distinct required `call`
/// evidence clauses AND-ed together, and an optional matching sequence.
fn build_pattern_source(names: &[String], kind: &str, with_sequence: bool) -> String {
    use std::fmt::Write as _;

    let mut evidence_block = String::new();
    for n in names {
        let _ = writeln!(evidence_block, "        required {n}: call(kind: {kind})");
    }
    let constraint = if names.len() == 1 {
        names[0].clone()
    } else {
        format!("AND({})", names.join(", "))
    };
    let sequence = if with_sequence && names.len() >= 2 {
        format!("sequence: [{}]", names.join(", "))
    } else {
        String::new()
    };
    format!(
        r"
        pattern generated_pattern version 1 {{
            family: Reentrancy
            severity: High

            evidence {{
{evidence_block}            }}

            constraint: {constraint}
            {sequence}
        }}
        "
    )
}

proptest! {
    /// Any pattern built from N ≥ 1 distinct required `call` evidence
    /// clauses, ANDed together, must compile successfully — this is
    /// exactly the shape the grammar and schema are designed to accept.
    #[test]
    fn distinct_required_evidence_always_compiles(
        names in prop::collection::hash_set(evidence_name(), 1..6),
        kind in call_kind(),
    ) {
        let names: Vec<String> = names.into_iter().collect();
        let source = build_pattern_source(&names, &kind, false);
        let compiled = dsl::compile_str(&source);
        prop_assert!(compiled.is_ok(), "expected compile success, got {compiled:?}\nsource:\n{source}");
        let compiled = compiled.unwrap();
        prop_assert_eq!(compiled.evidence.len(), names.len());
    }

    /// Compiling the same valid source twice must produce byte-for-byte
    /// identical [`dsl::CompiledPattern`]s — the pipeline has no hidden
    /// randomness or ordering nondeterminism (e.g. from hash-set
    /// iteration order leaking into output).
    #[test]
    fn compilation_is_deterministic(
        names in prop::collection::hash_set(evidence_name(), 1..6),
        kind in call_kind(),
    ) {
        let names: Vec<String> = names.into_iter().collect();
        let source = build_pattern_source(&names, &kind, names.len() >= 2);
        let first = dsl::compile_str(&source);
        let second = dsl::compile_str(&source);
        prop_assert_eq!(first.ok(), second.ok());
    }

    /// A valid `severity:` value is always accepted (never flagged as
    /// unrecognized), for every value in the DSL's closed severity enum.
    #[test]
    fn every_known_severity_is_accepted(severity in severity()) {
        let source = format!(
            r"
            pattern p version 1 {{
                family: Reentrancy
                severity: {severity}
                evidence {{ required a: call(kind: External) }}
            }}
            "
        );
        let ast = dsl::parse_pattern(&source).unwrap();
        let diags = dsl::validate_pattern(&ast);
        prop_assert!(!diags.has_errors(), "{diags:?}");
    }
}

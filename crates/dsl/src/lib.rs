//! # dsl
//!
//! The Pattern DSL: a strongly-typed language for describing
//! deterministic exploit patterns, plus the parser, validator, and
//! compiler that turn source text into an immutable, matcher-ready
//! representation.
//!
//! This crate is responsible **only** for parsing, validating,
//! representing, and compiling pattern definitions. It does not perform
//! matching, evaluation, or grounding against a real
//! `fact_model::Trace` — those responsibilities belong to the (not yet
//! implemented) `matcher` and `grounding` crates. See the Architecture
//! document's Pattern DSL / Pattern Compiler sections for the
//! requirements this crate satisfies.
//!
//! ## Pipeline
//!
//! Source text flows through four independent stages, each in its own
//! module, matching this crate's design requirement to keep lexical
//! parsing, structural parsing, and semantic validation separate:
//!
//! 1. [`lexer`] — source text to [`lexer::Token`] stream (lexical parsing).
//! 2. [`parser`] — token stream to [`ast::PatternAst`] (structural parsing).
//! 3. [`validate`] — [`ast::PatternAst`] to [`diagnostics::Diagnostics`]
//!    (semantic validation; empty means valid).
//! 4. [`compile`] — validated [`ast::PatternAst`] to [`ir::CompiledPattern`]
//!    (compilation into the immutable internal representation).
//!
//! [`diagnostics::Diagnostic`] is the single rich-diagnostic type shared
//! by all four stages: every diagnostic carries a source location, a
//! name for the offending construct, a reason, and (where applicable) an
//! actionable suggestion.
//!
//! ## Public API
//!
//! Most callers only need the three top-level functions below, which
//! internally drive the pipeline stages and translate their diagnostics
//! into a [`DslError`]:
//!
//! - [`parse_pattern`] — parse only (stages 1-2), for callers that want
//!   to inspect the surface AST directly (e.g. a formatter or linter).
//! - [`compile_str`] — parse, validate, and compile a pattern from
//!   source text in one call; the common case.
//! - [`load_pattern_file`] — read a pattern from disk, then
//!   [`compile_str`] it.
//!
//! ## How compiled patterns will be consumed
//!
//! A [`ir::CompiledPattern`] is designed to be produced once (at
//! pattern-library load time) and then matched against many traces. Its
//! [`ir::CompiledConstraint`] tree and [`ir::CompiledSequence`] reference
//! evidence clauses by [`ir::EvidenceRef`] (a small `Copy` index), not by
//! name, so the future `matcher` crate can walk the constraint tree
//! evaluating `EvidenceRef`s against grounding results without any
//! string comparison in the hot path — directly mirroring how
//! `fact-model` itself uses small `Copy` ID newtypes instead of names or
//! pointers for the same reason. Each [`ir::CompiledEvidence`]'s
//! [`ir::CompiledPredicate`] names a `fact-model` type it ultimately
//! grounds against (see [`schema::PredicateSchema::grounds_against`]),
//! but this crate stops at describing *that* correspondence — actually
//! walking a `fact_model::Trace` to find matching
//! `Call`/`StorageChange`/etc. facts is the matcher/grounding crates'
//! job, not this one's.
//!
//! ## Integration with `fact-model` and `ingestion`
//!
//! This crate has **no compile-time dependency on `fact-model` or
//! `ingestion`** — it does not need one. A pattern *definition* is
//! meaningful on its own, independent of any specific trace; only
//! *matching* a compiled pattern against a specific `fact_model::Trace`
//! (itself produced by `ingestion` from raw archive-node data) requires
//! that dependency, which belongs in the `matcher` crate. Keeping `dsl`
//! decoupled from `fact-model` means pattern authoring, parsing, and
//! validation tooling (a linter, a language-server-style checker,
//! `cargo test -p dsl`) never needs a real trace, an archive-node
//! connection, or even the `ingestion` crate compiled in.

#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::missing_const_for_fn
    )
)]

pub mod ast;
pub mod compile;
pub mod diagnostics;
pub mod error;
pub mod ir;
pub mod lexer;
pub mod parser;
pub mod schema;
pub mod span;
pub mod validate;

pub use ast::PatternAst;
pub use diagnostics::{Diagnostic, Diagnostics, Severity as DiagnosticSeverity};
pub use error::DslError;
pub use ir::{CompiledPattern, EvidenceRef, Severity};

/// The crate's own semantic version, re-exported for diagnostic and
/// benchmark-harness provenance purposes — the same convention
/// `fact-model` uses (`fact_model::CRATE_VERSION`), so a stored compiled
/// pattern or benchmark result can record which `dsl` version produced
/// it.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

use std::path::Path;

/// Parse `source` into a surface [`PatternAst`], without validating or
/// compiling it.
///
/// # Errors
/// Returns [`DslError::Parse`] if `source` is not lexically or
/// structurally valid DSL. A pattern that parses successfully may still
/// fail [`validate_pattern`] — this function performs no semantic
/// checking.
pub fn parse_pattern(source: &str) -> Result<PatternAst, DslError> {
    parser::parse(source).map_err(DslError::Parse)
}

/// Validate an already-parsed [`PatternAst`], returning every diagnostic
/// found (which may be empty, warnings-only, or contain errors).
#[must_use]
pub fn validate_pattern(pattern: &PatternAst) -> Diagnostics {
    validate::validate(pattern)
}

/// Parse, validate, and compile a pattern from source text.
///
/// This is the primary entry point most callers want: it runs the full
/// pipeline and surfaces the first stage that fails as a [`DslError`].
///
/// # Errors
/// - [`DslError::Parse`] if `source` is not lexically or structurally
///   valid.
/// - [`DslError::Validation`] if `source` parses but fails semantic
///   validation (duplicate identifiers, invalid references, missing
///   required fields, incompatible predicates, etc.).
pub fn compile_str(source: &str) -> Result<CompiledPattern, DslError> {
    let ast = parse_pattern(source)?;
    compile::compile(&ast).map_err(DslError::Validation)
}

/// Read a pattern definition from `path` and [`compile_str`] it.
///
/// # Errors
/// - [`DslError::Io`] if `path` cannot be read.
/// - [`DslError::Parse`] / [`DslError::Validation`] as in [`compile_str`].
pub fn load_pattern_file(path: &Path) -> Result<CompiledPattern, DslError> {
    let source = std::fs::read_to_string(path).map_err(|e| DslError::Io {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;
    compile_str(&source)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
        pattern donation_attack version 1 {
            family: OracleManipulation
            severity: High
            tags: ["oracle", "donation"]
            references: ["SCWE-042"]

            evidence {
                required donation_transfer: token_transfer(direction: In, unexpected: true)
                required price_read: storage(changed: true, role: PriceOracle)
                optional attacker_flash_loan: call(kind: External)
            }

            constraint: AND(donation_transfer, price_read, NOT(attacker_flash_loan))
            sequence: [donation_transfer, price_read] within: 60
        }
    "#;

    #[test]
    fn end_to_end_compiles() {
        let compiled = compile_str(VALID).expect("should compile");
        assert_eq!(compiled.id.0, "donation_attack");
    }

    #[test]
    fn end_to_end_compiles_with_storage_call_kind_attribute() {
        let src = r#"
            pattern storage_call_kind_demo version 1 {
                family: DelegatecallStorageCollision
                severity: Critical
                tags: ["delegatecall"]
                references: ["SWC-112"]

                evidence {
                    required slot_write: storage(changed: true, slot: "0x00", call_kind: Delegate)
                }

                constraint: slot_write
            }
        "#;
        let compiled = compile_str(src).expect("should compile");
        assert_eq!(compiled.id.0, "storage_call_kind_demo");
    }

    #[test]
    fn end_to_end_parse_error() {
        let err = compile_str("pattern foo version 1 {").unwrap_err();
        assert!(matches!(err, DslError::Parse(_)));
    }

    #[test]
    fn end_to_end_validation_error() {
        let src = r"
            pattern p version 1 {
                evidence { required a: call(kind: External) }
            }
        ";
        let err = compile_str(src).unwrap_err();
        assert!(matches!(err, DslError::Validation(_)));
    }

    #[test]
    fn load_pattern_file_reports_io_error() {
        let err = load_pattern_file(Path::new("/nonexistent/path/pattern.rcdsl")).unwrap_err();
        assert!(matches!(err, DslError::Io { .. }));
    }

    #[test]
    fn load_pattern_file_reads_and_compiles() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("dsl_test_{}.rcdsl", std::process::id()));
        std::fs::write(&path, VALID).unwrap();
        let compiled = load_pattern_file(&path).expect("should load and compile");
        assert_eq!(compiled.id.0, "donation_attack");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn dsl_error_render_includes_reason() {
        let err = compile_str("pattern foo version 1 {").unwrap_err();
        let rendered = err.render("pattern foo version 1 {");
        assert!(!rendered.is_empty());
    }
}

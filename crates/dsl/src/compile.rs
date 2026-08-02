//! Compilation: turns a structurally-parsed [`PatternAst`] into an
//! immutable [`CompiledPattern`], after running it through
//! [`crate::validate::validate`].
//!
//! This is the only place name-based evidence references (as written by
//! a pattern author, and as parsed into [`crate::ast::BoolExpr`] /
//! [`crate::ast::SequenceConstraint`]) are resolved into the
//! index-based [`EvidenceRef`]s the compiled IR uses. Resolution can't
//! fail here: [`validate`] has already guaranteed every reference names
//! a declared evidence clause, so this stage is infallible with respect
//! to *references* — the only way [`compile`] returns `Err` is if
//! validation itself found a problem.
//!
//! Every `.expect(...)` in this module documents, at its call site, the
//! specific [`validate`] check that makes it unreachable in practice —
//! mirroring how `fact-model`'s own arena construction scopes
//! `#[allow(clippy::expect_used)]` per call with a `# Panics` note
//! (see `fact_model::arena::FactArenaBuilder::next_call_id`), rather
//! than reaching for `unwrap_or`-with-a-silently-wrong-default, which
//! would hide a real validation-compilation desync as a miscompiled
//! pattern instead of a loud panic.
#![allow(clippy::expect_used)]

use crate::ast::{AttrValue as AstAttrValue, BoolExpr, MetadataValue, PatternAst};
use crate::diagnostics::Diagnostics;
use crate::ir::{
    AttrValue, CompiledAttr, CompiledConstraint, CompiledEvidence, CompiledPattern,
    CompiledPredicate, CompiledSameCall, CompiledSequence, EvidenceRef, PatternFamily, PatternId,
    PatternVersion, Requiredness, Severity,
};
use crate::validate::validate;

/// Compile `pattern` into an immutable [`CompiledPattern`].
///
/// # Errors
/// Returns the [`Diagnostics`] produced by [`validate`] if `pattern`
/// fails semantic validation. Note this may include warnings alongside
/// errors; check
/// [`Diagnostics::has_errors`](crate::diagnostics::Diagnostics::has_errors)
/// to distinguish "did not compile" from "compiled with warnings" if
/// calling this directly rather than through [`crate::compile_str`].
///
/// # Panics
/// Does not panic on any input: every internal `.expect(...)` this
/// function (transitively) uses is guarded by a [`validate`] check run
/// first, documented at each call site. A panic here indicates a bug
/// where [`validate`] and [`compile`] have gone out of sync, not a
/// malformed `pattern`.
pub fn compile(pattern: &PatternAst) -> Result<CompiledPattern, Diagnostics> {
    let diags = validate(pattern);
    if diags.has_errors() {
        return Err(diags);
    }

    // Evidence clauses compile in declaration order; their position in
    // this Vec *is* their EvidenceRef, which is what makes reference
    // resolution below a simple lookup rather than needing a
    // separately-carried index.
    let evidence: Vec<CompiledEvidence> = pattern
        .evidence
        .iter()
        .map(|decl| CompiledEvidence {
            name: decl.name.value.clone(),
            requiredness: match decl.requiredness {
                crate::ast::Requiredness::Required => Requiredness::Required,
                crate::ast::Requiredness::Optional => Requiredness::Optional,
            },
            predicate: CompiledPredicate {
                kind: decl.predicate.kind.value.clone(),
                attributes: decl
                    .predicate
                    .attributes
                    .iter()
                    .map(|a| CompiledAttr {
                        name: a.name.value.clone(),
                        value: match &a.value {
                            AstAttrValue::Ident(s) => AttrValue::Ident(s.value.clone()),
                            AstAttrValue::Str(s) => AttrValue::Str(s.value.clone()),
                            AstAttrValue::Int(i) => AttrValue::Int(i.value),
                            AstAttrValue::Bool(b) => AttrValue::Bool(b.value),
                        },
                    })
                    .collect(),
            },
        })
        .collect();

    // `resolve` looks up an evidence name's declaration-order position
    // and narrows it into `EvidenceRef`'s `u32` index space. The lookup
    // itself cannot return `None`: `validate::validate` (just run above)
    // rejects every `BoolExpr`/`SequenceConstraint` reference that does
    // not name a declared evidence clause, so every name this closure is
    // ever called with is guaranteed present. The `u32` narrowing cannot
    // lose information in practice either: `pattern.evidence`'s length
    // is bounded by how many evidence clauses fit in a single parsed
    // source file, vastly short of `u32::MAX`.
    let resolve = |name: &str| -> EvidenceRef {
        let idx = pattern
            .evidence
            .iter()
            .position(|d| d.name.value == name)
            .expect("validate() guarantees every reference names a declared evidence clause");
        EvidenceRef(u32::try_from(idx).unwrap_or(u32::MAX))
    };

    let constraint = pattern.constraint.as_ref().map_or_else(
        || implicit_required_and(pattern, resolve),
        |expr| compile_bool_expr(expr, resolve),
    );

    let sequence = pattern.sequence.as_ref().map(|seq| CompiledSequence {
        steps: seq.steps.iter().map(|s| resolve(&s.value)).collect(),
        within_seconds: seq.within_seconds.and_then(|w| u32::try_from(w.value).ok()),
    });

    let same_call = pattern.same_call.as_ref().map(|sc| CompiledSameCall {
        members: sc.members.iter().map(|m| resolve(&m.value)).collect(),
    });

    // `family`/`severity` lookups: `validate::validate_metadata` rejects
    // a pattern missing either key, or where `family` isn't a bare
    // identifier, or where `severity` isn't one of
    // `schema::SEVERITY_VALUES` — so both `.expect(...)` calls below are
    // likewise unreachable for any `pattern` that passed the `validate`
    // call above.
    let family = pattern
        .metadata
        .iter()
        .find(|m| m.key.value == "family")
        .and_then(|m| match &m.value {
            MetadataValue::Ident(s) => Some(s.value.clone()),
            _ => None,
        })
        .expect("validate() guarantees `family` is present and is an identifier");

    let severity = pattern
        .metadata
        .iter()
        .find(|m| m.key.value == "severity")
        .and_then(|m| match &m.value {
            MetadataValue::Ident(s) => severity_from_str(&s.value),
            _ => None,
        })
        .expect("validate() guarantees `severity` is present and is a recognized value");

    let tags = dedup_strings(string_list(pattern, "tags"));
    let references = dedup_strings(string_list(pattern, "references"));

    Ok(CompiledPattern {
        id: PatternId(pattern.id.value.clone()),
        version: PatternVersion(
            u32::try_from(pattern.version.value).expect("validate() guarantees version >= 1"),
        ),
        family: PatternFamily(family),
        severity,
        tags,
        references,
        evidence,
        constraint,
        sequence,
        same_call,
    })
}

fn implicit_required_and(
    pattern: &PatternAst,
    resolve: impl Fn(&str) -> EvidenceRef,
) -> CompiledConstraint {
    let mut refs: Vec<CompiledConstraint> = pattern
        .evidence
        .iter()
        .filter(|d| d.requiredness == crate::ast::Requiredness::Required)
        .map(|d| CompiledConstraint::Evidence(resolve(&d.name.value)))
        .collect();
    if refs.len() == 1 {
        refs.pop().expect("length checked above")
    } else {
        CompiledConstraint::And(refs)
    }
}

fn compile_bool_expr(
    expr: &BoolExpr,
    resolve: impl Fn(&str) -> EvidenceRef + Copy,
) -> CompiledConstraint {
    match expr {
        BoolExpr::EvidenceRef(name) => CompiledConstraint::Evidence(resolve(&name.value)),
        BoolExpr::And(items, _) => CompiledConstraint::And(
            items
                .iter()
                .map(|i| compile_bool_expr(i, resolve))
                .collect(),
        ),
        BoolExpr::Or(items, _) => CompiledConstraint::Or(
            items
                .iter()
                .map(|i| compile_bool_expr(i, resolve))
                .collect(),
        ),
        BoolExpr::Not(inner, _) => {
            CompiledConstraint::Not(Box::new(compile_bool_expr(inner, resolve)))
        }
    }
}

fn string_list(pattern: &PatternAst, key: &str) -> Vec<String> {
    pattern
        .metadata
        .iter()
        .find(|m| m.key.value == key)
        .and_then(|m| match &m.value {
            MetadataValue::StrList(items) => Some(items.iter().map(|s| s.value.clone()).collect()),
            _ => None,
        })
        .unwrap_or_default()
}

fn dedup_strings(items: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    items
        .into_iter()
        .filter(|s| seen.insert(s.clone()))
        .collect()
}

fn severity_from_str(s: &str) -> Option<Severity> {
    match s {
        "Low" => Some(Severity::Low),
        "Medium" => Some(Severity::Medium),
        "High" => Some(Severity::High),
        "Critical" => Some(Severity::Critical),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    const VALID: &str = r#"
        pattern donation_attack version 1 {
            family: OracleManipulation
            severity: High
            tags: ["oracle", "oracle", "donation"]
            references: ["SCWE-042"]

            evidence {
                required donation_transfer: token_transfer(direction: In, unexpected: true)
                required price_read: storage(changed: true, role: PriceOracle)
                optional attacker_flash_loan: call(kind: External)
            }

            constraint: AND(donation_transfer, price_read, NOT(attacker_flash_loan))
            sequence: [donation_transfer, price_read] within: 60
            same_call: [donation_transfer, price_read]
        }
    "#;

    #[test]
    fn compiles_valid_pattern() {
        let ast = parse(VALID).unwrap();
        let compiled = compile(&ast).unwrap();
        assert_eq!(compiled.id.0, "donation_attack");
        assert_eq!(compiled.version.0, 1);
        assert_eq!(compiled.family.0, "OracleManipulation");
        assert_eq!(compiled.severity, Severity::High);
        assert_eq!(
            compiled.tags,
            vec!["oracle".to_string(), "donation".to_string()]
        );
        assert_eq!(compiled.evidence.len(), 3);
        let seq = compiled.sequence.unwrap();
        assert_eq!(seq.steps, vec![EvidenceRef(0), EvidenceRef(1)]);
        assert_eq!(seq.within_seconds, Some(60));
        let same_call = compiled.same_call.unwrap();
        assert_eq!(same_call.members, vec![EvidenceRef(0), EvidenceRef(1)]);
    }

    #[test]
    fn pattern_without_same_call_compiles_to_none() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: Low
                evidence { required a: call(kind: External) }
            }
        ";
        let ast = parse(src).unwrap();
        let compiled = compile(&ast).unwrap();
        assert!(compiled.same_call.is_none());
    }

    #[test]
    fn constraint_resolves_to_evidence_refs() {
        let ast = parse(VALID).unwrap();
        let compiled = compile(&ast).unwrap();
        match &compiled.constraint {
            CompiledConstraint::And(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], CompiledConstraint::Evidence(EvidenceRef(0)));
                assert_eq!(items[1], CompiledConstraint::Evidence(EvidenceRef(1)));
                assert_eq!(
                    items[2],
                    CompiledConstraint::Not(Box::new(CompiledConstraint::Evidence(EvidenceRef(2))))
                );
            }
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn implicit_constraint_is_and_of_required() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: Low
                evidence {
                    required a: call(kind: External)
                    required b: call(kind: External)
                    optional c: call(kind: External)
                }
            }
        ";
        let ast = parse(src).unwrap();
        let compiled = compile(&ast).unwrap();
        assert_eq!(
            compiled.constraint,
            CompiledConstraint::And(vec![
                CompiledConstraint::Evidence(EvidenceRef(0)),
                CompiledConstraint::Evidence(EvidenceRef(1)),
            ])
        );
    }

    #[test]
    fn implicit_constraint_single_required_is_bare_evidence() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: Low
                evidence { required a: call(kind: External) }
            }
        ";
        let ast = parse(src).unwrap();
        let compiled = compile(&ast).unwrap();
        assert_eq!(
            compiled.constraint,
            CompiledConstraint::Evidence(EvidenceRef(0))
        );
    }

    #[test]
    fn invalid_pattern_fails_to_compile() {
        let src = r"
            pattern p version 1 {
                evidence { required a: call(kind: External) }
            }
        ";
        let ast = parse(src).unwrap();
        let err = compile(&ast).unwrap_err();
        assert!(err.has_errors());
    }

    #[test]
    fn resolve_looks_up_compiled_evidence() {
        let ast = parse(VALID).unwrap();
        let compiled = compile(&ast).unwrap();
        let ev = compiled.resolve(EvidenceRef(0)).unwrap();
        assert_eq!(ev.name, "donation_transfer");
        assert!(compiled.resolve(EvidenceRef(99)).is_none());
    }
}

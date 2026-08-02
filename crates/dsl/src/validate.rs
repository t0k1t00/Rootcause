//! Semantic validation: everything structural parsing cannot catch,
//! because it requires knowing the meaning of names, not just their
//! shape.
//!
//! Unlike [`crate::parser`], this stage never stops at the first
//! problem: it walks the whole pattern and accumulates every diagnostic
//! it finds, so a pattern author fixing validation errors doesn't have
//! to re-run validation once per mistake.

use std::collections::{HashMap, HashSet};

use crate::ast::{AttrValue, BoolExpr, MetadataValue, PatternAst, Predicate, Requiredness};
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::schema::{self, AttrShape};

/// Validate `pattern`, returning every diagnostic found.
///
/// An empty [`Diagnostics`] means the pattern is valid and safe to pass
/// to [`crate::compile::compile`]. A non-empty result may contain only
/// warnings (compilable, but worth a pattern author's attention) or may
/// contain errors (not compilable) — check
/// [`Diagnostics::has_errors`](crate::diagnostics::Diagnostics::has_errors).
#[must_use]
pub fn validate(pattern: &PatternAst) -> Diagnostics {
    let mut diags = Diagnostics::new();

    validate_metadata(pattern, &mut diags);
    let evidence_names = validate_evidence(pattern, &mut diags);
    validate_constraint(pattern, &evidence_names, &mut diags);
    validate_sequence(pattern, &evidence_names, &mut diags);
    validate_same_call(pattern, &evidence_names, &mut diags);

    diags
}

fn validate_metadata(pattern: &PatternAst, diags: &mut Diagnostics) {
    if pattern.version.value < 1 {
        diags.push(Diagnostic::error(
            pattern.version.span,
            "pattern version",
            format!(
                "version must be a positive integer, found `{}`",
                pattern.version.value
            ),
        ));
    }

    let mut seen: HashMap<&str, _> = HashMap::new();
    let mut has_family = false;
    let mut has_severity = false;

    for entry in &pattern.metadata {
        let key = entry.key.value.as_str();
        if let Some(first_span) = seen.get(key) {
            diags.push(
                Diagnostic::error(
                    entry.key.span,
                    format!("metadata key `{key}`"),
                    "duplicate metadata key",
                )
                .with_suggestion(format!(
                    "remove this entry; `{key}` was already set at {first_span}"
                )),
            );
            continue;
        }
        seen.insert(key, entry.key.span);
        validate_metadata_entry(entry, key, diags, &mut has_family, &mut has_severity);
    }

    if !has_family {
        diags.push(
            Diagnostic::error(
                pattern.span,
                "pattern metadata",
                "missing required `family` entry",
            )
            .with_suggestion("add `family: <FamilyName>` to the pattern body"),
        );
    }
    if !has_severity {
        diags.push(
            Diagnostic::error(
                pattern.span,
                "pattern metadata",
                "missing required `severity` entry",
            )
            .with_suggestion(format!(
                "add `severity: <level>`, one of: {}",
                schema::SEVERITY_VALUES.join(", ")
            )),
        );
    }
}

/// Validates one metadata `key: value` entry's shape against what that
/// key expects, setting `has_family`/`has_severity` when the
/// corresponding key is seen (regardless of whether its value is
/// well-formed, so the "missing required entry" check below doesn't
/// double-report a malformed-but-present entry as also missing).
fn validate_metadata_entry(
    entry: &crate::ast::MetadataEntry,
    key: &str,
    diags: &mut Diagnostics,
    has_family: &mut bool,
    has_severity: &mut bool,
) {
    match key {
        "family" => {
            *has_family = true;
            if !matches!(&entry.value, MetadataValue::Ident(_)) {
                diags.push(Diagnostic::error(
                    entry.span,
                    "`family` metadata",
                    "expected a bare identifier, e.g. `family: OracleManipulation`",
                ));
            }
        }
        "severity" => {
            *has_severity = true;
            validate_severity_value(entry, diags);
        }
        "tags" | "references" => {
            if !matches!(&entry.value, MetadataValue::StrList(_)) {
                diags.push(Diagnostic::error(
                    entry.span,
                    format!("`{key}` metadata"),
                    format!("expected a string list, e.g. `{key}: [\"a\", \"b\"]`"),
                ));
            }
        }
        other => {
            diags.push(Diagnostic::warning(
                entry.key.span,
                format!("metadata key `{other}`"),
                "not a recognized metadata key; it will be ignored by compilation",
            ));
        }
    }
}

fn validate_severity_value(entry: &crate::ast::MetadataEntry, diags: &mut Diagnostics) {
    match &entry.value {
        MetadataValue::Ident(ident) => {
            if !schema::SEVERITY_VALUES.contains(&ident.value.as_str()) {
                diags.push(
                    Diagnostic::error(
                        ident.span,
                        format!("`severity` value `{}`", ident.value),
                        "not a recognized severity",
                    )
                    .with_suggestion(format!(
                        "use one of: {}",
                        schema::SEVERITY_VALUES.join(", ")
                    )),
                );
            }
        }
        _ => diags.push(Diagnostic::error(
            entry.span,
            "`severity` metadata",
            "expected a bare identifier, e.g. `severity: High`",
        )),
    }
}

/// Validates evidence declarations and their predicates, returning the
/// set of declared (non-duplicate) evidence names for later stages to
/// check references against.
fn validate_evidence(pattern: &PatternAst, diags: &mut Diagnostics) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut has_required = false;

    for decl in &pattern.evidence {
        if decl.requiredness == Requiredness::Required {
            has_required = true;
        }
        if !names.insert(decl.name.value.clone()) {
            diags.push(
                Diagnostic::error(
                    decl.name.span,
                    format!("evidence name `{}`", decl.name.value),
                    "duplicate evidence declaration name",
                )
                .with_suggestion("evidence names must be unique within a pattern"),
            );
        }
        validate_predicate(&decl.predicate, diags);
    }

    if pattern.evidence.is_empty() {
        diags.push(Diagnostic::error(
            pattern.span,
            "pattern",
            "pattern declares no evidence at all; a pattern with nothing to ground can never \
             produce a classification",
        ));
    } else if !has_required {
        diags.push(
            Diagnostic::error(
                pattern.span,
                "pattern evidence",
                "pattern declares only optional evidence; at least one `required` clause is \
                 needed, or every match is trivially unfalsifiable",
            )
            .with_suggestion("mark at least one evidence clause `required`"),
        );
    }

    names
}

fn validate_predicate(predicate: &Predicate, diags: &mut Diagnostics) {
    let Some(schema) = schema::lookup(&predicate.kind.value) else {
        let known: Vec<&str> = schema::ALL.iter().map(|s| s.kind).collect();
        diags.push(
            Diagnostic::error(
                predicate.kind.span,
                format!("predicate kind `{}`", predicate.kind.value),
                "not a recognized predicate kind",
            )
            .with_suggestion(format!("use one of: {}", known.join(", "))),
        );
        return;
    };

    validate_predicate_attrs(predicate, schema, diags);
    validate_required_attrs_present(predicate, schema, diags);
    validate_predicate_incompatibilities(predicate, schema, diags);
}

fn validate_predicate_attrs(
    predicate: &Predicate,
    schema: &crate::schema::PredicateSchema,
    diags: &mut Diagnostics,
) {
    let mut seen = HashSet::new();
    for attr in &predicate.attributes {
        if !seen.insert(attr.name.value.clone()) {
            diags.push(Diagnostic::error(
                attr.name.span,
                format!("attribute `{}`", attr.name.value),
                "duplicate attribute within predicate",
            ));
            continue;
        }
        let Some(attr_schema) = schema.attrs.iter().find(|a| a.name == attr.name.value) else {
            let known: Vec<&str> = schema.attrs.iter().map(|a| a.name).collect();
            diags.push(
                Diagnostic::error(
                    attr.name.span,
                    format!("attribute `{}` on `{}`", attr.name.value, schema.kind),
                    "not a recognized attribute for this predicate kind",
                )
                .with_suggestion(format!("use one of: {}", known.join(", "))),
            );
            continue;
        };

        let shape_ok = match (&attr.value, attr_schema.shape) {
            (AttrValue::Str(_), AttrShape::Str)
            | (AttrValue::Int(_), AttrShape::Int)
            | (AttrValue::Bool(_), AttrShape::Bool) => true,
            (AttrValue::Ident(ident), AttrShape::Enum(values)) => {
                if !values.contains(&ident.value.as_str()) {
                    diags.push(
                        Diagnostic::error(
                            ident.span,
                            format!("value `{}` for `{}`", ident.value, attr.name.value),
                            "not a recognized value for this attribute",
                        )
                        .with_suggestion(format!("use one of: {}", values.join(", "))),
                    );
                }
                // Already reported a more specific diagnostic above (or
                // the value was fine); either way this is not the
                // generic wrong-shape case handled below.
                true
            }
            _ => false,
        };
        if !shape_ok {
            diags.push(Diagnostic::error(
                attr.span,
                format!("attribute `{}`", attr.name.value),
                format!(
                    "value has the wrong shape for this attribute (expected {})",
                    describe_shape(attr_schema.shape)
                ),
            ));
        }
    }
}

fn validate_required_attrs_present(
    predicate: &Predicate,
    schema: &crate::schema::PredicateSchema,
    diags: &mut Diagnostics,
) {
    for attr_schema in schema.attrs.iter().filter(|a| a.required) {
        if !predicate
            .attributes
            .iter()
            .any(|a| a.name.value == attr_schema.name)
        {
            diags.push(
                Diagnostic::error(
                    predicate.span,
                    format!("predicate `{}`", schema.kind),
                    format!("missing required attribute `{}`", attr_schema.name),
                )
                .with_suggestion(format!(
                    "add `{}: ...` inside `{}(...)`",
                    attr_schema.name, schema.kind
                )),
            );
        }
    }
}

/// Incompatible-predicates check: a static call cannot reenter with
/// observable state change, so declaring both `kind: StaticCall` and
/// `reentrant: true` on the same `call` predicate describes a
/// structurally impossible fact and would silently never ground.
fn validate_predicate_incompatibilities(
    predicate: &Predicate,
    schema: &crate::schema::PredicateSchema,
    diags: &mut Diagnostics,
) {
    if schema.kind != "call" {
        return;
    }
    let is_static = predicate.attributes.iter().any(|a| {
        a.name.value == "kind" && matches!(&a.value, AttrValue::Ident(i) if i.value == "StaticCall")
    });
    let is_reentrant = predicate
        .attributes
        .iter()
        .any(|a| a.name.value == "reentrant" && matches!(&a.value, AttrValue::Bool(b) if b.value));
    if is_static && is_reentrant {
        diags.push(
            Diagnostic::error(
                predicate.span,
                "`call` predicate",
                "`kind: StaticCall` and `reentrant: true` are incompatible: a static call \
                 cannot perform a state-changing reentrant call by construction",
            )
            .with_suggestion("drop `reentrant: true`, or change `kind` to `External`/`Delegate`"),
        );
    }
}

fn validate_constraint(
    pattern: &PatternAst,
    evidence_names: &HashSet<String>,
    diags: &mut Diagnostics,
) {
    let Some(constraint) = &pattern.constraint else {
        return;
    };
    let mut referenced = HashSet::new();
    check_bool_expr(constraint, evidence_names, &mut referenced, diags);

    for decl in &pattern.evidence {
        if decl.requiredness == Requiredness::Required && !referenced.contains(&decl.name.value) {
            diags.push(
                Diagnostic::warning(
                    decl.name.span,
                    format!("required evidence `{}`", decl.name.value),
                    "required evidence clause is not referenced by the pattern's `constraint` \
                     expression; it will still be enforced, but the constraint expression \
                     reads as if it weren't load-bearing",
                )
                .with_suggestion(format!(
                    "reference `{}` inside the `constraint:` expression, or remove `constraint:` \
                     to use the implicit AND-of-required default",
                    decl.name.value
                )),
            );
        }
    }
}

fn check_bool_expr(
    expr: &BoolExpr,
    evidence_names: &HashSet<String>,
    referenced: &mut HashSet<String>,
    diags: &mut Diagnostics,
) {
    match expr {
        BoolExpr::EvidenceRef(name) => {
            if evidence_names.contains(&name.value) {
                referenced.insert(name.value.clone());
            } else {
                diags.push(
                    Diagnostic::error(
                        name.span,
                        format!("evidence reference `{}`", name.value),
                        "references an evidence clause that is not declared in this pattern",
                    )
                    .with_suggestion("check for a typo, or declare this evidence clause"),
                );
            }
        }
        BoolExpr::And(items, _) | BoolExpr::Or(items, _) => {
            for item in items {
                check_bool_expr(item, evidence_names, referenced, diags);
            }
        }
        BoolExpr::Not(inner, _) => check_bool_expr(inner, evidence_names, referenced, diags),
    }
}

fn validate_sequence(
    pattern: &PatternAst,
    evidence_names: &HashSet<String>,
    diags: &mut Diagnostics,
) {
    let Some(sequence) = &pattern.sequence else {
        return;
    };

    if sequence.steps.len() < 2 {
        diags.push(
            Diagnostic::error(
                sequence.span,
                "sequence constraint",
                "a sequence constraint needs at least two steps to express an ordering",
            )
            .with_suggestion("add another step, or remove the `sequence:` section entirely"),
        );
    }

    let mut seen = HashSet::new();
    for step in &sequence.steps {
        if !evidence_names.contains(&step.value) {
            diags.push(
                Diagnostic::error(
                    step.span,
                    format!("sequence step `{}`", step.value),
                    "references an evidence clause that is not declared in this pattern",
                )
                .with_suggestion("check for a typo, or declare this evidence clause"),
            );
        }
        if !seen.insert(step.value.clone()) {
            diags.push(Diagnostic::error(
                step.span,
                format!("sequence step `{}`", step.value),
                "the same evidence clause appears more than once in the sequence; a single \
                 grounded occurrence cannot satisfy two positions in a temporal ordering",
            ));
        }
    }

    if let Some(within) = &sequence.within_seconds {
        if within.value <= 0 {
            diags.push(Diagnostic::error(
                within.span,
                "sequence `within` clause",
                "the time window must be a positive number of seconds",
            ));
        }
    }
}

fn validate_same_call(
    pattern: &PatternAst,
    evidence_names: &HashSet<String>,
    diags: &mut Diagnostics,
) {
    let Some(same_call) = &pattern.same_call else {
        return;
    };

    if same_call.members.len() < 2 {
        diags.push(
            Diagnostic::error(
                same_call.span,
                "same_call constraint",
                "a same_call constraint needs at least two members to express a correlation",
            )
            .with_suggestion("add another member, or remove the `same_call:` section entirely"),
        );
    }

    let mut seen = HashSet::new();
    for member in &same_call.members {
        if !evidence_names.contains(&member.value) {
            diags.push(
                Diagnostic::error(
                    member.span,
                    format!("same_call member `{}`", member.value),
                    "references an evidence clause that is not declared in this pattern",
                )
                .with_suggestion("check for a typo, or declare this evidence clause"),
            );
        }
        if !seen.insert(member.value.clone()) {
            diags.push(Diagnostic::error(
                member.span,
                format!("same_call member `{}`", member.value),
                "the same evidence clause appears more than once in the same_call group; a \
                 single occurrence is trivially the same call as itself",
            ));
        }
    }
}

const fn describe_shape(shape: AttrShape) -> &'static str {
    match shape {
        AttrShape::Enum(_) => "an identifier",
        AttrShape::Str => "a string literal",
        AttrShape::Int => "an integer literal",
        AttrShape::Bool => "a boolean literal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn diags_for(src: &str) -> Diagnostics {
        let ast = parse(src).expect("should parse structurally");
        validate(&ast)
    }

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
            same_call: [donation_transfer, price_read]
        }
    "#;

    #[test]
    fn valid_pattern_has_no_errors() {
        let diags = diags_for(VALID);
        assert!(!diags.has_errors(), "{diags:?}");
    }

    #[test]
    fn missing_family_and_severity_reported() {
        let src = r"
            pattern p version 1 {
                evidence { required a: call(kind: External) }
            }
        ";
        let diags = diags_for(src);
        assert!(diags.iter().any(|d| d.reason.contains("`family`")));
        assert!(diags.iter().any(|d| d.reason.contains("`severity`")));
    }

    #[test]
    fn duplicate_metadata_key_reported() {
        let src = r"
            pattern p version 1 {
                family: X
                family: Y
                severity: High
                evidence { required a: call(kind: External) }
            }
        ";
        let diags = diags_for(src);
        assert!(diags
            .iter()
            .any(|d| d.reason.contains("duplicate metadata key")));
    }

    #[test]
    fn invalid_severity_reported() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: Extreme
                evidence { required a: call(kind: External) }
            }
        ";
        let diags = diags_for(src);
        assert!(diags
            .iter()
            .any(|d| d.reason.contains("not a recognized severity")));
    }

    #[test]
    fn duplicate_evidence_name_reported() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: High
                evidence {
                    required a: call(kind: External)
                    required a: call(kind: External)
                }
            }
        ";
        let diags = diags_for(src);
        assert!(diags
            .iter()
            .any(|d| d.reason.contains("duplicate evidence")));
    }

    #[test]
    fn only_optional_evidence_reported() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: High
                evidence { optional a: call(kind: External) }
            }
        ";
        let diags = diags_for(src);
        assert!(diags
            .iter()
            .any(|d| d.reason.contains("only optional evidence")));
    }

    #[test]
    fn unknown_predicate_kind_reported() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: High
                evidence { required a: frobnicate(x: 1) }
            }
        ";
        let diags = diags_for(src);
        assert!(diags
            .iter()
            .any(|d| d.reason.contains("not a recognized predicate kind")));
    }

    #[test]
    fn unknown_attribute_reported() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: High
                evidence { required a: call(kind: External, bogus: true) }
            }
        ";
        let diags = diags_for(src);
        assert!(diags
            .iter()
            .any(|d| d.reason.contains("not a recognized attribute")));
    }

    #[test]
    fn missing_required_attribute_reported() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: High
                evidence { required a: call(reentrant: true) }
            }
        ";
        let diags = diags_for(src);
        assert!(diags
            .iter()
            .any(|d| d.reason.contains("missing required attribute")));
    }

    #[test]
    fn wrong_shape_attribute_reported() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: High
                evidence { required a: call(kind: External, reentrant: 1) }
            }
        ";
        let diags = diags_for(src);
        assert!(diags.iter().any(|d| d.reason.contains("wrong shape")));
    }

    #[test]
    fn invalid_enum_value_reported() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: High
                evidence { required a: call(kind: Teleport) }
            }
        ";
        let diags = diags_for(src);
        assert!(diags
            .iter()
            .any(|d| d.reason.contains("not a recognized value")));
    }

    #[test]
    fn incompatible_static_reentrant_reported() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: High
                evidence { required a: call(kind: StaticCall, reentrant: true) }
            }
        ";
        let diags = diags_for(src);
        assert!(diags.iter().any(|d| d.reason.contains("incompatible")));
    }

    #[test]
    fn invalid_constraint_reference_reported() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: High
                evidence { required a: call(kind: External) }
                constraint: AND(a, ghost)
            }
        ";
        let diags = diags_for(src);
        assert!(diags.iter().any(|d| d.reason.contains("not declared")));
    }

    #[test]
    fn invalid_sequence_reference_reported() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: High
                evidence {
                    required a: call(kind: External)
                    required b: call(kind: External)
                }
                sequence: [a, ghost]
            }
        ";
        let diags = diags_for(src);
        assert!(diags.iter().any(|d| d.reason.contains("not declared")));
    }

    #[test]
    fn duplicate_sequence_step_reported() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: High
                evidence {
                    required a: call(kind: External)
                    required b: call(kind: External)
                }
                sequence: [a, a]
            }
        ";
        let diags = diags_for(src);
        assert!(diags.iter().any(|d| d.reason.contains("more than once")));
    }

    #[test]
    fn single_step_sequence_reported() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: High
                evidence { required a: call(kind: External) }
                sequence: [a]
            }
        ";
        let diags = diags_for(src);
        assert!(diags
            .iter()
            .any(|d| d.reason.contains("at least two steps")));
    }

    #[test]
    fn invalid_same_call_reference_reported() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: High
                evidence {
                    required a: call(kind: External)
                    required b: call(kind: External)
                }
                same_call: [a, ghost]
            }
        ";
        let diags = diags_for(src);
        assert!(diags.iter().any(|d| d.reason.contains("not declared")));
    }

    #[test]
    fn duplicate_same_call_member_reported() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: High
                evidence {
                    required a: call(kind: External)
                    required b: call(kind: External)
                }
                same_call: [a, a]
            }
        ";
        let diags = diags_for(src);
        assert!(diags.iter().any(|d| d.reason.contains("more than once")));
    }

    #[test]
    fn single_member_same_call_reported() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: High
                evidence { required a: call(kind: External) }
                same_call: [a]
            }
        ";
        let diags = diags_for(src);
        assert!(diags
            .iter()
            .any(|d| d.reason.contains("at least two members")));
    }

    #[test]
    fn pattern_without_same_call_has_no_errors() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: High
                evidence { required a: call(kind: External) }
            }
        ";
        let diags = diags_for(src);
        assert!(!diags.has_errors(), "{diags:?}");
    }

    #[test]
    fn unreferenced_required_evidence_warns() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: High
                evidence {
                    required a: call(kind: External)
                    required b: call(kind: External)
                }
                constraint: AND(a, a)
            }
        ";
        // `b` is required but never referenced by the constraint.
        let diags = diags_for(src);
        assert!(!diags.has_errors());
        assert!(diags.iter().any(|d| d
            .reason
            .contains("not referenced by the pattern's `constraint`")));
    }

    #[test]
    fn unknown_metadata_key_warns_not_errors() {
        let src = r#"
            pattern p version 1 {
                family: X
                severity: High
                author: "someone"
                evidence { required a: call(kind: External) }
            }
        "#;
        let diags = diags_for(src);
        assert!(!diags.has_errors(), "{diags:?}");
        assert!(diags
            .iter()
            .any(|d| d.reason.contains("not a recognized metadata key")));
    }
}

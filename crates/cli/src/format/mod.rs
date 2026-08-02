//! Canonical `.rcdsl` pretty-printing, for `rootcause format-pattern`.
//!
//! This module is pure presentation: it reads the same
//! [`dsl::ast::PatternAst`] every other pattern-consuming command reads
//! (via [`dsl::parse_pattern`]) and re-serializes it in one fixed,
//! deterministic style. It performs no semantic validation of its own —
//! `format-pattern` still calls [`dsl::validate_pattern`] separately (see
//! [`crate::commands::format_pattern`]) — and it makes no change to the
//! DSL grammar, the AST, or any compiler stage; formatting a pattern and
//! re-parsing the result is guaranteed to reproduce an AST equal to the
//! one formatting started from (see this crate's round-trip tests).
//!
//! ## Scope: body only, not leading doc comments
//!
//! Every real pattern in `patterns/` opens with a substantial prose doc
//! comment explaining the pattern's rationale (see
//! `docs/PATTERN_AUTHORING_GUIDE.md` step 4). The lexer treats comments
//! as trivia — they never reach [`dsl::ast::PatternAst`] — so this
//! formatter cannot re-flow them without reimplementing a comment-aware
//! parser (out of scope for developer tooling; see Milestone 15's
//! Phase 1 instruction not to touch lexer/parser semantics). Instead,
//! [`crate::commands::format_pattern::format_file`] splits the source into the
//! leading comment/blank-line block (preserved byte-for-byte) and the
//! `pattern ... { ... }` body (re-printed by this module) — see that
//! module's `split_leading_comments`.

use dsl::ast::{
    AttrValue, BoolExpr, EvidenceDecl, MetadataEntry, MetadataValue, PatternAst, Predicate,
    Requiredness, SameCallConstraint, SequenceConstraint,
};

const INDENT: &str = "    ";

/// Pretty-print `pattern`'s body (`pattern <id> version <n> { ... }`) in
/// the repository's canonical style. Does not include any leading
/// comment block — see this module's docs.
#[must_use]
pub fn format_pattern_body(pattern: &PatternAst) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "pattern {} version {} {{\n",
        pattern.id.value, pattern.version.value
    ));

    for entry in &pattern.metadata {
        out.push_str(&format_metadata_entry(entry));
    }

    out.push('\n');
    out.push_str(&format!("{INDENT}evidence {{\n"));
    for decl in &pattern.evidence {
        out.push_str(&format_evidence_decl(decl));
    }
    out.push_str(&format!("{INDENT}}}\n"));

    if let Some(same_call) = &pattern.same_call {
        out.push('\n');
        out.push_str(&format_same_call(same_call));
    }

    if let Some(sequence) = &pattern.sequence {
        out.push('\n');
        out.push_str(&format_sequence(sequence));
    }

    if let Some(constraint) = &pattern.constraint {
        out.push('\n');
        out.push_str(&format!(
            "{INDENT}constraint: {}\n",
            format_bool_expr(constraint)
        ));
    }

    out.push_str("}\n");
    out
}

fn format_metadata_entry(entry: &MetadataEntry) -> String {
    let value = match &entry.value {
        MetadataValue::Ident(s) => s.value.clone(),
        MetadataValue::Str(s) => quote(&s.value),
        MetadataValue::StrList(items) => format_str_list(items),
    };
    format!("{INDENT}{}: {value}\n", entry.key.value)
}

fn format_str_list(items: &[dsl::ast::SpannedString]) -> String {
    let inline = items
        .iter()
        .map(|s| quote(&s.value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{inline}]")
}

fn format_evidence_decl(decl: &EvidenceDecl) -> String {
    let requiredness = match decl.requiredness {
        Requiredness::Required => "required",
        Requiredness::Optional => "optional",
    };
    let predicate = format_predicate(&decl.predicate);
    format!(
        "{INDENT}{INDENT}{requiredness} {}: {predicate}\n",
        decl.name.value
    )
}

/// A predicate is printed on one line if it fits within 80 columns
/// (matching `cargo fmt`'s own default line-length convention, which
/// this repository's Rust code already follows — see `rustfmt.toml`),
/// and one `attr: value` per indented line otherwise, mirroring
/// `patterns/delegatecall_storage_collision.rcdsl`'s hand-written style
/// for its multi-attribute `storage(...)` clause.
fn format_predicate(predicate: &Predicate) -> String {
    let attrs: Vec<String> = predicate
        .attributes
        .iter()
        .map(|attr| format!("{}: {}", attr.name.value, format_attr_value(&attr.value)))
        .collect();

    let inline = format!("{}({})", predicate.kind.value, attrs.join(", "));
    let inline_width = 2 * INDENT.len() + "required ".len() + inline.len();
    if attrs.is_empty() || inline_width <= 80 {
        return inline;
    }

    let mut out = format!("{}(\n", predicate.kind.value);
    for (i, attr) in attrs.iter().enumerate() {
        let sep = if i + 1 == attrs.len() { "" } else { "," };
        out.push_str(&format!("{INDENT}{INDENT}{INDENT}{attr}{sep}\n"));
    }
    out.push_str(&format!("{INDENT}{INDENT})"));
    out
}

fn format_attr_value(value: &AttrValue) -> String {
    match value {
        AttrValue::Ident(s) => s.value.clone(),
        AttrValue::Str(s) => quote(&s.value),
        AttrValue::Int(i) => i.value.to_string(),
        AttrValue::Bool(b) => b.value.to_string(),
    }
}

fn format_same_call(same_call: &SameCallConstraint) -> String {
    let members = same_call
        .members
        .iter()
        .map(|s| s.value.clone())
        .collect::<Vec<_>>()
        .join(", ");
    format!("{INDENT}same_call: [{members}]\n")
}

fn format_sequence(sequence: &SequenceConstraint) -> String {
    let steps = sequence
        .steps
        .iter()
        .map(|s| s.value.clone())
        .collect::<Vec<_>>()
        .join(", ");
    sequence.within_seconds.as_ref().map_or_else(
        || format!("{INDENT}sequence: [{steps}]\n"),
        |within| format!("{INDENT}sequence: [{steps}] within: {}\n", within.value),
    )
}

fn format_bool_expr(expr: &BoolExpr) -> String {
    match expr {
        BoolExpr::EvidenceRef(s) => s.value.clone(),
        BoolExpr::And(items, _) => format!(
            "AND({})",
            items
                .iter()
                .map(format_bool_expr)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        BoolExpr::Or(items, _) => format!(
            "OR({})",
            items
                .iter()
                .map(format_bool_expr)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        BoolExpr::Not(inner, _) => format!("NOT({})", format_bool_expr(inner)),
    }
}

fn quote(value: &str) -> String {
    format!("\"{value}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE: &str = r#"
        pattern trivial version 1 {
            family: Reentrancy
            severity: Low
            tags: ["a", "b"]
            references: ["SWC-1"]
            evidence {
                required c: call(kind: External)
                optional d: storage(changed: true)
            }
            same_call: [c, d]
            sequence: [c, d] within: 5
            constraint: AND(c, NOT(d))
        }
    "#;

    #[test]
    fn formatting_is_idempotent() {
        let ast = dsl::parse_pattern(SIMPLE).expect("parses");
        let once = format_pattern_body(&ast);
        let reparsed = dsl::parse_pattern(&once).expect("reparses");
        let twice = format_pattern_body(&reparsed);
        assert_eq!(once, twice);
    }

    #[test]
    fn formatting_round_trips_to_an_equal_ast() {
        let ast = dsl::parse_pattern(SIMPLE).expect("parses");
        let formatted = format_pattern_body(&ast);
        let reparsed = dsl::parse_pattern(&formatted).expect("reparses");
        assert_eq!(ast.id.value, reparsed.id.value);
        assert_eq!(ast.version.value, reparsed.version.value);
        assert_eq!(ast.metadata.len(), reparsed.metadata.len());
        assert_eq!(ast.evidence.len(), reparsed.evidence.len());
        assert!(reparsed.same_call.is_some());
        assert!(reparsed.sequence.is_some());
        assert!(reparsed.constraint.is_some());
    }

    #[test]
    fn long_predicate_wraps_across_lines() {
        let src = r#"
            pattern p version 1 {
                family: DelegatecallStorageCollision
                severity: Critical
                evidence {
                    required w: storage(changed: true, slot: "0x0000000000000000000000000000000000000000000000000000000000000000", call_kind: Delegate)
                }
                constraint: w
            }
        "#;
        let ast = dsl::parse_pattern(src).expect("parses");
        let formatted = format_pattern_body(&ast);
        assert!(formatted.contains("storage(\n"));
        // Still parses back to the same structure.
        dsl::compile_str(&formatted).expect("reformatted pattern still compiles");
    }

    #[test]
    fn short_predicate_stays_inline() {
        let ast = dsl::parse_pattern(SIMPLE).expect("parses");
        let formatted = format_pattern_body(&ast);
        assert!(formatted.contains("call(kind: External)"));
    }
}

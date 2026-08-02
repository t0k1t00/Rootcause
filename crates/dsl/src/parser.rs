//! Structural parsing: the token stream produced by [`crate::lexer`] into
//! the surface [`crate::ast::PatternAst`].
//!
//! This stage enforces the DSL's grammar shape only — braces balance,
//! sections appear where expected, predicates look like
//! `kind(attr: value, ...)`. It performs **no semantic validation**
//! (duplicate names, unknown evidence references, unknown predicate
//! kinds); that is entirely [`crate::validate`]'s job. Where a list of
//! items is expected (metadata entries, evidence declarations), the
//! parser recovers from a malformed item by skipping to the next
//! plausible item boundary and continuing, so a single pattern file can
//! surface more than one structural diagnostic per parse rather than
//! stopping at the first.
//!
//! Several list-item parse points below use `if let Some(x) = ... {
//! push(x) } else { recover_to(...) }` rather than clippy's preferred
//! `Option::map_or_else`: both branches call `self.recover_to(...)` or
//! `Vec::push`, side-effecting mutations on `self`/the accumulator that
//! read far more clearly as a plain `if`/`else` than as two closures
//! passed to `map_or_else`, so that lint is allowed at module scope.
#![allow(clippy::option_if_let_else)]

use crate::ast::{
    AttrValue, BoolExpr, EvidenceDecl, MetadataEntry, MetadataValue, PatternAst, Predicate,
    PredicateAttr, Requiredness, SameCallConstraint, SequenceConstraint, SpannedBool, SpannedInt,
    SpannedString,
};
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::lexer::{self, Token, TokenKind};
use crate::span::Span;

/// Parse `source` into a [`PatternAst`].
///
/// # Errors
/// Returns [`Diagnostics`] (guaranteed non-empty, with at least one
/// [`crate::diagnostics::Severity::Error`]) if `source` is not
/// lexically or structurally valid. This stage does not perform semantic
/// validation; a structurally valid-but-semantically-broken pattern
/// parses successfully and is caught later by [`crate::validate`].
pub fn parse(source: &str) -> Result<PatternAst, Diagnostics> {
    let tokens = lexer::tokenize(source).map_err(|d| {
        let mut diags = Diagnostics::new();
        diags.push(d);
        diags
    })?;
    let mut parser = Parser {
        tokens,
        pos: 0,
        diagnostics: Diagnostics::new(),
    };
    let pattern = parser.parse_pattern();
    match pattern {
        Some(pattern) if !parser.diagnostics.has_errors() => Ok(pattern),
        _ => {
            if parser.diagnostics.is_empty() {
                parser.diagnostics.push(Diagnostic::error(
                    Span::start_of_file(),
                    "pattern file",
                    "failed to parse pattern for an unknown reason",
                ));
            }
            Err(parser.diagnostics)
        }
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    diagnostics: Diagnostics,
}

impl Parser {
    fn peek(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn peek_span(&self) -> Span {
        self.tokens[self.pos].span
    }

    fn is_eof(&self) -> bool {
        matches!(self.peek(), TokenKind::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn error(&mut self, construct: &str, reason: impl Into<String>) {
        self.diagnostics.push(Diagnostic::error(
            self.peek_span(),
            construct,
            reason.into(),
        ));
    }

    /// Consume the current token if it matches `kind` exactly (by
    /// discriminant, ignoring payload), returning it. Otherwise records
    /// an error diagnostic and returns `None`, without consuming.
    fn expect(&mut self, kind: &TokenKind, construct: &str) -> Option<Token> {
        if std::mem::discriminant(self.peek()) == std::mem::discriminant(kind) {
            Some(self.advance())
        } else {
            self.error(
                construct,
                format!(
                    "expected {}, found {}",
                    describe(kind),
                    describe(self.peek())
                ),
            );
            None
        }
    }

    fn expect_ident(&mut self, construct: &str) -> Option<SpannedString> {
        let span = self.peek_span();
        if let TokenKind::Ident(name) = self.peek().clone() {
            self.advance();
            Some(SpannedString { value: name, span })
        } else {
            self.error(
                construct,
                format!("expected an identifier, found {}", describe(self.peek())),
            );
            None
        }
    }

    fn expect_string(&mut self, construct: &str) -> Option<SpannedString> {
        let span = self.peek_span();
        if let TokenKind::StringLit(value) = self.peek().clone() {
            self.advance();
            Some(SpannedString { value, span })
        } else {
            self.error(
                construct,
                format!("expected a string literal, found {}", describe(self.peek())),
            );
            None
        }
    }

    fn expect_int(&mut self, construct: &str) -> Option<SpannedInt> {
        let span = self.peek_span();
        if let TokenKind::IntLit(value) = self.peek().clone() {
            self.advance();
            Some(SpannedInt { value, span })
        } else {
            self.error(
                construct,
                format!(
                    "expected an integer literal, found {}",
                    describe(self.peek())
                ),
            );
            None
        }
    }

    /// Skip tokens until the next token at which resuming parsing of a
    /// list item is plausible: the start of another item, or a closing
    /// brace/EOF. Used for panic-mode recovery so one malformed evidence
    /// declaration or metadata entry doesn't prevent every other
    /// diagnostic in the file from being reported.
    fn recover_to(&mut self, stop: &[TokenKind]) {
        while !self.is_eof() {
            if stop
                .iter()
                .any(|k| std::mem::discriminant(self.peek()) == std::mem::discriminant(k))
            {
                return;
            }
            self.advance();
        }
    }

    fn parse_pattern(&mut self) -> Option<PatternAst> {
        let start = self.peek_span();
        self.expect(&TokenKind::KwPattern, "pattern declaration")?;
        let id = self.expect_ident("pattern id")?;
        self.expect(&TokenKind::KwVersion, "pattern declaration")?;
        let version = self.expect_int("pattern version")?;
        self.expect(&TokenKind::LBrace, "pattern body")?;

        let mut metadata = Vec::new();
        let mut evidence = Vec::new();
        let mut constraint = None;
        let mut sequence = None;
        let mut same_call = None;

        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            match self.peek().clone() {
                TokenKind::KwEvidence => {
                    if let Some(decls) = self.parse_evidence_section() {
                        evidence = decls;
                    }
                }
                TokenKind::KwConstraint => {
                    self.advance();
                    if self
                        .expect(&TokenKind::Colon, "constraint section")
                        .is_some()
                    {
                        if let Some(expr) = self.parse_bool_expr() {
                            constraint = Some(expr);
                        }
                    }
                }
                TokenKind::KwSequence => {
                    if let Some(seq) = self.parse_sequence_section() {
                        sequence = Some(seq);
                    }
                }
                TokenKind::KwSameCall => {
                    if let Some(sc) = self.parse_same_call_section() {
                        same_call = Some(sc);
                    }
                }
                TokenKind::Ident(_) => {
                    if let Some(entry) = self.parse_metadata_entry() {
                        metadata.push(entry);
                    }
                }
                _ => {
                    self.error(
                        "pattern body",
                        format!("unexpected {} inside pattern body", describe(self.peek())),
                    );
                    self.recover_to(&[
                        TokenKind::KwEvidence,
                        TokenKind::KwConstraint,
                        TokenKind::KwSequence,
                        TokenKind::KwSameCall,
                        TokenKind::RBrace,
                    ]);
                }
            }
        }
        let end_tok = self.expect(&TokenKind::RBrace, "pattern body");
        let end_span = end_tok.map_or(self.peek_span(), |t| t.span);

        Some(PatternAst {
            id,
            version,
            metadata,
            evidence,
            constraint,
            sequence,
            same_call,
            span: start.merge(end_span),
        })
    }

    fn parse_metadata_entry(&mut self) -> Option<MetadataEntry> {
        let key = self.expect_ident("metadata entry")?;
        self.expect(&TokenKind::Colon, "metadata entry")?;
        let value = match self.peek().clone() {
            TokenKind::LBracket => {
                self.advance();
                let mut items = Vec::new();
                if !matches!(self.peek(), TokenKind::RBracket) {
                    loop {
                        if let Some(s) = self.expect_string("metadata list item") {
                            items.push(s);
                        } else {
                            self.recover_to(&[TokenKind::Comma, TokenKind::RBracket]);
                        }
                        if matches!(self.peek(), TokenKind::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(&TokenKind::RBracket, "metadata list")?;
                MetadataValue::StrList(items)
            }
            TokenKind::StringLit(_) => MetadataValue::Str(self.expect_string("metadata entry")?),
            TokenKind::Ident(_) => MetadataValue::Ident(self.expect_ident("metadata entry")?),
            _ => {
                self.error(
                    "metadata entry",
                    format!(
                        "expected a value (identifier, string, or list), found {}",
                        describe(self.peek())
                    ),
                );
                self.recover_to(&[
                    TokenKind::KwEvidence,
                    TokenKind::KwConstraint,
                    TokenKind::KwSequence,
                    TokenKind::KwSameCall,
                    TokenKind::RBrace,
                ]);
                return None;
            }
        };
        Some(MetadataEntry {
            span: key.span.merge(match &value {
                MetadataValue::Ident(s) | MetadataValue::Str(s) => s.span,
                MetadataValue::StrList(items) => items.last().map_or(key.span, |s| s.span),
            }),
            key,
            value,
        })
    }

    fn parse_evidence_section(&mut self) -> Option<Vec<EvidenceDecl>> {
        self.expect(&TokenKind::KwEvidence, "evidence section")?;
        self.expect(&TokenKind::LBrace, "evidence section")?;
        let mut decls = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let requiredness = match self.peek() {
                TokenKind::KwRequired => {
                    self.advance();
                    Requiredness::Required
                }
                TokenKind::KwOptional => {
                    self.advance();
                    Requiredness::Optional
                }
                _ => {
                    self.error(
                        "evidence declaration",
                        format!(
                            "expected `required` or `optional`, found {}",
                            describe(self.peek())
                        ),
                    );
                    self.recover_to(&[
                        TokenKind::KwRequired,
                        TokenKind::KwOptional,
                        TokenKind::RBrace,
                    ]);
                    continue;
                }
            };
            let start = self.peek_span();
            let Some(name) = self.expect_ident("evidence declaration name") else {
                self.recover_to(&[
                    TokenKind::KwRequired,
                    TokenKind::KwOptional,
                    TokenKind::RBrace,
                ]);
                continue;
            };
            if self
                .expect(&TokenKind::Colon, "evidence declaration")
                .is_none()
            {
                self.recover_to(&[
                    TokenKind::KwRequired,
                    TokenKind::KwOptional,
                    TokenKind::RBrace,
                ]);
                continue;
            }
            let Some(predicate) = self.parse_predicate() else {
                self.recover_to(&[
                    TokenKind::KwRequired,
                    TokenKind::KwOptional,
                    TokenKind::RBrace,
                ]);
                continue;
            };
            let span = start.merge(predicate.span);
            decls.push(EvidenceDecl {
                requiredness,
                name,
                predicate,
                span,
            });
        }
        self.expect(&TokenKind::RBrace, "evidence section")?;
        Some(decls)
    }

    fn parse_predicate(&mut self) -> Option<Predicate> {
        let kind = self.expect_ident("predicate")?;
        self.expect(&TokenKind::LParen, "predicate arguments")?;
        let mut attributes = Vec::new();
        if !matches!(self.peek(), TokenKind::RParen) {
            loop {
                if let Some(attr) = self.parse_predicate_attr() {
                    attributes.push(attr);
                } else {
                    self.recover_to(&[TokenKind::Comma, TokenKind::RParen]);
                }
                if matches!(self.peek(), TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        let close = self.expect(&TokenKind::RParen, "predicate arguments")?;
        Some(Predicate {
            span: kind.span.merge(close.span),
            kind,
            attributes,
        })
    }

    fn parse_predicate_attr(&mut self) -> Option<PredicateAttr> {
        let name = self.expect_ident("predicate attribute")?;
        self.expect(&TokenKind::Colon, "predicate attribute")?;
        let span_start = name.span;
        let value = match self.peek().clone() {
            TokenKind::StringLit(_) => {
                AttrValue::Str(self.expect_string("predicate attribute value")?)
            }
            TokenKind::IntLit(_) => AttrValue::Int(self.expect_int("predicate attribute value")?),
            TokenKind::BoolLit(value) => {
                let span = self.peek_span();
                self.advance();
                AttrValue::Bool(SpannedBool { value, span })
            }
            TokenKind::Ident(_) => {
                AttrValue::Ident(self.expect_ident("predicate attribute value")?)
            }
            _ => {
                self.error(
                    "predicate attribute value",
                    format!(
                        "expected an identifier, string, integer, or boolean, found {}",
                        describe(self.peek())
                    ),
                );
                return None;
            }
        };
        let value_span = match &value {
            AttrValue::Ident(s) | AttrValue::Str(s) => s.span,
            AttrValue::Int(s) => s.span,
            AttrValue::Bool(s) => s.span,
        };
        Some(PredicateAttr {
            name,
            value,
            span: span_start.merge(value_span),
        })
    }

    fn parse_bool_expr(&mut self) -> Option<BoolExpr> {
        match self.peek().clone() {
            TokenKind::KwAnd | TokenKind::KwOr => {
                let is_and = matches!(self.peek(), TokenKind::KwAnd);
                let start = self.peek_span();
                self.advance();
                self.expect(&TokenKind::LParen, "boolean expression")?;
                let mut items = Vec::new();
                loop {
                    if let Some(expr) = self.parse_bool_expr() {
                        items.push(expr);
                    } else {
                        self.recover_to(&[TokenKind::Comma, TokenKind::RParen]);
                    }
                    if matches!(self.peek(), TokenKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                let close = self.expect(&TokenKind::RParen, "boolean expression")?;
                let span = start.merge(close.span);
                if items.len() < 2 {
                    self.diagnostics.push(
                        Diagnostic::error(
                            span,
                            if is_and {
                                "AND expression"
                            } else {
                                "OR expression"
                            },
                            "requires at least two sub-expressions",
                        )
                        .with_suggestion(
                            "add another operand, or replace with the single operand directly",
                        ),
                    );
                }
                Some(if is_and {
                    BoolExpr::And(items, span)
                } else {
                    BoolExpr::Or(items, span)
                })
            }
            TokenKind::KwNot => {
                let start = self.peek_span();
                self.advance();
                self.expect(&TokenKind::LParen, "NOT expression")?;
                let inner = self.parse_bool_expr();
                let close = self.expect(&TokenKind::RParen, "NOT expression")?;
                let span = start.merge(close.span);
                inner.map(|inner| BoolExpr::Not(Box::new(inner), span))
            }
            TokenKind::Ident(_) => {
                let name = self.expect_ident("evidence reference")?;
                Some(BoolExpr::EvidenceRef(name))
            }
            _ => {
                self.error(
                    "boolean expression",
                    format!(
                        "expected `AND(...)`, `OR(...)`, `NOT(...)`, or an evidence name, found {}",
                        describe(self.peek())
                    ),
                );
                None
            }
        }
    }

    fn parse_sequence_section(&mut self) -> Option<SequenceConstraint> {
        let start = self.peek_span();
        self.expect(&TokenKind::KwSequence, "sequence section")?;
        self.expect(&TokenKind::Colon, "sequence section")?;
        self.expect(&TokenKind::LBracket, "sequence section")?;
        let mut steps = Vec::new();
        if !matches!(self.peek(), TokenKind::RBracket) {
            loop {
                if let Some(name) = self.expect_ident("sequence step") {
                    steps.push(name);
                } else {
                    self.recover_to(&[TokenKind::Comma, TokenKind::RBracket]);
                }
                if matches!(self.peek(), TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        let close = self.expect(&TokenKind::RBracket, "sequence section")?;
        let mut span = start.merge(close.span);

        let within_seconds = if matches!(self.peek(), TokenKind::KwWithin) {
            self.advance();
            self.expect(&TokenKind::Colon, "sequence `within` clause")?;
            let value = self.expect_int("sequence `within` clause")?;
            span = span.merge(value.span);
            Some(value)
        } else {
            None
        };

        Some(SequenceConstraint {
            steps,
            within_seconds,
            span,
        })
    }

    fn parse_same_call_section(&mut self) -> Option<SameCallConstraint> {
        let start = self.peek_span();
        self.expect(&TokenKind::KwSameCall, "same_call section")?;
        self.expect(&TokenKind::Colon, "same_call section")?;
        self.expect(&TokenKind::LBracket, "same_call section")?;
        let mut members = Vec::new();
        if !matches!(self.peek(), TokenKind::RBracket) {
            loop {
                if let Some(name) = self.expect_ident("same_call member") {
                    members.push(name);
                } else {
                    self.recover_to(&[TokenKind::Comma, TokenKind::RBracket]);
                }
                if matches!(self.peek(), TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        let close = self.expect(&TokenKind::RBracket, "same_call section")?;
        let span = start.merge(close.span);

        Some(SameCallConstraint { members, span })
    }
}

fn describe(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Ident(name) => format!("identifier `{name}`"),
        TokenKind::StringLit(s) => format!("string literal \"{s}\""),
        TokenKind::IntLit(n) => format!("integer literal `{n}`"),
        TokenKind::BoolLit(b) => format!("boolean literal `{b}`"),
        TokenKind::KwPattern => "keyword `pattern`".to_string(),
        TokenKind::KwVersion => "keyword `version`".to_string(),
        TokenKind::KwEvidence => "keyword `evidence`".to_string(),
        TokenKind::KwRequired => "keyword `required`".to_string(),
        TokenKind::KwOptional => "keyword `optional`".to_string(),
        TokenKind::KwConstraint => "keyword `constraint`".to_string(),
        TokenKind::KwSequence => "keyword `sequence`".to_string(),
        TokenKind::KwWithin => "keyword `within`".to_string(),
        TokenKind::KwSameCall => "keyword `same_call`".to_string(),
        TokenKind::KwAnd => "keyword `AND`".to_string(),
        TokenKind::KwOr => "keyword `OR`".to_string(),
        TokenKind::KwNot => "keyword `NOT`".to_string(),
        TokenKind::LBrace => "`{`".to_string(),
        TokenKind::RBrace => "`}`".to_string(),
        TokenKind::LBracket => "`[`".to_string(),
        TokenKind::RBracket => "`]`".to_string(),
        TokenKind::LParen => "`(`".to_string(),
        TokenKind::RParen => "`)`".to_string(),
        TokenKind::Colon => "`:`".to_string(),
        TokenKind::Comma => "`,`".to_string(),
        TokenKind::Eof => "end of input".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
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
    fn parses_minimal_pattern() {
        let pattern = parse(MINIMAL).unwrap();
        assert_eq!(pattern.id.value, "donation_attack");
        assert_eq!(pattern.version.value, 1);
        assert_eq!(pattern.metadata.len(), 4);
        assert_eq!(pattern.evidence.len(), 3);
        assert!(pattern.constraint.is_some());
        let seq = pattern.sequence.unwrap();
        assert_eq!(seq.steps.len(), 2);
        assert_eq!(seq.within_seconds.unwrap().value, 60);
        let same_call = pattern.same_call.unwrap();
        assert_eq!(same_call.members.len(), 2);
        assert_eq!(same_call.members[0].value, "donation_transfer");
        assert_eq!(same_call.members[1].value, "price_read");
    }

    #[test]
    fn parses_pattern_without_same_call() {
        let src = r"
            pattern p version 1 {
                family: X
                severity: Low
                evidence { required a: call(kind: External) }
            }
        ";
        let pattern = parse(src).unwrap();
        assert!(pattern.same_call.is_none());
    }

    #[test]
    fn missing_brace_is_reported() {
        let err = parse("pattern foo version 1 { family: X").unwrap_err();
        assert!(err.has_errors());
    }

    #[test]
    fn empty_source_is_reported() {
        let err = parse("").unwrap_err();
        assert!(err.has_errors());
    }

    #[test]
    fn and_with_one_operand_is_reported() {
        let src = r"
            pattern p version 1 {
                evidence { required a: call(kind: External) }
                constraint: AND(a)
            }
        ";
        let err = parse(src).unwrap_err();
        assert!(err.iter().any(|d| d.reason.contains("at least two")));
    }

    #[test]
    fn recovers_multiple_metadata_errors() {
        // Two malformed metadata entries (missing colon) in one file:
        // parsing should report more than one diagnostic, not stop at
        // the first.
        let src = r"
            pattern p version 1 {
                family
                severity
                evidence { required a: call(kind: External) }
            }
        ";
        let err = parse(src).unwrap_err();
        assert!(
            err.len() >= 2,
            "expected recovery to surface multiple diagnostics, got {err:?}"
        );
    }

    #[test]
    fn unknown_predicate_kind_still_parses_structurally() {
        // Structural parsing doesn't know which predicate kinds are
        // valid; that's `validate`'s job. `frobnicate(...)` should parse
        // fine here.
        let src = r"
            pattern p version 1 {
                evidence { required a: frobnicate(x: 1) }
            }
        ";
        let pattern = parse(src).unwrap();
        assert_eq!(pattern.evidence[0].predicate.kind.value, "frobnicate");
    }
}

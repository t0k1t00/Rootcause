//! Lexical parsing: raw source text to a flat token stream.
//!
//! Kept strictly separate from [`crate::parser`] (structural parsing) per
//! the crate's design requirement to separate lexical parsing, structural
//! parsing, and semantic validation into distinct stages. The lexer knows
//! nothing about pattern grammar (sections, evidence, constraints); it
//! only knows characters, and produces a stream of [`Token`]s or a single
//! [`crate::diagnostics::Diagnostic`] describing the first illegal
//! character sequence encountered.
//!
//! Every `usize`-to-`u32` byte-offset cast in this module is bounded by
//! source-file size, not value range — see [`crate::span`]'s module docs
//! for the full justification; the allow is scoped here for the same
//! reason.
#![allow(clippy::cast_possible_truncation)]

use crate::diagnostics::Diagnostic;
use crate::span::Span;

/// One lexical token, with the exact span of source it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// What kind of token this is, and any associated literal value.
    pub kind: TokenKind,
    /// The exact source span this token covers.
    pub span: Span,
}

/// The kind of a lexical token.
///
/// Only structural keywords the grammar itself dispatches on
/// (`pattern`, `evidence`, `required`, `optional`, `constraint`,
/// `sequence`, `within`, `AND`, `OR`, `NOT`) are reserved words; every
/// other name (metadata keys, predicate kinds, attribute names, evidence
/// identifiers) is a plain [`TokenKind::Ident`] that the parser
/// interprets contextually. This keeps the lexer small and keeps the set
/// of reserved words — the only names a pattern author can never use as
/// an identifier — explicit and minimal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// An identifier: `[A-Za-z_][A-Za-z0-9_]*`.
    Ident(String),
    /// A double-quoted string literal, with escapes already resolved.
    StringLit(String),
    /// An integer literal.
    IntLit(i64),
    /// A boolean literal (`true` / `false`).
    BoolLit(bool),

    /// `pattern`
    KwPattern,
    /// `version`
    KwVersion,
    /// `evidence`
    KwEvidence,
    /// `required`
    KwRequired,
    /// `optional`
    KwOptional,
    /// `constraint`
    KwConstraint,
    /// `sequence`
    KwSequence,
    /// `within`
    KwWithin,
    /// `same_call`
    KwSameCall,
    /// `AND`
    KwAnd,
    /// `OR`
    KwOr,
    /// `NOT`
    KwNot,

    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `:`
    Colon,
    /// `,`
    Comma,
    /// end of input
    Eof,
}

/// Tokenize `source`, returning the full token stream (always terminated
/// by a single [`TokenKind::Eof`]) or the first lexical error
/// encountered.
///
/// # Errors
/// Returns a [`Diagnostic`] describing the first illegal character or
/// unterminated literal found. Lexing stops at the first error rather
/// than attempting error recovery — later stages (structural parsing)
/// cannot proceed meaningfully over a corrupted token stream, so nothing
/// is gained by continuing to scan.
pub fn tokenize(source: &str) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new(source).run()
}

struct Lexer<'src> {
    source: &'src str,
    pos: usize,
    tokens: Vec<Token>,
}

impl<'src> Lexer<'src> {
    const fn new(source: &'src str) -> Self {
        Self {
            source,
            pos: 0,
            tokens: Vec::new(),
        }
    }

    fn run(mut self) -> Result<Vec<Token>, Diagnostic> {
        loop {
            self.skip_whitespace_and_comments();
            let start = self.pos;
            let Some(ch) = self.peek() else {
                self.tokens.push(Token {
                    kind: TokenKind::Eof,
                    span: Span::new(start as u32, start as u32),
                });
                break;
            };

            let kind = match ch {
                '{' => self.single(TokenKind::LBrace),
                '}' => self.single(TokenKind::RBrace),
                '[' => self.single(TokenKind::LBracket),
                ']' => self.single(TokenKind::RBracket),
                '(' => self.single(TokenKind::LParen),
                ')' => self.single(TokenKind::RParen),
                ':' => self.single(TokenKind::Colon),
                ',' => self.single(TokenKind::Comma),
                '"' => self.string_literal()?,
                c if c == '-' || c.is_ascii_digit() => self.number_literal()?,
                c if c.is_alphabetic() || c == '_' => self.ident_or_keyword(),
                other => {
                    return Err(Diagnostic::error(
                        Span::new(start as u32, (start + other.len_utf8()) as u32),
                        format!("character `{other}`"),
                        "unexpected character not part of any valid token",
                    )
                    .with_suggestion(
                        "remove this character, or check for a typo in a keyword or symbol",
                    ));
                }
            };
            let end = self.pos;
            self.tokens.push(Token {
                kind,
                span: Span::new(start as u32, end as u32),
            });
        }
        Ok(self.tokens)
    }

    fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.source[self.pos..].chars().nth(offset)
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn single(&mut self, kind: TokenKind) -> TokenKind {
        self.advance();
        kind
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.advance();
                }
                Some('/') if self.peek_at(1) == Some('/') => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.advance();
                    }
                }
                _ => break,
            }
        }
    }

    fn ident_or_keyword(&mut self) -> TokenKind {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }
        let text = &self.source[start..self.pos];
        match text {
            "pattern" => TokenKind::KwPattern,
            "version" => TokenKind::KwVersion,
            "evidence" => TokenKind::KwEvidence,
            "required" => TokenKind::KwRequired,
            "optional" => TokenKind::KwOptional,
            "constraint" => TokenKind::KwConstraint,
            "sequence" => TokenKind::KwSequence,
            "within" => TokenKind::KwWithin,
            "same_call" => TokenKind::KwSameCall,
            "AND" => TokenKind::KwAnd,
            "OR" => TokenKind::KwOr,
            "NOT" => TokenKind::KwNot,
            "true" => TokenKind::BoolLit(true),
            "false" => TokenKind::BoolLit(false),
            other => TokenKind::Ident(other.to_string()),
        }
    }

    fn number_literal(&mut self) -> Result<TokenKind, Diagnostic> {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.advance();
        }
        let digits_start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }
        if self.pos == digits_start {
            return Err(Diagnostic::error(
                Span::new(start as u32, self.pos as u32),
                "number literal",
                "a `-` must be followed by at least one digit",
            ));
        }
        let text = &self.source[start..self.pos];
        text.parse::<i64>().map(TokenKind::IntLit).map_err(|_| {
            Diagnostic::error(
                Span::new(start as u32, self.pos as u32),
                format!("number literal `{text}`"),
                "integer literal out of range or malformed",
            )
        })
    }

    fn string_literal(&mut self) -> Result<TokenKind, Diagnostic> {
        let start = self.pos;
        self.advance(); // opening quote
        let mut value = String::new();
        loop {
            match self.advance() {
                None | Some('\n') => {
                    return Err(Diagnostic::error(
                        Span::new(start as u32, self.pos as u32),
                        "string literal",
                        "unterminated string literal (missing closing `\"`)",
                    )
                    .with_suggestion("add a closing `\"` before the end of the line"));
                }
                Some('"') => break,
                Some('\\') => match self.advance() {
                    Some('"') => value.push('"'),
                    Some('\\') => value.push('\\'),
                    Some('n') => value.push('\n'),
                    Some('t') => value.push('\t'),
                    Some(other) => {
                        return Err(Diagnostic::error(
                            Span::new(start as u32, self.pos as u32),
                            format!("escape sequence `\\{other}`"),
                            "unknown escape sequence in string literal",
                        )
                        .with_suggestion("valid escapes are \\\", \\\\, \\n, and \\t"));
                    }
                    None => {
                        return Err(Diagnostic::error(
                            Span::new(start as u32, self.pos as u32),
                            "string literal",
                            "unterminated escape sequence at end of input",
                        ));
                    }
                },
                Some(c) => value.push(c),
            }
        }
        Ok(TokenKind::StringLit(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        tokenize(source)
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn tokenizes_keywords() {
        assert_eq!(
            kinds(
                "pattern evidence required optional constraint sequence within same_call AND OR NOT"
            ),
            vec![
                TokenKind::KwPattern,
                TokenKind::KwEvidence,
                TokenKind::KwRequired,
                TokenKind::KwOptional,
                TokenKind::KwConstraint,
                TokenKind::KwSequence,
                TokenKind::KwWithin,
                TokenKind::KwSameCall,
                TokenKind::KwAnd,
                TokenKind::KwOr,
                TokenKind::KwNot,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn tokenizes_idents_and_symbols() {
        assert_eq!(
            kinds("foo_bar: 42, [\"x\"]"),
            vec![
                TokenKind::Ident("foo_bar".to_string()),
                TokenKind::Colon,
                TokenKind::IntLit(42),
                TokenKind::Comma,
                TokenKind::LBracket,
                TokenKind::StringLit("x".to_string()),
                TokenKind::RBracket,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn tokenizes_negative_integers() {
        assert_eq!(kinds("-17"), vec![TokenKind::IntLit(-17), TokenKind::Eof]);
    }

    #[test]
    fn tokenizes_bool_literals() {
        assert_eq!(
            kinds("true false"),
            vec![
                TokenKind::BoolLit(true),
                TokenKind::BoolLit(false),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn skips_line_comments() {
        assert_eq!(
            kinds("foo // this is a comment\nbar"),
            vec![
                TokenKind::Ident("foo".to_string()),
                TokenKind::Ident("bar".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn string_literal_resolves_escapes() {
        assert_eq!(
            kinds(r#""a\"b\\c\nd""#),
            vec![
                TokenKind::StringLit("a\"b\\c\nd".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn unterminated_string_is_an_error() {
        let err = tokenize("\"abc").unwrap_err();
        assert!(err.reason.contains("unterminated"));
    }

    #[test]
    fn unknown_character_is_an_error() {
        let err = tokenize("@").unwrap_err();
        assert!(err.reason.contains("unexpected character"));
    }

    #[test]
    fn bare_minus_is_an_error() {
        let err = tokenize("- ").unwrap_err();
        assert!(err.reason.contains("digit"));
    }

    #[test]
    fn empty_source_yields_only_eof() {
        assert_eq!(kinds(""), vec![TokenKind::Eof]);
        assert_eq!(kinds("   \n\t "), vec![TokenKind::Eof]);
    }
}

//! Surface AST: the direct, unvalidated structural representation of a
//! parsed pattern definition.
//!
//! Everything in this module is produced purely by structural parsing
//! (matching the grammar's shape) with **no semantic checking** — a
//! `PatternAst` may reference an evidence identifier that doesn't exist,
//! declare the same evidence name twice, or use an unknown predicate
//! kind. [`crate::validate`] is the stage that catches all of that. This
//! separation is deliberate: it lets the parser's grammar rules stay
//! purely syntactic, and lets validation produce complete diagnostics
//! (multiple errors per pattern) instead of stopping at the first
//! semantic problem the parser happens to trip over.

use crate::span::Span;

/// A fully parsed, not-yet-validated pattern definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternAst {
    /// The pattern's identifier, e.g. `donation_attack`.
    pub id: SpannedString,
    /// The pattern's version number, e.g. `1` in `version 1`.
    pub version: SpannedInt,
    /// Metadata key/value entries (`family`, `severity`, `tags`,
    /// `references`, and any future keys), in source order.
    pub metadata: Vec<MetadataEntry>,
    /// Evidence declarations, in source order.
    pub evidence: Vec<EvidenceDecl>,
    /// The pattern's boolean constraint expression, if present. A
    /// pattern with no `constraint:` section implicitly requires every
    /// `required` evidence clause to hold — see
    /// [`crate::validate::validate`].
    pub constraint: Option<BoolExpr>,
    /// The pattern's temporal/sequence constraint, if present.
    pub sequence: Option<SequenceConstraint>,
    /// The pattern's cross-evidence call-identity correlation, if
    /// present: a requirement that every listed evidence clause's
    /// binding was produced by the identical `CallId`.
    pub same_call: Option<SameCallConstraint>,
    /// The full span of the pattern definition, for top-level
    /// diagnostics.
    pub span: Span,
}

/// A string value together with the source span it came from, so
/// validation errors about it (e.g. "duplicate pattern id") can point at
/// the exact source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpannedString {
    /// The string's value.
    pub value: String,
    /// Where it appears in the source.
    pub span: Span,
}

/// An integer value together with the source span it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpannedInt {
    /// The integer's value.
    pub value: i64,
    /// Where it appears in the source.
    pub span: Span,
}

/// One `key: value` entry inside a pattern's metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataEntry {
    /// The metadata key, e.g. `family`, `severity`, `tags`, `references`.
    pub key: SpannedString,
    /// The metadata value.
    pub value: MetadataValue,
    /// The full span of the `key: value` entry.
    pub span: Span,
}

/// The value side of a metadata entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataValue {
    /// A bare identifier value, e.g. `family: OracleManipulation`.
    Ident(SpannedString),
    /// A string literal value.
    Str(SpannedString),
    /// A list of string literals, e.g. `tags: ["oracle", "donation"]`.
    StrList(Vec<SpannedString>),
}

/// One `required`/`optional` evidence declaration:
/// `required <name>: <predicate>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceDecl {
    /// Whether this clause is load-bearing (`required`) or merely
    /// contextual (`optional`).
    ///
    /// This is the DSL's real, author-visible required/optional
    /// distinction the Architecture document calls for (Phase 6:
    /// "optional clauses silently treated as load-bearing" is the
    /// grounding verifier's top threat to defend against) — the DSL's
    /// job is only to make that distinction expressible and enforce it
    /// structurally, not to perform grounding itself.
    pub requiredness: Requiredness,
    /// The evidence clause's name, used to reference it from `sequence`
    /// and `constraint` sections.
    pub name: SpannedString,
    /// The predicate this evidence clause must satisfy.
    pub predicate: Predicate,
    /// The full span of the declaration.
    pub span: Span,
}

/// Whether an evidence clause is required or optional.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requiredness {
    /// Declared with `required`.
    Required,
    /// Declared with `optional`.
    Optional,
}

/// A predicate: `<kind>(<attr>: <value>, ...)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Predicate {
    /// The predicate kind, e.g. `call`, `storage`, `value_flow`,
    /// `token_transfer`, `transaction`.
    pub kind: SpannedString,
    /// The predicate's attribute clauses, in source order.
    pub attributes: Vec<PredicateAttr>,
    /// The full span of the predicate expression.
    pub span: Span,
}

/// One `attr: value` clause inside a predicate's parentheses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PredicateAttr {
    /// The attribute's name.
    pub name: SpannedString,
    /// The attribute's value.
    pub value: AttrValue,
    /// The full span of the `name: value` clause.
    pub span: Span,
}

/// The value side of a predicate attribute clause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttrValue {
    /// A bare identifier, e.g. `kind: External`.
    Ident(SpannedString),
    /// A string literal.
    Str(SpannedString),
    /// An integer literal.
    Int(SpannedInt),
    /// A boolean literal.
    Bool(SpannedBool),
}

/// A boolean value together with the source span it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpannedBool {
    /// The boolean's value.
    pub value: bool,
    /// Where it appears in the source.
    pub span: Span,
}

/// A boolean composition tree over evidence names: `AND`, `OR`, `NOT`, or
/// a leaf reference to an evidence clause by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoolExpr {
    /// A leaf: a reference to an evidence clause's name.
    EvidenceRef(SpannedString),
    /// `AND(a, b, ...)` — all sub-expressions must hold. Stored as an
    /// n-ary list (rather than a binary `And(Box, Box)`) because the
    /// grammar itself is n-ary (`AND(a, b, c)`), and forcing an n-ary
    /// parse into a binary tree would be a lossy, purely cosmetic
    /// transformation with no benefit to any later stage.
    And(Vec<BoolExpr>, Span),
    /// `OR(a, b, ...)` — at least one sub-expression must hold.
    Or(Vec<BoolExpr>, Span),
    /// `NOT(a)` — the sub-expression must not hold.
    Not(Box<BoolExpr>, Span),
}

impl BoolExpr {
    /// The span covering this entire sub-expression.
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::EvidenceRef(s) => s.span,
            Self::And(_, span) | Self::Or(_, span) | Self::Not(_, span) => *span,
        }
    }
}

/// A temporal/sequence constraint: an ordered list of evidence names that
/// must occur in that relative order, optionally within a time window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceConstraint {
    /// The evidence names, in the order they must occur.
    pub steps: Vec<SpannedString>,
    /// An optional maximum number of seconds (block-timestamp delta)
    /// separating the first and last step, from a trailing
    /// `within: <seconds>` clause.
    pub within_seconds: Option<SpannedInt>,
    /// The full span of the `sequence: [...]` section.
    pub span: Span,
}

/// A cross-evidence identity correlation: an unordered group of evidence
/// names whose bindings must all be produced by the identical `CallId`.
///
/// This is deliberately narrower than [`SequenceConstraint`]: it asserts
/// nothing about relative ordering, only that every named clause's
/// binding resolves to the same underlying call. See the `matcher`
/// crate's `same_call` support for how "the call that produced a fact"
/// is derived for each fact kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SameCallConstraint {
    /// The evidence names that must share a `CallId`, in source order.
    pub members: Vec<SpannedString>,
    /// The full span of the `same_call: [...]` section.
    pub span: Span,
}

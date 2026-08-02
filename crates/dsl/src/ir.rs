//! The compiled, immutable internal representation of a pattern.
//!
//! Everything in [`crate::ast`] is throwaway: it exists only to get from
//! source text to a validated pattern and is discarded once compilation
//! succeeds. Everything in this module is what survives — the form a
//! future `matcher` crate is meant to consume. Two structural properties
//! matter for that consumer:
//!
//! - **Evidence clauses are index-addressed, not name-addressed.** Every
//!   [`EvidenceRef`] a compiled constraint or sequence uses is a `usize`
//!   index into [`CompiledPattern::evidence`], resolved once at compile
//!   time (see [`crate::compile`]) rather than a `String` name looked up
//!   repeatedly at match time. This mirrors the same
//!   arena-of-facts-plus-small-`Copy`-IDs shape `fact-model` itself
//!   uses for exactly the same reason: cheap, `Copy`, allocation-free
//!   references instead of repeated string comparison in what will be a
//!   hot path once matching is implemented.
//! - **Nothing here is mutable after construction.** A `CompiledPattern`
//!   is built once, wholesale, by [`crate::compile::compile`] and never
//!   modified in place — the same "immutable after construction"
//!   invariant `fact-model` states for its own types, for the same
//!   reason: a pattern library loaded once at startup and matched
//!   against many traces must not be susceptible to accidental in-place
//!   mutation from one match run bleeding into the next.

use serde::{Deserialize, Serialize};

/// A pattern's identifier, e.g. `"donation_attack"`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PatternId(pub String);

impl std::fmt::Display for PatternId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A pattern's version number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PatternVersion(pub u32);

impl std::fmt::Display for PatternVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "v{}", self.0)
    }
}

/// A pattern's severity, mapped from the DSL's closed
/// `severity:` enum (see [`crate::schema::SEVERITY_VALUES`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Severity {
    /// Cosmetic or informational only.
    Low,
    /// Meaningful but limited-impact.
    Medium,
    /// Significant, likely-exploitable impact.
    High,
    /// Severe, high-confidence exploit impact.
    Critical,
}

/// A named group of related patterns, e.g. `"OracleManipulation"`. Kept
/// as a free-form identifier (not a closed enum, unlike [`Severity`])
/// because the Architecture document's exploit-family list is explicitly
/// non-exhaustive and expected to grow as new pattern families are
/// authored; closing this enum here would make adding a family a
/// breaking change to this crate for no benefit.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PatternFamily(pub String);

/// An index into [`CompiledPattern::evidence`], resolved once at compile
/// time from an [`crate::ast::EvidenceDecl`] name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EvidenceRef(pub u32);

impl EvidenceRef {
    /// This reference's raw index into [`CompiledPattern::evidence`].
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Whether an evidence clause is load-bearing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Requiredness {
    /// A required clause: every valid match must ground it, or the
    /// match is discarded (Architecture: "No evidence → No
    /// classification").
    Required,
    /// An optional clause: contextual, never load-bearing on its own.
    Optional,
}

/// The value shape of a compiled predicate attribute. Structurally the
/// same four variants as [`crate::ast::AttrValue`], but decoupled from
/// it deliberately: the AST type carries a [`crate::span::Span`] for
/// diagnostics, which a compiled pattern has no use for and should not
/// pay to store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttrValue {
    /// An identifier value, e.g. a `CallKind` variant name.
    Ident(String),
    /// A string value.
    Str(String),
    /// An integer value.
    Int(i64),
    /// A boolean value.
    Bool(bool),
}

/// One resolved `name: value` predicate attribute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledAttr {
    /// The attribute's name.
    pub name: String,
    /// The attribute's value.
    pub value: AttrValue,
}

/// A compiled predicate: a kind name (validated against
/// [`crate::schema`]) plus its resolved attributes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledPredicate {
    /// The predicate kind, e.g. `"call"`.
    pub kind: String,
    /// The predicate's attributes, in source order.
    pub attributes: Vec<CompiledAttr>,
}

impl CompiledPredicate {
    /// Look up an attribute's value by name.
    #[must_use]
    pub fn attr(&self, name: &str) -> Option<&AttrValue> {
        self.attributes
            .iter()
            .find(|a| a.name == name)
            .map(|a| &a.value)
    }
}

/// One compiled evidence clause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledEvidence {
    /// The clause's name, as declared in the source pattern.
    pub name: String,
    /// Required vs. optional.
    pub requiredness: Requiredness,
    /// The predicate this clause must ground.
    pub predicate: CompiledPredicate,
}

/// A compiled boolean constraint tree over [`EvidenceRef`]s.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledConstraint {
    /// A leaf: a specific evidence clause must ground.
    Evidence(EvidenceRef),
    /// All sub-constraints must hold.
    And(Vec<CompiledConstraint>),
    /// At least one sub-constraint must hold.
    Or(Vec<CompiledConstraint>),
    /// The sub-constraint must not hold.
    Not(Box<CompiledConstraint>),
}

/// A compiled temporal/sequence constraint: an ordered list of evidence
/// clauses that must ground in that relative order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledSequence {
    /// The evidence clauses, in required order.
    pub steps: Vec<EvidenceRef>,
    /// An optional maximum time window (seconds) between the first and
    /// last step.
    pub within_seconds: Option<u32>,
}

/// A compiled cross-evidence identity correlation: every listed evidence
/// clause's binding must be produced by the identical `CallId`.
///
/// Deliberately narrower than [`CompiledSequence`]: it carries no
/// ordering information, only an unordered group of clauses that must
/// resolve to the same underlying call. See the `matcher` crate for how
/// "the call that produced a fact" is derived per fact kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledSameCall {
    /// The evidence clauses that must share a `CallId`, in source
    /// order. Always has at least two members (see
    /// [`crate::validate`]).
    pub members: Vec<EvidenceRef>,
}

/// A fully compiled, immutable pattern, ready to be handed to a future
/// matcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledPattern {
    /// The pattern's identifier.
    pub id: PatternId,
    /// The pattern's version.
    pub version: PatternVersion,
    /// The exploit family this pattern belongs to.
    pub family: PatternFamily,
    /// The pattern's severity.
    pub severity: Severity,
    /// Free-form tags, in source order, deduplicated.
    pub tags: Vec<String>,
    /// External references (e.g. `"SCWE-042"`), in source order.
    pub references: Vec<String>,
    /// Every evidence clause this pattern declares, in source order.
    /// [`EvidenceRef`]s elsewhere in this struct index into this `Vec`.
    pub evidence: Vec<CompiledEvidence>,
    /// The pattern's boolean constraint tree. Always present after
    /// compilation: a source pattern with no explicit `constraint:`
    /// section compiles to the implicit `AND` of every required
    /// evidence clause (see [`crate::compile`]).
    pub constraint: CompiledConstraint,
    /// The pattern's temporal/sequence constraint, if declared.
    pub sequence: Option<CompiledSequence>,
    /// The pattern's cross-evidence call-identity correlation, if
    /// declared.
    pub same_call: Option<CompiledSameCall>,
}

impl CompiledPattern {
    /// Resolve an [`EvidenceRef`] to its [`CompiledEvidence`].
    ///
    /// This never panics on a `CompiledPattern` produced by
    /// [`crate::compile::compile`]: every `EvidenceRef` a compiled
    /// pattern contains was resolved against `self.evidence` at compile
    /// time and is guaranteed in-bounds. A `CompiledPattern` assembled
    /// by hand (e.g. via `serde` deserialization of a hand-edited file)
    /// carries no such guarantee, which is why this returns `Option`
    /// rather than indexing directly.
    #[must_use]
    pub fn resolve(&self, r: EvidenceRef) -> Option<&CompiledEvidence> {
        self.evidence.get(r.index())
    }
}

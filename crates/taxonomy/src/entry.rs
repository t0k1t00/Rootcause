//! [`TaxonomyEntry`]: one external-taxonomy classification produced by
//! a single [`crate::mapper::TaxonomyMapper`].

/// One entry in an external vulnerability taxonomy (e.g. one SCWE
/// weakness, one SWC registry entry) that a [`crate::mapper::TaxonomyMapper`]
/// judges applicable to a given [`dsl::ir::PatternFamily`].
///
/// A single pattern family routinely corresponds to more than one entry
/// in the same taxonomy (e.g. a reentrancy family might plausibly touch
/// both a general reentrancy weakness and a more specific
/// state-update-ordering weakness) — see this crate's top-level docs,
/// "one-to-many mappings," for why [`crate::mapper::TaxonomyMapper::map_family`]
/// returns a `Vec<TaxonomyEntry>` rather than at most one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaxonomyEntry {
    /// Which taxonomy this entry belongs to (e.g. `"SCWE"`, `"SWC"`),
    /// matching the owning [`crate::mapper::TaxonomyMapper::taxonomy`].
    /// Kept on the entry itself (not just implied by which mapper
    /// produced it) so a [`TaxonomyEntry`] is self-describing once
    /// collected into a flat list.
    pub taxonomy: &'static str,
    /// The taxonomy's own identifier for this entry, e.g. `"SCWE-046"`
    /// or `"SWC-107"`.
    pub code: String,
    /// A short, human-readable title for the entry, e.g.
    /// `"Reentrancy Attacks"` — copied in so a report is readable
    /// without cross-referencing the external taxonomy's own document.
    pub title: String,
    /// A stable link to the taxonomy's own canonical entry, if this
    /// mapper knows one. `None` rather than an empty string when
    /// unknown, so a caller does not need a separate "is this URL
    /// meaningful" check.
    pub reference_url: Option<&'static str>,
}

impl TaxonomyEntry {
    /// Construct an entry. Deliberately takes `code`/`title` as
    /// `impl Into<String>` rather than requiring callers (i.e. every
    /// built-in mapper's static table) to write `.to_string()` at every
    /// call site.
    #[must_use]
    pub fn new(
        taxonomy: &'static str,
        code: impl Into<String>,
        title: impl Into<String>,
        reference_url: Option<&'static str>,
    ) -> Self {
        Self {
            taxonomy,
            code: code.into(),
            title: title.into(),
            reference_url,
        }
    }
}

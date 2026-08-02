//! [`TaxonomyReport`]: the complete result of mapping one
//! [`grounding::GroundingResult`] across every registered taxonomy.

use dsl::ir::{PatternFamily, PatternId, PatternVersion};

use grounding::{CandidateId, GroundingStatus};

use crate::entry::TaxonomyEntry;

/// Every entry one [`crate::mapper::TaxonomyMapper`] produced for a
/// single [`grounding::GroundingResult`]'s pattern family.
///
/// Only ever constructed with a non-empty [`Self::entries`] — a
/// taxonomy that produced no entries is recorded in
/// [`TaxonomyReport::unmapped_taxonomies`] instead, not as a
/// `TaxonomyMapping` with an empty `entries` list, so a caller can
/// distinguish "this taxonomy applies, here's what it says" from
/// "this taxonomy has nothing to say about this family" with one
/// field check rather than inspecting `entries.is_empty()` on every
/// mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaxonomyMapping {
    /// Which taxonomy these entries belong to (matches every
    /// [`TaxonomyEntry::taxonomy`] in [`Self::entries`]).
    pub taxonomy: &'static str,
    /// Every entry this taxonomy's mapper found applicable — plural,
    /// preserving one-to-many mappings rather than collapsing to a
    /// single "best" entry (see this crate's top-level docs).
    pub entries: Vec<TaxonomyEntry>,
}

/// The complete result of mapping one [`grounding::GroundingResult`]
/// across every taxonomy in a [`crate::registry::TaxonomyRegistry`].
///
/// Produced for **every** [`grounding::GroundingResult`] passed to
/// [`crate::engine::MappingEngine::map`], regardless of whether any
/// taxonomy actually had something to say about it — an all-unmapped
/// result is still a [`TaxonomyReport`] (with an empty
/// [`Self::mappings`] and every registered taxonomy listed in
/// [`Self::unmapped_taxonomies`]), never dropped or turned into an
/// error, per this crate's "preserve unmapped results" requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaxonomyReport {
    /// The source [`grounding::GroundingResult`]'s pattern identity,
    /// copied for convenience.
    pub pattern_id: PatternId,
    /// The source result's pattern version, copied for convenience.
    pub pattern_version: PatternVersion,
    /// The source result's pattern family — what every mapper's
    /// [`crate::mapper::TaxonomyMapper::map_family`] call was keyed on.
    pub pattern_family: PatternFamily,
    /// The source result's candidate fingerprint, copied so a
    /// [`TaxonomyReport`] can be correlated back to the exact
    /// [`grounding::GroundingResult`] it was derived from without
    /// keeping a reference to it.
    pub candidate_id: CandidateId,
    /// The source result's own [`GroundingStatus`], copied forward
    /// unchanged. This crate never derives, adjusts, or re-scores
    /// confidence of its own — see this crate's top-level docs,
    /// "confidence-preserving mapping": a [`TaxonomyReport`] reports
    /// *what a pattern family is called externally*, not *how
    /// confident grounding was*, and keeps those two facts visibly
    /// separate rather than blending them into one number.
    pub grounding_status: GroundingStatus,
    /// Every taxonomy that had at least one applicable entry, in
    /// [`crate::registry::TaxonomyRegistry::taxonomies`]'s
    /// deterministic order.
    pub mappings: Vec<TaxonomyMapping>,
    /// Every taxonomy that was queried but had **no** applicable entry
    /// for this result's pattern family — the explicit record of what
    /// this report does *not* claim to classify, preserved rather than
    /// silently omitted (see [`Self`]'s own docs).
    pub unmapped_taxonomies: Vec<&'static str>,
}

impl TaxonomyReport {
    /// Whether every registered taxonomy failed to map this result —
    /// i.e. [`Self::mappings`] is empty. A convenience predicate over
    /// the same information [`Self::mappings`] and
    /// [`Self::unmapped_taxonomies`] already carry.
    #[must_use]
    pub fn is_fully_unmapped(&self) -> bool {
        self.mappings.is_empty()
    }
}

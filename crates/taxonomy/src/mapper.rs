//! [`TaxonomyMapper`]: one implementation per external taxonomy (SCWE,
//! SWC, and — per this crate's stated extensibility goal — any future
//! taxonomy such as CWE, DASP, or `EthTrust`) dispatched through a
//! [`crate::registry::TaxonomyRegistry`], the same "trait +
//! kind-keyed-registry" shape `grounding::verifier` already uses for
//! its own extensible verifier dispatch (see that module's docs for the
//! precedent this crate deliberately follows).
//!
//! # Why this is keyed on [`PatternFamily`], not [`grounding::GroundingResult`]
//!
//! A taxonomy answers "what kind of weakness is this," which is a
//! property of *which pattern matched* (its exploit family), not of
//! *how well the evidence for this specific candidate held up* (the
//! grounding status). Keeping [`TaxonomyMapper::map_family`]'s
//! signature narrow to [`PatternFamily`] alone — rather than the full
//! [`grounding::GroundingResult`] — is what makes "taxonomy mapping
//! never influences matching or grounding" structurally true rather
//! than a convention a mapper implementation could accidentally break:
//! a mapper physically cannot read, branch on, or feed back into a
//! [`grounding::GroundingStatus`], an [`grounding::EvidenceOutcome`],
//! or any other grounding-internal detail, because none of that is in
//! scope of the function it implements. [`crate::engine::MappingEngine`]
//! is the one place that reads a full [`grounding::GroundingResult`],
//! and it does so only to *copy* the status/confidence forward into the
//! report (see that module's docs) — never to pass it to a mapper.

use dsl::ir::PatternFamily;

use crate::entry::TaxonomyEntry;

/// A mapper from [`PatternFamily`] to zero or more entries in one
/// external taxonomy.
///
/// Implementations are looked up by [`Self::taxonomy`] through a
/// [`crate::registry::TaxonomyRegistry`]. Adding support for a new
/// taxonomy (CWE, DASP, `EthTrust`, ...) means implementing this trait
/// and registering it via [`crate::registry::TaxonomyRegistry::register`]
/// — no change to [`crate::engine::MappingEngine`], [`crate::report`],
/// or any existing mapper, mirroring the extensibility
/// `grounding::verifier::Verifier` already provides for predicate
/// kinds.
pub trait TaxonomyMapper: Send + Sync {
    /// The taxonomy this mapper covers, e.g. `"SCWE"` or `"SWC"` —
    /// matches every [`TaxonomyEntry::taxonomy`] this mapper produces,
    /// and is this mapper's registration key in a
    /// [`crate::registry::TaxonomyRegistry`].
    fn taxonomy(&self) -> &'static str;

    /// Every entry in this mapper's taxonomy that applies to `family`,
    /// in a fixed, deterministic order (built-in mappers return their
    /// static table's entries in declaration order). Returns an empty
    /// `Vec` — never a placeholder or a guess — when this taxonomy has
    /// no entry covering `family` at all: an empty result is a genuine,
    /// preserved "unmapped in this taxonomy" outcome (see
    /// [`crate::report::TaxonomyReport`]'s own docs), not an error and
    /// not something a caller should treat as this mapper being
    /// incomplete.
    fn map_family(&self, family: &PatternFamily) -> Vec<TaxonomyEntry>;
}

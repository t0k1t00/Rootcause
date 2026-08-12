//! # taxonomy
//!
//! Maps a [`grounding::GroundingResult`] into external vulnerability
//! taxonomies — SCWE (primary) and SWC (secondary/compatibility) to
//! start, per the Architecture document's Assumption A-3 — without
//! ever feeding back into matching or grounding.
//!
//! ## Design
//!
//! - **Multiple taxonomies, independently.** Each taxonomy is a
//!   separate [`TaxonomyMapper`] implementation ([`scwe::ScweMapper`],
//!   [`swc::SwcMapper`]), looked up by name through a
//!   [`TaxonomyRegistry`] — the same "trait + kind-keyed registry"
//!   shape `grounding::verifier` already uses, so one taxonomy's
//!   mapping table can be added, changed, or removed without touching
//!   any other's.
//! - **One-to-many mappings, preserved.** [`TaxonomyMapper::map_family`]
//!   returns `Vec<TaxonomyEntry>`; a family that plausibly corresponds
//!   to several entries in the same taxonomy keeps all of them, not
//!   just a "best" one.
//! - **Unmapped results, preserved.** A pattern family with no
//!   applicable entry in some (or every) taxonomy still produces a
//!   full [`TaxonomyReport`] — the taxonomy is listed in
//!   [`TaxonomyReport::unmapped_taxonomies`] rather than the whole
//!   result being dropped or defaulted to a guess. See
//!   [`swc::SwcMapper`]'s own docs for a genuine, real-world example
//!   (oracle manipulation has no SWC entry).
//! - **Confidence-preserving.** This crate invents no probability or
//!   score of its own. [`MappingEngine::map`] copies the source
//!   [`grounding::GroundingResult`]'s own [`grounding::GroundingStatus`]
//!   into [`TaxonomyReport::grounding_status`] unchanged — taxonomy
//!   mapping answers "what is this called externally," grounding
//!   status answers "how sure are we," and the two are kept visibly
//!   separate rather than blended.
//! - **Never influences matching or grounding.** This crate depends on
//!   `dsl` (for [`dsl::ir::PatternFamily`]) and `grounding` (for
//!   [`grounding::GroundingResult`]); neither `matcher` nor `grounding`
//!   depends on `taxonomy` — see [`engine`]'s own docs for why this
//!   makes the "never influences" requirement a fact about the
//!   dependency graph, not just a convention.
//! - **Extensible.** A future taxonomy (CWE, DASP, `EthTrust`, ...) is
//!   added by implementing [`TaxonomyMapper`] and calling
//!   [`TaxonomyRegistry::register`] — no change to [`MappingEngine`],
//!   [`report`], or any existing mapper.
//!
//! ## Module map
//!
//! - [`entry`] — [`TaxonomyEntry`], one taxonomy classification.
//! - [`mapper`] — the [`TaxonomyMapper`] trait, this crate's extension
//!   point.
//! - [`registry`] — [`TaxonomyRegistry`], the extensible dispatch table.
//! - [`scwe`], [`swc`] — the two built-in mappers.
//! - [`report`] — [`TaxonomyReport`] and [`report::TaxonomyMapping`].
//! - [`engine`] — [`MappingEngine`], orchestrating every phase above.
//!
//! ## Out of scope (by this task's explicit instruction)
//!
//! No benchmark harness, no CLI, no UI — this crate is a pure mapping
//! library; `benchmark-harness` and `cli` are dedicated, later tasks
//! that will consume it.

#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::missing_const_for_fn
    )
)]

pub mod engine;
pub mod entry;
pub mod mapper;
pub mod registry;
pub mod report;
pub mod scwe;
pub mod swc;

pub use engine::MappingEngine;
pub use entry::TaxonomyEntry;
pub use mapper::TaxonomyMapper;
pub use registry::TaxonomyRegistry;
pub use report::{TaxonomyMapping, TaxonomyReport};

/// The crate's own semantic version, re-exported for the same
/// provenance-tracking reason as `fact_model::CRATE_VERSION`.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// See the equivalent constant in `grounding`/`matcher` for rationale.
pub const DSL_VERSION_USED: &str = dsl::CRATE_VERSION;

/// See [`DSL_VERSION_USED`].
pub const GROUNDING_VERSION_USED: &str = grounding::CRATE_VERSION;

#[cfg(test)]
#[allow(
    clippy::const_is_empty,
    reason = "these consts are env!(\"CARGO_PKG_VERSION\") — clippy can't see through env!, and can never actually be empty; the asserts are intentional canaries that env! resolved and cross-crate version re-exports link"
)]
mod tests {
    use super::*;

    #[test]
    fn crate_version_is_nonempty() {
        assert!(!CRATE_VERSION.is_empty());
    }

    #[test]
    fn dependencies_link() {
        assert!(!DSL_VERSION_USED.is_empty());
        assert!(!GROUNDING_VERSION_USED.is_empty());
    }
}

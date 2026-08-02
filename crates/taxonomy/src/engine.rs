//! [`MappingEngine`]: applies every registered
//! [`crate::mapper::TaxonomyMapper`] to a
//! [`grounding::GroundingResult`], producing one
//! [`TaxonomyReport`].
//!
//! # Independence from matching and grounding
//!
//! This module reads exactly two things off a
//! [`grounding::GroundingResult`]: its pattern identity/family (passed
//! to every mapper, read-only) and its [`grounding::GroundingStatus`]
//! (copied into the report, never inspected or branched on here). It
//! calls nothing in `matcher` or `grounding`, and neither of those
//! crates depends on `taxonomy` (see `crates/taxonomy/Cargo.toml`) — so
//! "taxonomy mapping never influences matching or grounding" is a fact
//! about the dependency graph, not just this module's behavior.

use dsl::ir::PatternFamily;

use grounding::GroundingResult;

use crate::registry::TaxonomyRegistry;
use crate::report::{TaxonomyMapping, TaxonomyReport};

/// Maps [`GroundingResult`]s to [`TaxonomyReport`]s using a
/// configurable [`TaxonomyRegistry`].
pub struct MappingEngine {
    registry: TaxonomyRegistry,
}

impl Default for MappingEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MappingEngine {
    /// A `MappingEngine` using the default mapper registry (SCWE and
    /// SWC — see [`TaxonomyRegistry::with_default_mappers`]).
    #[must_use]
    pub fn new() -> Self {
        Self {
            registry: TaxonomyRegistry::with_default_mappers(),
        }
    }

    /// A `MappingEngine` using a caller-supplied registry — for
    /// injecting a mapper for a taxonomy not yet built into this crate
    /// (see [`TaxonomyRegistry::register`]), or for restricting mapping
    /// to a subset of taxonomies.
    #[must_use]
    pub const fn with_registry(registry: TaxonomyRegistry) -> Self {
        Self { registry }
    }

    /// Map one [`GroundingResult`] across every taxonomy this engine's
    /// registry covers.
    ///
    /// Always succeeds and always returns a [`TaxonomyReport`]: mapping
    /// is a lookup against static tables, never a fallible operation
    /// (see this crate's top-level docs — there is deliberately no
    /// `TaxonomyError`).
    #[must_use]
    pub fn map(&self, result: &GroundingResult) -> TaxonomyReport {
        let mut mappings = Vec::new();
        let mut unmapped_taxonomies = Vec::new();

        for taxonomy in self.registry.taxonomies() {
            let Some(mapper) = self.registry.get(taxonomy) else {
                continue;
            };
            let entries = mapper.map_family(&result.pattern_family);
            if entries.is_empty() {
                unmapped_taxonomies.push(taxonomy);
            } else {
                mappings.push(TaxonomyMapping { taxonomy, entries });
            }
        }

        TaxonomyReport {
            pattern_id: result.pattern_id.clone(),
            pattern_version: result.pattern_version,
            pattern_family: result.pattern_family.clone(),
            candidate_id: result.candidate_id,
            grounding_status: result.status,
            mappings,
            unmapped_taxonomies,
        }
    }

    /// Map every result in `results`, preserving order —
    /// the batched counterpart to calling [`Self::map`] once per
    /// result.
    #[must_use]
    pub fn map_all(&self, results: &[GroundingResult]) -> Vec<TaxonomyReport> {
        results.iter().map(|r| self.map(r)).collect()
    }

    /// Directly map a bare [`PatternFamily`] across every registered
    /// taxonomy, without a [`GroundingResult`] in hand — useful for
    /// exploring or testing a taxonomy's coverage independent of any
    /// specific grounding run.
    #[must_use]
    pub fn map_family(&self, family: &PatternFamily) -> Vec<TaxonomyMapping> {
        let mut mappings = Vec::new();
        for taxonomy in self.registry.taxonomies() {
            let Some(mapper) = self.registry.get(taxonomy) else {
                continue;
            };
            let entries = mapper.map_family(family);
            if !entries.is_empty() {
                mappings.push(TaxonomyMapping { taxonomy, entries });
            }
        }
        mappings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use dsl::ir::{PatternId, PatternVersion, Severity};
    use grounding::report::ConfidenceMetadata;
    use grounding::{CandidateId, GroundingStatus};

    fn result_for(family: &str) -> GroundingResult {
        GroundingResult {
            pattern_id: PatternId("test.pattern".to_string()),
            pattern_version: PatternVersion(1),
            pattern_family: PatternFamily(family.to_string()),
            pattern_severity: Severity::High,
            candidate_id: CandidateId(42),
            status: GroundingStatus::Grounded,
            verified_evidence: Vec::new(),
            failed_evidence: Vec::new(),
            unavailable_evidence: Vec::new(),
            unsupported_evidence: Vec::new(),
            confidence: ConfidenceMetadata {
                required_total: 0,
                required_verified: 0,
                optional_total: 0,
                optional_verified: 0,
                sequence_verified: None,
                same_call_verified: None,
            },
            unresolved_attributes: Vec::new(),
            evidence_chain: Vec::new(),
            abstain_reasons: Vec::new(),
            explanation: "test".to_string(),
        }
    }

    #[test]
    fn reentrancy_maps_in_both_taxonomies() {
        let engine = MappingEngine::new();
        let report = engine.map(&result_for("Reentrancy"));
        assert_eq!(report.mappings.len(), 2);
        assert!(report.unmapped_taxonomies.is_empty());
        assert!(!report.is_fully_unmapped());
        assert_eq!(report.grounding_status, GroundingStatus::Grounded);
    }

    #[test]
    fn oracle_manipulation_maps_in_scwe_only() {
        let engine = MappingEngine::new();
        let report = engine.map(&result_for("OracleManipulation"));
        assert_eq!(report.mappings.len(), 1);
        assert_eq!(report.mappings[0].taxonomy, "SCWE");
        assert_eq!(report.unmapped_taxonomies, vec!["SWC"]);
    }

    #[test]
    fn unauthorized_upgrade_maps_in_scwe_only() {
        let engine = MappingEngine::new();
        let report = engine.map(&result_for("UnauthorizedUpgrade"));
        assert_eq!(report.mappings.len(), 1);
        assert_eq!(report.mappings[0].taxonomy, "SCWE");
        assert_eq!(report.mappings[0].entries[0].code, "SCWE-005");
        assert_eq!(report.unmapped_taxonomies, vec!["SWC"]);
    }

    #[test]
    fn unknown_family_is_fully_unmapped_but_still_reported() {
        let engine = MappingEngine::new();
        let report = engine.map(&result_for("SomeFutureFamily"));
        assert!(report.mappings.is_empty());
        assert_eq!(report.unmapped_taxonomies, vec!["SCWE", "SWC"]);
        assert!(report.is_fully_unmapped());
        // The report itself is still produced, preserving the
        // unmapped result rather than dropping it.
        assert_eq!(
            report.pattern_family,
            PatternFamily("SomeFutureFamily".to_string())
        );
    }

    #[test]
    fn map_all_preserves_order() {
        let engine = MappingEngine::new();
        let results = vec![result_for("Reentrancy"), result_for("OracleManipulation")];
        let reports = engine.map_all(&results);
        assert_eq!(reports.len(), 2);
        assert_eq!(
            reports[0].pattern_family,
            PatternFamily("Reentrancy".to_string())
        );
        assert_eq!(
            reports[1].pattern_family,
            PatternFamily("OracleManipulation".to_string())
        );
    }

    #[test]
    fn empty_registry_produces_fully_unmapped_reports() {
        let engine = MappingEngine::with_registry(TaxonomyRegistry::new());
        let report = engine.map(&result_for("Reentrancy"));
        assert!(report.mappings.is_empty());
        assert!(report.unmapped_taxonomies.is_empty());
    }
}

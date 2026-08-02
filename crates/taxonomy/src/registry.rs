//! [`TaxonomyRegistry`]: an extensible, taxonomy-name-keyed collection
//! of [`TaxonomyMapper`]s — the same shape as
//! `grounding::verifier::VerifierRegistry`, deliberately, so a reader
//! already familiar with that crate recognizes the pattern immediately.

use std::collections::HashMap;

use crate::mapper::TaxonomyMapper;

/// An extensible registry of [`TaxonomyMapper`]s, dispatched by
/// taxonomy name.
pub struct TaxonomyRegistry {
    mappers: HashMap<&'static str, Box<dyn TaxonomyMapper>>,
}

impl Default for TaxonomyRegistry {
    fn default() -> Self {
        Self::with_default_mappers()
    }
}

impl TaxonomyRegistry {
    /// An empty registry with no mappers. Populating every taxonomy a
    /// caller cares about is the caller's own responsibility with an
    /// empty registry — most callers want [`Self::with_default_mappers`]
    /// instead.
    #[must_use]
    pub fn new() -> Self {
        Self {
            mappers: HashMap::new(),
        }
    }

    /// A registry covering every taxonomy this crate ships a built-in
    /// mapper for: SCWE ([`crate::scwe::ScweMapper`]) and SWC
    /// ([`crate::swc::SwcMapper`]).
    #[must_use]
    pub fn with_default_mappers() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(crate::scwe::ScweMapper));
        registry.register(Box::new(crate::swc::SwcMapper));
        registry
    }

    /// Register `mapper`, replacing any existing mapper already
    /// registered for the same [`TaxonomyMapper::taxonomy`]. This is
    /// how a future taxonomy is supported: implement
    /// [`TaxonomyMapper`], register it here, and every existing
    /// mapper's code is untouched.
    pub fn register(&mut self, mapper: Box<dyn TaxonomyMapper>) {
        self.mappers.insert(mapper.taxonomy(), mapper);
    }

    /// Look up the mapper for `taxonomy`, if one is registered.
    #[must_use]
    pub fn get(&self, taxonomy: &str) -> Option<&dyn TaxonomyMapper> {
        self.mappers.get(taxonomy).map(std::convert::AsRef::as_ref)
    }

    /// Every taxonomy name this registry has a mapper for, in a fixed
    /// deterministic (lexicographic) order — `HashMap` iteration order
    /// is not itself stable across runs, and [`crate::engine::MappingEngine`]
    /// needs a reproducible mapper-visitation order for
    /// [`crate::report::TaxonomyReport::mappings`] to be deterministic.
    #[must_use]
    pub fn taxonomies(&self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = self.mappers.keys().copied().collect();
        names.sort_unstable();
        names
    }

    /// Whether this registry has no mappers registered at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.mappers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_covers_scwe_and_swc() {
        let registry = TaxonomyRegistry::with_default_mappers();
        assert!(registry.get("SCWE").is_some());
        assert!(registry.get("SWC").is_some());
    }

    #[test]
    fn empty_registry_has_no_mappers() {
        let registry = TaxonomyRegistry::new();
        assert!(registry.is_empty());
        assert!(registry.get("SCWE").is_none());
    }

    #[test]
    fn register_overwrites_existing_taxonomy() {
        let mut registry = TaxonomyRegistry::new();
        registry.register(Box::new(crate::scwe::ScweMapper));
        assert!(!registry.is_empty());
        registry.register(Box::new(crate::scwe::ScweMapper));
        assert_eq!(
            registry.get("SCWE").map(TaxonomyMapper::taxonomy),
            Some("SCWE")
        );
    }

    #[test]
    fn taxonomies_are_sorted_and_deterministic() {
        let registry = TaxonomyRegistry::with_default_mappers();
        assert_eq!(registry.taxonomies(), vec!["SCWE", "SWC"]);
    }
}

//! [`SwcMapper`]: maps [`dsl::ir::PatternFamily`] to entries in the
//! Smart Contract Weakness Classification (SWC) registry — this
//! project's secondary/compatibility taxonomy (Architecture doc,
//! Assumption A-3).
//!
//! # Table provenance and scope
//!
//! - `"Reentrancy"` → SWC-107 ("Reentrancy"), a long-standing,
//!   unambiguous entry in the SWC registry.
//! - `"OracleManipulation"` has **no** dedicated SWC entry: the SWC
//!   registry predates price-oracle manipulation's emergence as a
//!   distinct, commonly-cataloged exploit class (it is a later
//!   OWASP-SCS-era addition — see [`crate::scwe`]'s docs, SCWE-028).
//!   [`SwcMapper::map_family`] therefore returns an empty `Vec` for
//!   `"OracleManipulation"`, deliberately, rather than forcing a fit
//!   onto an unrelated SWC entry (e.g. front-running's SWC-114) that
//!   would misrepresent the weakness. This is this crate's own
//!   canonical example of a genuine, taxonomy-specific "unmapped"
//!   result — see [`crate::report::TaxonomyReport`]'s docs.
//! - `"UnauthorizedUpgrade"` is unmapped in SWC for the identical
//!   reason: the SWC registry predates the proxy/upgradeable-contract
//!   pattern's prominence, and has no entry for insecure upgrade
//!   authorization (SWC-105/106 cover unprotected ether withdrawal and
//!   `selfdestruct` specifically, not `upgradeTo`-style logic
//!   pointer changes — a real but different weakness). Left
//!   deliberately unmapped rather than forced onto either.
//! - `"UncheckedExternalCall"` → SWC-104 ("Unchecked Call Return
//!   Value"), a long-standing, unambiguous SWC registry entry for
//!   exactly this weakness.
//! - `"DelegatecallStorageCollision"` → SWC-112 ("Delegatecall to
//!   Untrusted Callee"). SWC-112's own description is exactly this
//!   mechanism — "the code at the target address can change any
//!   storage values of the caller" — so, unlike `OracleManipulation`
//!   and `UnauthorizedUpgrade` above, this is a genuine, unambiguous
//!   fit rather than a forced one.
//! - `"UnsafeSelfdestruct"` → SWC-106 ("Unprotected SELFDESTRUCT
//!   Instruction"), the SWC registry's own entry for exactly this
//!   weakness — added alongside `fact_model::CallKind::Selfdestruct`
//!   and the `unsafe_selfdestruct` pattern (Milestone 6).
//! - `"DelegatecallReachableSelfdestruct"` → SWC-106 as well
//!   (Milestone 9). Unlike the SCWE table (where this family
//!   deliberately gets its own, more specific entry), the SWC registry
//!   has only the one SELFDESTRUCT-related code — there is no
//!   delegatecall-specific SWC variant to prefer, and forcing a fit
//!   onto SWC-112 ("Delegatecall to Untrusted Callee") instead would
//!   misrepresent the weakness as a storage-collision issue, which it
//!   is not. Reusing SWC-106 here is a genuine, unambiguous fit, not a
//!   forced one: SWC-106's own description ("a SELFDESTRUCT
//!   instruction is not properly guarded") is exactly what this
//!   pattern flags, and the two families being one-to-one in SCWE but
//!   many-to-one in SWC is an accurate reflection of the two
//!   registries' differing granularity — not an inconsistency this
//!   crate needs to paper over.
//! - `"SpoofedTransferEvent"` (Milestone 12) is unmapped in SWC for the
//!   same reason as `"OracleManipulation"` and `"UnauthorizedUpgrade"`
//!   above: the SWC registry predates event-log-spoofing's emergence as
//!   a distinct, commonly-cataloged weakness (it is, like its SCWE-063
//!   counterpart, a later addition the SWC registry has no equivalent
//!   for), so this is left unmapped rather than forced onto an
//!   unrelated entry.

use dsl::ir::PatternFamily;

use crate::entry::TaxonomyEntry;
use crate::mapper::TaxonomyMapper;

/// Maps [`PatternFamily`] to SWC registry entries. See this module's
/// own docs for the table's current coverage, including the
/// deliberate absence of an oracle-manipulation entry.
pub struct SwcMapper;

impl TaxonomyMapper for SwcMapper {
    fn taxonomy(&self) -> &'static str {
        "SWC"
    }

    fn map_family(&self, family: &PatternFamily) -> Vec<TaxonomyEntry> {
        match family.0.as_str() {
            "Reentrancy" => vec![TaxonomyEntry::new(
                "SWC",
                "SWC-107",
                "Reentrancy",
                Some("https://swcregistry.io/docs/SWC-107/"),
            )],
            "UncheckedExternalCall" => vec![TaxonomyEntry::new(
                "SWC",
                "SWC-104",
                "Unchecked Call Return Value",
                Some("https://swcregistry.io/docs/SWC-104/"),
            )],
            "DelegatecallStorageCollision" => vec![TaxonomyEntry::new(
                "SWC",
                "SWC-112",
                "Delegatecall to Untrusted Callee",
                Some("https://swcregistry.io/docs/SWC-112/"),
            )],
            "UnsafeSelfdestruct" | "DelegatecallReachableSelfdestruct" => {
                vec![TaxonomyEntry::new(
                    "SWC",
                    "SWC-106",
                    "Unprotected SELFDESTRUCT Instruction",
                    Some("https://swcregistry.io/docs/SWC-106/"),
                )]
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reentrancy_maps_to_swc_107() {
        let entries = SwcMapper.map_family(&PatternFamily("Reentrancy".to_string()));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].code, "SWC-107");
        assert_eq!(entries[0].taxonomy, "SWC");
    }

    #[test]
    fn oracle_manipulation_is_genuinely_unmapped_in_swc() {
        let entries = SwcMapper.map_family(&PatternFamily("OracleManipulation".to_string()));
        assert!(entries.is_empty());
    }

    #[test]
    fn unauthorized_upgrade_is_genuinely_unmapped_in_swc() {
        let entries = SwcMapper.map_family(&PatternFamily("UnauthorizedUpgrade".to_string()));
        assert!(entries.is_empty());
    }

    #[test]
    fn spoofed_transfer_event_is_genuinely_unmapped_in_swc() {
        let entries = SwcMapper.map_family(&PatternFamily("SpoofedTransferEvent".to_string()));
        assert!(entries.is_empty());
    }

    #[test]
    fn unchecked_external_call_maps_to_swc_104() {
        let entries = SwcMapper.map_family(&PatternFamily("UncheckedExternalCall".to_string()));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].code, "SWC-104");
        assert_eq!(entries[0].taxonomy, "SWC");
    }

    #[test]
    fn delegatecall_storage_collision_maps_to_swc_112() {
        let entries =
            SwcMapper.map_family(&PatternFamily("DelegatecallStorageCollision".to_string()));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].code, "SWC-112");
        assert_eq!(entries[0].taxonomy, "SWC");
    }

    #[test]
    fn unsafe_selfdestruct_maps_to_swc_106() {
        let entries = SwcMapper.map_family(&PatternFamily("UnsafeSelfdestruct".to_string()));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].code, "SWC-106");
        assert_eq!(entries[0].taxonomy, "SWC");
    }

    #[test]
    fn delegatecall_reachable_selfdestruct_maps_to_swc_106() {
        let entries = SwcMapper.map_family(&PatternFamily(
            "DelegatecallReachableSelfdestruct".to_string(),
        ));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].code, "SWC-106");
        assert_eq!(entries[0].taxonomy, "SWC");
    }
}

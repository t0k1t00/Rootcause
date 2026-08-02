//! [`ScweMapper`]: maps [`dsl::ir::PatternFamily`] to entries in
//! OWASP's Smart Contract Weakness Enumeration (SCWE) — this project's
//! primary taxonomy (Architecture doc, Assumption A-3).
//!
//! # Table provenance and scope
//!
//! The Architecture document explicitly declines to specify a mapping
//! table structure ("No mapping table structure is specified by the
//! Review; this is left for a follow-on document") and the Research
//! doc names exactly two families as actually in scope for the current
//! project stage: oracle/price-manipulation and reentrancy-family
//! attacks. This table is seeded with real, verifiable SCWE entries for
//! those two families only:
//! - `"Reentrancy"` → SCWE-046 ("Reentrancy Attacks")
//! - `"OracleManipulation"` → SCWE-028 ("Price Oracle Manipulation")
//! - `"UnauthorizedUpgrade"` → SCWE-005 ("Insecure Upgradeable Proxy
//!   Design"), added alongside the first selector-dependent pattern
//!   (`unauthorized_upgrade_to`): a proxy `upgradeTo()` call reaching a
//!   live trace, invoked by an address other than the trace's own
//!   sender, is exactly the structural shape SCWE-005 describes.
//! - `"UncheckedExternalCall"` → SCWE-048 ("Unchecked Call Return
//!   Value"), added alongside the `unchecked_external_call` pattern: a
//!   `call` that reverted (`succeeded: false`) while the enclosing
//!   transaction still reports `Success` is exactly the structural
//!   signature of a caller that never checked the low-level call's
//!   return value.
//!
//! - `"DelegatecallStorageCollision"` → SCWE-150 ("Storage Slot
//!   Collision When Upgrading Implementation"), added alongside the
//!   `delegatecall_storage_collision` pattern: a `DELEGATECALL`
//!   (which runs callee code against the *caller's* storage) paired
//!   with a storage write landing on a specific, exact-match slot is
//!   exactly the structural shape of a caller/callee (or proxy/
//!   implementation) storage-layout collision.
//! - `"UnsafeSelfdestruct"` → SCWE-050 ("Unprotected SELFDESTRUCT
//!   Instruction"), added alongside the `unsafe_selfdestruct` pattern
//!   (Milestone 6): a `SELFDESTRUCT` call succeeding in a trace is
//!   exactly the structural shape OWASP's SCSVS-AUTH entry describes.
//! - `"DelegatecallReachableSelfdestruct"` → SCWE-038 ("Insecure Use
//!   of Selfdestruct"), added alongside the
//!   `delegatecall_reachable_selfdestruct` pattern (Milestone 9): this
//!   is deliberately a *different* SCWE entry than plain
//!   `"UnsafeSelfdestruct"`'s SCWE-050, not the same one reused. Both
//!   describe a SELFDESTRUCT weakness, but this family's structural
//!   signature is strictly more specific — the destruct call's direct
//!   parent frame must be a DELEGATECALL — so mapping it to the same
//!   entry as the plain-selfdestruct family would blur two distinct
//!   findings (which may both legitimately fire on the same trace)
//!   into one taxonomy code. SCWE-038's broader "used without proper
//!   safeguards" framing fits the delegatecall-reachable case, whose
//!   real-world example is the Parity multisig wallet library
//!   incident: a SELFDESTRUCT in a library contract, invoked only ever
//!   via DELEGATECALL from proxy wallets, destroyed the shared library
//!   and froze every depending wallet's funds — a strictly worse
//!   outcome than an ordinary unprotected SELFDESTRUCT, since the
//!   destroyed contract need not even be the caller's own.
//! - `"SpoofedTransferEvent"` → SCWE-063 ("Insecure Event Emission"),
//!   added alongside the `spoofed_transfer_event` pattern (Milestone
//!   12): a log carrying the standard ERC-20 `Transfer` event's topic0
//!   but not shaped like a real, decodable transfer (see
//!   `crates/ingestion/src/normalize.rs::try_decode_token_transfer`) is
//!   exactly SCWE-063's "emitted event data does not correspond to the
//!   actual state/outcome" weakness — an off-chain observer filtering
//!   logs by topic0 alone (a common shortcut) would be misled into
//!   believing a transfer occurred that the trace never actually
//!   completed.
//!
//! Every other [`PatternFamily`] this crate is asked about is
//! deliberately left unmapped ([`TaxonomyMapper::map_family`] returns
//! an empty `Vec`) rather than guessed at — see
//! [`crate::report::TaxonomyReport`]'s own docs for why an empty result
//! is a genuine, preserved outcome and not a gap this mapper should
//! paper over. Extending this table to further families is expected
//! future work, not something this crate's structure needs to change
//! to support (add a match arm; no other file changes).

use dsl::ir::PatternFamily;

use crate::entry::TaxonomyEntry;
use crate::mapper::TaxonomyMapper;

/// Maps [`PatternFamily`] to OWASP SCWE entries. See this module's own
/// docs for the table's current (deliberately narrow) coverage.
pub struct ScweMapper;

impl TaxonomyMapper for ScweMapper {
    fn taxonomy(&self) -> &'static str {
        "SCWE"
    }

    fn map_family(&self, family: &PatternFamily) -> Vec<TaxonomyEntry> {
        match family.0.as_str() {
            "Reentrancy" => vec![TaxonomyEntry::new(
                "SCWE",
                "SCWE-046",
                "Reentrancy Attacks",
                Some("https://scs.owasp.org/SCWE/SCSVS-CODE/SCWE-046/"),
            )],
            "OracleManipulation" => vec![TaxonomyEntry::new(
                "SCWE",
                "SCWE-028",
                "Price Oracle Manipulation",
                Some("https://scs.owasp.org/SCWE/SCSVS-ORACLE/SCWE-028/"),
            )],
            "UnauthorizedUpgrade" => vec![TaxonomyEntry::new(
                "SCWE",
                "SCWE-005",
                "Insecure Upgradeable Proxy Design",
                Some("https://scs.owasp.org/SCWE/SCSVS-ARCH/SCWE-005/"),
            )],
            "UncheckedExternalCall" => vec![TaxonomyEntry::new(
                "SCWE",
                "SCWE-048",
                "Unchecked Call Return Value",
                Some("https://scs.owasp.org/SCWE/SCSVS-CODE/SCWE-048/"),
            )],
            "DelegatecallStorageCollision" => vec![TaxonomyEntry::new(
                "SCWE",
                "SCWE-150",
                "Storage Slot Collision When Upgrading Implementation",
                Some("https://scs.owasp.org/SCWE/SCSVS-ARCH/SCWE-150/"),
            )],
            "UnsafeSelfdestruct" => vec![TaxonomyEntry::new(
                "SCWE",
                "SCWE-050",
                "Unprotected SELFDESTRUCT Instruction",
                Some("https://scs.owasp.org/SCWE/SCSVS-AUTH/SCWE-050/"),
            )],
            "DelegatecallReachableSelfdestruct" => vec![TaxonomyEntry::new(
                "SCWE",
                "SCWE-038",
                "Insecure Use of Selfdestruct",
                Some("https://scs.owasp.org/SCWE/SCSVS-AUTH/SCWE-038/"),
            )],
            "SpoofedTransferEvent" => vec![TaxonomyEntry::new(
                "SCWE",
                "SCWE-063",
                "Insecure Event Emission",
                Some("https://scs.owasp.org/SCWE/SCSVS-COMM/SCWE-063/"),
            )],
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reentrancy_maps_to_scwe_046() {
        let entries = ScweMapper.map_family(&PatternFamily("Reentrancy".to_string()));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].code, "SCWE-046");
        assert_eq!(entries[0].taxonomy, "SCWE");
    }

    #[test]
    fn oracle_manipulation_maps_to_scwe_028() {
        let entries = ScweMapper.map_family(&PatternFamily("OracleManipulation".to_string()));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].code, "SCWE-028");
    }

    #[test]
    fn unauthorized_upgrade_maps_to_scwe_005() {
        let entries = ScweMapper.map_family(&PatternFamily("UnauthorizedUpgrade".to_string()));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].code, "SCWE-005");
        assert_eq!(entries[0].taxonomy, "SCWE");
    }

    #[test]
    fn unchecked_external_call_maps_to_scwe_048() {
        let entries = ScweMapper.map_family(&PatternFamily("UncheckedExternalCall".to_string()));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].code, "SCWE-048");
        assert_eq!(entries[0].taxonomy, "SCWE");
    }

    #[test]
    fn delegatecall_storage_collision_maps_to_scwe_150() {
        let entries =
            ScweMapper.map_family(&PatternFamily("DelegatecallStorageCollision".to_string()));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].code, "SCWE-150");
        assert_eq!(entries[0].taxonomy, "SCWE");
    }

    #[test]
    fn delegatecall_reachable_selfdestruct_maps_to_scwe_038() {
        let entries = ScweMapper.map_family(&PatternFamily(
            "DelegatecallReachableSelfdestruct".to_string(),
        ));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].code, "SCWE-038");
        assert_eq!(entries[0].taxonomy, "SCWE");
    }

    #[test]
    fn unknown_family_is_unmapped() {
        let entries = ScweMapper.map_family(&PatternFamily("SomeFutureFamily".to_string()));
        assert!(entries.is_empty());
    }

    #[test]
    fn unsafe_selfdestruct_maps_to_scwe_050() {
        let entries = ScweMapper.map_family(&PatternFamily("UnsafeSelfdestruct".to_string()));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].code, "SCWE-050");
        assert_eq!(entries[0].taxonomy, "SCWE");
    }

    #[test]
    fn spoofed_transfer_event_maps_to_scwe_063() {
        let entries = ScweMapper.map_family(&PatternFamily("SpoofedTransferEvent".to_string()));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].code, "SCWE-063");
        assert_eq!(entries[0].taxonomy, "SCWE");
    }
}

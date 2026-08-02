//! [`StorageSlot`] and [`StorageChange`]: EVM storage state facts.

use serde::{Deserialize, Serialize};

use crate::ids::CallId;
use crate::primitives::{Address, Word};

/// A storage slot key, scoped to a specific contract [`Address`] — EVM
/// storage is per-contract, so a slot key alone (without the address) is
/// not a complete identity.
///
/// ## Why a distinct type from [`Word`]
/// A slot *key* and a slot *value* are both raw 256-bit words, but they
/// are never interchangeable: a pattern predicate that mixes them up
/// (e.g. compares a key to a value) is a bug that should be caught by
/// the type checker, not at runtime. Wrapping the key half separately —
/// paired with the owning `Address` — makes that class of bug
/// unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StorageSlot {
    /// The contract this slot belongs to.
    pub contract: Address,
    /// The slot key within that contract's storage.
    pub key: Word,
}

impl StorageSlot {
    /// Construct a `StorageSlot`. Infallible: any `(Address, Word)` pair
    /// is a structurally valid slot identity, whether or not that
    /// contract has ever actually written to that key.
    #[must_use]
    pub const fn new(contract: Address, key: Word) -> Self {
        Self { contract, key }
    }
}

/// A single before/after change to one [`StorageSlot`], attributed to
/// the specific [`crate::Call`] that performed the write.
///
/// This is the fact type the Research document's Week-1 experiment
/// (Phase 8) names as required to ground an "oracle staleness / donation
/// attack" predicate: such a predicate's evidence is precisely "slot X
/// changed from value A to value B during call Y."
///
/// ## Invariants
/// `before != after` is intentionally **not** enforced here. A
/// zero-net-change write (writing the same value back) is a legitimate,
/// if unusual, real EVM event, and rejecting it would make this type
/// unable to represent a real trace faithfully — `fact-model`'s job is
/// to represent what happened, not to pre-filter what ingestion
/// considers interesting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageChange {
    /// The call that performed this write.
    pub call_id: CallId,
    /// Which slot changed.
    pub slot: StorageSlot,
    /// The slot's value immediately before this write. [`Word::ZERO`]
    /// for a slot's first-ever write.
    pub before: Word,
    /// The slot's value immediately after this write.
    pub after: Word,
}

impl StorageChange {
    /// Construct a `StorageChange`. Infallible at the single-fact level;
    /// cross-referential validity (`call_id` must name a real call in
    /// the same arena) is enforced by [`crate::FactArenaBuilder::build`],
    /// not here, for the same reason as [`crate::Call::new`].
    #[must_use]
    pub const fn new(call_id: CallId, slot: StorageSlot, before: Word, after: Word) -> Self {
        Self {
            call_id,
            slot,
            before,
            after,
        }
    }

    /// Whether this write actually changed the slot's value.
    #[must_use]
    pub fn is_net_change(&self) -> bool {
        self.before != self.after
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::CallId;

    fn addr(byte: u8) -> Address {
        Address::new([byte; 20])
    }

    #[test]
    fn is_net_change_true_when_values_differ() {
        let change = StorageChange::new(
            CallId::from_index(0),
            StorageSlot::new(addr(1), Word::ZERO),
            Word::new([0; 32]),
            Word::new([1; 32]),
        );
        assert!(change.is_net_change());
    }

    #[test]
    fn is_net_change_false_when_values_equal() {
        let same = Word::new([7; 32]);
        let change = StorageChange::new(
            CallId::from_index(0),
            StorageSlot::new(addr(1), Word::ZERO),
            same,
            same,
        );
        assert!(!change.is_net_change());
    }

    #[test]
    fn storage_change_json_roundtrip() {
        let change = StorageChange::new(
            CallId::from_index(2),
            StorageSlot::new(addr(9), Word::new([3; 32])),
            Word::new([0; 32]),
            Word::new([9; 32]),
        );
        let json = serde_json::to_string(&change).unwrap();
        let back: StorageChange = serde_json::from_str(&json).unwrap();
        assert_eq!(change, back);
    }
}

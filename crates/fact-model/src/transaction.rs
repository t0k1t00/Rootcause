//! [`Transaction`]: the top-level executed transaction being classified.

use serde::{Deserialize, Serialize};

use crate::primitives::{Address, Gas, Nonce, TxHash, Wei};

/// Whether a transaction succeeded or reverted at the top level.
///
/// A distinct enum rather than a `bool` for the same reason as
/// [`crate::CallKind`]: `TxStatus::Reverted` reads unambiguously at
/// every call site, whereas a bare `false` requires the reader to
/// remember which polarity was chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TxStatus {
    /// The transaction completed without reverting.
    Success,
    /// The transaction reverted. A reverted transaction can still
    /// contain a rich call tree, storage changes (from calls that ran
    /// before the revert, whose effects are then unwound at the EVM
    /// level — meaning ingestion may not have recorded them at all,
    /// since they never took effect on-chain) and logs; this crate does
    /// not assume a reverted transaction's arena is empty.
    Reverted,
}

/// The top-level transaction a [`crate::Trace`] represents — the unit of
/// execution Root Cause classifies.
///
/// ## Invariants
/// None enforced beyond field-level construction; a `Transaction` is a
/// direct restatement of on-chain fact (a real transaction with this
/// hash either did or did not have these properties) and this crate has
/// no basis to reject any particular combination of them.
///
/// ## Ownership
/// Owned directly by [`crate::Trace`] (one `Transaction` per `Trace`, no
/// arena indirection needed since there is exactly one and nothing else
/// ever needs to reference it by ID). `Clone` is derived for the same
/// ergonomic reason as the other fact types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    /// This transaction's hash.
    pub hash: TxHash,
    /// The externally-owned account that signed and sent this
    /// transaction.
    pub from: Address,
    /// The transaction's direct recipient. `None` for a
    /// contract-creation transaction (the EVM's own convention: a
    /// creation transaction has no `to`).
    pub to: Option<Address>,
    /// The wei value sent directly with this transaction (distinct from
    /// value moved by internal calls within it — see
    /// [`crate::ValueFlow`]).
    pub value: Wei,
    /// The sender's nonce at the time this transaction was sent.
    pub nonce: Nonce,
    /// Total gas consumed by this transaction's entire execution,
    /// including all internal calls.
    pub gas_used: Gas,
    /// Whether the transaction succeeded or reverted.
    pub status: TxStatus,
}

impl Transaction {
    /// Construct a `Transaction`. Infallible: every field is a direct
    /// restatement of on-chain fact with no cross-field constraint this
    /// crate can meaningfully enforce.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        hash: TxHash,
        from: Address,
        to: Option<Address>,
        value: Wei,
        nonce: Nonce,
        gas_used: Gas,
        status: TxStatus,
    ) -> Self {
        Self {
            hash,
            from,
            to,
            value,
            nonce,
            gas_used,
            status,
        }
    }

    /// Whether this transaction is a contract-creation transaction (no
    /// `to` address).
    #[must_use]
    pub const fn is_contract_creation(&self) -> bool {
        self.to.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::Word;

    fn addr(byte: u8) -> Address {
        Address::new([byte; 20])
    }

    fn hash(byte: u8) -> TxHash {
        TxHash(Word::new([byte; 32]))
    }

    #[test]
    fn contract_creation_detected_by_missing_to() {
        let tx = Transaction::new(
            hash(1),
            addr(1),
            None,
            Wei::ZERO,
            Nonce(0),
            Gas(21_000),
            TxStatus::Success,
        );
        assert!(tx.is_contract_creation());
    }

    #[test]
    fn ordinary_call_is_not_contract_creation() {
        let tx = Transaction::new(
            hash(1),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Nonce(0),
            Gas(21_000),
            TxStatus::Success,
        );
        assert!(!tx.is_contract_creation());
    }

    #[test]
    fn transaction_json_roundtrip() {
        let tx = Transaction::new(
            hash(9),
            addr(1),
            Some(addr(2)),
            Wei(42),
            Nonce(7),
            Gas(100_000),
            TxStatus::Reverted,
        );
        let json = serde_json::to_string(&tx).unwrap();
        let back: Transaction = serde_json::from_str(&json).unwrap();
        assert_eq!(tx, back);
    }
}

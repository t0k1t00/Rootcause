//! [`BlockContext`]: the block-level execution environment a
//! [`crate::Transaction`] ran within.

use serde::{Deserialize, Serialize};

use crate::primitives::{BlockNumber, ChainId, Timestamp, Wei};

/// The block-level context a transaction executed in: everything about
/// the surrounding block that a pattern predicate might legitimately
/// need to reference (e.g. a timestamp-dependent oracle-staleness check
/// needs the block timestamp, not just the transaction's own fields).
///
/// ## Invariants
/// None enforced beyond field-level construction, for the same reason as
/// [`crate::Transaction`]: this is a direct restatement of on-chain
/// block header fact.
///
/// ## Ownership
/// Owned directly by [`crate::Trace`] — one block per trace (a trace
/// covers exactly one transaction, which executed in exactly one
/// block), `Copy` since every field is itself `Copy` and small.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockContext {
    /// The block number the transaction was included in.
    pub number: BlockNumber,
    /// The block's Unix timestamp.
    pub timestamp: Timestamp,
    /// The chain this block belongs to — required for unambiguous
    /// cross-chain identity (see [`ChainId`]'s own documentation).
    pub chain_id: ChainId,
    /// The block's base fee, if the chain has EIP-1559 active at this
    /// block height. `None` for pre-EIP-1559 blocks or chains that never
    /// adopted it.
    pub base_fee: Option<Wei>,
}

impl BlockContext {
    /// Construct a `BlockContext`. Infallible: every field is a direct
    /// restatement of on-chain block-header fact.
    #[must_use]
    pub const fn new(
        number: BlockNumber,
        timestamp: Timestamp,
        chain_id: ChainId,
        base_fee: Option<Wei>,
    ) -> Self {
        Self {
            number,
            timestamp,
            chain_id,
            base_fee,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_context_json_roundtrip() {
        let block = BlockContext::new(
            BlockNumber(18_000_000),
            Timestamp(1_700_000_000),
            ChainId(1),
            Some(Wei(30_000_000_000)),
        );
        let json = serde_json::to_string(&block).unwrap();
        let back: BlockContext = serde_json::from_str(&json).unwrap();
        assert_eq!(block, back);
    }

    #[test]
    fn block_context_without_base_fee() {
        let block = BlockContext::new(BlockNumber(1), Timestamp(0), ChainId(1), None);
        let json = serde_json::to_string(&block).unwrap();
        let back: BlockContext = serde_json::from_str(&json).unwrap();
        assert_eq!(block, back);
        assert!(back.base_fee.is_none());
    }
}

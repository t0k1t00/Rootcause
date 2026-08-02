//! [`Trace`]: the complete, validated fact model for one transaction.

use serde::{Deserialize, Serialize};

use crate::arena::FactArena;
use crate::block::BlockContext;
use crate::error::FactModelError;
use crate::primitives::{BlockNumber, ChainId, TxHash};
use crate::transaction::Transaction;

/// Where a [`Trace`]'s facts were ingested from.
///
/// Per Assumption A-5 (Architecture doc), archive-node RPC is the only
/// ingestion source directly evidenced by the canonical specification;
/// this enum reflects exactly that — it is not a placeholder for sources
/// the specification does not support (Foundry, Tenderly, Phalcon, etc.
/// are explicitly out of scope, per the Architecture document's own
/// "Trace Ingestion" section).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceSource {
    /// Pulled from an archive node's `debug_traceTransaction` (or
    /// equivalent) RPC method, plus a storage diff, as described in the
    /// Research document's Week-1/two-week-prototype experiments.
    ArchiveNodeRpc {
        /// The RPC endpoint's host, recorded for provenance (e.g. so a
        /// benchmark result can note which provider's trace format was
        /// used) — deliberately just a host label, not a full URL with
        /// credentials, since a `Trace` may be persisted/published (the
        /// benchmark is a stated citable artifact) and must not leak
        /// access tokens.
        endpoint_label: String,
    },
}

/// Provenance metadata for a [`Trace`], distinct from the on-chain facts
/// themselves.
///
/// ## Why this crate never records an ingestion timestamp
/// A "when was this ingested" field would make two `Trace`s built from
/// the exact same on-chain transaction compare unequal depending on
/// *when* ingestion ran — directly undermining the determinism
/// invariant. `TraceMetadata` therefore records only facts about the
/// *source*, not facts about the *ingestion process's own execution*.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceMetadata {
    /// This trace's transaction hash, duplicated here (in addition to
    /// living on [`Transaction::hash`]) so metadata alone is enough to
    /// identify which transaction a trace is for, without needing to
    /// look inside the transaction — useful for benchmark indexing,
    /// logging, and error messages that should not need a full `Trace`
    /// in scope.
    pub transaction_hash: TxHash,
    /// The chain this trace's transaction executed on.
    pub chain_id: ChainId,
    /// The block this trace's transaction was included in.
    pub block_number: BlockNumber,
    /// Where these facts were ingested from.
    pub source: TraceSource,
}

impl TraceMetadata {
    /// Construct `TraceMetadata`. Infallible: every field is either a
    /// direct restatement of on-chain fact or a source-provenance label.
    #[must_use]
    pub const fn new(
        transaction_hash: TxHash,
        chain_id: ChainId,
        block_number: BlockNumber,
        source: TraceSource,
    ) -> Self {
        Self {
            transaction_hash,
            chain_id,
            block_number,
            source,
        }
    }
}

/// The complete, validated fact model for one transaction: everything
/// every other crate in the workspace needs to classify it.
///
/// ## Invariants
/// In addition to [`FactArena`]'s own tree/cross-reference invariants
/// (enforced when the arena itself was built), [`Trace::new`] enforces
/// **cross-type** consistency:
/// - `metadata.transaction_hash == transaction.hash`
/// - `metadata.chain_id == block.chain_id`
/// - `metadata.block_number == block.number`
///
/// These exist because `TraceMetadata` deliberately duplicates a few
/// fields already present on `Transaction`/`BlockContext` (for
/// standalone-lookup convenience, see [`TraceMetadata`]'s own
/// documentation) — duplication without a consistency check would be a
/// real bug surface, so `Trace::new` is the one place that check lives,
/// rather than repeating it in every downstream consumer.
///
/// ## Ownership
/// The top-level owned value every other crate is handed. `Clone` is
/// **not** derived: a `Trace` can contain an arbitrarily large arena (see
/// "suitable for large traces"), and cloning an entire trace is a real
/// cost that should be visible at call sites, not hidden behind an
/// innocuous-looking `.clone()`. Crates that need to share a `Trace`
/// across threads or hold onto it while also passing it around should
/// wrap it in `Arc<Trace>` at their own call site — `fact-model` does
/// not impose that choice by baking `Arc` into the type itself, since
/// not every consumer needs shared ownership.
#[derive(Debug, PartialEq, Eq)]
pub struct Trace {
    /// Provenance metadata.
    pub metadata: TraceMetadata,
    /// The block this trace's transaction executed in.
    pub block: BlockContext,
    /// The transaction being classified.
    pub transaction: Transaction,
    /// Every call, storage change, log, and token transfer observed
    /// during execution.
    pub arena: FactArena,
}

impl Trace {
    /// Construct a `Trace`, enforcing cross-type consistency between
    /// `metadata`, `block`, and `transaction`.
    ///
    /// The `arena` parameter is a pre-built [`FactArena`] (via
    /// [`crate::FactArenaBuilder::build`]) rather than raw fact lists,
    /// since arena-level validation is that type's own responsibility —
    /// `Trace::new` only adds the validation that is specifically about
    /// *this* type's fields relating to each other.
    ///
    /// # Errors
    /// Returns [`FactModelError`] variants reused from the arena's own
    /// vocabulary would be a category error (these aren't arena
    /// invariants), so cross-type mismatches are reported via a
    /// dedicated `&'static str`, consistent with how [`crate::Call::new`]
    /// and [`crate::LogEvent::new`] report their own local,
    /// non-arena-level invariants.
    pub fn new(
        metadata: TraceMetadata,
        block: BlockContext,
        transaction: Transaction,
        arena: FactArena,
    ) -> Result<Self, &'static str> {
        if metadata.transaction_hash != transaction.hash {
            return Err("TraceMetadata.transaction_hash does not match Transaction.hash");
        }
        if metadata.chain_id != block.chain_id {
            return Err("TraceMetadata.chain_id does not match BlockContext.chain_id");
        }
        if metadata.block_number != block.number {
            return Err("TraceMetadata.block_number does not match BlockContext.number");
        }
        Ok(Self {
            metadata,
            block,
            transaction,
            arena,
        })
    }
}

/// Everything that can go wrong building a complete [`Trace`] from
/// scratch: either the [`FactArena`] itself failed to validate, or the
/// arena was fine but didn't agree with the trace's own metadata/block/
/// transaction fields.
///
/// This is a separate type from [`FactModelError`] (rather than adding
/// cross-type-mismatch variants directly to `FactModelError`) because
/// `FactModelError` is scoped to arena-internal invariants by its own
/// documented contract; a caller pattern-matching on `FactModelError`
/// should not need to account for variants that have nothing to do with
/// arenas.
#[derive(Debug, thiserror::Error)]
pub enum TraceConstructionError {
    /// The [`FactArena`] itself failed validation.
    #[error(transparent)]
    Arena(#[from] FactModelError),
    /// The arena was valid, but `Trace::new`'s cross-type consistency
    /// check failed.
    #[error("{0}")]
    Inconsistent(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call::{Call, CallKind};
    use crate::primitives::{Address, CallDepth, Gas, Nonce, Timestamp, Wei, Word};
    use crate::transaction::TxStatus;
    use crate::FactArenaBuilder;

    fn addr(byte: u8) -> Address {
        Address::new([byte; 20])
    }

    fn hash(byte: u8) -> TxHash {
        TxHash(Word::new([byte; 32]))
    }

    fn build_minimal_arena() -> FactArena {
        let mut builder = FactArenaBuilder::new();
        let root = Call::new(
            builder.next_call_id(),
            None,
            CallKind::Call,
            CallDepth(0),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Gas(21_000),
            Gas(21_000),
            true,
        )
        .unwrap();
        builder.add_call(root);
        builder.build().unwrap()
    }

    #[test]
    fn consistent_trace_constructs() {
        let arena = build_minimal_arena();
        let tx = Transaction::new(
            hash(1),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Nonce(0),
            Gas(21_000),
            TxStatus::Success,
        );
        let block = BlockContext::new(BlockNumber(100), Timestamp(1_000), ChainId(1), None);
        let metadata = TraceMetadata::new(
            hash(1),
            ChainId(1),
            BlockNumber(100),
            TraceSource::ArchiveNodeRpc {
                endpoint_label: "example-provider".to_string(),
            },
        );
        assert!(Trace::new(metadata, block, tx, arena).is_ok());
    }

    #[test]
    fn mismatched_transaction_hash_rejected() {
        let arena = build_minimal_arena();
        let tx = Transaction::new(
            hash(1),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Nonce(0),
            Gas(21_000),
            TxStatus::Success,
        );
        let block = BlockContext::new(BlockNumber(100), Timestamp(1_000), ChainId(1), None);
        let metadata = TraceMetadata::new(
            hash(2), // mismatched
            ChainId(1),
            BlockNumber(100),
            TraceSource::ArchiveNodeRpc {
                endpoint_label: "example-provider".to_string(),
            },
        );
        assert!(Trace::new(metadata, block, tx, arena).is_err());
    }

    #[test]
    fn mismatched_chain_id_rejected() {
        let arena = build_minimal_arena();
        let tx = Transaction::new(
            hash(1),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Nonce(0),
            Gas(21_000),
            TxStatus::Success,
        );
        let block = BlockContext::new(BlockNumber(100), Timestamp(1_000), ChainId(1), None);
        let metadata = TraceMetadata::new(
            hash(1),
            ChainId(999), // mismatched
            BlockNumber(100),
            TraceSource::ArchiveNodeRpc {
                endpoint_label: "example-provider".to_string(),
            },
        );
        assert!(Trace::new(metadata, block, tx, arena).is_err());
    }

    #[test]
    fn mismatched_block_number_rejected() {
        let arena = build_minimal_arena();
        let tx = Transaction::new(
            hash(1),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Nonce(0),
            Gas(21_000),
            TxStatus::Success,
        );
        let block = BlockContext::new(BlockNumber(100), Timestamp(1_000), ChainId(1), None);
        let metadata = TraceMetadata::new(
            hash(1),
            ChainId(1),
            BlockNumber(999), // mismatched
            TraceSource::ArchiveNodeRpc {
                endpoint_label: "example-provider".to_string(),
            },
        );
        assert!(Trace::new(metadata, block, tx, arena).is_err());
    }

    #[test]
    fn trace_construction_error_from_fact_model_error() {
        let err: TraceConstructionError = FactModelError::EmptyCallList.into();
        assert!(matches!(err, TraceConstructionError::Arena(_)));
    }

    #[test]
    fn trace_metadata_json_roundtrip() {
        let metadata = TraceMetadata::new(
            hash(1),
            ChainId(1),
            BlockNumber(100),
            TraceSource::ArchiveNodeRpc {
                endpoint_label: "example-provider".to_string(),
            },
        );
        let json = serde_json::to_string(&metadata).unwrap();
        let back: TraceMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(metadata, back);
    }
}

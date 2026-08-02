//! Execution-order approximation: turns a [`FactRef`] into a sortable
//! key usable by [`crate::sequence`] to check a pattern's `sequence:`
//! constraint.
//!
//! # Modeling assumption
//! `fact-model` guarantees calls are stored in a valid topological order
//! (parent before every child — see `FactArena::calls`'s own docs), so
//! a call's arena index is a sound proxy for "how early this call
//! occurred relative to other calls." `fact-model` does **not**
//! similarly document that `StorageChange`/`LogEvent` insertion order
//! matches real execution order relative to *calls* (only that each
//! belongs to a specific call). This module's order key is therefore an
//! approximation, not an exact trace-wide execution order: every fact is
//! ordered by the arena index of the call it occurred *during* (its
//! "anchor call"), so facts from different calls compare correctly
//! whenever their anchor calls do, but facts sharing the same anchor
//! call (e.g. two storage writes performed by the same call) are treated
//! as co-occurring rather than ordered relative to each other. This is
//! conservative in the direction that matters for a candidate-generation
//! stage: it never *invents* an ordering fact-model doesn't actually
//! provide, so [`crate::sequence`] cannot reject a real match because of
//! a fabricated tiebreak, only fail to distinguish same-call facts that
//! genuinely might need finer-grained data than a structural matcher has
//! access to. See this crate's top-level docs, "Sequence evaluation,"
//! for the full rationale.

use fact_model::{FactRef, Trace};

/// A fact's approximate execution-order position, or `None` for a fact
/// with no single point-in-time occurrence.
///
/// [`FactRef::Call`] currently never resolves to `None` (see
/// [`order_key`]'s own docs) — the `Option` exists because a future
/// evidence kind describing a whole-trace property (this crate's own
/// `transaction` predicate is exactly that shape, though it does not
/// itself produce a citable [`FactRef`] today; see [`crate::predicate`])
/// would need to report "compatible with any position" rather than a
/// specific one, and [`crate::sequence`] already treats `None` that way.
pub type OrderKey = Option<u32>;

/// Compute `fact`'s approximate execution-order key within `trace`.
///
/// Returns `None` only if `fact` does not resolve against `trace` at all
/// (a `FactRef` from a different trace, or a malformed one) — callers
/// should treat that the same as "no order information," not as a
/// structural-matching bug, since this crate builds every `FactRef` it
/// hands out from the same `trace` it is currently matching and should
/// therefore never itself construct one that fails to resolve.
#[must_use]
pub fn order_key(trace: &Trace, fact: FactRef) -> OrderKey {
    match fact {
        FactRef::Call(id) => trace.arena.call(id).map(|c| c.id.index()),
        FactRef::StorageChange(id) => trace
            .arena
            .storage_change(id)
            .and_then(|change| trace.arena.call(change.call_id))
            .map(|c| c.id.index()),
        FactRef::Log(id) => trace
            .arena
            .log(id)
            .and_then(|log| trace.arena.call(log.call_id))
            .map(|c| c.id.index()),
        FactRef::TokenTransfer(id) => trace.arena.token_transfer(id).and_then(|transfer| {
            trace
                .arena
                .log(transfer.log_id)
                .and_then(|log| trace.arena.call(log.call_id))
                .map(|c| c.id.index())
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fact_model::{
        BlockContext, BlockNumber, CallDepth, CallId, CallKind, ChainId, FactArenaBuilder, Gas,
        LogEvent, LogIndex, Nonce, StorageChange, StorageSlot, Timestamp, TokenTransfer,
        TraceMetadata, TraceSource, Transaction, TxStatus, Wei, Word,
    };

    use fact_model::Address;

    fn addr(byte: u8) -> Address {
        Address::new([byte; 20])
    }

    fn hash(byte: u8) -> fact_model::TxHash {
        fact_model::TxHash(Word::new([byte; 32]))
    }

    fn build_trace(arena: fact_model::FactArena) -> Trace {
        let tx = Transaction::new(
            hash(1),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Nonce(0),
            Gas(21_000),
            TxStatus::Success,
        );
        let block = BlockContext::new(BlockNumber(1), Timestamp(1), ChainId(1), None);
        let metadata = TraceMetadata::new(
            hash(1),
            ChainId(1),
            BlockNumber(1),
            TraceSource::ArchiveNodeRpc {
                endpoint_label: "test".to_string(),
            },
        );
        Trace::new(metadata, block, tx, arena).unwrap()
    }

    #[test]
    fn call_order_key_is_own_index() {
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0);
        let root_id = builder.add_call(root);
        let child = make_call(&builder, Some(root_id), 1);
        let child_id = builder.add_call(child);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena);

        assert_eq!(order_key(&trace, FactRef::Call(root_id)), Some(0));
        assert_eq!(order_key(&trace, FactRef::Call(child_id)), Some(1));
    }

    #[test]
    fn storage_change_order_key_uses_owning_call() {
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0);
        let root_id = builder.add_call(root);
        let child = make_call(&builder, Some(root_id), 1);
        let child_id = builder.add_call(child);
        let change_id = builder.add_storage_change(StorageChange::new(
            child_id,
            StorageSlot::new(addr(9), Word::ZERO),
            Word::ZERO,
            Word::new([1; 32]),
        ));
        let arena = builder.build().unwrap();
        let trace = build_trace(arena);

        assert_eq!(
            order_key(&trace, FactRef::StorageChange(change_id)),
            Some(1)
        );
    }

    #[test]
    fn token_transfer_order_key_uses_underlying_logs_call() {
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0);
        let root_id = builder.add_call(root);
        let log_id = builder.next_log_id();
        builder
            .add_log(LogEvent::new(log_id, root_id, addr(5), vec![], vec![], LogIndex(0)).unwrap());
        let transfer_id = builder.add_token_transfer(TokenTransfer::new(
            log_id,
            addr(5),
            addr(1),
            addr(2),
            Wei(1),
        ));
        let arena = builder.build().unwrap();
        let trace = build_trace(arena);

        assert_eq!(
            order_key(&trace, FactRef::TokenTransfer(transfer_id)),
            Some(0)
        );
    }

    fn make_call(
        builder: &FactArenaBuilder,
        parent: Option<CallId>,
        depth: u16,
    ) -> fact_model::Call {
        fact_model::Call::new(
            builder.next_call_id(),
            parent,
            CallKind::Call,
            CallDepth(depth),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Gas(1),
            Gas(1),
            true,
        )
        .unwrap()
    }
}

//! Cross-fact call identity: deriving *which* [`CallId`] produced an
//! arbitrary [`FactRef`], regardless of fact kind.
//!
//! This is the one genuinely new piece of logic `same_call:` support
//! needs (see the crate-level docs' "`same_call` correlation" section):
//! everything it depends on already exists in `fact-model` today — a
//! `Call` is itself the call it names, `StorageChange`/`LogEvent` name
//! their producing call directly (`call_id`), and `TokenTransfer` names
//! it one hop away, through the `LogEvent` it was emitted alongside
//! (`log_id → LogEvent.call_id`). [`producing_call`] is a pure, total
//! function over that existing data — it adds no new fact, no new
//! index, and no mutation.
//!
//! Both [`crate::engine`] (structural matching) and the `grounding`
//! crate (independent re-verification) call this exact function rather
//! than each deriving their own notion of "the call that produced this
//! fact" — see this crate's top-level docs, "Why grounding is
//! intentionally excluded," for the general principle this mirrors:
//! shared semantics belong in one place, reused, not redefined per
//! consumer.

use fact_model::{CallId, FactRef, Trace};

/// The [`CallId`] that produced `fact`, or `None` if `fact` does not
/// resolve against `trace` at all (a dangling reference — see
/// [`crate::error::MatcherError`] and the `grounding` crate's own
/// "never trust the candidate's own binding" principle for why this
/// returns `Option` rather than panicking).
///
/// - A [`FactRef::Call`] *is* the call it names: its own [`CallId`].
/// - A [`FactRef::StorageChange`] or [`FactRef::Log`] names its
///   producing call directly, via `call_id`.
/// - A [`FactRef::TokenTransfer`] names it one hop away: through the
///   [`fact_model::LogEvent`] it was emitted alongside (`log_id`),
///   which in turn names its own producing call.
#[must_use]
pub fn producing_call(trace: &Trace, fact: FactRef) -> Option<CallId> {
    match fact {
        FactRef::Call(id) => Some(id),
        FactRef::StorageChange(id) => trace.arena.storage_change(id).map(|sc| sc.call_id),
        FactRef::Log(id) => trace.arena.log(id).map(|log| log.call_id),
        FactRef::TokenTransfer(id) => trace
            .arena
            .token_transfer(id)
            .and_then(|transfer| trace.arena.log(transfer.log_id))
            .map(|log| log.call_id),
    }
}

#[cfg(test)]
mod tests {
    use fact_model::{
        Address, BlockContext, BlockNumber, Call, CallDepth, CallKind, ChainId, FactArenaBuilder,
        Gas, LogEvent, LogIndex, Nonce, StorageChange, StorageSlot, Timestamp, TokenTransfer,
        TraceMetadata, TraceSource, Transaction, TxHash, TxStatus, Wei, Word,
    };

    use super::*;

    fn addr(byte: u8) -> Address {
        Address::new([byte; 20])
    }

    fn hash(byte: u8) -> TxHash {
        TxHash(Word::new([byte; 32]))
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
    fn call_producing_call_is_itself() {
        let mut builder = FactArenaBuilder::new();
        let call = Call::new(
            builder.next_call_id(),
            None,
            CallKind::Call,
            CallDepth(0),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Gas(100_000),
            Gas(50_000),
            true,
        )
        .unwrap();
        let call_id = builder.add_call(call);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena);

        assert_eq!(
            producing_call(&trace, FactRef::Call(call_id)),
            Some(call_id)
        );
    }

    #[test]
    fn storage_change_producing_call_is_its_call_id() {
        let mut builder = FactArenaBuilder::new();
        let call = Call::new(
            builder.next_call_id(),
            None,
            CallKind::Call,
            CallDepth(0),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Gas(100_000),
            Gas(50_000),
            true,
        )
        .unwrap();
        let call_id = builder.add_call(call);
        let sc_id = builder.add_storage_change(StorageChange::new(
            call_id,
            StorageSlot::new(addr(2), Word::ZERO),
            Word::ZERO,
            Word::new([1; 32]),
        ));
        let arena = builder.build().unwrap();
        let trace = build_trace(arena);

        assert_eq!(
            producing_call(&trace, FactRef::StorageChange(sc_id)),
            Some(call_id)
        );
    }

    #[test]
    fn log_producing_call_is_its_call_id() {
        let mut builder = FactArenaBuilder::new();
        let call = Call::new(
            builder.next_call_id(),
            None,
            CallKind::Call,
            CallDepth(0),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Gas(100_000),
            Gas(50_000),
            true,
        )
        .unwrap();
        let call_id = builder.add_call(call);
        let log_id = builder.add_log(
            LogEvent::new(
                builder.next_log_id(),
                call_id,
                addr(2),
                vec![],
                vec![],
                LogIndex(0),
            )
            .unwrap(),
        );
        let arena = builder.build().unwrap();
        let trace = build_trace(arena);

        assert_eq!(producing_call(&trace, FactRef::Log(log_id)), Some(call_id));
    }

    #[test]
    fn token_transfer_producing_call_is_its_log_s_call_id() {
        let mut builder = FactArenaBuilder::new();
        let call = Call::new(
            builder.next_call_id(),
            None,
            CallKind::Call,
            CallDepth(0),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Gas(100_000),
            Gas(50_000),
            true,
        )
        .unwrap();
        let call_id = builder.add_call(call);
        let log_id = builder.add_log(
            LogEvent::new(
                builder.next_log_id(),
                call_id,
                addr(2),
                vec![],
                vec![],
                LogIndex(0),
            )
            .unwrap(),
        );
        let transfer_id = builder.add_token_transfer(TokenTransfer::new(
            log_id,
            addr(2),
            addr(1),
            addr(3),
            Wei(1),
        ));
        let arena = builder.build().unwrap();
        let trace = build_trace(arena);

        assert_eq!(
            producing_call(&trace, FactRef::TokenTransfer(transfer_id)),
            Some(call_id)
        );
    }
}

//! The Construct Fact Model stage: [`NormalizedTrace`] →
//! [`fact_model::Trace`].

use fact_model::{
    BlockContext, Call, CallId, FactArenaBuilder, LogEvent, LogId, StorageChange, StorageSlot,
    TokenTransfer, Trace, TraceMetadata, TraceSource as FactTraceSource, Transaction,
};

use crate::error::IngestionError;
use crate::normalize::NormalizedTrace;

/// Build a validated [`fact_model::Trace`] from a [`NormalizedTrace`].
///
/// Calls are added to the [`FactArenaBuilder`] in the same depth-first
/// order [`crate::normalize::normalize`] already established, so each
/// call's assigned [`CallId`] (returned by
/// [`FactArenaBuilder::add_call`]) is tracked in a plain `Vec<CallId>`
/// indexed by the same `usize` positions `NormalizedTrace::calls` uses —
/// `fact-model` deliberately does not let this crate fabricate a
/// `CallId` from a raw index (`CallId::from_index` is `pub(crate)` to
/// `fact-model`, by design: see `fact_model::ids`'s own documentation),
/// so tracking the real, returned IDs is the only correct way to resolve
/// `normalized.calls[i].parent_index` into the parent's actual `CallId`.
///
/// # Errors
/// Returns [`IngestionError::ArenaConstruction`] or
/// [`IngestionError::TraceConstruction`] if `fact-model`'s own
/// construction-time validation rejects the result. This should not be
/// reachable from any malformed *input* — [`crate::normalize::normalize`]
/// already enforces every invariant `fact-model` would otherwise reject
/// on (parent precedence, depth consistency, cross-references) — so
/// reaching this path indicates a bug in this crate's own normalization,
/// surfaced rather than panicking.
pub fn build(
    normalized: &NormalizedTrace,
    source: FactTraceSource,
) -> Result<Trace, IngestionError> {
    let mut arena_builder = FactArenaBuilder::new();

    let mut call_ids: Vec<CallId> = Vec::with_capacity(normalized.calls.len());
    for nc in &normalized.calls {
        let id = arena_builder.next_call_id();
        // `parent_index` always refers to a call already pushed into
        // `call_ids` in an earlier iteration of this same loop, since
        // `normalize::walk_call` only ever records a parent's index
        // after that parent has already been assigned one (depth-first,
        // parent-before-child).
        let parent = nc.parent_index.map(|i| call_ids[i]);
        let call = Call::new(
            id,
            parent,
            nc.kind,
            nc.depth,
            nc.from,
            nc.to,
            nc.value,
            nc.gas_limit,
            nc.gas_used,
            nc.succeeded,
        )
        .map_err(IngestionError::TraceConstruction)?;
        let call = match nc.selector {
            Some(selector) => call.with_selector(selector),
            None => call,
        };
        arena_builder.add_call(call);
        call_ids.push(id);
    }

    for sc in &normalized.storage_changes {
        let call_id = call_ids[sc.call_index];
        arena_builder.add_storage_change(StorageChange::new(
            call_id,
            StorageSlot::new(sc.contract, sc.slot),
            sc.before,
            sc.after,
        ));
    }

    let mut log_ids: Vec<LogId> = Vec::with_capacity(normalized.logs.len());
    for log in &normalized.logs {
        let call_id = call_ids[log.call_index];
        let log_id = arena_builder.next_log_id();
        let log_event = LogEvent::new(
            log_id,
            call_id,
            log.address,
            log.topics.clone(),
            log.data.clone(),
            log.log_index,
        )
        .map_err(IngestionError::TraceConstruction)?;
        arena_builder.add_log(log_event);
        log_ids.push(log_id);
    }

    for transfer in &normalized.token_transfers {
        let log_id = log_ids[transfer.log_index_in_vec];
        arena_builder.add_token_transfer(TokenTransfer::new(
            log_id,
            transfer.token,
            transfer.from,
            transfer.to,
            transfer.amount,
        ));
    }

    let arena = arena_builder.build()?;

    let transaction = Transaction::new(
        normalized.transaction_hash,
        normalized.transaction_from,
        normalized.transaction_to,
        normalized.transaction_value,
        normalized.transaction_nonce,
        normalized.transaction_gas_used,
        normalized.transaction_status,
    );

    let block = BlockContext::new(
        normalized.block_number,
        normalized.block_timestamp,
        normalized.block_chain_id,
        normalized.block_base_fee,
    );

    let metadata = TraceMetadata::new(
        normalized.transaction_hash,
        normalized.block_chain_id,
        normalized.block_number,
        source,
    );

    Trace::new(metadata, block, transaction, arena).map_err(IngestionError::TraceConstruction)
}

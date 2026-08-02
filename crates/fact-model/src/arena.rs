//! [`FactArena`]: the immutable, validated owner of every fact in a
//! trace.

use crate::call::Call;
use crate::error::FactModelError;
use crate::ids::{CallId, LogId, StorageChangeId, TokenTransferId};
use crate::log::{LogEvent, TokenTransfer};
use crate::primitives::Address;
use crate::storage::StorageChange;
use crate::value_flow::ValueFlow;

/// The immutable, validated owner of every [`Call`], [`StorageChange`],
/// [`LogEvent`], and [`TokenTransfer`] in a single trace.
///
/// ## Invariants (enforced once, at construction, by [`FactArenaBuilder::build`])
/// - The call list is non-empty.
/// - Exactly one call has `parent == None` (the root), and it is at
///   index 0.
/// - Every non-root call's `parent` names a call ID that appears
///   *earlier* in the call list (no forward references).
/// - Every call's `depth` equals its parent's `depth + 1` (root is 0).
/// - Every [`StorageChange::call_id`] and [`LogEvent::call_id`] names a
///   real call in this arena.
/// - Every [`TokenTransfer::log_id`] names a real log in this arena.
///
/// Once built, none of these can be violated later, because there is no
/// API to mutate a `FactArena` after construction — every accessor
/// returns `&T` or an owned `Copy`/`Clone`, never `&mut T`.
///
/// ## Ownership
/// Owns its facts directly in `Vec`s, indexed by an ID's `.index()`. All
/// cross-fact references (a `StorageChange`'s `call_id`, etc.) are by ID,
/// never by pointer or reference, which is what makes `FactArena`
/// trivially `Send + Sync`: there is no interior mutability and no
/// shared ownership anywhere in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactArena {
    calls: Vec<Call>,
    storage_changes: Vec<StorageChange>,
    logs: Vec<LogEvent>,
    token_transfers: Vec<TokenTransfer>,
}

impl FactArena {
    /// The arena's root call — its first call, by construction (see the
    /// type-level invariants). Every arena has exactly one, so this
    /// never returns `None` for a validly-constructed `FactArena`.
    #[must_use]
    pub fn root_call(&self) -> &Call {
        // Invariant, enforced at construction: calls[0] exists and is
        // the root. Indexing (not .first()) documents that this is a
        // guaranteed-present element, not an incidental first item of a
        // possibly-empty collection.
        &self.calls[0]
    }

    /// Look up a call by ID. Returns `None` only if `id` did not
    /// originate from this same arena (IDs are not meaningful across
    /// different `FactArena` instances).
    #[must_use]
    pub fn call(&self, id: CallId) -> Option<&Call> {
        self.calls.get(id.index() as usize)
    }

    /// Look up a storage change by ID.
    #[must_use]
    pub fn storage_change(&self, id: StorageChangeId) -> Option<&StorageChange> {
        self.storage_changes.get(id.index() as usize)
    }

    /// Look up a log by ID.
    #[must_use]
    pub fn log(&self, id: LogId) -> Option<&LogEvent> {
        self.logs.get(id.index() as usize)
    }

    /// Look up a token transfer by ID.
    #[must_use]
    pub fn token_transfer(&self, id: TokenTransferId) -> Option<&TokenTransfer> {
        self.token_transfers.get(id.index() as usize)
    }

    /// Iterate every call in the arena, in arena (insertion) order —
    /// which, by the depth/parent-precedence invariant, is always a
    /// valid topological order of the call tree (every parent appears
    /// before its children).
    pub fn calls(&self) -> impl Iterator<Item = &Call> {
        self.calls.iter()
    }

    /// Iterate every storage change in the arena.
    pub fn storage_changes(&self) -> impl Iterator<Item = &StorageChange> {
        self.storage_changes.iter()
    }

    /// Iterate every storage change in the arena paired with its
    /// [`StorageChangeId`].
    ///
    /// Unlike [`Call`] and [`LogEvent`], [`StorageChange`] carries no
    /// `id` field of its own (see that type's own documentation), so
    /// without this method an external crate has no way at all to
    /// obtain a valid `StorageChangeId` to cite in a [`crate::FactRef`]
    /// — `StorageChangeId::from_index` is deliberately `pub(crate)`,
    /// per its own documented rationale, to prevent a caller from
    /// manufacturing an ID that doesn't correspond to a real fact. This
    /// method is the arena's own sanctioned way of handing out real
    /// IDs instead: `enumerate()`'s index is guaranteed to equal each
    /// change's assigned `StorageChangeId` by construction —
    /// [`crate::FactArenaBuilder::add_storage_change`] assigns IDs
    /// sequentially via [`crate::FactArenaBuilder::next_storage_change_id`]
    /// in the same order changes are pushed, so arena (vec) position and
    /// ID always agree.
    pub fn storage_changes_with_ids(
        &self,
    ) -> impl Iterator<Item = (StorageChangeId, &StorageChange)> {
        self.storage_changes
            .iter()
            .enumerate()
            .map(|(index, change)| {
                (
                    StorageChangeId::from_index(u32::try_from(index).unwrap_or(u32::MAX)),
                    change,
                )
            })
    }

    /// Iterate every log in the arena.
    pub fn logs(&self) -> impl Iterator<Item = &LogEvent> {
        self.logs.iter()
    }

    /// Iterate every token transfer in the arena.
    pub fn token_transfers(&self) -> impl Iterator<Item = &TokenTransfer> {
        self.token_transfers.iter()
    }

    /// Iterate every token transfer in the arena paired with its
    /// [`TokenTransferId`]. See [`Self::storage_changes_with_ids`] for
    /// why this method exists: [`TokenTransfer`] likewise carries no
    /// `id` field of its own, so this is the only sanctioned way for an
    /// external crate to obtain a valid `TokenTransferId`.
    pub fn token_transfers_with_ids(
        &self,
    ) -> impl Iterator<Item = (TokenTransferId, &TokenTransfer)> {
        self.token_transfers
            .iter()
            .enumerate()
            .map(|(index, transfer)| {
                (
                    TokenTransferId::from_index(u32::try_from(index).unwrap_or(u32::MAX)),
                    transfer,
                )
            })
    }

    /// The direct children of a given call, in arena order.
    ///
    /// # Complexity
    /// O(n) in the total number of calls. `FactArena` does not
    /// pre-compute a parent→children index because doing so would add a
    /// second, derived data structure that must stay consistent with
    /// `calls` — for the trace sizes this project targets (a single
    /// transaction's call tree, bounded by the EVM's own 1024 call-depth
    /// limit and realistic gas limits to at most a few thousand calls),
    /// a linear scan is not a meaningful cost, and avoiding the derived
    /// index avoids an entire class of consistency bug.
    pub fn children_of(&self, parent: CallId) -> impl Iterator<Item = &Call> {
        self.calls.iter().filter(move |c| c.parent == Some(parent))
    }

    /// Every storage change performed by a specific call (not its
    /// descendants; callers needing the full subtree can combine this
    /// with [`Self::children_of`]).
    ///
    /// # Complexity
    /// O(n) in the total number of storage changes, for the same reason
    /// as [`Self::children_of`].
    pub fn storage_changes_for_call(
        &self,
        call_id: CallId,
    ) -> impl Iterator<Item = &StorageChange> {
        self.storage_changes
            .iter()
            .filter(move |sc| sc.call_id == call_id)
    }

    /// Every non-zero [`ValueFlow`] in the arena, derived fresh from
    /// each call's `value` field (see [`ValueFlow`]'s own documentation
    /// for why this is computed rather than stored).
    pub fn value_flows(&self) -> impl Iterator<Item = ValueFlow> + '_ {
        self.calls.iter().filter_map(ValueFlow::from_call)
    }

    /// The net wei value extracted *to* a given address across the
    /// entire trace: total received minus total sent, considering only
    /// calls where that address is the direct sender or receiver.
    ///
    /// This directly supports the Architecture document's requirement
    /// (Assumption A-4, sourced from the Review's Phase 2 family-by-
    /// family analysis) to express "net value extracted." A positive
    /// result means `address` gained value overall in this trace; a
    /// negative result (represented as `None` from the underlying
    /// checked arithmetic, since callers should not silently see a
    /// wrapped/incorrect number) means the accounting overflowed —
    /// realistically only reachable with adversarially-crafted input,
    /// never a real trace, given [`crate::Wei`]'s documented bound.
    #[must_use]
    pub fn net_value_extracted(&self, address: Address) -> Option<i128> {
        let mut net: i128 = 0;
        for flow in self.value_flows() {
            if flow.to == Some(address) {
                net = net.checked_add(i128::try_from(flow.amount.0).ok()?)?;
            }
            if flow.from == address {
                net = net.checked_sub(i128::try_from(flow.amount.0).ok()?)?;
            }
        }
        Some(net)
    }
}

/// A validating constructor for [`FactArena`].
///
/// Facts are added via [`Self::add_call`], [`Self::add_storage_change`],
/// [`Self::add_log`], and [`Self::add_token_transfer`], each of which
/// hands back the `Copy` ID the fact was assigned — calls **must** be
/// added in a valid topological order (parent before child), since
/// [`CallId`] assignment is simply "next arena index" and the
/// parent-precedence invariant depends on that order matching addition
/// order. This mirrors how a real ingestion source naturally produces
/// calls (a trace is walked depth-first or breadth-first from the root,
/// never in an arbitrary order), so it is not a burdensome requirement
/// in practice — it is a burden this builder chooses not to hide behind
/// a sorting pass, because silently reordering caller-supplied calls
/// would make a bug in the caller's own tree construction invisible.
///
/// [`Self::build`] performs all arena-level validation exactly once and
/// consumes the builder, so a `FactArena` can never exist in a
/// not-yet-validated state.
#[derive(Debug, Default)]
pub struct FactArenaBuilder {
    calls: Vec<Call>,
    storage_changes: Vec<StorageChange>,
    logs: Vec<LogEvent>,
    token_transfers: Vec<TokenTransfer>,
}

impl FactArenaBuilder {
    /// Create an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The `CallId` that will be assigned to the next call added via
    /// [`Self::add_call`]. Callers use this to know a call's own ID
    /// before constructing it (a `Call`'s `id` field is set by its
    /// constructor, not inferred), and to know what ID to use as
    /// `parent` for calls added afterward.
    ///
    /// # Panics
    /// Panics if this arena already holds more than `u32::MAX` calls —
    /// unreachable for any real trace, since the EVM's own call-depth
    /// and gas limits bound a single trace to far fewer calls; a caller
    /// hitting this has a bug, not a legitimately large trace.
    #[must_use]
    pub fn next_call_id(&self) -> CallId {
        // See this method's `# Panics` section: unreachable for any real trace.
        #[allow(clippy::expect_used)]
        CallId::from_index(u32::try_from(self.calls.len()).expect(
            "more than u32::MAX calls in a single trace, which exceeds the EVM's own \
             call-depth/gas limits by many orders of magnitude and indicates a caller bug",
        ))
    }

    /// Add a call to the arena, returning its assigned ID (which must
    /// already equal `call.id`, checked by [`Self::build`] — see that
    /// method's documentation for why this check is deferred rather than
    /// performed here).
    pub fn add_call(&mut self, call: Call) -> CallId {
        let id = call.id;
        self.calls.push(call);
        id
    }

    /// The `LogId` that will be assigned to the next log added via
    /// [`Self::add_log`].
    ///
    /// # Panics
    /// Panics if this arena already holds more than `u32::MAX` logs —
    /// unreachable for any real trace; see [`Self::next_call_id`]'s
    /// `# Panics` section for the same reasoning.
    #[must_use]
    pub fn next_log_id(&self) -> LogId {
        // See this method's `# Panics` section: unreachable for any real trace.
        #[allow(clippy::expect_used)]
        LogId::from_index(
            u32::try_from(self.logs.len()).expect("more than u32::MAX logs in a single trace"),
        )
    }

    /// The `StorageChangeId` that will be assigned to the next storage
    /// change added via [`Self::add_storage_change`].
    ///
    /// # Panics
    /// Panics if this arena already holds more than `u32::MAX` storage
    /// changes — unreachable for any real trace; see
    /// [`Self::next_call_id`]'s `# Panics` section for the same
    /// reasoning.
    #[must_use]
    pub fn next_storage_change_id(&self) -> StorageChangeId {
        // See this method's `# Panics` section: unreachable for any real trace.
        #[allow(clippy::expect_used)]
        StorageChangeId::from_index(
            u32::try_from(self.storage_changes.len())
                .expect("more than u32::MAX storage changes in a single trace"),
        )
    }

    /// The `TokenTransferId` that will be assigned to the next transfer
    /// added via [`Self::add_token_transfer`].
    ///
    /// # Panics
    /// Panics if this arena already holds more than `u32::MAX` token
    /// transfers — unreachable for any real trace; see
    /// [`Self::next_call_id`]'s `# Panics` section for the same
    /// reasoning.
    #[must_use]
    pub fn next_token_transfer_id(&self) -> TokenTransferId {
        // See this method's `# Panics` section: unreachable for any real trace.
        #[allow(clippy::expect_used)]
        TokenTransferId::from_index(
            u32::try_from(self.token_transfers.len())
                .expect("more than u32::MAX token transfers in a single trace"),
        )
    }

    /// Add a storage change to the arena.
    pub fn add_storage_change(&mut self, change: StorageChange) -> StorageChangeId {
        let id = self.next_storage_change_id();
        self.storage_changes.push(change);
        id
    }

    /// Add a log to the arena, returning its assigned ID.
    pub fn add_log(&mut self, log: LogEvent) -> LogId {
        let id = log.id;
        self.logs.push(log);
        id
    }

    /// Add a token transfer to the arena.
    pub fn add_token_transfer(&mut self, transfer: TokenTransfer) -> TokenTransferId {
        let id = self.next_token_transfer_id();
        self.token_transfers.push(transfer);
        id
    }

    /// Validate every arena-level invariant and produce an immutable
    /// [`FactArena`], or the first violated invariant found.
    ///
    /// Validation proceeds call-tree-first (empty check, root
    /// uniqueness/position, parent precedence, depth consistency), then
    /// cross-references (storage changes, logs, token transfers), each
    /// in a single linear pass, so this is O(n) in total fact count, not
    /// O(n²) — important given "suitable for large traces" is a stated
    /// requirement.
    ///
    /// # Errors
    /// Returns the specific [`FactModelError`] variant describing the
    /// first invariant violation found, per this crate's error strategy
    /// (ADR-0003) of always naming which fact and which invariant, never
    /// a generic "invalid trace."
    pub fn build(self) -> Result<FactArena, FactModelError> {
        let Self {
            calls,
            storage_changes,
            logs,
            token_transfers,
        } = self;

        Self::validate_call_tree(&calls)?;

        for (index, change) in storage_changes.iter().enumerate() {
            if calls.get(change.call_id.index() as usize).is_none() {
                return Err(FactModelError::StorageChangeReferencesUnknownCall {
                    storage_change: StorageChangeId::from_index(u32::try_from(index).unwrap_or(0)),
                    call: change.call_id,
                });
            }
        }

        for (index, log) in logs.iter().enumerate() {
            if calls.get(log.call_id.index() as usize).is_none() {
                return Err(FactModelError::LogReferencesUnknownCall {
                    log: LogId::from_index(u32::try_from(index).unwrap_or(0)),
                    call: log.call_id,
                });
            }
        }

        for transfer in &token_transfers {
            if logs.get(transfer.log_id.index() as usize).is_none() {
                return Err(FactModelError::TokenTransferReferencesUnknownLog {
                    log: transfer.log_id,
                });
            }
        }

        Ok(FactArena {
            calls,
            storage_changes,
            logs,
            token_transfers,
        })
    }

    /// Validates the call-tree-shaped invariants: non-empty, exactly one
    /// root at index 0, every parent precedes its child, and depth
    /// consistency. Factored out of [`Self::build`] because it is a
    /// self-contained pass over one `Vec`, distinct from the
    /// cross-collection reference checks that follow it.
    fn validate_call_tree(calls: &[Call]) -> Result<(), FactModelError> {
        let Some(first) = calls.first() else {
            return Err(FactModelError::EmptyCallList);
        };
        if first.parent.is_some() {
            return Err(FactModelError::FirstCallIsNotRoot { call: first.id });
        }

        for call in &calls[1..] {
            if call.parent.is_none() {
                return Err(FactModelError::MultipleRootCalls {
                    first_root: first.id,
                    second_root: call.id,
                });
            }
        }

        for call in calls {
            let Some(parent_id) = call.parent else {
                continue; // root already validated above
            };
            let parent_index = parent_id.index() as usize;
            let Some(parent) = calls.get(parent_index) else {
                return Err(FactModelError::ParentNotYetSeen {
                    call: call.id,
                    parent: parent_id,
                });
            };
            // Parent precedence: the parent must appear at an earlier
            // arena index than the child (a forward reference — parent
            // index >= child index — is rejected even if the parent ID
            // technically exists somewhere in the arena).
            if parent_index >= call.id.index() as usize {
                return Err(FactModelError::ParentNotYetSeen {
                    call: call.id,
                    parent: parent_id,
                });
            }
            let expected_depth = parent.depth.0 + 1;
            if call.depth.0 != expected_depth {
                return Err(FactModelError::InconsistentCallDepth {
                    call: call.id,
                    parent: parent_id,
                    parent_depth: parent.depth.0,
                    actual: call.depth.0,
                    expected: expected_depth,
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call::CallKind;
    use crate::primitives::{CallDepth, Gas, Wei, Word};
    use crate::storage::StorageSlot;

    fn addr(byte: u8) -> Address {
        Address::new([byte; 20])
    }

    fn root_call(builder: &FactArenaBuilder) -> Call {
        Call::new(
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
        .unwrap()
    }

    fn child_call(builder: &FactArenaBuilder, parent: CallId, depth: u16) -> Call {
        Call::new(
            builder.next_call_id(),
            Some(parent),
            CallKind::Call,
            CallDepth(depth),
            addr(2),
            Some(addr(3)),
            Wei::ZERO,
            Gas(50_000),
            Gas(20_000),
            true,
        )
        .unwrap()
    }

    #[test]
    fn empty_arena_rejected() {
        let builder = FactArenaBuilder::new();
        assert_eq!(builder.build(), Err(FactModelError::EmptyCallList));
    }

    #[test]
    fn single_root_call_builds_successfully() {
        let mut builder = FactArenaBuilder::new();
        let root = root_call(&builder);
        builder.add_call(root);
        let arena = builder.build().unwrap();
        assert_eq!(arena.calls().count(), 1);
        assert!(arena.root_call().is_root());
    }

    #[test]
    fn first_call_must_be_root() {
        let mut builder = FactArenaBuilder::new();
        let not_root = Call::new(
            builder.next_call_id(),
            Some(CallId::from_index(99)),
            CallKind::Call,
            CallDepth(1),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Gas(1),
            Gas(1),
            true,
        )
        .unwrap();
        builder.add_call(not_root);
        assert!(matches!(
            builder.build(),
            Err(FactModelError::FirstCallIsNotRoot { .. })
        ));
    }

    #[test]
    fn multiple_roots_rejected() {
        let mut builder = FactArenaBuilder::new();
        let root1 = root_call(&builder);
        builder.add_call(root1);
        let root2 = Call::new(
            builder.next_call_id(),
            None,
            CallKind::Call,
            CallDepth(0),
            addr(9),
            Some(addr(9)),
            Wei::ZERO,
            Gas(1),
            Gas(1),
            true,
        )
        .unwrap();
        builder.add_call(root2);
        assert!(matches!(
            builder.build(),
            Err(FactModelError::MultipleRootCalls { .. })
        ));
    }

    #[test]
    fn forward_parent_reference_rejected() {
        let mut builder = FactArenaBuilder::new();
        // Root references a parent ID that comes "after" it (impossible
        // organically, constructed here to prove the check fires).
        let root = Call::new(
            builder.next_call_id(),
            None,
            CallKind::Call,
            CallDepth(0),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Gas(1),
            Gas(1),
            true,
        )
        .unwrap();
        let root_id = builder.add_call(root);
        // Child claims a parent ID equal to its own index (not < it).
        let bad_child = Call::new(
            CallId::from_index(1),
            Some(CallId::from_index(1)),
            CallKind::Call,
            CallDepth(1),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Gas(1),
            Gas(1),
            true,
        )
        .unwrap();
        builder.add_call(bad_child);
        let _ = root_id;
        assert!(matches!(
            builder.build(),
            Err(FactModelError::ParentNotYetSeen { .. })
        ));
    }

    #[test]
    fn inconsistent_depth_rejected() {
        let mut builder = FactArenaBuilder::new();
        let root = root_call(&builder);
        let root_id = builder.add_call(root);
        // Child's depth should be 1, but claims 5.
        let bad_child = Call::new(
            builder.next_call_id(),
            Some(root_id),
            CallKind::Call,
            CallDepth(5),
            addr(2),
            Some(addr(3)),
            Wei::ZERO,
            Gas(1),
            Gas(1),
            true,
        )
        .unwrap();
        builder.add_call(bad_child);
        assert!(matches!(
            builder.build(),
            Err(FactModelError::InconsistentCallDepth { .. })
        ));
    }

    #[test]
    fn valid_two_level_tree_builds() {
        let mut builder = FactArenaBuilder::new();
        let root = root_call(&builder);
        let root_id = builder.add_call(root);
        let child = child_call(&builder, root_id, 1);
        let child_id = builder.add_call(child);
        let grandchild = child_call(&builder, child_id, 2);
        builder.add_call(grandchild);
        let arena = builder.build().unwrap();
        assert_eq!(arena.calls().count(), 3);
        assert_eq!(arena.children_of(root_id).count(), 1);
    }

    #[test]
    fn storage_change_referencing_unknown_call_rejected() {
        let mut builder = FactArenaBuilder::new();
        let root = root_call(&builder);
        builder.add_call(root);
        builder.add_storage_change(StorageChange::new(
            CallId::from_index(99),
            StorageSlot::new(addr(1), Word::ZERO),
            Word::ZERO,
            Word::new([1; 32]),
        ));
        assert!(matches!(
            builder.build(),
            Err(FactModelError::StorageChangeReferencesUnknownCall { .. })
        ));
    }

    #[test]
    fn log_referencing_unknown_call_rejected() {
        let mut builder = FactArenaBuilder::new();
        let root = root_call(&builder);
        builder.add_call(root);
        builder.add_log(
            LogEvent::new(
                builder.next_log_id(),
                CallId::from_index(99),
                addr(1),
                vec![],
                vec![],
                crate::primitives::LogIndex(0),
            )
            .unwrap(),
        );
        assert!(matches!(
            builder.build(),
            Err(FactModelError::LogReferencesUnknownCall { .. })
        ));
    }

    #[test]
    fn token_transfer_referencing_unknown_log_rejected() {
        let mut builder = FactArenaBuilder::new();
        let root = root_call(&builder);
        builder.add_call(root);
        builder.add_token_transfer(TokenTransfer::new(
            LogId::from_index(99),
            addr(1),
            addr(2),
            addr(3),
            Wei(1),
        ));
        assert!(matches!(
            builder.build(),
            Err(FactModelError::TokenTransferReferencesUnknownLog { .. })
        ));
    }

    #[test]
    fn net_value_extracted_accounts_gains_and_losses() {
        let mut builder = FactArenaBuilder::new();
        let root = root_call(&builder);
        let root_id = builder.add_call(root);
        // A call sending 100 wei from addr(1) to addr(9).
        let gain = Call::new(
            builder.next_call_id(),
            Some(root_id),
            CallKind::Call,
            CallDepth(1),
            addr(1),
            Some(addr(9)),
            Wei(100),
            Gas(1),
            Gas(1),
            true,
        )
        .unwrap();
        builder.add_call(gain);
        // A call sending 30 wei from addr(9) onward.
        let loss = Call::new(
            builder.next_call_id(),
            Some(root_id),
            CallKind::Call,
            CallDepth(1),
            addr(9),
            Some(addr(2)),
            Wei(30),
            Gas(1),
            Gas(1),
            true,
        )
        .unwrap();
        builder.add_call(loss);
        let arena = builder.build().unwrap();
        assert_eq!(arena.net_value_extracted(addr(9)), Some(70));
    }

    #[test]
    fn value_flows_skips_zero_value_calls() {
        let mut builder = FactArenaBuilder::new();
        let root = root_call(&builder); // zero value
        builder.add_call(root);
        let arena = builder.build().unwrap();
        assert_eq!(arena.value_flows().count(), 0);
    }

    #[test]
    fn storage_changes_with_ids_match_arena_position() {
        let mut builder = FactArenaBuilder::new();
        let root = root_call(&builder);
        let root_id = builder.add_call(root);
        let change_a = StorageChange::new(
            root_id,
            StorageSlot::new(addr(1), Word::ZERO),
            Word::ZERO,
            Word::new([1; 32]),
        );
        let change_b = StorageChange::new(
            root_id,
            StorageSlot::new(addr(1), Word::new([1; 32])),
            Word::ZERO,
            Word::new([2; 32]),
        );
        builder.add_storage_change(change_a);
        builder.add_storage_change(change_b);
        let arena = builder.build().unwrap();

        let pairs: Vec<_> = arena.storage_changes_with_ids().collect();
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].0.index(), 0);
        assert_eq!(pairs[1].0.index(), 1);
        // Each returned ID must actually resolve back to the same
        // change via the arena's own by-ID lookup.
        for (id, change) in pairs {
            assert_eq!(arena.storage_change(id), Some(change));
        }
    }

    #[test]
    fn token_transfers_with_ids_match_arena_position() {
        let mut builder = FactArenaBuilder::new();
        let root = root_call(&builder);
        builder.add_call(root);
        builder.add_log(
            LogEvent::new(
                builder.next_log_id(),
                CallId::from_index(0),
                addr(5),
                vec![],
                vec![],
                crate::primitives::LogIndex(0),
            )
            .unwrap(),
        );
        builder.add_token_transfer(TokenTransfer::new(
            LogId::from_index(0),
            addr(5),
            addr(1),
            addr(2),
            Wei(10),
        ));
        let arena = builder.build().unwrap();

        let pairs: Vec<_> = arena.token_transfers_with_ids().collect();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0.index(), 0);
        assert_eq!(arena.token_transfer(pairs[0].0), Some(pairs[0].1));
    }
}

//! [`TraceIndex`]: precomputed, reusable structure over a single
//! [`Trace`], built once and shared across every pattern matched against
//! that trace (see this crate's top-level docs, "Internal indexes," for
//! the full rationale and complexity accounting).

use std::collections::{HashMap, HashSet};

use fact_model::{Address, Call, CallId, Trace};

/// Everything this crate precomputes about a [`Trace`] once, rather than
/// recomputing per pattern or per predicate evaluation.
///
/// # What is cached and why
/// - [`Self::is_reentrant`]: whether a call's target address already
///   appears among its own strict ancestors' targets — the structural
///   definition of "reentrant" this crate uses (see the crate-level
///   docs' "Predicate semantics" section). Computing this for one call
///   requires walking that call's entire ancestor chain; computing it
///   for *every* call independently would be O(calls × average depth)
///   repeated on every `reentrant: true/false` predicate evaluation.
///   Precomputing it once, alongside the equivalent-cost full-arena
///   walk building each call's ancestor-target set, amortizes that to a
///   single O(calls × average depth) pass regardless of how many
///   patterns (each potentially with several `call(reentrant: ...)`
///   evidence clauses) are subsequently matched.
/// - [`Self::call`]: `O(1)` call lookup by [`CallId`], reused by every
///   ancestor walk and every ordering computation instead of calling
///   [`fact_model::FactArena::call`] (itself already `O(1)`, a direct
///   `Vec` index — this cache exists for locality/ergonomics inside
///   this crate's own algorithms, not to fix an `O(n)` lookup that
///   didn't exist).
pub struct TraceIndex<'trace> {
    trace: &'trace Trace,
    /// `CallId → &Call`, mirroring `FactArena::call` but borrowed once
    /// up front for convenient reuse across this module's algorithms.
    calls_by_id: HashMap<CallId, &'trace Call>,
    /// Every [`CallId`] whose `to` address already appears among its own
    /// strict ancestors' `to` addresses — see this type's own docs.
    reentrant_calls: HashSet<CallId>,
}

impl<'trace> TraceIndex<'trace> {
    /// Build every precomputed structure for `trace`. `O(calls ×
    /// average call depth)` — see this type's own docs for why that
    /// bound is acceptable (bounded by the EVM's own 1024-deep call
    /// limit) and where the cost goes.
    #[must_use]
    pub fn build(trace: &'trace Trace) -> Self {
        let calls_by_id: HashMap<CallId, &Call> =
            trace.arena.calls().map(|call| (call.id, call)).collect();

        let mut reentrant_calls = HashSet::new();
        // `trace.arena.calls()` is a valid topological order (every
        // parent before its children — see `FactArena::calls`'s own
        // docs), so a single forward pass can compute each call's
        // ancestor-target set from its (already-computed) parent's set
        // without a second traversal.
        let mut ancestor_targets: HashMap<CallId, HashSet<Address>> = HashMap::new();
        for call in trace.arena.calls() {
            let mut this_call_targets = call
                .parent
                .and_then(|parent_id| ancestor_targets.get(&parent_id))
                .cloned()
                .unwrap_or_default();

            // Insert the parent's own target *before* checking `call`
            // for reentrancy: `this_call_targets` must equal the full
            // set of every *strict* ancestor's target (root through
            // `call`'s direct parent, inclusive) at the point of the
            // check below, or a call that re-enters its own immediate
            // parent's target (the most common real shape) would be
            // missed — `ancestor_targets[parent]` alone only carries
            // targets from *above* the parent, not the parent's own.
            if let Some(parent) = call.parent.and_then(|p| calls_by_id.get(&p)) {
                if let Some(parent_to) = parent.to {
                    this_call_targets.insert(parent_to);
                }
            }

            if let Some(to) = call.to {
                if this_call_targets.contains(&to) {
                    reentrant_calls.insert(call.id);
                }
            }

            ancestor_targets.insert(call.id, this_call_targets);
        }

        Self {
            trace,
            calls_by_id,
            reentrant_calls,
        }
    }

    /// The trace this index was built for.
    #[must_use]
    pub const fn trace(&self) -> &'trace Trace {
        self.trace
    }

    /// `O(1)` call lookup by ID.
    #[must_use]
    pub fn call(&self, id: CallId) -> Option<&'trace Call> {
        self.calls_by_id.get(&id).copied()
    }

    /// Whether `id` names a structurally reentrant call: its target
    /// address already appears among its own strict ancestors' target
    /// addresses (i.e. this call re-enters a contract already active on
    /// the call stack).
    ///
    /// # Modeling assumption
    /// This is a structural approximation, not a semantic one: it flags
    /// *any* repeated target address on the call stack, regardless of
    /// whether the reentered contract's state was actually observably
    /// inconsistent at that point (the hallmark of an exploitable
    /// reentrancy, as opposed to an intentional, safe re-entry pattern).
    /// Distinguishing those requires reasoning this crate's "structural
    /// matching only, no grounding" scope explicitly excludes (see the
    /// crate-level docs) — a `call(reentrant: true)` candidate binding
    /// is exactly that, a candidate, for the grounding crate to confirm
    /// or reject with the storage/state evidence this crate does not
    /// interpret.
    #[must_use]
    pub fn is_reentrant(&self, id: CallId) -> bool {
        self.reentrant_calls.contains(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fact_model::{
        BlockContext, BlockNumber, CallDepth, ChainId, FactArenaBuilder, Gas, Nonce, Timestamp,
        TraceMetadata, TraceSource, Transaction, TxStatus, Wei, Word,
    };

    fn addr(byte: u8) -> Address {
        Address::new([byte; 20])
    }

    fn hash(byte: u8) -> fact_model::TxHash {
        fact_model::TxHash(Word::new([byte; 32]))
    }

    fn make_call(
        builder: &FactArenaBuilder,
        parent: Option<CallId>,
        depth: u16,
        from: Address,
        to: Address,
    ) -> Call {
        Call::new(
            builder.next_call_id(),
            parent,
            fact_model::CallKind::Call,
            CallDepth(depth),
            from,
            Some(to),
            Wei::ZERO,
            Gas(100_000),
            Gas(50_000),
            true,
        )
        .unwrap()
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

    /// root(A->B) -> child(B->C) -> grandchild(C->B): the grandchild
    /// re-enters B, which is already an ancestor target (the root's
    /// call targeted B). The child does not re-enter anything.
    #[test]
    fn detects_reentrant_call_two_levels_up() {
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, addr(1), addr(2)); // A -> B
        let root_id = builder.add_call(root);
        let child = make_call(&builder, Some(root_id), 1, addr(2), addr(3)); // B -> C
        let child_id = builder.add_call(child);
        let grandchild = make_call(&builder, Some(child_id), 2, addr(3), addr(2)); // C -> B (reentrant!)
        let grandchild_id = builder.add_call(grandchild);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena);

        let index = TraceIndex::build(&trace);
        assert!(!index.is_reentrant(root_id));
        assert!(!index.is_reentrant(child_id));
        assert!(index.is_reentrant(grandchild_id));
    }

    /// Regression test for a real bug found during implementation: the
    /// ancestor-target set must include the *immediate parent's* own
    /// target before checking the current call, not only targets from
    /// higher ancestors — otherwise the most common real reentrancy
    /// shape (a call whose direct parent already targeted the same
    /// address) goes undetected. root(A->B) -> child(B->B): child
    /// directly re-enters its own parent's target.
    #[test]
    fn detects_reentrant_call_into_immediate_parent_target() {
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, addr(1), addr(2)); // A -> B
        let root_id = builder.add_call(root);
        let child = make_call(&builder, Some(root_id), 1, addr(2), addr(2)); // B -> B (reentrant!)
        let child_id = builder.add_call(child);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena);

        let index = TraceIndex::build(&trace);
        assert!(!index.is_reentrant(root_id));
        assert!(index.is_reentrant(child_id));
    }

    #[test]
    fn non_reentrant_tree_has_no_reentrant_calls() {
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, addr(1), addr(2));
        let root_id = builder.add_call(root);
        let child = make_call(&builder, Some(root_id), 1, addr(2), addr(3));
        builder.add_call(child);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena);

        let index = TraceIndex::build(&trace);
        assert!(!index.is_reentrant(root_id));
        assert!(!index.is_reentrant(child_id_unused(&trace)));
    }

    fn child_id_unused(trace: &Trace) -> CallId {
        trace.arena.calls().nth(1).unwrap().id
    }

    #[test]
    fn call_lookup_matches_arena() {
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, addr(1), addr(2));
        let root_id = builder.add_call(root);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena);

        let index = TraceIndex::build(&trace);
        assert_eq!(index.call(root_id), trace.arena.call(root_id));
    }
}

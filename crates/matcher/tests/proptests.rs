//! Property tests: generated random call trees and evidence-order
//! scenarios, checked against invariants that must hold for *any* valid
//! input, complementing the fixed-example tests elsewhere in this crate.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use proptest::prelude::*;

use fact_model::{
    Address, BlockContext, BlockNumber, Call, CallDepth, CallId, CallKind, ChainId,
    FactArenaBuilder, Gas, Nonce, Timestamp, Trace, TraceMetadata, TraceSource, Transaction,
    TxHash, TxStatus, Wei,
};

fn addr(byte: u8) -> Address {
    Address::new([byte; 20])
}

fn hash(byte: u8) -> TxHash {
    TxHash(fact_model::Word::new([byte; 32]))
}

/// Build a random call tree: `n` calls, each (after the first) attached
/// to a uniformly-random earlier call as its parent, with a `to` address
/// drawn from a small pool (`address_pool_size` distinct addresses) so
/// repeated targets — and therefore reentrancy — are common enough to
/// exercise both the `true` and `false` cases.
fn arbitrary_call_tree(
    n: usize,
    address_pool_size: u8,
) -> impl Strategy<Value = (Vec<Option<usize>>, Vec<u8>)> {
    let raw_parents = prop::collection::vec(any::<u32>(), n.saturating_sub(1));
    let targets = prop::collection::vec(0..address_pool_size, n);
    (raw_parents, targets).prop_map(move |(raw_parents, targets)| {
        let mut parents = vec![None];
        for (offset, raw) in raw_parents.into_iter().enumerate() {
            let this_index = offset + 1; // this call's own index (starts at 1)
            let choice = (raw as usize) % this_index; // any strictly earlier index
            parents.push(Some(choice));
        }
        (parents, targets)
    })
}

fn build_trace(parents: &[Option<usize>], targets: &[u8]) -> Trace {
    let attacker = addr(1);
    let mut builder = FactArenaBuilder::new();
    let mut ids: Vec<CallId> = Vec::with_capacity(parents.len());

    for (i, parent_idx) in parents.iter().enumerate() {
        let parent_id = parent_idx.map(|p| ids[p]);
        let depth = parent_idx.map_or(0, |p| {
            // Depth must equal parent's depth + 1; since parents are
            // always an earlier index, this recursion terminates.
            depth_of(parents, p) + 1
        });
        let from = parent_idx.map_or(attacker, |p| addr(targets[p]));
        let call = Call::new(
            builder.next_call_id(),
            parent_id,
            CallKind::Call,
            CallDepth(depth),
            from,
            Some(addr(targets[i])),
            Wei::ZERO,
            Gas(1),
            Gas(1),
            true,
        )
        .unwrap();
        ids.push(builder.add_call(call));
    }

    let arena = builder.build().unwrap();
    let tx = Transaction::new(
        hash(1),
        attacker,
        Some(addr(2)),
        Wei::ZERO,
        Nonce(0),
        Gas(1),
        TxStatus::Success,
    );
    let block = BlockContext::new(BlockNumber(1), Timestamp(1), ChainId(1), None);
    let metadata = TraceMetadata::new(
        hash(1),
        ChainId(1),
        BlockNumber(1),
        TraceSource::ArchiveNodeRpc {
            endpoint_label: "proptest".to_string(),
        },
    );
    Trace::new(metadata, block, tx, arena).unwrap()
}

fn depth_of(parents: &[Option<usize>], index: usize) -> u16 {
    parents[index].map_or(0, |p| depth_of(parents, p) + 1)
}

/// Reference (naive, O(depth) per call, no caching) reentrancy check:
/// walk strict ancestors directly and compare targets, independent of
/// `TraceIndex`'s own (differently-structured) implementation. Used to
/// cross-check `TraceIndex::is_reentrant` rather than duplicating its
/// logic.
fn naive_is_reentrant(trace: &Trace, call_id: CallId) -> bool {
    let call = trace.arena.call(call_id).unwrap();
    let Some(target) = call.to else {
        return false;
    };
    let mut current = call.parent;
    while let Some(id) = current {
        let ancestor = trace.arena.call(id).unwrap();
        if ancestor.to == Some(target) {
            return true;
        }
        current = ancestor.parent;
    }
    false
}

proptest! {
    /// `TraceIndex::is_reentrant` must agree with the naive, direct
    /// ancestor walk for every call in a randomly generated tree.
    #[test]
    fn reentrancy_matches_naive_reference(
        (parents, targets) in arbitrary_call_tree(30, 4)
    ) {
        let trace = build_trace(&parents, &targets);
        let index = matcher::TraceIndex::build(&trace);
        for call in trace.arena.calls() {
            let expected = naive_is_reentrant(&trace, call.id);
            prop_assert_eq!(
                index.is_reentrant(call.id),
                expected,
                "mismatch for call {:?}",
                call.id
            );
        }
    }

    /// Matching the same pattern against the same trace twice must
    /// produce byte-for-byte identical results — this crate's core
    /// determinism requirement.
    #[test]
    fn matching_is_deterministic(
        (parents, targets) in arbitrary_call_tree(20, 3)
    ) {
        let trace = build_trace(&parents, &targets);
        let src = r"
            pattern p version 1 {
                family: Reentrancy
                severity: Low
                evidence { required a: call(kind: External, reentrant: true) }
            }
        ";
        let pattern = dsl::compile_str(src).unwrap();
        let first = matcher::find_candidate_matches(&trace, &pattern).unwrap();
        let second = matcher::find_candidate_matches(&trace, &pattern).unwrap();
        prop_assert_eq!(first, second);
    }

    /// The number of candidates for a single-required-evidence,
    /// no-sequence pattern must equal exactly the number of facts
    /// satisfying that evidence's predicate — no more (no
    /// over-generation), no fewer (no dropped occurrences).
    #[test]
    fn candidate_count_matches_predicate_match_count(
        (parents, targets) in arbitrary_call_tree(25, 5)
    ) {
        let trace = build_trace(&parents, &targets);
        let src = r"
            pattern p version 1 {
                family: Reentrancy
                severity: Low
                evidence { required a: call(kind: External, reentrant: true) }
            }
        ";
        let pattern = dsl::compile_str(src).unwrap();
        let index = matcher::TraceIndex::build(&trace);
        let expected = trace.arena.calls().filter(|c| index.is_reentrant(c.id)).count();

        let candidates = matcher::find_candidate_matches(&trace, &pattern).unwrap();
        prop_assert_eq!(candidates.len(), expected);
    }
}

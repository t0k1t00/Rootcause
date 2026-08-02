//! Large-trace test: builds a trace near the EVM's own structural
//! bounds (deep call chain, many storage writes) and checks matching
//! still completes correctly and promptly — a regression guard against
//! accidentally-quadratic behavior creeping into `matcher`'s linear-scan
//! design (see the crate's top-level docs, "Complexity").
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use std::time::{Duration, Instant};

use fact_model::{
    Address, BlockContext, BlockNumber, Call, CallDepth, CallKind, ChainId, FactArenaBuilder, Gas,
    Nonce, StorageChange, StorageSlot, Timestamp, Trace, TraceMetadata, TraceSource, Transaction,
    TxHash, TxStatus, Wei, Word,
};

fn addr(byte: u8) -> Address {
    Address::new([byte; 20])
}

fn hash(byte: u8) -> TxHash {
    TxHash(Word::new([byte; 32]))
}

/// Build a `depth`-deep chain of calls (each call's sole child is the
/// next), alternating between two addresses so the deepest call
/// re-enters the very first address — a single reentrant occurrence
/// buried at the bottom of a realistically deep call stack (EVM caps
/// call depth at 1024 — `depth` here stays under that).
fn build_deep_chain_trace(depth: u16) -> Trace {
    let attacker = addr(1);
    let addr_a = addr(2);
    let addr_b = addr(3);

    let mut builder = FactArenaBuilder::new();
    let root = Call::new(
        builder.next_call_id(),
        None,
        CallKind::Call,
        CallDepth(0),
        attacker,
        Some(addr_a),
        Wei::ZERO,
        Gas(1),
        Gas(1),
        true,
    )
    .unwrap();
    let mut parent_id = builder.add_call(root);
    let mut current_to = addr_a;

    for d in 1..depth {
        let next_to = if d % 2 == 0 { addr_a } else { addr_b };
        let call = Call::new(
            builder.next_call_id(),
            Some(parent_id),
            CallKind::Call,
            CallDepth(d),
            current_to,
            Some(next_to),
            Wei::ZERO,
            Gas(1),
            Gas(1),
            true,
        )
        .unwrap();
        parent_id = builder.add_call(call);
        current_to = next_to;

        // One storage write per call, so the trace also carries a
        // realistic number of non-call facts.
        builder.add_storage_change(StorageChange::new(
            parent_id,
            StorageSlot::new(
                current_to,
                Word::new([u8::try_from(d % 256).unwrap_or(0); 32]),
            ),
            Word::ZERO,
            Word::new([1; 32]),
        ));
    }

    let arena = builder.build().unwrap();
    let tx = Transaction::new(
        hash(1),
        attacker,
        Some(addr_a),
        Wei::ZERO,
        Nonce(0),
        Gas(30_000_000),
        TxStatus::Success,
    );
    let block = BlockContext::new(BlockNumber(1), Timestamp(1), ChainId(1), None);
    let metadata = TraceMetadata::new(
        hash(1),
        ChainId(1),
        BlockNumber(1),
        TraceSource::ArchiveNodeRpc {
            endpoint_label: "large-trace-test".to_string(),
        },
    );
    Trace::new(metadata, block, tx, arena).unwrap()
}

const REENTRANCY_PATTERN: &str = r"
    pattern classic_reentrancy version 1 {
        family: Reentrancy
        severity: Critical
        evidence {
            required vulnerable_call: call(kind: External, reentrant: true)
            required state_write: storage(changed: true)
        }
        constraint: AND(vulnerable_call, state_write)
    }
";

#[test]
fn deep_call_chain_matches_correctly_and_quickly() {
    // 1000 calls: near the EVM's 1024 call-depth limit, well beyond any
    // realistic pattern's evidence count, exercising `TraceIndex::build`
    // and every predicate's linear scan at real scale.
    let trace = build_deep_chain_trace(1000);
    assert_eq!(trace.arena.calls().count(), 1000);
    assert_eq!(trace.arena.storage_changes().count(), 999);

    let pattern = dsl::compile_str(REENTRANCY_PATTERN).expect("pattern should compile");

    let start = Instant::now();
    let candidates =
        matcher::find_candidate_matches(&trace, &pattern).expect("matching should succeed");
    let elapsed = start.elapsed();

    // Correctness: the alternating a/b/a/b... chain revisits `addr_a`
    // starting at depth 2 and every even depth after, each such call
    // structurally reentrant. Every one of them has a storage write
    // during its own call, satisfying `state_write` too.
    assert!(
        !candidates.is_empty(),
        "expected at least one reentrant match in the deep chain"
    );

    // Performance: this is a regression guard, not a strict benchmark —
    // a linear-scan design over 1000 calls should comfortably finish in
    // well under a second on any real machine. A generous bound avoids
    // flaking on slow CI runners while still catching an accidental
    // quadratic-or-worse regression (which would take vastly longer at
    // this size).
    assert!(
        elapsed < Duration::from_secs(5),
        "matching 1000 calls took {elapsed:?}, expected well under 5s"
    );
}

#[test]
fn many_independent_reentrant_occurrences_all_found() {
    // A wide (not deep) trace: 200 independent, sibling reentrant call
    // pairs, each hung directly off the root. Exercises the "one
    // candidate per real occurrence" property at scale.
    let attacker = addr(1);
    let mut builder = FactArenaBuilder::new();
    let root = Call::new(
        builder.next_call_id(),
        None,
        CallKind::Call,
        CallDepth(0),
        attacker,
        Some(addr(2)),
        Wei::ZERO,
        Gas(1),
        Gas(1),
        true,
    )
    .unwrap();
    let root_id = builder.add_call(root);

    let occurrences = 200u16;
    for i in 0..occurrences {
        let mut victim_bytes = [0u8; 20];
        // Encode `i` across the last two bytes, offset well clear of
        // the single-byte addresses used elsewhere in this test
        // (`addr(1)`, `addr(2)`), so no victim address can ever
        // collide with the attacker or root target.
        victim_bytes[18] = (i >> 8) as u8;
        victim_bytes[19] = (i & 0xff) as u8;
        victim_bytes[0] = 0xff;
        let victim = Address::new(victim_bytes);
        let entry = Call::new(
            builder.next_call_id(),
            Some(root_id),
            CallKind::Call,
            CallDepth(1),
            attacker,
            Some(victim),
            Wei::ZERO,
            Gas(1),
            Gas(1),
            true,
        )
        .unwrap();
        let entry_id = builder.add_call(entry);
        let reenter = Call::new(
            builder.next_call_id(),
            Some(entry_id),
            CallKind::Call,
            CallDepth(2),
            victim,
            Some(victim),
            Wei::ZERO,
            Gas(1),
            Gas(1),
            true,
        )
        .unwrap();
        let reenter_id = builder.add_call(reenter);
        builder.add_storage_change(StorageChange::new(
            reenter_id,
            StorageSlot::new(victim, Word::ZERO),
            Word::ZERO,
            Word::new([1; 32]),
        ));
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
            endpoint_label: "wide-trace-test".to_string(),
        },
    );
    let trace = Trace::new(metadata, block, tx, arena).unwrap();

    let pattern = dsl::compile_str(REENTRANCY_PATTERN).unwrap();
    let candidates = matcher::find_candidate_matches(&trace, &pattern).unwrap();
    assert_eq!(candidates.len(), usize::from(occurrences));
}

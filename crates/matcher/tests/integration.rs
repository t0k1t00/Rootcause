//! Integration tests exercising `matcher`'s public API end to end:
//! `dsl::compile_str` a realistic pattern, build a realistic
//! [`fact_model::Trace`], and check [`matcher::find_candidate_matches`]
//! against it — no direct access to this crate's internal modules.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use fact_model::{
    Address, BlockContext, BlockNumber, Call, CallDepth, CallId, CallKind, ChainId,
    FactArenaBuilder, Gas, LogEvent, LogIndex, Nonce, StorageChange, StorageSlot, Timestamp,
    TokenTransfer, Trace, TraceMetadata, TraceSource, Transaction, TxHash, TxStatus, Wei, Word,
};

fn addr(byte: u8) -> Address {
    Address::new([byte; 20])
}

fn hash(byte: u8) -> TxHash {
    TxHash(Word::new([byte; 32]))
}

fn make_call(
    builder: &FactArenaBuilder,
    parent: Option<CallId>,
    depth: u16,
    kind: CallKind,
    from: Address,
    to: Address,
    value: u128,
) -> Call {
    Call::new(
        builder.next_call_id(),
        parent,
        kind,
        CallDepth(depth),
        from,
        Some(to),
        Wei(value),
        Gas(200_000),
        Gas(100_000),
        true,
    )
    .unwrap()
}

fn build_trace(arena: fact_model::FactArena, from: Address, status: TxStatus) -> Trace {
    let tx = Transaction::new(
        hash(1),
        from,
        Some(addr(2)),
        Wei::ZERO,
        Nonce(0),
        Gas(500_000),
        status,
    );
    let block = BlockContext::new(
        BlockNumber(19_000_000),
        Timestamp(1_700_000_000),
        ChainId(1),
        None,
    );
    let metadata = TraceMetadata::new(
        hash(1),
        ChainId(1),
        BlockNumber(19_000_000),
        TraceSource::ArchiveNodeRpc {
            endpoint_label: "integration-test".to_string(),
        },
    );
    Trace::new(metadata, block, tx, arena).unwrap()
}

/// A donation/oracle-manipulation-shaped pattern: an unexpected inbound
/// token transfer followed by a changed price-oracle-role storage slot,
/// in that order, with an optional flash-loan call this trace does not
/// provide (so the pattern must still match without it).
const DONATION_PATTERN: &str = r#"
    pattern donation_attack version 1 {
        family: OracleManipulation
        severity: High
        tags: ["oracle", "donation"]
        references: ["SCWE-042"]

        evidence {
            required donation_transfer: token_transfer(direction: In, unexpected: true)
            required price_read: storage(changed: true, role: PriceOracle)
            optional attacker_flash_loan: call(kind: Delegate)
        }

        constraint: AND(donation_transfer, price_read, NOT(attacker_flash_loan))
        sequence: [donation_transfer, price_read]
    }
"#;

#[test]
fn donation_attack_shaped_trace_matches() {
    let attacker = addr(1);
    let victim = addr(9);
    let token = addr(0xaa);

    let mut builder = FactArenaBuilder::new();
    let root = make_call(&builder, None, 0, CallKind::Call, attacker, victim, 0);
    let root_id = builder.add_call(root);

    // The donation: an inbound token transfer to the attacker.
    let log_id = builder.next_log_id();
    builder.add_log(LogEvent::new(log_id, root_id, token, vec![], vec![], LogIndex(0)).unwrap());
    builder.add_token_transfer(TokenTransfer::new(
        log_id,
        token,
        victim,
        attacker,
        Wei(1_000_000),
    ));

    // The oracle read: a storage change on the victim contract.
    builder.add_storage_change(StorageChange::new(
        root_id,
        StorageSlot::new(victim, Word::ZERO),
        Word::ZERO,
        Word::new([7; 32]),
    ));

    let arena = builder.build().unwrap();
    let trace = build_trace(arena, attacker, TxStatus::Success);

    let pattern = dsl::compile_str(DONATION_PATTERN).expect("pattern should compile");
    let candidates =
        matcher::find_candidate_matches(&trace, &pattern).expect("matching should succeed");

    assert_eq!(candidates.len(), 1);
    let candidate = &candidates[0];
    assert_eq!(candidate.pattern_id.0, "donation_attack");
    // donation_transfer + price_read bound; attacker_flash_loan absent
    // (correctly negated, no binding); the pattern matches without it.
    assert_eq!(candidate.bindings.len(), 2);
    assert!(candidate.metadata.sequence_checked);
    // `unexpected` and `role` are both structurally unverifiable.
    assert_eq!(candidate.metadata.unresolved_attributes.len(), 2);
}

#[test]
fn donation_pattern_does_not_match_when_flash_loan_present() {
    let attacker = addr(1);
    let victim = addr(9);
    let token = addr(0xaa);

    let mut builder = FactArenaBuilder::new();
    let root = make_call(&builder, None, 0, CallKind::Call, attacker, victim, 0);
    let root_id = builder.add_call(root);

    let log_id = builder.next_log_id();
    builder.add_log(LogEvent::new(log_id, root_id, token, vec![], vec![], LogIndex(0)).unwrap());
    builder.add_token_transfer(TokenTransfer::new(
        log_id,
        token,
        victim,
        attacker,
        Wei(1_000_000),
    ));

    builder.add_storage_change(StorageChange::new(
        root_id,
        StorageSlot::new(victim, Word::ZERO),
        Word::ZERO,
        Word::new([7; 32]),
    ));

    // The forbidden flash-loan delegate call is now present.
    let flash_loan = make_call(
        &builder,
        Some(root_id),
        1,
        CallKind::DelegateCall,
        attacker,
        addr(50),
        0,
    );
    builder.add_call(flash_loan);

    let arena = builder.build().unwrap();
    let trace = build_trace(arena, attacker, TxStatus::Success);

    let pattern = dsl::compile_str(DONATION_PATTERN).unwrap();
    let candidates = matcher::find_candidate_matches(&trace, &pattern).unwrap();
    assert!(candidates.is_empty());
}

#[test]
fn donation_pattern_does_not_match_out_of_order_evidence() {
    // Same facts as the matching case, but the storage change happens
    // during an *earlier* call than the token transfer, violating the
    // declared `sequence: [donation_transfer, price_read]` order.
    let attacker = addr(1);
    let victim = addr(9);
    let token = addr(0xaa);

    let mut builder = FactArenaBuilder::new();
    let root = make_call(&builder, None, 0, CallKind::Call, attacker, victim, 0);
    let root_id = builder.add_call(root);
    let later = make_call(
        &builder,
        Some(root_id),
        1,
        CallKind::Call,
        victim,
        addr(3),
        0,
    );
    let later_id = builder.add_call(later);

    // Storage change during the *earlier* root call...
    builder.add_storage_change(StorageChange::new(
        root_id,
        StorageSlot::new(victim, Word::ZERO),
        Word::ZERO,
        Word::new([7; 32]),
    ));
    // ...but the token transfer happens during the *later* call.
    let log_id = builder.next_log_id();
    builder.add_log(LogEvent::new(log_id, later_id, token, vec![], vec![], LogIndex(0)).unwrap());
    builder.add_token_transfer(TokenTransfer::new(
        log_id,
        token,
        victim,
        attacker,
        Wei(1_000_000),
    ));

    let arena = builder.build().unwrap();
    let trace = build_trace(arena, attacker, TxStatus::Success);

    let pattern = dsl::compile_str(DONATION_PATTERN).unwrap();
    let candidates = matcher::find_candidate_matches(&trace, &pattern).unwrap();
    assert!(candidates.is_empty());
}

const TRANSACTION_STATUS_PATTERN: &str = r"
    pattern reverted_high_gas version 1 {
        family: Other
        severity: Low
        evidence {
            required reverted: transaction(status: Reverted, gas_used_min: 100000)
        }
    }
";

#[test]
fn transaction_predicate_matches_whole_trace_property() {
    let attacker = addr(1);
    let mut builder = FactArenaBuilder::new();
    let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
    builder.add_call(root);
    let arena = builder.build().unwrap();
    let trace = build_trace(arena, attacker, TxStatus::Reverted);

    let pattern = dsl::compile_str(TRANSACTION_STATUS_PATTERN).unwrap();
    let candidates = matcher::find_candidate_matches(&trace, &pattern).unwrap();
    assert_eq!(candidates.len(), 1);
}

#[test]
fn match_engine_reuses_index_across_multiple_patterns() {
    let attacker = addr(1);
    let mut builder = FactArenaBuilder::new();
    let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
    builder.add_call(root);
    let arena = builder.build().unwrap();
    let trace = build_trace(arena, attacker, TxStatus::Success);

    let pattern_a = dsl::compile_str(TRANSACTION_STATUS_PATTERN).unwrap();
    let pattern_b_src = r"
        pattern always_true version 1 {
            family: Other
            severity: Low
            evidence { required tx: transaction(status: Success) }
        }
    ";
    let pattern_b = dsl::compile_str(pattern_b_src).unwrap();

    let engine = matcher::MatchEngine::new(&trace);
    let matches_a = engine.find_matches(&pattern_a).unwrap();
    let matches_b = engine.find_matches(&pattern_b).unwrap();
    assert!(matches_a.is_empty()); // trace succeeded, pattern wants Reverted
    assert_eq!(matches_b.len(), 1);

    let all = engine.find_all_matches(&[pattern_a, pattern_b]).unwrap();
    assert_eq!(all.len(), 1);
}

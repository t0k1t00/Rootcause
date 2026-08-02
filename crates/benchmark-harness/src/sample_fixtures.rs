//! Canonical, realistic end-to-end fixtures: four small traces plus
//! matching patterns exercising each of the outcomes this crate's
//! metrics distinguish (see [`crate::types::PatternOutcome`]).
//!
//! These are built directly via `fact-model`'s arena builder rather
//! than as raw ingestible JSON — the same approach `matcher`'s and
//! `grounding`'s own test suites use — so this module has no on-disk
//! dependency and is exercised by both this crate's own tests and, via
//! [`BenchmarkCase::from_trace`], anyone downstream wanting a quick,
//! realistic example without writing a fixture file.
//!
//! Every `.expect(...)` in this module is on a hand-built fixture whose
//! inputs are valid by construction (fixed addresses, fixed small
//! traces) — mirroring how `dsl::compile` scopes the same allowance for
//! the same reason (see that module's own docs).
#![allow(clippy::expect_used, clippy::missing_const_for_fn)]

use fact_model::{
    Address, BlockContext, BlockNumber, Call, CallDepth, CallKind, ChainId, FactArenaBuilder, Gas,
    Nonce, StorageChange, StorageSlot, Timestamp, Trace, TraceMetadata, TraceSource, Transaction,
    TxHash, TxStatus, Wei, Word,
};

use crate::types::{BenchmarkCase, ExpectedFinding, PatternOutcome};

fn addr(byte: u8) -> Address {
    Address::new([byte; 20])
}

fn hash(byte: u8) -> TxHash {
    TxHash(Word::new([byte; 32]))
}

fn simple_call(
    builder: &FactArenaBuilder,
    parent: Option<fact_model::CallId>,
    depth: u16,
    from: Address,
    to: Address,
) -> Call {
    Call::new(
        builder.next_call_id(),
        parent,
        CallKind::Call,
        CallDepth(depth),
        from,
        Some(to),
        Wei::ZERO,
        Gas(100_000),
        Gas(50_000),
        true,
    )
    .expect("valid call fixture parameters")
}

fn wrap_trace(arena: fact_model::FactArena, from: Address, label: &str) -> Trace {
    let tx = Transaction::new(
        hash(1),
        from,
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
            endpoint_label: label.to_string(),
        },
    );
    Trace::new(metadata, block, tx, arena).expect("internally consistent trace fixture")
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

/// A trace containing one genuine reentrant call plus a state write,
/// paired with `REENTRANCY_PATTERN` — the pipeline should reach
/// [`PatternOutcome::Grounded`] end to end.
///
/// # Panics
/// Never panics in practice: every `.expect(..)` below is on a
/// hand-built fixture whose arena/pattern/trace are known-valid by
/// construction.
#[must_use]
pub fn grounded_reentrancy_case() -> BenchmarkCase {
    let attacker = addr(1);
    let victim = addr(9);
    let mut builder = FactArenaBuilder::new();
    let root = simple_call(&builder, None, 0, attacker, victim);
    let root_id = builder.add_call(root);
    let mid = simple_call(&builder, Some(root_id), 1, victim, addr(5));
    let mid_id = builder.add_call(mid);
    let reentrant = simple_call(&builder, Some(mid_id), 2, addr(5), victim);
    builder.add_call(reentrant);
    builder.add_storage_change(StorageChange::new(
        root_id,
        StorageSlot::new(victim, Word::ZERO),
        Word::ZERO,
        Word::new([1; 32]),
    ));
    let arena = builder.build().expect("valid arena fixture");
    let trace = wrap_trace(arena, attacker, "fixture:grounded_reentrancy");

    let pattern = dsl::compile_str(REENTRANCY_PATTERN).expect("valid fixture pattern source");
    BenchmarkCase::from_trace(
        "grounded_reentrancy",
        "A single classic reentrant call with an accompanying state write; \
         every required clause is fully checkable and holds, so the \
         pipeline should reach Grounded.",
        trace,
        vec![pattern],
        vec![ExpectedFinding::new(
            "classic_reentrancy",
            PatternOutcome::Grounded,
        )],
    )
}

const ORACLE_MANIPULATION_PATTERN: &str = r"
    pattern oracle_price_manipulation version 1 {
        family: OracleManipulation
        severity: High
        evidence {
            required price_write: storage(changed: true, role: PriceOracle)
        }
        constraint: price_write
    }
";

/// A trace containing a storage write that structurally satisfies
/// `ORACLE_MANIPULATION_PATTERN`'s `changed` attribute, but whose
/// `role: PriceOracle` attribute names a semantic classification
/// `fact-model` has no structural representation for (see
/// `matcher::UnresolvedAttribute`'s own docs). The pipeline should
/// reach [`PatternOutcome::Abstain`]: not confidently confirmed, not
/// confidently refuted.
///
/// # Panics
/// Never panics in practice: every `.expect(..)` below is on a
/// hand-built fixture whose arena/pattern/trace are known-valid by
/// construction.
#[must_use]
pub fn abstained_oracle_manipulation_case() -> BenchmarkCase {
    let attacker = addr(1);
    let oracle = addr(7);
    let mut builder = FactArenaBuilder::new();
    let root = simple_call(&builder, None, 0, attacker, oracle);
    let root_id = builder.add_call(root);
    builder.add_storage_change(StorageChange::new(
        root_id,
        StorageSlot::new(oracle, Word::ZERO),
        Word::new([1; 32]),
        Word::new([99; 32]),
    ));
    let arena = builder.build().expect("valid arena fixture");
    let trace = wrap_trace(arena, attacker, "fixture:abstained_oracle_manipulation");

    let pattern =
        dsl::compile_str(ORACLE_MANIPULATION_PATTERN).expect("valid fixture pattern source");
    BenchmarkCase::from_trace(
        "abstained_oracle_manipulation",
        "A storage write whose `role: PriceOracle` attribute cannot be \
         structurally resolved against fact-model data; the pipeline \
         should abstain rather than confirm or refute the finding.",
        trace,
        vec![pattern],
        vec![ExpectedFinding::new(
            "oracle_price_manipulation",
            PatternOutcome::Abstain,
        )],
    )
}

/// A trace with no reentrant call and no storage write at all, paired
/// with `REENTRANCY_PATTERN`: `matcher` should find zero candidates,
/// so the pipeline should reach [`PatternOutcome::NoMatch`] without
/// grounding or taxonomy mapping ever running.
///
/// # Panics
/// Never panics in practice: every `.expect(..)` below is on a
/// hand-built fixture whose arena/pattern/trace are known-valid by
/// construction.
#[must_use]
pub fn no_match_case() -> BenchmarkCase {
    let attacker = addr(1);
    let counterparty = addr(3);
    let mut builder = FactArenaBuilder::new();
    let root = simple_call(&builder, None, 0, attacker, counterparty);
    builder.add_call(root);
    let arena = builder.build().expect("valid arena fixture");
    let trace = wrap_trace(arena, attacker, "fixture:no_match");

    let pattern = dsl::compile_str(REENTRANCY_PATTERN).expect("valid fixture pattern source");
    BenchmarkCase::from_trace(
        "no_match",
        "A single, non-reentrant, non-state-mutating call; the \
         reentrancy pattern's trigger clause has nothing to bind to, so \
         matcher should produce zero candidates.",
        trace,
        vec![pattern],
        vec![ExpectedFinding::new(
            "classic_reentrancy",
            PatternOutcome::NoMatch,
        )],
    )
}

const SINGLE_CALL_PATTERN: &str = r"
    pattern suspicious_external_call version 1 {
        family: Reentrancy
        severity: Medium
        evidence {
            required flagged_call: call(kind: External, reentrant: true)
        }
        constraint: flagged_call
    }
";

/// A hand-constructed [`matcher::CandidateMatch`] that structurally
/// cites a call which does **not** actually satisfy
/// `SINGLE_CALL_PATTERN`'s `reentrant: true` attribute (a genuinely
/// non-reentrant call: it shares no target address with any ancestor).
/// Real candidate generation (`matcher::find_candidate_matches`) could
/// never produce this binding on its own; it is assembled directly here
/// to exercise grounding's independent re-verification path as a
/// last-line-of-defense against a structurally-plausible but factually
/// wrong candidate — the "false-positive structural match" fixture
/// family this crate's tests are required to cover.
///
/// Returned as a raw `(Trace, CompiledPattern, CandidateMatch)` tuple
/// rather than a [`BenchmarkCase`]: [`crate::runner::run_case`] always
/// generates its own candidates via `matcher`, which — correctly —
/// would bind the trace's one genuine reentrant call rather than the
/// non-reentrant `root_id` call the fabricated candidate below (wrongly)
/// cites. A consumer wanting to exercise this scenario calls
/// `grounding::ground_all` on the returned candidate directly; see this
/// crate's own `false_positive_grounding_catches_fabricated_candidate`
/// test.
///
/// # Panics
/// Never panics in practice: every `.expect(..)` below is on a
/// hand-built fixture whose arena/pattern/trace are known-valid by
/// construction.
#[must_use]
pub fn false_positive_ungrounded_fixture(
) -> (Trace, dsl::ir::CompiledPattern, matcher::CandidateMatch) {
    let attacker = addr(1);
    let counterparty = addr(4);
    let mut builder = FactArenaBuilder::new();

    // A genuinely non-reentrant call: this is the one the fabricated
    // candidate below will (wrongly) cite as satisfying
    // `reentrant: true`. It is also the trace's single root call —
    // fact-model requires exactly one.
    let root = simple_call(&builder, None, 0, attacker, counterparty);
    let root_id = builder.add_call(root);

    // A genuinely reentrant call hanging off that same root: it calls
    // back into the root's own target address two levels down. This
    // gives the predicate's freshly-computed match set a real member —
    // without it, grounding would correctly report "no matching fact
    // anywhere" (`Unavailable`/`Abstain`) rather than "the cited fact
    // doesn't hold up" (`Failed`/`Ungrounded`), since an empty match set
    // is missing evidence, not a contradiction.
    let mid = simple_call(&builder, Some(root_id), 1, counterparty, addr(5));
    let mid_id = builder.add_call(mid);
    let reentrant = simple_call(&builder, Some(mid_id), 2, addr(5), counterparty);
    builder.add_call(reentrant);

    let arena = builder.build().expect("valid arena fixture");
    let trace = wrap_trace(arena, attacker, "fixture:false_positive_ungrounded");

    let pattern = dsl::compile_str(SINGLE_CALL_PATTERN).expect("valid fixture pattern source");

    // Hand-built candidate: claims `flagged_call` (evidence index 0)
    // binds to `root_id`, which is a genuine, non-reentrant top-level
    // call — the predicate's `reentrant: true` requirement does not
    // actually hold for it. The real reentrant call above is what
    // independent re-verification will find instead, exposing the
    // mismatch.
    let fabricated = matcher::CandidateMatch {
        pattern_id: pattern.id.clone(),
        pattern_version: pattern.version,
        pattern_family: pattern.family.clone(),
        pattern_severity: pattern.severity,
        bindings: vec![matcher::EvidenceBinding {
            evidence: dsl::EvidenceRef(0),
            fact: fact_model::FactRef::Call(root_id),
        }],
        metadata: matcher::MatchMetadata::default(),
    };

    (trace, pattern, fabricated)
}

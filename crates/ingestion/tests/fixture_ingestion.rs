//! Integration tests running the full ingestion pipeline against
//! realistic, hand-authored fixture files under `tests/fixtures/`.
//!
//! This file is its own compilation unit (a `tests/` integration test
//! binary), so it does not inherit `src/lib.rs`'s `cfg(test)` lint
//! exception — restated here for the same reason: test code's canonical
//! failure mode is panicking via `.unwrap()`/`.expect()`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use fact_model::{Address, TraceSource as FactTraceSource, Wei, Word};
use ingestion::{ingest, FileTraceSource};

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn selector_call_fixture_extracts_selector_end_to_end() {
    let source = FileTraceSource::new(fixture_path("selector_call.json"));
    let provenance = FactTraceSource::ArchiveNodeRpc {
        endpoint_label: "fixture:selector_call".to_string(),
    };
    let trace = ingest(&source, provenance).expect("fixture should ingest successfully");

    assert_eq!(trace.arena.calls().count(), 2);

    let root = trace.arena.root_call();
    // erc20 `transfer(address,uint256)` selector, extracted from the
    // root call's real calldata, end-to-end through the whole
    // ingestion pipeline (raw JSON -> normalize -> fact-model Call).
    assert_eq!(root.selector, Some([0xa9, 0x05, 0x9c, 0xbb]));

    let child = trace
        .arena
        .calls()
        .find(|c| !c.is_root())
        .expect("child call must exist");
    // The child call has no `input` field at all in the fixture, so its
    // selector must be `None`, not accidentally inherited from the
    // parent or defaulted to some other value.
    assert_eq!(child.selector, None);
}

#[test]
fn erc20_transfer_fixture_ingests_successfully() {
    let source = FileTraceSource::new(fixture_path("erc20_transfer.json"));
    let provenance = FactTraceSource::ArchiveNodeRpc {
        endpoint_label: "fixture:erc20_transfer".to_string(),
    };
    let trace = ingest(&source, provenance).expect("fixture should ingest successfully");

    // One call (the root), one storage change, one log, one decoded
    // token transfer.
    assert_eq!(trace.arena.calls().count(), 1);
    assert_eq!(trace.arena.storage_changes().count(), 1);
    assert_eq!(trace.arena.logs().count(), 1);
    assert_eq!(trace.arena.token_transfers().count(), 1);

    let token_contract = Address::new([0x22; 20]);
    let sender = Address::new([0x11; 20]);
    let recipient = Address::new([0x33; 20]);

    let transfer = trace.arena.token_transfers().next().unwrap();
    assert_eq!(transfer.token, token_contract);
    assert_eq!(transfer.from, sender);
    assert_eq!(transfer.to, recipient);
    assert_eq!(transfer.amount, Wei(0x384));

    let storage_change = trace.arena.storage_changes().next().unwrap();
    assert_eq!(storage_change.slot.contract, token_contract);
    assert_eq!(
        storage_change.before,
        Word::new({
            let mut b = [0u8; 32];
            b[31] = 0xe8;
            b[30] = 0x03;
            b
        })
    );

    assert_eq!(trace.transaction.hash.0, Word::new([0xaa; 32]));
    assert_eq!(trace.block.number.0, 0x0123_4567);
}

#[test]
fn erc20_transfer_fixture_trace_round_trips_through_arena_lookups() {
    let source = FileTraceSource::new(fixture_path("erc20_transfer.json"));
    let provenance = FactTraceSource::ArchiveNodeRpc {
        endpoint_label: "fixture:erc20_transfer".to_string(),
    };
    let trace = ingest(&source, provenance).unwrap();

    let root = trace.arena.root_call();
    assert!(root.is_root());
    let storage_change = trace
        .arena
        .storage_changes_for_call(root.id)
        .next()
        .unwrap();
    assert_eq!(storage_change.call_id, root.id);
}

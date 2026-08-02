//! The Validate+Normalize stage: [`RawTraceDocument`] →
//! [`NormalizedTrace`].
//!
//! A single depth-first walk of the raw call tree does three things at
//! once, because all three need the same traversal and the same
//! already-parsed values: (1) parses every hex field into `fact-model`
//! primitives, (2) computes each call's depth and storage-context
//! address and validates every nested storage change's declared
//! contract against it, and (3) best-effort-decodes ERC-20-shaped
//! `Transfer` logs into [`fact_model::TokenTransfer`] facts.
//!
//! Output is [`NormalizedTrace`]: fully `fact-model`-typed data, indexed
//! by plain `usize` positions rather than [`fact_model::CallId`]/
//! [`fact_model::LogId`] — those arena IDs are assigned by
//! [`crate::build`], the next stage, since assigning them is itself part
//! of "constructing the fact model," not normalizing raw data.

use std::collections::HashSet;

use fact_model::{
    Address, BlockNumber, CallDepth, CallKind, ChainId, Gas, LogIndex, Nonce, Timestamp, TxHash,
    TxStatus, Wei, Word,
};

use crate::error::IngestionError;
use crate::hex_util::{parse_address, parse_hex_bytes, parse_hex_u128, parse_hex_u64, parse_word};
use crate::raw::{RawCall, RawTraceDocument};

/// The real EVM's protocol-enforced call-depth limit (EIP-150). A raw
/// trace nested deeper than this is definitionally not a real EVM
/// execution and is rejected as an unsupported/malformed feature rather
/// than accepted and silently truncated.
const MAX_CALL_DEPTH: u16 = 1024;

/// The `keccak256("Transfer(address,address,uint256)")` event topic —
/// the standard, publicly documented ERC-20/ERC-721 `Transfer` event
/// signature hash used to recognize a transfer-shaped log for best-
/// effort decoding into a [`fact_model::TokenTransfer`].
const TRANSFER_EVENT_TOPIC0: Word = Word::new([
    0xdd, 0xf2, 0x52, 0xad, 0x1b, 0xe2, 0xc8, 0x9b, 0x69, 0xc2, 0xb0, 0x68, 0xfc, 0x37, 0x8d, 0xaa,
    0x95, 0x2b, 0xa7, 0xf1, 0x63, 0xc4, 0xa1, 0x16, 0x28, 0xf5, 0x5a, 0x4d, 0xf5, 0x23, 0xb3, 0xef,
]);

/// A normalized call, indexed by its position in
/// [`NormalizedTrace::calls`] (which is always a valid depth-first
/// order: every parent's index is smaller than its children's).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedCall {
    /// The index (in this same `calls` vec) of this call's parent, or
    /// `None` for the root.
    pub parent_index: Option<usize>,
    /// This call's depth; root is 0.
    pub depth: CallDepth,
    /// The call kind.
    pub kind: CallKind,
    /// The initiating address.
    pub from: Address,
    /// The target address, if resolved.
    pub to: Option<Address>,
    /// The wei value transferred.
    pub value: Wei,
    /// Gas made available.
    pub gas_limit: Gas,
    /// Gas consumed.
    pub gas_used: Gas,
    /// Whether the call succeeded.
    pub succeeded: bool,
    /// The first four bytes of this call's calldata, if any was
    /// recorded and it was at least 4 bytes long. See
    /// `extract_selector` for exactly what is (and is not) done here.
    pub selector: Option<[u8; 4]>,
}

/// A normalized storage change, attributed to a call by its position in
/// [`NormalizedTrace::calls`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizedStorageChange {
    /// Index into [`NormalizedTrace::calls`] of the call that performed
    /// this write.
    pub call_index: usize,
    /// The contract whose storage was written.
    pub contract: Address,
    /// The slot key.
    pub slot: Word,
    /// The value before this write.
    pub before: Word,
    /// The value after this write.
    pub after: Word,
}

/// A normalized log, attributed to a call by its position in
/// [`NormalizedTrace::calls`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedLog {
    /// Index into [`NormalizedTrace::calls`] of the call that emitted
    /// this log.
    pub call_index: usize,
    /// The emitting address.
    pub address: Address,
    /// The log's topics.
    pub topics: Vec<Word>,
    /// The log's data payload.
    pub data: Vec<u8>,
    /// The log's index within the transaction's full log list.
    pub log_index: LogIndex,
}

/// A normalized, best-effort-decoded token transfer, attributed to a log
/// by its position in [`NormalizedTrace::logs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizedTokenTransfer {
    /// Index into [`NormalizedTrace::logs`] of the log this was decoded
    /// from.
    pub log_index_in_vec: usize,
    /// The token contract (the log's emitting address).
    pub token: Address,
    /// The sender.
    pub from: Address,
    /// The recipient.
    pub to: Address,
    /// The transferred amount.
    pub amount: Wei,
}

/// The complete result of normalization: fully typed, but not yet
/// inserted into a [`fact_model::FactArena`] (see [`crate::build`] for
/// that final step).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedTrace {
    /// The transaction's hash.
    pub transaction_hash: TxHash,
    /// The transaction's sender.
    pub transaction_from: Address,
    /// The transaction's direct recipient, or `None` for a
    /// contract-creation transaction.
    pub transaction_to: Option<Address>,
    /// The wei value sent directly with the transaction.
    pub transaction_value: Wei,
    /// The sender's nonce.
    pub transaction_nonce: Nonce,
    /// Total gas consumed by the transaction.
    pub transaction_gas_used: Gas,
    /// Whether the transaction succeeded or reverted.
    pub transaction_status: TxStatus,
    /// The block number the transaction was included in.
    pub block_number: BlockNumber,
    /// The block's Unix timestamp.
    pub block_timestamp: Timestamp,
    /// The chain the block belongs to.
    pub block_chain_id: ChainId,
    /// The block's base fee, if EIP-1559 is active.
    pub block_base_fee: Option<Wei>,
    /// Every call, in depth-first (parent-before-child) order.
    pub calls: Vec<NormalizedCall>,
    /// Every storage change, in the order encountered.
    pub storage_changes: Vec<NormalizedStorageChange>,
    /// Every log, in the order encountered.
    pub logs: Vec<NormalizedLog>,
    /// Every best-effort-decoded token transfer.
    pub token_transfers: Vec<NormalizedTokenTransfer>,
}

/// Mutable accumulator threaded through the recursive tree walk, kept as
/// a single struct so the recursive function signature stays manageable.
#[derive(Default)]
struct Accumulator {
    calls: Vec<NormalizedCall>,
    storage_changes: Vec<NormalizedStorageChange>,
    logs: Vec<NormalizedLog>,
    token_transfers: Vec<NormalizedTokenTransfer>,
    seen_log_indices: HashSet<u64>,
}

/// Normalize a complete [`RawTraceDocument`] into a [`NormalizedTrace`].
///
/// # Errors
/// See [`IngestionError`]'s variants; in particular
/// [`IngestionError::UnsupportedFeature`] for an unrecognized call kind,
/// an invalid transaction status string, or excessive call depth;
/// [`IngestionError::InvalidStorageOwnership`] for a storage change
/// whose declared contract doesn't match its call's storage context;
/// [`IngestionError::DuplicateLogIndex`] for two logs sharing a
/// `log_index`; and [`IngestionError::MalformedField`] for any
/// individual hex field that fails to parse.
pub fn normalize(doc: &RawTraceDocument) -> Result<NormalizedTrace, IngestionError> {
    let transaction_hash = TxHash(parse_word("transaction.hash", &doc.transaction.hash)?);
    let transaction_from = parse_address("transaction.from", &doc.transaction.from)?;
    let transaction_to = doc
        .transaction
        .to
        .as_deref()
        .map(|s| parse_address("transaction.to", s))
        .transpose()?;
    let transaction_value = Wei(parse_hex_u128("transaction.value", &doc.transaction.value)?);
    let transaction_nonce = Nonce(parse_hex_u64("transaction.nonce", &doc.transaction.nonce)?);
    let transaction_gas_used = Gas(parse_hex_u64(
        "transaction.gasUsed",
        &doc.transaction.gas_used,
    )?);
    let transaction_status = match doc.transaction.status.as_str() {
        "success" => TxStatus::Success,
        "reverted" => TxStatus::Reverted,
        other => {
            return Err(IngestionError::UnsupportedFeature {
                reason: format!("unrecognized transaction status `{other}`"),
            })
        }
    };

    let block_number = BlockNumber(parse_hex_u64("block.number", &doc.block.number)?);
    let block_timestamp = Timestamp(parse_hex_u64("block.timestamp", &doc.block.timestamp)?);
    let block_chain_id = ChainId(parse_hex_u64("block.chainId", &doc.block.chain_id)?);
    let block_base_fee = doc
        .block
        .base_fee
        .as_deref()
        .map(|s| parse_hex_u128("block.baseFee", s).map(Wei))
        .transpose()?;

    let mut acc = Accumulator::default();
    walk_tree(&doc.root, &mut acc)?;

    Ok(NormalizedTrace {
        transaction_hash,
        transaction_from,
        transaction_to,
        transaction_value,
        transaction_nonce,
        transaction_gas_used,
        transaction_status,
        block_number,
        block_timestamp,
        block_chain_id,
        block_base_fee,
        calls: acc.calls,
        storage_changes: acc.storage_changes,
        logs: acc.logs,
        token_transfers: acc.token_transfers,
    })
}

/// Parse a raw call kind string into a [`CallKind`].
fn parse_call_kind(path: &str, raw: &str) -> Result<CallKind, IngestionError> {
    match raw.to_ascii_lowercase().as_str() {
        "call" => Ok(CallKind::Call),
        "staticcall" => Ok(CallKind::StaticCall),
        "delegatecall" => Ok(CallKind::DelegateCall),
        "callcode" => Ok(CallKind::CallCode),
        "create" => Ok(CallKind::Create),
        "create2" => Ok(CallKind::Create2),
        "selfdestruct" => Ok(CallKind::Selfdestruct),
        other => Err(IngestionError::UnsupportedFeature {
            reason: format!("unrecognized call kind `{other}` at {path}"),
        }),
    }
}

/// One unit of pending work in the iterative call-tree walk: a raw call
/// node still needing normalization, plus the context its parent
/// already established.
struct PendingCall<'a> {
    raw: &'a RawCall,
    parent_index: Option<usize>,
    depth: u16,
    parent_storage_context: Option<Address>,
    path: String,
}

/// Validate and normalize every storage change nested directly under one
/// call node, pushing results into `acc`. Factored out of [`walk_tree`]
/// to keep that function under a manageable line count and because this
/// logic is a self-contained unit: it only needs the call's already-
/// computed `storage_context` and index, not any other traversal state.
fn process_storage_changes(
    raw: &RawCall,
    this_index: usize,
    storage_context: Address,
    path: &str,
    acc: &mut Accumulator,
) -> Result<(), IngestionError> {
    for change in &raw.storage_changes {
        let declared = parse_address("storageChange.contract", &change.contract)?;
        if declared != storage_context {
            return Err(IngestionError::InvalidStorageOwnership {
                call_path: path.to_string(),
                declared: declared.to_string(),
                expected: storage_context.to_string(),
            });
        }
        acc.storage_changes.push(NormalizedStorageChange {
            call_index: this_index,
            contract: declared,
            slot: parse_word("storageChange.slot", &change.slot)?,
            before: parse_word("storageChange.before", &change.before)?,
            after: parse_word("storageChange.after", &change.after)?,
        });
    }
    Ok(())
}

/// Validate and normalize every log nested directly under one call
/// node, pushing results (and any best-effort-decoded token transfer)
/// into `acc`. Factored out of [`walk_tree`] for the same reason as
/// [`process_storage_changes`].
fn process_logs(
    raw: &RawCall,
    this_index: usize,
    path: &str,
    acc: &mut Accumulator,
) -> Result<(), IngestionError> {
    for log in &raw.logs {
        let log_index_raw = parse_hex_u64("log.logIndex", &log.log_index)?;
        if !acc.seen_log_indices.insert(log_index_raw) {
            return Err(IngestionError::DuplicateLogIndex {
                log_index: log_index_raw,
            });
        }
        let address = parse_address("log.address", &log.address)?;
        let topics = log
            .topics
            .iter()
            .map(|t| parse_word("log.topics[]", t))
            .collect::<Result<Vec<_>, _>>()?;
        if topics.len() > 4 {
            return Err(IngestionError::UnsupportedFeature {
                reason: format!(
                    "log at {path} has more than 4 topics, which the EVM cannot produce"
                ),
            });
        }
        let data = parse_hex_bytes("log.data", &log.data)?;

        let normalized_log_index = acc.logs.len();
        if let Some(transfer) = try_decode_token_transfer(address, &topics, &data) {
            acc.token_transfers.push(NormalizedTokenTransfer {
                log_index_in_vec: normalized_log_index,
                token: transfer.0,
                from: transfer.1,
                to: transfer.2,
                amount: transfer.3,
            });
        }

        acc.logs.push(NormalizedLog {
            call_index: this_index,
            address,
            topics,
            data,
            log_index: LogIndex(u32::try_from(log_index_raw).map_err(|_| {
                IngestionError::UnsupportedFeature {
                    reason: format!("log index {log_index_raw} exceeds u32::MAX"),
                }
            })?),
        });
    }
    Ok(())
}

/// Normalize the entire raw call tree rooted at `root`, using an
/// explicit work-stack rather than native recursion.
///
/// ## Why iterative, not recursive
/// A call tree's depth is attacker/input-controlled (bounded only by
/// [`MAX_CALL_DEPTH`], which itself exists precisely because the EVM
/// permits up to 1024 levels of nesting). A naively recursive walk
/// therefore recurses up to ~1024 stack frames deep on entirely
/// legitimate input — and test-harness threads (and some production
/// environments) use a smaller stack than a process's main thread,
/// making that recursion depth a real stack-overflow risk in practice,
/// not merely a hypothetical one (this was caught by this crate's own
/// `excessive_call_depth_rejected` test during development, which
/// crashed with a stack overflow before this function was made
/// iterative). An explicit `Vec`-backed stack moves that memory to the
/// heap, where its size is bounded by available memory rather than a
/// fixed thread stack.
///
/// Processing order (parent fully handled — including all of its own
/// storage changes and logs — before any child) is preserved by
/// pushing each node's children onto the stack in **reverse** order, so
/// the first child is popped and processed immediately after its
/// parent, exactly matching what a recursive pre-order traversal would
/// do. This ordering matters because [`CallId`] assignment (in
/// `crate::build`) depends on `NormalizedTrace::calls` being in a valid
/// depth-first, parent-before-child order.
fn walk_tree(root: &RawCall, acc: &mut Accumulator) -> Result<(), IngestionError> {
    let mut stack = vec![PendingCall {
        raw: root,
        parent_index: None,
        depth: 0,
        parent_storage_context: None,
        path: "root".to_string(),
    }];

    while let Some(item) = stack.pop() {
        if item.depth > MAX_CALL_DEPTH {
            return Err(IngestionError::UnsupportedFeature {
                reason: format!(
                    "call tree depth at {} exceeds the EVM's protocol call-depth limit of {MAX_CALL_DEPTH}",
                    item.path
                ),
            });
        }

        let raw = item.raw;
        let kind = parse_call_kind(&item.path, &raw.kind)?;
        let from = parse_address("call.from", &raw.from)?;
        let to = raw
            .to
            .as_deref()
            .map(|s| parse_address("call.to", s))
            .transpose()?;
        let value = Wei(parse_hex_u128("call.value", &raw.value)?);
        let gas_limit = Gas(parse_hex_u64("call.gasLimit", &raw.gas_limit)?);
        let gas_used = Gas(parse_hex_u64("call.gasUsed", &raw.gas_used)?);
        let selector = match &raw.input {
            Some(input) => extract_selector(&parse_hex_bytes("call.input", input)?),
            None => None,
        };

        let storage_context = if kind.executes_in_caller_context() {
            item.parent_storage_context
                .ok_or_else(|| IngestionError::UnsupportedFeature {
                    reason: format!(
                        "call at {} is a delegatecall/callcode with no parent to inherit a storage context from",
                        item.path
                    ),
                })?
        } else {
            to.unwrap_or(from)
        };

        let this_index = acc.calls.len();
        acc.calls.push(NormalizedCall {
            parent_index: item.parent_index,
            depth: CallDepth(item.depth),
            kind,
            from,
            to,
            value,
            gas_limit,
            gas_used,
            succeeded: raw.succeeded,
            selector,
        });

        process_storage_changes(raw, this_index, storage_context, &item.path, acc)?;
        process_logs(raw, this_index, &item.path, acc)?;

        // Push children in reverse so the first child is popped (and
        // thus processed) immediately next, matching recursive
        // pre-order semantics.
        for (i, child) in raw.calls.iter().enumerate().rev() {
            stack.push(PendingCall {
                raw: child,
                parent_index: Some(this_index),
                depth: item.depth + 1,
                parent_storage_context: Some(storage_context),
                path: format!("{}.calls[{i}]", item.path),
            });
        }
    }

    Ok(())
}

/// Extract a call's function selector: the first 4 bytes of its
/// calldata, and nothing more.
///
/// Returns `None` if `calldata` has fewer than 4 bytes (e.g. a plain
/// ETH transfer with empty `input`) — that is not an error, just the
/// absence of a selector, exactly mirroring how
/// [`try_decode_token_transfer`] treats a non-matching log as `None`
/// rather than a parse failure.
///
/// Deliberately minimal: no 4-byte-signature database lookup, no ABI
/// parameter decoding of anything past those 4 bytes. Widening this is
/// explicitly out of scope for this milestone.
fn extract_selector(calldata: &[u8]) -> Option<[u8; 4]> {
    calldata.get(..4)?.try_into().ok()
}

/// Best-effort-decode a log matching the standard ERC-20/ERC-721
/// `Transfer(address,address,uint256)` shape: exactly 3 topics
/// (signature + two indexed addresses) with topic0 equal to
/// [`TRANSFER_EVENT_TOPIC0`], and exactly 32 bytes of data encoding the
/// amount.
///
/// Returns `None` (not an error) for anything that doesn't match this
/// shape — a non-matching log is not malformed, it simply isn't a
/// decodable transfer, per [`fact_model::TokenTransfer`]'s own
/// documented reasoning for why interpretation failures don't invalidate
/// the underlying [`fact_model::LogEvent`].
fn try_decode_token_transfer(
    token: Address,
    topics: &[Word],
    data: &[u8],
) -> Option<(Address, Address, Address, Wei)> {
    if topics.len() != 3 || topics[0] != TRANSFER_EVENT_TOPIC0 || data.len() != 32 {
        return None;
    }
    let from = address_from_topic(topics[1])?;
    let to = address_from_topic(topics[2])?;
    // The EVM ABI left-pads a uint256 amount to 32 bytes; per ADR-0006,
    // only amounts fitting in u128 are representable — anything larger
    // is treated as non-decodable rather than truncated.
    if data[..16] != [0u8; 16] {
        return None;
    }
    let mut low = [0u8; 16];
    low.copy_from_slice(&data[16..]);
    Some((token, from, to, Wei(u128::from_be_bytes(low))))
}

/// Extract an `Address` from a 32-byte topic that ABI-encodes it
/// left-padded with zeros. Returns `None` if the padding bytes aren't
/// actually zero (not a standard address-shaped topic).
fn address_from_topic(word: Word) -> Option<Address> {
    let bytes = word.as_bytes();
    if bytes[..12] != [0u8; 12] {
        return None;
    }
    let mut addr_bytes = [0u8; 20];
    addr_bytes.copy_from_slice(&bytes[12..]);
    Some(Address::new(addr_bytes))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::raw::{RawBlock, RawLog, RawStorageChange, RawTransaction};

    fn addr_hex(byte: u8) -> String {
        format!("0x{}", hex::encode([byte; 20]))
    }

    mod hex {
        use std::fmt::Write as _;

        pub fn encode(bytes: impl AsRef<[u8]>) -> String {
            bytes.as_ref().iter().fold(String::new(), |mut acc, b| {
                let _ = write!(acc, "{b:02x}");
                acc
            })
        }
    }

    fn minimal_doc() -> RawTraceDocument {
        RawTraceDocument {
            transaction: RawTransaction {
                hash: format!("0x{}", "11".repeat(32)),
                from: addr_hex(1),
                to: Some(addr_hex(2)),
                value: "0x0".to_string(),
                nonce: "0x0".to_string(),
                gas_used: "0x5208".to_string(),
                status: "success".to_string(),
            },
            block: RawBlock {
                number: "0x64".to_string(),
                timestamp: "0x1".to_string(),
                chain_id: "0x1".to_string(),
                base_fee: None,
            },
            root: RawCall {
                kind: "call".to_string(),
                from: addr_hex(1),
                to: Some(addr_hex(2)),
                value: "0x0".to_string(),
                gas_limit: "0x5208".to_string(),
                gas_used: "0x5208".to_string(),
                succeeded: true,
                input: None,
                storage_changes: vec![],
                logs: vec![],
                calls: vec![],
            },
        }
    }

    #[test]
    fn extract_selector_returns_none_for_empty_calldata() {
        assert_eq!(extract_selector(&[]), None);
    }

    #[test]
    fn extract_selector_returns_none_for_short_calldata() {
        assert_eq!(extract_selector(&[0xa9, 0x05, 0x9c]), None);
    }

    #[test]
    fn extract_selector_returns_first_four_bytes_exactly() {
        assert_eq!(
            extract_selector(&[0xa9, 0x05, 0x9c, 0xbb]),
            Some([0xa9, 0x05, 0x9c, 0xbb])
        );
    }

    #[test]
    fn extract_selector_ignores_bytes_past_the_fourth() {
        // No ABI parameter decoding: everything after byte 4 is ignored.
        assert_eq!(
            extract_selector(&[0xa9, 0x05, 0x9c, 0xbb, 0xde, 0xad, 0xbe, 0xef]),
            Some([0xa9, 0x05, 0x9c, 0xbb])
        );
    }

    #[test]
    fn normalize_sets_none_selector_when_no_calldata_present() {
        let doc = minimal_doc();
        let normalized = normalize(&doc).unwrap();
        assert_eq!(normalized.calls[0].selector, None);
    }

    #[test]
    fn normalize_extracts_selector_from_calldata() {
        let mut doc = minimal_doc();
        doc.root.input = Some("0xa9059cbb000000000000000000000000".to_string());
        let normalized = normalize(&doc).unwrap();
        assert_eq!(normalized.calls[0].selector, Some([0xa9, 0x05, 0x9c, 0xbb]));
    }

    #[test]
    fn normalize_sets_none_selector_for_short_calldata() {
        let mut doc = minimal_doc();
        doc.root.input = Some("0xa905".to_string());
        let normalized = normalize(&doc).unwrap();
        assert_eq!(normalized.calls[0].selector, None);
    }

    #[test]
    fn normalize_rejects_malformed_calldata_hex() {
        let mut doc = minimal_doc();
        doc.root.input = Some("0xzzzz".to_string());
        assert!(matches!(
            normalize(&doc),
            Err(IngestionError::MalformedField { .. })
        ));
    }

    #[test]
    fn normalize_minimal_document() {
        let doc = minimal_doc();
        let normalized = normalize(&doc).unwrap();
        assert_eq!(normalized.calls.len(), 1);
        assert_eq!(normalized.calls[0].depth.0, 0);
        assert!(normalized.calls[0].parent_index.is_none());
    }

    #[test]
    fn normalize_rejects_unknown_call_kind() {
        let mut doc = minimal_doc();
        doc.root.kind = "invalidopcode".to_string();
        assert!(matches!(
            normalize(&doc),
            Err(IngestionError::UnsupportedFeature { .. })
        ));
    }

    #[test]
    fn normalize_accepts_selfdestruct_call_kind() {
        let mut doc = minimal_doc();
        doc.root.kind = "selfdestruct".to_string();
        let normalized = normalize(&doc).unwrap();
        assert_eq!(normalized.calls[0].kind, CallKind::Selfdestruct);
    }

    #[test]
    fn normalize_rejects_unknown_status() {
        let mut doc = minimal_doc();
        doc.transaction.status = "pending".to_string();
        assert!(matches!(
            normalize(&doc),
            Err(IngestionError::UnsupportedFeature { .. })
        ));
    }

    #[test]
    fn normalize_computes_nested_depth() {
        let mut doc = minimal_doc();
        doc.root.calls.push(RawCall {
            kind: "call".to_string(),
            from: addr_hex(2),
            to: Some(addr_hex(3)),
            value: "0x0".to_string(),
            gas_limit: "0x1".to_string(),
            gas_used: "0x1".to_string(),
            succeeded: true,
            input: None,
            storage_changes: vec![],
            logs: vec![],
            calls: vec![],
        });
        let normalized = normalize(&doc).unwrap();
        assert_eq!(normalized.calls.len(), 2);
        assert_eq!(normalized.calls[1].depth.0, 1);
        assert_eq!(normalized.calls[1].parent_index, Some(0));
    }

    #[test]
    fn normalize_accepts_matching_storage_ownership() {
        let mut doc = minimal_doc();
        doc.root.storage_changes.push(RawStorageChange {
            contract: addr_hex(2), // matches root.to
            slot: format!("0x{}", "00".repeat(32)),
            before: format!("0x{}", "00".repeat(32)),
            after: format!("0x{}", "01".repeat(32)),
        });
        let normalized = normalize(&doc).unwrap();
        assert_eq!(normalized.storage_changes.len(), 1);
    }

    #[test]
    fn normalize_rejects_mismatched_storage_ownership() {
        let mut doc = minimal_doc();
        doc.root.storage_changes.push(RawStorageChange {
            contract: addr_hex(9), // does NOT match root.to (addr 2)
            slot: format!("0x{}", "00".repeat(32)),
            before: format!("0x{}", "00".repeat(32)),
            after: format!("0x{}", "01".repeat(32)),
        });
        assert!(matches!(
            normalize(&doc),
            Err(IngestionError::InvalidStorageOwnership { .. })
        ));
    }

    #[test]
    fn delegatecall_child_inherits_parent_storage_context() {
        let mut doc = minimal_doc();
        doc.root.calls.push(RawCall {
            kind: "delegatecall".to_string(),
            from: addr_hex(2),
            to: Some(addr_hex(3)), // executes addr(3)'s code, but in addr(2)'s storage
            value: "0x0".to_string(),
            gas_limit: "0x1".to_string(),
            gas_used: "0x1".to_string(),
            succeeded: true,
            input: None,
            storage_changes: vec![RawStorageChange {
                contract: addr_hex(2), // must be the PARENT's context (root.to), not addr(3)
                slot: format!("0x{}", "00".repeat(32)),
                before: format!("0x{}", "00".repeat(32)),
                after: format!("0x{}", "01".repeat(32)),
            }],
            logs: vec![],
            calls: vec![],
        });
        let normalized = normalize(&doc).unwrap();
        assert_eq!(normalized.storage_changes.len(), 1);
        assert_eq!(
            normalized.storage_changes[0].contract,
            Address::new([2; 20])
        );
    }

    #[test]
    fn normalize_rejects_duplicate_log_index() {
        let mut doc = minimal_doc();
        let log = RawLog {
            address: addr_hex(2),
            topics: vec![],
            data: "0x".to_string(),
            log_index: "0x0".to_string(),
        };
        doc.root.logs.push(log.clone());
        doc.root.calls.push(RawCall {
            kind: "call".to_string(),
            from: addr_hex(2),
            to: Some(addr_hex(3)),
            value: "0x0".to_string(),
            gas_limit: "0x1".to_string(),
            gas_used: "0x1".to_string(),
            succeeded: true,
            input: None,
            storage_changes: vec![],
            logs: vec![log], // same log_index "0x0" again
            calls: vec![],
        });
        assert!(matches!(
            normalize(&doc),
            Err(IngestionError::DuplicateLogIndex { .. })
        ));
    }

    #[test]
    fn normalize_decodes_transfer_shaped_log() {
        let mut doc = minimal_doc();
        let mut from_topic = vec![0u8; 32];
        from_topic[12..].copy_from_slice(&[1u8; 20]);
        let mut to_topic = vec![0u8; 32];
        to_topic[12..].copy_from_slice(&[2u8; 20]);
        let mut amount_data = vec![0u8; 32];
        amount_data[16..].copy_from_slice(&1_000u128.to_be_bytes());

        doc.root.logs.push(RawLog {
            address: addr_hex(9), // token contract
            topics: vec![
                format!("0x{}", hex::encode(TRANSFER_EVENT_TOPIC0.as_bytes())),
                format!("0x{}", hex::encode(&from_topic)),
                format!("0x{}", hex::encode(&to_topic)),
            ],
            data: format!("0x{}", hex::encode(&amount_data)),
            log_index: "0x0".to_string(),
        });

        let normalized = normalize(&doc).unwrap();
        assert_eq!(normalized.token_transfers.len(), 1);
        let transfer = normalized.token_transfers[0];
        assert_eq!(transfer.token, Address::new([9; 20]));
        assert_eq!(transfer.from, Address::new([1; 20]));
        assert_eq!(transfer.to, Address::new([2; 20]));
        assert_eq!(transfer.amount, Wei(1_000));
    }

    #[test]
    fn normalize_skips_non_transfer_shaped_log_without_erroring() {
        let mut doc = minimal_doc();
        doc.root.logs.push(RawLog {
            address: addr_hex(9),
            topics: vec![format!("0x{}", "ab".repeat(32))], // not the Transfer topic
            data: "0x".to_string(),
            log_index: "0x0".to_string(),
        });
        let normalized = normalize(&doc).unwrap();
        assert_eq!(normalized.logs.len(), 1);
        assert_eq!(normalized.token_transfers.len(), 0);
    }

    #[test]
    fn excessive_call_depth_rejected() {
        // Build a chain of MAX_CALL_DEPTH + 2 nested calls, iteratively
        // (see `walk_tree`'s own documentation for why this crate avoids
        // depth-proportional recursion generally).
        fn leaf_call() -> RawCall {
            RawCall {
                kind: "call".to_string(),
                from: addr_hex(1),
                to: Some(addr_hex(2)),
                value: "0x0".to_string(),
                gas_limit: "0x1".to_string(),
                gas_used: "0x1".to_string(),
                succeeded: true,
                input: None,
                storage_changes: vec![],
                logs: vec![],
                calls: vec![],
            }
        }
        let mut chain = leaf_call();
        for _ in 0..(usize::from(MAX_CALL_DEPTH) + 2) {
            let mut parent = leaf_call();
            parent.calls.push(chain);
            chain = parent;
        }
        let mut doc = minimal_doc();
        doc.root = chain;
        assert!(matches!(
            normalize(&doc),
            Err(IngestionError::UnsupportedFeature { .. })
        ));
    }

    proptest! {
        /// For any randomly-generated, structurally valid call tree
        /// (bounded depth/breadth, always well-formed `kind` strings and
        /// hex fields), `normalize` never rejects it, and every produced
        /// `NormalizedCall`'s depth equals its parent's depth + 1 (root
        /// is 0) — the same invariant `fact_model::FactArenaBuilder`
        /// enforces at construction, proven here to hold for arbitrary
        /// tree shapes rather than only the hand-picked cases above.
        #[test]
        fn normalize_always_produces_depth_consistent_calls(
            child_counts in prop::collection::vec(0usize..3, 1..20)
        ) {
            // Build a tree whose shape is driven by `child_counts`: a
            // flat sequence of "how many children does this node have"
            // values consumed breadth-first-ish to build varied shapes
            // without risking pathological depth.
            fn build(counts: &[usize], idx: &mut usize) -> RawCall {
                let children_wanted = if *idx < counts.len() {
                    let c = counts[*idx];
                    *idx += 1;
                    c
                } else {
                    0
                };
                let mut calls = Vec::new();
                for _ in 0..children_wanted {
                    if *idx >= counts.len() {
                        break;
                    }
                    calls.push(build(counts, idx));
                }
                RawCall {
                    kind: "call".to_string(),
                    from: "0x0101010101010101010101010101010101010101".to_string(),
                    to: Some("0x0202020202020202020202020202020202020202".to_string()),
                    value: "0x0".to_string(),
                    gas_limit: "0x1".to_string(),
                    gas_used: "0x1".to_string(),
                    succeeded: true,
                    input: None,
                    storage_changes: vec![],
                    logs: vec![],
                    calls,
                }
            }
            let mut idx = 0;
            let mut doc = minimal_doc();
            doc.root = build(&child_counts, &mut idx);

            let normalized = normalize(&doc).unwrap();
            for call in &normalized.calls {
                match call.parent_index {
                    None => prop_assert_eq!(call.depth.0, 0),
                    Some(parent_idx) => {
                        let parent_depth = normalized.calls[parent_idx].depth.0;
                        prop_assert_eq!(call.depth.0, parent_depth + 1);
                    }
                }
            }
        }
    }
}

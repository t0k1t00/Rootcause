//! Converts a real Geth `debug_traceTransaction` `callTracer` response
//! into Root Cause's `ingestion::raw::RawTraceDocument` schema.
//!
//! # Schema studied
//! This module's input schema is Geth's `callTracer` output (the
//! nested-call-frame tracer, not the flat opcode logger), verified
//! against real JSON-RPC responses from a Goerli archive node — see
//! `demo/samples/geth_calltracer_goerli.json`, saved verbatim from
//! <https://github.com/ethereum/go-ethereum/issues/26726> (tx
//! `0xc0ffcf21dc1881c3f20160b530d76f1d37af7b40552f7c59d3f339bad3023dad`,
//! geth v1.10.26). Milestone 11 additionally verified this exact
//! parsing logic, unmodified, against a real Erigon `callTracer`
//! response — see `demo/samples/erigon_calltracer_polygon.json`, saved
//! verbatim from <https://github.com/erigontech/erigon/issues/7568>
//! (Polygon mainnet tx
//! `0x19afd31a9327af30b1d7abf1d46efa228af0747268e618fcf06dcb62655999f8`,
//! Erigon v2.43.0): Erigon's `callTracer` uses the identical field
//! names and uppercase `type` values, so no client-specific branching
//! was ever needed here. Every call frame has this shape:
//!
//! ```json
//! {
//!   "type": "CALL" | "STATICCALL" | "DELEGATECALL" | "CALLCODE"
//!         | "CREATE" | "CREATE2" | "SELFDESTRUCT",
//!   "from": "0x...", "to": "0x...", "value": "0x...",
//!   "gas": "0x...", "gasUsed": "0x...",
//!   "input": "0x...", "output": "0x...",
//!   "error": "execution reverted"   // present only on a reverted frame
//!   "calls": [ ...same shape... ]   // present only if there are children
//! }
//! ```
//!
//! # What this converter can and cannot represent
//!
//! **Faithfully converted:** the call tree shape (`from`/`to`/`value`/
//! `gas`/`gasUsed`), call kind (`type`, lowercased, matching
//! `ingestion::raw`'s accepted set exactly), and success/failure
//! (`succeeded = error.is_none()`).
//!
//! **Not converted — and deliberately left empty, not guessed at:**
//! storage changes and logs. `callTracer` alone carries neither. Geth's
//! *other* tracers that do carry this information don't solve the
//! problem either:
//!
//! - `prestateTracer` (`diffMode: true`) returns storage diffs keyed by
//!   **address only** — a flat `{address: {storage: {slot: [from, to]}}}`
//!   map with no attribution to *which call in the tree* produced each
//!   write. Root Cause's raw schema (see `ingestion::raw`'s own module
//!   docs) requires that attribution, specifically because neither
//!   Geth tracer alone supplies it.
//! - Reconstructing that attribution would require a heuristic (e.g.
//!   "the last call frame targeting this address wrote this slot"),
//!   which breaks down as soon as one address is called more than once
//!   in a trace, or a `DELEGATECALL` is involved (the write shows up
//!   under the *storage-context* address, not the executing code's own
//!   address) — silently getting this wrong in a security tool is worse
//!   than reporting nothing.
//!
//! Traces converted from `callTracer` alone will therefore have empty
//! `storageChanges` and `logs` on every call. Patterns whose evidence
//! depends on a `storage(...)` predicate (e.g. `classic_reentrancy`)
//! will correctly find no candidates on such a trace — not because
//! nothing happened, but because this converter cannot yet supply that
//! evidence. This is a known, documented limitation, not a bug.
//!
//! `SELFDESTRUCT` frames convert like any other call kind
//! (`fact_model::CallKind::Selfdestruct`, added alongside the
//! `unsafe_selfdestruct` pattern — see the top-level README's exploit
//! corpus notes). Earlier versions of this converter rejected them
//! with an explicit error because no target variant existed yet; now
//! that one does, rejecting them would silently *lose* real trace
//! shape this converter is otherwise faithful about preserving.

use ingestion::raw::{RawBlock, RawCall, RawStorageChange, RawTraceDocument, RawTransaction};
use serde::Deserialize;

/// One call frame as Geth's `callTracer` emits it.
#[derive(Debug, Clone, Deserialize)]
struct GethCallFrame {
    #[serde(rename = "type")]
    kind: String,
    from: String,
    to: Option<String>,
    #[serde(default = "default_value")]
    value: String,
    gas: String,
    #[serde(rename = "gasUsed")]
    gas_used: String,
    /// `0x`-prefixed hex calldata. `#[serde(default)]` so frames that
    /// omit it (e.g. `CREATE`/`CREATE2` with no explicit `input` on some
    /// tracer versions) still parse.
    #[serde(default)]
    input: Option<String>,
    error: Option<String>,
    #[serde(default)]
    calls: Vec<GethCallFrame>,
}

fn default_value() -> String {
    "0x0".to_string()
}

/// Everything this converter needs beyond what `callTracer` itself
/// supplies. Geth's tracer response describes *execution only*; it has
/// no concept of which block a transaction landed in, or the
/// transaction's own hash/nonce/final status independent of the root
/// call frame's `error` field — those come from `eth_getTransactionReceipt`
/// / `eth_getBlockByNumber`, which this crate deliberately does not fetch
/// itself (no network I/O in a converter — see `ingestion`'s own
/// `TraceSource` design for the same reasoning).
#[derive(Debug, Clone)]
pub struct GethTraceContext {
    /// `0x`-prefixed 32-byte transaction hash.
    pub transaction_hash: String,
    /// `0x`-prefixed hex transaction nonce.
    pub nonce: String,
    /// `0x`-prefixed hex block number the transaction executed in.
    pub block_number: String,
    /// `0x`-prefixed hex chain ID.
    pub chain_id: String,
}

/// Everything that can go wrong converting a Geth trace.
#[derive(Debug, thiserror::Error)]
pub enum GethConvertError {
    /// The input wasn't valid JSON, or didn't match `callTracer`'s frame shape.
    #[error("input is not valid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    /// A call frame's `type` wasn't one of the seven kinds `ingestion::raw` accepts.
    #[error("call frame has type `{0}`, not one of the seven kinds ingestion::raw accepts")]
    UnknownCallKind(String),
}

/// Convert a raw `callTracer` JSON-RPC response body (the `"result"`
/// object, or a bare call-frame object — both accepted) into a
/// [`RawTraceDocument`].
///
/// # Errors
/// Returns [`GethConvertError`] if the input isn't valid JSON in the
/// expected shape, or a call frame's `type` isn't one of the kinds
/// `ingestion::raw` accepts.
pub fn convert(
    call_tracer_json: &str,
    ctx: &GethTraceContext,
) -> Result<RawTraceDocument, GethConvertError> {
    let value: serde_json::Value = serde_json::from_str(call_tracer_json)?;
    // Accept either the full `{"jsonrpc":..., "result": {...}}` envelope
    // or a bare call-frame object, so a person can paste either the raw
    // curl response or an already-unwrapped `.result` field.
    let root_value = value.get("result").cloned().unwrap_or(value);
    let root_frame: GethCallFrame = serde_json::from_value(root_value)?;

    let root_call = convert_frame(&root_frame)?;

    Ok(RawTraceDocument {
        transaction: RawTransaction {
            hash: ctx.transaction_hash.clone(),
            from: root_frame.from.clone(),
            to: root_frame.to.clone(),
            value: root_frame.value.clone(),
            nonce: ctx.nonce.clone(),
            gas_used: root_frame.gas_used.clone(),
            status: if root_frame.error.is_none() {
                "success".to_string()
            } else {
                "reverted".to_string()
            },
        },
        block: RawBlock {
            number: ctx.block_number.clone(),
            timestamp: "0x0".to_string(),
            chain_id: ctx.chain_id.clone(),
            base_fee: None,
        },
        root: root_call,
    })
}

fn convert_frame(frame: &GethCallFrame) -> Result<RawCall, GethConvertError> {
    let kind = frame.kind.to_lowercase();
    if !matches!(
        kind.as_str(),
        "call" | "staticcall" | "delegatecall" | "callcode" | "create" | "create2" | "selfdestruct"
    ) {
        return Err(GethConvertError::UnknownCallKind(frame.kind.clone()));
    }

    let calls = frame
        .calls
        .iter()
        .map(convert_frame)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(RawCall {
        kind,
        from: frame.from.clone(),
        to: frame.to.clone(),
        value: frame.value.clone(),
        gas_limit: frame.gas.clone(),
        gas_used: frame.gas_used.clone(),
        succeeded: frame.error.is_none(),
        // `callTracer` does carry calldata, unlike storage/logs (see
        // module docs) — pass it through so `call(selector: "0x...")`
        // patterns work against converted traces.
        input: frame.input.clone(),
        // See this module's docs: callTracer carries neither storage
        // diffs nor logs, and attributing prestateTracer's flat,
        // address-keyed diff back to individual call frames would
        // require an unverified heuristic. Left empty deliberately.
        storage_changes: Vec::<RawStorageChange>::new(),
        logs: Vec::new(),
        calls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> GethTraceContext {
        GethTraceContext {
            transaction_hash: "0xc0ffcf21dc1881c3f20160b530d76f1d37af7b40552f7c59d3f339bad3023dad"
                .to_string(),
            nonce: "0x1".to_string(),
            block_number: "0x1".to_string(),
            chain_id: "0x5".to_string(),
        }
    }

    #[test]
    fn converts_nested_call_and_delegatecall() {
        let json = r#"{
            "type": "CALL", "from": "0xaaaa000000000000000000000000000000aaaa",
            "to": "0xbbbb000000000000000000000000000000bbbb", "value": "0x1",
            "gas": "0x100", "gasUsed": "0x50",
            "calls": [
                { "type": "DELEGATECALL", "from": "0xbbbb000000000000000000000000000000bbbb",
                  "to": "0xcccc000000000000000000000000000000cccc",
                  "gas": "0x80", "gasUsed": "0x40" }
            ]
        }"#;
        let doc = convert(json, &ctx()).expect("valid callTracer frame must convert");
        assert_eq!(doc.root.kind, "call");
        assert!(doc.root.succeeded);
        assert_eq!(doc.root.calls.len(), 1);
        assert_eq!(doc.root.calls[0].kind, "delegatecall");
        assert!(doc.root.storage_changes.is_empty());
        assert!(doc.root.logs.is_empty());
    }

    #[test]
    fn error_field_maps_to_reverted_status_and_unsucceeded_call() {
        let json = r#"{
            "type": "CALL", "from": "0xaaaa000000000000000000000000000000aaaa",
            "to": "0xbbbb000000000000000000000000000000bbbb", "value": "0x0",
            "gas": "0x100", "gasUsed": "0x30", "error": "execution reverted"
        }"#;
        let doc = convert(json, &ctx()).expect("reverted frame must still convert");
        assert_eq!(doc.transaction.status, "reverted");
        assert!(!doc.root.succeeded);
    }

    #[test]
    fn selfdestruct_frame_converts_like_any_other_call_kind() {
        let json = r#"{
            "type": "CALL", "from": "0xaaaa000000000000000000000000000000aaaa",
            "to": "0xbbbb000000000000000000000000000000bbbb", "value": "0x0",
            "gas": "0x100", "gasUsed": "0x30",
            "calls": [
                { "type": "SELFDESTRUCT", "from": "0xbbbb000000000000000000000000000000bbbb",
                  "to": "0xdddd000000000000000000000000000000dddd",
                  "gas": "0x10", "gasUsed": "0x5" }
            ]
        }"#;
        let doc = convert(json, &ctx()).expect("SELFDESTRUCT must now convert");
        assert_eq!(doc.root.calls[0].kind, "selfdestruct");
    }

    #[test]
    fn unrecognized_call_kind_is_still_rejected() {
        let json = r#"{
            "type": "NOTAREALOPCODE", "from": "0xaaaa000000000000000000000000000000aaaa",
            "to": "0xbbbb000000000000000000000000000000bbbb", "value": "0x0",
            "gas": "0x100", "gasUsed": "0x30"
        }"#;
        let err = convert(json, &ctx()).expect_err("unknown call kind must not silently convert");
        assert!(matches!(err, GethConvertError::UnknownCallKind(_)));
    }

    #[test]
    fn accepts_full_jsonrpc_envelope_or_bare_result() {
        let bare = r#"{
            "type": "CALL", "from": "0xaaaa000000000000000000000000000000aaaa",
            "to": "0xbbbb000000000000000000000000000000bbbb", "value": "0x0",
            "gas": "0x100", "gasUsed": "0x30"
        }"#;
        let enveloped = format!(r#"{{"jsonrpc":"2.0","id":1,"result":{bare}}}"#);
        let from_bare = convert(bare, &ctx()).expect("bare frame must convert");
        let from_enveloped = convert(&enveloped, &ctx()).expect("enveloped frame must convert");
        assert_eq!(from_bare.root.from, from_enveloped.root.from);
    }

    #[test]
    fn missing_value_on_delegatecall_defaults_to_zero() {
        // Real geth v1.10.26 responses omit `value` on DELEGATECALL
        // frames entirely (see this module's doc comment) rather than
        // encoding a zero — confirmed directly against the saved
        // sample in demo/samples/geth_calltracer_goerli.json.
        let json = r#"{
            "type": "DELEGATECALL", "from": "0xaaaa000000000000000000000000000000aaaa",
            "to": "0xbbbb000000000000000000000000000000bbbb",
            "gas": "0x100", "gasUsed": "0x30"
        }"#;
        let doc = convert(json, &ctx()).expect("frame with omitted value must still convert");
        assert_eq!(doc.root.value, "0x0");
    }
}

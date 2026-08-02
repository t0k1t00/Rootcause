//! The raw, wire-shaped input schema ingestion consumes.
//!
//! This is "Root Cause Combined Trace Format v1" (ADR-0007): a
//! project-defined JSON schema, not a literal reproduction of any
//! specific archive-node provider's API response. It mirrors the shape
//! of two real Geth debug-tracer outputs (`callTracer`'s nested call
//! tree, `prestateTracer`'s diff-mode storage) but resolves the
//! call-attribution gap neither of those alone can supply by nesting
//! storage changes and logs directly inside the call node that produced
//! them.
//!
//! Every type here is intentionally "dumb": fields are hex strings or
//! bare JSON primitives, with no `fact-model` types and no validation
//! beyond what `serde`'s derive gives for free (field presence, basic
//! shape). Interpretation into `fact-model` types happens entirely in
//! [`crate::normalize`] — keeping this module free of that logic is
//! what makes it possible to point a different raw schema (a real
//! provider's actual response, once an adapter exists) at the same
//! normalization code, by producing a [`RawTraceDocument`] some other
//! way.

use serde::{Deserialize, Serialize};

/// The complete raw input for one trace: a transaction, the block it
/// executed in, and its call tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RawTraceDocument {
    /// The transaction being traced.
    pub transaction: RawTransaction,
    /// The block the transaction executed in.
    pub block: RawBlock,
    /// The root of the call tree (the transaction's own top-level call).
    pub root: RawCall,
}

/// Raw transaction fields, as hex strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RawTransaction {
    /// `0x`-prefixed 32-byte transaction hash.
    pub hash: String,
    /// `0x`-prefixed 20-byte sender address.
    pub from: String,
    /// `0x`-prefixed 20-byte recipient address, or `null` for a
    /// contract-creation transaction.
    pub to: Option<String>,
    /// `0x`-prefixed hex wei value.
    pub value: String,
    /// `0x`-prefixed hex nonce.
    pub nonce: String,
    /// `0x`-prefixed hex total gas used.
    pub gas_used: String,
    /// Either `"success"` or `"reverted"`.
    pub status: String,
}

/// Raw block fields, as hex strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RawBlock {
    /// `0x`-prefixed hex block number.
    pub number: String,
    /// `0x`-prefixed hex Unix timestamp.
    pub timestamp: String,
    /// `0x`-prefixed hex chain ID.
    pub chain_id: String,
    /// `0x`-prefixed hex base fee, or `null` on a pre-EIP-1559 chain/block.
    pub base_fee: Option<String>,
}

/// One node in the raw call tree, with its own storage changes and logs
/// nested inline (see this module's top-level documentation for why).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RawCall {
    /// One of `"call"`, `"staticcall"`, `"delegatecall"`, `"callcode"`,
    /// `"create"`, `"create2"` (case-insensitive).
    pub kind: String,
    /// `0x`-prefixed 20-byte sender address.
    pub from: String,
    /// `0x`-prefixed 20-byte target address, or `null` for a not-yet-
    /// resolved `create`/`create2` target.
    pub to: Option<String>,
    /// `0x`-prefixed hex wei value transferred by this call.
    pub value: String,
    /// `0x`-prefixed hex gas made available to this call.
    pub gas_limit: String,
    /// `0x`-prefixed hex gas actually consumed.
    pub gas_used: String,
    /// Whether this call succeeded.
    pub succeeded: bool,
    /// `0x`-prefixed hex calldata (the `input` field of a real Geth
    /// `callTracer` call node), or `None` if calldata was not recorded
    /// for this call. `#[serde(default)]` so documents produced before
    /// this field existed still parse unchanged — this crate's
    /// backward-compatibility rule for every additive raw-schema field.
    #[serde(default)]
    pub input: Option<String>,
    /// Storage changes performed directly by this call, in the order
    /// they occurred.
    #[serde(default)]
    pub storage_changes: Vec<RawStorageChange>,
    /// Logs emitted directly by this call, in the order they occurred.
    #[serde(default)]
    pub logs: Vec<RawLog>,
    /// Child calls, in execution order.
    #[serde(default)]
    pub calls: Vec<RawCall>,
}

/// One raw storage write.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RawStorageChange {
    /// The contract whose storage was written — the "storage context"
    /// address (see [`crate::normalize`] for how this is validated
    /// against the enclosing call).
    pub contract: String,
    /// `0x`-prefixed 32-byte storage slot key.
    pub slot: String,
    /// `0x`-prefixed 32-byte value before this write.
    pub before: String,
    /// `0x`-prefixed 32-byte value after this write.
    pub after: String,
}

/// One raw log.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RawLog {
    /// `0x`-prefixed 20-byte emitting address.
    pub address: String,
    /// Up to four `0x`-prefixed 32-byte topics.
    pub topics: Vec<String>,
    /// `0x`-prefixed hex data payload (may be `"0x"` for empty data).
    pub data: String,
    /// `0x`-prefixed hex log index within the transaction's full log
    /// list.
    pub log_index: String,
}

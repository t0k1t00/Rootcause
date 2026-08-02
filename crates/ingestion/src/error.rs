//! [`IngestionError`]: the crate's single exhaustive error type.
//!
//! Per the same error-handling philosophy `fact-model` uses (ADR-0003):
//! every variant names the specific field, call, or value involved, and
//! the enum is deliberately not `#[non_exhaustive]` so downstream code
//! (or a future CLI reporting ingestion failures to a user) can match
//! precisely on *why* ingestion failed.

use fact_model::FactModelError;

/// Everything that can cause ingestion to fail, from raw bytes to a
/// validated `fact_model::Trace`.
#[derive(Debug, thiserror::Error)]
pub enum IngestionError {
    /// Reading raw bytes from a [`crate::source::TraceSource`] failed
    /// (e.g. a file could not be opened).
    #[error("failed to load trace input: {reason}")]
    SourceUnavailable {
        /// A human-readable description of what went wrong.
        reason: String,
    },

    /// The raw bytes were not valid JSON, or did not match the expected
    /// [`crate::raw::RawTraceDocument`] shape (missing a required field,
    /// wrong type for a field, unknown field present).
    #[error("malformed trace input: {reason}")]
    MalformedInput {
        /// The underlying `serde_json` error, rendered to a string
        /// (kept as `String` rather than the error type itself, since
        /// `serde_json::Error` does not implement `Clone`/`PartialEq`
        /// and this crate's error strategy prefers comparable errors —
        /// see `fact-model`'s own `FactModelError`, which is `PartialEq`
        /// for exactly this reason).
        reason: String,
    },

    /// A specific field's value did not parse into the type it should
    /// have (e.g. `"value": "0xzz"` — present, but not valid hex).
    #[error("field `{field}` has an invalid value `{value}`: {reason}")]
    MalformedField {
        /// The field's name (dotted path where useful, e.g.
        /// `"root.calls[2].value"`).
        field: &'static str,
        /// The raw string value that failed to parse.
        value: String,
        /// Why it was rejected.
        reason: &'static str,
    },

    /// A required piece of information was structurally absent (used
    /// for cases `serde`'s own required-field checking can't catch,
    /// e.g. an empty call tree where at least one call is semantically
    /// required even though the JSON key itself was present).
    #[error("missing required field or data: {description}")]
    MissingRequired {
        /// What was missing.
        description: String,
    },

    /// The raw trace used a feature this ingestion pipeline does not
    /// support (e.g. an unrecognized `kind` string, or a call-tree depth
    /// beyond the EVM's protocol-enforced 1024-call limit).
    #[error("unsupported trace feature: {reason}")]
    UnsupportedFeature {
        /// What was unsupported.
        reason: String,
    },

    /// The trace's structure was internally inconsistent in a way that
    /// isn't captured by a more specific variant below (reserved for
    /// genuinely cross-cutting inconsistencies; prefer a specific
    /// variant when one applies).
    #[error("inconsistent trace structure: {reason}")]
    InconsistentStructure {
        /// What was inconsistent.
        reason: String,
    },

    /// A storage change's declared `contract` address did not match the
    /// storage context of the call it was nested under (see
    /// [`crate::normalize`] for how storage context is computed for
    /// `DelegateCall`/`CallCode`).
    #[error(
        "storage change in call at path {call_path} declares contract {declared}, but the \
         call's storage context is {expected}"
    )]
    InvalidStorageOwnership {
        /// A human-readable path identifying which call in the tree
        /// (e.g. `"root.calls[1].calls[0]"`).
        call_path: String,
        /// The contract address the raw storage change declared.
        declared: String,
        /// The storage context address the call actually executes in.
        expected: String,
    },

    /// Two logs in the same trace declared the same `log_index`, which
    /// should be a unique position within the transaction's full log
    /// list.
    #[error("duplicate log index {log_index}: appears more than once in this trace")]
    DuplicateLogIndex {
        /// The duplicated index.
        log_index: u64,
    },

    /// A [`fact_model::FactArenaBuilder::build`] or
    /// [`fact_model::Trace::new`] call failed, despite normalization
    /// having already run. This should not be reachable from malformed
    /// *input* — reaching it indicates a bug in this crate's
    /// normalization logic, not a bad trace — but is surfaced with full
    /// context rather than panicking, since "no unwrap in library code"
    /// applies here too.
    #[error("internal fact-model construction failure: {0}")]
    ArenaConstruction(#[from] FactModelError),

    /// `Trace::new`'s own cross-type consistency check failed (its
    /// `metadata`/`block`/`transaction` fields disagreed) — likewise
    /// should not be reachable from malformed input; a bug in this
    /// crate's normalization if it fires.
    #[error("internal trace construction failure: {0}")]
    TraceConstruction(&'static str),
}

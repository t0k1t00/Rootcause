//! The `fact-model` crate's single exhaustive error type.
//!
//! Per ADR-0003 (error-handling strategy), every fallible operation in
//! this crate returns `Result<_, FactModelError>`. The enum is
//! exhaustive and `#[non_exhaustive]` is deliberately **not** applied:
//! this crate's whole purpose is to let downstream crates pattern-match
//! precisely on *why* a trace failed to validate (e.g. to distinguish a
//! benchmark-relevant "unresolvable storage layout" case from a plain
//! ingestion bug), and `#[non_exhaustive]` would force every match to
//! carry a catch-all arm that defeats that.

use crate::ids::{CallId, LogId, StorageChangeId};

/// All ways constructing a [`crate::FactArena`] or [`crate::Trace`] can
/// fail.
///
/// Every variant names the specific fact(s) involved, not just "a" fact,
/// so a validation failure is immediately actionable without re-deriving
/// which fact was the problem.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FactModelError {
    /// The arena's call list was empty. Every trace must contain at
    /// least the top-level call that corresponds to the transaction
    /// itself.
    #[error("a trace's call list must contain at least one call (the root), but it was empty")]
    EmptyCallList,

    /// More than one call has `parent == None`. Exactly one root call is
    /// required — a well-formed call tree has exactly one entry point.
    #[error(
        "expected exactly one root call (parent == None), found a second root at {second_root}"
    )]
    MultipleRootCalls {
        /// The first root call found (always index 0, by construction).
        first_root: CallId,
        /// The unexpected second root call.
        second_root: CallId,
    },

    /// The call at index 0 has a non-`None` parent. By construction, the
    /// root call must be the first call added to the arena.
    #[error(
        "the first call in the arena must be the root (parent == None), but {call} has a parent"
    )]
    FirstCallIsNotRoot {
        /// The call found at index 0.
        call: CallId,
    },

    /// A non-root call names a parent ID that either does not exist in
    /// the arena, or exists but appears *after* this call (a forward
    /// reference). Forward references are rejected, not just missing
    /// ones, because several downstream algorithms (e.g. depth
    /// validation, single-pass tree traversal) depend on every call's
    /// parent already having been seen.
    #[error("call {call} references parent {parent}, which does not precede it in the arena")]
    ParentNotYetSeen {
        /// The call with the problematic parent reference.
        call: CallId,
        /// The parent ID that was referenced.
        parent: CallId,
    },

    /// A call's recorded [`crate::CallDepth`] does not equal its
    /// parent's depth + 1. This would indicate the ingestion source
    /// itself is internally inconsistent (a real bug in whatever
    /// produced the trace, not a modeling gap).
    #[error(
        "call {call} has depth {actual}, but its parent {parent} has depth {parent_depth}, so {call} should have depth {expected}"
    )]
    InconsistentCallDepth {
        /// The call with the wrong depth.
        call: CallId,
        /// Its parent.
        parent: CallId,
        /// The parent's actual depth.
        parent_depth: u16,
        /// The depth actually recorded on `call`.
        actual: u16,
        /// The depth `call` should have had.
        expected: u16,
    },

    /// A [`crate::StorageChange`] names a `call_id` that does not exist
    /// in the arena's call list.
    #[error("storage change {storage_change} references call {call}, which does not exist in this arena")]
    StorageChangeReferencesUnknownCall {
        /// The offending storage change.
        storage_change: StorageChangeId,
        /// The call ID it referenced.
        call: CallId,
    },

    /// A [`crate::LogEvent`] names a `call_id` that does not exist in
    /// the arena's call list.
    #[error("log {log} references call {call}, which does not exist in this arena")]
    LogReferencesUnknownCall {
        /// The offending log.
        log: LogId,
        /// The call ID it referenced.
        call: CallId,
    },

    /// A [`crate::TokenTransfer`] names a source `log_id` that does not
    /// exist in the arena's log list.
    #[error("token transfer references log {log}, which does not exist in this arena")]
    TokenTransferReferencesUnknownLog {
        /// The log ID it referenced.
        log: LogId,
    },
}

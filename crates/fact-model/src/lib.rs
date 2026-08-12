//! # fact-model
//!
//! Canonical, immutable representation of a decoded EVM transaction trace.
//!
//! This crate defines the "fact model": the shared vocabulary of facts
//! (calls, storage changes, value flows, logs, and their relationships)
//! that every other crate in the Root Cause workspace is built against.
//! It has no dependency on any other workspace crate — `ingestion`
//! produces fact models, `matcher`/`grounding` consume them, but
//! `fact-model` itself knows about neither.
//!
//! ## Design summary
//!
//! - **Immutable after construction.** Every type is built once via a
//!   fallible constructor and exposes no mutation API afterward.
//! - **Arena-indexed, not pointer/`Rc`-based.** [`FactArena`] owns every
//!   [`Call`], [`StorageChange`], [`LogEvent`], and [`TokenTransfer`];
//!   everything else refers to them by small `Copy` ID newtypes
//!   ([`CallId`], [`StorageChangeId`], [`LogId`], [`TokenTransferId`]).
//!   This is what lets a grounding result cite "a specific `Call`" (the
//!   Architecture document's central requirement) without `Rc<RefCell>`
//!   or lifetimes leaking into every downstream crate.
//! - **Strongly typed.** No public field is a raw `u64`/`String`/byte
//!   array; every semantically distinct scalar is a newtype (see
//!   [`primitives`]).
//! - **Deterministic and `Send + Sync` by construction**, not by added
//!   synchronization: no interior mutability, no `Rc`, no hash-order-
//!   sensitive collections in any public API.
//!
//! ## Module map
//!
//! - [`primitives`] — scalar newtypes (`Address`, `Word`, `Wei`, ...).
//! - [`ids`] — arena ID newtypes and [`FactRef`], the sum type grounding
//!   uses to cite an arbitrary fact.
//! - [`call`] — [`Call`] and [`CallKind`].
//! - [`storage`] — [`StorageChange`] and [`StorageSlot`].
//! - [`value_flow`] — the computed (not stored) [`ValueFlow`] view.
//! - [`log`] — [`LogEvent`] and [`TokenTransfer`].
//! - [`transaction`] — [`Transaction`] and [`TxStatus`].
//! - [`block`] — [`BlockContext`].
//! - [`arena`] — [`FactArena`] and [`FactArenaBuilder`].
//! - [`trace`] — [`Trace`], [`TraceMetadata`], [`TraceSource`].
//! - [`error`] — [`FactModelError`], the crate's single exhaustive error
//!   type (per ADR-0003).

#![forbid(unsafe_code)]
// Test code's canonical failure mode is panicking (`.unwrap()`/`.expect()`
// with a diagnostic message, `assert!`) — the workspace's `unwrap_used`/
// `expect_used`/`panic` lints exist to enforce "no unwrap() in *library*
// code" (see root Cargo.toml), not to fight the standard test idiom.
// Scoped to `cfg(test)` only, so the policy still applies at full force
// to every non-test line in this crate. `missing_const_for_fn` (a nursery
// lint) is also relaxed for tests: tiny test-only helpers like `addr()`
// below are not worth the ceremony of marking `const fn` one by one.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::missing_const_for_fn
    )
)]

pub mod arena;
pub mod block;
pub mod call;
pub mod error;
pub mod ids;
pub mod log;
pub mod primitives;
pub mod storage;
pub mod trace;
pub mod transaction;
pub mod value_flow;

pub use arena::{FactArena, FactArenaBuilder};
pub use block::BlockContext;
pub use call::{Call, CallKind};
pub use error::FactModelError;
pub use ids::{CallId, FactRef, LogId, StorageChangeId, TokenTransferId};
pub use log::{LogEvent, TokenTransfer};
pub use primitives::{
    Address, BlockNumber, CallDepth, ChainId, Gas, LogIndex, Nonce, Timestamp, TxHash, Wei, Word,
};
pub use storage::{StorageChange, StorageSlot};
pub use trace::{Trace, TraceConstructionError, TraceMetadata, TraceSource};
pub use transaction::{Transaction, TxStatus};
pub use value_flow::ValueFlow;

/// The crate's own semantic version, re-exported for diagnostic and
/// benchmark-harness provenance purposes (e.g. attaching a fact-model
/// version to a stored benchmark result so results remain reproducible
/// across crate versions).
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
#[allow(
    clippy::const_is_empty,
    reason = "these consts are env!(\"CARGO_PKG_VERSION\") — clippy can't see through env!, and can never actually be empty; the asserts are intentional canaries that env! resolved and cross-crate version re-exports link"
)]
mod tests {
    use super::*;

    #[test]
    fn crate_version_is_nonempty() {
        assert!(!CRATE_VERSION.is_empty());
    }
}

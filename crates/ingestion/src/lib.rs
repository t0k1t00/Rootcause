//! # ingestion
//!
//! Converts raw archive-node-style transaction traces into the
//! canonical `fact-model` [`fact_model::Trace`] representation.
//!
//! This crate performs **only** loading, parsing, validation,
//! normalization, and fact-model construction — no exploit analysis, no
//! pattern matching, and no grounding. Those are `matcher`'s and
//! `grounding`'s responsibilities, in later tasks.
//!
//! ## Pipeline
//!
//! ```text
//! TraceSource::load()  →  decode::decode()  →  normalize::normalize()  →  build::build()  →  fact_model::Trace
//!      (Source)              (Decode)          (Validate+Normalize)        (Construct)
//! ```
//!
//! [`pipeline::ingest`] runs all four stages. See ADR-0007 for why the
//! source abstraction and raw wire schema are designed the way they are
//! (in particular: why a real archive-node RPC client is a deferred
//! follow-on rather than built here, and why the raw schema nests
//! storage changes/logs inside call nodes rather than mirroring a raw
//! provider response literally).
//!
//! ## Module map
//!
//! - [`source`] — [`source::TraceSource`], the pluggable "where do raw
//!   bytes come from" trait, plus two network-free implementations.
//! - [`raw`] — the raw, wire-shaped JSON schema
//!   ([`raw::RawTraceDocument`]) this crate decodes.
//! - [`decode`] — bytes → [`raw::RawTraceDocument`].
//! - [`hex_util`] — shared hex-parsing helpers used by [`normalize`].
//! - [`normalize`] — [`raw::RawTraceDocument`] →
//!   [`normalize::NormalizedTrace`]: validation and type conversion.
//! - [`build`] — [`normalize::NormalizedTrace`] → [`fact_model::Trace`].
//! - [`pipeline`] — [`pipeline::ingest`], the single entry point running
//!   all four stages.
//! - [`error`] — [`error::IngestionError`], the crate's single
//!   exhaustive error type.

#![forbid(unsafe_code)]
// See fact-model's lib.rs for the identical rationale: test code's
// canonical failure mode is panicking, which the workspace's
// unwrap_used/expect_used/panic lints (correctly aimed at library code)
// would otherwise fight. Scoped to cfg(test) only.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::missing_const_for_fn
    )
)]

pub mod build;
pub mod decode;
pub mod error;
pub mod hex_util;
pub mod normalize;
pub mod pipeline;
pub mod raw;
pub mod source;

pub use error::IngestionError;
pub use pipeline::ingest;
pub use source::{FileTraceSource, InMemoryTraceSource, TraceSource};

/// Re-exported so downstream crates (and diagnostics/benchmark-harness
/// provenance records) can confirm which `fact-model` version this build
/// of `ingestion` was compiled against.
pub const FACT_MODEL_VERSION_USED: &str = fact_model::CRATE_VERSION;

/// The crate's own semantic version.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_version_is_nonempty() {
        assert!(!CRATE_VERSION.is_empty());
    }

    #[test]
    fn fact_model_dependency_links() {
        assert!(!FACT_MODEL_VERSION_USED.is_empty());
    }
}

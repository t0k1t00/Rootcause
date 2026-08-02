//! # benchmark-harness
//!
//! Orchestrates the complete Root Cause pipeline —
//! `ingestion -> matcher -> grounding -> taxonomy` — over a corpus of
//! benchmark cases, and computes the correctness and performance
//! metrics the Benchmark document requires: precision, recall, F1, a
//! confusion matrix, per-pattern and per-family statistics, per-stage
//! timings, and a flat failure summary.
//!
//! ## Design summary
//!
//! - **A benchmark case names an expectation, not just an input.**
//!   [`types::BenchmarkCase`] pairs a trace and one or more compiled
//!   patterns with what its author expects each pattern to reach (a
//!   [`types::PatternOutcome`]) -- precision/recall/F1 are meaningless
//!   without a ground-truth label to compare against, so this crate
//!   makes that label a first-class, mandatory-to-consider part of a
//!   case rather than an optional annotation bolted onto a bare trace.
//! - **Trace input is pluggable, not just "a JSON file."**
//!   [`types::TraceInput`] supports both re-ingesting from raw bytes on
//!   every run (a file, an in-memory buffer, or -- via `ingestion`'s
//!   own `TraceSource` trait -- anything a caller implements) and a
//!   trace already built directly via `fact-model`'s arena builder,
//!   accommodating both "realistic fixture file" and "hand-built
//!   minimal repro" benchmark cases with the same
//!   [`types::BenchmarkCase`] type.
//! - **A case-level or pattern-level failure never aborts a suite run.**
//!   [`runner::run_case`] and [`runner::run_suite`] capture every
//!   failure on the affected case's own [`runner::CaseResult`] instead
//!   of short-circuiting, so one broken fixture in a hundred-case suite
//!   still yields a complete report for the other ninety-nine -- see
//!   those functions' own docs.
//! - **Metrics are a pure function of case results.** [`metrics::compute`]
//!   takes `&[runner::CaseResult]` and returns a
//!   [`metrics::MetricsSummary`] with no side effects and no hidden
//!   state, so it (and the [`report::BenchmarkReport`] built from it)
//!   can be tested directly against hand-constructed results without
//!   running the real pipeline -- see this crate's `metrics` tests.
//! - **Export is a pure projection of [`report::BenchmarkReport`].**
//!   [`export::to_json`], [`export::to_csv`], and [`export::to_markdown`]
//!   all read the same report; none maintains its own separate view of
//!   the data, so the three formats cannot silently drift apart from
//!   each other or from what [`report::BenchmarkReport`] itself
//!   reports.
//!
//! ## Module map
//!
//! - [`error`] -- [`error::BenchmarkError`], this crate's single
//!   exhaustive error type (ADR-0003).
//! - [`types`] -- [`types::BenchmarkCase`], [`types::BenchmarkSuite`],
//!   [`types::TraceInput`], [`types::PatternOutcome`],
//!   [`types::ExpectedFinding`].
//! - [`timing`] -- [`timing::StageTimings`].
//! - [`runner`] -- [`runner::run_case`], [`runner::run_suite`],
//!   [`runner::CaseResult`], [`runner::PatternResult`]: orchestrates
//!   the pipeline itself.
//! - [`metrics`] -- [`metrics::compute`], [`metrics::MetricsSummary`],
//!   [`metrics::ConfusionMatrix`], [`metrics::BenchmarkMetrics`],
//!   [`metrics::PatternStats`], [`metrics::FamilyStats`]: derives
//!   correctness metrics from case results.
//! - [`report`] -- [`report::BenchmarkReport`], the strongly-typed
//!   top-level report every export format projects.
//! - [`export`] -- [`export::to_json`], [`export::to_csv`],
//!   [`export::to_markdown`].
//! - [`fixtures`] -- loading cases and suites from JSON benchmark
//!   definitions and fixture directories on disk.
//! - [`sample_fixtures`] -- four small, realistic, in-code fixtures (a
//!   grounded reentrancy, an abstained oracle-manipulation, a no-match
//!   case, and a false-positive structural match caught by grounding)
//!   used by this crate's own tests and reusable by downstream callers.

#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::missing_const_for_fn
    )
)]

pub mod error;
pub mod export;
pub mod fixtures;
pub mod metrics;
pub mod report;
pub mod runner;
pub mod sample_fixtures;
pub mod timing;
pub mod types;

pub use error::BenchmarkError;
pub use metrics::{BenchmarkMetrics, ConfusionMatrix, MetricsSummary};
pub use report::BenchmarkReport;
pub use runner::{run_case, run_suite, CaseResult, PatternResult};
pub use timing::StageTimings;
pub use types::{BenchmarkCase, BenchmarkSuite, ExpectedFinding, PatternOutcome, TraceInput};

/// The crate's own semantic version.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Re-exported so a caller (or a persisted [`BenchmarkReport`]) can
/// confirm which `fact-model` version this build of `benchmark-harness`
/// was compiled against -- see the equivalent constant in every other
/// workspace crate for the same rationale.
pub const FACT_MODEL_VERSION_USED: &str = fact_model::CRATE_VERSION;

/// See [`FACT_MODEL_VERSION_USED`].
pub const INGESTION_VERSION_USED: &str = ingestion::CRATE_VERSION;

/// See [`FACT_MODEL_VERSION_USED`].
pub const DSL_VERSION_USED: &str = dsl::CRATE_VERSION;

/// See [`FACT_MODEL_VERSION_USED`].
pub const MATCHER_VERSION_USED: &str = matcher::CRATE_VERSION;

/// See [`FACT_MODEL_VERSION_USED`].
pub const GROUNDING_VERSION_USED: &str = grounding::CRATE_VERSION;

/// See [`FACT_MODEL_VERSION_USED`].
pub const TAXONOMY_VERSION_USED: &str = taxonomy::CRATE_VERSION;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_version_is_nonempty() {
        assert!(!CRATE_VERSION.is_empty());
    }

    #[test]
    fn dependencies_link() {
        assert!(!FACT_MODEL_VERSION_USED.is_empty());
        assert!(!INGESTION_VERSION_USED.is_empty());
        assert!(!DSL_VERSION_USED.is_empty());
        assert!(!MATCHER_VERSION_USED.is_empty());
        assert!(!GROUNDING_VERSION_USED.is_empty());
        assert!(!TAXONOMY_VERSION_USED.is_empty());
    }
}

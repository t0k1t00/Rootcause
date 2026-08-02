//! [`BenchmarkError`], this crate's single exhaustive error type
//! (ADR-0003).

use std::path::PathBuf;

/// Everything that can go wrong running a benchmark case or suite.
///
/// Per ADR-0003 this crate follows the same "one exhaustive error enum,
/// wrapping the lower layer's own error rather than re-deriving it"
/// shape every other crate in the workspace uses. A single benchmark
/// case failing during ingestion/matching/grounding is not necessarily
/// fatal to a whole suite run — see [`crate::runner::run_suite`] for how
/// a per-case error is captured on that case's own result rather than
/// aborting the suite.
#[derive(Debug, thiserror::Error)]
pub enum BenchmarkError {
    /// Ingesting a case's raw trace input failed.
    #[error("case `{case_id}`: ingestion failed: {source}")]
    Ingestion {
        /// Which case failed.
        case_id: String,
        /// The underlying ingestion error.
        #[source]
        source: ingestion::IngestionError,
    },

    /// Compiling one of a case's DSL pattern sources failed.
    #[error("case `{case_id}`: pattern `{pattern_label}` failed to compile: {source}")]
    PatternCompile {
        /// Which case failed.
        case_id: String,
        /// A human-readable label for the pattern source that failed
        /// (its declared id when known, or the source's position in
        /// the case's pattern list).
        pattern_label: String,
        /// The underlying DSL compile error.
        #[source]
        source: dsl::DslError,
    },

    /// Structural matching failed for a case/pattern pair.
    #[error("case `{case_id}`: pattern `{pattern_id}`: matching failed: {source}")]
    Matching {
        /// Which case failed.
        case_id: String,
        /// Which pattern failed to match.
        pattern_id: String,
        /// The underlying matcher error.
        #[source]
        source: matcher::MatcherError,
    },

    /// Grounding failed for a case/pattern pair.
    #[error("case `{case_id}`: pattern `{pattern_id}`: grounding failed: {source}")]
    Grounding {
        /// Which case failed.
        case_id: String,
        /// Which pattern failed to ground.
        pattern_id: String,
        /// The underlying grounding error.
        #[source]
        source: grounding::GroundingError,
    },

    /// Reading a fixture or dataset file from disk failed.
    #[error("failed to read fixture file `{}`: {reason}", .path.display())]
    FixtureIo {
        /// The path that could not be read.
        path: PathBuf,
        /// A human-readable description of the underlying I/O failure.
        reason: String,
    },

    /// A fixture or benchmark-definition file was not valid JSON, or
    /// did not match the expected schema.
    #[error("malformed benchmark definition `{}`: {reason}", .path.display())]
    MalformedDefinition {
        /// The definition file that failed to parse.
        path: PathBuf,
        /// A human-readable description of the parse failure.
        reason: String,
    },

    /// A fixture directory did not exist, or was not a directory.
    #[error("fixture directory `{}` does not exist or is not a directory", .path.display())]
    InvalidFixtureDirectory {
        /// The path that was expected to be a fixture directory.
        path: PathBuf,
    },

    /// Report export (JSON/CSV/Markdown) failed.
    #[error("failed to export benchmark report as {format}: {reason}")]
    Export {
        /// Which export format failed (`"json"`, `"csv"`, or
        /// `"markdown"`).
        format: &'static str,
        /// A human-readable description of the failure.
        reason: String,
    },

    /// A benchmark suite named a case whose id was not unique within
    /// the suite.
    #[error("duplicate case id `{case_id}` in suite `{suite_name}`")]
    DuplicateCaseId {
        /// The suite containing the duplicate.
        suite_name: String,
        /// The id that appeared more than once.
        case_id: String,
    },
}

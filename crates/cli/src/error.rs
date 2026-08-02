//! [`CliError`]: the CLI's single exhaustive error type, and the exit
//! codes it maps onto.
//!
//! This crate follows the same convention as every other workspace
//! crate (ADR-0003: one `thiserror`-derived `Error` enum per crate) but
//! adds one thing no library crate needs: a mapping from each variant
//! to a process [`std::process::ExitCode`], since a CLI's contract with
//! its caller includes its exit status, not just its printed message.
//!
//! ## Exit code convention
//!
//! - `0` — success.
//! - `1` — usage error: bad arguments, an unreadable/malformed pattern
//!   or suite path, an unsupported output format for the given
//!   command. The person invoking the CLI can fix this by changing
//!   what they typed.
//! - `2` — pipeline error: ingestion, DSL compilation, matching,
//!   grounding, or benchmark execution itself failed against
//!   well-formed input (e.g. a malformed trace file, a pattern that
//!   fails to compile, an internal matcher/grounding error). This is a
//!   problem with the *input data* or the *engine*, not with how the
//!   CLI was invoked.
//! - `3` — I/O error unrelated to pipeline processing: the CLI could
//!   not write its output to the requested `--output` path.
//!
//! Every variant below is annotated with which of these it maps to.

use std::path::PathBuf;

/// Everything that can cause the `rootcause` CLI to fail.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// Bad arguments or an invalid combination of flags — caught before
    /// any pipeline stage runs. Exit code `1`.
    #[error("{0}")]
    Usage(String),

    /// A path the user named (a trace file, a patterns file/directory,
    /// a suite file/directory) could not be read. Exit code `1`: this
    /// is something the invocation got wrong, not a pipeline failure.
    #[error("failed to read `{path}`: {reason}")]
    InputPath {
        /// The path that could not be read.
        path: PathBuf,
        /// A human-readable description of the underlying failure.
        reason: String,
    },

    /// Writing the CLI's output to the `--output` path failed. Exit
    /// code `3`.
    #[error("failed to write output to `{path}`: {reason}")]
    OutputWrite {
        /// The path that could not be written.
        path: PathBuf,
        /// A human-readable description of the underlying failure.
        reason: String,
    },

    /// A pattern failed to parse, validate, or compile. Exit code `2`.
    #[error("{0}")]
    Dsl(#[from] dsl::DslError),

    /// Loading and building the trace's fact model failed. Exit code
    /// `2`.
    #[error("{0}")]
    Ingestion(#[from] ingestion::IngestionError),

    /// Structural pattern matching failed. Exit code `2`.
    #[error("{0}")]
    Matcher(#[from] matcher::MatcherError),

    /// Independent evidence grounding failed. Exit code `2`.
    #[error("{0}")]
    Grounding(#[from] grounding::GroundingError),

    /// Running a benchmark suite failed. Exit code `2`.
    #[error("{0}")]
    Benchmark(#[from] benchmark_harness::BenchmarkError),

    /// Serializing a report to its requested export format failed.
    /// Exit code `2`.
    #[error("failed to render report as {format}: {reason}")]
    Export {
        /// Which output format was being rendered.
        format: &'static str,
        /// A human-readable description of the underlying failure.
        reason: String,
    },

    /// `validate-pattern`, `doctor`, or `format-pattern --check` ran
    /// successfully and produced a full report, but that report found
    /// at least one failing check. The report itself has already been
    /// written; this only controls the process exit status (so CI can
    /// detect the failure) — mirroring `cargo fmt --check`'s and
    /// `cargo clippy -D warnings`' own "ran fine, found problems"
    /// exit-code convention. Exit code `1`: like [`Self::Usage`], this
    /// is something the invocation's *input* (the pattern under
    /// review) needs to change, not an engine failure.
    #[error("{0}")]
    ChecksFailed(String),
}

impl CliError {
    /// The process exit code this error should produce — see this
    /// module's own docs for the full convention.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Usage(_) | Self::InputPath { .. } | Self::ChecksFailed(_) => 1,
            Self::Dsl(_)
            | Self::Ingestion(_)
            | Self::Matcher(_)
            | Self::Grounding(_)
            | Self::Benchmark(_)
            | Self::Export { .. } => 2,
            Self::OutputWrite { .. } => 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_error_exits_one() {
        let err = CliError::Usage("bad flag".to_string());
        assert_eq!(err.exit_code(), 1);
        assert_eq!(err.to_string(), "bad flag");
    }

    #[test]
    fn input_path_error_exits_one() {
        let err = CliError::InputPath {
            path: PathBuf::from("/nowhere"),
            reason: "not found".to_string(),
        };
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn output_write_error_exits_three() {
        let err = CliError::OutputWrite {
            path: PathBuf::from("/nowhere/out.json"),
            reason: "permission denied".to_string(),
        };
        assert_eq!(err.exit_code(), 3);
    }

    #[test]
    fn export_error_exits_two() {
        let err = CliError::Export {
            format: "json",
            reason: "boom".to_string(),
        };
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn checks_failed_exits_one() {
        let err = CliError::ChecksFailed("2 issue(s) found".to_string());
        assert_eq!(err.exit_code(), 1);
        assert_eq!(err.to_string(), "2 issue(s) found");
    }
}

//! [`DslError`]: the crate's single exhaustive error type, per
//! ADR-0003 (every library crate defines its own `thiserror`-derived
//! `Error` enum in `src/error.rs`).

use std::path::PathBuf;

use crate::diagnostics::Diagnostics;

/// Everything that can cause pattern loading, parsing, validation, or
/// compilation to fail.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum DslError {
    /// Reading a pattern file from disk failed.
    #[error("failed to read pattern file `{path}`: {reason}")]
    Io {
        /// The path that could not be read.
        path: PathBuf,
        /// A human-readable description of the underlying I/O failure.
        reason: String,
    },

    /// The pattern source was not lexically or structurally valid. See
    /// the contained [`Diagnostics`] for full detail (location,
    /// offending construct, reason, suggestion) on every problem found.
    #[error("pattern failed to parse ({} diagnostic(s))", .0.len())]
    Parse(Diagnostics),

    /// The pattern parsed successfully but failed semantic validation
    /// (duplicate identifiers, invalid references, incompatible
    /// predicates, etc.). See the contained [`Diagnostics`] for detail.
    #[error("pattern failed validation ({} diagnostic(s))", .0.len())]
    Validation(Diagnostics),
}

impl DslError {
    /// Render every contained diagnostic as a human-readable report
    /// against `source`. Returns a single-line summary for
    /// [`DslError::Io`], which carries no source-anchored diagnostics.
    #[must_use]
    pub fn render(&self, source: &str) -> String {
        match self {
            Self::Io { .. } => self.to_string(),
            Self::Parse(diags) | Self::Validation(diags) => diags
                .iter()
                .map(|d| d.render(source))
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

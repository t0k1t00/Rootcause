//! Rich diagnostics shared by the lexer, parser, and validator.
//!
//! A single [`Diagnostic`] type is used across all three stages rather
//! than one error type per stage: callers (the CLI, an editor
//! integration, a test harness) want to collect and render diagnostics
//! uniformly regardless of which stage produced them, and the four
//! required fields — location, offending construct, reason, suggestion —
//! are the same shape at every stage.

use std::fmt;

use crate::span::Span;

/// How serious a diagnostic is. Only [`Severity::Error`] diagnostics
/// prevent a pattern from being usable; [`Severity::Warning`]
/// diagnostics are informational (e.g. an unusual but not invalid
/// severity/tag combination).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The pattern is invalid and cannot be compiled.
    Error,
    /// The pattern is valid but suspicious; compilation proceeds.
    Warning,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Error => "error",
            Self::Warning => "warning",
        })
    }
}

/// One diagnostic message, carrying everything needed to point a human
/// (or an editor) at the exact problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Error vs. warning.
    pub severity: Severity,
    /// Where in the source the problem was found.
    pub location: Span,
    /// A short name for the offending construct, e.g. `"pattern
    /// metadata"`, `"evidence declaration ``donation_transfer``"`,
    /// `"constraint expression"`.
    pub offending_construct: String,
    /// Why this is a problem, in a full sentence.
    pub reason: String,
    /// An optional actionable suggestion, e.g. `"did you mean
    /// `required`?"`.
    pub suggestion: Option<String>,
}

impl Diagnostic {
    /// Construct an error-severity diagnostic.
    #[must_use]
    pub fn error(
        location: Span,
        offending_construct: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Error,
            location,
            offending_construct: offending_construct.into(),
            reason: reason.into(),
            suggestion: None,
        }
    }

    /// Construct a warning-severity diagnostic.
    #[must_use]
    pub fn warning(
        location: Span,
        offending_construct: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Warning,
            location,
            offending_construct: offending_construct.into(),
            reason: reason.into(),
            suggestion: None,
        }
    }

    /// Attach a suggestion, builder-style.
    #[must_use]
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Render this diagnostic as a human-readable, single-block message
    /// against `source`, including a `line:column` location.
    #[must_use]
    pub fn render(&self, source: &str) -> String {
        let (line, col) = self.location.line_col(source);
        let mut out = format!(
            "{}: {} at {line}:{col} ({}): {}",
            self.severity, self.offending_construct, self.location, self.reason
        );
        if let Some(suggestion) = &self.suggestion {
            out.push_str("\n  suggestion: ");
            out.push_str(suggestion);
        }
        out
    }
}

/// A non-empty, ordered collection of [`Diagnostic`]s produced by a
/// failed parse or validation pass.
///
/// Kept distinct from `Vec<Diagnostic>` so the public API can guarantee
/// "if you got a `Diagnostics`, at least one entry is an error" rather
/// than every caller having to re-check for emptiness.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Diagnostics(Vec<Diagnostic>);

impl Diagnostics {
    /// An empty diagnostic set.
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// Add a diagnostic.
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.0.push(diagnostic);
    }

    /// Merge another diagnostic set into this one.
    pub fn extend(&mut self, other: Self) {
        self.0.extend(other.0);
    }

    /// Whether any diagnostic in this set is [`Severity::Error`]
    /// (as opposed to only containing warnings).
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.0.iter().any(|d| d.severity == Severity::Error)
    }

    /// Whether this set contains no diagnostics at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The number of diagnostics in this set.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Iterate over the contained diagnostics in the order they were
    /// pushed.
    pub fn iter(&self) -> std::slice::Iter<'_, Diagnostic> {
        self.0.iter()
    }
}

impl IntoIterator for Diagnostics {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Diagnostics {
    type Item = &'a Diagnostic;
    type IntoIter = std::slice::Iter<'a, Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl FromIterator<Diagnostic> for Diagnostics {
    fn from_iter<T: IntoIterator<Item = Diagnostic>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_errors_false_for_only_warnings() {
        let mut diags = Diagnostics::new();
        diags.push(Diagnostic::warning(Span::start_of_file(), "x", "y"));
        assert!(!diags.has_errors());
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn has_errors_true_when_any_error_present() {
        let mut diags = Diagnostics::new();
        diags.push(Diagnostic::warning(Span::start_of_file(), "x", "y"));
        diags.push(Diagnostic::error(Span::start_of_file(), "x", "z"));
        assert!(diags.has_errors());
    }

    #[test]
    fn render_includes_suggestion_when_present() {
        let diag = Diagnostic::error(Span::new(0, 1), "token", "unexpected `@`")
            .with_suggestion("remove the stray character");
        let rendered = diag.render("@abc");
        assert!(rendered.contains("unexpected `@`"));
        assert!(rendered.contains("remove the stray character"));
    }
}

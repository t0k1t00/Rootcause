//! Source positions and spans, shared by the lexer, parser, and
//! diagnostics.
//!
//! Positions are byte offsets, not `(line, col)` pairs: the lexer already
//! walks the source once and can record byte offsets for free, while
//! computing line/column requires either a second pass or bookkeeping on
//! every character. [`Span::line_col`] does that translation lazily, only
//! when a diagnostic actually needs to be rendered for a human.
//!
//! Offsets are stored as `u32`, not `usize`, for the same reason
//! `fact-model`'s ID newtypes are `u32`-backed (see
//! `fact_model::ids`): a single parsed pattern *definition* file is
//! bounded by realistic source size (far short of 4 GiB), `u32` keeps
//! [`Span`] `Copy` and cheap, and it keeps spans platform-independent.
//! Every `usize`-to-`u32` cast in this module and [`crate::lexer`] is
//! therefore an intentional, source-size-bounded narrowing, allowed at
//! module scope rather than silenced call-by-call.
#![allow(clippy::cast_possible_truncation)]

use std::fmt;

/// A single point in source text, as a byte offset from the start of the
/// input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Position(pub u32);

/// A contiguous byte range `[start, end)` in the original source text.
///
/// `end` is exclusive, matching Rust's own slice-indexing convention, so
/// `&source[span.start as usize..span.end as usize]` always recovers the
/// exact source text the span covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    /// Byte offset of the first byte covered by this span.
    pub start: Position,
    /// Byte offset one past the last byte covered by this span.
    pub end: Position,
}

impl Span {
    /// Construct a span from raw byte offsets.
    #[must_use]
    pub const fn new(start: u32, end: u32) -> Self {
        Self {
            start: Position(start),
            end: Position(end),
        }
    }

    /// A zero-width span at the very start of the input, used as a
    /// fallback when no more specific location is available (e.g. "file
    /// is empty").
    #[must_use]
    pub const fn start_of_file() -> Self {
        Self::new(0, 0)
    }

    /// The smallest span that covers both `self` and `other`.
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        Self {
            start: Position(self.start.0.min(other.start.0)),
            end: Position(self.end.0.max(other.end.0)),
        }
    }

    /// Extract the exact source text this span covers.
    ///
    /// Returns `None` if the span's offsets fall outside `source` or do
    /// not land on a UTF-8 character boundary (which should never happen
    /// for a span produced by [`crate::lexer::tokenize`] against the same
    /// source it was constructed from, but callers passing mismatched
    /// spans/sources should get `None`, not a panic).
    #[must_use]
    pub fn text(self, source: &str) -> Option<&str> {
        source.get(self.start.0 as usize..self.end.0 as usize)
    }

    /// Translate this span's start offset into a 1-indexed `(line,
    /// column)` pair against `source`, for human-readable diagnostics.
    #[must_use]
    pub fn line_col(self, source: &str) -> (u32, u32) {
        let mut line = 1u32;
        let mut col = 1u32;
        for (offset, ch) in source.char_indices() {
            if offset as u32 >= self.start.0 {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start.0, self.end.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_extracts_exact_slice() {
        let src = "pattern foo";
        let span = Span::new(0, 7);
        assert_eq!(span.text(src), Some("pattern"));
    }

    #[test]
    fn text_out_of_bounds_is_none() {
        let src = "abc";
        let span = Span::new(0, 100);
        assert_eq!(span.text(src), None);
    }

    #[test]
    fn merge_takes_widest_bounds() {
        let a = Span::new(5, 10);
        let b = Span::new(2, 7);
        assert_eq!(a.merge(b), Span::new(2, 10));
    }

    #[test]
    fn line_col_tracks_newlines() {
        let src = "a\nbc\ndef";
        // 'd' is at offset 5, on line 3, column 1.
        let span = Span::new(5, 6);
        assert_eq!(span.line_col(src), (3, 1));
    }
}

//! `rootcause format-pattern`: pretty-print a `.rcdsl` file using
//! [`crate::format`]'s canonical style.
//!
//! See [`crate::format`]'s module docs for why only the
//! `pattern ... { ... }` body is re-printed, and the leading
//! comment/blank-line block is preserved byte-for-byte.

use std::path::Path;

use crate::error::CliError;
use crate::format::format_pattern_body;

/// The output of formatting: whether `path` was already canonically
/// formatted, and the fully rendered (comment-preserving) text.
#[derive(Debug)]
pub struct FormatOutcome {
    /// `true` if `formatted` is byte-identical to the original source.
    pub already_formatted: bool,
    /// The formatted source (leading comments preserved verbatim,
    /// followed by the re-printed pattern body).
    pub formatted: String,
}

/// Format the pattern at `path`.
///
/// # Errors
/// - [`CliError::InputPath`] if `path` cannot be read.
/// - [`CliError::Dsl`] if `path`'s content does not parse (formatting
///   requires a valid AST; run `validate-pattern` first for a full
///   diagnostic).
pub fn format_file(path: &Path) -> Result<FormatOutcome, CliError> {
    let source = std::fs::read_to_string(path).map_err(|e| CliError::InputPath {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;

    let (leading, body_source) = split_leading_comments(&source);
    let ast = dsl::parse_pattern(body_source)?;
    let formatted_body = format_pattern_body(&ast);

    let mut formatted = String::new();
    formatted.push_str(leading);
    formatted.push_str(&formatted_body);

    Ok(FormatOutcome {
        already_formatted: formatted == source,
        formatted,
    })
}

/// Split `source` into a leading block of comment (`//...`) and blank
/// lines, and everything from the first non-comment, non-blank line
/// onward (expected to start with `pattern`).
///
/// This is deliberately line-oriented, not lexer-based: the DSL lexer
/// treats comments as trivia and never surfaces them (see
/// [`crate::format`]'s module docs), so recovering them verbatim means
/// working from the raw source text directly, before any DSL parsing
/// happens.
fn split_leading_comments(source: &str) -> (&str, &str) {
    let mut byte_offset = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            byte_offset += line.len();
        } else {
            break;
        }
    }
    source.split_at(byte_offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_file(label: &str, content: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "rootcause-cli-format-pattern-test-{}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        let path = dir.join(format!("{label}.rcdsl"));
        std::fs::write(&path, content).expect("write scratch file");
        path
    }

    const MESSY: &str = "// A doc comment explaining the pattern.\n// Second line.\n\npattern trivial version 1 {\n  family: Reentrancy\n  severity:Low\n  evidence { required c: call(kind: External) }\n  constraint: c\n}\n";

    #[test]
    fn preserves_leading_comment_block() {
        let path = scratch_file("messy", MESSY);
        let outcome = format_file(&path).expect("should format");
        assert!(outcome
            .formatted
            .starts_with("// A doc comment explaining the pattern.\n// Second line.\n\n"));
    }

    #[test]
    fn reports_already_formatted_files_as_such() {
        let path = scratch_file("already", MESSY);
        let first = format_file(&path).expect("should format");
        let path2 = scratch_file("already-2", &first.formatted);
        let second = format_file(&path2).expect("should format again");
        assert!(second.already_formatted);
        assert_eq!(second.formatted, first.formatted);
    }

    #[test]
    fn formatted_output_still_compiles() {
        let path = scratch_file("compiles", MESSY);
        let outcome = format_file(&path).expect("should format");
        dsl::compile_str(&outcome.formatted).expect("formatted pattern must still compile");
    }

    #[test]
    fn malformed_pattern_is_a_dsl_error() {
        let path = scratch_file("bad", "not a valid pattern");
        let err = format_file(&path).expect_err("must fail");
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn split_leading_comments_handles_no_comments() {
        let (leading, body) = split_leading_comments("pattern x version 1 {}\n");
        assert!(leading.is_empty());
        assert_eq!(body, "pattern x version 1 {}\n");
    }
}

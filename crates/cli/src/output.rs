//! Output format selection and the single place that decides whether
//! rendered report text goes to a file (`--output`) or stdout.

use std::fmt;
use std::fs;
use std::io::Write as _;
use std::path::Path;

use crate::error::CliError;

/// The output format a command was asked to render in.
///
/// `Csv` is only meaningful for `rootcause benchmark` (see
/// [`crate::commands::benchmark`]); `rootcause analyze` rejects it as a
/// [`CliError::Usage`] rather than silently falling back to another
/// format, since a silently-substituted format is a worse surprise than
/// an explicit error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "lower")]
pub enum Format {
    /// Human-readable console output (the default).
    Human,
    /// Machine-readable JSON.
    Json,
    /// Markdown, suitable for pasting into an issue or report.
    Markdown,
    /// Comma-separated values (`rootcause benchmark` only).
    Csv,
}

impl Default for Format {
    fn default() -> Self {
        Self::Human
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Human => "human",
            Self::Json => "json",
            Self::Markdown => "markdown",
            Self::Csv => "csv",
        })
    }
}

/// A minimal leveled logger for progress/diagnostic messages, kept
/// entirely separate from a command's actual report output (see
/// [`write_report`]): progress messages always go to stderr, so stdout
/// stays reserved for exactly the report text a caller may want to pipe
/// or redirect.
#[derive(Debug, Clone, Copy)]
pub struct Logger {
    quiet: bool,
    verbose: bool,
}

impl Logger {
    /// Build a logger from the CLI's global `--quiet`/`--verbose`
    /// flags. `quiet` takes precedence: `--quiet --verbose` together
    /// prints nothing extra, since silence was explicitly requested.
    #[must_use]
    pub const fn new(quiet: bool, verbose: bool) -> Self {
        Self { quiet, verbose }
    }

    /// Print a normal informational line to stderr, unless `--quiet`
    /// was given.
    pub fn info(self, message: impl fmt::Display) {
        if !self.quiet {
            eprintln!("{message}");
        }
    }

    /// Print a detailed progress line to stderr, only when `--verbose`
    /// was given and `--quiet` was not.
    pub fn verbose(self, message: impl fmt::Display) {
        if self.verbose && !self.quiet {
            eprintln!("{message}");
        }
    }
}

/// Write already-rendered report text to `output` if given, otherwise
/// to stdout.
///
/// This is the single chokepoint every command's rendered output passes
/// through, so `--output`/`--quiet` behave identically for `analyze`
/// and `benchmark` rather than each command reimplementing its own
/// file-vs-stdout logic.
///
/// # Errors
/// Returns [`CliError::OutputWrite`] if `output` is `Some` and the file
/// cannot be created or written.
pub fn write_report(content: &str, output: Option<&Path>, logger: Logger) -> Result<(), CliError> {
    let Some(path) = output else {
        println!("{content}");
        return Ok(());
    };

    let mut file = fs::File::create(path).map_err(|e| CliError::OutputWrite {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;
    file.write_all(content.as_bytes())
        .map_err(|e| CliError::OutputWrite {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;
    logger.info(format_args!("Wrote report to {}", path.display()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_display_matches_value_enum_names() {
        assert_eq!(Format::Human.to_string(), "human");
        assert_eq!(Format::Json.to_string(), "json");
        assert_eq!(Format::Markdown.to_string(), "markdown");
        assert_eq!(Format::Csv.to_string(), "csv");
    }

    #[test]
    fn default_format_is_human() {
        assert_eq!(Format::default(), Format::Human);
    }

    #[test]
    fn write_report_to_file_round_trips_content() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("rootcause_cli_test_{}.out", std::process::id()));
        let logger = Logger::new(true, false);
        write_report("hello output", Some(&path), logger).expect("write should succeed");
        let read_back = std::fs::read_to_string(&path).expect("file should exist");
        assert_eq!(read_back, "hello output");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_report_to_missing_directory_errors() {
        let path = std::path::PathBuf::from("/nonexistent-rootcause-dir/out.json");
        let logger = Logger::new(true, false);
        let err = write_report("x", Some(&path), logger).expect_err("must fail");
        assert_eq!(err.exit_code(), 3);
    }
}

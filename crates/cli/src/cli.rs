//! Command-line argument and subcommand definitions.
//!
//! This module only *describes* the CLI surface (`clap::Parser`
//! derives); it contains no pipeline logic of its own — see
//! [`crate::commands`] for that. Keeping the two separate means the
//! argument grammar can be inspected, tested (see
//! [`Cli::command().debug_assert()`] in this module's own tests), and
//! changed without touching orchestration code, and vice versa.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::output::Format;

/// Root Cause: deterministic, evidence-grounded smart-contract exploit
/// analysis.
#[derive(Debug, Parser)]
#[command(
    name = "rootcause",
    version,
    about,
    long_about = None,
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Which command to run. `None` means no subcommand was given —
    /// [`crate::run`] treats that as a request for a short usage
    /// summary, not an error (see that function's own docs).
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Write the rendered report to FILE instead of stdout.
    #[arg(long, global = true, value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Output format for the rendered report.
    #[arg(long, global = true, value_enum)]
    pub format: Option<Format>,

    /// Suppress non-essential output (progress messages, confirmations).
    /// The report itself is still printed/written.
    #[arg(long, global = true)]
    pub quiet: bool,

    /// Print detailed progress information for each pipeline stage to
    /// stderr. Ignored if `--quiet` is also given.
    #[arg(long, global = true)]
    pub verbose: bool,
}

/// Every `rootcause` subcommand.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the full pipeline (ingestion → matcher → grounding →
    /// taxonomy) against one trace and report every finding.
    Analyze(AnalyzeArgs),

    /// Run a benchmark suite through `benchmark-harness` and report its
    /// metrics.
    Benchmark(BenchmarkArgs),

    /// Print version information for `rootcause` and every engine crate
    /// it links.
    Version,

    /// Print a shell completion script for the given shell to stdout.
    ///
    /// Redirect the output to wherever your shell loads completions
    /// from, e.g. `rootcause completions bash > /etc/bash_completion.d/rootcause`
    /// or `rootcause completions zsh > ~/.zfunc/_rootcause`. See
    /// `README.md`'s "Shell completions" section for shell-specific
    /// install locations.
    Completions(CompletionsArgs),

    /// Print detailed usage information (equivalent to `--help`).
    Help,

    /// Scaffold a new pattern: a starter `.rcdsl` file, a positive and
    /// negative demo trace pair, and a README template.
    NewPattern(NewPatternArgs),

    /// Run the full authoring-validation pipeline (parse, validate,
    /// compile, taxonomy lookup, and — if demo traces are available —
    /// match/ground) against one pattern file.
    ValidatePattern(ValidatePatternArgs),

    /// Pretty-print a `.rcdsl` pattern file using the repository's
    /// canonical style.
    FormatPattern(FormatPatternArgs),

    /// Scan the pattern library for authoring mistakes: duplicate
    /// pattern ids, missing taxonomy mappings, broken demo references,
    /// and similar cross-file inconsistencies.
    Doctor(DoctorArgs),
}

/// Arguments for `rootcause completions`.
#[derive(Debug, Parser)]
pub struct CompletionsArgs {
    /// Which shell to generate a completion script for.
    pub shell: clap_complete::Shell,
}

/// Arguments for `rootcause analyze`.
#[derive(Debug, Parser)]
pub struct AnalyzeArgs {
    /// Path to the trace file to analyze (raw archive-node-style JSON;
    /// see the `ingestion` crate's documented schema).
    pub trace: PathBuf,

    /// Path to a pattern file (a single `.rcdsl` file) or a directory
    /// containing one or more `.rcdsl` pattern files to match against
    /// the trace.
    #[arg(long, value_name = "PATH")]
    pub patterns: PathBuf,
}

/// Arguments for `rootcause benchmark`.
#[derive(Debug, Parser)]
pub struct BenchmarkArgs {
    /// Path to a benchmark suite: either a single case-definition JSON
    /// file, or a directory of them (see
    /// `benchmark_harness::fixtures::load_suite_from_dir`'s documented
    /// schema).
    pub suite: PathBuf,
}

/// Arguments for `rootcause new-pattern`.
#[derive(Debug, Parser)]
pub struct NewPatternArgs {
    /// The new pattern's identifier, e.g. `my_new_pattern` (`snake_case`;
    /// becomes the `.rcdsl` filename and the DSL `pattern` name).
    pub name: String,

    /// Workspace root to scaffold into. Defaults to the current
    /// directory; the generated files are placed at
    /// `<root>/patterns/<name>.rcdsl` and
    /// `<root>/demo/<name>_{positive,negative}.json`, matching every
    /// existing pattern/demo pair's layout.
    #[arg(long, value_name = "DIR", default_value = ".")]
    pub dir: PathBuf,

    /// The pattern's exploit family (`family:` metadata). Defaults to a
    /// placeholder the author is expected to replace.
    #[arg(long, value_name = "NAME")]
    pub family: Option<String>,

    /// Overwrite any files that already exist at the target paths.
    #[arg(long)]
    pub force: bool,
}

/// Arguments for `rootcause validate-pattern`.
#[derive(Debug, Parser)]
pub struct ValidatePatternArgs {
    /// Path to the `.rcdsl` pattern file to validate.
    pub pattern: PathBuf,

    /// Path to a trace the pattern is expected to fire (`Grounded`) on.
    /// If omitted, `validate-pattern` looks for
    /// `demo/<pattern-name>_positive.json` next to the pattern's
    /// workspace root.
    #[arg(long, value_name = "FILE")]
    pub positive: Option<PathBuf>,

    /// Path to a trace the pattern is expected *not* to fire on. If
    /// omitted, `validate-pattern` looks for
    /// `demo/<pattern-name>_negative.json`.
    #[arg(long, value_name = "FILE")]
    pub negative: Option<PathBuf>,
}

/// Arguments for `rootcause format-pattern`.
#[derive(Debug, Parser)]
pub struct FormatPatternArgs {
    /// Path to the `.rcdsl` pattern file to format.
    pub pattern: PathBuf,

    /// Write the formatted output back to `pattern` instead of printing
    /// it to stdout.
    #[arg(long)]
    pub write: bool,

    /// Exit with a non-zero status if `pattern` is not already
    /// canonically formatted, without writing anything (for CI checks;
    /// mirrors `cargo fmt --check`). Conflicts with `--write`.
    #[arg(long, conflicts_with = "write")]
    pub check: bool,
}

/// Arguments for `rootcause doctor`.
#[derive(Debug, Parser)]
pub struct DoctorArgs {
    /// Directory containing `.rcdsl` pattern files to scan.
    #[arg(long, value_name = "DIR", default_value = "patterns")]
    pub patterns_dir: PathBuf,

    /// Directory containing demo trace files referenced by patterns by
    /// naming convention (`<pattern>_positive.json` /
    /// `<pattern>_negative.json`).
    #[arg(long, value_name = "DIR", default_value = "demo")]
    pub demo_dir: PathBuf,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory as _;

    use super::*;

    /// Clap's own internal consistency check (conflicting arg ids,
    /// invalid `value_enum` derives, etc.) — cheap and catches a whole
    /// class of "compiles fine, panics at startup" bugs.
    #[test]
    fn cli_definition_is_internally_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_analyze_with_required_patterns_flag() {
        let cli = Cli::try_parse_from([
            "rootcause",
            "analyze",
            "trace.json",
            "--patterns",
            "patterns/",
        ])
        .expect("should parse");
        match cli.command {
            Some(Command::Analyze(args)) => {
                assert_eq!(args.trace, PathBuf::from("trace.json"));
                assert_eq!(args.patterns, PathBuf::from("patterns/"));
            }
            other => panic!("expected Analyze, got {other:?}"),
        }
    }

    #[test]
    fn analyze_without_patterns_flag_is_a_parse_error() {
        let result = Cli::try_parse_from(["rootcause", "analyze", "trace.json"]);
        assert!(result.is_err());
    }

    #[test]
    fn parses_benchmark_with_suite_path() {
        let cli = Cli::try_parse_from(["rootcause", "benchmark", "suite/"]).expect("should parse");
        match cli.command {
            Some(Command::Benchmark(args)) => assert_eq!(args.suite, PathBuf::from("suite/")),
            other => panic!("expected Benchmark, got {other:?}"),
        }
    }

    #[test]
    fn parses_global_flags_after_subcommand() {
        let cli = Cli::try_parse_from([
            "rootcause",
            "analyze",
            "trace.json",
            "--patterns",
            "p.rcdsl",
            "--format",
            "json",
            "--quiet",
            "--output",
            "out.json",
        ])
        .expect("should parse");
        assert_eq!(cli.format, Some(Format::Json));
        assert!(cli.quiet);
        assert_eq!(cli.output, Some(PathBuf::from("out.json")));
    }

    #[test]
    fn parses_global_flags_before_subcommand() {
        let cli = Cli::try_parse_from(["rootcause", "--verbose", "benchmark", "suite/"])
            .expect("should parse");
        assert!(cli.verbose);
    }

    #[test]
    fn no_subcommand_parses_to_none() {
        let cli = Cli::try_parse_from(["rootcause"]).expect("should parse");
        assert!(cli.command.is_none());
    }

    #[test]
    fn version_and_help_subcommands_parse() {
        let cli = Cli::try_parse_from(["rootcause", "version"]).expect("should parse");
        assert!(matches!(cli.command, Some(Command::Version)));
        let cli = Cli::try_parse_from(["rootcause", "help"]).expect("should parse");
        assert!(matches!(cli.command, Some(Command::Help)));
    }

    #[test]
    fn completions_subcommand_parses_a_known_shell() {
        let cli = Cli::try_parse_from(["rootcause", "completions", "bash"]).expect("should parse");
        match cli.command {
            Some(Command::Completions(CompletionsArgs { shell })) => {
                assert_eq!(shell, clap_complete::Shell::Bash);
            }
            other => panic!("expected Completions, got {other:?}"),
        }
    }

    #[test]
    fn completions_subcommand_rejects_an_unknown_shell() {
        let result = Cli::try_parse_from(["rootcause", "completions", "not-a-shell"]);
        assert!(result.is_err());
    }
}

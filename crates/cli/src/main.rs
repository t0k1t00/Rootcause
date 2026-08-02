//! Command-line entry point for Root Cause.
//!
//! Binary name is `rootcause` (set via `[[bin]] name` in `Cargo.toml`,
//! independent of the crate's package name `cli`).
//!
//! ## Status
//!
//! This is a `[[bin]]`-only crate (no `[lib]` target — see
//! `Cargo.toml`), by design: the `rootcause` binary is the workspace's
//! single public interface, not a library other crates depend on (see
//! `integration-tests/Cargo.toml`'s own note on why it locates the
//! compiled binary by path rather than as a dependency). Every module
//! below is therefore private to this binary.
//!
//! `rootcause` orchestrates the already-implemented pipeline —
//! `ingestion → matcher → grounding → taxonomy` for `analyze`,
//! `benchmark-harness` for `benchmark` — and duplicates no logic any of
//! those crates already implements; see [`commands::analyze`] and
//! [`commands::benchmark`] for the two commands' orchestration, and
//! [`cli`] for the argument grammar.

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

mod cli;
mod commands;
mod error;
mod format;
mod output;
mod patterns;

use std::process::ExitCode;

use clap::{CommandFactory as _, Parser as _};

use crate::cli::{
    AnalyzeArgs, BenchmarkArgs, Cli, Command, CompletionsArgs, DoctorArgs, FormatPatternArgs,
    NewPatternArgs, ValidatePatternArgs,
};
use crate::error::CliError;
use crate::output::Logger;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(err.exit_code())
        }
    }
}

/// Dispatch a parsed [`Cli`] to the requested command, writing its
/// rendered output via [`output::write_report`].
///
/// No subcommand (`rootcause` alone) is not an error: it prints a short
/// usage summary to stdout and succeeds, matching the convention most
/// CLIs use for "did you mean to pass `--help`?" without actually
/// failing a bare invocation — see
/// `integration-tests/tests/workspace_smoke.rs`'s
/// `rootcause_binary_runs_successfully`, which specifically checks a
/// no-argument invocation exits successfully.
fn run(cli: Cli) -> Result<(), CliError> {
    let logger = Logger::new(cli.quiet, cli.verbose);
    let format = cli.format.unwrap_or_default();

    let Some(command) = cli.command else {
        print_short_usage();
        return Ok(());
    };

    match command {
        Command::Analyze(AnalyzeArgs { trace, patterns }) => {
            let rendered = commands::analyze::run(&trace, &patterns, format, logger)?;
            output::write_report(&rendered, cli.output.as_deref(), logger)
        }
        Command::Benchmark(BenchmarkArgs { suite }) => {
            let rendered = commands::benchmark::run(&suite, format, logger)?;
            output::write_report(&rendered, cli.output.as_deref(), logger)
        }
        Command::Version => {
            println!("{}", commands::version_info());
            Ok(())
        }
        Command::Completions(CompletionsArgs { shell }) => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "rootcause",
                &mut std::io::stdout(),
            );
            Ok(())
        }
        Command::Help => {
            let _ = Cli::command().print_long_help();
            Ok(())
        }
        other => run_pattern_sdk_command(other, cli.output.as_deref(), logger),
    }
}

/// Dispatch the four `new-pattern`/`validate-pattern`/`format-pattern`/
/// `doctor` "pattern SDK" subcommands. Split out from [`run`] purely to
/// stay under `clippy::too_many_lines` on that function — there is no
/// other reason these four aren't inline arms of the same `match`.
///
/// # Panics
/// Never, in practice: only ever called with a [`Command`] variant one
/// of the four match arms below covers (see [`run`]'s `other =>` arm).
fn run_pattern_sdk_command(
    command: Command,
    output: Option<&std::path::Path>,
    logger: Logger,
) -> Result<(), CliError> {
    match command {
        Command::NewPattern(NewPatternArgs {
            name,
            dir,
            family,
            force,
        }) => {
            let rendered = commands::new_pattern::run(&name, &dir, family.as_deref(), force)?;
            output::write_report(&rendered, output, logger)
        }
        Command::ValidatePattern(ValidatePatternArgs {
            pattern,
            positive,
            negative,
        }) => {
            let (rendered, passed) = commands::validate_pattern::run(
                &pattern,
                positive.as_deref(),
                negative.as_deref(),
            )?;
            output::write_report(&rendered, output, logger)?;
            if passed {
                Ok(())
            } else {
                Err(CliError::ChecksFailed(format!(
                    "validate-pattern found failing check(s) for `{}`",
                    pattern.display()
                )))
            }
        }
        Command::FormatPattern(FormatPatternArgs {
            pattern,
            write,
            check,
        }) => {
            let outcome = commands::format_pattern::format_file(&pattern)?;
            if check {
                return if outcome.already_formatted {
                    Ok(())
                } else {
                    Err(CliError::ChecksFailed(format!(
                        "`{}` is not canonically formatted; run `rootcause format-pattern {} --write`",
                        pattern.display(),
                        pattern.display()
                    )))
                };
            }
            if write {
                std::fs::write(&pattern, &outcome.formatted).map_err(|e| {
                    CliError::OutputWrite {
                        path: pattern.clone(),
                        reason: e.to_string(),
                    }
                })?;
                logger.info(format_args!("Formatted {}", pattern.display()));
                Ok(())
            } else {
                output::write_report(&outcome.formatted, output, logger)
            }
        }
        Command::Doctor(DoctorArgs {
            patterns_dir,
            demo_dir,
        }) => {
            let (rendered, ok) = commands::doctor::run(&patterns_dir, &demo_dir)?;
            output::write_report(&rendered, output, logger)?;
            if ok {
                Ok(())
            } else {
                Err(CliError::ChecksFailed(
                    "doctor found one or more issues".to_string(),
                ))
            }
        }
        Command::Analyze(_)
        | Command::Benchmark(_)
        | Command::Version
        | Command::Completions(_)
        | Command::Help => {
            unreachable!("run() only delegates NewPattern/ValidatePattern/FormatPattern/Doctor")
        }
    }
}

fn print_short_usage() {
    println!("{}", commands::version_info());
    println!();
    println!("Usage: rootcause <COMMAND> [OPTIONS]");
    println!();
    println!("Commands:");
    println!("  analyze          Run the full pipeline against one trace");
    println!("  benchmark        Run a benchmark suite and report its metrics");
    println!("  new-pattern      Scaffold a new pattern and its demo traces");
    println!("  validate-pattern Run every authoring check against one pattern");
    println!("  format-pattern   Pretty-print a pattern in the canonical style");
    println!("  doctor           Scan the pattern library for cross-file issues");
    println!("  version          Print version information");
    println!("  completions <SHELL>  Print a shell completion script");
    println!("  help             Print detailed usage information");
    println!();
    println!("Run `rootcause help` or `rootcause --help` for full details.");
}

#[cfg(test)]
mod tests {
    use clap::ValueEnum as _;

    use super::*;

    #[test]
    fn no_command_succeeds() {
        let cli = Cli::try_parse_from(["rootcause"]).expect("should parse");
        assert!(run(cli).is_ok());
    }

    #[test]
    fn version_command_succeeds() {
        let cli = Cli::try_parse_from(["rootcause", "version"]).expect("should parse");
        assert!(run(cli).is_ok());
    }

    #[test]
    fn completions_command_succeeds_for_every_supported_shell() {
        for shell in clap_complete::Shell::value_variants() {
            let cli = Cli::try_parse_from(["rootcause", "completions", &shell.to_string()])
                .expect("should parse");
            assert!(run(cli).is_ok(), "completions for {shell} should succeed");
        }
    }

    #[test]
    fn help_command_succeeds() {
        let cli = Cli::try_parse_from(["rootcause", "help"]).expect("should parse");
        assert!(run(cli).is_ok());
    }

    #[test]
    fn analyze_with_missing_trace_returns_pipeline_error() {
        let cli = Cli::try_parse_from([
            "rootcause",
            "--quiet",
            "analyze",
            "/nonexistent/trace.json",
            "--patterns",
            "/nonexistent/patterns",
        ])
        .expect("should parse");
        let err = run(cli).expect_err("must fail");
        // Trace ingestion is attempted before pattern loading, so a
        // missing trace surfaces as an ingestion (pipeline) error.
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn benchmark_with_missing_suite_returns_usage_error() {
        let cli = Cli::try_parse_from(["rootcause", "--quiet", "benchmark", "/nonexistent/suite"])
            .expect("should parse");
        let err = run(cli).expect_err("must fail");
        assert_eq!(err.exit_code(), 1);
    }
}

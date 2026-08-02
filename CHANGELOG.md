# Changelog

All notable changes to Root Cause are documented here.

The format loosely follows [Keep a Changelog](https://keepachangelog.com/),
adapted for a project still in active pre-1.0 development.

## [Unreleased]

### Added
- Four new `rootcause` subcommands ("the pattern SDK") that streamline the
  pattern-authoring workflow documented in `docs/PATTERN_AUTHORING_GUIDE.md`.
  No engine, DSL grammar, matcher, grounding, or fact-model changes — every
  command orchestrates existing `dsl`/`taxonomy`/`ingestion`/`matcher`/`grounding`
  infrastructure only:
  - `rootcause new-pattern <NAME>` — scaffold a starter `.rcdsl` file, a
    positive/negative demo trace pair, a `README.md` template, and a
    taxonomy-mapping TODO note.
  - `rootcause validate-pattern <PATTERN>` — run syntax, metadata,
    compilation, taxonomy-mapping, and positive/negative demo-trace checks
    against one pattern in a single command.
  - `rootcause format-pattern <PATTERN>` — pretty-print a `.rcdsl` file's
    body in the repository's canonical style (`--write` to rewrite in
    place, `--check` for CI); leading doc comments are preserved
    byte-for-byte.
  - `rootcause doctor` — scan an entire pattern library for cross-file
    issues a single-pattern check can't catch: duplicate pattern ids,
    unmapped taxonomy families, and missing demo trace files.
- Binary-level integration tests (`integration-tests/tests/pattern_sdk_cli.rs`)
  exercising all four pattern SDK commands against the compiled `rootcause`
  executable, including a full new-pattern → validate-pattern →
  format-pattern workflow and a `doctor` run against the repository's
  own shipped pattern library.
- `Dockerfile` and `.dockerignore` — multi-stage build producing a
  minimal, non-root runtime image with no system library dependencies.
- `docker-compose.yml` — example `docker compose run` usage.
- `.github/workflows/release.yml` — on a `v*` tag push, builds
  `rootcause` for Linux, macOS (x86_64 and arm64), and Windows, packages
  each with the pattern library and a `.sha256` checksum, builds and
  pushes a Docker image to `ghcr.io`, and publishes a GitHub Release
  attaching all of the above.
- `rootcause completions <shell>` subcommand (bash/zsh/fish/powershell/
  elvish), via `clap_complete`.
- README: binary-download, Docker, and shell-completion installation
  options; a version-reporting subsection; a Troubleshooting section.
- `CONTRIBUTING.md` — development setup, validation commands, and PR
  expectations.
- `SECURITY.md` — vulnerability disclosure policy.
- `docs/PATTERN_AUTHORING_GUIDE.md` — a from-scratch, task-oriented guide to
  writing a new `.rcdsl` pattern, aimed at first-time contributors (as
  opposed to `crates/dsl/README.md`, which documents the DSL crate's own
  internals for engine contributors).
- `.github/ISSUE_TEMPLATE/` (bug report, feature/pattern request) and
  `.github/PULL_REQUEST_TEMPLATE.md`.
- An "Installation" section in `README.md` documenting `cargo install
  --path crates/cli`, verified end to end against a clean install root.
- The `log(topic0: ..., decoded: ...)` predicate kind, exposing
  `fact_model::LogEvent` to patterns for the first time.
- The `spoofed_transfer_event` pattern (family `SpoofedTransferEvent`,
  severity High), mapped to SCWE-063 ("Insecure Event Emission").
- The `geth-convert` CLI (`crates/converters`), converting real
  `debug_traceTransaction`/`callTracer` output into Root Cause's fact model.
- The transitive delegatecall-reachable selfdestruct family.
- `parent_kind` on the `call(...)` predicate, and the delegatecall-reachable
  selfdestruct family.
- The top-level `same_call: [a, b, ...]` pattern section, and the
  delegatecall storage collision family.
- `selector:` on the `call(...)` predicate for 4-byte ABI function
  selector matching against raw calldata.
- Canonical fact model, ingestion, Pattern DSL, matcher, grounding, and
  taxonomy crates.
- `CallKind::Selfdestruct` and the unsafe selfdestruct and unchecked
  external call families.
- The exploit corpus, integration test suite, and CLI (`analyze`,
  `benchmark`, `version`).

### Changed
- Generalized `parent_kind` into `ancestor_kind` — a full ancestor-path
  check, not just a one-hop parent check.
- Verified the existing Geth `callTracer` converter against a real,
  publicly reported Erigon `callTracer` response, closing the previously
  unverified "Erigon should also work" inference.

### Fixed
- Top-level `README.md` "Quick start" commands referenced a placeholder
  `trace.json` and a non-existent top-level `fixtures/` directory that would
  fail for anyone copy-pasting them verbatim. Replaced with the bundled
  `demo/dao.json` example and the real, tested benchmark fixture path.

[Unreleased]: https://github.com/rootcause-project/rootcause/compare/main...HEAD

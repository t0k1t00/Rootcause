# benchmark-harness

Orchestrates the complete Root Cause pipeline —
`ingestion -> matcher -> grounding -> taxonomy` — over a corpus of
benchmark cases, and computes the correctness and performance metrics
the Benchmark document requires: precision, recall, F1, a confusion
matrix, per-pattern and per-family statistics, per-stage timings, and a
flat failure summary.

## Design summary

- **A benchmark case names an expectation, not just an input.**
  `types::BenchmarkCase` pairs a trace and one or more compiled patterns
  with what its author expects each pattern to reach (a
  `types::PatternOutcome`) — precision/recall/F1 are meaningless without
  a ground-truth label to compare against.
- **Trace input is pluggable, not just "a JSON file."**
  `types::TraceInput` supports both re-ingesting from raw bytes on every
  run (a file, an in-memory buffer, or anything implementing
  `ingestion::TraceSource`) and a trace already built directly via
  `fact-model`'s arena builder.
- **A case-level or pattern-level failure never aborts a suite run.**
  `runner::run_case` and `runner::run_suite` capture every failure on
  the affected case's own `runner::CaseResult` instead of
  short-circuiting, so one broken fixture in a hundred-case suite still
  yields a complete report for the other ninety-nine.
- **Metrics are a pure function of case results.** `metrics::compute`
  takes `&[runner::CaseResult]` and returns a `metrics::MetricsSummary`
  with no side effects and no hidden state.
- **Export is a pure projection of `report::BenchmarkReport`.**
  `export::to_json`, `export::to_csv`, and `export::to_markdown` all
  read the same report; none maintains its own separate view of the
  data.

## Module map

| Module | Contents |
|---|---|
| `error` | `BenchmarkError`, this crate's single exhaustive error type. |
| `types` | `BenchmarkCase`, `BenchmarkSuite`, `TraceInput`, `PatternOutcome`, `ExpectedFinding`. |
| `timing` | `StageTimings`. |
| `runner` | `run_case`, `run_suite`, `CaseResult`, `PatternResult`: orchestrates the pipeline itself. |
| `metrics` | `compute`, `MetricsSummary`, `ConfusionMatrix`, `BenchmarkMetrics`, `PatternStats`, `FamilyStats`. |
| `report` | `BenchmarkReport`, the strongly-typed top-level report every export format projects. |
| `export` | `to_json`, `to_csv`, `to_markdown`. |
| `fixtures` | Loading cases and suites from JSON benchmark definitions and fixture directories on disk. |
| `sample_fixtures` | Four small, realistic, in-code fixtures (a grounded reentrancy, an abstained oracle-manipulation, a no-match case, and a false-positive structural match caught by grounding) used by this crate's own tests and reusable by downstream callers. |

## Usage

```rust
use benchmark_harness::report::BenchmarkReport;
use benchmark_harness::types::BenchmarkSuite;

let suite = BenchmarkSuite::from_cases(
    "reentrancy-suite",
    vec![benchmark_harness::sample_fixtures::grounded_reentrancy_case()],
);
let report = BenchmarkReport::run(&suite)?;
println!("{}", benchmark_harness::export::to_json(&report)?);
```

See the `cli` crate's `rootcause benchmark` subcommand for a
command-line entry point over this crate.

## Fixture management (policy, not yet implemented)

Local archive-node trace fixtures used for benchmark development are
cached under `.trace-cache/` at the repository root, which is
git-ignored (see top-level `.gitignore`). Raw RPC responses pulled from
live archive nodes must not be committed to the repository wholesale:
only a derived, minimized, and license-cleared fixture set is intended
for version control. The concrete fixture directory layout and corpus
versioning scheme beyond the in-code `sample_fixtures` module and the
JSON schema documented on `fixtures` are not yet decided and will be
recorded as an ADR if and when a real external benchmark corpus is
added.

## Status

Complete. See crate-level rustdoc (`cargo doc -p benchmark-harness
--open`) for the full API reference.

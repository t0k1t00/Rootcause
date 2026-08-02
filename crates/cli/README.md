# `rootcause` CLI

The `rootcause` binary is the public command-line interface to the Root
Cause engine. It orchestrates the already-implemented pipeline —
`ingestion → matcher → grounding → taxonomy` for `analyze`,
`benchmark-harness` for `benchmark` — and implements no analysis logic
of its own.

## Installation / build

```sh
cargo build --release -p cli
# binary at target/release/rootcause
```

## Commands

### `rootcause analyze <TRACE> --patterns <PATH>`

Run the full pipeline against one trace file and report every finding.

- `<TRACE>` — path to a raw trace JSON file (the `ingestion` crate's
  documented archive-node-style schema).
- `--patterns <PATH>` — path to either a single `.rcdsl` pattern file,
  or a directory containing one or more `.rcdsl` files (loaded in
  sorted filename order).

Pipeline: ingest the trace → compile every named pattern → structurally
match every pattern against the trace (`matcher`) → independently
ground every candidate (`grounding`) → map every grounded/abstained/
ungrounded result to external taxonomies (`taxonomy`) → render the
result.

```sh
rootcause analyze trace.json --patterns patterns/reentrancy.rcdsl
rootcause analyze trace.json --patterns patterns/ --format json
rootcause analyze trace.json --patterns patterns/ --format markdown --output report.md
```

`analyze` does not support `--format csv` (there is no meaningful
per-row tabular shape for one trace's findings) and rejects it as a
usage error.

### `rootcause benchmark <SUITE>`

Run a benchmark suite through `benchmark-harness` and report its
correctness/timing metrics.

- `<SUITE>` — either a single case-definition JSON file, or a directory
  of them (see `benchmark_harness::fixtures`' documented schema:
  `{"id", "description", "trace_file", "patterns", "expected"}`).

```sh
rootcause benchmark fixtures/reentrancy_basic/case.json
rootcause benchmark fixtures/ --format markdown --output benchmark.md
rootcause benchmark fixtures/ --format csv --output metrics.csv
```

### `rootcause new-pattern <NAME>`

Scaffold a new pattern: a starter `.rcdsl` file, a positive and negative
demo trace pair, a `README.md` template, and a taxonomy-mapping TODO
note — everything `docs/PATTERN_AUTHORING_GUIDE.md` walks through by
hand, in one command.

- `<NAME>` — the new pattern's identifier (snake_case; becomes the
  `.rcdsl` filename and the DSL `pattern` name).
- `--dir <DIR>` — workspace root to scaffold into (default `.`). Files
  land at `<DIR>/patterns/<NAME>.rcdsl` and
  `<DIR>/demo/<NAME>_{positive,negative}.json`, matching every existing
  pattern/demo pair's layout.
- `--family <NAME>` — the pattern's `family:` metadata. Defaults to a
  placeholder (`TODO_ReplaceWithExploitFamily`) you're expected to
  replace.
- `--force` — overwrite files that already exist at the target paths.

```sh
rootcause new-pattern unsafe_delegatecall_target --family UnsafeDelegatecallTarget
```

### `rootcause validate-pattern <PATTERN>`

Run every check a pattern author needs before opening a PR, in one
command: syntax, metadata validation, compilation, taxonomy mapping,
and — if demo traces are available — that the pattern actually fires on
the positive trace and does not fire on the negative one.

- `<PATTERN>` — path to the `.rcdsl` file to validate.
- `--positive <FILE>` / `--negative <FILE>` — override the demo traces
  used for the last two checks. If omitted, `validate-pattern` looks
  for `demo/<pattern-name>_positive.json` / `..._negative.json` next to
  the pattern's workspace root (the same convention `new-pattern`
  scaffolds).

```sh
rootcause validate-pattern patterns/classic_reentrancy.rcdsl
```

A pattern with a failing check (e.g. unmapped taxonomy, no demo traces
yet) is a valid, reportable outcome, not a crash: the full report is
still printed, and the process exits `1` (`ChecksFailed`) so CI can
detect it.

### `rootcause format-pattern <PATTERN>`

Pretty-print a `.rcdsl` file in the repository's canonical style. Only
the `pattern ... { ... }` body is re-printed; any leading doc-comment
block is preserved byte-for-byte.

- `--write` — write the formatted output back to `<PATTERN>` in place.
- `--check` — exit non-zero if `<PATTERN>` is not already canonically
  formatted, without writing anything (for CI; mirrors
  `cargo fmt --check`). Conflicts with `--write`.
- With neither flag, the formatted text is printed to stdout (or
  `--output`).

```sh
rootcause format-pattern patterns/classic_reentrancy.rcdsl --check
rootcause format-pattern patterns/classic_reentrancy.rcdsl --write
```

### `rootcause doctor`

Scan the whole pattern library for cross-file authoring mistakes that
`validate-pattern` — which only ever looks at one pattern at a time —
can't catch: duplicate pattern ids, patterns with no taxonomy mapping,
and patterns missing their conventional demo trace files.

- `--patterns-dir <DIR>` — directory of `.rcdsl` files to scan (default
  `patterns`).
- `--demo-dir <DIR>` — directory demo traces are expected in, by
  naming convention (default `demo`).

```sh
rootcause doctor
rootcause doctor --patterns-dir patterns --demo-dir demo
```

Like `validate-pattern`, a library with issues is a reportable outcome:
the full report is printed and the process exits `1` if any issue was
found.

### `rootcause version`

Print `rootcause`'s own version and every engine crate's version it was
built against.

### `rootcause completions <SHELL>`

Print a shell completion script to stdout for `bash`, `zsh`, `fish`,
`powershell`, or `elvish`. See the top-level `README.md`'s "Shell
completions" section for install-location examples per shell.

### `rootcause help`

Print detailed usage information (equivalent to `--help`).

### No subcommand

Running `rootcause` with no subcommand prints a short usage summary and
exits successfully (exit code `0`) — it is not treated as an error.

## Global options

These apply to every command and may be given before or after the
subcommand:

| Flag | Description |
|---|---|
| `--output <FILE>` | Write the rendered report to `FILE` instead of stdout. |
| `--format <FORMAT>` | One of `human` (default), `json`, `markdown`, or `csv` (`benchmark` only). |
| `--quiet` | Suppress non-essential output (progress lines, the "wrote to file" confirmation). The report itself is still produced. |
| `--verbose` | Print per-stage progress lines to stderr. Ignored if `--quiet` is also given. |
| `-h`, `--help` | Print help. |
| `-V`, `--version` | Print version. |

Progress/diagnostic messages (`--verbose`, the pass/fail summary line,
the "wrote to file" confirmation) always go to **stderr**. Stdout is
reserved for the rendered report only, so `rootcause analyze ... |
jq .` and similar pipelines work without extra noise.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success. |
| `1` | Usage error — bad arguments, an unreadable/empty patterns path, an unsupported format for the given command. Fixable by changing what was typed. |
| `2` | Pipeline error — ingestion, DSL compilation, matching, grounding, or benchmark execution failed against otherwise well-formed input (e.g. a malformed trace, a pattern that fails to compile). |
| `3` | I/O error writing the requested `--output` file. |

## Examples

Analyze a trace with one pattern, human-readable output:

```sh
$ rootcause analyze trace.json --patterns classic_reentrancy.rcdsl
Trace: trace.json
  transaction 0xaaaa... (block 19088743, chain 1)
  1 call(s), 1 storage change(s), 1 log(s), 1 token transfer(s)

Ran 1 pattern(s), found 0 candidate match(es), 0 finding(s) after grounding:
  (no candidates found)
```

Analyze a trace and save a Markdown report:

```sh
rootcause analyze trace.json --patterns patterns/ --format markdown --output report.md
```

Run a benchmark suite and export CSV metrics:

```sh
rootcause benchmark fixtures/ --format csv --output metrics.csv
```

Pipe JSON findings to `jq`:

```sh
rootcause --quiet analyze trace.json --patterns patterns/ --format json \
  | jq '.findings[] | select(.status == "grounded")'
```

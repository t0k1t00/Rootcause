<parameter name="CodeContent"># Root Cause

> **Deterministic, evidence-grounded EVM exploit pattern detection — no ML, no confidence scores, no guessing.**

Root Cause takes a raw EVM transaction trace and a library of pattern definitions written in its own Pattern DSL, and tells you — with independently verified, clause-by-clause evidence — whether a known exploit shape is structurally present in the trace.

[![CI](https://github.com/t0k1t00/Rootcause/actions/workflows/ci.yml/badge.svg)](https://github.com/t0k1t00/Rootcause/actions/workflows/ci.yml)
[![Rust 1.87](https://img.shields.io/badge/rust-1.87.0-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

---

## Table of Contents

- [What Root Cause Does](#what-root-cause-does)
- [Architecture](#architecture)
- [Workspace Layout](#workspace-layout)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [Pattern Authoring Workflow](#pattern-authoring-workflow)
- [Pattern DSL Reference](#pattern-dsl-reference)
- [Detection Pattern Library](#detection-pattern-library)
- [Trace Format Converters](#trace-format-converters)
- [Exploit Corpus & Verified Outcomes](#exploit-corpus--verified-outcomes)
- [CI Pipeline](#ci-pipeline)
- [Docker](#docker)
- [Troubleshooting](#troubleshooting)
- [Contributing](#contributing)

---

## What Root Cause Does

Given a raw transaction trace (Geth/Erigon `callTracer` output, or Root Cause's own canonical JSON schema) and a library of `.rcdsl` pattern files, Root Cause runs a four-stage pipeline:

1. **Ingest** — Parse the raw trace into a canonical, immutable fact model: calls, storage changes, log events, token transfers, and transaction metadata.
2. **Match** — Structurally scan every pattern against the fact model, identifying every *candidate* occurrence of the exploit shape.
3. **Ground** — Independently re-derive, clause by clause, whether the trace actually supports each candidate, producing an explicit `Grounded` / `Abstain` / `Ungrounded` result — never a bare confidence score.
4. **Taxonomize** — Map every grounded/abstained result onto external vulnerability registries: [SCWE](https://scs.owasp.org/SCWE/) (primary) and [SWC](https://swcregistry.io/).

**Precision over recall throughout.** A pattern that cannot be independently confirmed abstains rather than guessing. Abstention is a first-class reported outcome, not a silent failure.

---

## Architecture

```mermaid
flowchart TD
    subgraph Input
        A["Raw Trace\n(Geth / Erigon callTracer JSON)"]
        B["Pattern Library\n(.rcdsl files)"]
    end

    subgraph Converters["crates/converters"]
        C["geth-convert\nbinary"]
    end

    subgraph Ingestion["crates/ingestion"]
        D["Normalize & validate\ncall tree, storage,\nlogs, token transfers"]
    end

    subgraph FactModel["crates/fact-model"]
        E["Immutable\nFactArena\n(Call · Storage · Log\nTokenTransfer · Tx)"]
    end

    subgraph DSL["crates/dsl"]
        F["Parse .rcdsl\n→ AST\n→ CompiledPattern"]
    end

    subgraph Matcher["crates/matcher"]
        G["Structural scan\nCandidate set\n(per pattern)"]
    end

    subgraph Grounding["crates/grounding"]
        H["Independent\nevidence re-verification\nGrounded / Abstain / Ungrounded"]
    end

    subgraph Taxonomy["crates/taxonomy"]
        I["SCWE + SWC\nmapping"]
    end

    subgraph Output
        J["Human · JSON\nMarkdown · CSV\nreport"]
    end

    subgraph CLI["crates/cli  (rootcause binary)"]
        K["analyze · benchmark\nversion · doctor\nnew-pattern · validate-pattern\nformat-pattern · completions"]
    end

    A -->|"callTracer output"| C
    C -->|"canonical JSON"| D
    A -->|"canonical JSON"| D
    B --> F
    D --> E
    E --> G
    F --> G
    G -->|"candidates"| H
    E -->|"re-verify"| H
    H --> I
    I --> J
    K --> D
    K --> F
    K --> G
    K --> H
    K --> I
    K --> J

    style FactModel fill:#1e3a5f,color:#fff,stroke:#4a9eff
    style Matcher fill:#1e3a5f,color:#fff,stroke:#4a9eff
    style Grounding fill:#1e3a5f,color:#fff,stroke:#4a9eff
    style DSL fill:#2d4a22,color:#fff,stroke:#6abf47
    style Converters fill:#4a2222,color:#fff,stroke:#bf4747
    style Taxonomy fill:#3a2d4a,color:#fff,stroke:#9b6abf
    style CLI fill:#1a1a2e,color:#fff,stroke:#888
```

### Pipeline detail

```mermaid
sequenceDiagram
    participant User
    participant CLI as rootcause CLI
    participant Conv as geth-convert
    participant Ing as ingestion
    participant Mat as matcher
    participant Grd as grounding
    participant Tax as taxonomy

    User->>CLI: rootcause analyze trace.json --patterns patterns/
    CLI->>Ing: parse + normalize raw trace
    Ing-->>CLI: FactArena (immutable)
    CLI->>Mat: compile patterns + scan arena
    Mat-->>CLI: CandidateSet (per pattern)
    loop For each candidate
        CLI->>Grd: re-verify evidence clauses independently
        Grd-->>CLI: Grounded | Abstain | Ungrounded
    end
    CLI->>Tax: map findings → SCWE / SWC
    Tax-->>CLI: taxonomy entries
    CLI-->>User: report (human / JSON / Markdown / CSV)

    Note over Conv,Ing: Optional pre-step for Geth/Erigon traces
    User->>Conv: geth-convert --input calltracer.json
    Conv-->>User: canonical JSON → then feed to CLI
```

### Crate dependency graph

```mermaid
graph BT
    fact-model
    ingestion --> fact-model
    dsl
    matcher --> fact-model
    matcher --> dsl
    grounding --> fact-model
    grounding --> matcher
    taxonomy --> dsl
    benchmark-harness --> fact-model
    benchmark-harness --> ingestion
    benchmark-harness --> dsl
    benchmark-harness --> matcher
    benchmark-harness --> grounding
    benchmark-harness --> taxonomy
    cli --> fact-model
    cli --> ingestion
    cli --> dsl
    cli --> matcher
    cli --> grounding
    cli --> taxonomy
    cli --> benchmark-harness
    converters --> fact-model
    converters --> ingestion

    style fact-model fill:#1e3a5f,color:#fff
    style matcher fill:#1e3a5f,color:#fff
    style grounding fill:#1e3a5f,color:#fff
    style dsl fill:#2d4a22,color:#fff
    style cli fill:#1a1a2e,color:#fff
    style converters fill:#4a2222,color:#fff
    style taxonomy fill:#3a2d4a,color:#fff
```

---

## Workspace Layout

```
rootcause/
├── crates/
│   ├── fact-model/          Canonical, immutable trace representation
│   │                        (Call, StorageChange, LogEvent, TokenTransfer, Tx)
│   ├── ingestion/           Normalize raw trace JSON → FactArena
│   ├── dsl/                 Pattern Definition Language: parse → AST → IR
│   ├── matcher/             Deterministic structural pattern matching engine
│   ├── grounding/           Independent evidence re-verification + abstention
│   ├── taxonomy/            SCWE-primary + SWC vulnerability taxonomy mappings
│   ├── benchmark-harness/   Precision/recall/F1 evaluation harness
│   ├── cli/                 `rootcause` binary — all user-facing subcommands
│   └── converters/          `geth-convert` binary — Geth/Erigon trace adapter
├── integration-tests/       Cross-crate binary-level integration tests
├── patterns/                Shipped detection pattern library (.rcdsl)
├── demo/                    Curated exploit trace corpus (JSON)
│   └── samples/             Real callTracer responses for converter tests
├── examples/                Annotated example patterns with walkthrough
├── docs/
│   ├── PATTERN_AUTHORING_GUIDE.md
│   ├── spec/                Architecture + engineering specification
│   └── adr/                 Architecture Decision Records
├── .github/workflows/
│   ├── ci.yml               fmt / clippy / test / doc / deny / coverage
│   └── release.yml          Cross-platform binaries + Docker + GitHub Release
├── Dockerfile
├── docker-compose.yml
├── rust-toolchain.toml      Pinned to 1.87.0
└── deny.toml                cargo-deny: license + advisory + duplicate checks
```

---

## Installation

### Option 1 — Prebuilt binary (recommended)

Download from [GitHub Releases](https://github.com/t0k1t00/Rootcause/releases). Archives are available for Linux (`x86_64-unknown-linux-gnu`), macOS (`x86_64` and `aarch64-apple-darwin`), and Windows (`x86_64-pc-windows-msvc`), each bundled with the pattern library and a `.sha256` checksum.

```sh
# Verify before running (Linux/macOS)
sha256sum -c rootcause-x86_64-unknown-linux-gnu.tar.gz.sha256

# Windows
certutil -hashfile rootcause-x86_64-pc-windows-msvc.zip SHA256
```

### Option 2 — `cargo install`

```sh
git clone https://github.com/t0k1t00/Rootcause
cd Rootcause
cargo install --path crates/cli --locked
rootcause --version
```

### Option 3 — Build in place

```sh
cargo build --release -p cli
./target/release/rootcause --version
```

> **Note:** Options 2 and 3 require the pinned Rust 1.87.0 toolchain. `rustup` reads `rust-toolchain.toml` automatically once you `cd` into the repo. No other runtime dependencies — no database, no network access, no external services.

### Option 4 — Docker

```sh
docker build -t rootcause .
docker run --rm -v "$(pwd)/demo:/data:ro" rootcause \
    analyze /data/dao.json --patterns /patterns
```

See [Docker](#docker) for full details and `docker-compose.yml` usage.

### Shell completions

```sh
rootcause completions bash   > /etc/bash_completion.d/rootcause
rootcause completions zsh    > "${fpath[1]}/_rootcause"
rootcause completions fish   > ~/.config/fish/completions/rootcause.fish
rootcause completions powershell >> $PROFILE
# also: elvish
```

---

## Quick Start

```sh
# Clone and analyze the bundled reentrancy example
git clone https://github.com/t0k1t00/Rootcause && cd Rootcause
cargo run -p cli -- analyze demo/dao.json --patterns patterns/
```

**Expected output:**

```
Trace: demo/dao.json
  transaction 0xbbbb...bbbb (block 238002, chain 1)
  3 call(s), 1 storage change(s), 0 log(s), 0 token transfer(s)

Ran 7 pattern(s), found 1 candidate match(es), 1 finding(s) after grounding:

[GROUNDED] classic_reentrancy v1 (Reentrancy, Critical)
  candidate: f803b1b939c2d12a
  required evidence: 2/2 verified; optional: 0/0 verified
  taxonomy:
    - SCWE SCWE-046: Reentrancy Attacks
    - SWC  SWC-107:  Reentrancy
```

`GROUNDED` means both evidence clauses were **independently re-derived from the trace's actual call structure** — not just matched once and trusted.

```sh
# JSON output
cargo run -p cli -- analyze demo/dao.json --patterns patterns/ --format json

# Markdown (suitable for GitHub issues / reports)
cargo run -p cli -- analyze demo/dao.json --patterns patterns/ --format markdown

# Run the benchmark harness
cargo run -p cli -- benchmark \
    crates/benchmark-harness/tests/fixtures/reentrancy_basic/case.json \
    --format markdown
```

---

## Pattern Authoring Workflow

Root Cause ships a pattern SDK — four subcommands that streamline writing and validating new `.rcdsl` detection patterns:

```sh
# 1. Scaffold: starter .rcdsl, positive/negative demo traces, README template
rootcause new-pattern my_pattern --family MyExploitFamily

# 2. Edit patterns/my_pattern.rcdsl and demo/{positive,negative}.json
#    (see docs/PATTERN_AUTHORING_GUIDE.md)

# 3. Validate: syntax → metadata → compilation → taxonomy → demo-trace checks
rootcause validate-pattern patterns/my_pattern.rcdsl

# 4. Format canonically before committing
rootcause format-pattern patterns/my_pattern.rcdsl --write

# 5. Scan the whole library for cross-file issues before opening a PR
rootcause doctor
```

`format-pattern --check` and `doctor` are CI-safe: both exit non-zero without writing anything when they find a problem, identical to `cargo fmt --check`.

See [`docs/PATTERN_AUTHORING_GUIDE.md`](docs/PATTERN_AUTHORING_GUIDE.md) for the full step-by-step guide.

---

## Pattern DSL Reference

Patterns are written in `.rcdsl` files. A minimal example:

```
pattern classic_reentrancy version 1 {
    family:   Reentrancy
    severity: Critical

    evidence {
        required reentrant_call: call(
            kind:      External,
            reentrant: true,
            value_out: true
        )
        required post_write: storage(
            changed: true,
            call_kind: External
        )
    }

    sequence: [reentrant_call, post_write]
    constraint: reentrant_call
}
```

### Evidence predicate kinds

| Predicate | Key attributes |
|-----------|---------------|
| `call(...)` | `kind`, `selector`, `succeeded`, `reentrant`, `value_out`, `parent_kind`, `ancestor_kind` |
| `storage(...)` | `slot`, `changed`, `call_kind` |
| `log(...)` | `topic0`, `decoded` |
| `token_transfer(...)` | `direction`, `token` |
| `value_flow(...)` | `direction`, `min`, `max` |
| `transaction(...)` | `status`, `min_gas`, `max_gas` |

### Cross-evidence correlation

| Section | Purpose |
|---------|---------|
| `sequence: [a, b, ...]` | Evidence clauses must appear in non-decreasing trace order |
| `same_call: [a, b, ...]` | All listed clauses must be produced by the **identical** call |
| `constraint: expr` | Logical predicate over evidence names (default: conjunction) |

### Grounding outcomes

| Status | Meaning |
|--------|---------|
| `Grounded` | All required evidence clauses independently re-verified |
| `Abstain` | Structural match found, but ≥1 clause is unresolvable from the fact model (by design — not a bug) |
| `Ungrounded` | Evidence clause re-verification failed |

---

## Detection Pattern Library

Eight patterns ship in `patterns/`, each exercised by the integration test suite:

| Pattern | Family | Severity | SCWE | SWC |
|---------|--------|----------|------|-----|
| `classic_reentrancy` | Reentrancy | Critical | SCWE-046 | SWC-107 |
| `unchecked_external_call` | UncheckedExternalCall | High | SCWE-048 | SWC-104 |
| `delegatecall_storage_collision` | DelegatecallStorageCollision | Critical | SCWE-150 | SWC-112 |
| `unsafe_selfdestruct` | UnsafeSelfdestruct | Critical | SCWE-050 | SWC-106 |
| `delegatecall_reachable_selfdestruct` | DelegatecallReachableSelfdestruct | Critical | SCWE-038 | SWC-106 |
| `delegatecall_reachable_selfdestruct_transitive` | DelegatecallReachableSelfdestruct | Critical | SCWE-038 | SWC-106 |
| `spoofed_transfer_event` | SpoofedTransferEvent | High | SCWE-063 | — |
| `oracle_manipulation` | OracleManipulation | High | SCWE-028 | — |

> The `oracle_manipulation` pattern correctly **abstains** by design — its `role: PriceOracle` attribute is structurally unresolvable from call-graph traces alone. See [Exploit Corpus](#exploit-corpus--verified-outcomes) for details.

---

## Trace Format Converters

`crates/converters` adapts real Ethereum tooling output into Root Cause's canonical schema. Converters are pure functions — no network I/O.

### Geth `debug_traceTransaction` / `callTracer`

```sh
cargo run -p converters --bin geth-convert -- \
    --input  demo/samples/geth_calltracer_goerli.json \
    --tx-hash  0xc0ffcf21dc1881c3f20160b530d76f1d37af7b40552f7c59d3f339bad3023dad \
    --block    0x876123 \
    --chain-id 0x5 \
    --nonce    0x1 \
    --output   demo/converted/geth_goerli.json

cargo run -p cli -- analyze demo/converted/geth_goerli.json --patterns patterns/
```

`demo/samples/geth_calltracer_goerli.json` is a **real** callTracer response (Goerli tx `0xc0ffcf...3dad`, geth v1.10.26) saved verbatim from a public go-ethereum issue report.

### Erigon `debug_traceTransaction` / `callTracer`

The same `geth-convert` binary handles Erigon output — Erigon's `callTracer` format is structurally identical to Geth's. Verified against a real Erigon Polygon trace in the integration test suite.

---

## Exploit Corpus & Verified Outcomes

Every row in this table is backed by an integration test in `integration-tests/tests/exploit_corpus.rs` that runs the **real `rootcause` binary** and asserts on its actual output.

| Trace | Pattern | Outcome |
|-------|---------|---------|
| `demo/dao.json` | `classic_reentrancy` | ✅ `GROUNDED` — Critical, SWC-107/SCWE-046 |
| `demo/cross_function_reentrancy.json` | `classic_reentrancy` | ✅ `GROUNDED` — Critical |
| `demo/reentrancy_safe.json` | `classic_reentrancy` | ✅ `0 matches` — no false positive |
| `demo/oracle_manipulation.json` | `oracle_manipulation` | ✅ `ABSTAIN` — by design (unresolvable role) |
| `demo/unchecked_call.json` | `unchecked_external_call` | ✅ `GROUNDED` — High, SWC-104/SCWE-048 |
| `demo/checked_call_reverts.json` | `unchecked_external_call` | ✅ `0 matches` — no false positive |
| `demo/delegatecall_storage_collision.json` | `delegatecall_storage_collision` | ✅ `GROUNDED` — Critical, SWC-112/SCWE-150 |
| `demo/delegatecall_storage_safe.json` | `delegatecall_storage_collision` | ✅ `0 matches` — no false positive |
| `demo/delegatecall_storage_adjacent_not_producing.json` | `delegatecall_storage_collision` | ✅ `0 matches` — precise call attribution |
| `demo/unsafe_selfdestruct.json` | `unsafe_selfdestruct` | ✅ `GROUNDED` — Critical, SWC-106/SCWE-050 |
| `demo/selfdestruct_reverted.json` | `unsafe_selfdestruct` | ✅ `0 matches` — no false positive |
| `demo/delegatecall_reachable_selfdestruct.json` | `delegatecall_reachable_selfdestruct` | ✅ `GROUNDED` — Critical, SWC-106/SCWE-038 |
| `demo/selfdestruct_direct_call_not_delegated.json` | `delegatecall_reachable_selfdestruct` | ✅ `0 matches` — `parent_kind` precision |
| `demo/delegatecall_reachable_selfdestruct_transitive.json` | `delegatecall_reachable_selfdestruct_transitive` | ✅ `GROUNDED` — Critical, transitive ancestor check |
| `demo/selfdestruct_delegatecall_sibling_not_ancestor.json` | `delegatecall_reachable_selfdestruct_transitive` | ✅ `0 matches` — `ancestor_kind` precision |
| `demo/spoofed_transfer_event.json` | `spoofed_transfer_event` | ✅ `GROUNDED` — High, SCWE-063 |
| `demo/real_transfer_event.json` | `spoofed_transfer_event` | ✅ `0 matches` — no false positive |

---

## CI Pipeline

Every push and pull request to `main` runs six jobs:

```mermaid
flowchart LR
    fmt["cargo fmt\n--check"]
    clippy["cargo clippy\n-D warnings"]
    test["cargo test\n--workspace"]
    doc["cargo doc\n-D warnings"]
    deny["cargo deny\ncheck"]
    coverage["cargo llvm-cov\n(informational)"]

    fmt --> test
    clippy --> test
    test --> doc
    test --> deny
    test --> coverage

    style fmt fill:#2d4a22,color:#fff
    style clippy fill:#2d4a22,color:#fff
    style test fill:#1e3a5f,color:#fff
    style doc fill:#1e3a5f,color:#fff
    style deny fill:#3a2d4a,color:#fff
    style coverage fill:#4a3a1e,color:#fff
```

| Job | Command | Fails CI? |
|-----|---------|-----------|
| `fmt` | `cargo fmt --all --check` | Yes |
| `clippy` | `cargo clippy --workspace --all-targets -- -D warnings` | Yes |
| `test` | `cargo build --workspace && cargo test --workspace` | Yes |
| `doc` | `cargo doc --workspace --no-deps` (`RUSTDOCFLAGS=-D warnings`) | Yes |
| `deny` | `cargo deny check` | Yes |
| `coverage` | `cargo llvm-cov` → `lcov.info` artifact | No (`continue-on-error: true`) |

---

## Docker

```sh
# Build the image
docker build -t rootcause .

# Analyze a trace (mount your trace as /data)
docker run --rm \
    -v "$(pwd)/demo:/data:ro" \
    rootcause analyze /data/dao.json --patterns /patterns

# Docker Compose example
docker compose run rootcause analyze /data/dao.json --patterns /patterns
```

The image bundles the pattern library at `/patterns`. Mount your own pattern directory as an additional volume to use custom patterns. Published images are available at `ghcr.io/t0k1t00/rootcause:<tag>` — `:latest` tracks the most recent `v*` tag.

---

## Troubleshooting

| Symptom | Likely cause | Fix |
|---------|-------------|-----|
| `error: failed to load trace input` | Wrong trace path or wrong working directory | `analyze`'s first positional arg is always the trace file |
| `error: failed to read ... --patterns` | `--patterns` needs one `.rcdsl` file or a directory of them (not nested) | Check path; directory must contain `.rcdsl` files directly |
| `benchmark` reports `0 cases run` | Directory mode is non-recursive | Place `.json` case files directly in the suite dir, not in subdirs |
| Pattern shows `0 candidate match(es)` when match expected | Missing evidence data in trace (e.g. `callTracer` with no storage/log) | Run `rootcause analyze ... --verbose` for per-stage diagnostics |
| Finding shows `[ABSTAIN]` | Evidence clause unresolvable — this is correct, not an error | See the pattern's own doc comment in `patterns/*.rcdsl` |
| Docker build fails fetching dependencies | `docker build` needs network for `cargo build` | This is expected; the runtime binary makes no network calls |

---

## Contributing

1. **Bug reports** — Use `.github/ISSUE_TEMPLATE/bug_report.md`. Always include `rootcause version` output (not just `--version`).
2. **New patterns** — Follow `docs/PATTERN_AUTHORING_GUIDE.md`. Use the pattern SDK (`new-pattern`, `validate-pattern`, `format-pattern`, `doctor`).
3. **Engine changes** — Read `docs/spec/01_ARCHITECTURE.md` and the relevant `docs/adr/` records before proposing structural changes.
4. **Security issues** — See `SECURITY.md` for the private disclosure policy.

All contributions must pass the CI pipeline (`fmt`, `clippy`, `test`, `doc`, `deny`) before merge. See `CONTRIBUTING.md` for the full setup guide.

---

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

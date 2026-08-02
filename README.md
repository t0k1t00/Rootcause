# Root Cause

Root Cause is a deterministic, evidence-grounded pattern-matching engine for
identifying and explaining root-cause vulnerability patterns in EVM
transaction traces.

Given a raw transaction trace and a library of pattern definitions written
in the Pattern DSL, Root Cause:

1. **Ingests** the raw trace into a canonical, immutable fact model.
2. **Matches** every pattern against the trace structurally, finding every
   candidate occurrence.
3. **Grounds** every candidate by independently re-deriving, clause by
   clause, whether the trace actually supports it — producing an explicit
   `Grounded` / `Abstain` / `Ungrounded` status, never a bare confidence
   score.
4. **Maps** every grounded/abstained/ungrounded result onto external
   vulnerability taxonomies (SCWE, SWC).

Precision is prioritized over recall throughout: a pattern that cannot be
independently confirmed abstains rather than guessing, and abstention is
a first-class, reported result rather than a silent failure. See
`docs/spec/01_ARCHITECTURE.md` for the full architectural rationale and
`docs/adr/` for the engineering decisions made while implementing it.

Contributing a pattern, a bug fix, or anything else? See
`CONTRIBUTING.md` (and `docs/PATTERN_AUTHORING_GUIDE.md` specifically for
patterns). Found a security issue in Root Cause itself? See
`SECURITY.md`. `CHANGELOG.md` tracks what's changed.

## Workspace layout

```
crates/
  fact-model/         Canonical, immutable representation of a decoded trace.
  ingestion/           Archive-node RPC trace ingestion into the fact model.
  dsl/                 Pattern definition language: parsing and compilation.
  matcher/             Deterministic structural pattern matching engine.
  grounding/           Independent evidence verification and abstention logic.
  taxonomy/            SCWE-primary vulnerability taxonomy with SWC mappings.
  benchmark-harness/   Evaluation harness and metrics (precision/recall/
                       abstention-rate) against a corpus of benchmark cases.
  cli/                 `rootcause` command-line entry point tying the above
                       together (`analyze`, `benchmark`, `version`).
integration-tests/     Cross-crate integration tests.
Dockerfile,             Container image build and (for compose) an
docker-compose.yml,      example `docker compose run` invocation — see
.dockerignore            "Installation" below.
.github/workflows/
  ci.yml                 fmt/clippy/build/test/doc/deny/coverage on
                         every push and PR.
  release.yml            Builds and publishes binaries, a Docker image,
                         and a GitHub Release on every `v*` tag push.
docs/
  PATTERN_AUTHORING_GUIDE.md
                       Task-oriented, from-scratch guide to writing a new
                       `.rcdsl` detection pattern.
  spec/                Canonical specification documents (architecture,
                       engineering spec, research notes, benchmark design).
  adr/                 Architecture Decision Records for engineering
                       decisions delegated by the canonical spec.
```

Each crate has its own `README.md` with a module map and usage examples;
`cargo doc --workspace --no-deps` builds the full API reference.

## Installation

### Option 1: download a prebuilt binary

Every [GitHub Release](https://github.com/rootcause-project/rootcause/releases)
(tagged `vX.Y.Z`) includes prebuilt `rootcause` archives for Linux
(`x86_64-unknown-linux-gnu`), macOS (`x86_64-apple-darwin` and
`aarch64-apple-darwin`), and Windows (`x86_64-pc-windows-msvc`), each
bundled with the pattern library, `LICENSE`, `README.md`, and a
`.sha256` checksum file. See `.github/workflows/release.yml` for exactly
how these are built. Verify the checksum before running an unfamiliar
binary:

```sh
sha256sum -c rootcause-x86_64-unknown-linux-gnu.tar.gz.sha256   # Linux/macOS
certutil -hashfile rootcause-x86_64-pc-windows-msvc.zip SHA256  # Windows
```

### Option 2: `cargo install`

```sh
git clone https://github.com/rootcause-project/rootcause
cd rootcause

cargo install --path crates/cli --locked
rootcause --version
```

### Option 3: build in place

```sh
cargo build --release -p cli
./target/release/rootcause --version
```

Options 2 and 3 require the pinned Rust toolchain in
`rust-toolchain.toml` (currently `1.75.0`); `rustup` picks it up
automatically once you `cd` into the repo. There are no other build or
runtime dependencies — no database, no network access, no external
services.

### Option 4: Docker

```sh
docker build -t rootcause .
docker run --rm -v "$(pwd)/demo:/data:ro" rootcause \
    analyze /data/dao.json --patterns /patterns
```

The image bundles the pattern library at `/patterns`; mount your own
trace file(s) (and, optionally, your own pattern directory) as a volume.
See `docker-compose.yml` for an equivalent `docker compose run` example.
Published images are available at
`ghcr.io/rootcause-project/rootcause:<tag>` (built by the same release
workflow as the binaries above; `:latest` tracks the most recent tag).

### Shell completions

```sh
# Bash (adjust the destination to your system's completion directory)
rootcause completions bash > /etc/bash_completion.d/rootcause

# Zsh
rootcause completions zsh > "${fpath[1]}/_rootcause"

# Fish
rootcause completions fish > ~/.config/fish/completions/rootcause.fish

# PowerShell
rootcause completions powershell >> $PROFILE
```

`elvish` is also supported. Run `rootcause completions --help` to see
the full list this build supports.

## Quick start

```sh
# Analyze the bundled worked example (a reentrancy trace) against the
# bundled pattern library
cargo run -p cli -- analyze demo/dao.json --patterns patterns/ --format json

# Run the bundled benchmark case and get precision/recall/F1 metrics
cargo run -p cli -- benchmark \
    crates/benchmark-harness/tests/fixtures/reentrancy_basic/case.json \
    --format markdown --output benchmark.md
```

`--patterns` also accepts a directory of many `.rcdsl` files (as above);
`benchmark` also accepts a directory of many `case.json`-shaped files
placed directly inside it (not nested in subdirectories — see
`crates/benchmark-harness/README.md` for the exact suite-directory
layout). Every command above is exercised as-is by this project's own
test suite, so it will keep working as the engine evolves.

See `crates/cli/README.md` for the full command reference, exit codes,
and more examples.

### Version reporting

```sh
rootcause --version    # short: "rootcause 0.1.0"
rootcause version      # long: rootcause's version plus every engine
                        # crate it links (fact-model, ingestion, dsl,
                        # matcher, grounding, taxonomy, benchmark-harness)
```

Include the `rootcause version` output (not just `--version`) in any bug
report — see `.github/ISSUE_TEMPLATE/bug_report.md`.

## Pattern authoring workflow

Root Cause ships a small "pattern SDK" of CLI commands so a contributor
never has to hand-copy an existing `.rcdsl` file to start a new one.
This is the same workflow `docs/PATTERN_AUTHORING_GUIDE.md` and
`CONTRIBUTING.md` walk through in full detail:

```sh
# 1. Scaffold a new pattern: a starter .rcdsl file, a positive/negative
#    demo trace pair, a README template, and a taxonomy-mapping TODO note.
rootcause new-pattern my_new_pattern --family MyExploitFamily

# 2. Edit patterns/my_new_pattern.rcdsl and the two demo/*.json traces
#    (see docs/PATTERN_AUTHORING_GUIDE.md steps 4 and 6).

# 3. Run every authoring check in one go: syntax, metadata, compilation,
#    taxonomy mapping, and the positive/negative demo traces.
rootcause validate-pattern patterns/my_new_pattern.rcdsl

# 4. Pretty-print the pattern in the repository's canonical style
#    before committing.
rootcause format-pattern patterns/my_new_pattern.rcdsl --write

# 5. Scan the whole pattern library for cross-file issues (duplicate
#    ids, unmapped families, missing demo traces) before opening a PR.
rootcause doctor
```

`format-pattern --check` and `doctor` are also suitable for CI: both
exit non-zero (without writing anything) when they find a problem, the
same way `cargo fmt --check` does.

See `crates/cli/README.md` for the full flag reference for each
command.

## Troubleshooting

- **`error: failed to load trace input: could not read ...`** — the
  trace path is wrong, or (if piping/scripting) relative to a different
  working directory than you expect. `analyze`'s first positional
  argument is always the trace file, not the pattern path.
- **`error: failed to read ...` for `--patterns`** — `--patterns` needs
  either one `.rcdsl` file or a directory containing `.rcdsl` files
  directly inside it (not nested further).
- **`benchmark` reports `0 cases run` against a directory** — directory
  mode is intentionally non-recursive: it only loads `.json` files
  placed directly inside the given directory, not nested in
  subdirectories. See `crates/benchmark-harness/README.md`.
- **A pattern you expected to match shows `0 candidate match(es)`
  instead** — this is very often correct, not a bug: Root Cause
  prioritizes precision over recall, so a trace missing even one
  required evidence clause (e.g. a `callTracer`-derived trace with no
  storage or log data — see the Geth/Erigon converter sections below)
  will not match, by design. Check `rootcause analyze ... --verbose` for
  per-stage diagnostics before assuming it's a bug.
- **A finding shows `[ABSTAIN]` instead of `[GROUNDED]`** — this is not
  an error. It means the pattern structurally matched but at least one
  evidence clause could not be independently re-verified — see
  "Grounding" in `docs/spec/01_ARCHITECTURE.md` and the specific
  pattern's own doc comment (in `patterns/*.rcdsl`) for why.
- **Docker build fails to fetch dependencies** — the build stage runs
  `cargo build`, which needs network access to crates.io during `docker
  build`; this is normal and expected (unlike the `rootcause` binary
  itself, which makes no network calls at runtime).
- **Still stuck?** Open an issue using
  `.github/ISSUE_TEMPLATE/bug_report.md` — include `rootcause version`
  output and, if possible, a minimal reproducing trace/pattern file.

`call(...)` patterns can additionally match on a call's 4-byte ABI
function selector, when the raw trace records calldata:

```
pattern erc20_transfer_call version 1 {
    family: UnauthorizedCall
    severity: Low
    evidence {
        required transfer_call: call(kind: External, selector: "0xa9059cbb")
    }
    constraint: transfer_call
}
```

Requirements and scope:

- The raw trace's call node needs an `input` field (`0x`-prefixed hex
  calldata) for a selector to be extracted at all; a call with no
  `input`, or `input` shorter than 4 bytes, has no selector and never
  matches a `selector:` clause.
- Only the bare first 4 bytes are compared. There is no 4-byte-signature
  database (so `selector: "0xa9059cbb"` matches, but Root Cause cannot
  tell you it means `transfer(address,uint256)`) and no ABI parameter
  decoding of anything past those 4 bytes.
- See `crates/ingestion/README.md` and `crates/fact-model/README.md` for
  where selector extraction happens in the pipeline.

An end-to-end example pattern built on this capability — flagging an
unauthorized proxy `upgradeTo()` call — lives in `examples/`; see
`examples/README.md`.

## 5-minute quick start

This repo ships one worked example end to end: `demo/dao.json`, a
sanitized, structurally-representative reentrancy trace (attacker calls a
victim contract, the victim pays out to the attacker before updating its
own balance, and the attacker's fallback re-enters the victim mid-call —
the classic "checks-effects-interactions" violation), and
`patterns/classic_reentrancy.rcdsl`, a Pattern DSL rule for exactly that
shape.

```sh
git clone https://github.com/rootcause-project/rootcause
cd rootcause
cargo run -p cli -- analyze demo/dao.json --patterns patterns/
```

Expected output:

```
Trace: demo/dao.json
  transaction 0xbbbb...bbbb (block 238002, chain 1)
  3 call(s), 1 storage change(s), 0 log(s), 0 token transfer(s)

Ran 7 pattern(s), found 1 candidate match(es), 1 finding(s) after grounding:

[GROUNDED] classic_reentrancy v1 (Reentrancy, Critical)
  candidate: f803b1b939c2d12a
  required evidence: 2/2 verified; optional: 0/0 verified
  pattern `classic_reentrancy` v1: grounded (2/2 required evidence clauses verified). sequence ordering confirmed.
  taxonomy:
    - SCWE SCWE-046: Reentrancy Attacks (https://scs.owasp.org/SCWE/SCSVS-CODE/SCWE-046/)
    - SWC SWC-107: Reentrancy (https://swcregistry.io/docs/SWC-107/)
```

`GROUNDED` here means something specific: both evidence clauses the
pattern requires — a reentrant, value-flowing-out external call, and a
storage write occurring at or after it — were **independently
re-derived from the trace's actual call structure**, not just matched
once and trusted. Ask for JSON or Markdown instead of the default
human-readable text with `--format json` or `--format markdown`; both
carry the same evidence, status, and taxonomy fields.

## Exploit corpus

Beyond the single quick-start example, `demo/` and `patterns/` hold a
small curated corpus — every trace below is verified by an integration
test in `integration-tests/tests/exploit_corpus.rs` that runs the real
`rootcause` binary and checks its actual output, not just that the
process exits zero.

| Trace | Pattern | Command | Verified outcome |
|---|---|---|---|
| `demo/dao.json` | `classic_reentrancy` | `analyze demo/dao.json --patterns patterns/` | `GROUNDED`, Critical, SWC-107 / SCWE-046 |
| `demo/cross_function_reentrancy.json` | `classic_reentrancy` (same pattern) | `analyze demo/cross_function_reentrancy.json --patterns patterns/` | `GROUNDED`, Critical — see note below |
| `demo/reentrancy_safe.json` | `classic_reentrancy` | `analyze demo/reentrancy_safe.json --patterns patterns/` | `0 candidate match(es)` — no false positive |
| `demo/oracle_manipulation.json` | `oracle_manipulation` | `analyze demo/oracle_manipulation.json --patterns patterns/` | `ABSTAIN`, High, SCWE-028 — see note below |
| `demo/unchecked_call.json` | `unchecked_external_call` | `analyze demo/unchecked_call.json --patterns patterns/` | `GROUNDED`, High, SWC-104 / SCWE-048 |
| `demo/checked_call_reverts.json` | `unchecked_external_call` | `analyze demo/checked_call_reverts.json --patterns patterns/` | `0 candidate match(es)` — no false positive |
| `demo/delegatecall_storage_collision.json` | `delegatecall_storage_collision` | `analyze demo/delegatecall_storage_collision.json --patterns patterns/` | `GROUNDED`, Critical, SWC-112 / SCWE-150 |
| `demo/delegatecall_storage_safe.json` | `delegatecall_storage_collision` | `analyze demo/delegatecall_storage_safe.json --patterns patterns/` | `0 candidate match(es)` — no false positive |
| `demo/delegatecall_storage_adjacent_not_producing.json` | `delegatecall_storage_collision` | `analyze demo/delegatecall_storage_adjacent_not_producing.json --patterns patterns/` | `0 candidate match(es)` — see "Precise storage-to-call attribution" below |
| `demo/unsafe_selfdestruct.json` | `unsafe_selfdestruct` | `analyze demo/unsafe_selfdestruct.json --patterns patterns/` | `GROUNDED`, Critical, SWC-106 / SCWE-050 |
| `demo/selfdestruct_reverted.json` | `unsafe_selfdestruct` | `analyze demo/selfdestruct_reverted.json --patterns patterns/` | `0 candidate match(es)` — no false positive |
| `demo/delegatecall_reachable_selfdestruct.json` | `delegatecall_reachable_selfdestruct` | `analyze demo/delegatecall_reachable_selfdestruct.json --patterns patterns/` | `GROUNDED`, Critical, SWC-106 / SCWE-038 (also matches `unsafe_selfdestruct`) |
| `demo/selfdestruct_direct_call_not_delegated.json` | `delegatecall_reachable_selfdestruct` | `analyze demo/selfdestruct_direct_call_not_delegated.json --patterns patterns/` | not matched by this pattern — see note below (still matches `unsafe_selfdestruct`) |
| `demo/delegatecall_reachable_selfdestruct_transitive.json` | `delegatecall_reachable_selfdestruct_transitive` | `analyze demo/delegatecall_reachable_selfdestruct_transitive.json --patterns patterns/` | `GROUNDED`, Critical, SWC-106 / SCWE-038 — see "Transitive delegatecall reachability" below (also matches `unsafe_selfdestruct`; does *not* match the one-hop `delegatecall_reachable_selfdestruct`) |
| `demo/selfdestruct_delegatecall_sibling_not_ancestor.json` | `delegatecall_reachable_selfdestruct_transitive` | `analyze demo/selfdestruct_delegatecall_sibling_not_ancestor.json --patterns patterns/` | not matched by this pattern — see note below (still matches `unsafe_selfdestruct`) |
| `demo/spoofed_transfer_event.json` | `spoofed_transfer_event` | `analyze demo/spoofed_transfer_event.json --patterns patterns/` | `GROUNDED`, High, SCWE-063 |
| `demo/real_transfer_event.json` | `spoofed_transfer_event` | `analyze demo/real_transfer_event.json --patterns patterns/` | `0 candidate match(es)` — no false positive |

**Why `cross_function_reentrancy.json` uses the same pattern as
`dao.json`, not a distinct one:** "cross-function" reentrancy differs
from classic reentrancy only in *which function* the attacker re-enters
through. The fact model (`crates/fact-model`) has no concept of a
function selector or calldata at all — a `Call` carries only
`from`/`to`/`value`/`kind`/`succeeded`, nothing about what was invoked.
Structurally, a reentrant call that targets an already-visited contract
address looks identical whether it calls the same function again or a
different one. The corpus includes this trace anyway because it's a
genuinely different, real-world exploit shape (modeled loosely on the
Cream Finance incident, where the reentrant call went through a
different lending-pool function than the original), and it's valuable
to show the existing pattern generalizes to it — but this is *not* a
"cross-function-specific" detector, and claiming one would exist would
be inaccurate.

**Why `oracle_manipulation.json` abstains instead of reporting a
finding:** the pattern's evidence requires a `token_transfer` with
`unexpected: true` and a `storage` write with `role: PriceOracle`. Both
`unexpected` and `role` are accepted by the DSL grammar (see
`crates/dsl/src/schema.rs`) but neither is backed by any field on the
corresponding `fact-model` type (`TokenTransfer` has no "was this
donation intentional" flag; `StorageChange` has no `role` field at
all) — `crates/matcher/src/predicate.rs` marks both as
permanently `unresolved_attrs`, and `crates/grounding/src/verifier.rs`
turns that into `EvidenceOutcome::Unsupported` every time, regardless
of trace content. The trace genuinely contains a large inbound token
transfer and a large storage delta shaped like a price update — the
engine finds a real candidate — but it correctly refuses to call this
"oracle manipulation" rather than guess at intent it cannot verify.
This is deliberate, existing behavior (the pre-existing
`crates/dsl/tests/fixtures/donation_attack.rcdsl` fixture takes the
same approach), not a bug this corpus works around.

**Why `unchecked_call.json` and `checked_call_reverts.json` are a
matched pair:** the EVM never auto-propagates a low-level call's
failure to its caller — a failed `CALL` just returns `false` — so
whether a caller "checked" that return value is only observable
indirectly, through what the *transaction* does next. `unchecked_call.json`
has an inner call fail (`Call::succeeded == false`) while the
transaction as a whole still reports `Success`: exactly what an
un-checked, silently-ignored failure looks like from a single-trace
fact model. `checked_call_reverts.json` has the identical inner
failure, but the transaction's own status is `Reverted` — the caller
propagated the failure, i.e. checked it — so `unchecked_external_call`
must not fire. Both traces are covered by
`integration-tests/tests/exploit_corpus.rs`, which runs the real
`rootcause` binary against each and checks its actual output.

**Why `delegatecall_storage_collision.json` and
`delegatecall_storage_safe.json` are a matched pair:** `DELEGATECALL`
runs the callee's code against the *caller's* own storage
(`fact_model::CallKind::executes_in_caller_context`), so whether a
given delegatecall is dangerous depends entirely on which storage slot
it ends up touching — a fact the trace already carries via
`StorageChange.slot.key`, exact-matched by the `storage(slot: "0x...")`
predicate. `delegatecall_storage_collision.json` has a `DELEGATECALL`
whose storage write lands on slot `0x0`, the conventional first slot
most proxies use for critical state (an owner/admin address, or, in
pre-EIP-1967 proxies, the implementation pointer itself). Structurally
identical in every other respect, `delegatecall_storage_safe.json` has
its `DELEGATECALL` write land on a namespaced, non-zero slot instead
(the kind of pseudo-random key an ERC-7201-style "unstructured
storage" proxy uses specifically to avoid this collision) — so
`delegatecall_storage_collision` correctly does not fire. See
`patterns/delegatecall_storage_collision.rcdsl` for the scope note on
what this pattern can and cannot verify.

**Why `unsafe_selfdestruct.json` and `selfdestruct_reverted.json` are a
matched pair:** `fact_model::CallKind` had no variant for `SELFDESTRUCT`
until this pattern was added — see "Exploit families
evaluated but not added" below for the gap this closed.
`unsafe_selfdestruct.json` has a `SELFDESTRUCT` call that executes and
succeeds (`Call.succeeded == true`), sending the contract's balance to
an unrelated beneficiary address — exactly the structural shape
`unsafe_selfdestruct` looks for. `selfdestruct_reverted.json` has the
identical `SELFDESTRUCT` frame, but it (and the enclosing transaction)
reports failure instead: the pattern requires `succeeded: true`, so it
correctly reports no finding. Both traces are covered by
`integration-tests/tests/exploit_corpus.rs`. See
`patterns/unsafe_selfdestruct.rcdsl` for the scope note on what this
pattern can and cannot verify (it cannot determine whether the call was
actually authorized — the fact model has no access-control fact).

### Precise storage-to-call attribution

`fact_model::StorageChange` has always carried `call_id`, the exact
call that produced it — `crates/ingestion/src/normalize.rs` stamps
this from the raw trace's own nesting (a storage change is only ever
parsed from *inside* the specific call node it's nested under), and
`FactArenaBuilder::build` enforces it names a real call in the same
arena. What was missing was a way for a *pattern* to query that
association: `crates/matcher/src/predicate.rs::evaluate_storage` used
to read `call_id` only to compute a coarse `order: u32` for
`sequence:` — "roughly happened around here in trace order," not
identity.

That gap is now closed: `storage(...)` accepts a new `call_kind`
attribute (`crates/dsl/src/schema.rs`) that dereferences a write's
`call_id` back to its producing call and filters on *that specific
call's* kind (`crates/matcher/src/predicate.rs`, reusing the same
`call_kind_matches` helper `call(kind: ...)` already used). This
turns "a delegatecall happened, and a storage write happened nearby"
into "this write was produced by a delegatecall" — a strictly
stronger, single-clause claim.

`patterns/delegatecall_storage_collision.rcdsl` was tightened from v1
(two evidence clauses — `call(kind: Delegate)` and `storage(...)` —
correlated only by `sequence:`'s non-decreasing order) to v2 (one
clause: `storage(..., call_kind: Delegate)`), closing exactly the
false-positive gap the v1 pattern's own scope note flagged.
`demo/delegatecall_storage_adjacent_not_producing.json` demonstrates
this concretely: it contains both a `DELEGATECALL` *and*, as a
separate sibling call, a slot-0 write — the shape v1's ordering-only
correlation would have matched. Run it against v2 and it correctly
reports `0 candidate match(es)`, because the write's own `call_id`
resolves to the sibling plain `Call`, not the delegatecall:

```sh
cargo run -p cli -- analyze \
    demo/delegatecall_storage_adjacent_not_producing.json \
    --patterns patterns/delegatecall_storage_collision.rcdsl
```

This capability is intentionally general — any future pattern needing
"this fact was produced by a call of kind X" can reuse `call_kind`
rather than falling back to `sequence:`'s loose ordering.

### Cross-evidence call-identity correlation (`same_call:`)

This adds a new top-level pattern section, `same_call: [a, b,
...]`: an assertion that every listed evidence clause's binding was
produced by the identical `CallId`. It is deliberately narrower than a
general relational query language — it expresses exactly one thing,
call-identity equality, and nothing else (no arbitrary attribute
comparison between evidence clauses).

**DSL.** `same_call:` parses to `ast::SameCallConstraint` (an unordered
list of evidence names, minimum two, no duplicates — enforced by
`validate::validate_same_call`) and compiles to
`ir::CompiledSameCall` (the same names resolved to `EvidenceRef`
indices `compile::compile` already uses for `constraint:` and
`sequence:`).

**Matcher.** `crates/matcher/src/identity.rs` adds the one genuinely
new piece of logic this needed: `producing_call(trace, fact_ref)`, a
pure function deriving *which* `CallId` produced an arbitrary fact,
regardless of fact kind — a `Call` is itself the call it names,
`StorageChange`/`LogEvent` name their producing call directly, and
`TokenTransfer` resolves one hop through the `LogEvent` it was emitted
alongside. `matcher::engine` calls this once per `same_call:` group
per candidate, rejecting (not just down-weighting) any candidate whose
group members resolve to different calls — see
`same_call_attributes_each_anchor_occurrence_to_its_own_write` and
`same_call_rejects_a_write_from_an_unrelated_call` in
`crates/matcher/src/engine.rs`'s test module.

**Grounding.** Consistent with this project's grounding-never-trusts-
matcher design (see `crates/grounding`'s own top-level docs), grounding
does not take matcher's `same_call:` binding on faith: `verify_same_call`
(`crates/grounding/src/engine.rs`) independently recomputes each
member's producing call from grounding's *own* re-verified evidence
chain — reusing `matcher::identity::producing_call` itself (the
correlation logic is shared, not re-derived, by design) but never
matcher's structural match. A `same_call:` group's outcome participates
in `ConfidenceMetadata::same_call_verified`, in abstention-precedence
aggregation (an `Unavailable`/`Unsupported` same_call outcome outranks
a confident `Failed` elsewhere, same as `sequence_verified`), and in
the human-readable explanation string.

**Example.** `examples/proxy_upgrade_self_write.rcdsl` demonstrates
the capability end to end — see `examples/README.md` for the full
walkthrough, including why the original `unauthorized_upgrade_to`
example was not force-fit into a `same_call:` pattern instead.

**Backwards compatibility.** `same_call:` is optional — every existing
pattern in `patterns/` and `examples/` compiles and matches exactly as
before, since `CompiledPattern::same_call` is `None` for any pattern
that doesn't declare the section, and matcher/grounding both simply
skip the check when it's absent.

**Limitations.** `same_call:` only expresses identity — it cannot
express "produced by calls of the same *kind*" (already covered,
differently, by `storage(call_kind: ...)`) or any ordering between
members (that's still `sequence:`'s job; the two sections compose,
they don't replace each other). A pattern can declare both.

### Delegatecall-reachable selfdestruct (`parent_kind`)

`parent_kind` is added to the `call(...)` predicate
(`crates/dsl/src/schema.rs`): an optional attribute that dereferences a
call's own `fact_model::Call::parent` field back to the specific call
that *directly invoked* it, and filters on that parent's kind — the
same "producing fact, not mere trace-order adjacency" idiom
`storage(call_kind: ...)` established for `StorageChange::call_id`,
applied here to the one-hop call-tree edge every
`Call` already carries instead. See `crates/matcher/src/predicate.rs`'s
`evaluate_call` and its `call_predicate_parent_kind_*` test group for
the implementation.

**Why this, and why now.** The README's own "Exploit families
evaluated but not added" table (below) lists every other plausible
next family, and each is blocked by the same underlying gap: the fact
model has no access-control/authorization fact, and inventing one
would mean guessing at intent the engine can't verify — exactly the
kind of fake support this project's grounding stage exists to
prevent. `parent_kind` needed no such invention: `Call::parent` already
existed (every call tree needs it for depth/ancestry), so the
only additive change here was a DSL attribute and a matcher
filter reading a field that was already there.

**The pattern this unlocks.** `patterns/delegatecall_reachable_selfdestruct.rcdsl`
uses a single evidence clause, `call(kind: SelfDestruct, succeeded:
true, parent_kind: Delegate)`, to flag a `SELFDESTRUCT` that executed
as the *direct child* of a `DELEGATECALL` frame — the structural shape
of the 2017 Parity multisig wallet library incident, where a
`SELFDESTRUCT` reachable only via `DELEGATECALL` from proxy wallets
destroyed the shared library contract itself, freezing every wallet
still depending on it. This is a strictly more specific (and more
dangerous) shape than the existing `unsafe_selfdestruct` pattern's
"a SELFDESTRUCT call succeeded anywhere" signature: destroying a
delegatecalled library breaks every proxy that depends on it, not just
the one caller in this trace. The two patterns are complementary, not
competing — `demo/delegatecall_reachable_selfdestruct.json` matches
both (see the "Exploit corpus" table above), and
`demo/selfdestruct_direct_call_not_delegated.json` demonstrates the
precision `parent_kind` adds: an otherwise-identical successful
`SELFDESTRUCT`, but reached via a plain `CALL` rather than a
`DELEGATECALL`, still correctly matches `unsafe_selfdestruct` but is
correctly rejected by `delegatecall_reachable_selfdestruct`.

**Taxonomy.** This family maps to its own SCWE entry, SCWE-038
("Insecure Use of Selfdestruct") — deliberately *not* reused from
`unsafe_selfdestruct`'s SCWE-050, since the two are genuinely different
findings that may both legitimately fire on the same trace, and
collapsing them onto one taxonomy code would blur that distinction. In
SWC, both families map to the same SWC-106 ("Unprotected SELFDESTRUCT
Instruction"): the SWC registry has no delegatecall-specific
selfdestruct entry, and SWC-106's own description already fits this
shape exactly, so reusing it is a genuine fit, not a forced one. See
`crates/taxonomy/src/scwe.rs` and `crates/taxonomy/src/swc.rs` for the
full rationale.

**Backwards compatibility.** `parent_kind` is optional, like every
other `call(...)` attribute; every pre-existing pattern
compiles and matches exactly as before.

**Scope note.** Like `unsafe_selfdestruct`, this pattern cannot verify
whether the delegatecall itself was authorized, or whether the
library's own destructor function lacked an access modifier in
source — the fact model still has no access-control fact (see the
"Access-control failure" row just below). It surfaces the structural
shape a reviewer needs to check, not a proof that access control was
missing.

### Transitive delegatecall reachability (`ancestor_kind`)

This generalizes `parent_kind` from a one-hop check to a full
ancestor-chain walk: a new `call(...)` attribute, `ancestor_kind`
(`crates/dsl/src/schema.rs`), dereferences `Call::parent` *repeatedly*,
back to the root, and asks whether **any** ancestor — not only the
immediate parent — has the given kind
(`crates/matcher/src/predicate.rs`'s `has_ancestor_of_kind`).

**Why this, and why now.** Every other still-deferred family (see
"Exploit families evaluated but not added" below) is blocked by the
fact model having no access-control/intent fact, and every one of
those was already ruled out for the same reason: adding
a pattern for them would mean guessing at intent the engine cannot
verify. `ancestor_kind` needed no such invention — `Call::parent`
already existed and `parent_kind` already walked it
once; the only change here is walking it *transitively*
instead of once. It reuses, unchanged: the `Call` fact type, the
`call_kind_matches` helper both `parent_kind` and `ancestor_kind`
share, the matcher's evidence/candidate/grounding pipeline (grounding
re-verifies `ancestor_kind` "for free," the same way it does every
other `call(...)` attribute, by re-running
`matcher::predicate::evaluate` against its own independently re-walked
trace — no grounding-specific code was needed), and the existing
`DelegatecallReachableSelfdestruct` family/taxonomy (SWC-106 /
SCWE-038): this is the same finding as the one-hop `parent_kind` pattern's, at a different
structural granularity, not a new vulnerability class.

**Why `parent_kind` alone left a real gap.** `parent_kind` only
inspects a SELFDESTRUCT's *direct* parent frame. But the danger in the
Parity-library shape comes from executing inside the delegatecall's
borrowed storage/code context, not specifically from being its literal
first child. A realistic variant — `DELEGATECALL → CALL →
SELFDESTRUCT`, e.g. the delegatecalled library code makes one more
internal call before destructing — is still exactly as dangerous, but
`parent_kind: Delegate` reports `0 candidate match(es)` on it: a false
negative.

**The pattern this unlocks.**
`patterns/delegatecall_reachable_selfdestruct_transitive.rcdsl` uses
`call(kind: SelfDestruct, succeeded: true, ancestor_kind: Delegate)` to
flag a SELFDESTRUCT reachable via a DELEGATECALL *anywhere* in its
ancestor chain, not just its direct parent.
`demo/delegatecall_reachable_selfdestruct_transitive.json` (a
`DELEGATECALL → CALL → SELFDESTRUCT` trace) demonstrates the gap
directly: it is `GROUNDED` by this new pattern and by
`unsafe_selfdestruct`, but **not** by the one-hop
`delegatecall_reachable_selfdestruct` — verified explicitly by
`integration-tests/tests/exploit_corpus.rs`'s
`delegatecall_reachable_selfdestruct_transitive_catches_a_non_direct_delegatecall`
test, which asserts on all three outcomes in one place.
`demo/selfdestruct_delegatecall_sibling_not_ancestor.json` is the
matching negative case: a `DELEGATECALL` is present in the trace, but
only as a *sibling* of the branch containing the SELFDESTRUCT, never
on its ancestor path — `ancestor_kind` correctly does not fire, proving
it checks actual tree ancestry, not mere trace-wide presence of a
DELEGATECALL anywhere.

**Relationship between the two patterns.** `delegatecall_reachable_selfdestruct`
and `delegatecall_reachable_selfdestruct_transitive`
are kept as two separate, complementary patterns rather
than one replacing the other. The transitive pattern strictly subsumes
the direct-parent pattern in *coverage* (every direct-child case is
also an ancestor-chain case), but the direct-parent pattern remains
the tighter, more immediately actionable signature for a reviewer —
"the delegatecall and the destructive call are adjacent" is a smaller,
easier-to-audit claim than "somewhere up this call's ancestry." Both
map to the same `DelegatecallReachableSelfdestruct` family and the
same SWC-106 / SCWE-038 taxonomy entries by design.

**Backwards compatibility.** `ancestor_kind` is optional, like every
other `call(...)` attribute; every pre-existing pattern —
including the `parent_kind`-based pattern — compiles and
matches exactly as before. No fact-model, ingestion, or grounding
changes were required.

**Scope note.** Identical to `delegatecall_reachable_selfdestruct`:
this pattern cannot verify whether the delegatecall itself was
authorized, or whether the destructor lacked an access modifier in
source. It surfaces the structural shape a reviewer needs to check,
not a proof that access control was missing.

### Spoofed transfer events (`log(topic0: ..., decoded: ...)`)

This adds a new predicate kind, `log(...)` (`crates/dsl/src/schema.rs`), exposing `fact_model::LogEvent` — every ingested EVM log — to the DSL for the first time. Every prior predicate kind (`call`, `storage`, `value_flow`, `token_transfer`, `transaction`) already existed; `log` is the sixth. This was not a gap earlier work missed carelessly: `FactRef::Log` and its full plumbing through `matcher::identity`, `matcher::ordering`, and `grounding::engine` were already present (logs have always been directly citable evidence), but no predicate kind ever produced a `Log`-backed [`Binding`], so no pattern could actually ask about a log's own content.

Two attributes, both fully, structurally checkable — unlike `storage(role: ...)` and `token_transfer(unexpected: ...)`, `log(...)` never records an `unresolved_attrs` entry:

- `topic0` exact-matches a log's first topic — the raw, publicly-computable `keccak256` event-signature hash the EVM itself records. This mirrors `call(selector: ...)`'s existing idiom exactly: comparing raw bytes the trace already carries, with no ABI parameter decoding and no signature-name database.
- `decoded` grounds against whether some `fact_model::TokenTransfer` in the same trace cites this log as its `log_id` — i.e. whether `ingestion`'s existing best-effort `Transfer(address,address,uint256)` decoder (`crates/ingestion/src/normalize.rs::try_decode_token_transfer`) was able to interpret this log's full shape (exactly 3 topics, 32-byte data). This is a structural fact about the log's own shape, not a claim about intent.

**Why this, and why now.** Every family in the README's own "Exploit families evaluated but not added" table (below) is blocked by the fact model having no access-control/intent fact — a wall identified early on and confirmed still standing ever since. `log(...)` needed no such invention: `LogEvent` already existed, fully ingested, with its own `FactRef` variant already wired through every downstream crate; the only actual gap was that no `dsl::schema` entry, `matcher::predicate` evaluator, or `grounding::Verifier` ever exposed it.

**The pattern this unlocks.** `patterns/spoofed_transfer_event.rcdsl` uses a single evidence clause, `log(topic0: "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef", decoded: false)`, to flag a log carrying the standard ERC-20/ERC-721 `Transfer` event's topic0 that is not actually shaped like a decodable transfer (wrong topic count, wrong data length, or both). This is the real, publicly documented "fake deposit event" technique used against exchanges and wallets that recognize a token deposit by filtering `eth_getLogs` on `topics[0]` alone — a common, cheap shortcut — without validating that the log also carries the two indexed addresses and 32-byte amount a genuine `Transfer` requires. `demo/spoofed_transfer_event.json` has exactly this shape: a log with the `Transfer` topic0 but only one topic and no data. `demo/real_transfer_event.json` is the matching negative case — a properly-shaped, 3-topic `Transfer` log that `ingestion` already decodes into a real `TokenTransfer` fact, so `decoded: false` correctly does not match it.

**Taxonomy.** This family maps to SCWE-063 ("Insecure Event Emission") — a genuine, unforced fit: SCWE-063 describes emitted event data that does not correspond to the contract's actual state or outcome, which is exactly what a topic0-mimicking, non-decodable log is. It is deliberately left unmapped in SWC, for the same reason `OracleManipulation` and `UnauthorizedUpgrade` are: the SWC registry predates event-log-spoofing's emergence as a distinct, commonly-cataloged weakness, and forcing a fit onto an unrelated entry would misrepresent it.

**Backwards compatibility.** `log` is a new predicate kind, not a new attribute on an existing one, but it required no change to any pre-existing pattern, predicate, or evaluator — every pre-existing pattern compiles and matches exactly as before.

**Scope note.** This pattern cannot (and does not claim to) determine whether the log's emitter intended to deceive an off-chain observer, whether any observer was actually fooled, or distinguish a genuinely malicious spoofed log from an exotic-but-legitimate non-standard event that happens to reuse this exact topic0 hash (vanishingly unlikely, not structurally impossible) — the fact model has no "author's intent" fact, only the log's own recorded shape. It surfaces a reviewable candidate, not a proof of malice.

### Exploit families evaluated but not added

The following were in scope for this phase but are not genuinely
expressible with the current fact model / DSL schema without
inventing evidence the engine can't actually check. None of these were
implemented — adding a pattern that *looks* like it detects them
without real grounding would be exactly the kind of fake support this
project's grounding stage exists to prevent.

("Unchecked external call" was originally listed here too; it has
since been implemented as the `unchecked_external_call` pattern — see
the "Exploit corpus" table above — once the minimal, additive
`succeeded: Bool` extension this table proposed was reviewed and
approved. "Storage collision" was also originally listed here; it has
since been implemented as the `delegatecall_storage_collision`
pattern — no fact-model extension was needed for it, only the
taxonomy registration this table's own "minimal extension" column
proposed. Unsafe `selfdestruct` was originally listed here too; it has
since been implemented as the `unsafe_selfdestruct` pattern —
Support for it added exactly the `CallKind::Selfdestruct` variant this
table's "minimal extension" column proposed, plus matching ingestion,
DSL, and converter support, per that approved plan.)

| Exploit | Blocker | Minimal extension that would be needed |
|---|---|---|
| tx.origin authentication | No opcode-level or bytecode-level fact at all — `fact-model::Call` has no field describing *how* a caller was authenticated. | A new fact type capturing comparison/branch instructions on `ORIGIN` vs `CALLER`, which requires instruction-level tracing, not just call-graph traces. |
| Access-control failure | No fact captures authorization intent (owner/allowlist/role-based modifier checks) — `role: AccessControl` exists in the `storage` schema but is exactly as unverifiable as `role: PriceOracle` above. | Would need the same kind of instruction/source-level fact as tx.origin, not a matcher-level fix. |
| Initialization bug / proxy upgrade mistake | No "has this contract been initialized" fact; distinguishing a legitimate first-time initializer write from a re-initialization requires tracking constructor/initializer-call history across multiple transactions, which single-trace analysis doesn't have. | Would need a multi-transaction or persistent-state extension beyond this project's current single-trace scope — significant, not proposed here. |
| Flash-loan price manipulation | Same root cause as oracle manipulation: any pattern precise enough to mean "flash loan" rather than "any large value in/out" would need `role: PriceOracle` or equivalent, which is unresolvable. A version *without* the role check would only detect "a large value came in and went back out in one tx" — true of many legitimate flash loans, arbitrage, and DEX routes, not just attacks — so it would be a poor detector, not a corner-cut good one. | Same as oracle manipulation: `storage.role` needs real backing (e.g. resolving Chainlink/Uniswap-TWAP-shaped storage layouts), a nontrivial ingestion-side feature. |

## Trace format converters

`crates/converters` turns real Ethereum tooling output into Root
Cause's raw schema, so people don't have to hand-write
`RawTraceDocument` JSON themselves. A converter is a pure function —
no network I/O — that takes bytes already retrieved by some other
means and emits a JSON file the existing, unmodified `analyze` command
consumes exactly as it always has.

### Geth `debug_traceTransaction` / `callTracer` — supported, verified

```sh
cargo run -p converters --bin geth-convert -- \
    --input demo/samples/geth_calltracer_goerli.json \
    --tx-hash 0xc0ffcf21dc1881c3f20160b530d76f1d37af7b40552f7c59d3f339bad3023dad \
    --block 0x876123 \
    --chain-id 0x5 \
    --nonce 0x1 \
    --output demo/converted/geth_goerli.json

cargo run -p cli -- analyze demo/converted/geth_goerli.json --patterns patterns/
```

`demo/samples/geth_calltracer_goerli.json` is a **real** callTracer
response (Goerli tx `0xc0ffcf...3dad`, geth v1.10.26), saved verbatim
from a publicly reported go-ethereum issue
([#26726](https://github.com/ethereum/go-ethereum/issues/26726)) —
only the long ABI-encoded calldata payloads were trimmed, since call
structure (not calldata content) is what this converter maps. Running
the two commands above against it produces a real 4-call nested tree
(`CALL → DELEGATECALL → CALL → DELEGATECALL`) faithfully preserved
end to end, and correctly reports `0 candidate match(es)` — every
pattern in `patterns/` needs storage or token-transfer evidence this
converter cannot supply from `callTracer` alone (see the next
section). This is exercised as an integration test in
`integration-tests/tests/geth_converter.rs`, which runs both compiled
binaries, not library internals.

**What's faithfully converted:** the call tree (`from`/`to`/`value`/
`gas`/`gasUsed`), call kind, and success/failure (`succeeded =
error.is_none()`).

**What's deliberately left empty, not guessed at:** storage changes
and logs. `callTracer` carries neither. Geth's `prestateTracer`
(`diffMode: true`) does carry storage diffs, but only as a flat
`{address: {storage: {...}}}` map with **no attribution to which call
in the tree produced each write** — and Root Cause's raw schema
requires that attribution (nesting storage changes inside the call
node that caused them), specifically because no single Geth tracer
supplies it on its own. Reconstructing that attribution would need a
heuristic (e.g. "last call frame targeting this address wrote this
slot"), which breaks down the moment an address is called more than
once in a trace, or a `DELEGATECALL` is involved. Getting that wrong
silently in a security tool seemed worse than reporting nothing, so
this converter reports nothing rather than guess — flagging this for
a decision on whether a documented, opt-in heuristic is worth adding
later, rather than adding one unilaterally.

`SELFDESTRUCT` frames convert like any other call kind:
`fact_model::CallKind::Selfdestruct` (see the "Exploit corpus"
table's `unsafe_selfdestruct` entry above), so there is now a faithful
target variant for this converter to produce.

### Erigon `trace_*` / `debug_traceTransaction` — supported, verified

Erigon implements the same `callTracer` interface as Geth (it's
explicitly Geth-API-compatible), so `geth-convert` was expected to
accept a real Erigon `callTracer` response unchanged — but per this
project's own rule ("never claim support unless verified against a
real sample"), that stayed an unverified inference until it was
closed with a real sample instead of further inference:

```sh
cargo run -p converters --bin geth-convert -- \
    --input demo/samples/erigon_calltracer_polygon.json \
    --tx-hash 0x19afd31a9327af30b1d7abf1d46efa228af0747268e618fcf06dcb62655999f8 \
    --block 0x2b710e1 \
    --chain-id 0x89 \
    --nonce 0x1 \
    --output demo/converted/erigon_polygon.json

cargo run -p cli -- analyze demo/converted/erigon_polygon.json --patterns patterns/
```

`demo/samples/erigon_calltracer_polygon.json` is a **real** Erigon
`callTracer` response (Polygon mainnet tx
`0x19afd31a...5999f8`, Erigon v2.43.0), saved verbatim from a publicly
reported erigontech/erigon issue
([#7568](https://github.com/erigontech/erigon/issues/7568)) — nothing
trimmed, since (unlike the Geth Goerli sample) its calldata was already
short. Running the two commands above against it converts and analyzes
without modifying `converters::geth` *at all*: Erigon's response uses
the identical field names, hex encoding, and uppercase `type` values
(`CALL`, `STATICCALL`) `GethCallFrame` already deserializes, and the
top-level `error: "execution reverted"` maps through the same
`error.is_none()` rule as any Geth frame — this really is the same
schema, not merely a compatible-looking one. The real call tree (root
`CALL`, reverted, with 3 `STATICCALL` children — a Uniswap-V2-style
reserve lookup across three pools) converts faithfully and correctly
reports `0 candidate match(es)`, for the identical reason the Geth
sample does: every pattern in `patterns/` needs storage or
token-transfer evidence `callTracer` alone — from either client —
cannot supply (see the previous section). This is exercised as an
integration test,
`crates/converters/tests/geth_converter.rs::erigon_calltracer_sample_converts_and_analyzes_with_no_false_positive`,
which runs both compiled binaries against the real sample, the same
way the Geth test does.

**Why this needed no code change, and why that's the right outcome, not
a coincidence:** `converters::geth::convert` was never actually
Geth-specific in its parsing logic — it deserializes `callTracer`'s
documented JSON-RPC shape, which both clients implement identically by
design (Erigon's own stated goal is Geth API compatibility). Verifying
this against a real sample rather than only asserting it from
documentation is the entire value this verification adds here: the code
path, the fact model, the matcher, and the CLI are all completely
unchanged and already sufficient.

### Foundry, Hardhat, Tenderly, Nethermind — not implemented this phase

None of these were implemented. Each would need a real sample response
studied and converted before any claim of support, and none was
available:

- **Foundry** (`cast run --debug` / `forge test -vvvv`) has no stable,
  publicly documented JSON output schema — its trace format
  (`CallTraceArena`/`DebugArena`) is an internal Rust data structure
  primarily rendered as colored terminal output, not a machine-JSON
  contract. Foundry's Anvil node *does* expose Geth-compatible
  `debug_traceTransaction`, though, so `geth-convert` should already
  handle Anvil-sourced traces — again unverified against a real Anvil
  response.
- **Hardhat**: whether vanilla Hardhat Network's `debug_traceTransaction`
  emits Geth's `structLogs` opcode-log shape or a `callTracer`-compatible
  shape by default wasn't confirmed against a real sample in this
  session — needs checking before claiming either way.
- **Tenderly**: exports its own proprietary, richer JSON shape (decoded
  function calls, asset transfers) via its dashboard/API, structurally
  different from `callTracer`. Needs a real exported sample to map.
- **Nethermind**: implements `debug_traceTransaction` and its own
  Parity-style `trace_*` API, similarly to Erigon — plausible but
  unverified.

If you can supply a real sample response for any of these (a saved
JSON-RPC result, not documentation prose), I can study its actual
schema and build a verified converter the same way as Geth's above.




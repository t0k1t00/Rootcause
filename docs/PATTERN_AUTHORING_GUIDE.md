# Writing a new pattern

This is a task-oriented, from-scratch walkthrough for adding a new
`.rcdsl` detection pattern to Root Cause. It assumes no prior familiarity
with the codebase. If you're contributing to the DSL engine itself
(parser, matcher, grounding) rather than authoring a pattern, read
`crates/dsl/README.md`, `crates/matcher/README.md`, and
`crates/grounding/README.md` instead — this guide is about *using* the
DSL, not implementing it.

## The one rule that matters most

**Every predicate you write must ground against a fact the trace actually
records — never an inferred or assumed one.** Root Cause's entire design
centers on this: a pattern's evidence clauses are independently
re-verified against the raw trace after matching (the "grounding" stage),
and a clause that can't be re-verified makes the whole finding `Abstain`
rather than a false `Grounded`. If what you want to detect requires
knowing something the trace doesn't record — who is authorized to call a
function, what a contract's deployer intended, whether an off-chain
observer would be fooled — it likely cannot be expressed as a precise
pattern today. See the "Exploit families evaluated but not added" section
of the top-level `README.md` for several real examples of exactly this
wall, and what would be required to remove it.

## 0. Scaffold the files with `rootcause new-pattern`

You don't have to hand-copy an existing pattern's files to start. Run:

```sh
rootcause new-pattern my_new_pattern --family MyFamilyName
```

This creates `patterns/my_new_pattern.rcdsl` (a TODO-annotated starter
pattern), `demo/my_new_pattern_{positive,negative}.json` (minimal,
valid, single-call traces you'll edit into real cases in step 6),
`patterns/my_new_pattern.README.md`, and
`docs/taxonomy-todo/my_new_pattern.md` (a reminder for step 5, deleted
once the mapping exists). Pass `--dir <PATH>` to scaffold into a
workspace root other than the current directory, and `--force` to
overwrite files that already exist at those paths.

The rest of this guide — steps 1 through 8 — is what to put in those
scaffolded files.

## 1. Decide what you're detecting, and find the fact-model support for it

Before writing any DSL, check `crates/fact-model/src/` (`call.rs`,
`storage.rs`, `log.rs`, `transaction.rs`, `ids.rs`) for what's actually
recorded per call, storage write, log, and transaction. If the fact you
need isn't there yet, that's a fact-model change, not a pattern-authoring
task — open an issue describing the gap rather than trying to work around
it with a looser, less precise predicate.

## 2. Predicate reference

Every evidence clause has the shape `kind(attr: value, ...)`. Six
predicate kinds exist today:

| Kind | Grounds against | Attributes |
|---|---|---|
| `call` | `fact_model::Call` | `kind` (required — `Call`, `Delegate`, `Static`, `SelfDestruct`, ...), `reentrant`, `value_flow` (`In`/`Out`), `min_value`, `selector` (4-byte hex), `succeeded`, `parent_kind`, `ancestor_kind` |
| `storage` | `fact_model::StorageChange` | `changed` (required), `role` (`PriceOracle`/`Balance`/`Accounting`/`AccessControl` — see note below), `slot` (hex word), `call_kind` |
| `value_flow` | native-value transfer | `direction` (required — `In`/`Out`), `min`, `max` |
| `token_transfer` | `fact_model::TokenTransfer` | `direction` (required), `unexpected`, `token` (address) |
| `transaction` | `fact_model::Transaction` | `status` (required — `Success`/`Reverted`), `gas_used_min` |
| `log` | `fact_model::LogEvent` | `topic0` (required — 32-byte hex event-signature hash), `decoded` (whether a `TokenTransfer` fact was successfully decoded from this log) |

Run `cargo doc -p dsl --no-deps --open` and browse `dsl::schema` for the
authoritative, always-current version of this table, including the doc
comment on each predicate explaining exactly what it grounds against and
why each attribute is honestly checkable.

A few attributes (`storage(role: ...)`, `token_transfer(unexpected: ...)`)
are intentionally weaker: they can match structurally but cannot always be
independently re-verified, so they show up as `unresolved_attrs` at
grounding time and can push a candidate to `Abstain` even when it
structurally matched. This is deliberate, not a bug — read the doc
comments on `dsl::schema::STORAGE` and `dsl::schema::TOKEN_TRANSFER` for
the full reasoning before relying on either in a new pattern.

## 3. Evidence, constraints, and correlation

```
pattern my_new_pattern version 1 {
    family: MyFamilyName
    severity: High
    tags: ["short", "descriptive", "tags"]
    references: ["SCWE-XXX"]   // optional; see step 5

    evidence {
        required first_clause: call(kind: Delegate)
        required second_clause: storage(changed: true)
        optional supporting_clause: transaction(status: Reverted)
    }

    // same_call asserts every listed clause's binding came from the
    // identical call node — use this whenever your pattern's precision
    // depends on two facts happening in the *same* call, not merely
    // somewhere in the same trace.
    same_call: [first_clause, second_clause]

    constraint: first_clause and second_clause
}
```

- `severity` is one of `Low`, `Medium`, `High`, `Critical`.
- `family` is a bare identifier grouping related patterns (e.g. multiple
  versions of the same detector, or variants of one vulnerability class)
  for taxonomy mapping — see step 5.
- `constraint` is a boolean expression (`and`/`or`/`not`) over your named
  evidence clauses. `required` clauses must all be satisfiable for a
  candidate to exist at all; `optional` clauses refine confidence without
  gating candidacy.
- If your pattern's precision depends on structural relationships beyond
  "these facts exist somewhere in the trace" — same call, parent/ancestor
  relationship, sequence ordering — use `same_call`, `parent_kind`/
  `ancestor_kind`, or `sequence:` rather than relying on coincidental
  co-occurrence. Every existing pattern in `patterns/` demonstrates one of
  these; `patterns/delegatecall_storage_collision.rcdsl` and
  `patterns/spoofed_transfer_event.rcdsl` are good short references.

## 4. Write the doc comment first

Every pattern file in `patterns/` opens with a substantial comment
explaining: what the pattern detects, why the chosen evidence is a
genuine structural signature (not a coincidental proxy) for it, and — a
dedicated "Scope note" — what the pattern *cannot* determine and why
(usually: intent, authorization, or off-chain state the fact model
doesn't carry). Write this before the DSL body. If you can't write a
convincing scope note, that's a signal the pattern may be over-claiming;
revisit step 1.

## 5. Map it to a taxonomy

Add an entry to `crates/taxonomy/src/scwe.rs` (and, only where a genuine,
unforced fit exists, `crates/taxonomy/src/swc.rs`) mapping your
`PatternFamily` name to a real [SCWE](https://scs.owasp.org/SCWE/) code —
search the registry for a real match rather than picking the closest
available number. It is entirely acceptable, and already precedented
(`OracleManipulation`, `UnauthorizedUpgrade`, `SpoofedTransferEvent`), to
leave a family unmapped in SWC if the SWC registry genuinely has no
counterpart — do not force a fit. Add a test asserting the mapping,
mirroring the existing tests in both files.

## 6. Build a positive and a negative demo trace

Every pattern needs at least one trace where it fires and one
structurally similar trace where it correctly does not (to guard against
false positives). Look at an existing pair for the JSON shape — e.g.
`demo/spoofed_transfer_event.json` (positive) and
`demo/real_transfer_event.json` (negative), or
`demo/delegatecall_storage_collision.json` and
`demo/delegatecall_storage_adjacent_not_producing.json`. The raw trace
schema itself is documented in `crates/ingestion/src/raw.rs`.

Verify both by hand before writing any test:

```sh
cargo run -p cli -- analyze demo/my_positive_case.json --patterns patterns/
cargo run -p cli -- analyze demo/my_negative_case.json --patterns patterns/
```

The positive case should show `[GROUNDED] my_new_pattern`; the negative
case should show `found 0 candidate match(es)` (or a candidate that
`Abstain`s, if that's the honest outcome for that trace).

Once both traces are in place at the conventional
`demo/my_new_pattern_{positive,negative}.json` paths, `rootcause
validate-pattern patterns/my_new_pattern.rcdsl` runs the same two checks
(and syntax, metadata, and taxonomy) in one command — see step 9.

## 7. Add integration tests

Add both cases to `integration-tests/tests/exploit_corpus.rs`, following
the existing tests as a template (e.g.
`spoofed_transfer_event_is_grounded_high` /
`real_transfer_event_produces_no_finding`). These are what actually keep
your pattern working as the engine evolves.

## 8. Document it

Add a row to the CLI-examples table in `README.md`'s exploit-corpus
section, and — if the pattern introduces a new idea worth explaining
(a new predicate, a new correlation mechanism), not just a new instance of
an existing one — a short prose section, matching the style of the
existing "Spoofed transfer events" / "Transitive delegatecall
reachability" sections.

## 9. Validate

First, run the pattern-specific checks:

```sh
rootcause validate-pattern patterns/my_new_pattern.rcdsl
rootcause format-pattern patterns/my_new_pattern.rcdsl --write
rootcause doctor
```

`validate-pattern` re-runs syntax, metadata, compilation, taxonomy, and
the positive/negative trace checks from steps 5–6 as one report — fix
everything it flags before moving on. `format-pattern --write`
canonicalizes the file's style in place (run `--check` instead in CI or
a pre-commit hook — it exits non-zero without writing anything).
`doctor` catches library-wide issues no single-pattern check can, like a
duplicate pattern id.

Then run the full workspace validation:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo test --workspace
```

All of the above must pass before opening a PR — see `CONTRIBUTING.md`
for the full checklist.

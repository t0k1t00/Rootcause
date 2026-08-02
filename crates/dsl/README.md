# dsl

The Pattern DSL: a strongly-typed language for describing deterministic
exploit patterns, plus the parser, validator, and compiler that turn
source text into an immutable, matcher-ready representation.

This crate is responsible **only** for parsing, validating, representing,
and compiling pattern definitions. It does not perform matching,
evaluation, or grounding against a real `fact_model::Trace` — those
responsibilities belong to `matcher` and `grounding`.

## Pipeline

Source text flows through four independent stages, each in its own
module, keeping lexical parsing, structural parsing, and semantic
validation separate:

1. `lexer` — source text to a `Token` stream (lexical parsing).
2. `parser` — token stream to `ast::PatternAst` (structural parsing).
3. `validate` — `ast::PatternAst` to `diagnostics::Diagnostics` (semantic
   validation; empty means valid).
4. `compile` — validated `ast::PatternAst` to `ir::CompiledPattern`
   (compilation into the immutable internal representation).

`diagnostics::Diagnostic` is the single rich-diagnostic type shared by
all four stages: every diagnostic carries a source location, a name for
the offending construct, a reason, and (where applicable) an actionable
suggestion.

## Public API

Most callers only need three top-level functions, which internally drive
the pipeline stages and translate their diagnostics into a `DslError`:

- `parse_pattern` — parse only (stages 1-2), for callers that want to
  inspect the surface AST directly (e.g. a formatter or linter).
- `compile_str` — parse, validate, and compile a pattern from source text
  in one call; the common case.
- `load_pattern_file` — read a pattern from disk, then `compile_str` it.

## Usage

```rust
let pattern = dsl::compile_str(r#"
    pattern classic_reentrancy version 1 {
        family: Reentrancy
        severity: Critical
        evidence {
            required vulnerable_call: call(kind: External, reentrant: true)
            required state_write: storage(changed: true)
        }
        constraint: AND(vulnerable_call, state_write)
    }
"#)?;
```

A `CompiledPattern` is designed to be produced once, at pattern-library
load time, and then matched against many traces by `matcher`.

## Status

Complete. See crate-level rustdoc (`cargo doc -p dsl --open`) for the
full API reference and the DSL grammar.

# 0004. CI toolchain and coverage strategy

Status: Accepted

## Context
Neither the Architecture nor Engineering Specification document specifies
a test runner or coverage tool (the Engineering Spec's "CI" section states
this outright: not specified). The task instructions mark cargo-nextest
and coverage configuration as "if appropriate," delegating the decision.

## Decision
- **Test runner**: `cargo test` (built-in) for this task, not `cargo-nextest`.
  Rationale: nextest's main advantages (test isolation via per-test
  process, faster wall-clock on large suites) do not yet apply — at this
  stage every crate has zero-to-few trivial tests. Introducing nextest now
  would add a CI dependency with no present benefit. This is reconsidered
  once `matcher`/`grounding` accumulate enough integration-style tests
  that process-per-test isolation and nextest's retry/flake handling
  becomes valuable — that will be a new ADR, not a silent tooling swap.
- **Coverage**: `cargo llvm-cov`, chosen over `cargo-tarpaulin` because it
  uses the same LLVM source-based coverage instrumentation as `rustc`
  itself (more accurate branch coverage, better multi-crate workspace
  support) and is what `cargo` upstream documentation itself points to.
  Wired into CI as a non-blocking report upload for this task (coverage
  thresholds are not yet meaningful with near-empty crates); a coverage
  *gate* (minimum %) is deferred to a later ADR once `fact-model` and
  later crates have real logic to measure.

## Consequences
CI's `coverage` job is present but informational-only in this task; it
will need a follow-up ADR to convert it into a blocking check with a
concrete threshold once there's non-trivial logic to cover. Switching test
runners or coverage tools later requires only a CI workflow edit plus a
new/superseding ADR — no crate code depends on either choice.

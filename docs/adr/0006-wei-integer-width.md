# 0006. `Wei` narrowed to `u128`, not a full 256-bit integer

Status: Accepted

## Context

EVM balances and call values are represented on-chain as 256-bit words,
same as arbitrary storage slot values. Neither the Architecture nor
Engineering Specification document specifies an integer width for value
amounts in the fact model — Assumption A-4 says only that the fact model
must be able to express "net value extracted." This is therefore a
delegated decision: no source document specifies it, and the choice does
not change any externally visible grounding semantics (a `Wei` value
still round-trips and compares correctly for every value any real
transaction could contain).

## Decision

`fact-model::Wei` wraps a `u128`, not a 256-bit integer type. Total ETH
supply (~120,000,000 ETH ≈ 1.2 × 10²⁶ wei) fits inside `u128`
(max ≈ 3.4 × 10³⁸) with 12 orders of magnitude of headroom — no real
transaction's value or aggregate value-flow computation can exceed this.

Storage slot *values* (`fact_model::Word`) are explicitly **not**
narrowed the same way, because they are arbitrary 256-bit bit patterns
(packed structs, hashes, left-padded addresses) with no economic-quantity
upper bound to exploit.

`Wei::from_word` provides fallible narrowing from a full `Word` (as raw
EVM data will arrive during ingestion), returning `None` rather than
panicking or wrapping if a 256-bit value genuinely doesn't fit — this
keeps the narrowing decision auditable and non-silent at the one point
it's actually exercised, rather than baking an assumption into every
downstream computation.

## Consequences

- Every arithmetic operation on `Wei` (`checked_add`, `checked_sub`) is
  cheaper and simpler than 256-bit arithmetic would be, and needs no
  external big-integer dependency.
- If a future requirement genuinely needs values beyond `u128::MAX` wei
  (which would represent an economically nonsensical transaction under
  any current or foreseeable chain), `Wei`'s internal representation can
  be changed without changing its public API surface (`checked_add`,
  `checked_sub`, `Display`, `Serialize`) — callers depend on the type,
  not its width.
- This does not affect `Word`, which remains the full-width type used
  for storage keys/values and log topics.

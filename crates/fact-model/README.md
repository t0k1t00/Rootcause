# fact-model

Canonical, immutable representation of a decoded EVM transaction trace.

`fact-model` defines the shared vocabulary of facts — calls, storage
changes, value flows, logs, and their relationships — that every other
crate in the Root Cause workspace is built against. It has no dependency
on any other workspace crate: `ingestion` produces fact models,
`matcher`/`grounding` consume them, but `fact-model` itself knows about
neither.

## Design summary

- **Immutable after construction.** Every type is built once via a
  fallible constructor and exposes no mutation API afterward.
- **Arena-indexed, not pointer/`Rc`-based.** `FactArena` owns every
  `Call`, `StorageChange`, `LogEvent`, and `TokenTransfer`; everything
  else refers to them by small `Copy` ID newtypes (`CallId`,
  `StorageChangeId`, `LogId`, `TokenTransferId`). This is what lets a
  grounding result cite "a specific `Call`" without `Rc<RefCell>` or
  lifetimes leaking into every downstream crate.
- **Strongly typed.** No public field is a raw `u64`/`String`/byte
  array; every semantically distinct scalar is a newtype (see the
  `primitives` module).
- **Deterministic and `Send + Sync` by construction**, not by added
  synchronization: no interior mutability, no `Rc`, no hash-order-
  sensitive collections in any public API.

## Module map

| Module | Contents |
|---|---|
| `primitives` | Scalar newtypes (`Address`, `Word`, `Wei`, ...). |
| `ids` | Arena ID newtypes and `FactRef`, the sum type grounding uses to cite an arbitrary fact. |
| `call` | `Call` and `CallKind`. |
| `storage` | `StorageChange` and `StorageSlot`. |
| `value_flow` | The computed (not stored) `ValueFlow` view. |
| `log` | `LogEvent` and `TokenTransfer`. |
| `transaction` | `Transaction` and `TxStatus`. |
| `block` | `BlockContext`. |
| `arena` | `FactArena`, `FactArenaBuilder`, and every accessor over the arena. |

## Usage

```rust
use fact_model::{Address, Call, CallDepth, CallKind, FactArenaBuilder, Gas, Wei};

let mut builder = FactArenaBuilder::new();
let call = Call::new(
    builder.next_call_id(),
    None,
    CallKind::Call,
    CallDepth(0),
    Address::new([1; 20]),
    Some(Address::new([2; 20])),
    Wei::ZERO,
    Gas(21_000),
    Gas(21_000),
    true,
)?;
builder.add_call(call);
let arena = builder.build()?;
```

See `crate::sample_fixtures` in `benchmark-harness` for complete,
realistic worked examples of building a full `Trace` (arena + block +
transaction + metadata).

## Status

Complete. See crate-level rustdoc (`cargo doc -p fact-model --open`)
for the full API reference.

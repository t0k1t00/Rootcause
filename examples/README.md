# Examples

## `unauthorized_upgrade_to`

The first pattern built on the `call(selector: "0x...")` matcher
attribute (see the top-level README's "Function selectors" section).
It flags a proxy `upgradeTo(address)` call (selector `0x3659cea8`)
present anywhere in a trace, where that call was **not** initiated by
the trace's own transaction sender.

Files:

- `unauthorized_upgrade_to.rcdsl` — the pattern definition.
- `positive_unauthorized_upgrade.json` — a trace where the sender
  (`0x0101...01`) calls an intermediate contract (`0x0505...05`),
  which in turn calls `upgradeTo()` on a proxy (`0x0303...03`). The
  `upgradeTo()` call's `from` is the intermediate contract, not the
  transaction's sender — the pattern matches.
- `negative_authorized_upgrade.json` — a trace where the sender calls
  `upgradeTo()` directly. The call's `from` equals the transaction's
  own sender, so `NOT(sender_initiated_upgrade)` is false and the
  pattern does not match.

Run both through the CLI:

```sh
cargo run -p cli -- analyze examples/positive_unauthorized_upgrade.json \
    --patterns examples/unauthorized_upgrade_to.rcdsl

cargo run -p cli -- analyze examples/negative_authorized_upgrade.json \
    --patterns examples/unauthorized_upgrade_to.rcdsl
```

The first run should report `unauthorized_upgrade_to` as grounded; the
second should report no findings for it.

### Known limitation

This pattern cannot (and does not claim to) verify that the caller
genuinely lacked admin/owner rights — the fact model has no
access-control-role fact. It flags "upgradeTo() invoked by an address
other than the trace's own sender" as a reviewable candidate, not a
proof of unauthorized access. See `crates/taxonomy/src/scwe.rs`'s
`UnauthorizedUpgrade` → SCWE-005 mapping note for the same caveat in
context.

## `proxy_upgrade_self_write` (`same_call:`)

A second, separate pattern demonstrating the `same_call:` DSL section.
`unauthorized_upgrade_to` above was evaluated for
promotion to `same_call:` first, since it was the assigned candidate —
see "Why `unauthorized_upgrade_to` was not promoted" below for why a
new pattern was added instead.

`proxy_upgrade_self_write` requires that the call matching the
`upgradeTo(address)` selector (`0x3659cea8`) is the *same* call that
wrote the proxy's implementation storage slot (slot 0, the same
convention `patterns/delegatecall_storage_collision.rcdsl` uses) —
closing a gap `unauthorized_upgrade_to` has no way to close: it never
checks that the call actually mutated any storage at all, only that
its selector matched.

Files:

- `proxy_upgrade_self_write.rcdsl` — the pattern definition.
- `positive_same_call_upgrade.json` — the `upgradeTo()` call's own
  frame contains the slot-0 write. Matches.
- `negative_same_call_upgrade.json` — an `upgradeTo()` call exists,
  and a slot-0 write exists elsewhere in the trace, but on a
  *different* call. Before `same_call:`, `sequence:`-based ordering
  could not tell these two shapes apart; with it, the pattern
  correctly does not match.

```sh
cargo run -p cli -- analyze examples/positive_same_call_upgrade.json \
    --patterns examples/proxy_upgrade_self_write.rcdsl

cargo run -p cli -- analyze examples/negative_same_call_upgrade.json \
    --patterns examples/proxy_upgrade_self_write.rcdsl
```

The first run reports `proxy_upgrade_self_write` as grounded, with
"same_call correlation confirmed." in its explanation. The second
reports no candidate matches at all — `same_call:` correlation
prevents the two evidence clauses from ever binding to the same
candidate in the first place, so grounding never even runs.

### Why `unauthorized_upgrade_to` was not promoted

`unauthorized_upgrade_to` has exactly one substantive evidence clause
(`upgrade_call`); its second clause, `sender_initiated_upgrade`, is a
negated variant of the same predicate used to disqualify a match, not
a separate fact to correlate. There is nothing for `same_call:` to
add there: with only one evidence clause producing a fact, "these
clauses share a `CallId`" is vacuously about a single binding.

Two other existing patterns do have two ANDed evidence clauses
(`classic_reentrancy`, `oracle_manipulation`), but in both cases the
two facts are *supposed* to come from different calls — that is the
vulnerability. In `classic_reentrancy`, `state_write_after_call` is
produced by the reentrant callback (a nested call), not by
`vulnerable_call`'s own frame (see `demo/dao.json`); forcing
`same_call:` there would make the DAO-shaped exploit fixture stop
matching. In `oracle_manipulation`, the donation transfer and the
price read structurally happen in different contracts' calls. Adding
`same_call:` to either would silently break a correct, already-tested
pattern rather than demonstrate the new capability, and neither is
in scope under this project's "don't redesign existing patterns"
constraint.

`proxy_upgrade_self_write` was added instead: a small, self-contained
pattern whose two evidence clauses are genuinely supposed to share a
`CallId`, so the demonstration is honest about what `same_call:` is
for.

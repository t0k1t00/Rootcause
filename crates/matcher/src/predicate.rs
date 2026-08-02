//! Predicate evaluation: for one [`CompiledPredicate`], find every fact
//! in a [`Trace`] that satisfies it.
//!
//! # Predicate semantics
//!
//! Every attribute `dsl::schema` defines is evaluated here against a
//! concrete, documented structural rule. Two address-relative attributes
//! (`call.value_flow` and `token_transfer`/`value_flow`'s `direction`)
//! need an anchor address to be "in"/"out" *relative to*, which the DSL
//! schema does not itself specify (it only says `In`/`Out`, not relative
//! to what). This crate's modeling choice — documented once here, not
//! repeated per predicate — is: **relative to the trace's own
//! transaction sender (`trace.transaction.from`)**. This is the only
//! address every trace structurally provides without inventing
//! protocol-specific "victim contract" knowledge `fact-model` does not
//! record: `In` means the fact's value/tokens moved *to* that sender,
//! `Out` means *from* it.
//!
//! # Attributes this crate cannot evaluate
//!
//! Two attributes describe a semantic classification with no
//! corresponding structural fact anywhere in `fact-model`:
//! `storage(role: ...)` (which slot is "the price oracle") and
//! `token_transfer(unexpected: ...)` (whether a transfer was
//! "unexpected" by the receiving protocol's own logic). Neither can be
//! computed from `Call`/`StorageChange`/`TokenTransfer`/`Transaction`
//! alone — doing so would require either hardcoded per-protocol
//! knowledge (out of scope for a deterministic structural matcher) or
//! genuine semantic interpretation (the grounding crate's job, per this
//! crate's top-level docs). When a predicate names one of these
//! attributes, this crate does **not** filter on it (every fact that
//! satisfies every attribute it *can* check is still returned), and
//! instead records an [`crate::binding::UnresolvedAttribute`] on the resulting binding
//! so [`crate::engine`] can propagate it into the final
//! [`crate::CandidateMatch`]'s metadata — never silently treating an
//! unchecked claim as a checked one.

use fact_model::{Address, CallKind, FactRef, Trace, TxStatus, Word};

use dsl::ir::{AttrValue, CompiledPredicate};

use crate::index::TraceIndex;

/// One fact that satisfied a predicate, together with everything
/// [`crate::sequence`] and [`crate::engine`] need to use it: its
/// approximate execution-order key and any attribute this crate could
/// not structurally check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// The fact satisfying the predicate.
    pub fact: FactRef,
    /// This fact's approximate execution-order position — see
    /// [`crate::ordering`].
    pub order: Option<u32>,
    /// Attribute names this predicate declared that could not be
    /// structurally evaluated (see this module's own docs).
    pub unresolved_attrs: Vec<&'static str>,
}

/// Evaluate `predicate` against every fact `trace`/`index` provide of the
/// relevant kind, returning every satisfying [`Binding`], sorted by
/// execution order (facts with no order — none currently produced by
/// this crate — sort first, deterministically, via `Option`'s own `Ord`).
///
/// # Complexity
/// `O(facts of the relevant kind in the trace)` — a single linear scan,
/// per predicate, over exactly the fact list the predicate's kind names
/// (`arena.calls()`, `arena.storage_changes()`, etc.). See this crate's
/// top-level docs, "Complexity," for the aggregate cost across an entire
/// pattern/trace pair.
#[must_use]
pub fn evaluate(
    trace: &Trace,
    index: &TraceIndex<'_>,
    predicate: &CompiledPredicate,
) -> Vec<Binding> {
    let mut bindings = match predicate.kind.as_str() {
        "call" => evaluate_call(trace, index, predicate),
        "storage" => evaluate_storage(trace, predicate),
        "value_flow" => evaluate_value_flow(trace, predicate),
        "token_transfer" => evaluate_token_transfer(trace, predicate),
        "transaction" => evaluate_transaction(trace, predicate),
        "log" => evaluate_log(trace, predicate),
        // `dsl::validate` rejects every predicate kind outside
        // `dsl::schema::ALL` before a pattern can ever compile, so a
        // `CompiledPattern` this crate is handed can only name one of
        // the six kinds matched above. An unrecognized kind here means
        // the `CompiledPattern` was not produced by `dsl`'s own
        // compiler; the conservative, safe answer is "matches nothing"
        // rather than guessing.
        _ => Vec::new(),
    };
    bindings.sort_by_key(|b| b.order);
    bindings
}

/// The trace's own sender, used as the anchor address for every
/// `direction`/`value_flow` attribute this module evaluates — see this
/// module's top-level docs.
const fn anchor(trace: &Trace) -> Address {
    trace.transaction.from
}

fn ident<'p>(predicate: &'p CompiledPredicate, name: &str) -> Option<&'p str> {
    match predicate.attr(name) {
        Some(AttrValue::Ident(s)) => Some(s.as_str()),
        _ => None,
    }
}

fn bool_attr(predicate: &CompiledPredicate, name: &str) -> Option<bool> {
    match predicate.attr(name) {
        Some(AttrValue::Bool(b)) => Some(*b),
        _ => None,
    }
}

fn int_attr(predicate: &CompiledPredicate, name: &str) -> Option<i64> {
    match predicate.attr(name) {
        Some(AttrValue::Int(i)) => Some(*i),
        _ => None,
    }
}

fn str_attr<'p>(predicate: &'p CompiledPredicate, name: &str) -> Option<&'p str> {
    match predicate.attr(name) {
        Some(AttrValue::Str(s)) => Some(s.as_str()),
        _ => None,
    }
}

fn call_kind_matches(kind: CallKind, ident: &str) -> bool {
    matches!(
        (kind, ident),
        (CallKind::Call, "External")
            | (CallKind::StaticCall, "StaticCall")
            | (CallKind::DelegateCall | CallKind::CallCode, "Delegate")
            | (CallKind::Create | CallKind::Create2, "Create")
            | (CallKind::Selfdestruct, "SelfDestruct")
    )
}

/// Walk `fact_model::Call::parent` from `start` back toward the root,
/// one edge at a time, returning whether any ancestor visited along the
/// way — not including `start` itself — has the kind `ident` names.
///
/// # Termination
/// `Trace`s are built exclusively via `FactArenaBuilder`, which
/// enforces the call tree is acyclic (a call's `parent` can only name a
/// call built strictly earlier in the same ingestion pass — see
/// `crates/fact-model/README.md`'s construction invariants), so this
/// walk strictly decreases toward a root (`parent == None`) every step
/// and always terminates, in at most the trace's own call-tree depth.
/// If a `call_id` somehow failed to resolve (impossible for a
/// `Trace` built through the normal constructor, but not something this
/// function assumes), the walk simply stops there — the conservative
/// "no match" answer, the same posture every other unresolved-reference
/// case in this module takes.
fn has_ancestor_of_kind(trace: &Trace, start: fact_model::CallId, ident: &str) -> bool {
    let mut current = trace.arena.call(start).and_then(|call| call.parent);
    while let Some(call_id) = current {
        let Some(call) = trace.arena.call(call_id) else {
            return false;
        };
        if call_kind_matches(call.kind, ident) {
            return true;
        }
        current = call.parent;
    }
    false
}

/// Parse a `0x`-prefixed, 64-hex-digit string into a [`Word`], for the
/// `storage(slot: "0x...")` attribute. `fact-model`'s own `Word` type
/// intentionally exposes no public hex-parsing constructor beyond
/// `serde` (its `Deserialize` impl expects a *quoted JSON string*, not a
/// bare literal), so this is a small, self-contained parser rather than
/// a workaround through `serde_json` string-quoting.
fn parse_hex_word(s: &str) -> Option<Word> {
    let digits = s.strip_prefix("0x").unwrap_or(s);
    if digits.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        let hi = (digits.as_bytes()[i * 2] as char).to_digit(16)?;
        let lo = (digits.as_bytes()[i * 2 + 1] as char).to_digit(16)?;
        *byte = u8::try_from(hi * 16 + lo).ok()?;
    }
    Some(Word::new(bytes))
}

/// Parse a `0x`-prefixed, 8-hex-digit string into a 4-byte function
/// selector, for the `call(selector: "0x...")` attribute. Mirrors
/// [`parse_hex_word`] exactly, just at the 4-byte width a selector
/// requires instead of a 32-byte word.
fn parse_hex_selector(s: &str) -> Option<[u8; 4]> {
    let digits = s.strip_prefix("0x").unwrap_or(s);
    if digits.len() != 8 {
        return None;
    }
    let mut bytes = [0u8; 4];
    for (i, byte) in bytes.iter_mut().enumerate() {
        let hi = (digits.as_bytes()[i * 2] as char).to_digit(16)?;
        let lo = (digits.as_bytes()[i * 2 + 1] as char).to_digit(16)?;
        *byte = u8::try_from(hi * 16 + lo).ok()?;
    }
    Some(bytes)
}

fn evaluate_call(
    trace: &Trace,
    index: &TraceIndex<'_>,
    predicate: &CompiledPredicate,
) -> Vec<Binding> {
    let want_kind = ident(predicate, "kind");
    let want_reentrant = bool_attr(predicate, "reentrant");
    let want_value_flow = ident(predicate, "value_flow");
    let want_min_value = int_attr(predicate, "min_value");
    let want_selector = str_attr(predicate, "selector").map(parse_hex_selector);
    let want_succeeded = bool_attr(predicate, "succeeded");
    let want_parent_kind = ident(predicate, "parent_kind");
    let want_ancestor_kind = ident(predicate, "ancestor_kind");
    let anchor_addr = anchor(trace);

    trace
        .arena
        .calls()
        .filter(|call| want_kind.map_or(true, |k| call_kind_matches(call.kind, k)))
        .filter(|call| want_reentrant.map_or(true, |want| index.is_reentrant(call.id) == want))
        .filter(|call| {
            want_value_flow.map_or(true, |dir| match dir {
                "In" => call.to == Some(anchor_addr),
                "Out" => call.from == anchor_addr,
                _ => false,
            })
        })
        .filter(|call| {
            want_min_value.map_or(true, |min| {
                min <= 0 || i128::try_from(call.value.0).unwrap_or(i128::MAX) >= i128::from(min)
            })
        })
        .filter(|call| {
            // Same idiom as `want_slot` in `evaluate_storage`: `Some(None)`
            // means the attribute was present but failed to parse as an
            // 8-hex-digit selector, which can only happen for a
            // `CompiledPattern` `dsl::validate` didn't produce (it checks
            // only that the attribute is *a string*). Treat that the same
            // as "never matches."
            match want_selector {
                None => true,
                Some(None) => false,
                Some(Some(sel)) => call.selector == Some(sel),
            }
        })
        .filter(|call| want_succeeded.map_or(true, |want| call.succeeded == want))
        .filter(|call| {
            // Dereference `Call::parent` back to the exact call that
            // directly invoked this one, and check *that* call's own
            // kind — the same "producing fact, not mere adjacency"
            // idiom `evaluate_storage`'s `call_kind` attribute uses for
            // `StorageChange::call_id`, applied to the one-hop parent
            // edge every `Call` already carries. A root call (no
            // parent) can never satisfy a `parent_kind` clause, which
            // is the conservative, correct answer — there is no parent
            // call to have been "reached via."
            want_parent_kind.map_or(true, |ident| {
                call.parent.is_some_and(|parent_id| {
                    trace
                        .arena
                        .call(parent_id)
                        .is_some_and(|parent| call_kind_matches(parent.kind, ident))
                })
            })
        })
        .filter(|call| {
            // Generalizes the `parent_kind` idiom above from a single
            // hop to the full ancestor chain: walk `Call::parent`
            // repeatedly back to the root, checking whether *any*
            // ancestor along the way (not only the direct parent) has
            // the requested kind. A root call (no parent) has an empty
            // ancestor chain and can never satisfy this, same
            // conservative posture as `parent_kind`. Bounded by
            // `has_ancestor_of_kind`, which itself terminates in at
            // most the trace's own call-tree depth (finite, and a
            // proper decreasing walk toward the root — see that
            // function's own docs for why it cannot loop).
            want_ancestor_kind.map_or(true, |ident| has_ancestor_of_kind(trace, call.id, ident))
        })
        .map(|call| Binding {
            fact: FactRef::Call(call.id),
            order: Some(call.id.index()),
            unresolved_attrs: Vec::new(),
        })
        .collect()
}

fn evaluate_storage(trace: &Trace, predicate: &CompiledPredicate) -> Vec<Binding> {
    let want_changed = bool_attr(predicate, "changed");
    let want_slot = str_attr(predicate, "slot").map(parse_hex_word);
    let want_call_kind = ident(predicate, "call_kind");
    let has_role = predicate.attr("role").is_some();

    trace
        .arena
        .storage_changes_with_ids()
        .filter(|(_, change)| want_changed.map_or(true, |want| change.is_net_change() == want))
        .filter(|(_, change)| {
            // `want_slot` is `Some(None)` when the attribute was present
            // but failed to parse as valid hex — that can only happen
            // for a `CompiledPattern` `dsl::validate` didn't produce
            // (it only checks the attribute is *a string*, not that the
            // string is valid hex, since that would require `dsl` to
            // know `fact-model`'s `Word` encoding — a layering `dsl`
            // deliberately avoids, see its own crate docs). Treat an
            // unparseable slot the same as "never matches," the
            // conservative answer.
            match want_slot {
                None => true,
                Some(None) => false,
                Some(Some(word)) => change.slot.key == word,
            }
        })
        .filter(|(_, change)| {
            // Dereference `StorageChange::call_id` back to the exact
            // `Call` that produced this write, and check *that* call's
            // own kind — not merely "some call of this kind exists
            // somewhere in the trace." A `call_id` that fails to
            // resolve can't happen for a `Trace` built via
            // `FactArenaBuilder` (it enforces referential validity at
            // construction), but this stays conservative — "no match" —
            // rather than panicking, the same posture `want_slot`'s
            // unparseable case takes above.
            want_call_kind.map_or(true, |ident| {
                trace
                    .arena
                    .call(change.call_id)
                    .is_some_and(|call| call_kind_matches(call.kind, ident))
            })
        })
        .map(|(id, change)| Binding {
            fact: FactRef::StorageChange(id),
            order: trace.arena.call(change.call_id).map(|c| c.id.index()),
            unresolved_attrs: if has_role { vec!["role"] } else { Vec::new() },
        })
        .collect()
}

fn evaluate_value_flow(trace: &Trace, predicate: &CompiledPredicate) -> Vec<Binding> {
    let want_direction = ident(predicate, "direction");
    let want_min = int_attr(predicate, "min");
    let want_max = int_attr(predicate, "max");
    let anchor_addr = anchor(trace);

    trace
        .arena
        .value_flows()
        .filter(|flow| {
            want_direction.map_or(true, |dir| match dir {
                "In" => flow.to == Some(anchor_addr),
                "Out" => flow.from == anchor_addr,
                _ => false,
            })
        })
        .filter(|flow| {
            want_min.map_or(true, |min| {
                min <= 0 || i128::try_from(flow.amount.0).unwrap_or(i128::MAX) >= i128::from(min)
            })
        })
        .filter(|flow| {
            want_max.map_or(true, |max| {
                max >= 0 && i128::try_from(flow.amount.0).unwrap_or(i128::MAX) <= i128::from(max)
            })
        })
        .map(|flow| Binding {
            fact: FactRef::Call(flow.call_id),
            order: Some(flow.call_id.index()),
            unresolved_attrs: Vec::new(),
        })
        .collect()
}

fn evaluate_token_transfer(trace: &Trace, predicate: &CompiledPredicate) -> Vec<Binding> {
    let want_direction = ident(predicate, "direction");
    let want_token = str_attr(predicate, "token").map(parse_address);
    let has_unexpected = predicate.attr("unexpected").is_some();
    let anchor_addr = anchor(trace);

    trace
        .arena
        .token_transfers_with_ids()
        .filter(|(_, transfer)| {
            want_direction.map_or(true, |dir| match dir {
                "In" => transfer.to == anchor_addr,
                "Out" => transfer.from == anchor_addr,
                _ => false,
            })
        })
        .filter(|(_, transfer)| match want_token {
            None => true,
            Some(None) => false,
            Some(Some(addr)) => transfer.token == addr,
        })
        .map(|(id, transfer)| Binding {
            fact: FactRef::TokenTransfer(id),
            order: trace
                .arena
                .log(transfer.log_id)
                .and_then(|log| trace.arena.call(log.call_id))
                .map(|c| c.id.index()),
            unresolved_attrs: if has_unexpected {
                vec!["unexpected"]
            } else {
                Vec::new()
            },
        })
        .collect()
}

/// Parse a `0x`-prefixed, 40-hex-digit string into an [`Address`], for
/// the `token_transfer(token: "0x...")` attribute. See
/// [`parse_hex_word`] for why this is hand-rolled rather than reusing
/// `fact-model`'s `serde` impl.
fn parse_address(s: &str) -> Option<Address> {
    let digits = s.strip_prefix("0x").unwrap_or(s);
    if digits.len() != 40 {
        return None;
    }
    let mut bytes = [0u8; 20];
    for (i, byte) in bytes.iter_mut().enumerate() {
        let hi = (digits.as_bytes()[i * 2] as char).to_digit(16)?;
        let lo = (digits.as_bytes()[i * 2 + 1] as char).to_digit(16)?;
        *byte = u8::try_from(hi * 16 + lo).ok()?;
    }
    Some(Address::new(bytes))
}

fn evaluate_transaction(trace: &Trace, predicate: &CompiledPredicate) -> Vec<Binding> {
    let want_status = ident(predicate, "status");
    let want_gas_min = int_attr(predicate, "gas_used_min");

    let status_ok = want_status.map_or(true, |s| {
        matches!(
            (trace.transaction.status, s),
            (TxStatus::Success, "Success") | (TxStatus::Reverted, "Reverted")
        )
    });
    let gas_ok = want_gas_min.map_or(true, |min| {
        min <= 0 || i128::from(trace.transaction.gas_used.0) >= i128::from(min)
    });

    if status_ok && gas_ok {
        // A `transaction` predicate describes the whole trace, not one
        // point-in-time fact, so it has no natural `FactRef` of its own
        // in `fact-model`'s vocabulary (there is no `FactRef::
        // Transaction` variant — `fact-model` never needed one, since
        // nothing else cites "the transaction itself" as evidence
        // before this crate). The transaction's root call is the
        // closest real fact standing in for "the transaction as a
        // whole executed," and doubles as a reasonable citation for
        // grounding to inspect. `order: None` marks it as compatible
        // with any position in a `sequence:` constraint (see
        // `crate::ordering`), since "the whole transaction" has no
        // single point in execution order.
        vec![Binding {
            fact: FactRef::Call(trace.arena.root_call().id),
            order: None,
            unresolved_attrs: Vec::new(),
        }]
    } else {
        Vec::new()
    }
}

/// Evaluate a `log(topic0: ..., decoded: ...)` predicate.
///
/// Unlike `storage(role: ...)` and `token_transfer(unexpected: ...)`,
/// every attribute this predicate accepts is fully, structurally
/// checkable from `fact-model` data alone (see `dsl::schema::LOG`'s own
/// docs) — this function never records an `unresolved_attrs` entry.
fn evaluate_log(trace: &Trace, predicate: &CompiledPredicate) -> Vec<Binding> {
    let want_topic0 = str_attr(predicate, "topic0").map(parse_hex_word);
    let want_decoded = bool_attr(predicate, "decoded");

    trace
        .arena
        .logs()
        .filter(|log| {
            // `Some(None)` means `topic0` was present but failed to
            // parse as a 64-hex-digit word — the same "unparseable
            // means never matches" posture `evaluate_storage`'s
            // `want_slot` takes.
            match want_topic0 {
                None => true,
                Some(None) => false,
                Some(Some(word)) => log.topics.first() == Some(&word),
            }
        })
        .filter(|log| {
            want_decoded.map_or(true, |want| {
                let is_decoded = trace
                    .arena
                    .token_transfers()
                    .any(|transfer| transfer.log_id == log.id);
                is_decoded == want
            })
        })
        .map(|log| Binding {
            fact: FactRef::Log(log.id),
            order: trace.arena.call(log.call_id).map(|c| c.id.index()),
            unresolved_attrs: Vec::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fact_model::{
        BlockContext, BlockNumber, CallDepth, CallId, ChainId, FactArenaBuilder, Gas, LogEvent,
        LogIndex, Nonce, StorageChange, StorageSlot, Timestamp, TokenTransfer, TraceMetadata,
        TraceSource, Transaction, Wei,
    };

    fn addr(byte: u8) -> Address {
        Address::new([byte; 20])
    }

    fn hash(byte: u8) -> fact_model::TxHash {
        fact_model::TxHash(Word::new([byte; 32]))
    }

    fn tx_with_from(from: Address, status: TxStatus) -> Transaction {
        Transaction::new(
            hash(1),
            from,
            Some(addr(2)),
            Wei::ZERO,
            Nonce(0),
            Gas(21_000),
            status,
        )
    }

    fn build_trace(arena: fact_model::FactArena, tx: Transaction) -> Trace {
        let block = BlockContext::new(BlockNumber(1), Timestamp(1), ChainId(1), None);
        let metadata = TraceMetadata::new(
            tx.hash,
            ChainId(1),
            BlockNumber(1),
            TraceSource::ArchiveNodeRpc {
                endpoint_label: "test".to_string(),
            },
        );
        Trace::new(metadata, block, tx, arena).unwrap()
    }

    fn make_call(
        builder: &FactArenaBuilder,
        parent: Option<CallId>,
        depth: u16,
        kind: CallKind,
        from: Address,
        to: Address,
        value: u128,
    ) -> fact_model::Call {
        fact_model::Call::new(
            builder.next_call_id(),
            parent,
            kind,
            CallDepth(depth),
            from,
            Some(to),
            Wei(value),
            Gas(100_000),
            Gas(50_000),
            true,
        )
        .unwrap()
    }

    fn predicate(kind: &str, attrs: &[(&str, AttrValue)]) -> CompiledPredicate {
        CompiledPredicate {
            kind: kind.to_string(),
            attributes: attrs
                .iter()
                .map(|(name, value)| dsl::ir::CompiledAttr {
                    name: (*name).to_string(),
                    value: value.clone(),
                })
                .collect(),
        }
    }

    #[test]
    fn call_predicate_filters_by_kind() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        let root_id = builder.add_call(root);
        let static_call = make_call(
            &builder,
            Some(root_id),
            1,
            CallKind::StaticCall,
            addr(2),
            addr(3),
            0,
        );
        builder.add_call(static_call);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate(
            "call",
            &[("kind", AttrValue::Ident("StaticCall".to_string()))],
        );
        let bindings = evaluate(&trace, &index, &pred);
        assert_eq!(bindings.len(), 1);
    }

    #[test]
    fn call_predicate_filters_by_selfdestruct_kind() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        let root_id = builder.add_call(root);
        let destruct_call = make_call(
            &builder,
            Some(root_id),
            1,
            CallKind::Selfdestruct,
            addr(2),
            addr(3),
            0,
        );
        builder.add_call(destruct_call);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate(
            "call",
            &[("kind", AttrValue::Ident("SelfDestruct".to_string()))],
        );
        let bindings = evaluate(&trace, &index, &pred);
        assert_eq!(bindings.len(), 1);
    }

    #[test]
    fn call_predicate_reentrant_filter_uses_index() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(9), 0);
        let root_id = builder.add_call(root);
        let child = make_call(
            &builder,
            Some(root_id),
            1,
            CallKind::Call,
            addr(9),
            addr(5),
            0,
        );
        let child_id = builder.add_call(child);
        let grandchild = make_call(
            &builder,
            Some(child_id),
            2,
            CallKind::Call,
            addr(5),
            addr(9),
            0,
        );
        let grandchild_id = builder.add_call(grandchild);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate("call", &[("reentrant", AttrValue::Bool(true))]);
        let bindings = evaluate(&trace, &index, &pred);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].fact, FactRef::Call(grandchild_id));
    }

    #[test]
    fn call_predicate_value_flow_direction_relative_to_sender() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 100);
        let root_id = builder.add_call(root);
        let refund = make_call(
            &builder,
            Some(root_id),
            1,
            CallKind::Call,
            addr(2),
            attacker,
            50,
        );
        builder.add_call(refund);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let out_pred = predicate(
            "call",
            &[("value_flow", AttrValue::Ident("Out".to_string()))],
        );
        let in_pred = predicate(
            "call",
            &[("value_flow", AttrValue::Ident("In".to_string()))],
        );
        assert_eq!(evaluate(&trace, &index, &out_pred).len(), 1);
        assert_eq!(evaluate(&trace, &index, &in_pred).len(), 1);
    }

    #[test]
    fn call_predicate_selector_filters() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let transfer_call = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0)
            .with_selector([0xa9, 0x05, 0x9c, 0xbb]); // erc20 transfer
        let root_id = builder.add_call(transfer_call);
        let approve_call = make_call(
            &builder,
            Some(root_id),
            1,
            CallKind::Call,
            attacker,
            addr(2),
            0,
        )
        .with_selector([0x09, 0x5e, 0xa7, 0xb3]); // erc20 approve
        builder.add_call(approve_call);
        let no_selector_call = make_call(
            &builder,
            Some(root_id),
            1,
            CallKind::Call,
            attacker,
            addr(3),
            0,
        );
        builder.add_call(no_selector_call);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate(
            "call",
            &[("selector", AttrValue::Str("0xa9059cbb".to_string()))],
        );
        let bindings = evaluate(&trace, &index, &pred);
        assert_eq!(bindings.len(), 1);

        let pred_no_match = predicate(
            "call",
            &[("selector", AttrValue::Str("0xdeadbeef".to_string()))],
        );
        assert!(evaluate(&trace, &index, &pred_no_match).is_empty());
    }

    #[test]
    fn call_predicate_malformed_selector_matches_nothing() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0)
            .with_selector([0xa9, 0x05, 0x9c, 0xbb]);
        builder.add_call(root);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        // Not 8 hex digits — `dsl::validate` would never produce this,
        // but this crate still treats it as "never matches" rather than
        // panicking, per the same idiom `evaluate_storage` uses for a
        // malformed `slot`.
        let pred = predicate("call", &[("selector", AttrValue::Str("0xzz".to_string()))]);
        assert!(evaluate(&trace, &index, &pred).is_empty());
    }

    #[test]
    fn call_predicate_omitted_selector_does_not_filter() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0)
            .with_selector([0xa9, 0x05, 0x9c, 0xbb]);
        builder.add_call(root);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate("call", &[]);
        assert_eq!(evaluate(&trace, &index, &pred).len(), 1);
    }

    #[test]
    fn call_predicate_succeeded_filters() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let failed = fact_model::Call::new(
            builder.next_call_id(),
            None,
            CallKind::Call,
            CallDepth(0),
            attacker,
            Some(addr(2)),
            Wei(0),
            Gas(100_000),
            Gas(50_000),
            false,
        )
        .unwrap();
        builder.add_call(failed);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred_failed = predicate("call", &[("succeeded", AttrValue::Bool(false))]);
        assert_eq!(evaluate(&trace, &index, &pred_failed).len(), 1);

        let pred_succeeded = predicate("call", &[("succeeded", AttrValue::Bool(true))]);
        assert!(evaluate(&trace, &index, &pred_succeeded).is_empty());
    }

    #[test]
    fn call_predicate_omitted_succeeded_does_not_filter() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        builder.add_call(root);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate("call", &[]);
        assert_eq!(evaluate(&trace, &index, &pred).len(), 1);
    }

    #[test]
    fn call_predicate_min_value_filters() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 5);
        builder.add_call(root);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate("call", &[("min_value", AttrValue::Int(10))]);
        assert!(evaluate(&trace, &index, &pred).is_empty());
        let pred_ok = predicate("call", &[("min_value", AttrValue::Int(5))]);
        assert_eq!(evaluate(&trace, &index, &pred_ok).len(), 1);
    }

    #[test]
    fn call_predicate_parent_kind_filters_to_the_direct_parent() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        let root_id = builder.add_call(root);
        let delegate = make_call(
            &builder,
            Some(root_id),
            1,
            CallKind::DelegateCall,
            addr(2),
            addr(3),
            0,
        );
        let delegate_id = builder.add_call(delegate);
        // The SELFDESTRUCT executes as a direct child of the
        // DELEGATECALL frame — the Parity-multisig-library shape.
        let destruct = make_call(
            &builder,
            Some(delegate_id),
            2,
            CallKind::Selfdestruct,
            addr(3),
            addr(9),
            0,
        );
        let destruct_id = builder.add_call(destruct);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate(
            "call",
            &[
                ("kind", AttrValue::Ident("SelfDestruct".to_string())),
                ("parent_kind", AttrValue::Ident("Delegate".to_string())),
            ],
        );
        let bindings = evaluate(&trace, &index, &pred);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].fact, FactRef::Call(destruct_id));
    }

    #[test]
    fn call_predicate_parent_kind_rejects_a_selfdestruct_not_reached_via_delegatecall() {
        // Same successful SELFDESTRUCT shape as the passing test above,
        // but its direct parent is a plain `CALL`, not a
        // `DELEGATECALL` — `parent_kind: Delegate` must reject it, the
        // same way `storage(call_kind: ...)` rejects a merely adjacent
        // (not producing) call.
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        let root_id = builder.add_call(root);
        let destruct = make_call(
            &builder,
            Some(root_id),
            1,
            CallKind::Selfdestruct,
            addr(2),
            addr(9),
            0,
        );
        builder.add_call(destruct);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate(
            "call",
            &[
                ("kind", AttrValue::Ident("SelfDestruct".to_string())),
                ("parent_kind", AttrValue::Ident("Delegate".to_string())),
            ],
        );
        assert!(evaluate(&trace, &index, &pred).is_empty());
    }

    #[test]
    fn call_predicate_parent_kind_rejects_a_root_call() {
        // A root call has no parent at all — `parent_kind` must reject
        // it rather than panic or treat "no parent" as a wildcard match.
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        builder.add_call(root);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate(
            "call",
            &[("parent_kind", AttrValue::Ident("Delegate".to_string()))],
        );
        assert!(evaluate(&trace, &index, &pred).is_empty());
    }

    #[test]
    fn call_predicate_omitted_parent_kind_does_not_filter() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        builder.add_call(root);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate("call", &[]);
        assert_eq!(evaluate(&trace, &index, &pred).len(), 1);
    }

    #[test]
    fn call_predicate_ancestor_kind_matches_the_direct_parent() {
        // ancestor_kind must subsume parent_kind's own one-hop case.
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        let root_id = builder.add_call(root);
        let delegate = make_call(
            &builder,
            Some(root_id),
            1,
            CallKind::DelegateCall,
            addr(2),
            addr(3),
            0,
        );
        let delegate_id = builder.add_call(delegate);
        let destruct = make_call(
            &builder,
            Some(delegate_id),
            2,
            CallKind::Selfdestruct,
            addr(3),
            addr(9),
            0,
        );
        let destruct_id = builder.add_call(destruct);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate(
            "call",
            &[
                ("kind", AttrValue::Ident("SelfDestruct".to_string())),
                ("ancestor_kind", AttrValue::Ident("Delegate".to_string())),
            ],
        );
        let bindings = evaluate(&trace, &index, &pred);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].fact, FactRef::Call(destruct_id));
    }

    #[test]
    fn call_predicate_ancestor_kind_matches_a_non_direct_ancestor() {
        // DELEGATECALL -> CALL -> SELFDESTRUCT: the delegatecall is the
        // *grandparent*, not the direct parent. `parent_kind` would
        // reject this (verified by the sibling test below); the whole
        // point of `ancestor_kind` is to still catch it.
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        let root_id = builder.add_call(root);
        let delegate = make_call(
            &builder,
            Some(root_id),
            1,
            CallKind::DelegateCall,
            addr(2),
            addr(3),
            0,
        );
        let delegate_id = builder.add_call(delegate);
        let inner_call = make_call(
            &builder,
            Some(delegate_id),
            2,
            CallKind::Call,
            addr(2),
            addr(4),
            0,
        );
        let inner_call_id = builder.add_call(inner_call);
        let destruct = make_call(
            &builder,
            Some(inner_call_id),
            3,
            CallKind::Selfdestruct,
            addr(4),
            addr(9),
            0,
        );
        let destruct_id = builder.add_call(destruct);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        // parent_kind must miss this (regression guard on the gap this
        // milestone's pattern exists to close).
        let parent_pred = predicate(
            "call",
            &[
                ("kind", AttrValue::Ident("SelfDestruct".to_string())),
                ("parent_kind", AttrValue::Ident("Delegate".to_string())),
            ],
        );
        assert!(evaluate(&trace, &index, &parent_pred).is_empty());

        // ancestor_kind must catch it.
        let ancestor_pred = predicate(
            "call",
            &[
                ("kind", AttrValue::Ident("SelfDestruct".to_string())),
                ("ancestor_kind", AttrValue::Ident("Delegate".to_string())),
            ],
        );
        let bindings = evaluate(&trace, &index, &ancestor_pred);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].fact, FactRef::Call(destruct_id));
    }

    #[test]
    fn call_predicate_ancestor_kind_rejects_a_sibling_delegatecall() {
        // A DELEGATECALL exists elsewhere in the trace, but it is a
        // *sibling*, not an ancestor, of the SELFDESTRUCT. ancestor_kind
        // must reject this — presence anywhere in the trace is not
        // enough, it must be on this specific call's own ancestor path.
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        let root_id = builder.add_call(root);
        let delegate = make_call(
            &builder,
            Some(root_id),
            1,
            CallKind::DelegateCall,
            addr(2),
            addr(3),
            0,
        );
        builder.add_call(delegate);
        let other_branch = make_call(
            &builder,
            Some(root_id),
            1,
            CallKind::Call,
            addr(2),
            addr(4),
            0,
        );
        let other_branch_id = builder.add_call(other_branch);
        let destruct = make_call(
            &builder,
            Some(other_branch_id),
            2,
            CallKind::Selfdestruct,
            addr(4),
            addr(9),
            0,
        );
        builder.add_call(destruct);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate(
            "call",
            &[
                ("kind", AttrValue::Ident("SelfDestruct".to_string())),
                ("ancestor_kind", AttrValue::Ident("Delegate".to_string())),
            ],
        );
        assert!(evaluate(&trace, &index, &pred).is_empty());
    }

    #[test]
    fn call_predicate_ancestor_kind_rejects_a_root_call() {
        // A root call has no ancestors at all.
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        builder.add_call(root);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate(
            "call",
            &[("ancestor_kind", AttrValue::Ident("Delegate".to_string()))],
        );
        assert!(evaluate(&trace, &index, &pred).is_empty());
    }

    #[test]
    fn call_predicate_omitted_ancestor_kind_does_not_filter() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        builder.add_call(root);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate("call", &[]);
        assert_eq!(evaluate(&trace, &index, &pred).len(), 1);
    }

    #[test]
    fn storage_predicate_changed_and_slot() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        let root_id = builder.add_call(root);
        let slot_key = Word::new([7; 32]);
        builder.add_storage_change(StorageChange::new(
            root_id,
            StorageSlot::new(addr(2), slot_key),
            Word::ZERO,
            Word::new([1; 32]),
        ));
        builder.add_storage_change(StorageChange::new(
            root_id,
            StorageSlot::new(addr(2), Word::new([9; 32])),
            Word::new([1; 32]),
            Word::new([1; 32]), // unchanged
        ));
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate("storage", &[("changed", AttrValue::Bool(true))]);
        assert_eq!(evaluate(&trace, &index, &pred).len(), 1);

        let slot_hex = format!("0x{}", "07".repeat(32));
        let pred_slot = predicate(
            "storage",
            &[
                ("changed", AttrValue::Bool(true)),
                ("slot", AttrValue::Str(slot_hex)),
            ],
        );
        let bindings = evaluate(&trace, &index, &pred_slot);
        assert_eq!(bindings.len(), 1);
    }

    #[test]
    fn storage_predicate_role_is_unresolved_not_filtering() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        let root_id = builder.add_call(root);
        builder.add_storage_change(StorageChange::new(
            root_id,
            StorageSlot::new(addr(2), Word::ZERO),
            Word::ZERO,
            Word::new([1; 32]),
        ));
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate(
            "storage",
            &[
                ("changed", AttrValue::Bool(true)),
                ("role", AttrValue::Ident("PriceOracle".to_string())),
            ],
        );
        let bindings = evaluate(&trace, &index, &pred);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].unresolved_attrs, vec!["role"]);
    }

    #[test]
    fn storage_predicate_call_kind_filters_to_the_producing_call() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        let root_id = builder.add_call(root);
        let delegate = make_call(
            &builder,
            Some(root_id),
            1,
            CallKind::DelegateCall,
            addr(2),
            addr(3),
            0,
        );
        let delegate_id = builder.add_call(delegate);
        // The write is attributed to the DELEGATECALL frame itself.
        builder.add_storage_change(StorageChange::new(
            delegate_id,
            StorageSlot::new(addr(2), Word::ZERO),
            Word::ZERO,
            Word::new([1; 32]),
        ));
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred_delegate = predicate(
            "storage",
            &[
                ("changed", AttrValue::Bool(true)),
                ("call_kind", AttrValue::Ident("Delegate".to_string())),
            ],
        );
        assert_eq!(evaluate(&trace, &index, &pred_delegate).len(), 1);

        let pred_external = predicate(
            "storage",
            &[
                ("changed", AttrValue::Bool(true)),
                ("call_kind", AttrValue::Ident("External".to_string())),
            ],
        );
        assert!(evaluate(&trace, &index, &pred_external).is_empty());
    }

    #[test]
    fn storage_predicate_call_kind_rejects_a_merely_adjacent_call() {
        // Same shape a loose `AND(call(kind: Delegate), storage(...))` +
        // `sequence:` pattern would accept (a Delegate call followed, in
        // trace order, by *some* storage write) — but here the write is
        // attributed to a *sibling plain call*, not the delegatecall
        // itself. `call_kind` must reject this, since it grounds
        // against the write's own producing call, not mere adjacency.
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        let root_id = builder.add_call(root);
        let delegate = make_call(
            &builder,
            Some(root_id),
            1,
            CallKind::DelegateCall,
            addr(2),
            addr(3),
            0,
        );
        builder.add_call(delegate);
        let plain = make_call(
            &builder,
            Some(root_id),
            1,
            CallKind::Call,
            addr(2),
            addr(4),
            0,
        );
        let plain_id = builder.add_call(plain);
        // The write is attributed to the plain CALL, not the delegatecall.
        builder.add_storage_change(StorageChange::new(
            plain_id,
            StorageSlot::new(addr(2), Word::ZERO),
            Word::ZERO,
            Word::new([1; 32]),
        ));
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate(
            "storage",
            &[
                ("changed", AttrValue::Bool(true)),
                ("call_kind", AttrValue::Ident("Delegate".to_string())),
            ],
        );
        assert!(evaluate(&trace, &index, &pred).is_empty());
    }

    #[test]
    fn storage_predicate_omitted_call_kind_does_not_filter() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        let root_id = builder.add_call(root);
        builder.add_storage_change(StorageChange::new(
            root_id,
            StorageSlot::new(addr(2), Word::ZERO),
            Word::ZERO,
            Word::new([1; 32]),
        ));
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate("storage", &[("changed", AttrValue::Bool(true))]);
        assert_eq!(evaluate(&trace, &index, &pred).len(), 1);
    }

    #[test]
    fn value_flow_predicate_min_max_bounds() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(9), 500);
        builder.add_call(root);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate(
            "value_flow",
            &[
                ("direction", AttrValue::Ident("Out".to_string())),
                ("min", AttrValue::Int(100)),
                ("max", AttrValue::Int(1000)),
            ],
        );
        assert_eq!(evaluate(&trace, &index, &pred).len(), 1);

        let pred_too_high = predicate(
            "value_flow",
            &[
                ("direction", AttrValue::Ident("Out".to_string())),
                ("max", AttrValue::Int(100)),
            ],
        );
        assert!(evaluate(&trace, &index, &pred_too_high).is_empty());
    }

    #[test]
    fn token_transfer_predicate_direction_and_token() {
        let attacker = addr(1);
        let token = addr(0xaa);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        let root_id = builder.add_call(root);
        let log_id = builder.next_log_id();
        builder
            .add_log(LogEvent::new(log_id, root_id, token, vec![], vec![], LogIndex(0)).unwrap());
        builder.add_token_transfer(TokenTransfer::new(
            log_id,
            token,
            addr(2),
            attacker,
            Wei(10),
        ));
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let token_hex = format!("0x{}", "aa".repeat(20));
        let pred = predicate(
            "token_transfer",
            &[
                ("direction", AttrValue::Ident("In".to_string())),
                ("token", AttrValue::Str(token_hex)),
            ],
        );
        assert_eq!(evaluate(&trace, &index, &pred).len(), 1);

        let pred_wrong_dir = predicate(
            "token_transfer",
            &[("direction", AttrValue::Ident("Out".to_string()))],
        );
        assert!(evaluate(&trace, &index, &pred_wrong_dir).is_empty());
    }

    #[test]
    fn transaction_predicate_status_and_gas() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        builder.add_call(root);
        let arena = builder.build().unwrap();
        let mut tx = tx_with_from(attacker, TxStatus::Reverted);
        tx.gas_used = Gas(500_000);
        let trace = build_trace(arena, tx);
        let index = TraceIndex::build(&trace);

        let pred = predicate(
            "transaction",
            &[
                ("status", AttrValue::Ident("Reverted".to_string())),
                ("gas_used_min", AttrValue::Int(100_000)),
            ],
        );
        let bindings = evaluate(&trace, &index, &pred);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].order, None);

        let pred_wrong_status = predicate(
            "transaction",
            &[("status", AttrValue::Ident("Success".to_string()))],
        );
        assert!(evaluate(&trace, &index, &pred_wrong_status).is_empty());
    }

    const TRANSFER_TOPIC0_HEX: &str =
        "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

    fn transfer_topic0() -> Word {
        parse_hex_word(TRANSFER_TOPIC0_HEX).unwrap()
    }

    #[test]
    fn log_predicate_topic0_matches_undecoded_log() {
        let attacker = addr(1);
        let spoofer = addr(0xee);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, spoofer, 0);
        let root_id = builder.add_call(root);
        let log_id = builder.next_log_id();
        // A log carrying the standard `Transfer` topic0 but with only
        // one topic (no indexed `from`/`to`) — exactly the shape
        // `ingestion::normalize::try_decode_token_transfer` refuses to
        // decode (it requires exactly 3 topics), so no `TokenTransfer`
        // fact is ever produced for it.
        builder.add_log(
            LogEvent::new(
                log_id,
                root_id,
                spoofer,
                vec![transfer_topic0()],
                vec![],
                LogIndex(0),
            )
            .unwrap(),
        );
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate(
            "log",
            &[
                ("topic0", AttrValue::Str(TRANSFER_TOPIC0_HEX.to_string())),
                ("decoded", AttrValue::Bool(false)),
            ],
        );
        let bindings = evaluate(&trace, &index, &pred);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].fact, FactRef::Log(log_id));
        assert!(bindings[0].unresolved_attrs.is_empty());
    }

    #[test]
    fn log_predicate_decoded_true_excludes_a_properly_shaped_transfer() {
        let attacker = addr(1);
        let token = addr(0xaa);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, token, 0);
        let root_id = builder.add_call(root);
        let log_id = builder.next_log_id();
        builder.add_log(
            LogEvent::new(
                log_id,
                root_id,
                token,
                vec![transfer_topic0(), Word::ZERO, Word::ZERO],
                vec![0; 32],
                LogIndex(0),
            )
            .unwrap(),
        );
        builder.add_token_transfer(TokenTransfer::new(log_id, token, addr(2), attacker, Wei(1)));
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred_undecoded = predicate(
            "log",
            &[
                ("topic0", AttrValue::Str(TRANSFER_TOPIC0_HEX.to_string())),
                ("decoded", AttrValue::Bool(false)),
            ],
        );
        assert!(evaluate(&trace, &index, &pred_undecoded).is_empty());

        let pred_decoded = predicate(
            "log",
            &[
                ("topic0", AttrValue::Str(TRANSFER_TOPIC0_HEX.to_string())),
                ("decoded", AttrValue::Bool(true)),
            ],
        );
        assert_eq!(evaluate(&trace, &index, &pred_decoded).len(), 1);
    }

    #[test]
    fn log_predicate_topic0_mismatch_matches_nothing() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        let root_id = builder.add_call(root);
        let log_id = builder.next_log_id();
        builder.add_log(
            LogEvent::new(
                log_id,
                root_id,
                addr(2),
                vec![Word::ZERO],
                vec![],
                LogIndex(0),
            )
            .unwrap(),
        );
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate(
            "log",
            &[("topic0", AttrValue::Str(TRANSFER_TOPIC0_HEX.to_string()))],
        );
        assert!(evaluate(&trace, &index, &pred).is_empty());
    }

    #[test]
    fn unrecognized_predicate_kind_matches_nothing() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = make_call(&builder, None, 0, CallKind::Call, attacker, addr(2), 0);
        builder.add_call(root);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, tx_with_from(attacker, TxStatus::Success));
        let index = TraceIndex::build(&trace);

        let pred = predicate("frobnicate", &[]);
        assert!(evaluate(&trace, &index, &pred).is_empty());
    }
}

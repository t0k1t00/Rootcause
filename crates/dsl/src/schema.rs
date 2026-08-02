//! The closed schema of predicate kinds and their attributes.
//!
//! This module is the single source of truth [`crate::validate`] checks
//! predicates against. It intentionally does **not** interpret
//! attributes against the `fact-model` crate's actual `Call` /
//! `StorageChange` / `ValueFlow` / `TokenTransfer` / `Transaction` types
//! — this crate parses, validates, and compiles pattern *definitions*
//! only; grounding a predicate against real facts is the (not yet
//! implemented) `matcher`/`grounding` crates' job. What lives here is
//! just enough structure — which attribute names are legal for which
//! predicate kind, and what shape of value each expects — to reject
//! obviously malformed or incompatible pattern authoring before it ever
//! reaches those later stages.

/// The shape of value a predicate attribute expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrShape {
    /// A bare identifier drawn from a fixed set of allowed values.
    Enum(&'static [&'static str]),
    /// A string literal, unconstrained.
    Str,
    /// An integer literal, unconstrained beyond fitting `i64`.
    Int,
    /// A boolean literal.
    Bool,
}

/// One attribute a predicate kind accepts.
#[derive(Debug, Clone, Copy)]
pub struct AttrSchema {
    /// The attribute's name, e.g. `"direction"`.
    pub name: &'static str,
    /// The shape of value it expects.
    pub shape: AttrShape,
    /// Whether every use of this predicate kind must supply this
    /// attribute.
    pub required: bool,
}

/// The full schema for one predicate kind.
#[derive(Debug, Clone, Copy)]
pub struct PredicateSchema {
    /// The predicate kind's name, e.g. `"call"`.
    pub kind: &'static str,
    /// One line describing what real-world fact this predicate kind
    /// ultimately grounds against (for diagnostics and documentation
    /// only — this crate does not itself perform grounding).
    pub grounds_against: &'static str,
    /// The attributes this predicate kind accepts.
    pub attrs: &'static [AttrSchema],
}

const CALL_KIND_VALUES: &[&str] = &[
    "External",
    "Internal",
    "Delegate",
    "StaticCall",
    "Create",
    "SelfDestruct",
];
const DIRECTION_VALUES: &[&str] = &["In", "Out"];
const TX_STATUS_VALUES: &[&str] = &["Success", "Reverted"];

/// `call(kind: ..., reentrant: ..., value_flow: ..., min_value: ..., selector: ..., succeeded: ..., parent_kind: ..., ancestor_kind: ...)`
///
/// `parent_kind` grounds against the kind of the specific call that
/// *directly invoked* this call (via `fact-model`'s existing
/// `Call::parent` field) — mirroring `storage(call_kind: ...)`'s own
/// "dereference to the producing call, not mere trace-order adjacency"
/// idiom, but walking one edge of the call tree itself rather than a
/// `StorageChange::call_id` reference. This is what lets a single
/// `call(...)` clause assert "this call executed as a direct child of
/// *that kind* of call" precisely — e.g. "this SELFDESTRUCT executed
/// inside a DELEGATECALL frame" — instead of only being able to assert
/// each call's own kind in isolation.
///
/// `ancestor_kind` (Milestone 10) generalizes `parent_kind` from a
/// one-hop edge to the full ancestor chain: it walks `Call::parent`
/// repeatedly back to the root and asks whether *any* ancestor — not
/// only the immediate parent — has the given kind. This closes a real
/// gap `parent_kind` leaves open: a `DELEGATECALL` that is reachable
/// but not the *direct* parent (e.g. `DELEGATECALL → CALL →
/// SELFDESTRUCT`) still executes the `SELFDESTRUCT` against the
/// delegatecall's borrowed storage/library context, and is still the
/// same dangerous shape `delegatecall_reachable_selfdestruct` exists to
/// flag — `parent_kind` alone misses it because the intervening plain
/// `CALL` sits directly between them.
pub const CALL: PredicateSchema = PredicateSchema {
    kind: "call",
    grounds_against: "fact_model::Call",
    attrs: &[
        AttrSchema {
            name: "kind",
            shape: AttrShape::Enum(CALL_KIND_VALUES),
            required: true,
        },
        AttrSchema {
            name: "reentrant",
            shape: AttrShape::Bool,
            required: false,
        },
        AttrSchema {
            name: "value_flow",
            shape: AttrShape::Enum(DIRECTION_VALUES),
            required: false,
        },
        AttrSchema {
            name: "min_value",
            shape: AttrShape::Int,
            required: false,
        },
        AttrSchema {
            name: "selector",
            shape: AttrShape::Str,
            required: false,
        },
        AttrSchema {
            name: "succeeded",
            shape: AttrShape::Bool,
            required: false,
        },
        AttrSchema {
            name: "parent_kind",
            shape: AttrShape::Enum(CALL_KIND_VALUES),
            required: false,
        },
        AttrSchema {
            name: "ancestor_kind",
            shape: AttrShape::Enum(CALL_KIND_VALUES),
            required: false,
        },
    ],
};

/// `storage(changed: ..., role: ..., slot: ..., call_kind: ...)`
///
/// `call_kind` grounds against the kind of the specific call that
/// produced this storage change (via `fact-model`'s existing
/// `StorageChange::call_id` field) — not a separate fact, and not
/// merely an ordering hint the way `sequence:` is. This is what lets a
/// single `storage(...)` clause assert "this write happened as part of
/// *that kind* of call's own frame" precisely, instead of "some call
/// of that kind happened somewhere nearby in trace order."
pub const STORAGE: PredicateSchema = PredicateSchema {
    kind: "storage",
    grounds_against: "fact_model::StorageChange",
    attrs: &[
        AttrSchema {
            name: "changed",
            shape: AttrShape::Bool,
            required: true,
        },
        AttrSchema {
            name: "role",
            shape: AttrShape::Enum(&["PriceOracle", "Balance", "Accounting", "AccessControl"]),
            required: false,
        },
        AttrSchema {
            name: "slot",
            shape: AttrShape::Str,
            required: false,
        },
        AttrSchema {
            name: "call_kind",
            shape: AttrShape::Enum(CALL_KIND_VALUES),
            required: false,
        },
    ],
};

/// `value_flow(direction: ..., min: ..., max: ...)`
pub const VALUE_FLOW: PredicateSchema = PredicateSchema {
    kind: "value_flow",
    grounds_against: "fact_model::ValueFlow",
    attrs: &[
        AttrSchema {
            name: "direction",
            shape: AttrShape::Enum(DIRECTION_VALUES),
            required: true,
        },
        AttrSchema {
            name: "min",
            shape: AttrShape::Int,
            required: false,
        },
        AttrSchema {
            name: "max",
            shape: AttrShape::Int,
            required: false,
        },
    ],
};

/// `token_transfer(direction: ..., unexpected: ..., token: ...)`
pub const TOKEN_TRANSFER: PredicateSchema = PredicateSchema {
    kind: "token_transfer",
    grounds_against: "fact_model::TokenTransfer",
    attrs: &[
        AttrSchema {
            name: "direction",
            shape: AttrShape::Enum(DIRECTION_VALUES),
            required: true,
        },
        AttrSchema {
            name: "unexpected",
            shape: AttrShape::Bool,
            required: false,
        },
        AttrSchema {
            name: "token",
            shape: AttrShape::Str,
            required: false,
        },
    ],
};

/// `transaction(status: ..., gas_used_min: ...)`
pub const TRANSACTION: PredicateSchema = PredicateSchema {
    kind: "transaction",
    grounds_against: "fact_model::Transaction",
    attrs: &[
        AttrSchema {
            name: "status",
            shape: AttrShape::Enum(TX_STATUS_VALUES),
            required: true,
        },
        AttrSchema {
            name: "gas_used_min",
            shape: AttrShape::Int,
            required: false,
        },
    ],
};

/// `log(topic0: ..., decoded: ...)`
///
/// Both attributes ground against fields `fact-model` already records
/// on every ingested `LogEvent`/`TokenTransfer` pair — neither is an
/// invented or interpreted fact:
///
/// - `topic0` exact-matches a log's first topic (`LogEvent.topics[0]`),
///   the raw, publicly-computable `keccak256` event-signature hash the
///   EVM itself records. This mirrors `call(selector: ...)`'s existing
///   idiom exactly (Milestone 7): comparing raw bytes the trace already
///   carries, with no ABI parameter decoding and no signature-name
///   database involved.
/// - `decoded` grounds against whether some `fact_model::TokenTransfer`
///   in the same trace cites this log as its `log_id` — i.e. whether
///   `ingestion`'s own best-effort `Transfer(address,address,uint256)`
///   decoder (see `crates/ingestion/src/normalize.rs`) was able to
///   interpret this log's full shape (exactly 3 topics, 32-byte data).
///   This is a structural fact about the log's own shape, not a claim
///   about intent.
pub const LOG: PredicateSchema = PredicateSchema {
    kind: "log",
    grounds_against: "fact_model::LogEvent",
    attrs: &[
        AttrSchema {
            name: "topic0",
            shape: AttrShape::Str,
            required: true,
        },
        AttrSchema {
            name: "decoded",
            shape: AttrShape::Bool,
            required: false,
        },
    ],
};

/// Every known predicate kind's schema.
pub const ALL: &[PredicateSchema] = &[CALL, STORAGE, VALUE_FLOW, TOKEN_TRANSFER, TRANSACTION, LOG];

/// Look up a predicate kind's schema by name.
#[must_use]
pub fn lookup(kind: &str) -> Option<&'static PredicateSchema> {
    ALL.iter().find(|s| s.kind == kind)
}

/// The closed set of valid `severity:` metadata values.
pub const SEVERITY_VALUES: &[&str] = &["Low", "Medium", "High", "Critical"];

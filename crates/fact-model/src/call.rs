//! [`Call`]: one node in a transaction's call tree.

use serde::{Deserialize, Serialize};

use crate::ids::CallId;
use crate::primitives::{Address, CallDepth, Gas, Wei};

/// The kind of EVM call a [`Call`] represents.
///
/// This distinguishes exactly the call kinds the Architecture document
/// requires be expressible (value flow, call relationships, and
/// `delegatecall`, Assumption A-4) plus the remaining kinds needed for a
/// call tree to be structurally complete: a trace containing an
/// undecoded `CALLCODE` or `CREATE2` would be silently wrong, not merely
/// incomplete, if this enum couldn't name it.
///
/// ## Why an exhaustive enum, not an open string
/// The EVM defines a fixed, closed set of call opcodes. An open
/// `String`-typed "kind" field would let a typo (`"delegatecal"`) pass
/// silently through the type system; this enum makes that a compile
/// error instead, per the project rule to prefer exhaustive enums and
/// compile-time guarantees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CallKind {
    /// A plain `CALL`: executes in the callee's context, callee's
    /// storage, can transfer value.
    Call,
    /// A `STATICCALL`: like `Call`, but the callee (and everything it
    /// calls) is prohibited from state-modifying operations.
    StaticCall,
    /// A `DELEGATECALL`: executes the callee's code in the *caller's*
    /// storage and context, preserving `msg.sender`/`msg.value` from the
    /// caller's own caller. Named explicitly in Assumption A-4 as a call
    /// relationship the fact model must be able to express.
    DelegateCall,
    /// A `CALLCODE`: the historical predecessor to `DelegateCall`,
    /// executing the callee's code in the caller's storage but *not*
    /// preserving the original `msg.sender`. Rare in modern contracts
    /// but still a distinct, real opcode a historical trace can contain.
    CallCode,
    /// A `CREATE`: deploys new contract code at an address derived from
    /// the sender and nonce.
    Create,
    /// A `CREATE2`: deploys new contract code at an address derived from
    /// a caller-supplied salt, making the deployment address
    /// predictable in advance — relevant to several exploit families
    /// that pre-compute a counterfactual contract address.
    Create2,
    /// A `SELFDESTRUCT`: irrevocably removes the executing contract's
    /// code and sends its entire remaining balance to `to` (the
    /// beneficiary address). Unlike every other variant, this is not a
    /// message call that transfers control — it terminates the calling
    /// frame's contract. Modeled here as a `Call` anyway (rather than a
    /// new fact type) because a `SELFDESTRUCT` frame already arrives
    /// from every trace source this project ingests (Geth's
    /// `callTracer`, and this crate's own raw schema) shaped exactly
    /// like every other call frame: `from`/`to`/`value`, nested at the
    /// point in the call tree it executed. Giving it its own `CallKind`
    /// variant reuses that existing shape instead of inventing a
    /// parallel fact type for one opcode.
    Selfdestruct,
}

impl CallKind {
    /// Whether this call kind can carry a nonzero `value` transfer.
    /// `StaticCall` cannot, by EVM rule (any attempt reverts the call);
    /// every other kind can.
    #[must_use]
    pub const fn can_carry_value(self) -> bool {
        !matches!(self, Self::StaticCall)
    }

    /// Whether this call kind executes in the *caller's* storage context
    /// rather than its own (i.e. is a delegation of code, not of state).
    #[must_use]
    pub const fn executes_in_caller_context(self) -> bool {
        matches!(self, Self::DelegateCall | Self::CallCode)
    }
}

/// One node in a transaction's call tree.
///
/// ## Invariants
/// A `Call` alone enforces only its local, single-node invariants (see
/// [`Call::new`]); tree-level invariants — parent existence, exactly one
/// root, depth consistency — are enforced by [`crate::FactArenaBuilder`]
/// at the point a `Call` is added to an arena, since those invariants
/// are properties of the *tree*, not of any one call in isolation.
///
/// ## Ownership
/// Owned by [`crate::FactArena`]; referenced elsewhere by [`CallId`].
/// `Clone` is derived because arena construction needs to move `Call`
/// values into its backing `Vec`, and because downstream crates (e.g.
/// `matcher`, when building a candidate match) legitimately need an
/// owned copy of a specific call's data without holding a borrow of the
/// whole arena — a `Call` is small and copying it is cheap relative to
/// the analysis performed on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Call {
    /// This call's identity within its arena.
    pub id: CallId,
    /// The parent call, or `None` iff this is the trace's root call.
    pub parent: Option<CallId>,
    /// The EVM opcode this call represents.
    pub kind: CallKind,
    /// This call's depth in the tree; the root call has depth 0.
    pub depth: CallDepth,
    /// The address that initiated this call.
    pub from: Address,
    /// The address this call targeted. `None` only for a `Create`/
    /// `Create2` call whose deployment address is not yet knowable at
    /// the point of the call itself (ingestion may resolve it once the
    /// call completes and populate a *separate*, later `Call` — this
    /// crate does not invent a "deployed-address" convention beyond what
    /// the Architecture document specifies, per Assumption A-4's stated
    /// minimalism).
    pub to: Option<Address>,
    /// The wei value transferred by this call. Always [`Wei::ZERO`] for
    /// a [`CallKind::StaticCall`] (enforced by [`Call::new`]).
    pub value: Wei,
    /// Gas made available to this call.
    pub gas_limit: Gas,
    /// Gas actually consumed by this call and its subtree.
    pub gas_used: Gas,
    /// Whether this call completed successfully (`true`) or reverted
    /// (`false`). A reverted call's subtree may still contain calls —
    /// the EVM executes children before a parent's revert is known —
    /// which is exactly why grounding needs per-call success status
    /// rather than assuming an entire subtree either all-succeeded or
    /// all-failed.
    pub succeeded: bool,
    /// The first four bytes of this call's calldata (the ABI function
    /// selector), if calldata was present and at least 4 bytes long.
    ///
    /// `None` means either no calldata was recorded for this call, or
    /// the recorded calldata was shorter than 4 bytes (e.g. a plain
    /// ETH transfer with empty `input`) — this type does not
    /// distinguish those two cases, matching [`crate`]'s general
    /// policy of not inventing a distinction the underlying trace data
    /// doesn't itself make. Only the bare 4 bytes are exposed: no
    /// function-signature lookup and no ABI parameter decoding is
    /// performed anywhere in this crate.
    pub selector: Option<[u8; 4]>,
}

impl Call {
    /// Construct a `Call`, enforcing this type's local invariants.
    ///
    /// # Errors
    /// Returns a descriptive `&'static str` if `kind` is
    /// [`CallKind::StaticCall`] and `value` is nonzero — a structurally
    /// impossible combination under EVM semantics, per
    /// [`CallKind::can_carry_value`].
    ///
    /// This constructor deliberately does **not** validate `id`,
    /// `parent`, or `depth` against any other call: those are tree-level
    /// concerns validated by [`crate::FactArenaBuilder::build`], not
    /// concerns a single `Call` can check in isolation (it has no way to
    /// see its siblings).
    // Every argument is a distinct, already-strongly-typed field; the two
    // same-typed pairs (`from`/`to: Address`, `gas_limit`/`gas_used: Gas`)
    // are given unambiguous parameter names and are exactly the fields a
    // `Call` fundamentally has — a builder would add call-site ceremony
    // (`.from(x).to(y)...`) without removing the same-type-mixup risk a
    // builder's own chained setters share.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: CallId,
        parent: Option<CallId>,
        kind: CallKind,
        depth: CallDepth,
        from: Address,
        to: Option<Address>,
        value: Wei,
        gas_limit: Gas,
        gas_used: Gas,
        succeeded: bool,
    ) -> Result<Self, &'static str> {
        if !kind.can_carry_value() && value != Wei::ZERO {
            return Err("a StaticCall cannot carry a nonzero value transfer");
        }
        Ok(Self {
            id,
            parent,
            kind,
            depth,
            from,
            to,
            value,
            gas_limit,
            gas_used,
            succeeded,
            selector: None,
        })
    }

    /// Whether this is the root call of its trace (has no parent).
    #[must_use]
    pub const fn is_root(&self) -> bool {
        self.parent.is_none()
    }

    /// Set this call's function selector, returning the modified call.
    ///
    /// Additive by design: added alongside [`Call::new`] rather than as
    /// a new constructor parameter, so every pre-existing call site
    /// across the workspace continues to compile unchanged. A call
    /// with no calldata (or calldata shorter than 4 bytes) simply never
    /// calls this method, leaving `selector` at its `Call::new` default
    /// of `None`.
    #[must_use]
    pub const fn with_selector(mut self, selector: [u8; 4]) -> Self {
        self.selector = Some(selector);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(byte: u8) -> Address {
        Address::new([byte; 20])
    }

    #[test]
    fn static_call_rejects_nonzero_value() {
        let result = Call::new(
            CallId::from_index(0),
            None,
            CallKind::StaticCall,
            CallDepth(0),
            addr(1),
            Some(addr(2)),
            Wei(1),
            Gas(21_000),
            Gas(21_000),
            true,
        );
        assert!(result.is_err());
    }

    #[test]
    fn static_call_accepts_zero_value() {
        let result = Call::new(
            CallId::from_index(0),
            None,
            CallKind::StaticCall,
            CallDepth(0),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Gas(21_000),
            Gas(21_000),
            true,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn root_call_has_no_parent() {
        let call = Call::new(
            CallId::from_index(0),
            None,
            CallKind::Call,
            CallDepth(0),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Gas(21_000),
            Gas(21_000),
            true,
        )
        .unwrap();
        assert!(call.is_root());
    }

    #[test]
    fn delegate_call_and_call_code_execute_in_caller_context() {
        assert!(CallKind::DelegateCall.executes_in_caller_context());
        assert!(CallKind::CallCode.executes_in_caller_context());
        assert!(!CallKind::Call.executes_in_caller_context());
        assert!(!CallKind::StaticCall.executes_in_caller_context());
        assert!(!CallKind::Create.executes_in_caller_context());
        assert!(!CallKind::Create2.executes_in_caller_context());
        assert!(!CallKind::Selfdestruct.executes_in_caller_context());
    }

    #[test]
    fn selfdestruct_can_carry_the_beneficiary_transfer() {
        // SELFDESTRUCT sends the contract's entire remaining balance to
        // its beneficiary — the one non-`StaticCall` case this
        // predicate exists to keep permissive.
        assert!(CallKind::Selfdestruct.can_carry_value());
        let call = Call::new(
            CallId::from_index(0),
            None,
            CallKind::Selfdestruct,
            CallDepth(0),
            addr(1),
            Some(addr(2)),
            Wei(1_000),
            Gas(5_000),
            Gas(5_000),
            true,
        );
        assert!(call.is_ok());
    }

    #[test]
    fn call_new_defaults_selector_to_none() {
        let call = Call::new(
            CallId::from_index(0),
            None,
            CallKind::Call,
            CallDepth(0),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Gas(21_000),
            Gas(21_000),
            true,
        )
        .unwrap();
        assert_eq!(call.selector, None);
    }

    #[test]
    fn with_selector_sets_the_field() {
        let call = Call::new(
            CallId::from_index(0),
            None,
            CallKind::Call,
            CallDepth(0),
            addr(1),
            Some(addr(2)),
            Wei::ZERO,
            Gas(21_000),
            Gas(21_000),
            true,
        )
        .unwrap()
        .with_selector([0xa9, 0x05, 0x9c, 0xbb]);
        assert_eq!(call.selector, Some([0xa9, 0x05, 0x9c, 0xbb]));
    }

    #[test]
    fn call_json_roundtrip_with_selector() {
        let call = Call::new(
            CallId::from_index(3),
            Some(CallId::from_index(0)),
            CallKind::DelegateCall,
            CallDepth(1),
            addr(1),
            Some(addr(2)),
            Wei(500),
            Gas(100_000),
            Gas(80_000),
            false,
        )
        .unwrap()
        .with_selector([0x09, 0x5e, 0xa7, 0xb3]);
        let json = serde_json::to_string(&call).unwrap();
        let back: Call = serde_json::from_str(&json).unwrap();
        assert_eq!(call, back);
        assert_eq!(back.selector, Some([0x09, 0x5e, 0xa7, 0xb3]));
    }

    #[test]
    fn call_json_roundtrip() {
        let call = Call::new(
            CallId::from_index(3),
            Some(CallId::from_index(0)),
            CallKind::DelegateCall,
            CallDepth(1),
            addr(1),
            Some(addr(2)),
            Wei(500),
            Gas(100_000),
            Gas(80_000),
            false,
        )
        .unwrap();
        let json = serde_json::to_string(&call).unwrap();
        let back: Call = serde_json::from_str(&json).unwrap();
        assert_eq!(call, back);
    }
}

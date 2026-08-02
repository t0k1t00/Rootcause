//! [`ValueFlow`]: a computed view of value movement between two
//! addresses, derived from [`crate::Call`] data — not a stored fact.

use crate::call::Call;
use crate::ids::CallId;
use crate::primitives::{Address, Wei};

/// One edge of value movement: `amount` wei moved from `from` to `to`
/// during a specific call.
///
/// ## Why this is computed, not stored in [`crate::FactArena`]
/// Every [`Call`] already carries a `value: Wei` field — that *is* the
/// value-flow fact, as directly observed by ingestion. Storing a second,
/// separate `ValueFlow` fact for the same information in the arena would
/// create two representations of one underlying fact that could
/// disagree (e.g. if one were updated and the other weren't) — the
/// project's own emphasis on avoiding invented duplicate state and on
/// grounding always citing a *specific* fact would then have to specify
/// which of the two an evidence citation means. Instead, `ValueFlow`
/// values are produced on demand by [`ValueFlow::from_call`] and
/// [`crate::FactArena::value_flows`], always derived fresh from the
/// `Call` that is the actual fact — so a `ValueFlow` can never drift
/// from the `Call` it describes, and any evidence citation naming a
/// value-flow observation cites the underlying [`CallId`], not a
/// separate, redundant ID.
///
/// ## Ownership
/// `Copy` — small and derived, never stored, so there is no arena
/// ownership question to answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValueFlow {
    /// The call this value movement occurred during — the fact this
    /// `ValueFlow` is derived from and the citation grounding should use.
    pub call_id: CallId,
    /// The sending address.
    pub from: Address,
    /// The receiving address. `None` if the call had no resolved
    /// recipient (mirrors [`Call::to`]).
    pub to: Option<Address>,
    /// The amount moved.
    pub amount: Wei,
}

impl ValueFlow {
    /// Derive a `ValueFlow` from a single [`Call`], if that call actually
    /// moved a nonzero amount. Returns `None` for a zero-value call
    /// (there is no "flow" to report — value-flow is meant to answer
    /// "where did value move," and a zero-value call is not an instance
    /// of that question).
    #[must_use]
    pub const fn from_call(call: &Call) -> Option<Self> {
        if call.value.0 == 0 {
            return None;
        }
        Some(Self {
            call_id: call.id,
            from: call.from,
            to: call.to,
            amount: call.value,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call::CallKind;
    use crate::ids::CallId;
    use crate::primitives::{CallDepth, Gas};

    fn addr(byte: u8) -> Address {
        Address::new([byte; 20])
    }

    #[test]
    fn from_call_none_for_zero_value() {
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
        assert!(ValueFlow::from_call(&call).is_none());
    }

    #[test]
    fn from_call_some_for_nonzero_value() {
        let call = Call::new(
            CallId::from_index(0),
            None,
            CallKind::Call,
            CallDepth(0),
            addr(1),
            Some(addr(2)),
            Wei(500),
            Gas(21_000),
            Gas(21_000),
            true,
        )
        .unwrap();
        let flow = ValueFlow::from_call(&call).unwrap();
        assert_eq!(flow.from, addr(1));
        assert_eq!(flow.to, Some(addr(2)));
        assert_eq!(flow.amount, Wei(500));
        assert_eq!(flow.call_id, call.id);
    }
}

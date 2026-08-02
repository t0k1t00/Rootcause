//! Arena identity: small `Copy` ID newtypes and [`FactRef`], the sum type
//! used to cite an arbitrary fact by identity rather than by reference.
//!
//! Every ID here is a thin wrapper around a `u32` index into the
//! corresponding [`crate::FactArena`] vector. `u32` (not `usize`) is a
//! deliberate choice: it bounds a single trace to ~4 billion facts of any
//! one kind (far beyond any real trace, which the EVM's own call-depth
//! and gas limits keep to at most a few thousand calls), keeps IDs
//! `Copy` and cheap to pass around and store in benchmark output, and
//! keeps ID size platform-independent (`usize` varies; a persisted
//! benchmark result citing a `usize`-sized ID would not be portable).

use std::fmt;

use serde::{Deserialize, Serialize};

/// Defines a `u32`-backed arena ID newtype with the standard set of
/// derives every such ID needs, plus an `index()` accessor for arena
/// implementations to use when indexing their backing `Vec`.
macro_rules! arena_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        pub struct $name(u32);

        impl $name {
            /// Construct an ID from a raw arena index.
            ///
            /// This is `pub(crate)`, not `pub`: IDs are meant to be
            /// handed out by [`crate::FactArenaBuilder`] as facts are
            /// added, in order, so that an ID's numeric value always
            /// equals its position in the arena's backing `Vec`. Public
            /// construction would let a caller manufacture an ID that
            /// does not correspond to any real fact, defeating the
            /// entire point of using IDs instead of references for
            /// grounding citations.
            #[must_use]
            pub(crate) const fn from_index(index: u32) -> Self {
                Self(index)
            }

            /// The raw arena index this ID refers to.
            #[must_use]
            pub const fn index(self) -> u32 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}#{}", stringify!($name), self.0)
            }
        }
    };
}

arena_id!(
    /// Identifies a [`crate::Call`] within its [`crate::FactArena`].
    CallId
);

arena_id!(
    /// Identifies a [`crate::StorageChange`] within its
    /// [`crate::FactArena`].
    StorageChangeId
);

arena_id!(
    /// Identifies a [`crate::LogEvent`] within its [`crate::FactArena`].
    LogId
);

arena_id!(
    /// Identifies a [`crate::TokenTransfer`] within its
    /// [`crate::FactArena`].
    TokenTransferId
);

/// A reference to exactly one fact in a [`crate::FactArena`], of any
/// kind.
///
/// This is the type a grounding result (in the `grounding` crate, not
/// implemented here) uses to cite the specific evidence backing a
/// matched pattern clause — directly satisfying the Architecture
/// document's central requirement that a classification ground against
/// "a specific `Call`/`StorageChange`" (Assumption A-4) and record
/// exactly which fact backs each clause (Assumption A-8).
///
/// `fact-model` defines this type, but does not itself interpret it:
/// this crate has no notion of "grounding," "matching," or "evidence" —
/// it only provides the identity primitive those concepts are built on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id")]
pub enum FactRef {
    /// Refers to a specific [`crate::Call`].
    Call(CallId),
    /// Refers to a specific [`crate::StorageChange`].
    StorageChange(StorageChangeId),
    /// Refers to a specific [`crate::LogEvent`].
    Log(LogId),
    /// Refers to a specific [`crate::TokenTransfer`].
    TokenTransfer(TokenTransferId),
}

impl fmt::Display for FactRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Call(id) => write!(f, "{id}"),
            Self::StorageChange(id) => write!(f, "{id}"),
            Self::Log(id) => write!(f, "{id}"),
            Self::TokenTransfer(id) => write!(f, "{id}"),
        }
    }
}

impl From<CallId> for FactRef {
    fn from(id: CallId) -> Self {
        Self::Call(id)
    }
}

impl From<StorageChangeId> for FactRef {
    fn from(id: StorageChangeId) -> Self {
        Self::StorageChange(id)
    }
}

impl From<LogId> for FactRef {
    fn from(id: LogId) -> Self {
        Self::Log(id)
    }
}

impl From<TokenTransferId> for FactRef {
    fn from(id: TokenTransferId) -> Self {
        Self::TokenTransfer(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_id_display_includes_index() {
        let id = CallId::from_index(7);
        assert_eq!(id.to_string(), "CallId#7");
        assert_eq!(id.index(), 7);
    }

    #[test]
    fn fact_ref_from_conversions() {
        let call_ref: FactRef = CallId::from_index(1).into();
        assert_eq!(call_ref, FactRef::Call(CallId::from_index(1)));

        let storage_ref: FactRef = StorageChangeId::from_index(2).into();
        assert_eq!(
            storage_ref,
            FactRef::StorageChange(StorageChangeId::from_index(2))
        );

        let log_ref: FactRef = LogId::from_index(3).into();
        assert_eq!(log_ref, FactRef::Log(LogId::from_index(3)));

        let transfer_ref: FactRef = TokenTransferId::from_index(4).into();
        assert_eq!(
            transfer_ref,
            FactRef::TokenTransfer(TokenTransferId::from_index(4))
        );
    }

    #[test]
    fn fact_ref_json_roundtrip() {
        let refs = [
            FactRef::Call(CallId::from_index(0)),
            FactRef::StorageChange(StorageChangeId::from_index(1)),
            FactRef::Log(LogId::from_index(2)),
            FactRef::TokenTransfer(TokenTransferId::from_index(3)),
        ];
        for r in refs {
            let json = serde_json::to_string(&r).unwrap();
            let back: FactRef = serde_json::from_str(&json).unwrap();
            assert_eq!(r, back);
        }
    }

    #[test]
    fn ids_are_ordered_by_index() {
        let a = CallId::from_index(1);
        let b = CallId::from_index(2);
        assert!(a < b);
    }
}

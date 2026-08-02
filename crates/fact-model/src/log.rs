//! [`LogEvent`] (a raw EVM log fact) and [`TokenTransfer`] (an
//! interpreted fact derived from one).

use serde::{Deserialize, Serialize};

use crate::ids::{CallId, LogId};
use crate::primitives::{Address, LogIndex, Wei, Word};

/// A raw EVM log emitted by a `LOG0`-`LOG4` opcode during a specific
/// call.
///
/// ## Why topics are `Vec<Word>` and not up to four named fields
/// The EVM permits 0-4 topics; a fixed four-field struct would need
/// `Option<Word>` for each and could still represent an invalid
/// five-topic log. A `Vec` with a length invariant enforced at
/// construction (see [`LogEvent::new`]) is both simpler and strictly
/// more precise about what's actually representable.
///
/// ## Ownership
/// `data` is a `Vec<u8>` of arbitrary, ingestion-determined length (log
/// data has no protocol-level size bound); `Clone` is derived for the
/// same reason as [`crate::Call`] — downstream crates need to hold an
/// owned copy of a specific log's data without borrowing the whole
/// arena.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEvent {
    /// This log's identity within its arena.
    pub id: LogId,
    /// The call that emitted this log.
    pub call_id: CallId,
    /// The contract address that emitted this log (the currently-
    /// executing address, which for a log emitted mid-`delegatecall` is
    /// *not* necessarily the same as the code's original deploying
    /// address — this crate records the fact as the EVM defines it, not
    /// as any particular interpretation of "who really did this").
    pub address: Address,
    /// This log's topics, in order (`topics[0]` is conventionally the
    /// event signature hash, by Solidity's ABI convention, but
    /// `fact-model` does not assume or enforce that convention — it is
    /// an `ingestion`/`grounding`-level interpretation, not a structural
    /// fact).
    pub topics: Vec<Word>,
    /// The log's non-indexed data payload.
    pub data: Vec<u8>,
    /// This log's index within its transaction's full log list
    /// (`logIndex` in JSON-RPC receipt terms) — distinct from `id`,
    /// which is this trace's own arena-local identity; `log_index` is
    /// the externally-meaningful position ingestion observed on-chain,
    /// preserved for citing evidence in a form a third party could
    /// independently verify against a block explorer.
    pub log_index: LogIndex,
}

impl LogEvent {
    /// Construct a `LogEvent`.
    ///
    /// # Errors
    /// Returns a descriptive `&'static str` if `topics.len() > 4`, which
    /// the EVM's `LOG0`-`LOG4` opcodes make structurally impossible.
    pub fn new(
        id: LogId,
        call_id: CallId,
        address: Address,
        topics: Vec<Word>,
        data: Vec<u8>,
        log_index: LogIndex,
    ) -> Result<Self, &'static str> {
        if topics.len() > 4 {
            return Err("an EVM log can have at most 4 topics (LOG0..LOG4)");
        }
        Ok(Self {
            id,
            call_id,
            address,
            topics,
            data,
            log_index,
        })
    }
}

/// An interpreted fact: a token transfer decoded from a log that matches
/// the standard ERC-20/ERC-721 `Transfer(address,address,uint256)` shape.
///
/// ## Why this is a distinct type from [`LogEvent`], not a derived
/// accessor method on it
/// A `LogEvent` is a directly-observed fact: the EVM really did emit
/// exactly these bytes. A `TokenTransfer` is an *interpretation* of one
/// — a claim that a specific log's topics/data decode to a transfer with
/// these specific parties and amount, under an assumed event signature.
/// Collapsing that distinction (e.g. by giving `LogEvent` a
/// `fn as_token_transfer(&self) -> TokenTransfer` that always succeeds)
/// would silently launder an interpretation into something that looks
/// like a directly-observed fact — precisely the failure mode ("optional
/// evidence treated as load-bearing," Architecture doc, Assumption A-8's
/// source discussion) the grounding verifier exists to prevent. Keeping
/// `TokenTransfer` a separate, explicitly-constructed type means every
/// downstream consumer can see, at the type level, that this fact
/// depends on a decoding assumption the raw `LogEvent` does not carry.
///
/// `fact-model` does not itself perform this decoding — no ABI
/// knowledge lives in this crate — it only defines the shape of the
/// result once some other crate (`ingestion`) has done so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenTransfer {
    /// The log this interpretation was decoded from.
    pub log_id: LogId,
    /// The token contract address (the log's emitting address).
    pub token: Address,
    /// The sending address.
    pub from: Address,
    /// The receiving address.
    pub to: Address,
    /// The transferred amount.
    ///
    /// For an ERC-721 transfer this is a token ID, not a value — a
    /// simplification of Assumption A-4's minimal scope: the Architecture
    /// document names value-flow tracking as an ERC-20/native-value
    /// concern (Phase 2's exploit-family analysis discusses "net value
    /// extracted," not NFT identity), so ERC-721-specific handling
    /// (a dedicated `token_id` field, ownership-history semantics) is
    /// left for a follow-on task rather than invented here.
    pub amount: Wei,
}

impl TokenTransfer {
    /// Construct a `TokenTransfer`. Infallible at the single-fact level;
    /// `log_id` referencing a real log is a cross-referential invariant
    /// enforced by [`crate::FactArenaBuilder::build`].
    #[must_use]
    pub const fn new(
        log_id: LogId,
        token: Address,
        from: Address,
        to: Address,
        amount: Wei,
    ) -> Self {
        Self {
            log_id,
            token,
            from,
            to,
            amount,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(byte: u8) -> Address {
        Address::new([byte; 20])
    }

    #[test]
    fn log_event_rejects_more_than_four_topics() {
        let topics = vec![Word::ZERO; 5];
        let result = LogEvent::new(
            LogId::from_index(0),
            CallId::from_index(0),
            addr(1),
            topics,
            vec![],
            LogIndex(0),
        );
        assert!(result.is_err());
    }

    #[test]
    fn log_event_accepts_exactly_four_topics() {
        let topics = vec![Word::ZERO; 4];
        let result = LogEvent::new(
            LogId::from_index(0),
            CallId::from_index(0),
            addr(1),
            topics,
            vec![],
            LogIndex(0),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn log_event_json_roundtrip() {
        let log = LogEvent::new(
            LogId::from_index(1),
            CallId::from_index(0),
            addr(2),
            vec![Word::new([1; 32]), Word::new([2; 32])],
            vec![0xde, 0xad, 0xbe, 0xef],
            LogIndex(5),
        )
        .unwrap();
        let json = serde_json::to_string(&log).unwrap();
        let back: LogEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(log, back);
    }

    #[test]
    fn token_transfer_json_roundtrip() {
        let transfer =
            TokenTransfer::new(LogId::from_index(0), addr(1), addr(2), addr(3), Wei(1_000));
        let json = serde_json::to_string(&transfer).unwrap();
        let back: TokenTransfer = serde_json::from_str(&json).unwrap();
        assert_eq!(transfer, back);
    }
}

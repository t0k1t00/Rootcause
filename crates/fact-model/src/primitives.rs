//! Scalar newtypes shared across the fact model.
//!
//! Every type here wraps a single primitive value and exists purely to
//! prevent semantically distinct quantities (a block number, a chain ID,
//! a gas amount) from being interchangeable at the type level, and to
//! give fixed-size byte data (addresses, 256-bit words) a human-readable
//! serialized form.

use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A 20-byte EVM address.
///
/// ## Invariants
/// None beyond the fixed length — any 20-byte value is a syntactically
/// valid `Address`. Whether it corresponds to a real, deployed contract
/// or an EOA is not a fact this type can express; that is a semantic
/// question for `ingestion`/`grounding`, not a structural one for
/// `fact-model`.
///
/// ## Ownership
/// `Copy` — 20 bytes is cheap to duplicate, and treating an address like
/// a value (as every EVM tool does) avoids borrow-checker friction
/// throughout the rest of the workspace for no real cost.
///
/// ## Serialization
/// Serializes as a lowercase `0x`-prefixed 40-hex-digit string (e.g.
/// `"0xd8da6bf26964af9d7eed9e03e53415d37aa96045"`), matching every
/// Ethereum tool's convention, rather than the derived `[u8; 20]` JSON
/// array form, which no external tool or human would recognize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Address([u8; 20]);

impl Address {
    /// Construct an `Address` from raw bytes. Infallible: any 20 bytes
    /// are a structurally valid address.
    #[must_use]
    pub const fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    /// The all-zero address, commonly used as a sentinel for "no
    /// recipient" (e.g. a `CREATE` call target before deployment) in
    /// some tooling. `fact-model` does not itself assign this meaning —
    /// callers that need "no address" should prefer `Option<Address>`,
    /// which this crate uses throughout (see [`crate::Call::to`]).
    pub const ZERO: Self = Self([0u8; 20]);

    /// Borrow the underlying bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x")?;
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::LowerHex for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// A raw 256-bit EVM word: a storage slot key, a storage slot value, or a
/// log topic. Unlike [`Wei`], this is intentionally **not** narrowed to a
/// smaller integer type, because storage values are arbitrary bit
/// patterns (packed structs, hashes, addresses left-padded to 32 bytes)
/// with no guaranteed numeric interpretation.
///
/// ## Ownership
/// `Copy`, for the same reason as [`Address`]: 32 bytes is cheap, and
/// storage keys/values are naturally value types.
///
/// ## Serialization
/// Serializes as a `0x`-prefixed 64-hex-digit string, for the same
/// human/tool-readability reason as [`Address`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Word([u8; 32]);

impl Word {
    /// Construct a `Word` from raw bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The all-zero word — the default/unset value of every EVM storage
    /// slot before it is first written.
    pub const ZERO: Self = Self([0u8; 32]);

    /// Borrow the underlying bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x")?;
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// A transaction hash — structurally a [`Word`], given a distinct name
/// because "a 32-byte value that happens to be a tx hash" and "a 32-byte
/// storage value" are never interchangeable in practice, and conflating
/// them at the type level would be exactly the primitive-obsession
/// failure mode this module exists to avoid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TxHash(pub Word);

impl fmt::Display for TxHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// A quantity of wei (10⁻¹⁸ ETH), used for transaction/call `value` and
/// derived [`crate::ValueFlow`] amounts.
///
/// ## Why `u128` and not a full 256-bit integer
/// EVM balances are technically 256-bit, but total ETH supply
/// (~120,000,000 ETH ≈ 1.2 × 10²⁶ wei) fits inside `u128`
/// (max ≈ 3.4 × 10³⁸) with 12 orders of magnitude to spare, and every
/// value this project needs to reason about (a real economic quantity
/// moved in a real transaction) is bounded by realistic economic
/// activity, not by the full 256-bit range. This is a deliberate,
/// documented narrowing — see ADR-0006 — and is why `Wei` is distinct
/// from [`Word`]: storage slot *values* are arbitrary bit patterns with
/// no such bound and are never narrowed.
///
/// Construction from a `Word` (as ingestion will need, since the EVM
/// itself represents balances as 256-bit words) is therefore fallible —
/// see [`Wei::from_word`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Wei(pub u128);

impl Wei {
    /// Zero wei.
    pub const ZERO: Self = Self(0);

    /// Attempt to narrow a full 256-bit EVM word into a `Wei`.
    ///
    /// # Errors
    /// Returns `None` if the word's value exceeds `u128::MAX` — i.e. more
    /// than ~3.4 × 10²⁰ ETH, several orders of magnitude beyond total
    /// ETH supply. In practice this only fires on malformed or
    /// adversarially-crafted ingestion input, never on a real balance.
    #[must_use]
    pub fn from_word(word: Word) -> Option<Self> {
        let bytes = word.as_bytes();
        // A Word is big-endian, 32 bytes. It fits in u128 iff the first
        // 16 bytes (the high-order half) are all zero.
        if bytes[..16] != [0u8; 16] {
            return None;
        }
        let mut low = [0u8; 16];
        low.copy_from_slice(&bytes[16..]);
        Some(Self(u128::from_be_bytes(low)))
    }

    /// Checked addition, returning `None` on overflow rather than
    /// panicking or silently wrapping — value-extraction accounting
    /// (see [`crate::ValueFlow`]) must never silently produce a wrong
    /// number.
    #[must_use]
    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0).map(Self)
    }

    /// Checked subtraction, returning `None` on underflow.
    #[must_use]
    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.0.checked_sub(rhs.0).map(Self)
    }
}

impl fmt::Display for Wei {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} wei", self.0)
    }
}

/// Macro to define a `Copy` newtype wrapping a small integer, with the
/// small set of trait impls every such type in this module needs. Using
/// a macro here (rather than hand-writing seven near-identical blocks)
/// is itself a maintainability decision: it makes it structurally
/// impossible for one of these types to accidentally skip a derive that
/// the others have.
macro_rules! integer_newtype {
    ($(#[$meta:meta])* $name:ident($inner:ty)) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        pub struct $name(pub $inner);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

integer_newtype!(
    /// The block number a trace's transaction was included in.
    BlockNumber(u64)
);

integer_newtype!(
    /// The EIP-155 chain ID a transaction was executed on. Required for
    /// unambiguous cross-chain trace identity: the same `TxHash` is not
    /// guaranteed unique across chains.
    ChainId(u64)
);

integer_newtype!(
    /// A block's Unix timestamp, in seconds. Distinct from any
    /// "ingestion time" (which this crate deliberately never records —
    /// see [`crate::trace::TraceMetadata`] — to keep the fact model
    /// itself free of non-deterministic, wall-clock-dependent state).
    Timestamp(u64)
);

integer_newtype!(
    /// A transaction's nonce.
    Nonce(u64)
);

integer_newtype!(
    /// A gas quantity (used, limit, or price context — the specific
    /// meaning is carried by the field name, not by this type).
    Gas(u64)
);

integer_newtype!(
    /// A call's depth in its trace's call tree; the root call is depth 0.
    /// `u16` is sufficient (and deliberately narrower than `u64`) because
    /// EVM call depth is protocol-limited to 1024 (EIP-150) — any value
    /// this type could represent but the EVM could never produce is a
    /// bug, not a legitimate future case.
    CallDepth(u16)
);

integer_newtype!(
    /// The index of a log within its transaction's log list (`logIndex`
    /// in JSON-RPC receipt terms).
    LogIndex(u32)
);

/// A `serde` [`Visitor`] shared by the hex-string `Deserialize` impls for
/// [`Address`] and [`Word`], parameterized over the expected byte length.
/// Factored out once rather than duplicated per type, since the parsing
/// logic (strip `0x`, validate length, decode hex) is identical.
struct HexBytesVisitor<const N: usize>;

impl<const N: usize> Visitor<'_> for HexBytesVisitor<N> {
    type Value = [u8; N];

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a 0x-prefixed hex string encoding {N} bytes")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let hex_digits = v.strip_prefix("0x").unwrap_or(v);
        if hex_digits.len() != N * 2 {
            return Err(E::invalid_length(hex_digits.len(), &self));
        }
        let mut out = [0u8; N];
        for (i, byte_out) in out.iter_mut().enumerate() {
            let hi = hex_digit(hex_digits.as_bytes()[i * 2])
                .ok_or_else(|| E::invalid_value(de::Unexpected::Str(v), &"a valid hex string"))?;
            let lo = hex_digit(hex_digits.as_bytes()[i * 2 + 1])
                .ok_or_else(|| E::invalid_value(de::Unexpected::Str(v), &"a valid hex string"))?;
            *byte_out = (hi << 4) | lo;
        }
        Ok(out)
    }
}

/// Decode a single ASCII hex digit, case-insensitively. Returns `None`
/// for any non-hex-digit byte.
const fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

impl Serialize for Address {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer
            .deserialize_str(HexBytesVisitor::<20>)
            .map(Self)
    }
}

impl Serialize for Word {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Word {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer
            .deserialize_str(HexBytesVisitor::<32>)
            .map(Self)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn address_display_is_lowercase_hex_with_prefix() {
        let addr = Address::new([0xd8; 20]);
        assert_eq!(addr.to_string(), format!("0x{}", "d8".repeat(20)));
    }

    #[test]
    fn address_json_roundtrip() {
        let addr = Address::new([0xab; 20]);
        let json = serde_json::to_string(&addr).unwrap();
        assert_eq!(json, format!("\"0x{}\"", "ab".repeat(20)));
        let back: Address = serde_json::from_str(&json).unwrap();
        assert_eq!(addr, back);
    }

    #[test]
    fn address_json_accepts_uppercase_and_missing_prefix() {
        let json_no_prefix = format!("\"{}\"", "AB".repeat(20));
        let addr: Address = serde_json::from_str(&json_no_prefix).unwrap();
        assert_eq!(addr, Address::new([0xab; 20]));
    }

    #[test]
    fn address_json_rejects_wrong_length() {
        let json = "\"0xdead\"";
        let result: Result<Address, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn address_json_rejects_non_hex() {
        let json = format!("\"0x{}\"", "zz".repeat(20));
        let result: Result<Address, _> = serde_json::from_str(&format!("\"{json}\""));
        assert!(result.is_err());
    }

    #[test]
    fn word_json_roundtrip() {
        let word = Word::new([0x42; 32]);
        let json = serde_json::to_string(&word).unwrap();
        let back: Word = serde_json::from_str(&json).unwrap();
        assert_eq!(word, back);
    }

    #[test]
    fn wei_from_word_narrow_case() {
        let mut bytes = [0u8; 32];
        bytes[31] = 42;
        let word = Word::new(bytes);
        assert_eq!(Wei::from_word(word), Some(Wei(42)));
    }

    #[test]
    fn wei_from_word_overflow_case() {
        let word = Word::new([0xff; 32]); // far larger than u128::MAX
        assert_eq!(Wei::from_word(word), None);
    }

    #[test]
    fn wei_checked_add_overflow() {
        let max = Wei(u128::MAX);
        assert_eq!(max.checked_add(Wei(1)), None);
    }

    #[test]
    fn wei_checked_sub_underflow() {
        assert_eq!(Wei(0).checked_sub(Wei(1)), None);
    }

    proptest! {
        /// Any 20-byte sequence round-trips through Address's hex
        /// serialization unchanged, for arbitrary input — not just the
        /// hand-picked cases above.
        #[test]
        fn address_roundtrip_prop(bytes in prop::array::uniform20(any::<u8>())) {
            let addr = Address::new(bytes);
            let json = serde_json::to_string(&addr).unwrap();
            let back: Address = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(addr, back);
        }

        /// Any 32-byte sequence round-trips through Word's hex
        /// serialization unchanged.
        #[test]
        fn word_roundtrip_prop(bytes in prop::array::uniform32(any::<u8>())) {
            let word = Word::new(bytes);
            let json = serde_json::to_string(&word).unwrap();
            let back: Word = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(word, back);
        }

        /// `Wei::from_word` never panics on arbitrary 32-byte input, and
        /// whenever it succeeds, converting back to big-endian bytes and
        /// re-narrowing gives the same value (round-trip on the subset
        /// where narrowing is defined).
        #[test]
        fn wei_from_word_never_panics(bytes in prop::array::uniform32(any::<u8>())) {
            let word = Word::new(bytes);
            if let Some(wei) = Wei::from_word(word) {
                let mut roundtrip_bytes = [0u8; 32];
                roundtrip_bytes[16..].copy_from_slice(&wei.0.to_be_bytes());
                prop_assert_eq!(Word::new(roundtrip_bytes), word);
            }
        }

        /// checked_add/checked_sub never panic, and when they succeed,
        /// they're inverses of each other.
        #[test]
        fn wei_checked_add_sub_inverse(a in any::<u64>(), b in any::<u64>()) {
            let a = Wei(u128::from(a));
            let b = Wei(u128::from(b));
            if let Some(sum) = a.checked_add(b) {
                prop_assert_eq!(sum.checked_sub(b), Some(a));
            }
        }
    }
}

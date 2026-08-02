//! Hex-string parsing helpers used by [`crate::normalize`].
//!
//! Fixed-width fields (addresses, 32-byte words) are parsed by reusing
//! `fact-model`'s own hex `Deserialize` impls (routed through
//! `serde_json::from_value`) rather than duplicating hex-parsing logic —
//! there is exactly one place in the whole workspace that knows how to
//! turn a `0x`-prefixed string into an `Address`/`Word`, and it lives in
//! `fact-model`. Variable-width numeric fields (block numbers, gas,
//! nonces, wei amounts) have no `fact-model` equivalent to reuse (they're
//! plain integers, not fixed-byte-length types), so this module parses
//! those directly.

use fact_model::{Address, Word};
use serde_json::Value;

use crate::error::IngestionError;

/// Parse a `0x`-prefixed hex string into a `u64`, in a named field
/// context for error reporting.
///
/// # Errors
/// Returns [`IngestionError::MalformedField`] if `raw` (after stripping
/// an optional `0x` prefix) is not valid hex, or overflows `u64`.
pub fn parse_hex_u64(field: &'static str, raw: &str) -> Result<u64, IngestionError> {
    let digits = raw.strip_prefix("0x").unwrap_or(raw);
    u64::from_str_radix(digits, 16).map_err(|_| IngestionError::MalformedField {
        field,
        value: raw.to_string(),
        reason: "not a valid hex-encoded u64",
    })
}

/// Parse a `0x`-prefixed hex string into a `u128`, in a named field
/// context for error reporting.
///
/// Per ADR-0006, wei amounts are represented as `u128` throughout the
/// fact model; a value that overflows `u128` (astronomically larger
/// than total ETH supply) is treated as malformed input, not silently
/// truncated.
///
/// # Errors
/// Returns [`IngestionError::MalformedField`] if `raw` (after stripping
/// an optional `0x` prefix) is not valid hex, or overflows `u128`.
pub fn parse_hex_u128(field: &'static str, raw: &str) -> Result<u128, IngestionError> {
    let digits = raw.strip_prefix("0x").unwrap_or(raw);
    u128::from_str_radix(digits, 16).map_err(|_| IngestionError::MalformedField {
        field,
        value: raw.to_string(),
        reason: "not a valid hex-encoded u128",
    })
}

/// Parse a `0x`-prefixed hex string into a `fact_model::Address`, reusing
/// `fact-model`'s own hex `Deserialize` impl.
///
/// # Errors
/// Returns [`IngestionError::MalformedField`] if `raw` is not a valid
/// 20-byte hex address.
pub fn parse_address(field: &'static str, raw: &str) -> Result<Address, IngestionError> {
    serde_json::from_value(Value::String(raw.to_string())).map_err(|_| {
        IngestionError::MalformedField {
            field,
            value: raw.to_string(),
            reason: "not a valid 20-byte hex address",
        }
    })
}

/// Parse a `0x`-prefixed hex string into a `fact_model::Word`, reusing
/// `fact-model`'s own hex `Deserialize` impl.
///
/// # Errors
/// Returns [`IngestionError::MalformedField`] if `raw` is not a valid
/// 32-byte hex word.
pub fn parse_word(field: &'static str, raw: &str) -> Result<Word, IngestionError> {
    serde_json::from_value(Value::String(raw.to_string())).map_err(|_| {
        IngestionError::MalformedField {
            field,
            value: raw.to_string(),
            reason: "not a valid 32-byte hex word",
        }
    })
}

/// Parse a `0x`-prefixed hex data payload (arbitrary length, unlike
/// [`parse_word`]) into raw bytes. `"0x"` (no digits) parses to an empty
/// `Vec`.
///
/// # Errors
/// Returns [`IngestionError::MalformedField`] if `raw` has an odd number
/// of hex digits, or contains a non-hex-digit character.
pub fn parse_hex_bytes(field: &'static str, raw: &str) -> Result<Vec<u8>, IngestionError> {
    let digits = raw.strip_prefix("0x").unwrap_or(raw);
    if digits.len() % 2 != 0 {
        return Err(IngestionError::MalformedField {
            field,
            value: raw.to_string(),
            reason: "hex data must have an even number of digits",
        });
    }
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    let digit_bytes = digits.as_bytes();
    for chunk in digit_bytes.chunks_exact(2) {
        let hi = hex_digit(chunk[0]).ok_or(IngestionError::MalformedField {
            field,
            value: raw.to_string(),
            reason: "invalid hex digit",
        })?;
        let lo = hex_digit(chunk[1]).ok_or(IngestionError::MalformedField {
            field,
            value: raw.to_string(),
            reason: "invalid hex digit",
        })?;
        bytes.push((hi << 4) | lo);
    }
    Ok(bytes)
}

/// Decode a single ASCII hex digit, case-insensitively.
const fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn parse_hex_u64_accepts_valid_value() {
        assert_eq!(parse_hex_u64("test", "0x1a").unwrap(), 26);
    }

    #[test]
    fn parse_hex_u64_rejects_non_hex() {
        assert!(parse_hex_u64("test", "0xzz").is_err());
    }

    #[test]
    fn parse_hex_u128_accepts_valid_value() {
        assert_eq!(parse_hex_u128("test", "0xff").unwrap(), 255);
    }

    #[test]
    fn parse_address_accepts_valid_value() {
        let addr = parse_address("test", &format!("0x{}", "ab".repeat(20))).unwrap();
        assert_eq!(addr, Address::new([0xab; 20]));
    }

    #[test]
    fn parse_address_rejects_wrong_length() {
        assert!(parse_address("test", "0xdead").is_err());
    }

    #[test]
    fn parse_word_accepts_valid_value() {
        let word = parse_word("test", &format!("0x{}", "cd".repeat(32))).unwrap();
        assert_eq!(word, Word::new([0xcd; 32]));
    }

    #[test]
    fn parse_hex_bytes_empty() {
        assert_eq!(parse_hex_bytes("test", "0x").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn parse_hex_bytes_odd_length_rejected() {
        assert!(parse_hex_bytes("test", "0xabc").is_err());
    }

    #[test]
    fn parse_hex_bytes_valid() {
        assert_eq!(
            parse_hex_bytes("test", "0xdeadbeef").unwrap(),
            vec![0xde, 0xad, 0xbe, 0xef]
        );
    }

    #[test]
    fn parse_hex_bytes_invalid_digit() {
        assert!(parse_hex_bytes("test", "0xzzzz").is_err());
    }

    proptest! {
        /// `parse_hex_u64` never panics on arbitrary strings, and for
        /// any value that actually is a valid u64, formatting it as hex
        /// and parsing it back gives the same value.
        #[test]
        fn parse_hex_u64_roundtrip(n in any::<u64>()) {
            let hex = format!("0x{n:x}");
            prop_assert_eq!(parse_hex_u64("test", &hex).unwrap(), n);
        }

        /// `parse_hex_u128` round-trips for any valid u128.
        #[test]
        fn parse_hex_u128_roundtrip(n in any::<u128>()) {
            let hex = format!("0x{n:x}");
            prop_assert_eq!(parse_hex_u128("test", &hex).unwrap(), n);
        }

        /// `parse_address` round-trips for any 20-byte sequence.
        #[test]
        fn parse_address_roundtrip(bytes in prop::array::uniform20(any::<u8>())) {
            let addr = Address::new(bytes);
            let hex = addr.to_string();
            prop_assert_eq!(parse_address("test", &hex).unwrap(), addr);
        }

        /// `parse_word` round-trips for any 32-byte sequence.
        #[test]
        fn parse_word_roundtrip(bytes in prop::array::uniform32(any::<u8>())) {
            let word = Word::new(bytes);
            let hex = word.to_string();
            prop_assert_eq!(parse_word("test", &hex).unwrap(), word);
        }

        /// `parse_hex_bytes` never panics on arbitrary strings, and
        /// round-trips for any byte sequence encoded as lowercase hex.
        #[test]
        fn parse_hex_bytes_roundtrip(bytes in prop::collection::vec(any::<u8>(), 0..64)) {
            use std::fmt::Write as _;
            let hex_digits = bytes.iter().fold(String::new(), |mut acc, b| {
                let _ = write!(acc, "{b:02x}");
                acc
            });
            let hex = format!("0x{hex_digits}");
            prop_assert_eq!(parse_hex_bytes("test", &hex).unwrap(), bytes);
        }

        /// `parse_hex_bytes` never panics on arbitrary, entirely
        /// unstructured input strings (not necessarily valid hex at
        /// all) — it either returns `Ok` or a well-formed `Err`, never
        /// panics.
        #[test]
        fn parse_hex_bytes_never_panics(s in ".*") {
            let _ = parse_hex_bytes("test", &s);
        }
    }
}

//! The Decode stage: raw bytes → [`RawTraceDocument`].
//!
//! Pure `serde_json` deserialization. Every structural problem
//! `serde`'s derive can catch for free — missing required fields, wrong
//! JSON types, unknown fields (rejected via `#[serde(deny_unknown_fields)]`
//! on every raw type, so a typo'd field name fails loudly instead of
//! being silently ignored) — is caught here, before any
//! `fact-model`-aware logic runs.

use crate::error::IngestionError;
use crate::raw::RawTraceDocument;

/// Decode raw bytes into a [`RawTraceDocument`].
///
/// # Errors
/// Returns [`IngestionError::MalformedInput`] if `bytes` is not valid
/// JSON, or does not match [`RawTraceDocument`]'s shape (a missing
/// required field, a field with the wrong JSON type, or an unrecognized
/// field name).
pub fn decode(bytes: &[u8]) -> Result<RawTraceDocument, IngestionError> {
    serde_json::from_slice(bytes).map_err(|e| IngestionError::MalformedInput {
        reason: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_rejects_invalid_json() {
        let result = decode(b"not json at all");
        assert!(matches!(result, Err(IngestionError::MalformedInput { .. })));
    }

    #[test]
    fn decode_rejects_missing_required_field() {
        // Missing "block" and "root".
        let json = br#"{"transaction": {"hash": "0x00", "from": "0x00", "to": null, "value": "0x0", "nonce": "0x0", "gasUsed": "0x0", "status": "success"}}"#;
        let result = decode(json);
        assert!(matches!(result, Err(IngestionError::MalformedInput { .. })));
    }

    #[test]
    fn decode_rejects_unknown_field() {
        let json = br#"{
            "transaction": {"hash": "0x00", "from": "0x00", "to": null, "value": "0x0", "nonce": "0x0", "gasUsed": "0x0", "status": "success", "unexpectedField": true},
            "block": {"number": "0x1", "timestamp": "0x1", "chainId": "0x1", "baseFee": null},
            "root": {"kind": "call", "from": "0x00", "to": null, "value": "0x0", "gasLimit": "0x0", "gasUsed": "0x0", "succeeded": true}
        }"#;
        let result = decode(json);
        assert!(matches!(result, Err(IngestionError::MalformedInput { .. })));
    }

    #[test]
    fn decode_accepts_minimal_valid_document() {
        let json = br#"{
            "transaction": {"hash": "0x00", "from": "0x00", "to": null, "value": "0x0", "nonce": "0x0", "gasUsed": "0x0", "status": "success"},
            "block": {"number": "0x1", "timestamp": "0x1", "chainId": "0x1", "baseFee": null},
            "root": {"kind": "call", "from": "0x00", "to": null, "value": "0x0", "gasLimit": "0x0", "gasUsed": "0x0", "succeeded": true}
        }"#;
        let result = decode(json);
        assert!(result.is_ok());
    }
}

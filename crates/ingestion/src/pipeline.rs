//! The top-level ingestion pipeline: [`TraceSource`] → bytes → decode →
//! normalize → build → [`fact_model::Trace`].

use fact_model::{Trace, TraceSource as FactTraceSource};

use crate::build;
use crate::decode::decode;
use crate::error::IngestionError;
use crate::normalize::normalize;
use crate::source::TraceSource;

/// Run the complete ingestion pipeline.
///
/// `source` supplies the raw bytes (see [`TraceSource`]); `provenance` is
/// the [`fact_model::TraceSource`] metadata to record on the resulting
/// [`Trace`]. These are kept as two separate parameters rather than one
/// derived from the other because they answer two different questions —
/// "where did these bytes physically come from" (a file, memory, a
/// future RPC client) versus "what should the fact model itself record
/// as this trace's provenance" (currently only
/// `fact_model::TraceSource::ArchiveNodeRpc`, per Assumption A-5) — and
/// conflating them would mean labeling a test fixture loaded from a file
/// as if it came from archive-node RPC, which would be false.
///
/// # Errors
/// Returns the first [`IngestionError`] encountered from any stage:
/// loading (`SourceUnavailable`), decoding (`MalformedInput`),
/// normalizing (`MalformedField`, `UnsupportedFeature`,
/// `InvalidStorageOwnership`, `DuplicateLogIndex`, ...), or constructing
/// the fact model (`ArenaConstruction`, `TraceConstruction`).
pub fn ingest(
    source: &dyn TraceSource,
    provenance: FactTraceSource,
) -> Result<Trace, IngestionError> {
    let bytes = source.load()?;
    let raw = decode(&bytes)?;
    let normalized = normalize(&raw)?;
    build::build(&normalized, provenance)
}

#[cfg(test)]
mod tests {
    use fact_model::TraceSource as FactTraceSource;

    use super::*;
    use crate::source::InMemoryTraceSource;

    fn minimal_json() -> Vec<u8> {
        format!(
            r#"{{
            "transaction": {{"hash": "0x{}", "from": "0x0101010101010101010101010101010101010101", "to": "0x0202020202020202020202020202020202020202", "value": "0x0", "nonce": "0x0", "gasUsed": "0x5208", "status": "success"}},
            "block": {{"number": "0x64", "timestamp": "0x1", "chainId": "0x1", "baseFee": null}},
            "root": {{"kind": "call", "from": "0x0101010101010101010101010101010101010101", "to": "0x0202020202020202020202020202020202020202", "value": "0x0", "gasLimit": "0x5208", "gasUsed": "0x5208", "succeeded": true}}
        }}"#,
            "11".repeat(32)
        )
        .into_bytes()
    }

    #[test]
    fn full_pipeline_produces_valid_trace() {
        let source = InMemoryTraceSource::new(minimal_json());
        let provenance = FactTraceSource::ArchiveNodeRpc {
            endpoint_label: "test-fixture".to_string(),
        };
        let trace = ingest(&source, provenance).unwrap();
        assert_eq!(trace.arena.calls().count(), 1);
    }

    #[test]
    fn pipeline_propagates_source_error() {
        struct FailingSource;
        impl TraceSource for FailingSource {
            fn load(&self) -> Result<Vec<u8>, IngestionError> {
                Err(IngestionError::SourceUnavailable {
                    reason: "simulated failure".to_string(),
                })
            }
        }
        let provenance = FactTraceSource::ArchiveNodeRpc {
            endpoint_label: "test".to_string(),
        };
        let result = ingest(&FailingSource, provenance);
        assert!(matches!(
            result,
            Err(IngestionError::SourceUnavailable { .. })
        ));
    }

    #[test]
    fn pipeline_propagates_decode_error() {
        let source = InMemoryTraceSource::new(b"not json".to_vec());
        let provenance = FactTraceSource::ArchiveNodeRpc {
            endpoint_label: "test".to_string(),
        };
        let result = ingest(&source, provenance);
        assert!(matches!(result, Err(IngestionError::MalformedInput { .. })));
    }

    #[test]
    fn pipeline_propagates_normalization_error() {
        let json = format!(
            r#"{{
            "transaction": {{"hash": "0x{}", "from": "0x0101010101010101010101010101010101010101", "to": null, "value": "0x0", "nonce": "0x0", "gasUsed": "0x1", "status": "success"}},
            "block": {{"number": "0x1", "timestamp": "0x1", "chainId": "0x1", "baseFee": null}},
            "root": {{"kind": "not-a-real-kind", "from": "0x0101010101010101010101010101010101010101", "to": null, "value": "0x0", "gasLimit": "0x1", "gasUsed": "0x1", "succeeded": true}}
        }}"#,
            "11".repeat(32)
        )
        .into_bytes();
        let source = InMemoryTraceSource::new(json);
        let provenance = FactTraceSource::ArchiveNodeRpc {
            endpoint_label: "test".to_string(),
        };
        let result = ingest(&source, provenance);
        assert!(matches!(
            result,
            Err(IngestionError::UnsupportedFeature { .. })
        ));
    }
}

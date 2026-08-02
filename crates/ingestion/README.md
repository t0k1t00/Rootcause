# ingestion

Converts raw archive-node-style transaction traces into the canonical
`fact-model` `Trace` representation.

This crate performs **only** loading, parsing, validation, normalization,
and fact-model construction — no exploit analysis, no pattern matching,
and no grounding. Those are `matcher`'s and `grounding`'s responsibilities.

## Pipeline

```text
TraceSource::load()  →  decode::decode()  →  normalize::normalize()  →  build::build()  →  fact_model::Trace
     (Source)              (Decode)          (Validate+Normalize)        (Construct)
```

`pipeline::ingest` runs all four stages in one call. See
[ADR-0007](../../docs/adr/0007-ingestion-source-and-schema.md) for why the
source abstraction and raw wire schema are designed the way they are — in
particular, why a real archive-node RPC client is a deferred follow-on
rather than built here, and why the raw schema nests storage changes and
logs inside call nodes rather than mirroring a raw provider response
literally.

## Module map

| Module | Contents |
|---|---|
| `source` | `TraceSource`, the pluggable "where do raw bytes come from" trait, plus two network-free implementations (`InMemoryTraceSource`, `FileTraceSource`). |
| `raw` | The raw, wire-shaped JSON schema (`RawTraceDocument`) this crate decodes. |
| `decode` | Bytes → `RawTraceDocument`. |
| `hex_util` | Shared hex-parsing helpers used by `normalize`. |
| `normalize` | `RawTraceDocument` → `NormalizedTrace`: validation and type conversion. |
| `build` | `NormalizedTrace` → `fact_model::Trace`. |
| `pipeline` | `pipeline::ingest`, the single entry point running all four stages. |
| `error` | `IngestionError`, the crate's single exhaustive error type. |

## Usage

```rust
use fact_model::TraceSource as FactTraceSource;
use ingestion::pipeline::ingest;
use ingestion::source::FileTraceSource;

let source = FileTraceSource::new("trace.json");
let provenance = FactTraceSource::ArchiveNodeRpc {
    endpoint_label: "local-fixture".to_string(),
};
let trace = ingest(&source, provenance)?;
```

## Status

Complete. See crate-level rustdoc (`cargo doc -p ingestion --open`) for
the full API reference, and `crate::raw::RawTraceDocument`'s own docs for
the wire schema `decode` expects.

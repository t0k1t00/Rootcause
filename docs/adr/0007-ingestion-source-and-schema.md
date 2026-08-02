# 0007. Ingestion source abstraction and raw wire schema

Status: Accepted

## Context

The Architecture document names archive-node RPC as the only ingestion
source evidenced by the canonical specification (Assumption A-5), but
explicitly states "the Review does not specify the mechanism." Two gaps
follow directly from that: (1) no document specifies a concrete wire
format for what an archive-node RPC response looks like, and (2) a real
RPC client requires network I/O, which requires an async HTTP dependency
(`tokio`/`reqwest` or similar) not currently in the workspace and not
authorized by this task. Both are delegated decisions: no source document
specifies them, and reasonable choices for either do not change any
downstream crate's externally visible behavior (`matcher`/`grounding`
consume a `fact_model::Trace` regardless of how it was produced).

## Decision

**Source abstraction.** `ingestion` defines a `TraceSource` trait
(`fn load(&self) -> Result<Vec<u8>, IngestionError>`) that any concrete
source — file, in-memory fixture, or a future real RPC client —
implements. This task ships two network-free implementations
(`InMemoryTraceSource`, `FileTraceSource`) sufficient to prove the full
decode → normalize → build pipeline end-to-end against realistic fixture
data. A real `ArchiveRpcTraceSource` implementing the same trait, backed
by an actual HTTP client, is deferred to a follow-on task that will add
the necessary dependency; no pipeline code changes when that happens —
this is precisely what the trait boundary is for.

**Raw wire schema ("Root Cause Combined Trace Format v1").** Rather than
inventing an arbitrary ad hoc JSON shape, the raw schema in
`ingestion::raw` mirrors two real, standard Geth debug-tracer output
shapes (`callTracer`'s nested call tree; `prestateTracer`'s diff-mode
storage representation) but is not identical to either, because neither
carries call-level attribution for storage writes or logs — which is
exactly the "storage-slot-to-variable resolution mechanism" Assumption
A-5 says is unspecified. This schema resolves attribution by definition:
storage changes and logs are nested inside the call node that produced
them, as a combined/custom tracer would emit them. This is a concrete,
documented interchange format `ingestion` expects to receive, not a
literal reproduction of any specific archive-node provider's API
response — a thin adapter converting a real provider's actual response
into this shape is left for the follow-on RPC-client task.

## Consequences

- `ingestion`'s pipeline (decode → normalize → build) is fully testable
  today against realistic, hand-authored fixtures without any network
  dependency or live archive-node access.
- Adding a real archive-node RPC source later requires only a new
  `TraceSource` implementation plus an adapter from the provider's actual
  response shape to `raw::RawTraceDocument` — no change to `decode.rs`,
  `normalize.rs`, or `build.rs`.
- If per-call storage/log attribution ever needs to come from a real
  provider that cannot supply it directly (e.g. a plain `prestateTracer`
  with no call-scoping), that adapter will need its own attribution
  heuristic — this ADR does not solve that; it only defines the shape
  `ingestion` consumes once attribution is available.

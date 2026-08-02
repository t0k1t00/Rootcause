//! Converters from real Ethereum tooling trace formats into
//! `ingestion::raw::RawTraceDocument`, Root Cause's internal raw
//! schema. Each submodule documents exactly which fields of its source
//! format map where, and — just as importantly — which fields cannot
//! currently be represented and why. See each submodule's own docs for
//! the schema it was studied and verified against.
//!
//! Deliberately out of scope for every converter here: network I/O.
//! A converter takes bytes/JSON already retrieved by some other means
//! (a saved RPC response, a CLI-piped file) and produces a
//! `RawTraceDocument`; it does not itself call an RPC endpoint. This
//! mirrors `ingestion::source::TraceSource`'s own separation of
//! "where bytes come from" from "how they're interpreted".

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod geth;

/// The crate's own semantic version.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

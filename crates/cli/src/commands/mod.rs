//! Every `rootcause` subcommand's orchestration logic.
//!
//! Each submodule wires together already-implemented crates
//! (`ingestion`, `dsl`, `matcher`, `grounding`, `taxonomy`,
//! `benchmark-harness`) for one command; none re-implements pipeline
//! logic that belongs in one of those crates.

pub mod analyze;
pub mod benchmark;
pub mod doctor;
pub mod format_pattern;
pub mod new_pattern;
pub mod validate_pattern;

/// Version information for `rootcause` itself and every engine crate it
/// links — the `rootcause version` command's output, and also printed
/// by `rootcause` when run with no subcommand (see [`crate::run`]).
#[must_use]
pub fn version_info() -> String {
    format!(
        "rootcause {}\n  fact-model:        {}\n  ingestion:         {}\n  dsl:               {}\n  matcher:           {}\n  grounding:         {}\n  taxonomy:          {}\n  benchmark-harness: {}",
        env!("CARGO_PKG_VERSION"),
        fact_model::CRATE_VERSION,
        ingestion::CRATE_VERSION,
        dsl::CRATE_VERSION,
        matcher::CRATE_VERSION,
        grounding::CRATE_VERSION,
        taxonomy::CRATE_VERSION,
        benchmark_harness::CRATE_VERSION,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_info_includes_every_crate() {
        let info = version_info();
        assert!(info.contains("rootcause"));
        assert!(info.contains("fact-model"));
        assert!(info.contains("ingestion"));
        assert!(info.contains("dsl"));
        assert!(info.contains("matcher"));
        assert!(info.contains("grounding"));
        assert!(info.contains("taxonomy"));
        assert!(info.contains("benchmark-harness"));
    }
}

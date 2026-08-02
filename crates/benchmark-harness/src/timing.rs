//! Per-stage timing: how long each pipeline phase took for one case.

use std::time::Duration;

/// Wall-clock duration spent in each pipeline stage while running one
/// [`crate::types::BenchmarkCase`].
///
/// Durations, not [`std::time::Instant`]s: this type is meant to be
/// aggregated (summed, averaged) across many cases by
/// [`crate::metrics`], for which owning plain [`Duration`]s is simpler
/// than re-deriving them from timestamps later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StageTimings {
    /// Time spent ingesting the case's trace (raw bytes → `fact_model::Trace`).
    /// Always [`Duration::ZERO`] for a case built with
    /// [`crate::types::TraceInput::Prebuilt`] — see that variant's own
    /// docs.
    pub ingestion: Duration,
    /// Time spent structurally matching every pattern against the
    /// trace (summed across all of the case's patterns).
    pub matching: Duration,
    /// Time spent independently grounding every candidate match (summed
    /// across all of the case's patterns).
    pub grounding: Duration,
    /// Time spent mapping every grounded/abstained/ungrounded result to
    /// external taxonomies (summed across all of the case's patterns).
    pub taxonomy: Duration,
}

impl StageTimings {
    /// The sum of every stage's duration.
    #[must_use]
    pub fn total(&self) -> Duration {
        self.ingestion + self.matching + self.grounding + self.taxonomy
    }

    /// Add another case's timings into this running total — used to
    /// aggregate per-case timings into a suite-wide total.
    pub fn accumulate(&mut self, other: &Self) {
        self.ingestion += other.ingestion;
        self.matching += other.matching;
        self.grounding += other.grounding;
        self.taxonomy += other.taxonomy;
    }
}

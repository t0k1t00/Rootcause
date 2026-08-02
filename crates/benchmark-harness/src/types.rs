//! Benchmark cases and suites: the inputs this crate's runner consumes.

use std::fmt;

use dsl::ir::{CompiledPattern, PatternId};
use fact_model::{Trace, TraceSource as FactTraceSource};
use ingestion::TraceSource;

/// A benchmark case's identifier, unique within its containing
/// [`BenchmarkSuite`].
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct CaseId(pub String);

impl fmt::Display for CaseId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for CaseId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for CaseId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// The outcome one pattern reaches against one trace, coarsened to the
/// four buckets a benchmark case can assert an expectation about.
///
/// This deliberately does not reuse [`grounding::GroundingStatus`]
/// directly: that type has no variant for "the pattern produced no
/// candidate matches at all" (matcher's absence-of-a-match is a fact
/// about candidate generation, upstream of grounding entirely), and a
/// benchmark fixture legitimately wants to assert exactly that (the
/// "no-match case" fixture family this crate's tests are required to
/// cover). [`PatternOutcome::NoMatch`] fills that gap; the other three
/// variants mirror [`grounding::GroundingStatus`] one-for-one.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum PatternOutcome {
    /// The pattern produced at least one candidate that grounded.
    Grounded,
    /// The pattern produced candidates, and the strongest outcome among
    /// them was an abstention.
    Abstain,
    /// The pattern produced candidates, and every one of them was
    /// independently refuted (ungrounded), none abstained.
    Ungrounded,
    /// The pattern produced no candidate matches against the trace at
    /// all.
    NoMatch,
}

impl PatternOutcome {
    /// Coarsen a [`grounding::GroundingStatus`] into a
    /// [`PatternOutcome`]. Used when at least one candidate exists;
    /// callers handle the "zero candidates" case ([`Self::NoMatch`])
    /// separately, since [`grounding::GroundingStatus`] has no
    /// corresponding variant.
    #[must_use]
    pub const fn from_grounding_status(status: grounding::GroundingStatus) -> Self {
        match status {
            grounding::GroundingStatus::Grounded => Self::Grounded,
            grounding::GroundingStatus::Abstain => Self::Abstain,
            grounding::GroundingStatus::Ungrounded => Self::Ungrounded,
        }
    }

    /// The strongest of two outcomes, using the precedence
    /// `Grounded > Abstain > Ungrounded` — the same precedence
    /// `grounding::engine` uses internally for "uncertainty always
    /// outranks a confident negative," extended one step further so a
    /// trace with several candidates for the same pattern is
    /// summarized by its single most conclusive result. [`Self::NoMatch`]
    /// never appears as an operand here: it is only ever assigned when
    /// there are zero candidates, in which case there is nothing to
    /// combine.
    #[must_use]
    pub const fn strongest(self, other: Self) -> Self {
        match (self, other) {
            (Self::Grounded, _) | (_, Self::Grounded) => Self::Grounded,
            (Self::Abstain, _) | (_, Self::Abstain) => Self::Abstain,
            (Self::Ungrounded, Self::Ungrounded) => Self::Ungrounded,
            (Self::NoMatch, other) => other,
            (this, Self::NoMatch) => this,
        }
    }
}

impl fmt::Display for PatternOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Grounded => "grounded",
            Self::Abstain => "abstain",
            Self::Ungrounded => "ungrounded",
            Self::NoMatch => "no-match",
        })
    }
}

/// One case's expectation for a single pattern: "pattern `P` should
/// reach outcome `O` against this case's trace."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedFinding {
    /// The pattern this expectation concerns.
    pub pattern_id: PatternId,
    /// The outcome the case's author asserts that pattern should reach.
    pub expected: PatternOutcome,
}

impl ExpectedFinding {
    /// Construct an expectation.
    #[must_use]
    pub fn new(pattern_id: impl Into<String>, expected: PatternOutcome) -> Self {
        Self {
            pattern_id: PatternId(pattern_id.into()),
            expected,
        }
    }
}

/// Where a [`BenchmarkCase`]'s trace comes from.
///
/// Kept as two variants rather than always re-ingesting from bytes:
/// [`Self::FromSource`] is the realistic path for a fixture file or a
/// JSON benchmark definition (see [`crate::fixtures`]), and is timed as
/// part of [`crate::runner::run_case`]'s ingestion stage on every run.
/// [`Self::Prebuilt`] exists for callers (chiefly this crate's own
/// tests) that already hold a [`fact_model::Trace`] built directly via
/// `fact-model`'s arena builder and have no raw bytes to re-ingest —
/// its ingestion stage timing is always reported as zero, which
/// [`crate::report::BenchmarkReport`] callers should treat as "not
/// applicable" rather than "instantaneous."
pub enum TraceInput {
    /// Re-ingest from a [`TraceSource`] on every run.
    FromSource {
        /// Supplies the raw trace bytes.
        source: Box<dyn TraceSource>,
        /// The provenance label recorded on the resulting
        /// [`fact_model::Trace`] (see [`fact_model::TraceSource::ArchiveNodeRpc`]).
        provenance_label: String,
    },
    /// A trace already built by the caller.
    Prebuilt(Box<Trace>),
}

impl fmt::Debug for TraceInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FromSource {
                provenance_label, ..
            } => f
                .debug_struct("FromSource")
                .field("provenance_label", provenance_label)
                .finish(),
            Self::Prebuilt(_) => f.debug_tuple("Prebuilt").finish(),
        }
    }
}

/// One benchmark case: a trace, the pattern(s) to run against it, and
/// what the case's author expects each pattern to find.
#[derive(Debug)]
pub struct BenchmarkCase {
    /// This case's unique (within its suite) identifier.
    pub id: CaseId,
    /// A human-readable description of what this case exercises.
    pub description: String,
    /// Where this case's trace comes from.
    pub trace_input: TraceInput,
    /// Every pattern this case runs against its trace.
    pub patterns: Vec<CompiledPattern>,
    /// What this case's author expects each pattern (by id) to reach.
    /// A pattern with no entry here is run and reported on but not
    /// scored against an expectation.
    pub expected: Vec<ExpectedFinding>,
}

impl BenchmarkCase {
    /// Build a case whose trace is ingested fresh (and timed) on every
    /// run, from an in-memory byte buffer.
    #[must_use]
    pub fn from_bytes(
        id: impl Into<CaseId>,
        description: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
        provenance_label: impl Into<String>,
        patterns: Vec<CompiledPattern>,
        expected: Vec<ExpectedFinding>,
    ) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            trace_input: TraceInput::FromSource {
                source: Box::new(ingestion::InMemoryTraceSource::new(bytes)),
                provenance_label: provenance_label.into(),
            },
            patterns,
            expected,
        }
    }

    /// Build a case whose trace is ingested fresh (and timed) on every
    /// run, from a file on disk.
    #[must_use]
    pub fn from_file(
        id: impl Into<CaseId>,
        description: impl Into<String>,
        path: impl AsRef<std::path::Path>,
        provenance_label: impl Into<String>,
        patterns: Vec<CompiledPattern>,
        expected: Vec<ExpectedFinding>,
    ) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            trace_input: TraceInput::FromSource {
                source: Box::new(ingestion::FileTraceSource::new(path)),
                provenance_label: provenance_label.into(),
            },
            patterns,
            expected,
        }
    }

    /// Build a case from an already-constructed [`fact_model::Trace`],
    /// skipping ingestion entirely (see [`TraceInput::Prebuilt`]).
    #[must_use]
    pub fn from_trace(
        id: impl Into<CaseId>,
        description: impl Into<String>,
        trace: Trace,
        patterns: Vec<CompiledPattern>,
        expected: Vec<ExpectedFinding>,
    ) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            trace_input: TraceInput::Prebuilt(Box::new(trace)),
            patterns,
            expected,
        }
    }

    /// The default provenance recorded for a case built from raw bytes:
    /// an archive-node RPC label naming this case.
    #[must_use]
    pub(crate) fn provenance(&self) -> FactTraceSource {
        match &self.trace_input {
            TraceInput::FromSource {
                provenance_label, ..
            } => FactTraceSource::ArchiveNodeRpc {
                endpoint_label: provenance_label.clone(),
            },
            TraceInput::Prebuilt(_) => FactTraceSource::ArchiveNodeRpc {
                endpoint_label: "prebuilt-fixture".to_string(),
            },
        }
    }
}

/// A named collection of [`BenchmarkCase`]s run and reported on
/// together.
#[derive(Debug, Default)]
pub struct BenchmarkSuite {
    /// The suite's name, used as [`crate::report::BenchmarkReport::suite_name`].
    pub name: String,
    /// Every case in the suite, in run order.
    pub cases: Vec<BenchmarkCase>,
}

impl BenchmarkSuite {
    /// An empty suite named `name`.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            cases: Vec::new(),
        }
    }

    /// Append a case to this suite.
    pub fn push(&mut self, case: BenchmarkCase) {
        self.cases.push(case);
    }

    /// Build a suite from a name and a vec of cases.
    #[must_use]
    pub fn from_cases(name: impl Into<String>, cases: Vec<BenchmarkCase>) -> Self {
        Self {
            name: name.into(),
            cases,
        }
    }

    /// Validate that every case id in this suite is unique.
    ///
    /// # Errors
    /// Returns [`crate::error::BenchmarkError::DuplicateCaseId`] naming
    /// the first duplicate found.
    pub fn validate(&self) -> Result<(), crate::error::BenchmarkError> {
        let mut seen = std::collections::HashSet::new();
        for case in &self.cases {
            if !seen.insert(&case.id) {
                return Err(crate::error::BenchmarkError::DuplicateCaseId {
                    suite_name: self.name.clone(),
                    case_id: case.id.0.clone(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_outcome_strongest_precedence() {
        assert_eq!(
            PatternOutcome::Grounded.strongest(PatternOutcome::Abstain),
            PatternOutcome::Grounded
        );
        assert_eq!(
            PatternOutcome::Abstain.strongest(PatternOutcome::Ungrounded),
            PatternOutcome::Abstain
        );
        assert_eq!(
            PatternOutcome::Ungrounded.strongest(PatternOutcome::Ungrounded),
            PatternOutcome::Ungrounded
        );
        assert_eq!(
            PatternOutcome::NoMatch.strongest(PatternOutcome::Ungrounded),
            PatternOutcome::Ungrounded
        );
    }

    #[test]
    fn pattern_outcome_display_matches_expected_words() {
        assert_eq!(PatternOutcome::Grounded.to_string(), "grounded");
        assert_eq!(PatternOutcome::Abstain.to_string(), "abstain");
        assert_eq!(PatternOutcome::Ungrounded.to_string(), "ungrounded");
        assert_eq!(PatternOutcome::NoMatch.to_string(), "no-match");
    }

    #[test]
    fn pattern_outcome_json_roundtrip() {
        for outcome in [
            PatternOutcome::Grounded,
            PatternOutcome::Abstain,
            PatternOutcome::Ungrounded,
            PatternOutcome::NoMatch,
        ] {
            let json = serde_json::to_string(&outcome).expect("serializable");
            let back: PatternOutcome = serde_json::from_str(&json).expect("deserializable");
            assert_eq!(outcome, back);
        }
    }

    #[test]
    fn pattern_outcome_json_uses_kebab_case() {
        let json = serde_json::to_string(&PatternOutcome::NoMatch).expect("serializable");
        assert_eq!(json, "\"no-match\"");
    }

    #[test]
    fn suite_validate_detects_duplicate_case_ids() {
        let case_a = BenchmarkCase::from_bytes(
            "dup",
            "first",
            b"{}".to_vec(),
            "test",
            Vec::new(),
            Vec::new(),
        );
        let case_b = BenchmarkCase::from_bytes(
            "dup",
            "second",
            b"{}".to_vec(),
            "test",
            Vec::new(),
            Vec::new(),
        );
        let suite = BenchmarkSuite::from_cases("dup-suite", vec![case_a, case_b]);
        let err = suite.validate().unwrap_err();
        assert!(matches!(
            err,
            crate::error::BenchmarkError::DuplicateCaseId { .. }
        ));
    }

    #[test]
    fn suite_validate_accepts_unique_case_ids() {
        let case_a =
            BenchmarkCase::from_bytes("a", "first", b"{}".to_vec(), "test", Vec::new(), Vec::new());
        let case_b = BenchmarkCase::from_bytes(
            "b",
            "second",
            b"{}".to_vec(),
            "test",
            Vec::new(),
            Vec::new(),
        );
        let suite = BenchmarkSuite::from_cases("ok-suite", vec![case_a, case_b]);
        assert!(suite.validate().is_ok());
    }
}

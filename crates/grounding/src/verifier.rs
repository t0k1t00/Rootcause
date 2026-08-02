//! Verifier architecture: one [`Verifier`] per predicate kind, dispatched
//! through a [`VerifierRegistry`] so a future predicate kind can be
//! supported by registering a new [`Verifier`] without touching any
//! existing one — the "future predicate types should be addable without
//! changing existing verifier code" requirement.
//!
//! # Shared verification logic
//!
//! Every built-in [`Verifier`] (see [`crate::verifiers`]) delegates its
//! actual checking to [`verify_via_matcher_predicate`] in this module,
//! parameterized only by which predicate kind to ask `matcher` about.
//! This is a deliberate choice, not an accident of the five kinds
//! happening to be similar: **`matcher::predicate` is the single source
//! of truth for what a predicate structurally means** (which facts of
//! which kind, filtered by which attributes, count as a match — see
//! that module's own extensive "Predicate semantics" docs). Grounding's
//! job is not to redefine that meaning a second time — doing so would
//! let the two crates' notions of "the `call` predicate" drift apart,
//! which is a far worse failure mode than the two crates sharing logic.
//! Grounding's real, distinct contribution — verified here, not in
//! `matcher` — is *independence from the candidate's own claims*:
//! [`verify_via_matcher_predicate`] recomputes the match set fresh from
//! `trace`/`index` every time, and separately checks whether the
//! specific fact a [`matcher::CandidateMatch`] cited is actually a
//! member of that freshly-computed set, rather than trusting the
//! candidate's binding at face value. A future predicate kind whose
//! *meaning* genuinely differs enough to need bespoke verification logic
//! (not just "which matcher function to call") can still implement
//! [`Verifier`] directly instead of going through the shared helper —
//! the trait does not require using it.

use fact_model::{FactRef, Trace};

use dsl::ir::CompiledPredicate;

use crate::polarity::Polarity;
use crate::status::EvidenceOutcome;

/// The result of independently verifying one evidence clause's
/// predicate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierOutcome {
    /// The independently-determined outcome.
    pub outcome: EvidenceOutcome,
    /// The fact grounding cites for this outcome: the candidate's own
    /// cited fact if independent re-verification confirmed it, or a
    /// fact grounding discovered itself if the candidate cited none.
    /// `None` for [`EvidenceOutcome::Unavailable`] (nothing to cite) and
    /// for a confirmed-absent [`EvidenceOutcome::Verified`] on a
    /// negatively-referenced clause (there is no fact to cite for an
    /// absence — see `matcher::CandidateMatch`'s own docs for the same
    /// principle).
    pub fact: Option<FactRef>,
    /// Attribute names this predicate declared that could not be
    /// structurally checked (see [`crate::verifiers`]'s module docs).
    pub unresolved_attrs: Vec<&'static str>,
    /// A human-readable explanation of the outcome, always present
    /// (never an empty string) so every entry in a
    /// [`crate::report::GroundingResult`]'s evidence chain is
    /// self-explanatory without cross-referencing other fields.
    pub reason: String,
}

/// A verifier for one predicate kind.
///
/// Implementations are looked up by [`Self::kind`] through a
/// [`VerifierRegistry`]; see this module's own docs for why every
/// built-in implementation delegates to
/// [`verify_via_matcher_predicate`] rather than re-implementing
/// predicate semantics.
pub trait Verifier: Send + Sync {
    /// The predicate kind this verifier handles (e.g. `"call"`),
    /// matching [`dsl::schema::PredicateSchema::kind`] for the kind it
    /// covers.
    fn kind(&self) -> &'static str;

    /// Independently verify `predicate` against `trace`, given
    /// `polarity` (whether the pattern's constraint requires this
    /// clause's predicate to hold or to not hold) and `cited_fact` (the
    /// specific fact the [`matcher::CandidateMatch`] bound to this
    /// clause, if any).
    fn verify(
        &self,
        trace: &Trace,
        index: &matcher::TraceIndex<'_>,
        predicate: &CompiledPredicate,
        polarity: Polarity,
        cited_fact: Option<FactRef>,
    ) -> VerifierOutcome;
}

/// Shared verification logic every built-in [`Verifier`] delegates to —
/// see this module's own top-level docs for why this is the right
/// amount of sharing.
///
/// # Algorithm
/// 1. Ask `matcher::predicate::evaluate` for every fact that
///    structurally satisfies `predicate` right now, freshly computed
///    from `trace`/`index` (never from the candidate).
/// 2. If that set is empty: [`EvidenceOutcome::Unavailable`] for a
///    [`Polarity::Positive`]/[`Polarity::Unreferenced`] clause (nothing
///    to ground), or [`EvidenceOutcome::Verified`] for
///    [`Polarity::Negative`] (absence confirmed — and confirmed
///    regardless of any unresolved attribute, since `matcher` already
///    treats unresolved attributes maximally permissively: if *nothing*
///    matches even under that permissive treatment, nothing could
///    possibly have matched under the true, unknown semantics either).
/// 3. If the set is non-empty and the predicate names an unresolved
///    attribute (see `matcher::predicate`'s docs — `storage.role`,
///    `token_transfer.unexpected`): [`EvidenceOutcome::Unsupported`],
///    for *either* polarity — a non-empty permissive match set cannot
///    confirm presence (some members might not really qualify) and
///    cannot confirm absence either (the same uncertainty).
/// 4. Otherwise (non-empty, fully checkable):
///    - [`Polarity::Negative`]: [`EvidenceOutcome::Failed`] — the
///      forbidden condition was found present.
///    - [`Polarity::Positive`]/[`Polarity::Unreferenced`]: if
///      `cited_fact` is `Some` and appears in the match set,
///      [`EvidenceOutcome::Verified`] citing it; if `cited_fact` is
///      `Some` and does *not* appear, [`EvidenceOutcome::Failed`] (the
///      candidate's own claim does not hold up); if `cited_fact` is
///      `None`, [`EvidenceOutcome::Verified`] citing grounding's own
///      first (deterministically ordered) discovered fact.
#[must_use]
pub fn verify_via_matcher_predicate(
    trace: &Trace,
    index: &matcher::TraceIndex<'_>,
    predicate: &CompiledPredicate,
    polarity: Polarity,
    cited_fact: Option<FactRef>,
) -> VerifierOutcome {
    let bindings = matcher::predicate::evaluate(trace, index, predicate);
    let unresolved: Vec<&'static str> = bindings
        .first()
        .map(|b| b.unresolved_attrs.clone())
        .unwrap_or_default();

    if bindings.is_empty() {
        return if matches!(polarity, Polarity::Negative) {
            VerifierOutcome {
                outcome: EvidenceOutcome::Verified,
                fact: None,
                unresolved_attrs: Vec::new(),
                reason: "no fact in the trace satisfies this predicate, confirming the \
                         required absence"
                    .to_string(),
            }
        } else {
            VerifierOutcome {
                outcome: EvidenceOutcome::Unavailable,
                fact: None,
                unresolved_attrs: Vec::new(),
                reason: "no fact in the trace satisfies this predicate".to_string(),
            }
        };
    }

    if !unresolved.is_empty() {
        return VerifierOutcome {
            outcome: EvidenceOutcome::Unsupported,
            fact: None,
            unresolved_attrs: unresolved.clone(),
            reason: format!(
                "{} matching fact(s) exist, but attribute(s) {} cannot be structurally \
                 verified against fact-model data, so neither presence nor absence can be \
                 confirmed with confidence",
                bindings.len(),
                unresolved.join(", "),
            ),
        };
    }

    if matches!(polarity, Polarity::Negative) {
        return VerifierOutcome {
            outcome: EvidenceOutcome::Failed,
            fact: bindings.first().map(|b| b.fact),
            unresolved_attrs: Vec::new(),
            reason: format!(
                "{} fact(s) satisfy this predicate, contradicting the pattern's requirement \
                 that it be absent",
                bindings.len()
            ),
        };
    }

    match cited_fact {
        Some(cited) if bindings.iter().any(|b| b.fact == cited) => VerifierOutcome {
            outcome: EvidenceOutcome::Verified,
            fact: Some(cited),
            unresolved_attrs: Vec::new(),
            reason: "the candidate's cited fact was independently re-confirmed against the \
                     trace"
                .to_string(),
        },
        Some(cited) => VerifierOutcome {
            outcome: EvidenceOutcome::Failed,
            fact: Some(cited),
            unresolved_attrs: Vec::new(),
            reason: "the candidate cited a fact that does not actually satisfy this predicate \
                     under independent re-verification"
                .to_string(),
        },
        None => VerifierOutcome {
            outcome: EvidenceOutcome::Verified,
            fact: bindings.first().map(|b| b.fact),
            unresolved_attrs: Vec::new(),
            reason: "the candidate did not cite a fact for this clause; grounding found and \
                     verified its own supporting fact"
                .to_string(),
        },
    }
}

/// An extensible registry of [`Verifier`]s, dispatched by predicate
/// kind.
pub struct VerifierRegistry {
    verifiers: std::collections::HashMap<&'static str, Box<dyn Verifier>>,
}

impl Default for VerifierRegistry {
    fn default() -> Self {
        Self::with_default_verifiers()
    }
}

impl VerifierRegistry {
    /// An empty registry with no verifiers. Registering a verifier for
    /// every predicate kind a pattern library actually uses is the
    /// caller's responsibility with an empty registry — most callers
    /// want [`Self::with_default_verifiers`] instead.
    #[must_use]
    pub fn new() -> Self {
        Self {
            verifiers: std::collections::HashMap::new(),
        }
    }

    /// A registry covering every predicate kind [`dsl::schema::ALL`]
    /// currently defines: `call`, `storage`, `value_flow`,
    /// `token_transfer`, `transaction`, `log` (see
    /// [`crate::verifiers`]).
    #[must_use]
    pub fn with_default_verifiers() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(crate::verifiers::CallVerifier));
        registry.register(Box::new(crate::verifiers::StorageVerifier));
        registry.register(Box::new(crate::verifiers::ValueFlowVerifier));
        registry.register(Box::new(crate::verifiers::TokenTransferVerifier));
        registry.register(Box::new(crate::verifiers::TransactionVerifier));
        registry.register(Box::new(crate::verifiers::LogVerifier));
        registry
    }

    /// Register `verifier`, replacing any existing verifier already
    /// registered for the same [`Verifier::kind`]. This is how a future
    /// predicate kind is supported: implement [`Verifier`], register it
    /// here, and every existing verifier's code is untouched.
    pub fn register(&mut self, verifier: Box<dyn Verifier>) {
        self.verifiers.insert(verifier.kind(), verifier);
    }

    /// Look up the verifier for `kind`, if one is registered.
    #[must_use]
    pub fn get(&self, kind: &str) -> Option<&dyn Verifier> {
        self.verifiers.get(kind).map(std::convert::AsRef::as_ref)
    }

    /// Whether this registry has no verifiers registered at all — used
    /// to distinguish a genuine configuration mistake
    /// ([`crate::GroundingError::UnsupportedVerifier`]) from ordinary
    /// DSL-schema evolution outpacing this crate's verifier coverage
    /// (which degrades to a per-evidence
    /// [`crate::EvidenceOutcome::Unsupported`] instead — see
    /// [`crate::GroundingError::UnsupportedVerifier`]'s own docs).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.verifiers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_covers_every_schema_kind() {
        let registry = VerifierRegistry::with_default_verifiers();
        for schema in dsl::schema::ALL {
            assert!(
                registry.get(schema.kind).is_some(),
                "missing verifier for `{}`",
                schema.kind
            );
        }
    }

    #[test]
    fn empty_registry_has_no_verifiers() {
        let registry = VerifierRegistry::new();
        assert!(registry.is_empty());
        assert!(registry.get("call").is_none());
    }

    #[test]
    fn register_overwrites_existing_kind() {
        let mut registry = VerifierRegistry::new();
        registry.register(Box::new(crate::verifiers::CallVerifier));
        assert!(!registry.is_empty());
        registry.register(Box::new(crate::verifiers::CallVerifier));
        assert_eq!(registry.get("call").map(Verifier::kind), Some("call"));
    }
}

//! One [`Verifier`] implementation per predicate kind
//! [`dsl::schema::ALL`] currently defines. See [`crate::verifier`]'s
//! module docs for why each of these is a thin dispatch to
//! [`verify_via_matcher_predicate`] rather than bespoke logic — the
//! kind-specific knowledge already lives in `matcher::predicate`, and
//! duplicating it here would risk the two crates' notions of a
//! predicate's meaning drifting apart.

use fact_model::{FactRef, Trace};

use dsl::ir::CompiledPredicate;

use crate::polarity::Polarity;
use crate::verifier::{verify_via_matcher_predicate, Verifier, VerifierOutcome};

/// Verifies `call(...)` evidence clauses.
pub struct CallVerifier;

impl Verifier for CallVerifier {
    fn kind(&self) -> &'static str {
        "call"
    }

    fn verify(
        &self,
        trace: &Trace,
        index: &matcher::TraceIndex<'_>,
        predicate: &CompiledPredicate,
        polarity: Polarity,
        cited_fact: Option<FactRef>,
    ) -> VerifierOutcome {
        verify_via_matcher_predicate(trace, index, predicate, polarity, cited_fact)
    }
}

/// Verifies `storage(...)` evidence clauses.
pub struct StorageVerifier;

impl Verifier for StorageVerifier {
    fn kind(&self) -> &'static str {
        "storage"
    }

    fn verify(
        &self,
        trace: &Trace,
        index: &matcher::TraceIndex<'_>,
        predicate: &CompiledPredicate,
        polarity: Polarity,
        cited_fact: Option<FactRef>,
    ) -> VerifierOutcome {
        verify_via_matcher_predicate(trace, index, predicate, polarity, cited_fact)
    }
}

/// Verifies `value_flow(...)` evidence clauses.
pub struct ValueFlowVerifier;

impl Verifier for ValueFlowVerifier {
    fn kind(&self) -> &'static str {
        "value_flow"
    }

    fn verify(
        &self,
        trace: &Trace,
        index: &matcher::TraceIndex<'_>,
        predicate: &CompiledPredicate,
        polarity: Polarity,
        cited_fact: Option<FactRef>,
    ) -> VerifierOutcome {
        verify_via_matcher_predicate(trace, index, predicate, polarity, cited_fact)
    }
}

/// Verifies `token_transfer(...)` evidence clauses.
pub struct TokenTransferVerifier;

impl Verifier for TokenTransferVerifier {
    fn kind(&self) -> &'static str {
        "token_transfer"
    }

    fn verify(
        &self,
        trace: &Trace,
        index: &matcher::TraceIndex<'_>,
        predicate: &CompiledPredicate,
        polarity: Polarity,
        cited_fact: Option<FactRef>,
    ) -> VerifierOutcome {
        verify_via_matcher_predicate(trace, index, predicate, polarity, cited_fact)
    }
}

/// Verifies `transaction(...)` evidence clauses.
pub struct TransactionVerifier;

impl Verifier for TransactionVerifier {
    fn kind(&self) -> &'static str {
        "transaction"
    }

    fn verify(
        &self,
        trace: &Trace,
        index: &matcher::TraceIndex<'_>,
        predicate: &CompiledPredicate,
        polarity: Polarity,
        cited_fact: Option<FactRef>,
    ) -> VerifierOutcome {
        verify_via_matcher_predicate(trace, index, predicate, polarity, cited_fact)
    }
}

/// Verifies `log(...)` evidence clauses.
pub struct LogVerifier;

impl Verifier for LogVerifier {
    fn kind(&self) -> &'static str {
        "log"
    }

    fn verify(
        &self,
        trace: &Trace,
        index: &matcher::TraceIndex<'_>,
        predicate: &CompiledPredicate,
        polarity: Polarity,
        cited_fact: Option<FactRef>,
    ) -> VerifierOutcome {
        verify_via_matcher_predicate(trace, index, predicate, polarity, cited_fact)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::EvidenceOutcome;
    use dsl::ir::CompiledAttr;
    use fact_model::{
        Address, BlockContext, BlockNumber, Call, CallDepth, CallKind, ChainId, FactArenaBuilder,
        Gas, Nonce, Timestamp, TraceMetadata, TraceSource, Transaction, TxHash, TxStatus, Wei,
        Word,
    };

    fn addr(byte: u8) -> Address {
        Address::new([byte; 20])
    }

    fn hash(byte: u8) -> TxHash {
        TxHash(Word::new([byte; 32]))
    }

    fn build_trace(arena: fact_model::FactArena, from: Address) -> Trace {
        let tx = Transaction::new(
            hash(1),
            from,
            Some(addr(2)),
            Wei::ZERO,
            Nonce(0),
            Gas(1),
            TxStatus::Success,
        );
        let block = BlockContext::new(BlockNumber(1), Timestamp(1), ChainId(1), None);
        let metadata = TraceMetadata::new(
            hash(1),
            ChainId(1),
            BlockNumber(1),
            TraceSource::ArchiveNodeRpc {
                endpoint_label: "test".to_string(),
            },
        );
        Trace::new(metadata, block, tx, arena).unwrap()
    }

    fn call_predicate(kind: &str) -> CompiledPredicate {
        CompiledPredicate {
            kind: "call".to_string(),
            attributes: vec![CompiledAttr {
                name: "kind".to_string(),
                value: dsl::ir::AttrValue::Ident(kind.to_string()),
            }],
        }
    }

    #[test]
    fn call_verifier_confirms_matching_fact() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = Call::new(
            builder.next_call_id(),
            None,
            CallKind::Call,
            CallDepth(0),
            attacker,
            Some(addr(2)),
            Wei::ZERO,
            Gas(1),
            Gas(1),
            true,
        )
        .unwrap();
        let root_id = builder.add_call(root);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, attacker);
        let index = matcher::TraceIndex::build(&trace);

        let predicate = call_predicate("External");
        let outcome = CallVerifier.verify(
            &trace,
            &index,
            &predicate,
            Polarity::Positive,
            Some(FactRef::Call(root_id)),
        );
        assert_eq!(outcome.outcome, EvidenceOutcome::Verified);
        assert_eq!(outcome.fact, Some(FactRef::Call(root_id)));
    }

    #[test]
    fn call_verifier_flags_missing_fact_as_unavailable() {
        let attacker = addr(1);
        let mut builder = FactArenaBuilder::new();
        let root = Call::new(
            builder.next_call_id(),
            None,
            CallKind::StaticCall,
            CallDepth(0),
            attacker,
            Some(addr(2)),
            Wei::ZERO,
            Gas(1),
            Gas(1),
            true,
        )
        .unwrap();
        builder.add_call(root);
        let arena = builder.build().unwrap();
        let trace = build_trace(arena, attacker);
        let index = matcher::TraceIndex::build(&trace);

        let predicate = call_predicate("External");
        let outcome = CallVerifier.verify(&trace, &index, &predicate, Polarity::Positive, None);
        assert_eq!(outcome.outcome, EvidenceOutcome::Unavailable);
    }
}

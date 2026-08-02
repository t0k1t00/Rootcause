# Root Cause — Research Foundations

*Primary source: "Scientific Review: Does the Core Hypothesis Hold?" This document reproduces and organizes that Review's content; it does not add new research claims.*

## Scientific hypothesis

**Informal claim:** most important exploit classes can be represented as deterministic structural patterns over execution traces, storage changes, value flow, and source-code relationships with sufficiently high precision and useful recall.

**Formalized** (as the Review states it):

- *E* = the set of all historically significant smart-contract-adjacent security incidents, weighted by loss value or frequency (the Review notes a choice must be made between these weightings and flags this as mattering).
- *E_onchain* ⊆ *E* = the subset of incidents that manifest as an anomalous, analyzable on-chain transaction trace — explicitly excluding incidents where a legitimate-looking transaction was authorized by a compromised or malicious signer.
- *P* = a finite, human-authored library of deterministic structural patterns.
- **Precision** = correctly-matching classifications ÷ all classifications emitted.
- **Recall** = correctly-classified *E_onchain* incidents ÷ all of *E_onchain*.
- **Assumption A1** (the Review's own, not this document's): the hypothesis is only ever a claim about *E_onchain*, never about *E* as a whole — conflating the two is identified as the single most likely overclaiming failure mode.

## Scope

In scope, per the Review's Phase 2 analysis:

- **Well-representable**: oracle manipulation, flash-loan price manipulation, donation/inflation attacks, classic reentrancy.
- **Partially representable**: governance attacks (only the flash-loan-funded, temporally-anomalous voting-power-spike variant; slower off-chain vote-buying is not representable), cross-chain/bridge exploits (only the drain side; the root-compromise side typically is not), read-only reentrancy (representable but awkward — stresses the DSL toward cross-transaction/cross-contract-boundary reasoning), MEV-assisted exploits (representable to the extent the exploit itself has structural shape).

## Out-of-scope classes

- **Private-key/multisig compromise, phishing, signer social engineering**: cannot be represented at all — a structural, category-level exclusion, not a DSL weakness, since the resulting transaction is validly authorized and structurally indistinguishable from a legitimate action.
- **Novel, protocol-specific logic errors**: frequently not representable — the Review's single most important finding (its words: "the hardest-won finding of this review"). No generic structural signature generalizes across "some assumption somewhere about another contract's semantics became false," because the assumption differs by definition in every case. A real 2025 incident (a third-party adapter's fee calculation breaking after an unrelated, correctly-documented upstream accounting change) is cited as the illustrative case: the root cause was one conditional branch encoding a stale assumption, with no generalizable pattern.
- **Unknown/novel classes generally**: never representable until a human writes a pattern for them — stated as the honest, accepted nature of any signature-based system (the Review draws the explicit analogy to YARA/Sigma/Semgrep, which openly accept the same limitation).

## Coverage estimates

Per the Review's Phase 3, grounded in cited industry data:

- **Out-of-scope by category** (off-chain: keys, phishing, signer compromise, most governance/social attacks): roughly 50-57% of incident count, 70-80%+ of value — cited to Halborn's 2025 top-100-hacks report (56.5% of attacks, 80.5% of stolen funds off-chain in 2024) and Chainalysis (private-key compromise as the largest single 2024 stolen-value category).
- **In-scope, well-templated** (oracle manipulation, reentrancy variants, donation/inflation, classic flash-loan price attacks): plausibly 20-35% of *E_onchain* by count, likely a larger share by value given oracle manipulation's outsized average loss.
- **In-scope but poorly-templated** (novel, protocol-specific logic errors): a large remaining share of *E_onchain*, honestly not well-served by deterministic patterns, expected to produce mostly Abstain results — correctly, per the Review.
- **Ambiguous**: cross-chain/bridge incidents split unpredictably depending on which half of the incident is analyzed.

Supporting figures cited in the Review: a 2024 incident review found logic errors the single largest root-cause category (50 of that year's incidents), followed by input-validation issues (20) and price manipulation (18), out of 100+ reviewed on-chain incidents; a separate multi-year Halborn analysis found logic errors caused 66.7% of 2023 hacks by occurrence and 74.8% of that year's lost value, with flawed oracles responsible for 49.3% of losses within the price-manipulation sub-category.

**This directly reframes the hypothesis**, per the Review's own conclusion: "most important exploit classes" is false if "most" is read across all incidents or all value. The hypothesis is plausibly true only in the narrower form: *a meaningful, recurring, high-average-severity minority of on-chain technical exploits — specifically oracle/price-manipulation and reentrancy-family attacks — can be captured with high precision by deterministic structural patterns.*

## Precision/Recall goals

Per-family estimates from the Review's Phase 5:

| Family | Precision ceiling | Recall ceiling |
|---|---|---|
| Oracle/price manipulation | ~90%+ | ~50-60% of subfamily |
| Classic reentrancy | ~90%+ | ~60%+ |
| Read-only reentrancy | ~70-80% | ~30-40% |
| Donation/inflation | ~90%+ | ~50% |
| Novel protocol-specific logic errors | not meaningfully estimable / near-zero by design | near-zero, correctly |
| Off-chain (keys/phishing/governance-social) | N/A — out of scope | 0% by category |

Overall success criteria (Phase 1): precision ≥ ~90% on a held-out benchmark, recall ≥ a useful-but-modest threshold (≥30-40% of *E_onchain*) — recall need not be high, since abstention is a valid, first-class output. Failure condition: precision cannot be pushed above roughly 70-80% without either the grounding architecture failing to prevent false positives, or recall collapsing to near-zero to compensate — either falsifies the hypothesis as practically useful.

## Publishable contributions

Ranked, per the Review's Phase 7:

1. The grounding/abstention mechanism and its formal completeness guarantee — most novel, most rigorously defensible, publishable as a systems/methodology contribution independent of any specific pattern's accuracy.
2. The benchmark itself (curated, independently-sourced, versioned, false-positive/negative tracked separately) — genuinely useful as a citable artifact even if engine coverage stays modest.
3. The coverage/limits analysis (i.e., this Review's own Phase 3 finding) — an honest, data-grounded negative result the field currently lacks a clear citation for.
4. The DSL — useful, but closer in kind to existing rule DSLs (YARA/Sigma/Semgrep) than a novel formalism; incremental.
5. The matching engine — sound engineering, not novel science, deliberately the lightest mechanism that fits.
6. Taxonomy mapping — useful glue, not a contribution (reuses SCWE/SWC/DASP rather than inventing anything).

## Competitive positioning

**Not addressed in the Scientific Review.** No competing tool, project, or prior art is named or compared against anywhere in the source document, beyond the passing structural analogy to YARA/Sigma/Semgrep as examples of signature-based systems that share the same known-unknowns limitation. This document does not invent a competitive analysis; that exercise, if wanted, belongs in a separate document explicitly sourced from its own research (as was done in this project's prior, separate Competitive Analysis exercise), not asserted here as if the Review contained it.

## Failure modes

Per the Review's Phase 6, ranked by existential severity:

- **Existential**: the grounding architecture fails to keep precision high in practice — e.g., "optional" evidence clauses get silently treated as load-bearing by pattern authors under time pressure, quietly eroding the no-hallucination guarantee.
- **Existential**: ground-truth ambiguity in the benchmark — several real incidents have genuinely disputed root-cause attribution across different public post-mortems; if benchmark ground truth is shakier than the tool's own confidence claims, the evaluation itself becomes unreliable.
- **Serious but not existential**: pattern explosion / rule maintenance burden as protocol diversity grows.
- **Serious but not existential**: overfitting a pattern to the exact incident(s) used to author it, producing benchmark-passing but real-world-brittle patterns.
- **Not existential, expected and acceptable**: low recall on novel/protocol-specific logic errors — per the Review, this is the honestly-scoped boundary of the hypothesis, not a bug.

## Week-1 experiment

Per the Review's Phase 8: do not build the DSL, matching engine, or CLI in week one. Build the smallest possible thing that tests the actual risky assumption — can a hand-written, one-off structural predicate (raw code, no DSL) correctly and fully ground a classification for a single, well-documented oracle-manipulation incident, using only facts pulled from a real archive-node trace? If a single hand-coded predicate against one real transaction can't be made to fully ground (every required fact traceable, no hand-waving), the entire premise is in trouble regardless of DSL or matcher quality — the hard part was never DSL syntax, it's whether the underlying facts are actually recoverable and sufficient from a real trace.

## Two-week prototype

Per the Review's Phase 10:

- **Code**: a minimal, hard-coded (no DSL) script that (1) pulls a real trace + storage diff for 5 hand-picked, well-documented oracle-manipulation incidents via archive-node RPC, (2) hand-codes the single "oracle staleness / donation attack" predicate directly against the Fact Model types, (3) for each incident, either fully grounds a match or explicitly fails and records exactly which clause couldn't be grounded and why.
- **Dataset**: exactly 5 real, independently well-documented oracle/donation-attack incidents, including at least one "easy" canonical case and at least two structurally different implementations (different DEX/oracle designs), to test generalization rather than memorization of one shape.
- **Metrics**: grounding success rate (fully ground / partially ground / fail entirely), and for failures, a precise written note on why (missing fact in the archive-node trace, storage layout unresolvable, or genuine structural mismatch).

## Success criteria

Per the Review's Phase 10: at least 4 of the 5 incidents must fully ground with no hand-waved evidence, **and** the failure mode on the 5th (if any) must be a clearly fixable data/ingestion gap rather than a fundamental "the pattern concept doesn't generalize" gap. If fewer than 3 of 5 fully ground, or if failures trace back to the concept rather than engineering gaps, that is the signal to stop and rethink before investing the remaining five months (Phase 10) — this two-week result is described as the actual go/no-go gate for the engineering effort, more informative than architectural review alone.

Overall project-level go/no-go, per Phase 9: **GO — but only on the reformed, narrower hypothesis**, stated explicitly in the project's own materials from day one: deterministic structural patterns can classify, with high precision and honest partial recall, the recurring subfamily of on-chain exploits — chiefly oracle/price-manipulation and reentrancy-family attacks — while correctly abstaining on off-chain incidents (out of scope by category) and novel protocol-specific logic errors (out of scope by construction, until a pattern is authored). The original, broader framing is explicitly not supported and should be retired from the project's own abstract/README.

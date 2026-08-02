# Root Cause — Benchmark

*Scope note: the Scientific Review specifies the benchmark concretely only at the scale of the two-week prototype (Phase 10) — a 5-case dataset for a single pattern family. It does not specify a benchmark design for the full, multi-family, ongoing project. This document reconstructs what the Review actually supports and marks everything beyond that as an assumption or as unaddressed.*

## Benchmark methodology

Per the Review's Phase 10, the methodology at the prototype stage is:

1. Select a small number of real, independently well-documented incidents for one pattern family (oracle/donation-attack, in the Review's own example).
2. For each incident, pull the real trace plus storage diff via archive-node RPC.
3. Attempt to hand-ground a single, hand-coded predicate against each incident's facts.
4. Record, per incident, whether the predicate fully grounds, partially grounds, or fails — and if it doesn't fully ground, record the specific unmet clause and the reason.

**Assumption B-1:** for pattern families beyond the one the Review examines directly (oracle/donation-attack), the same methodology is the smallest reasonable generalization — repeat the same four steps per family. The Review does not state this generalization explicitly; it is inferred from the fact that Phase 5's precision/recall table treats multiple families symmetrically.

## Ground truth

The Review requires each case to be "well-documented" and "independently" so, but does not specify a formal ground-truth schema (e.g., confidence tiers, required citation count, or a process for handling disputed attribution) — despite identifying disputed ground-truth attribution as one of the two existential failure risks to the whole evaluation (Phase 6: "several real incidents have genuinely disputed root-cause attribution across different public post-mortems... if the benchmark's ground truth is shakier than the tool's own confidence claims, the evaluation itself becomes unreliable").

**Assumption B-2:** given the Review names this as an existential risk without prescribing a mitigation mechanism, the smallest reasonable response is to record, per case, at least the source(s) used to establish ground truth, so a disputed case can later be identified and excluded from headline metrics if needed. This is an assumption filling a gap the Review itself flags as important but does not resolve.

## Dataset sources

The Review specifies exactly one dataset-sourcing method: **archive-node RPC**, pulling both the transaction trace and its storage diff (Phase 8, Phase 10). No other data source (block explorers, third-party trace exporters, community incident databases) is named anywhere in the source document. This document does not add any, per the instruction not to broaden scope.

## Evaluation metrics

Directly specified by the Review (Phase 10):

- **Grounding success rate**: how many of the dataset's cases fully ground, versus partially ground, versus fail entirely.
- **Failure-cause classification**: for any non-fully-grounding case, a precise written note on why — the Review names three example causes (missing fact in the archive-node trace; storage layout unresolvable; genuine structural mismatch between the incident and the intended pattern), which this document treats as the minimal required categorization, per Assumption E-4 in `02_ENGINEERING_SPEC.md`.

**Assumption B-3:** the Review's Phase 1 and Phase 5 discuss precision and recall as formal metrics with target thresholds (≥90% precision, ≥30-40% recall), but these are stated as properties of the eventual, matured system evaluated against a "held-out benchmark" — not as something the 5-case, single-family, two-week prototype is expected to measure directly (5 cases is too small a sample to meaningfully estimate a precision/recall ratio). This document treats grounding-success-rate as the prototype-stage metric, and precision/recall as the later, larger-benchmark-stage metrics, since conflating the two would misrepresent what the Review's own two-week gate actually measures.

## Regression testing

**Not addressed in the Scientific Review.** No mention of re-running the benchmark after a change, detecting a change in outcome for a previously-passing case, or any CI-style regression process appears anywhere in the source document.

## Benchmark versioning

**Not addressed in the Scientific Review.** No mention of versioning the case set, tracking historical results, or treating the benchmark as a released artifact with its own version number appears in the source document — despite the Review elsewhere (Phase 7) describing the benchmark itself as a "citable artifact," which would typically imply some form of versioning for citation stability. This document notes the gap rather than inventing a versioning scheme to fill it.

## Continuous evaluation

**Not addressed in the Scientific Review.** No CI process, automated re-evaluation cadence, or dashboard is discussed.

---

**Summary for the reader:** the Review supports a real, concrete methodology for exactly one thing — a small, single-family, one-time validation prototype used as a go/no-go gate before further engineering investment. It does not support (and this document does not invent) a full ongoing-benchmark design covering versioning, regression detection, continuous evaluation, or multi-family dataset construction at scale. Those remain open design work for whoever picks this project up next, to be done once the Phase 10 prototype's own gate (≥4 of 5 cases fully grounding, Phase 10) has actually been cleared — building out full benchmark infrastructure before that gate is cleared would be premature investment the Review's own Phase 8 explicitly warns against ("do not build... in week one").

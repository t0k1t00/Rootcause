# Root Cause — Architecture

## Executive Summary

Root Cause is a deterministic engine that classifies historical smart-contract exploit transactions against a curated library of recurring, structurally-templatable on-chain exploit patterns. Every emitted classification is backed by a complete, mechanically-checkable evidence chain drawn from the transaction's execution trace. If that chain cannot be fully constructed, the engine abstains rather than guessing. The project deliberately optimizes for high precision over high recall, and states its scope narrowly and honestly: it classifies a recurring minority of *on-chain* exploit families, not exploits in general.

**Assumption A-1:** the Scientific Review does not itself specify a working name; "Root Cause" is carried forward from context as the project's name for documentation purposes.

## Goals

- Classify recurring on-chain exploit families — chiefly oracle/price manipulation and reentrancy-family attacks — with precision at or above roughly 90% on a maintained benchmark (per the Scientific Review's stated success criteria, Phase 1).
- Achieve a useful-but-modest recall floor (the Review's Phase 1 cites roughly 30-40% of *E_onchain*, the on-chain-only incident subset) rather than attempting comprehensive coverage.
- Guarantee that every positive classification is fully evidence-grounded, with abstention as a first-class, correct output rather than a failure state.
- Publish an honestly-scoped benchmark and coverage/limits analysis as a citable artifact independent of the engine's own adoption.

## Non-goals

Directly per the Scientific Review's Phase 2/3 findings:

- Classifying off-chain root causes: compromised private keys, phishing, signer/multisig compromise, and most social-engineering-based governance capture. These produce validly-authorized transactions with no anomalous on-chain structure to match against — a structural exclusion, not a tooling gap.
- Achieving high recall on novel, protocol-specific logic errors (the "one wrong conditional branch" class). The Review's Phase 2/4 conclude these are not expressible in a deterministic pattern in principle, since the violated assumption differs in every such case by construction. The system is expected to abstain on these, correctly.
- Claiming to be a general exploit classifier. The Review's Phase 9 explicitly requires the broader "most important exploit classes" framing to be retired in favor of the narrower, defensible claim above.

## High-Level Architecture

**Assumption A-2:** the Scientific Review evaluates a hypothesis and a DSL/benchmark design but does not itself specify subsystem boundaries, data flow, or trust boundaries. The subsystem list below is the smallest reasonable structure implied by the Review's own phase structure (it discusses a DSL, a matching/grounding step, and a benchmark as distinct things) and is marked as reconstructed, not sourced.

Reconstructed pipeline, in the order the Review's own phases discuss these concerns:

1. **Trace Ingestion** — turns a raw transaction trace into a normalized fact representation.
2. **Pattern DSL & Compiler** — turns human-authored pattern definitions into an executable form.
3. **Matching Engine** — finds structural candidate matches between a pattern and a trace's facts.
4. **Grounding Verifier** — the Review's central object of study (Phase 4: "No evidence → No classification"); independently confirms every required clause of a candidate match against a concrete fact, or discards it.
5. **Taxonomy Layer** — maps a confirmed classification to an external vocabulary (the Review does not name one explicitly; see Assumption A-3 below).
6. **Benchmark** — the independent evaluation corpus and harness described extensively in the Review's Phase 3, 5, and 10.

**Assumption A-3:** the Review does not name a specific taxonomy standard. The smallest reasonable choice, consistent with the Review's own references to "SCWE/SWC/DASP" (Phase 2 discusses SWC informally when naming exploit classes), is to adopt SCWE as primary with SWC/DASP as secondary mappings. This is marked as an assumption, not a Review-sourced decision.

## Fact Model

**Assumption A-4:** the Scientific Review's Phase 8 ("Week-1 experiment") explicitly references a "Fact Model" and "Call/StorageChange types already defined in the architecture," implying a fact model existed in prior (now-lost) work, but the Review itself does not define its fields. The following is the minimal reconstruction needed to support the Review's own stated claims (that classifications ground against "a specific Call/StorageChange," Phase 8), not a full specification:

- **Transaction**: the top-level executed transaction being classified.
- **Call**: one node in the transaction's call tree, sufficient to express "value flow," "call relationships," and "delegatecall" (all referenced in the Review's Phase 4/5 discussion of exploit families).
- **StorageChange**: a before/after slot value change, needed to ground the "oracle staleness / donation attack" predicate the Review's Phase 8 names explicitly.
- **BalanceChange / ValueFlow**: needed to express "net value extracted," referenced throughout Phase 2's family-by-family analysis.

Fields, encodings, and lifecycle rules beyond this minimal set are **not specified by the Review** and are left for a follow-on engineering pass; this document does not invent them.

## Trace Ingestion

The Review's Phase 8 (Week-1 experiment) specifies the ingestion requirement most concretely: facts must be "pulled from a real archive-node trace," and the two-week prototype (Phase 10) specifies pulling "a real trace + storage diff... via archive-node RPC." **Archive-node RPC is therefore the only ingestion source directly evidenced by the canonical artifact.** Any other source format (Foundry, Tenderly, Phalcon, etc.) is not mentioned anywhere in the Review and is **not included in this reconstruction** — introducing them here would broaden scope beyond what the Review supports, which this task's instructions explicitly prohibit.

**Assumption A-5:** ingestion must, at minimum, resolve storage layout well enough to identify a named "oracle staleness / donation attack" predicate's supporting facts (Phase 8) — implying some form of storage-slot-to-variable resolution is required, even though the Review does not specify the mechanism.

## Pattern DSL

The Review does not specify DSL syntax. It does, however, establish firm requirements on what the DSL must be able to express, which any DSL design must satisfy:

- Must express oracle-manipulation, donation/inflation, and classic-reentrancy patterns "cleanly" (Phase 4).
- Must support a distinction between clauses that are always required for a match to be legitimate, since the entire grounding claim (Phase 6, "optional clauses silently treated as load-bearing") depends on the DSL having a real, author-visible required/optional distinction.
- Read-only reentrancy is explicitly flagged as "awkward" to express, requiring reasoning across a third contract mid-reentrancy (Phase 4) — a real, named limitation any DSL design must accept rather than paper over.
- Novel protocol-specific logic errors are explicitly stated as **not expressible in principle**, "no DSL enhancement fixes this" (Phase 4) — the DSL must not attempt to close this gap.

**Assumption A-6:** beyond these constraints, no grammar is specified in the Review. A concrete grammar is out of scope for this reconstruction; it belongs in a follow-on DSL design document once this architecture doc is agreed.

## Pattern Compiler

**Not addressed in the Scientific Review.** The Review discusses patterns only at the level of what they must express and how they perform (Phase 5's precision/recall table), never how they are parsed or executed. **Assumption A-7:** a compiler subsystem is assumed to exist (turning authored patterns into an executable form) purely because the Review's Phase 8 refers to "the DSL syntax" as distinct from "the underlying facts," implying some translation step — no further detail is invented here.

## Matching Engine

**Not directly specified.** The Review's Phase 5 table (precision/recall ceilings per family) implies a matching mechanism exists and has per-family accuracy characteristics, but says nothing about its internal algorithm. This document does not invent one; algorithm choice belongs in the engineering spec once made, informed by, but not sourced from, this Review.

## Grounding Verifier

This is the one subsystem the Scientific Review discusses in real depth, because it is the central object of the hypothesis under review. Requirements directly sourced from the Review:

- The central claim under test is **"No evidence → No classification"** (Phase 4 of this doc's source material, referenced throughout the Review).
- The Review's Phase 6 identifies the top existential risk as: "optional" evidence clauses getting silently treated as load-bearing by pattern authors under time pressure, quietly eroding the no-hallucination guarantee. **Any grounding verifier design must treat this as its primary threat to defend against.**
- The Review's Phase 8/10 establish the concrete acceptance bar for the grounding mechanism: a hand-coded predicate against a real archive-node trace must either fully ground (every clause backed by a specific `Call`/`StorageChange`) or explicitly fail with a recorded reason. **This — full grounding or an explicit, categorized failure — is the minimum bar any implementation must clear.**

**Assumption A-8:** the Review does not specify an abstention-reason taxonomy (e.g., missing-evidence vs. conflicting-evidence) beyond "explicitly fails and records exactly which clause couldn't be grounded and why" (Phase 10). A minimal two-category reason set (missing evidence; contradicted evidence) is the smallest reasonable structure satisfying that requirement and is marked as an assumption.

## Taxonomy Layer

**Not specified in the Review beyond incidental mentions of SWC when naming exploit classes informally** (e.g., Phase 4's reference to SWC-numbered classes in passing). Per Assumption A-3, SCWE is adopted as primary with SWC/DASP as secondary, as the smallest reasonable extension of what the Review already implicitly gestures at. No mapping table structure is specified by the Review; this is left for a follow-on document.

## Benchmark Architecture

The most thoroughly specified non-grounding subsystem in the Review. Requirements sourced directly:

- Dataset: **exactly 5** real, independently well-documented oracle/donation-attack incidents at the two-week-prototype stage (Phase 10), deliberately including at least one canonical "easy" case and at least two structurally-different implementations, to test generalization rather than memorization of one shape.
- Metrics collected: grounding success rate (full ground / partial ground / fail entirely) per case, and for failures, a precise written note on cause (missing archive-node fact, unresolvable storage layout, or genuine structural mismatch).
- Gate for continued investment: at least 4 of 5 incidents must fully ground with no hand-waved evidence, **and** any failure on the 5th must be a clearly fixable data/ingestion gap rather than a fundamental conceptual mismatch. Fewer than 3 of 5 fully grounding, or concept-level (not engineering-level) failures, is the Review's explicit stop-and-rethink signal (Phase 10).
- The benchmark itself, independent of engine performance, is identified as a standalone publishable contribution (Phase 7, item 2) and should be built and versioned with that in mind from the start.

See `docs/04_BENCHMARK.md` for the full reconstruction.

## Repository Structure

**Not specified in the Scientific Review.** No file, directory, or repository layout is discussed anywhere in the source document. **This document deliberately does not invent one** — proposing a repository structure not supported by the canonical artifact would exceed this task's explicit instruction not to broaden scope. A repository layout should be proposed in a separate, clearly-labeled follow-on document once this architecture is agreed, not asserted here as if it were Review-derived.

## Threat Model

**Not specified in the Scientific Review**, with one partial exception: Phase 6 identifies "ground-truth ambiguity in the benchmark" (disputed root-cause attribution across post-mortems) as an existential risk to the evaluation itself — this is a data-integrity threat to the benchmark, not a security threat model for the engine, and is the only threat-adjacent content the Review actually contains. A full threat model (poisoned patterns, supply chain, malformed traces, etc.) is not invented here; it would exceed what the canonical artifact supports.

## Black Hat MVP

**Not specified in the Scientific Review.** The Review discusses a "Week-1 experiment" and a "two-week prototype" (Phases 8 and 10) as risk-reduction steps, not an MVP or conference-demo scope. No six-month/one-engineer MVP boundary is stated anywhere in the source document. This section is intentionally left undefined here rather than invented; the Week-1/two-week experiments below (see Research doc) are the only concretely-scoped near-term deliverables the canonical artifact supports.

## Engineering Roadmap

Directly reconstructable from the Review's own Phase 8 and Phase 10, and only these two steps:

1. **Week 1-2**: hand-coded, no-DSL, no-matcher, single predicate against 5 real oracle-manipulation-incident traces pulled via archive-node RPC, checking full groundability. Go/no-go gate: 4/5 fully ground.
2. **Weeks 3-4 (the Review's "two more weeks")**: a minimal script (Python or Rust, per Phase 10 — the Review names both as options, not a firm choice) implementing the same single "oracle staleness / donation attack" predicate directly against the (assumed, Assumption A-4) Fact Model types, across the 5-case dataset described above, with per-case grounding-success/failure recording.

**Assumption A-9:** no roadmap step beyond the two-week prototype is specified in the Review. Anything past Week 4 (DSL implementation, matching engine, full benchmark, MVP hardening) is unaddressed by the canonical artifact and is not invented here.

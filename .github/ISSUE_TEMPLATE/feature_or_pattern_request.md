---
name: Feature or pattern request
about: Propose a new detection pattern, DSL capability, or other feature
title: ""
labels: enhancement
---

**What are you trying to detect or do?**
Describe the vulnerability class, workflow, or capability.

**What fact-model evidence would ground it?**
Root Cause only adds detection capability that grounds against facts the
trace actually records (see "The one rule that matters most" in
`docs/PATTERN_AUTHORING_GUIDE.md`, and the "Exploit families evaluated but
not added" section of `README.md` for precedent on where this line has
already been drawn). If your proposal needs a fact the engine doesn't
currently record (e.g. an intent, authorization, or off-chain-state fact),
say so explicitly — that's useful information even if the feature can't
be built as described yet.

**Is this a new pattern, or a new DSL/engine capability a pattern would
need?**
- [ ] A new `.rcdsl` pattern using existing predicates
- [ ] A new predicate or predicate attribute
- [ ] Something else (describe below)

**Additional context**
Links to a real exploit writeup, SCWE/SWC entry, or similar are
especially helpful.

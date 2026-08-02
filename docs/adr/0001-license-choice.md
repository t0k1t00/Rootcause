# 0001. License choice

Status: Accepted

## Context
Neither the Architecture document, the Engineering Specification, the
Research document, nor the Benchmark document specifies a license for the
repository. A license is required before any public artifact (including a
Black Hat Arsenal demonstration repository) can be shared. This does not
alter externally visible engine behavior and does not touch any stated
architectural invariant, so it is eligible for delegated decision.

## Decision
Apache License 2.0 for the whole workspace, declared once in
`[workspace.package] license` and enforced via `deny.toml`'s license
allow-list (which also permits MIT/BSD-2/BSD-3/Unicode variants for
dependencies, since a strict single-license transitive closure is
unrealistic in the Rust ecosystem).

## Consequences
Apache-2.0's explicit patent grant is a deliberate choice given the
project's subject matter (vulnerability detection): it reduces ambiguity
about patent licensing for anyone building on top of the taxonomy or
grounding engine. This decision does not preclude dual-licensing later if
a canonical document specifies otherwise; that would be a new ADR
superseding this one, not a silent edit.

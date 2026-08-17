# Semantic facts conformance corpus v0

This compact corpus is organized by semantic capability rather than compiler
flags. Every case contains a machine-readable `case.json`, a short statement of
what the case proves, one project configuration, and only the source needed for
that proof.

The corpus deliberately separates core graph shapes, callable and generic
signatures, advanced type operators, occurrence contexts, and recovery/budget
pressure. The manifest smoke test keeps every selection resolvable and every
snapshot valid. Golden normalization and structural assertions build on these
same cases in the conformance milestone.

Primitive/literal shadow selections add a structured `conformance` expectation
to every selected fixture. The expectation pins the Go fact state, all five
type-view states, and the structured actual type, then classifies the Rust/OXC
observation as `supported`, `unsupported`, `budget`, or `mapping`. The
conformance report lists unrelated corpus cases as excluded and reports every
selected fact in exactly one class, so unsupported or recovery evidence cannot
silently disappear from a compatibility denominator.

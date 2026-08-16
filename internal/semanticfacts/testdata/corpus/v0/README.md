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

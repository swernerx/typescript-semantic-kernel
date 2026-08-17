# Architecture Decision Records

Architecture Decision Records (ADRs) capture durable implementation decisions
that refine the repository's RFCs. Accepted ADRs are immutable; a changed
decision is recorded in a successor ADR that links to the record it replaces.

## Index

| ADR | Status | Decision |
| --- | --- | --- |
| [0001](0001-use-utf8-byte-offsets-for-semantic-facts.md) | Accepted | Use UTF-8 byte offsets for semantic facts schema v1 |
| [0002](0002-use-response-local-symbol-and-declaration-ids.md) | Accepted | Use response-local symbol and declaration IDs |
| [0003](0003-classify-symbol-backed-type-view-provenance.md) | Accepted | Classify symbol-backed type-view provenance |
| [0004](0004-make-optional-type-view-availability-explicit.md) | Accepted | Make optional type-view availability explicit |
| [0005](0005-use-response-local-referential-graph-tables.md) | Accepted | Use response-local referential graph tables |
| [0006](0006-negotiate-capabilities-and-bound-type-graphs.md) | Accepted | Negotiate capabilities and bound type graphs |
| [0007](0007-separate-semantic-snapshot-building-from-json-lines.md) | Accepted | Separate semantic snapshot building from JSON Lines transport |
| [0008](0008-export-core-composite-types-as-normalized-graph-nodes.md) | Accepted | Export core composite types as normalized graph nodes |
| [0009](0009-intern-object-symbol-and-signature-graphs-before-finalization.md) | Accepted | Intern object, symbol, and signature graphs before finalization |
| [0010](0010-export-advanced-types-through-semantic-detail-records.md) | Accepted | Export advanced types through semantic detail records |
| [0011](0011-build-api-facts-from-pinned-project-programs.md) | Accepted | Build API facts from pinned project programs |
| [0012](0012-normalize-conformance-snapshots-without-rewriting-graph-identity.md) | Accepted | Normalize conformance snapshots without rewriting graph identity |
| [0013](0013-correlate-occurrences-through-consumer-node-anchors.md) | Accepted | Correlate occurrences through consumer node anchors |
| [0014](0014-isolate-the-oxc-reference-consumer.md) | Accepted | Isolate the OXC reference consumer |
| [0015](0015-attach-semantic-facts-without-expanding-graph-identity.md) | Accepted | Attach semantic facts without expanding graph identity |
| [0016](0016-port-occurrence-attachment-before-semantic-categories.md) | Accepted | Port occurrence attachment before semantic categories |
| [0017](0017-project-primitive-literal-candidates-from-go-graph-identity.md) | Superseded | Project primitive/literal candidates from Go graph identity |
| [0018](0018-gate-rust-semantics-against-the-go-oracle.md) | Superseded | Gate Rust semantics against the Go oracle |
| [0019](0019-compute-primitive-literals-independently-in-rust.md) | Accepted | Compute primitive/literal candidates independently in Rust |

## Current migration decision

The completed [TS7-to-OXC/Rust spike](../evidence/ts7-oxc-spike-2026-08-17.json)
supports mechanically porting occurrence identity and attachment plumbing
behind differential comparison to the TypeScript 7 Go oracle. The isolated
Rust/OXC consumer remains a reference and migration harness; the evidence does
not establish compiler equivalence, production readiness, or a performance
advantage.

[ADR-0019](0019-compute-primitive-literals-independently-in-rust.md) runs an
independent Rust/OXC primitive/literal producer over the tagged shared corpus
and enforces exact structured agreement in shadow CI. It owns its type graph
and receives no Go semantic graph input. Passing the gate still leaves
primitive/literal construction Go-authoritative in production; no semantic
category is currently approved for replacement.

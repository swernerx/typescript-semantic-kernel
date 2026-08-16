# ADR-0015: Attach semantic facts without expanding graph identity

- Status: accepted
- Date: 2026-08-17
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

ADR-0014 isolates live OXC traversal and occurrence correlation in the internal
Rust reference consumer. A mapped `NodeId` still needs access to all semantic
type views and to the response-local type, symbol, signature, and declaration
tables produced by TypeScript 7. Expanding each root into a Rust tree would
duplicate shared nodes, lose cycles, and make consumer traversal look like a
second source of semantic truth.

The occurrence contract also permits repeated request selections. Two fact
records can therefore map to one arena-local `NodeId`; a side table that stores
only one fact would silently discard request order and provenance.

## Decision

The isolated Rust consumer decodes schema-v1 JSON Lines into one immutable,
reference-counted semantic snapshot. The snapshot owns one `TypeGraph`; fact
records retain response-local IDs and never copy or flatten graph nodes.

Successful correlation builds a consumer-local `NodeId -> [fact index]` side
table in fact order. Accessors resolve the five contract roots: actual,
contextual, widened, apparent, and declared. A `same-as-actual` view resolves to
the actual root while retaining its original view state. Inapplicable and
unavailable views remain explicit and do not fabricate an ID.

The graph inspector emits deterministic JSON containing fixed-order roots,
identity-preserving node and edge records, fact and entity completeness, graph
issues, and diagnostics. It traverses each response-local identity at most
once. Independent maximum-depth, node, and edge limits stop consumer traversal;
budget diagnostics identify the cutoff without changing producer graph state.

Correlation diagnostics remain available alongside attachments. Unmapped or
multiply mapped facts are not attached to an arbitrary node.

## Consequences

- Multiple OXC nodes can point at the same TypeID without copying its record.
- Repeated selections attached to one node remain independently observable.
- Cycles terminate by identity even when the depth budget is generous.
- Producer truncation, unsupported entities, checker errors, and consumer
  inspection cutoffs remain distinct machine-readable conditions.
- The TypeScript 7 Go checker remains the semantic oracle. OXC types stay in
  the isolated consumer and no Rust or OXC shape enters the producer protocol.

## Validation and review triggers

- Shared schema-v1 fixtures must round-trip through the Go reader and decode in
  the Rust consumer.
- Rust tests must exercise every type-view root, shared TypeIDs, repeated facts,
  recursive graphs, entity states, mapping diagnostics, and each inspector
  budget independently.
- Inspector output for identical input and limits must be byte-for-byte stable.
- Revisit the side-table shape only if schema semantics distinguish multiple
  attachments beyond fact order and fact index.

## References

- [ADR-0005](0005-use-response-local-referential-graph-tables.md)
- [ADR-0013](0013-correlate-occurrences-through-consumer-node-anchors.md)
- [ADR-0014](0014-isolate-the-oxc-reference-consumer.md)
- [Semantic facts protocol](../tsfacts-protocol.md)
- [Issue #17](https://github.com/swernerx/typescript-semantic-kernel/issues/17)

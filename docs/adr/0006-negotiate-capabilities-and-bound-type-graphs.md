# ADR-0006: Negotiate capabilities and bound type graphs

- Status: accepted
- Date: 2026-08-15
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

ADR-0005 defines a referential semantic graph, but an unbounded traversal can
consume arbitrary memory and an omitted edge does not explain whether data is
complete, unsupported, or lost to a failure. Schema version alone also cannot
express which independently delivered protocol slices a producer implements.
Consumers need deterministic limits and compatibility rules before snapshots
can serve as a conformance boundary for OXC or another backend.

## Decision

Schema v1 requests may list `requiredCapabilities`. The producer rejects an
unknown or duplicate requirement before loading the project. Every response
lists its capabilities in sorted order. The canonical capability set currently
names occurrence views, graph references and signatures, explicit entity
states, bounded type graphs, and the canonical fixture corpus v0.

Requests also carry `budgets.maxTypeNodes` and `budgets.maxTypeDepth`. Zero
selects the schema-v1 defaults of 4096 checker-backed type nodes and a depth of
32; negative values are invalid. The actual type root is depth zero. A node is
charged exactly once when its response-local ID is allocated. Shared or cyclic
references reuse that charge. Budget sentinel nodes are not charged because
they stand in for checker nodes that were not traversed.

Crossing either boundary produces one shared, referential `truncated` sentinel
per cutoff reason. Its `issues` entry contains `max-type-nodes` or
`max-type-depth` and the effective limit. Owners that reference an incomplete
node become `truncated` with `referenced-incomplete-type`. The header reports
the normalized limits, charged node count, deepest attempted traversal, and
whether a budget cutoff occurred. The deepest attempted traversal may exceed
the limit by one because observing the rejected edge is what proves the cutoff.

Types, symbols, and signatures carry one of four states:

- `complete`: every claimed structural edge is represented;
- `truncated`: the form is known but an explicit structural or traversal
  boundary omitted detail;
- `unsupported`: the checker form has no structural schema-v1 representation;
- `error`: the checker returned an error type or the entity could not be
  semantically recovered.

Every non-complete entity has a sorted, unique `issues` list. The legacy
`complete` and `truncated` booleans remain in schema v1 as exact projections of
the state. A fact is complete only if all referenced entities are complete and
the selected file has no diagnostics. `recovered` records checker output from a
file with diagnostics. `truncated` means that a referenced entity is actually
in the truncated state; unsupported and error entities make a fact incomplete
without misreporting a cutoff.

Normalization is part of the contract: capabilities and issue codes are
sorted, files and declarations use their documented source order, graph IDs
follow deterministic first discovery, table categories have a fixed emission
order, facts retain request order, and JSON object keys use deterministic
encoding. Canonical fixture corpus v0 is read, graph-validated, re-encoded, and
compared byte for byte in tests.

Within schema v1, consumers ignore unknown object fields, accept additional
sorted capability names, and preserve unknown issue codes as machine-readable
diagnostics. They reject unknown record kinds, entity states, type kinds, and
signature kinds. A producer may therefore add metadata or advertise a new
capability without a schema bump, but it must gate a new semantic variant on a
capability explicitly required by the request. An incompatible required field,
changed meaning, or unconditional new variant requires a new schema version.

The v0 budget report bounds checker-backed type traversal. Deep symbol and
signature traversal remains opt-in through `graph.references` and
`graph.signatures` as specified by ADR-0009. Independently bounded symbol and
signature counters require a future additive capability before that traversal
becomes default behavior.

## Considered options

### Abort the whole response when a limit is reached

This is simple but discards unaffected facts and gives consumers no partial
graph they can inspect or retry with a larger budget.

### Silently omit edges at a fixed implementation limit

Silent omission is indistinguishable from semantic absence and cannot support
conformance tests.

### Treat every incomplete entity as truncated

This conflates resource limits, missing schema support, and checker failures,
preventing useful retry and compatibility decisions.

### Accept every future enum value

Guessing the meaning of an unknown semantic variant can silently corrupt a
consumer's analysis. Additive fields are safe to ignore; variants are not.

## Consequences

- Consumers can decide whether to retry with a larger budget, reject an
  unsupported form, or surface checker recovery separately.
- Sharing and cycles do not inflate accounting, and the same request and
  project produce the same cutoff and IDs.
- Complete entities remain closed over complete references.
- The duplicated state booleans are temporary schema-v1 compatibility fields
  and must remain validator-checked.
- Adding symbol or signature budgets is an additive capability milestone, not
  an undocumented change to the existing counters. Until then, deep traversal
  remains explicitly negotiated.

## Validation and review triggers

- Tests cover default and invalid budgets, deterministic node and depth
  cutoffs, required capability rejection, and state coherence.
- Canonical fixtures cover sharing, cycles, missing occurrence views,
  truncation, diagnostic recovery, unsupported forms, and checker errors.
- The fixture reader accepts additive fields and capabilities but rejects
  unknown semantic variants.
- Revisit defaults with measured OXC workloads or when new graph dimensions
  become recursively traversable.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [ADR-0005](0005-use-response-local-referential-graph-tables.md)
- [ADR-0009](0009-intern-object-symbol-and-signature-graphs-before-finalization.md)
- [`tsfacts` protocol](../tsfacts-protocol.md)
- [Canonical fixture corpus v0](../../internal/tsfacts/testdata/canonical/v0/)

# ADR-0007: Separate semantic snapshot building from JSON Lines transport

- Status: accepted
- Date: 2026-08-15
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

The first `tsfacts` slice combined checker traversal, the semantic graph model,
graph validation, and JSON Lines transport in one package. That was sufficient
to prove the process protocol, but it made the transport the only practical way
to obtain semantic facts. RFC 0001 also calls for an in-process API, and future
consumers need to reuse the same snapshot semantics without serializing and
decoding JSON.

Issue #9 additionally introduces file-wide snapshots. File-wide and explicit
selection requests must not grow separate semantic exporters: the same source
occurrence must produce the same roots, graph entities, completeness state, and
diagnostic recovery in either mode.

## Decision

`internal/semanticfacts` owns the schema-v1 request and result model, checker
adapter, occurrence-root collection, graph interning, budgets, normalization,
and graph validation. The model remains JSON-serializable because it is the
shared schema representation, but it does not read or write a stream.

`internal/tsfacts` owns the JSON Lines process transport. It decodes a request,
encodes validated records in canonical table order, and reads canonical fixture
streams. `cmd/tsfacts` composes the two packages. A later public asynchronous Go
API can therefore wrap `semanticfacts` directly without routing through the
command protocol.

A request has two occurrence scopes:

- When `selections` is non-empty, selections are analyzed in request order.
  `files`, when present, is an allow-list for those selections.
- When `selections` is omitted or empty, every entry in `files` is a file-wide
  target. Target files are ordered by canonical file ID. Within each file,
  parser-owned semantic tokens are ordered by start offset, end offset, and
  syntax kind before analysis.

File-wide enumeration considers identifiers, private identifiers, literals and
template-literal tokens, keyword expressions, and keyword type nodes for which
the checker returns a type. It does not synthesize punctuation occurrences or
implicitly expand the scope to every file in the project.

Both modes pass their occurrences through the same root collector and graph
builder. Missing optional roots remain explicit in `typeViewStates`; an
explicit selection without an actual checker type fails the request. File-wide
enumeration includes only occurrences for which an actual checker type exists.
The checker is observed through its existing APIs and is not changed to support
snapshot construction.

The producer advertises file-wide support with the
`occurrence.file-wide` capability.

## Considered options

### Keep semantic construction and transport in one package

This preserves fewer packages but forces in-process consumers to depend on a
streaming concern and makes the later asynchronous API a wrapper around JSON.

### Define a second model for an in-process API

Two models would require continuous conversion and could drift on IDs,
availability states, graph sharing, and completeness semantics.

### Sweep every byte offset or scanner token for file-wide requests

Such a sweep would manufacture occurrences that do not correspond to the
parser's semantic nodes, handle compound syntax inconsistently, and duplicate
parser knowledge. Traversing the parser-owned tree gives stable source spans
and syntax kinds.

### Modify the checker to emit snapshots directly

That would widen the upstream patch surface and couple protocol evolution to
checker internals. The adapter can collect all required roots through existing
checker operations.

## Consequences

- The semantic snapshot becomes reusable independently of JSON Lines while the
  process protocol keeps the same schema and validation rules.
- Selection and file-wide results share one construction path and one set of
  graph invariants.
- File-wide output is deterministic for an identical project and request.
- The eligible file-wide occurrence set is deliberately parser-aware and may
  grow only with explicit tests and compatibility review.
- Transport fixtures remain under `internal/tsfacts`; semantic behavior tests
  live under `internal/semanticfacts`.

## Validation and review triggers

- Tests compare repeated file-wide snapshots byte for byte and cover canonical
  file ordering, occurrence ordering, contextual roots, and empty scope
  rejection.
- Command tests exercise file-wide requests through the JSON Lines boundary.
- Existing selection, graph-validation, budget, recovery, and canonical fixture
  tests must remain green after the package split.
- Revisit the occurrence eligibility set when a consumer demonstrates a
  missing semantic token class or when TypeScript changes the parser/checker
  boundary.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [ADR-0004](0004-make-optional-type-view-availability-explicit.md)
- [ADR-0006](0006-negotiate-capabilities-and-bound-type-graphs.md)
- [`tsfacts` protocol](../tsfacts-protocol.md)
- [Issue #9](https://github.com/swernerx/typescript-semantic-kernel/issues/9)

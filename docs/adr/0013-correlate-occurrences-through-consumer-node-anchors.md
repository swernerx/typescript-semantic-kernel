# ADR-0013: Correlate occurrences through consumer node anchors

- Status: accepted
- Date: 2026-08-16
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

Schema-v1 facts identify semantic occurrences by project-relative file, UTF-8
byte span, and TypeScript syntax kind. An OXC consumer ultimately needs to join
those facts to arena-local `NodeId`s, but this repository has no Rust package,
OXC dependency, or ownership of a consumer AST arena. Adding parser-specific
code to the Go producer would leak a downstream tree model across the semantic
boundary and duplicate syntax parsing.

Range-only matching is also insufficient. Parsers sometimes assign delimiters
or wrapper spans to different nodes, and several nodes can share one range.
Traversal-order tie breaking would turn those cases into silent,
nondeterministic semantic attachments.

## Decision

Keep OXC traversal and `NodeId` ownership in the consumer. Define a portable
candidate contract and a Go reference correlator in this repository:

- canonical keys contain normalized file identity, half-open UTF-8 span, and
  semantic-facts syntax kind;
- consumer adapters may provide allow-listed alternative anchors with a closed,
  validated normalization rule;
- canonical exact matches are indexed and always take precedence;
- zero candidates produce `unmapped`, while multiple distinct candidates
  produce `multiply-mapped` and no attachment;
- ambiguity candidates are sorted by node ID and never resolved by traversal
  order; and
- aggregate and per-syntax-kind counts distinguish exact, normalized, unmapped,
  and multiply mapped occurrences.

The reference uses hash indexes, so construction is linear in candidate and
anchor count and each fact needs at most two average-constant-time lookups.
Equivalent consumers may use logarithmic ordered indexes.

Checked-in JSON fixtures are the cross-language conformance boundary. Opaque
fixture node IDs demonstrate the algorithm; they do not claim to be live OXC
arena values.

## Considered options

### Add OXC to the Go semantic producer

This would introduce a second parser, require a cross-language binding, and put
consumer AST ownership in the producer. It conflicts with the semantic-overlay
architecture.

### Match by containment when exact identity fails

Nearest-parent or first-child heuristics can attach a fact to a plausible but
incorrect node and change when traversal or AST shapes change. Explicit anchors
make every tolerated difference reviewable and testable.

### Pick the lowest node ID on ambiguity

Sorting candidates makes diagnostics deterministic, but a numeric arena ID has
no semantic priority. Ambiguity therefore remains a diagnostic and an unmapped
side-table entry.

## Consequences

- This repository can specify and test correlation behavior without depending
  on a named downstream tool or foreign AST implementation.
- OXC consumers have a small adapter obligation: project node roles and known
  boundary differences into candidate anchors.
- Exact matches remain cheap and normalization cannot degrade into an
  unbounded AST search.
- Coverage failures are machine-readable and attributable by syntax kind.
- Actual OXC `NodeId` attachment and real-project measurements remain
  consumer-side work because this repository has no OXC arena.

## Validation and review triggers

- Reference fixtures cover JSX, declarations, expressions, and flow-sensitive
  occurrences.
- Reversing node input order must not change a report.
- Every new normalization shape requires documented semantics and fixture
  coverage.
- Revisit the contract if OXC cannot produce an unambiguous anchor for a
  supported semantic occurrence or if another parser requires a relationship
  not expressible by v1.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [Occurrence correlation contract](../occurrence-correlation.md)
- [ADR-0001](0001-use-utf8-byte-offsets-for-semantic-facts.md)
- [Issue #16](https://github.com/swernerx/typescript-semantic-kernel/issues/16)

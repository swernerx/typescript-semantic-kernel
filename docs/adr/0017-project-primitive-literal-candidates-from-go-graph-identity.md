# ADR-0017: Project primitive/literal candidates from Go graph identity

- Status: accepted
- Date: 2026-08-17
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

ADR-0016 selected primitive and literal type construction as the first
independent Rust semantic candidate, while keeping TypeScript 7's Go checker as
the semantic oracle. The existing OXC reference consumer already attaches all
five TypeFacts roots to arena-local `NodeId`s and inspects the shared,
response-local graph without copying its identity.

A candidate record must be useful for later Go-versus-Rust comparison without
mistaking equal display text for semantic equivalence. It also cannot erase an
unavailable view, producer truncation, unsupported entity, checker error, or
literal-union edge.

## Decision

The internal OXC consumer emits a versioned Rust-owned primitive/literal
candidate from each attached TypeFacts record as part of graph inspection. The
candidate consumes the existing schema-v1 Go-produced graph; it does not add a
producer capability or protocol record.

Each candidate contains:

- the portable occurrence and fact completeness/recovery/truncation status;
- actual, contextual, widened, apparent, and declared roots in fixed view
  order, retaining their view state and effective response-local TypeID;
- one deterministic record per reachable response-local TypeID;
- structured primitive, literal, null-like, and union semantics, including the
  literal kind/value and ordered union member TypeIDs; and
- the source entity state/issues plus an explicit Rust candidate state and
  machine-readable reasons for truncated, unsupported, or error cases.

Candidate type records are ordered by response-local TypeID and repeated roots
share one record. Display text is not part of the Rust semantic record. The
first candidate covers boolean, string, number, and bigint primitives;
boolean, string, number, and bigint literal values; null, undefined, and void;
and unions over those records.

The candidate version is internal to the reference consumer. TypeScript 7's Go
checker remains authoritative for type construction and all TypeFacts roots.
This decision neither changes the TS7 producer protocol nor satisfies
ADR-0016's replacement gate.

## Consequences

- Future differential work can compare structured kinds, literal values,
  member identity, roots, and states rather than display strings.
- OXC attachment remains the ownership seam: no Rust or OXC type enters the Go
  producer protocol.
- Shared canonical fixtures can be decoded and round-tripped by Go while the
  Rust consumer verifies its candidate projection through the same graph.
- Unsupported and truncated observations stay useful evidence instead of
  being coerced into a supported primitive/literal record.
- Replacement readiness still requires an independent producer and the exact
  structured differential threshold in ADR-0016.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [ADR-0015](0015-attach-semantic-facts-without-expanding-graph-identity.md)
- [ADR-0016](0016-port-occurrence-attachment-before-semantic-categories.md)
- [Issue #40](https://github.com/swernerx/typescript-semantic-kernel/issues/40)

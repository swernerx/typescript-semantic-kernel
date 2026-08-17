# ADR-0019: Compute primitive/literal candidates independently in Rust

- Status: accepted
- Date: 2026-08-17
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: ADR-0017 and ADR-0018
- Superseded by: —

## Context

ADR-0017 defined structured primitive/literal candidate records, but projected
them from the graph already produced by the Go checker. ADR-0018 added an exact
shadow comparison, but that comparison could only verify the projection. It
could not establish that Rust/OXC derived the semantic answer independently.

The first independent slice must preserve the existing shadow boundary. It
must not change the TS7 producer protocol, affect external consumers, route
production traffic to Rust, or weaken Go's role as semantic authority and
fallback.

## Decision

The isolated OXC workspace contains a version-2 primitive/literal producer that
accepts project source, selections, capabilities, and budgets. It parses and
builds OXC semantics, resolves selected identifiers through OXC references and
symbol declarations, and derives the following forms without reading a Go
`TypeGraph`:

- boolean, string, number, and bigint primitives;
- boolean, string, number, and bigint literal values;
- null, undefined, and void;
- unions whose members are all in the supported set; and
- contextual primitive annotations for supported literal initializers.

The producer owns response-local type interning, emits actual, contextual,
widened, apparent, and declared roots in fixed order, and retains explicit
complete, unsupported, and truncated states. Boxed apparent types are outside
this narrow category and therefore remain named unsupported/truncated records
instead of being approximated. Unsupported type syntax is also explicit.
Recoverable OXC parser or semantic diagnostics mark every selected fact in that
file as recovered and therefore incomplete, matching the producer's file-level
recovery contract without reading Go diagnostics.

`run-conformance.sh` invokes the Go oracle and the Rust producer independently.
Only the comparator receives both outputs. It requires exact occurrence and OXC
node mapping, compares the five view roots and structured records recursively,
and validates response-local identity with a bijection rather than requiring
the two producers to allocate equal ID strings. Repeated Rust output must be
byte-identical.

The executable shadow threshold is at least 15 complete supported records,
1,000,000 parts-per-million structured agreement, and zero semantic,
transport, or mapping differences. The tagged baseline contains 11 facts: 10
supported facts plus one named unsupported object, 50 compared roots with 50
identity matches, and 11 exact OXC mappings. Expected apparent-type truncation
and unsupported states remain visible outside the supported denominator.

This producer remains an internal shadow implementation. The Go checker stays
the semantic authority and production fallback. No TS7 producer protocol,
external consumer, capability negotiation, or production routing changes are
part of this decision.

## Consequences

- Primitive/literal conformance now measures independently computed Rust/OXC
  semantics instead of a Go-graph projection.
- The narrow category has deterministic source-to-record and graph-identity
  evidence, including contextual literals, literal unions, unsupported forms,
  and budget truncation.
- Graph inspection continues to describe Go responses; it no longer embeds a
  misleading projected Rust candidate.
- Passing this shadow gate does not itself authorize a production authority
  switch. Broader project loading, recovery, inference, flow, object, generic,
  and callable semantics remain Go-authoritative.
- Any future authority switch requires a separate decision, maintained Go
  fallback, supported-matrix CI, and evidence for the production integration.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [ADR-0016](0016-port-occurrence-attachment-before-semantic-categories.md)
- [ADR-0017](0017-project-primitive-literal-candidates-from-go-graph-identity.md)
- [ADR-0018](0018-gate-rust-semantics-against-the-go-oracle.md)
- [Issue #45](https://github.com/swernerx/typescript-semantic-kernel/issues/45)

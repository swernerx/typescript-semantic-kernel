# ADR-0005: Use response-local referential graph tables

- Status: accepted
- Date: 2026-08-15
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

The semantic boundary must preserve sharing, recursive types, aliases, generic
instantiations, members, and overloads without serializing TypeScript checker
objects. Tree-shaped JSON would duplicate shared entities and either recurse
forever or silently cut cycles. Compiler addresses and internal numeric IDs are
not stable protocol identities.

ADR-0002 established response-local symbol and declaration handles. The full
contract also needs type and signature identity plus compatible behavior for an
unrecognized graph variant.

## Decision

Schema v1 represents types, symbols, declarations, and signatures as separate
referential tables. IDs use the response-local namespaces `type:`, `symbol:`,
`declaration:`, and `signature:`. An entity is allocated before any of its
outgoing edges are traversed, so an edge may refer forward, backward, or to the
same entity. IDs preserve identity only inside one response and must never be
persisted as cross-response identities.

Type nodes may link to union or intersection members, a defining symbol,
generic target and arguments, constraint and default types, property symbols,
and call, construct, or index signatures. Symbols may link to declarations,
their alias target, value and declared types, and member symbols. Signature
nodes link to a declaration, type parameters, an optional `this` type,
parameter symbols, and a required return type.

Before JSON Lines are emitted, the adapter validates that every ID is unique,
every edge resolves in the corresponding table, every entity has a supported
variant, and completeness flags are internally coherent. Validation resolves
references by lookup and deliberately does not recursively walk edges, so
cycles are valid. Unknown variants fail explicitly instead of being interpreted
as a known type or signature kind. Explicit `opaque`, `unsupported`, or
`truncated` variants remain compatible protocol states when named by the
schema.

## Considered options

### Inline nested entities

Inlining is easy to inspect for small examples but duplicates shared nodes and
cannot represent arbitrary cycles without an additional identity mechanism.

### Export checker IDs or addresses

These values are implementation details, can change between runs, and would
couple all consumers to one backend.

### Break cycles during serialization

Replacing a recursive edge with display text or an implicit omission loses
semantic structure and makes completeness unknowable.

## Consequences

- Consumers can build arenas or maps first and resolve all edges in a second
  pass.
- Recursive and mutually recursive graphs serialize without duplication or
  infinite traversal.
- Display order does not imply ownership, nesting, or lifetime.
- The current exporter may leave newly defined edge fields absent until the
  corresponding traversal milestone implements them; absent edges are not a
  claim that the checker entity has no such relationship.
- Adding a new entity variant requires coordinated protocol evolution rather
  than a fallback guess by consumers.

## Validation and review triggers

- Executable fixtures cover shared types, cycles across type/symbol/signature
  tables, duplicate IDs, dangling edges, and unknown variants.
- Every JSON Lines response passes graph validation before its first byte is
  written.
- Revisit the table model only if measurements show a concrete transport or
  streaming limitation.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [ADR-0002](0002-use-response-local-symbol-and-declaration-ids.md)
- [`tsfacts` protocol](../tsfacts-protocol.md)

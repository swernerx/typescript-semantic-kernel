# ADR-0002: Use response-local symbol and declaration IDs

- Status: accepted
- Date: 2026-08-15
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

RFC 0001 requires occurrence facts to identify TypeScript symbols and their
source declarations without exposing the checker's Go pointer graph or its
allocation-dependent numeric IDs. Symbols can merge several declarations, and
an imported name has both a local alias declaration and a target declaration.
A consumer must be able to retain those distinctions while processing one
response.

Source files outside the configured project root introduce another boundary.
Absolute host paths are neither portable nor safe interoperability identities.
Default TypeScript libraries still need a stable logical identity because
ordinary global symbols originate there.

## Decision

Schema v1 uses opaque, deterministic, response-local `symbol:*` and
`declaration:*` handles. A declaration record identifies its source through a
file ID, the UTF-8 byte span of its declaration name when one exists, and its
syntax kind. Symbols expose stable semantic roles instead of raw checker flag
integers.

An occurrence links to the symbol returned by TypeScript at that exact source
location. Alias symbols remain visible and link to their resolved target through
`aliasedSymbol`; the adapter does not replace the local import symbol with its
target. Merged declarations are sorted by file, span, and syntax kind before
IDs are assigned.

Project files retain normalized project-relative IDs. Default library files use
the logical `typescript/lib/<filename>` namespace. Other declarations outside
the project root are not assigned host-path-derived IDs in this phase. Their
symbol and every referencing fact are marked truncated until project-reference
identity is designed explicitly.

## Considered options

### Reuse checker IDs or pointer addresses

This would be easy to emit but would expose allocation order and internal Go
state. The values are not a language-neutral or repeatable contract.

### Derive globally stable symbol hashes

Names and declaration coordinates are insufficient for every merged,
instantiated, synthetic, or aliased symbol. A premature hash would imply
cross-build stability the checker cannot currently guarantee.

### Resolve aliases eagerly

Emitting only the target would simplify lookup but lose the local import or
export declaration that the selected occurrence actually denotes. Keeping both
nodes makes the relationship explicit.

## Consequences

- Consumers can join occurrences, symbols, aliases, and declarations within a
  response without compiler internals.
- Handles must not be persisted or compared across separate requests; source
  identity and declaration coordinates are the durable correlation data.
- Supporting declaration files may add `file` records even when they were not
  selected for diagnostics.
- Project-reference declarations outside the project root remain an explicit
  incomplete case rather than leaking absolute paths.
- A future daemon may add session-scoped caching, but it must not silently
  reinterpret schema-v1 response-local handles as global identities.

## Validation and review triggers

- Fixtures cover local declarations, merged declarations, aliases, TypeScript
  library identities, deterministic output, and unsupported external files.
- Revisit the decision when project references are added or a long-lived
  transport needs identities that survive more than one response.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [ADR-0001](0001-use-utf8-byte-offsets-for-semantic-facts.md)

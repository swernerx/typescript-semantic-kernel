# ADR-0009: Intern object, symbol, and signature graphs before finalization

- Status: accepted
- Date: 2026-08-15
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

ADR-0005 defines response-local type, symbol, declaration, and signature tables,
but the first exporter milestones populated only type edges, selected symbols,
declarations, and aliases. Object properties, symbol types, callable signatures,
and instantiated signature arguments still collapsed to display text or an
`unsupported-structure` state.

These relationships are mutually recursive. An interface type points to its
symbol and properties, a property symbol points back to its type, a callable
type points to signatures, and signatures point through parameters and return
types into the same graph. Treating an allocated but unfinished record as
incomplete would falsely truncate every valid cycle. Traversing before assigning
an identity would instead duplicate shared nodes or recurse forever.

Instantiated generic signatures also retain their mapper only inside the
checker. The adapter needs the mapped type arguments without exposing that
mapper, compiler IDs, or other internal representation.

## Decision

Deep object traversal is opt-in in schema v1. Requiring `graph.references`
enables type-to-symbol, object-to-property, symbol-to-type, declared-type, and
member edges. Requiring `graph.signatures` additionally enables call,
construct, and index signature traversal and implies the reference traversal
needed by parameter symbols. Unnegotiated requests retain the previous shallow
symbol and truncated object behavior.

Protocol-native arrays and tuples retain their bounded target, argument, and
element representation from ADR-0008 rather than recursively exporting the
standard library method surface. Ordinary and generic class, interface,
function, and object types receive the deep object traversal.

The three interners allocate an ID and a provisionally complete record before
walking any outgoing edge. Checker type, symbol, signature, and index-info
pointers are used only as response-construction map keys; they are never
serialized. Repeated pointers reuse one response-local ID.

After every occurrence root has been collected, the builder runs a monotonic
fixed-point finalization over all three tables. A complete record that references
an incomplete type, symbol, or signature becomes truncated with a corresponding
`referenced-incomplete-*` issue. The process repeats until no state changes,
then occurrence completeness and truncation are recomputed from the finalized
roots. Cycles consisting entirely of complete records remain complete.

Property symbols and symbol members are sorted by escaped semantic name before
their first discovery. Call and construct overloads retain checker order because
their position is observable overload order. Index signatures are normalized by
stable key-type display. Declarations retain their existing file-and-span order.

Signature records preserve declaration, target signature, instantiated type
arguments, type parameters, explicit `this` type, ordered parameter symbols,
minimum argument count, rest state, and return type. Index signatures preserve
their key type, one-argument arity, value type in `returnType`, and readonly
state. Call and construct parameter symbols provide their own type and
declaration edges.

The checker exposes one narrow read-only adapter method,
`GetTypeArgumentsOfSignature`. It applies an instantiated signature's existing
mapper to its target type parameters, matching the checker's own signature
printer behavior. This is a documented review-gate exception: it derives
semantic types without changing resolution, inference, caching, or checker
ownership.

The existing type-node and type-depth budgets apply to every newly traversed
type edge. Symbol and signature counters remain a future additive capability;
deep traversal is therefore enabled only by an explicit request.

## Considered options

### Mark in-progress records incomplete

This makes completion easy to query during recursion, but valid mutual cycles
become permanently truncated depending on discovery order.

### Decide completeness during recursive return

This handles trees and simple self-cycles, but an incomplete node discovered
later in a mutual cycle does not reliably propagate to every owner.

### Serialize signature display text

Display text loses independent overload identity, parameter declarations,
generic targets and arguments, index-key types, sharing, and recursive edges.

### Expose the checker mapper

The mapper is compiler-owned behavior and not a language-neutral fact. Exporting
only its resulting type arguments keeps the protocol boundary semantic.

## Consequences

- Recursive objects and callable graphs serialize once without infinite
  traversal.
- Property type IDs, declaration locations, aliases, members, overloads, and
  generic signature relationships are independently traversable.
- Completeness is closed over cross-table references and is independent of
  recursive discovery order.
- Consumers can continue using schema v1 because the new fields are additive
  and deep traversal requires an already advertised capability.
- The adapter gains one intentionally narrow checker export that must be
  reevaluated if TypeScript provides an equivalent stable API.
- Extremely wide symbol or signature graphs are not yet independently counted;
  a later capability must introduce those counters before changing default
  traversal behavior.

## Validation and review triggers

- Focused fixtures cover recursive property cycles, symbol and declaration
  links, call and construct overloads, readonly index signatures, constraints,
  defaults, instantiated signature arguments, and byte-stable output.
- Compatibility tests prove unnegotiated requests retain shallow object output.
- Budget tests prove incomplete descendant types propagate through symbols to
  occurrence roots.
- Graph validation checks new signature targets and type edges and rejects
  malformed index signatures.
- Revisit the checker export when a stable public API can return instantiated
  signature arguments.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [ADR-0005](0005-use-response-local-referential-graph-tables.md)
- [ADR-0006](0006-negotiate-capabilities-and-bound-type-graphs.md)
- [ADR-0008](0008-export-core-composite-types-as-normalized-graph-nodes.md)
- [`tsfacts` protocol](../tsfacts-protocol.md)
- [Issue #11](https://github.com/swernerx/typescript-semantic-kernel/issues/11)

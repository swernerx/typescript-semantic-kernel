# ADR-0008: Export core composite types as normalized graph nodes

- Status: accepted
- Date: 2026-08-15
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

The initial exporter assigned response-local IDs to checker types but represented
most object-backed types only as truncated `object` records. Arrays, tuples, and
generic instantiations were therefore visible only in diagnostic display text.
Type parameters exposed a computed base constraint but omitted their declared
constraint, default, and instantiated target.

Consumers need protocol-native edges to reconstruct common TypeScript types,
preserve sharing, and compare another backend against the Go checker. At the
same time, schema-v1 readers reject unknown variants, so new type kinds cannot
appear in an unnegotiated response.

## Decision

The producer advertises `types.core-composite`. A request must list that
capability in `requiredCapabilities` before the producer emits the new
`array`, `tuple`, `reference`, `this`, `unique_symbol`, or `non_primitive`
variants. Without that opt-in, schema-v1 keeps its previous truncated or
unsupported representation for those forms.

Checker type pointers are interned to one response-local `type:` ID before
their edges are traversed. Repeated checker pointers therefore share an ID, and
originating generic targets use a valid self-target edge. Instantiations point
to that target and carry ordered `typeArguments`; displays never substitute for
those relationships.

Arrays carry one type argument plus explicit readonly metadata. Tuples carry
ordered type arguments and aligned semantic element metadata: required,
optional, rest, or variadic, with a source label when one exists. Tuple
readonly state is explicit. Generic references preserve their target and
positional arguments.

Ordinary type parameters preserve their direct checker constraint, default,
and instantiated target. Checker `this` type parameters use the distinct
`this` variant. Unique symbols and the intrinsic non-primitive `object` keyword
use distinct leaf variants.

Union and intersection membership is semantically unordered. When the
capability is requested, the exporter sorts constituents by protocol type
category and stable checker display before assigning descendant IDs. It does
not sort by checker numeric flags, checker IDs, addresses, or response-local
IDs. Type argument and tuple element order remains positional and is never
sorted.

Named checker flag strings remain diagnostic metadata in schema v1. Numeric
checker flags and compiler type IDs are never emitted as semantic identity or
classification.

## Considered options

### Keep composite structure in `display`

Display strings are intended for diagnostics. Parsing them would lose sharing,
targets, defaults, tuple modifiers, and a stable compatibility boundary.

### Expose checker type IDs and numeric flags

Those values are backend-specific implementation details and can change across
compiler revisions. They cannot serve as a cross-backend protocol.

### Preserve intersection source order

Intersection source order can assign different descendant IDs to equivalent
types. Normalizing semantically unordered members makes snapshots independent
of spelling while preserving positional order where it matters.

### Emit new variants without capability negotiation

Existing schema-v1 readers reject unknown variants by design. Unconditional
emission would turn an additive producer upgrade into an unannounced breaking
change.

## Consequences

- Common generic and collection types are reconstructable without parsing
  display strings.
- Equivalent unions and intersections receive deterministic constituent order
  for the same semantic display keys.
- Generic target self-cycles are explicit and valid under the graph contract.
- Tuple element kind, label, and readonly state are preserved independently of
  display formatting.
- Consumers must opt in to the new variants; discovery remains possible from
  the response header.
- Object members, defining symbols, and signatures remain the next negotiated
  exporter milestone rather than an implicit claim of this capability.

## Validation and review triggers

- Focused tests cover every intrinsic and literal leaf, arrays, readonly
  arrays, labeled tuples, generic references, type parameters, `this`, unique
  symbols, defaults, constraints, targets, shared IDs, and normalized composite
  order.
- Graph validation requires array and tuple metadata to match their type
  arguments and rejects unknown tuple element kinds.
- Compatibility tests prove that unnegotiated requests retain the previous
  object fallback.
- Revisit the sort key if two structurally different constituents can share the
  same category and checker display in conformance fixtures.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [ADR-0005](0005-use-response-local-referential-graph-tables.md)
- [ADR-0006](0006-negotiate-capabilities-and-bound-type-graphs.md)
- [`tsfacts` protocol](../tsfacts-protocol.md)
- [Issue #10](https://github.com/swernerx/typescript-semantic-kernel/issues/10)

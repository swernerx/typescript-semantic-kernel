# ADR-0010: Export advanced types through semantic detail records

- Status: accepted
- Date: 2026-08-16
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

ADR-0008 gives intrinsic, literal, union, intersection, collection, reference,
and type-parameter forms protocol-native structure. TypeScript also retains
conditional, mapped, indexed-access, index, template-literal, string-mapping,
and substitution types when generic work is deferred. Before this decision
those types collapsed to display text and `unsupported-type-form`, or a mapped
type appeared as a generic object with its defining transformation missing.

The additional forms have different edge shapes. Reusing generic `target`,
`constraint`, or `members` fields for every meaning would make consumers infer
semantics from the type kind and field combinations. Exposing checker structs
or numeric flags would instead couple the protocol to the implementation the
boundary is meant to hide.

## Decision

Schema v1 advertises `types.advanced`. A request must require that capability
before advanced type variants are emitted. Without it, non-object advanced
forms retain the existing explicit unsupported representation and mapped types
retain their previous object-compatible behavior.

Each negotiated variant has a semantic detail object:

- conditional types link check, extends, true, and false types, inferred type
  parameters, and distributive state;
- mapped types link the type parameter, constraint, optional name remapping,
  value template, optional modifiers source, and normalized readonly/optional
  operations (`add`, `remove`, or `preserve`);
- indexed-access types link object and index types;
- index (`keyof`) and string-mapping types use an explicit target edge;
- template-literal types preserve ordered text fragments and placeholder types;
- substitution types link their base type and the constraint known to hold.

All detail edges use response-local `TypeID` values and participate in the
existing allocate-before-walk interning and fixed-point completeness closure.
An expected checker edge that is absent makes the owner truncated with
`missing-type-edge`; it is never omitted while the owner remains complete.
Budget sentinels and referenced-incomplete propagation therefore work for
advanced types without a second graph algorithm.

The checker exposes two narrow read-only adapter methods. One returns inferred
parameters and distributive state for a conditional type. The other returns
the semantic components and normalized modifiers of a mapped type. They do not
expose mappers, caches, nodes, compiler IDs, or mutation and do not change type
resolution.

## Considered options

### Keep advanced forms opaque

This preserves a smaller schema but prevents consumers from traversing the
generic relationships that motivate the semantic graph.

### Reuse generic fields for all forms

This reduces field count but overloads `target`, `constraint`, and `members`
with unrelated meanings and weakens shape validation.

### Serialize checker structs and flags

This is mechanically complete but makes the language-neutral contract depend
on private TypeScript Go layout and numeric implementation flags.

## Consequences

- Deferred generic semantics are traversable without parsing display text.
- Advanced cycles, sharing, recovery, and budget cutoffs use the same graph
  invariants as core forms.
- Consumers must explicitly negotiate `types.advanced` and reject unknown type
  variants as required by schema v1.
- Mapped modifier intent is stable even if checker AST token representation
  changes.
- The two checker adapter methods remain review-gate exceptions and should be
  removed if a stable TypeScript API exposes equivalent facts.

## Validation and review triggers

- Focused tests cover every advanced variant, inferred conditional parameters,
  mapped name/modifier semantics, deterministic output, diagnostics, and depth
  truncation.
- Graph validation rejects dangling advanced edges, invalid mapped modifiers,
  and misaligned template fragments.
- Compatibility tests prove advanced variants remain request-negotiated.
- Revisit the shape if a TypeScript type form cannot be represented without a
  new required field or if the checker replaces one of the adapter facts.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [ADR-0006](0006-negotiate-capabilities-and-bound-type-graphs.md)
- [ADR-0008](0008-export-core-composite-types-as-normalized-graph-nodes.md)
- [ADR-0009](0009-intern-object-symbol-and-signature-graphs-before-finalization.md)
- [Issue #12](https://github.com/swernerx/typescript-semantic-kernel/issues/12)

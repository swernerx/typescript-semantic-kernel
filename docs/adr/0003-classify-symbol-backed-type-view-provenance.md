# ADR-0003: Classify symbol-backed type-view provenance

- Status: accepted
- Date: 2026-08-15
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

RFC 0001 requires consumers to distinguish a type written by the author, a type
inferred by TypeScript, and a type narrowed at one control-flow location. The
checker's type at a source location alone cannot preserve those distinctions:
two references to one symbol may have different observed types, while an
optional declaration can have a source annotation that differs from its
unflowed checker type.

The protocol must expose useful provenance without claiming to serialize the
checker's inference process or treating every enclosing type node as an
annotation of the selected symbol. It must also retain `typeAtLocation` as the
single authoritative observation for existing consumers.

## Decision

Schema v1 keeps required `typeAtLocation` and adds three optional, interned type
references for symbol-backed value occurrences:

- `annotationType` contains an unambiguous direct source annotation on a
  variable, parameter, property declaration, or property signature when that
  annotation describes the whole symbol.
- `inferredType` contains the unflowed checker type when no supported direct
  annotation exists. It records the inference result, not an inference trace.
- `narrowedType` repeats the observed `typeAtLocation` type only when it differs
  from the unflowed symbol type or the selected property is reached through a
  flow-narrowed receiver.

`annotationType` and `inferredType` are mutually exclusive. Narrowing is always
compared with the unflowed checker type, not the source annotation. This avoids
classifying the ordinary `string | undefined` type of `value?: string` as a
flow narrowing merely because its written annotation is `string`.

Only value occurrences and value declaration names receive these views.
Type-only occurrences remain unclassified. Destructuring-container annotations
and function return annotations are not reinterpreted as annotations of their
binding or callable symbols. Contextual, widened, and constraint views remain
independent occurrence views.

## Considered options

### Label every unflowed symbol type as inferred

This would make extraction simple but erase whether the author constrained the
symbol explicitly. Tools could not distinguish authored intent from checker
inference.

### Use the source annotation as the narrowing baseline

This fails for optional parameters and properties, where the checker adds
`undefined`, and for other declaration semantics that transform the written
type. The unflowed checker type is the correct control-flow baseline.

### Promote any containing type annotation

This would mislabel a destructuring input annotation as the type of each bound
name and a function return annotation as the callable's type. The protocol
instead omits uncertain annotation provenance and exposes the safe inferred
symbol result.

## Consequences

- Consumers can distinguish authored annotations, checker inference results,
  and control-flow-specific observations without traversing compiler objects.
- `typeAtLocation` remains sufficient for consumers that do not need
  provenance, and the optional fields are an additive schema-v1 change.
- The adapter deliberately reports an inference result rather than the steps or
  constraints that produced it.
- Unsupported, ambiguous, or containing annotations fall back to the inferred
  origin view instead of making a false provenance claim.
- Future declaration kinds require an explicit whole-symbol annotation rule
  before they can emit `annotationType`.

## Validation and review triggers

- Fixtures cover annotated and inferred variables, control-flow narrowing,
  optional-parameter baselines, destructuring, function return annotations,
  type-only positions, narrowed property receivers, generic instantiation,
  JSON omission, and deterministic output.
- Revisit the classification when JSDoc annotations, accessor pairs, binding
  elements, inference traces, or a stable upstream provenance API enter scope.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [`tsfacts` protocol](../tsfacts-protocol.md)

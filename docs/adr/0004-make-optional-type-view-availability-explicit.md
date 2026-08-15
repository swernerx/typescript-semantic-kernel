# ADR-0004: Make optional type-view availability explicit

- Status: accepted
- Date: 2026-08-15
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

An omitted optional type root is ambiguous without additional protocol state.
It can mean that the view does not apply to the selected syntax, that the
checker has no answer, or that the view is identical to the actual type and
would only duplicate an edge. A consumer must not infer those meanings from a
missing JSON field or confuse them with graph truncation and request failure.

Schema v1 already exposes occurrence-specific annotation, inference,
contextual, widening, narrowing, and constraint information. Issue #6 also
requires explicit actual, apparent, and declared roots while keeping the first
spike compatible.

## Decision

Every fact exposes `actualType` as the canonical occurrence type and retains
the equal `typeAtLocation` field for schema-v1 compatibility. Optional
`contextualType`, `widenedType`, `apparentType`, and `declaredType` roots are
accompanied by a required `typeViewStates` object. Each state is one of:

- `available`: the corresponding root field is present;
- `same-as-actual`: the view is valid but the root is omitted because it equals
  `actualType`;
- `inapplicable`: the view has no meaning for this occurrence; or
- `unavailable`: the view applies, but the checker returned no type.

`actualType` is always `available`. A truncated root remains `available` and
points to an explicit incomplete type node; fact and type completeness fields
continue to report graph truncation. A request that cannot resolve the actual
type fails as a request and emits no partial fact.

The declared view is the unflowed value-symbol type at value occurrences and
the checker's declared type at type occurrences. The apparent view is the
checker's apparent type of the actual root. Contextual types apply only to
expressions. Distinct optional roots are interned in the shared type graph.

## Considered options

### Treat every missing field as inapplicable

This erases the distinction between a valid view equal to the actual type and a
checker capability gap.

### Always repeat equal type roots

This avoids one ambiguity but creates redundant edges and still cannot explain
inapplicable or unavailable views.

### Encode failure as another type node

An exporter failure is not a semantic type. Keeping request failure,
availability, and graph completeness separate gives consumers actionable
states without inventing checker meaning.

## Consequences

- Consumers can interpret every absent contract root without checker-specific
  heuristics.
- Existing schema-v1 consumers can continue reading `typeAtLocation` and ignore
  additive fields.
- The actual/type-at-location duplication remains until a future major schema
  can remove the compatibility field.
- New optional occurrence views must define applicability and unavailability
  rules before entering the contract.

## Validation and review triggers

- Fixtures cover distinct, equal, inapplicable, and unavailable view handling,
  including value and type occurrences.
- JSON-level tests assert the canonical actual root and state object.
- Revisit the state vocabulary only through a protocol evolution decision.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [ADR-0003](0003-classify-symbol-backed-type-view-provenance.md)
- [`tsfacts` protocol](../tsfacts-protocol.md)

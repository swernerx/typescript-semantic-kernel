# ADR-0012: Normalize conformance snapshots without rewriting graph identity

- Status: accepted
- Date: 2026-08-16
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

The representative corpus must remain a useful oracle across repeated runs and
upstream TypeScript revisions. Raw schema-v1 output contains two categories that
make a long-lived golden noisy without changing the semantic graph: producer
version metadata and checker-generated numeric suffixes on internal well-known
symbol names such as `__@iterator@1234`.

Unbounded snapshots also reach deeply into the TypeScript standard library.
They are valid protocol results, but multi-megabyte goldens obscure changes in
the selected application types. Conversely, renumbering response-local IDs or
sorting union members, tuple elements, overloads, or generic arguments would
hide changes that consumers need to see.

## Decision

Conformance goldens use an explicitly bounded, test-only normalized view of a
valid schema-v1 result. The normalizer:

- replaces the TypeScript semantic version and source revision with a fixed
  marker;
- sorts capabilities and top-level response tables by their existing
  response-local identity, and facts by source identity;
- replaces only the numeric suffix on checker-internal well-known-symbol
  display names with a fixed marker; and
- leaves response-local IDs, graph edges, semantic record fields, entity
  states, issues, source spans, and ordered semantic collections unchanged.

Selected golden cases declare explicit type-node and depth budgets. Budget
sentinels and truncation are retained in the snapshot rather than filtered out.
The normalized JSON Lines must still decode and pass the same graph validator
as raw output.

Byte-level goldens run only with the embedded standard-library bundle. That
configuration is the canonical producer environment for checked-in snapshots;
the `noembed` build intentionally resolves the standard library from disk and
can therefore produce a different, still valid graph. Structural conformance
tests continue to run in both configurations.

Raw JSON Lines determinism remains a separate protocol requirement and is
tested independently. Normalized goldens do not change the wire format and are
not permission to introduce additional unstable protocol fields.

## Considered options

### Store raw unbounded output

This preserves every producer detail, but routine upstream version changes and
large standard-library expansions dominate reviews and make the oracle costly
to inspect.

### Compare only graph statistics

Counts and hashes are compact, but they do not show which type shape, edge, or
occurrence view changed.

### Renumber and reorder the entire graph

A fully canonical graph-isomorphism pass could tolerate more producer changes,
but it risks erasing meaningful ordering and identity-allocation regressions.

## Consequences

- High-value snapshots remain compact and reviewable while retaining complete
  graph records up to their declared budget.
- Version upgrades require an intentional golden update only when normalized
  semantic evidence changes.
- Internal well-known-symbol suffixes cannot create false byte-level diffs.
- The `noembed` build does not compare against goldens produced from a
  different standard-library source, while retaining structural coverage.
- A normalizer change is itself contract-sensitive and requires review because
  excessive normalization could conceal a semantic regression.

## Validation and review triggers

- In the canonical embedded configuration, each normalized snapshot is
  produced twice, compared byte for byte, decoded, and graph-validated before
  it is compared with the checked-in golden.
- Structural tests separately assert recursive sharing, generic signatures,
  aliases, occurrence views, recovery, truncation, and named capability gaps
  in both embedded and `noembed` builds.
- Revisit this decision if a new volatile field appears, a consumer needs the
  raw checker-internal well-known-symbol spelling, or normalization would need
  to rewrite response-local IDs or ordered semantic edges.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [ADR-0002](0002-use-response-local-symbol-and-declaration-ids.md)
- [ADR-0005](0005-use-response-local-referential-graph-tables.md)
- [ADR-0006](0006-negotiate-capabilities-and-bound-type-graphs.md)
- [Issue #19](https://github.com/swernerx/typescript-semantic-kernel/issues/19)

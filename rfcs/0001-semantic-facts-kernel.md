# RFC 0001: Expose a TypeScript semantic facts kernel

- Status: Accepted
- Date: 2026-08-15
- Upstream baseline: `microsoft/typescript-go@1bcfa18d79a3be41772223d5c05dfe4480e614ff`

## Summary

Add a narrow, versioned interface to the TypeScript 7 Go implementation that
attaches semantic type facts to source occurrences. Consumers such as linters,
codemods, localization tools, and alternative syntax trees should be able to
obtain TypeScript-compatible semantics without embedding the compiler or
depending on its internal object graph.

The first implementation remains a thin extension of upstream TypeScript 7.
It does not remove compiler subsystems. A later Rust implementation may replace
the Go backend only after it conforms to the same protocol and test corpus.

## Motivation

Many source-analysis tools need more information than syntax alone can provide:

- whether a string literal is user-facing text, an object key, or a member of a
  literal union;
- where a component property ultimately flows;
- whether an expression is callable, nullable, promise-like, or narrowed by
  control flow;
- which declaration and symbol a source occurrence refers to.

Reimplementing these answers independently in each tool creates semantic drift
from TypeScript. Embedding compiler internals instead couples every consumer to
unstable Go data structures and makes a later Rust implementation harder.

The reusable product is therefore not a smaller `tsc` executable. It is a
stable semantic boundary backed initially by the authoritative checker.

## Goals

1. Load real TypeScript projects, including `tsconfig.json`, module resolution,
   standard libraries, declaration files, JSX, and project references.
2. Associate semantic facts with exact source occurrences rather than only
   declarations or symbols.
3. Preserve distinct views of a type when they matter, including annotation,
   inferred, contextual, widened, constraint, and control-flow-narrowed types.
4. Expose a versioned, language-neutral representation that Rust and other
   clients can consume without linking compiler internals.
5. Keep the initial patch against upstream TypeScript 7 small and reviewable.
6. Build a conformance corpus that can serve as an oracle for a future Rust
   backend.

## Non-goals

- Designing a second TypeScript language or changing TypeScript semantics.
- Removing the parser, binder, module resolver, project loader, or checker in
  the first phase.
- Providing JavaScript emit, declaration emit, formatting, completions,
  refactorings, or an editor protocol through the semantic-facts interface.
- Serializing TypeScript's internal pointer graph or promising stable internal
  `Type`, `Symbol`, or node identifiers.
- Translating the complete TypeScript AST into the OXC AST.
- Requiring every possible TypeScript type to have an unlimited lossless wire
  representation in the first prototype.

## Why the kernel is not a small checker

Useful type assignment requires most of TypeScript's semantic pipeline:
parsing, binding, scopes, project construction, module and type resolution,
contextual typing, overload resolution, generic instantiation, control-flow
narrowing, and access to library and declaration files.

The kernel can omit many downstream products, but it cannot discard these
semantic dependencies without returning answers that differ materially from
TypeScript. Code removal is therefore deferred until measurements show a module
is unreachable from the facts interface and removing it reduces maintenance or
distribution cost.

## Semantic model

There is no single type that can always be assigned to a symbol or syntax node.
For example, two references to the same variable may have different types after
control-flow narrowing. Generic calls also produce instantiation-specific types
at each call site.

The protocol therefore models a fact as a relationship between a source
occurrence and one or more semantic views:

```text
SourceFact
  file_id
  span
  syntax_kind
  symbol_id?
  declaration_ids[]
  annotation_type?
  inferred_type?
  contextual_type?
  widened_type?
  narrowed_type?
  constraint_type?
```

Types form an interned graph rather than repeated display strings:

```text
TypeNode
  primitive | literal | union | intersection | object | callable
  generic_instance | type_parameter | indexed_access | conditional
  any | unknown | never | error | truncated
```

The graph format must support cycles, recursive types, deterministic ordering,
and explicit size and depth limits. Human-readable TypeScript display text may
be included for diagnostics, but it is not the interoperability contract.

## Source identity and OXC integration

The initial integration does not convert one compiler AST into another. The Go
backend emits a semantic overlay keyed by source identity; an OXC consumer
correlates it with its own nodes.

Every occurrence must include:

- a normalized project-relative file identity;
- start and end offsets with an explicit encoding (`utf8-bytes` or
  `utf16-code-units`);
- a syntax category sufficient to disambiguate nested nodes sharing a range;
- optional structural context if range and category are not unique.

OXC can then attach the result to its local `NodeId` side table. This keeps OXC
as the syntax and traversal frontend while TypeScript supplies semantic facts.

## Proposed architecture

```text
TypeScript project
      |
      v
TypeScript 7 project loader, binder, and checker (Go)
      |
      v
tsfacts adapter
      |
      v
versioned semantic-facts protocol
      |
      +----> internal OXC reference side tables (Rust)
      +----> linters and codemods
      +----> conformance snapshots

Future Rust semantic backend
      |
      +----> same protocol and conformance snapshots
```

The prototype should begin as a command such as `tsfacts` rather than a public
Go package. TypeScript 7.0 does not yet expose a stable programmatic API, and a
process boundary prevents accidental dependence on internal Go types.

## Protocol sketch

The prototype accepts a project and optional source selections:

```json
{
  "schemaVersion": 1,
  "project": "tsconfig.json",
  "files": ["src/example.tsx"],
  "selections": [{ "file": "src/example.tsx", "start": 120, "end": 125 }]
}
```

It produces a header followed by files, interned types, symbols, and occurrence
facts. JSON Lines is preferred for the spike because it is inspectable and can
stream. The schema is transport-neutral so a binary encoding can be introduced
later without changing its semantics.

Every result records at least:

- facts schema version;
- TypeScript semantic version and source revision;
- compiler options that affect semantics;
- offset encoding;
- whether each fact is complete, recovered from invalid code, or truncated.

## Upstream strategy

This repository retains `microsoft/typescript-go` as the `upstream` Git remote.
Changes are maintained as a small patch series on top of upstream `main`.

The first phase adds an adapter and tests without deleting upstream code. Before
any pruning, the project must measure the reachable Go package graph from the
adapter and document the concrete benefit of removal. Upstream synchronization
cost is treated as a primary design constraint.

When TypeScript publishes a stable API suitable for these facts, the adapter
should prefer that API and reduce or eliminate internal patches.

## Delivery phases

The protocol, corpus, and internal OXC/Rust reference path are implemented.
The project is now evaluating migration one bounded category at a time; this
does not make the Rust/OXC layer a production consumer or a semantic authority.

### Phase 0: Protocol spike (completed)

- Define source identity, fact roles, and the smallest useful type graph.
- Add `tsfacts` for a single configured project.
- Emit inspectable JSON Lines.
- Query only explicitly selected occurrences before considering full-project
  eager dumps.

### Phase 1: Internal OXC reference bridge (completed)

- Correlate TypeScript spans with OXC nodes.
- Attach facts to OXC `NodeId` side tables.
- Measure ambiguous and unmatched mappings.
- Exercise the integration in the repository-owned reference and migration
  harness without introducing a downstream product dependency.

### Phase 2: Conformance corpus (representative v0 slice completed)

The v0 corpus covers core and advanced graph shapes, `as const`, `satisfies`,
imports, overloads, generics, JSX properties, control-flow narrowing, recovery,
and budget pressure. Ambient declarations, project references, and further
invalid or incomplete programs remain corpus-expansion work.

Snapshots should test structured facts, not only pretty-printed types.

### Phase 3: Boundary evaluation (spike completed; production shape open)

Measure startup time, warm project reuse, memory use, response size, mapping
accuracy, and upstream rebase cost. Decide whether the production interface is
a long-lived process, library binding, or one-shot command.

### Phase 4: Rust feasibility (migration harness proven; semantics not ported)

Implement selected fact categories in Rust using arenas, interning, and stable
IDs rather than mechanically translating Go pointer graphs. Compare every Rust
result with the Go oracle. Replace the Go backend only category by category and
only at an explicit compatibility threshold.

## Phase 3/4 spike evidence (2026-08-17)

Issue #20 ran all six representative corpus cases through the real one-shot
`tsfacts` process, schema-v1 JSON Lines decoder, project-file OXC correlation,
TypeFacts attachment, and bounded Rust graph inspector. The exact command,
host/toolchain details, per-case timings, counters, diagnostics, and artifact
measurements are recorded in
[`docs/evidence/ts7-oxc-spike-2026-08-17.json`](../docs/evidence/ts7-oxc-spike-2026-08-17.json).

The Go producer emitted 25 facts. OXC parsed five of six selected files and all
22 facts in those valid files mapped and attached: 15 exact, seven normalized,
zero unmapped or ambiguous, and zero actual-root transport mismatches. The
intentional syntax-recovery file remained a named consumer failure with three
Go facts; it was not removed from the denominator or treated as an exporter
failure. The inspected graph preserved 6,368 edges and substantial shared
identity while separately reporting producer states and consumer cutoffs.

The local aggregate first/repeated one-shot times were 634,312,462 ns and
247,002,000 ns. The debug Rust executable was 13,425,232 bytes and peak measured
RSS was 11,829,248 bytes. These numbers characterize this spike host only. They
do not answer the open daemon/long-lived-process question or establish a
general performance claim.

This evidence approves mechanical porting of occurrence identity, correlation,
response-global fact indexing, and side-table attachment behind the Go oracle.
It approves no semantic checker category for replacement. Primitive/literal
record construction is the first proposed independent Rust semantic candidate,
but decoding Go-produced roots and correlating syntax is not semantic
equivalence. ADR-0016 defines the compatibility gates and lists the semantic
areas that remain Go-authoritative.

## Considered alternatives

### Prune TypeScript 7 immediately

This could reduce repository size early, but it obscures which changes are
necessary for the interface and makes upstream synchronization expensive.
Deferred until reachability and maintenance measurements justify it.

### Use TypeScript internal Go packages directly from every consumer

This is initially quick but leaks unstable APIs and Go-specific object models.
It also gives Rust consumers no durable seam. Rejected as the public boundary;
it may still be used behind the adapter.

### Implement a small independent checker directly over OXC

This offers a pure-Rust deployment but would establish semantic drift before a
conformance oracle exists. Retained as a later backend strategy, not the first
source of truth.

### Convert the full TypeScript AST to OXC

The trees have different ownership, trivia, recovery, and node-shape choices.
A full conversion adds cost unrelated to the desired semantic facts. Rejected
for the first integration in favor of source-identity correlation.

### Reuse the existing Oxlint/tsgolint split

Its Go semantic backend and Rust frontend validate the process-boundary model.
However, tsgolint is rule- and diagnostic-oriented rather than a general facts
protocol. It is prior art and a possible implementation source, not the desired
consumer contract.

## Consequences

### Positive

- Multiple tools can share TypeScript-compatible facts.
- Consumers remain decoupled from TypeScript's Go object model.
- The protocol and corpus make a gradual Rust implementation testable.
- The initial fork stays close enough to upstream to absorb semantic fixes.

### Costs and risks

- The semantic core remains substantial even without emit and editor features.
- Source-span correlation needs careful handling of encoding and nested nodes.
- Structured type graphs require cycle handling and bounded serialization.
- Invalid programs need explicit best-effort semantics.
- TypeScript 7's evolving API may force adapter changes.
- Maintaining a fork has an ongoing upstream merge cost even with a small patch
  series.

## Validation and acceptance criteria

This RFC is accepted because the protocol and completed reference-consumer
spike demonstrate:

1. facts for at least ten representative semantic cases;
2. deterministic output across repeated runs;
3. explicit behavior for invalid code and truncated types;
4. successful mapping of selected facts to OXC nodes with measured ambiguity;
5. no serialized compiler-internal pointers or unstable numeric IDs;
6. a documented upstream synchronization procedure;
7. an internal OXC/Rust reference consumer that attaches and inspects semantic
   facts without a full AST conversion or a downstream project dependency.

Acceptance establishes the versioned boundary, not compiler equivalence,
production readiness, or a Rust performance advantage. The TypeScript 7 Go
checker remains the semantic oracle. The next candidate is independent
primitive/literal Rust type-record construction under ADR-0016's differential
gates; no semantic category is approved for replacement yet.

## Open questions

- Should the first transport be a one-shot process or a reusable daemon?
- Which fact views can TypeScript 7 expose without invasive checker changes?
- Is source range plus syntax category sufficient for all relevant OXC nodes?
- Which portions of the type graph require exact structure in version 1, and
  which may initially be represented as opaque or truncated?
- What compatibility threshold should a future Rust category meet before it can
  replace the Go result? ADR-0016 defines the first gate; it must be revisited
  with independent Rust semantic output before replacement.
- Should the eventual public name be `tsfacts`, `TypeScript Semantic Kernel`, or
  another name that avoids implying an alternate TypeScript language?

## References

- [TypeScript 7 upstream](https://github.com/microsoft/typescript-go)
- [TypeScript 7.0 announcement](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/)
- [Oxlint type-aware architecture](https://oxc.rs/docs/guide/usage/linter/type-aware.html)
- [TS7-to-OXC/Rust spike evidence](../docs/evidence/ts7-oxc-spike-2026-08-17.json)
- [ADR-0016: Port occurrence attachment before semantic categories](../docs/adr/0016-port-occurrence-attachment-before-semantic-categories.md)
